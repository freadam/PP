use std::collections::HashMap;

use rusqlite::{params, params_from_iter, Row, ToSql};

use super::Store;
use crate::db;
use crate::error::{AppError, Result};
use crate::ids::{new_id, validate_id};
use crate::model::*;
use crate::time::{check_plausible, local_date, parse_date, zone};

/// §6.5: subtask depth ≤ 3. Deeper nesting is a UI problem, not a feature.
const MAX_DEPTH: usize = 3;
const MAX_TITLE: usize = 500;
/// How much of the completed tail the project page carries. Enough to see what
/// the week cost, short of loading a year of history into a list view.
const DONE_TAIL_LIMIT: u32 = 100;

/// The projection every task read shares. `now_param` is the 1-based index of
/// the "now" binding, so callers with a variable number of filter parameters
/// can put it last instead of first.
fn task_cols(now_param: usize) -> String {
    format!(
        "t.id, t.project_id, t.parent_id, t.title, t.status, t.estimate_sec, t.due_date, t.due_at,
         t.priority, t.energy, t.is_rollover, t.sort_rank, t.completed_at,
         t.created_at, t.updated_at,
         COALESCE((SELECT tracked_sec FROM task_tracked_cache c WHERE c.task_id = t.id), 0),
         (SELECT COUNT(*) FROM task s WHERE s.parent_id = t.id AND s.deleted_at IS NULL),
         (SELECT COUNT(*) FROM task s WHERE s.parent_id = t.id AND s.deleted_at IS NULL
                                       AND s.status = 'done'),
         EXISTS(SELECT 1 FROM scheduled_block b
                 WHERE b.task_id = t.id AND b.deleted_at IS NULL
                   AND b.starts_at + b.duration_sec*1000 >= ?{now_param})"
    )
}

impl Store {
    fn map_task(row: &Row) -> rusqlite::Result<TaskRow> {
        Ok(TaskRow {
            id: row.get(0)?,
            project_id: row.get(1)?,
            parent_id: row.get(2)?,
            title: row.get(3)?,
            status: Status::parse(&row.get::<_, String>(4)?).unwrap_or(Status::Open),
            estimate_sec: row.get(5)?,
            due_date: row.get(6)?,
            due_at: row.get(7)?,
            priority: row.get(8)?,
            energy: row.get(9)?,
            is_rollover: row.get::<_, i64>(10)? == 1,
            sort_rank: row.get(11)?,
            completed_at: row.get(12)?,
            created_at: row.get(13)?,
            updated_at: row.get(14)?,
            tracked_sec: row.get(15)?,
            subtask_total: row.get(16)?,
            subtask_done: row.get(17)?,
            is_scheduled: row.get::<_, i64>(18)? == 1,
            tags: Vec::new(),
        })
    }

    /// One extra query for the whole page rather than one per row — the week
    /// and list queries are budgeted in §7.1 and N+1 is how those budgets die.
    fn attach_tags(&self, rows: &mut [TaskRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; rows.len()].join(",");
        let sql = format!(
            "SELECT tt.task_id, g.id, g.name, g.colour
               FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
              WHERE tt.task_id IN ({placeholders})
              ORDER BY g.name",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let ids: Vec<&dyn ToSql> = rows.iter().map(|r| &r.id as &dyn ToSql).collect();
        let mapped = stmt.query_map(params_from_iter(ids), |r| {
            Ok((
                r.get::<_, String>(0)?,
                TagRow {
                    id: r.get(1)?,
                    name: r.get(2)?,
                    colour: r.get(3)?,
                },
            ))
        })?;
        let mut by_task: HashMap<String, Vec<TagRow>> = HashMap::new();
        for row in mapped {
            let (task_id, tag) = row?;
            by_task.entry(task_id).or_default().push(tag);
        }
        for row in rows.iter_mut() {
            if let Some(tags) = by_task.remove(&row.id) {
                row.tags = tags;
            }
        }
        Ok(())
    }

    /// A single task, hydrated the same way the list hydrates its rows.
    pub fn get_task(&self, id: &str) -> Result<TaskRow> {
        self.task_row(id)
    }

    pub(crate) fn task_row(&self, id: &str) -> Result<TaskRow> {
        let sql = format!("SELECT {} FROM task t WHERE t.id = ?2", task_cols(1));
        let mut row = self
            .conn
            .query_row(&sql, params![self.now(), id], Self::map_task)
            .map_err(|_| AppError::NotFound("task"))?;
        let mut slice = std::slice::from_mut(&mut row);
        self.attach_tags(&mut slice)?;
        Ok(row)
    }

    // ─── create / update ───────────────────────────────────────────────

    pub fn create_task(&mut self, input: NewTask) -> Result<TaskRow> {
        let title = input.title.trim().to_string();
        if title.is_empty() {
            return Err(AppError::invalid("A task needs a title."));
        }
        if title.chars().count() > MAX_TITLE {
            return Err(AppError::invalid(
                "Task titles are limited to 500 characters. Put the detail in the note.",
            ));
        }
        if let Some(p) = &input.project_id {
            validate_id(p, "project")?;
        }
        if let Some(parent) = &input.parent_id {
            validate_id(parent, "task")?;
            // `depth_of` counts the parent itself, so the child sits one lower.
            if self.depth_of(parent)? + 1 > MAX_DEPTH {
                return Err(AppError::SubtaskTooDeep);
            }
        }
        if let Some(e) = input.estimate_sec {
            if e <= 0 || e > 30 * 24 * 3600 {
                return Err(AppError::invalid("An estimate has to be between 1 second and 30 days."));
            }
        }
        if input.is_rollover && input.estimate_sec.is_some() {
            return Err(AppError::invalid(
                "A task is either estimated or a rollover, not both.",
            ));
        }
        if input.due_date.is_some() && input.due_at.is_some() {
            return Err(AppError::invalid(
                "A task has either a due date or a due time, not both.",
            ));
        }
        if let Some(d) = &input.due_date {
            parse_date(d)?;
        }
        let now = self.now();
        if let Some(at) = input.due_at {
            check_plausible(at, now)?;
        }
        let priority = input.priority.unwrap_or(0);
        if !(0..=3).contains(&priority) {
            return Err(AppError::invalid("Priority runs from 0 to 3."));
        }

        let id = new_id();
        let rank: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_rank), 0) + 1024 FROM task",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1024.0);

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO task
               (id, project_id, parent_id, title, status, estimate_sec, due_date, due_at,
                priority, energy, is_rollover, sort_rank, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,'open',?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",
            params![
                id,
                input.project_id,
                input.parent_id,
                title,
                input.estimate_sec,
                input.due_date,
                input.due_at,
                priority,
                input.energy,
                input.is_rollover as i64,
                rank,
                self.device_id,
                now
            ],
        )?;
        tx.execute(
            "INSERT INTO task_tracked_cache (task_id, tracked_sec, computed_at) VALUES (?1, 0, ?2)",
            params![id, now],
        )?;
        for tag in &input.tags {
            Self::attach_tag(&tx, &id, tag, now)?;
        }
        tx.commit()?;
        self.task_row(&id)
    }

    fn depth_of(&self, id: &str) -> Result<usize> {
        let mut depth = 0usize;
        let mut cursor = Some(id.to_string());
        while let Some(current) = cursor {
            let parent: Option<String> = self
                .conn
                .query_row("SELECT parent_id FROM task WHERE id = ?1", [&current], |r| {
                    r.get(0)
                })
                .map_err(|_| AppError::NotFound("task"))?;
            depth += 1;
            if depth > MAX_DEPTH + 1 {
                return Err(AppError::SubtaskTooDeep);
            }
            cursor = parent;
        }
        Ok(depth)
    }

    fn attach_tag(tx: &rusqlite::Transaction, task_id: &str, name: &str, now: i64) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(());
        }
        let existing: Option<String> = tx
            .query_row("SELECT id FROM tag WHERE name = ?1", [name], |r| r.get(0))
            .ok();
        let tag_id = match existing {
            Some(id) => id,
            None => {
                let id = new_id();
                tx.execute(
                    "INSERT INTO tag (id, name, created_at) VALUES (?1, ?2, ?3)",
                    params![id, name, now],
                )?;
                id
            }
        };
        tx.execute(
            "INSERT OR IGNORE INTO task_tag (task_id, tag_id) VALUES (?1, ?2)",
            params![task_id, tag_id],
        )?;
        Ok(())
    }

    pub fn update_task(&mut self, id: &str, patch: TaskPatch) -> Result<TaskRow> {
        validate_id(id, "task")?;
        let now = self.now();
        let exists: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM task WHERE id = ?1", [id], |r| r.get(0))?;
        if exists == 0 {
            return Err(AppError::NotFound("task"));
        }

        let tx = self.conn.transaction()?;
        if let Some(title) = &patch.title {
            let t = title.trim();
            if t.is_empty() {
                return Err(AppError::invalid("A task needs a title."));
            }
            tx.execute(
                "UPDATE task SET title = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, t, now],
            )?;
        }
        if let Some(project_id) = &patch.project_id {
            if let Some(p) = project_id {
                validate_id(p, "project")?;
            }
            tx.execute(
                "UPDATE task SET project_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, project_id, now],
            )?;
        }
        if let Some(estimate) = patch.estimate_sec {
            if let Some(e) = estimate {
                if e <= 0 {
                    return Err(AppError::invalid("An estimate has to be positive."));
                }
            }
            // An estimate and a rollover are mutually exclusive, so setting one
            // clears the other in the same statement — there is no window in
            // which both are true.
            tx.execute(
                "UPDATE task SET estimate_sec = ?2, is_rollover = 0, updated_at = ?3
                  WHERE id = ?1",
                params![id, estimate, now],
            )?;
        }
        // due_date and due_at are mutually exclusive (§6.2 CHECK); setting one
        // always clears the other so the constraint can never be hit.
        if let Some(due_date) = &patch.due_date {
            if let Some(d) = due_date {
                parse_date(d)?;
            }
            tx.execute(
                "UPDATE task SET due_date = ?2, due_at = NULL, updated_at = ?3 WHERE id = ?1",
                params![id, due_date, now],
            )?;
        }
        if let Some(due_at) = patch.due_at {
            if let Some(at) = due_at {
                check_plausible(at, now)?;
            }
            tx.execute(
                "UPDATE task SET due_at = ?2, due_date = NULL, updated_at = ?3 WHERE id = ?1",
                params![id, due_at, now],
            )?;
        }
        if let Some(priority) = patch.priority {
            if !(0..=3).contains(&priority) {
                return Err(AppError::invalid("Priority runs from 0 to 3."));
            }
            tx.execute(
                "UPDATE task SET priority = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, priority, now],
            )?;
        }
        if let Some(energy) = &patch.energy {
            if let Some(e) = energy {
                if !["low", "mid", "high"].contains(&e.as_str()) {
                    return Err(AppError::invalid("Energy is low, mid or high."));
                }
            }
            tx.execute(
                "UPDATE task SET energy = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, energy, now],
            )?;
        }
        if let Some(rollover) = patch.is_rollover {
            tx.execute(
                "UPDATE task SET is_rollover = ?2,
                        estimate_sec = CASE WHEN ?2 = 1 THEN NULL ELSE estimate_sec END,
                        updated_at = ?3
                  WHERE id = ?1",
                params![id, rollover as i64, now],
            )?;
        }
        if let Some(rank) = patch.sort_rank {
            tx.execute(
                "UPDATE task SET sort_rank = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, rank, now],
            )?;
        }
        if let Some(tags) = &patch.tags {
            tx.execute("DELETE FROM task_tag WHERE task_id = ?1", [id])?;
            for tag in tags {
                Self::attach_tag(&tx, id, tag, now)?;
            }
        }
        tx.commit()?;
        self.task_row(id)
    }

    /// `status='done' ⟺ completed_at IS NOT NULL` is a CHECK constraint, so the
    /// two are always written together (§6.5).
    pub fn set_task_status(&mut self, id: &str, status: Status) -> Result<TaskRow> {
        validate_id(id, "task")?;
        let now = self.now();
        let completed_at = (status == Status::Done).then_some(now);
        let n = self.conn.execute(
            "UPDATE task SET status = ?2, completed_at = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, status.as_str(), completed_at, now],
        )?;
        if n == 0 {
            return Err(AppError::NotFound("task"));
        }
        self.task_row(id)
    }

    pub fn delete_task(&mut self, id: &str) -> Result<UndoToken> {
        validate_id(id, "task")?;
        let title: String = self
            .conn
            .query_row("SELECT title FROM task WHERE id = ?1", [id], |r| r.get(0))
            .map_err(|_| AppError::NotFound("task"))?;
        let now = self.now();
        let tx = self.conn.transaction()?;
        // Soft delete cascades to subtasks by hand — ON DELETE CASCADE only
        // fires for hard deletes, and a soft-deleted parent with live children
        // would leave orphans in the list.
        tx.execute(
            "WITH RECURSIVE sub(id) AS (
               SELECT ?1
               UNION ALL
               SELECT t.id FROM task t JOIN sub ON t.parent_id = sub.id
             )
             UPDATE task SET deleted_at = ?2, updated_at = ?2
              WHERE id IN (SELECT id FROM sub) AND deleted_at IS NULL",
            params![id, now],
        )?;
        tx.execute(
            "UPDATE scheduled_block SET deleted_at = ?2, updated_at = ?2
              WHERE task_id = ?1 AND deleted_at IS NULL",
            params![id, now],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        Ok(UndoToken {
            entity: "task".into(),
            id: id.to_string(),
            label: format!("Deleted {title}"),
            at: now,
        })
    }

    // ─── read ──────────────────────────────────────────────────────────

    pub fn get_tasks(&self, query: TaskQuery) -> Result<Page<TaskRow>> {
        let mut where_parts: Vec<String> = vec!["t.deleted_at IS NULL".into()];
        // Filter bindings come first; "now" is appended last so the COUNT query
        // — which needs no "now" — can reuse the same indices.
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();

        match query.scope.as_deref().unwrap_or("open") {
            "open" => where_parts.push("t.status = 'open'".into()),
            "done" => where_parts.push("t.status = 'done'".into()),
            "deleted" => {
                where_parts.clear();
                where_parts.push("t.deleted_at IS NOT NULL".into());
            }
            _ => {}
        }
        if !query.include_subtasks {
            where_parts.push("t.parent_id IS NULL".into());
        }
        if let Some(project_id) = &query.project_id {
            if project_id == "inbox" {
                where_parts.push("t.project_id IS NULL".into());
            } else {
                validate_id(project_id, "project")?;
                args.push(Box::new(project_id.clone()));
                where_parts.push(format!("t.project_id = ?{}", args.len()));
            }
        }
        if let Some(tag) = &query.tag {
            args.push(Box::new(tag.clone()));
            where_parts.push(format!(
                "EXISTS(SELECT 1 FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
                         WHERE tt.task_id = t.id AND g.name = ?{})",
                args.len()
            ));
        }
        if let Some(text) = &query.text {
            let needle = format!("%{}%", text.trim());
            args.push(Box::new(needle));
            where_parts.push(format!("t.title LIKE ?{} COLLATE NOCASE", args.len()));
        }

        let where_sql = where_parts.join(" AND ");
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM task t WHERE {where_sql}"),
            params_from_iter(args.iter().map(|a| a.as_ref())),
            |r| r.get(0),
        )?;

        let limit = query.limit.unwrap_or(200).min(1000);
        let offset = query.offset.unwrap_or(0);
        args.push(Box::new(self.now()));
        let sql = format!(
            "SELECT {} FROM task t
              WHERE {where_sql}
              ORDER BY t.status ASC, t.completed_at DESC, t.priority DESC, t.sort_rank ASC
              LIMIT {limit} OFFSET {offset}",
            task_cols(args.len())
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mapped = stmt.query_map(
            params_from_iter(args.iter().map(|a| a.as_ref())),
            Self::map_task,
        )?;
        let mut rows: Vec<TaskRow> = mapped.collect::<std::result::Result<_, _>>()?;
        self.attach_tags(&mut rows)?;
        Ok(Page { rows, total })
    }

    /// §3.2 grouping: Overdue · Today · This week · No date · Someday, each
    /// with a count *and* a total estimate — a planner without a total is
    /// useless for capacity checks.
    pub fn get_backlog(&self, filter: BacklogFilter, tz: &str) -> Result<BacklogView> {
        let zone = zone(tz)?;
        let now = self.now();
        let today = filter
            .today
            .clone()
            .unwrap_or_else(|| local_date(now, &zone));
        let today_date = parse_date(&today)?;
        let week_end = today_date + chrono::Duration::days(6);

        let base = TaskQuery {
            project_id: filter.project_id.clone(),
            tag: filter.tag.clone(),
            include_subtasks: false,
            limit: Some(1000),
            ..Default::default()
        };
        let mut tasks = self
            .get_tasks(TaskQuery {
                scope: Some("open".into()),
                ..base.clone()
            })?
            .rows;

        if filter.unscheduled_only {
            tasks.retain(|t| !t.is_scheduled);
        }

        // Completed work goes to the bottom, not away: it is the record of what
        // the project actually cost, and hiding it makes a finished project look
        // empty. Capped, because the tail grows forever.
        let done = self
            .get_tasks(TaskQuery {
                scope: Some("done".into()),
                limit: Some(DONE_TAIL_LIMIT),
                ..base
            })?
            .rows;
        tasks.extend(done);

        let bucket_of = |t: &TaskRow| -> &'static str {
            if t.status != Status::Open {
                return "done";
            }
            let date = t
                .due_date
                .clone()
                .or_else(|| t.due_at.map(|at| local_date(at, &zone)));
            match date {
                None => "no-date",
                Some(d) => match parse_date(&d) {
                    Ok(d) if d < today_date => "overdue",
                    Ok(d) if d == today_date => "today",
                    Ok(d) if d <= week_end => "week",
                    Ok(_) => "someday",
                    Err(_) => "no-date",
                },
            }
        };

        let order = [
            ("overdue", "Overdue"),
            ("today", "Today"),
            ("week", "This week"),
            ("no-date", "No date"),
            ("someday", "Someday"),
            ("done", "Completed"),
        ];
        let groups = order
            .iter()
            .map(|(key, label)| {
                let members: Vec<&TaskRow> =
                    tasks.iter().filter(|t| bucket_of(t) == *key).collect();
                TaskGroup {
                    key: (*key).to_string(),
                    label: (*label).to_string(),
                    count: members.len() as i64,
                    estimate_sec: members.iter().filter_map(|t| t.estimate_sec).sum(),
                    task_ids: members.iter().map(|t| t.id.clone()).collect(),
                }
            })
            .collect();

        Ok(BacklogView { groups, tasks })
    }

    pub fn get_task_detail(&self, id: &str) -> Result<TaskDetail> {
        validate_id(id, "task")?;
        let task = self.task_row(id)?;
        let (note, note_updated_at): (String, Option<i64>) = self
            .conn
            .query_row(
                "SELECT markdown, updated_at FROM note WHERE task_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or_default();

        let sql = format!(
            "SELECT {} FROM task t
              WHERE t.parent_id = ?2 AND t.deleted_at IS NULL
              ORDER BY t.sort_rank",
            task_cols(1)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mapped = stmt.query_map(params![self.now(), id], Self::map_task)?;
        let mut subtasks: Vec<TaskRow> = mapped.collect::<std::result::Result<_, _>>()?;
        self.attach_tags(&mut subtasks)?;

        let project = match &task.project_id {
            Some(p) => self.project_row(p).ok(),
            None => None,
        };

        Ok(TaskDetail {
            sessions: self.sessions_for_task(id)?,
            blocks: self.blocks_for_task(id)?,
            subtasks,
            note,
            note_updated_at,
            project,
            task,
        })
    }

    /// Debounced in the renderer (500ms, 3s maxWait) and force-flushed on blur,
    /// view change and close (§6.9) — taken literally, "auto-saving" would mean
    /// a database write per keystroke.
    pub fn save_note(&mut self, task_id: &str, markdown: &str) -> Result<()> {
        validate_id(task_id, "task")?;
        if markdown.len() > 1_000_000 {
            return Err(AppError::invalid("Notes are limited to 1MB."));
        }
        let now = self.now();
        let n = self.conn.execute(
            "INSERT INTO note (task_id, markdown, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(task_id) DO UPDATE SET markdown = excluded.markdown,
                                                updated_at = excluded.updated_at",
            params![task_id, markdown, now],
        )?;
        if n == 0 {
            return Err(AppError::NotFound("task"));
        }
        Ok(())
    }

    pub fn get_tags(&self) -> Result<Vec<TagRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, colour FROM tag ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok(TagRow {
                id: r.get(0)?,
                name: r.get(1)?,
                colour: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}
