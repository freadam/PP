use rusqlite::params;

use super::Store;
use crate::db;
use crate::error::{AppError, Result};
use crate::ids::{new_id, validate_id};
use crate::model::*;
use crate::time::{day_start, local_date, parse_date, week_start, zone};

/// The eight project hues from §5.2 — all clear 3:1 against `--ink` and stay
/// distinguishable at 8px, including for the two common colour-vision
/// deficiencies.
pub const PROJECT_PALETTE: [&str; 8] = [
    "#56C2D6", "#7E8CF0", "#B07CE8", "#E070A8", "#E9A63C", "#6EBE8C", "#4FA8B8", "#C9A227",
];

const KINDS: [&str; 5] = ["work", "study", "personal", "side", "admin"];

impl Store {
    pub fn get_projects(&self, tz: &str) -> Result<Vec<ProjectRow>> {
        let zone = zone(tz)?;
        let today = local_date(self.now(), &zone);
        let ws = week_start(parse_date(&today)?);
        let week_from = day_start(ws, &zone);

        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.colour, p.icon, p.kind, p.sort_rank, p.is_archived,
                    p.weekly_target_sec, p.created_at, p.updated_at,
                    (SELECT COUNT(*) FROM task t
                      WHERE t.project_id = p.id AND t.status = 'open' AND t.deleted_at IS NULL),
                    (SELECT COALESCE(SUM(s.elapsed_sec), 0)
                       FROM time_session s JOIN task t2 ON t2.id = s.task_id
                      WHERE t2.project_id = p.id AND s.started_at >= ?1)
             FROM project p
             WHERE p.deleted_at IS NULL
             ORDER BY p.is_archived ASC, p.sort_rank ASC, p.name ASC",
        )?;
        let rows = stmt.query_map([week_from], |r| {
            Ok(ProjectRow {
                id: r.get(0)?,
                name: r.get(1)?,
                colour: r.get(2)?,
                icon: r.get(3)?,
                kind: r.get(4)?,
                sort_rank: r.get(5)?,
                is_archived: r.get::<_, i64>(6)? == 1,
                weekly_target_sec: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
                open_task_count: r.get(10)?,
                week_tracked_sec: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn create_project(&mut self, input: NewProject) -> Result<ProjectRow> {
        let name = clean_name(&input.name)?;
        let kind = input.kind.unwrap_or_else(|| "work".into());
        if !KINDS.contains(&kind.as_str()) {
            return Err(AppError::invalid(format!("'{kind}' is not a project kind.")));
        }
        if let Some(t) = input.weekly_target_sec {
            if t <= 0 || t > 7 * 24 * 3600 {
                return Err(AppError::invalid(
                    "A weekly target has to be between 1 second and 7 days.",
                ));
            }
        }
        let colour = match input.colour {
            Some(c) => {
                validate_colour(&c)?;
                c
            }
            None => {
                let n: i64 = self
                    .conn
                    .query_row("SELECT COUNT(*) FROM project", [], |r| r.get(0))?;
                PROJECT_PALETTE[(n as usize) % PROJECT_PALETTE.len()].to_string()
            }
        };
        let now = self.now();
        let rank: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_rank), 0) + 1024 FROM project",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1024.0);
        let id = new_id();
        self.conn.execute(
            "INSERT INTO project
               (id, name, colour, icon, kind, sort_rank, weekly_target_sec, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,NULL,?4,?5,?6,?7,?8,?8)",
            params![id, name, colour, kind, rank, input.weekly_target_sec, self.device_id, now],
        )?;
        self.project_row(&id)
    }

    pub fn update_project(&mut self, id: &str, patch: ProjectPatch) -> Result<ProjectRow> {
        validate_id(id, "project")?;
        let now = self.now();
        // Renaming goes through the same gate as naming: a project created with
        // a 120-character cap and then renamed past it would be a cap in name
        // only. The parser matches `#project` against these (§4.4 rule 7), so
        // the name is data other features read, not a label.
        if let Some(name) = &patch.name {
            let name = clean_name(name)?;
            self.conn.execute(
                "UPDATE project SET name = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, name, now],
            )?;
        }
        if let Some(colour) = &patch.colour {
            validate_colour(colour)?;
            self.conn.execute(
                "UPDATE project SET colour = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, colour, now],
            )?;
        }
        if let Some(kind) = &patch.kind {
            if !KINDS.contains(&kind.as_str()) {
                return Err(AppError::invalid(format!("'{kind}' is not a project kind.")));
            }
            self.conn.execute(
                "UPDATE project SET kind = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, kind, now],
            )?;
        }
        if let Some(archived) = patch.is_archived {
            self.conn.execute(
                "UPDATE project SET is_archived = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, archived as i64, now],
            )?;
        }
        if let Some(target) = patch.weekly_target_sec {
            self.conn.execute(
                "UPDATE project SET weekly_target_sec = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, target, now],
            )?;
        }
        self.project_row(id)
    }

    /// Deleting a project asks one question — delete its tasks, or move them to
    /// Inbox — and never guesses (§4.6). The answer arrives as `move_tasks_to_inbox`.
    pub fn delete_project(&mut self, id: &str, move_tasks_to_inbox: bool) -> Result<UndoToken> {
        validate_id(id, "project")?;
        let name: String = self
            .conn
            .query_row("SELECT name FROM project WHERE id = ?1", [id], |r| r.get(0))
            .map_err(|_| AppError::NotFound("project"))?;
        let now = self.now();
        let tx = self.conn.transaction()?;
        if move_tasks_to_inbox {
            tx.execute(
                "UPDATE task SET project_id = NULL, updated_at = ?2 WHERE project_id = ?1",
                params![id, now],
            )?;
        } else {
            tx.execute(
                "UPDATE task SET deleted_at = ?2, updated_at = ?2
                 WHERE project_id = ?1 AND deleted_at IS NULL",
                params![id, now],
            )?;
        }
        tx.execute(
            "UPDATE project SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        Ok(UndoToken {
            entity: "project".into(),
            id: id.to_string(),
            label: format!("Deleted {name}"),
            at: now,
        })
    }

    pub(crate) fn project_row(&self, id: &str) -> Result<ProjectRow> {
        let tz = self.tz_or("UTC");
        self.get_projects(&tz)?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or(AppError::NotFound("project"))
    }

    /// `(name, id)` pairs for the capture parser (§4.4 rule 7).
    pub fn project_names(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, id FROM project WHERE deleted_at IS NULL AND is_archived = 0")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

/// One gate for every path that sets a project's name, so creating and renaming
/// cannot disagree about what a name is allowed to be.
fn clean_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(AppError::invalid("A project needs a name."));
    }
    if name.chars().count() > 120 {
        return Err(AppError::invalid("Project names are limited to 120 characters."));
    }
    Ok(name.to_string())
}

/// The picker warns when a choice fails contrast (§5.2); the command layer
/// only enforces that the value is a colour at all.
pub fn validate_colour(c: &str) -> Result<()> {
    let ok = c.len() == 7
        && c.starts_with('#')
        && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
    if ok {
        Ok(())
    } else {
        Err(AppError::invalid(format!(
            "'{c}' is not a #RRGGBB colour."
        )))
    }
}
