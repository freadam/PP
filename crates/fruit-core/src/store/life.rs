//! Life areas and life entries (Plan Rev 3 §7) — confirmed non-work time.
//!
//! The workbook tracked this with cell colours and a column of hand-maintained
//! totals. Here it is two tables, so a life-area total is a `SUM` over records
//! rather than something a person keeps correct by hand.
//!
//! A `life_entry` is deliberately *not* a `time_session` with a flag:
//!
//! - it has no accumulator and no open state, because there is no timer for
//!   sleeping — it is always a closed interval asserted after the fact;
//! - it carries no `contribution`, which is how "contribution modes never apply
//!   to personal time" becomes a fact about the schema rather than a rule the
//!   UI is trusted to remember;
//! - it never links to a block, because a life entry is not work against a plan.
//!
//! Overlap between a life entry and a session is resolved on read by the
//! precedence in `day.rs`, not by refusing the write. Someone who tracks an
//! hour of work and then says they were actually at lunch is correcting the
//! record, and the correction should not require them to find and delete the
//! session first.

use rusqlite::{params, Row};

use super::Store;
use crate::error::{AppError, Result};
use crate::ids::{new_id, validate_id};
use crate::model::*;
use crate::time::{local_date, month_bounds, zone, Millis};

/// The workbook's categories. Seeded on first run so an imported month has
/// somewhere to land, and marked built-in so they cannot be deleted out from
/// under an import.
pub const BUILTIN_AREAS: &[(&str, &str, &str)] = &[
    ("Sleep/Rest", "#5B6472", "rest"),
    ("Personal/Spiritual", "#8B7FD4", "core"),
    ("Family", "#C2609B", "core"),
    ("Wellbeing", "#4FA374", "core"),
    ("Personal Development", "#3FA8BD", "core"),
    ("Community Participation", "#7FA440", "core"),
    ("Side Gig/Personal Admin", "#C98A2E", "core"),
    ("Friendship", "#5B8FC4", "core"),
    ("Team Time", "#56C2D6", "core"),
    ("Fun", "#C9603F", "entertainment"),
];

impl Store {
    pub(crate) const AREA_COLS: &'static str =
        "id, name, colour, kind, monthly_target_sec, sort_rank, is_builtin, is_archived";

    pub(crate) fn map_area(r: &Row) -> rusqlite::Result<LifeAreaRow> {
        let kind: String = r.get(3)?;
        Ok(LifeAreaRow {
            id: r.get(0)?,
            name: r.get(1)?,
            colour: r.get(2)?,
            kind: AreaKind::parse(&kind).unwrap_or(AreaKind::Other),
            monthly_target_sec: r.get(4)?,
            sort_rank: r.get(5)?,
            is_builtin: r.get::<_, i64>(6)? == 1,
            is_archived: r.get::<_, i64>(7)? == 1,
            month_tracked_sec: 0,
        })
    }

    /// Seeds the workbook's areas. Idempotent — safe on every launch, which is
    /// what lets a database created before this migration acquire them.
    pub fn seed_life_areas(&mut self) -> Result<usize> {
        let existing: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM life_area", [], |r| r.get(0))?;
        if existing > 0 {
            return Ok(0);
        }
        let now = self.now();
        let tx = self.conn.transaction()?;
        for (i, (name, colour, kind)) in BUILTIN_AREAS.iter().enumerate() {
            tx.execute(
                "INSERT INTO life_area
                   (id, name, colour, kind, sort_rank, is_builtin, device_id, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,1,?6,?7,?7)",
                params![new_id(), name, colour, kind, i as f64, self.device_id, now],
            )?;
        }
        tx.commit()?;
        Ok(BUILTIN_AREAS.len())
    }

    /// Areas with their confirmed seconds in the local month containing `today`,
    /// for target-versus-actual. The month is the plan's default reporting
    /// horizon, so it is the total the row carries.
    pub fn get_life_areas(&self, tz: &str, include_archived: bool) -> Result<Vec<LifeAreaRow>> {
        let zone_ = zone(tz)?;
        let (from, to) = month_bounds(local_date(self.now(), &zone_).as_str(), &zone_)?;

        let sql = format!(
            "SELECT {}, COALESCE((
                 SELECT SUM(MIN(e.ended_at, ?2) - MAX(e.started_at, ?1)) / 1000
                   FROM life_entry e
                  WHERE e.life_area_id = a.id AND e.deleted_at IS NULL
                    AND e.started_at < ?2 AND e.ended_at > ?1
               ), 0)
               FROM life_area a
              WHERE a.deleted_at IS NULL {}
              ORDER BY a.sort_rank, a.name",
            Self::AREA_COLS.replace("id,", "a.id,").replace(", name", ", a.name"),
            if include_archived { "" } else { "AND a.is_archived = 0" },
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from, to], |r| {
            let mut area = Self::map_area(r)?;
            area.month_tracked_sec = r.get(8)?;
            Ok(area)
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn create_life_area(&mut self, input: NewLifeArea) -> Result<LifeAreaRow> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(AppError::invalid("A life area needs a name."));
        }
        let kind = input.kind.as_deref().unwrap_or("core");
        if AreaKind::parse(kind).is_none() {
            return Err(AppError::invalid(format!("'{kind}' isn't a life-area kind.")));
        }
        let id = new_id();
        let now = self.now();
        let rank: f64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(sort_rank), 0) + 1 FROM life_area", [], |r| r.get(0))
            .unwrap_or(0.0);
        self.conn.execute(
            "INSERT INTO life_area
               (id, name, colour, kind, monthly_target_sec, sort_rank, is_builtin,
                device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,0,?7,?8,?8)",
            params![
                id,
                name,
                input.colour.as_deref().unwrap_or("#8B95A3"),
                kind,
                input.monthly_target_sec,
                rank,
                self.device_id,
                now
            ],
        )?;
        self.life_area(&id)
    }

    pub fn update_life_area(&mut self, id: &str, patch: LifeAreaPatch) -> Result<LifeAreaRow> {
        validate_id(id, "life area")?;
        let current = self.life_area(id)?;
        if let Some(kind) = patch.kind.as_deref() {
            if AreaKind::parse(kind).is_none() {
                return Err(AppError::invalid(format!("'{kind}' isn't a life-area kind.")));
            }
        }
        self.conn.execute(
            "UPDATE life_area SET name = ?2, colour = ?3, kind = ?4, monthly_target_sec = ?5,
                                  is_archived = ?6, updated_at = ?7
              WHERE id = ?1",
            params![
                id,
                patch.name.as_deref().map(str::trim).unwrap_or(&current.name),
                patch.colour.as_deref().unwrap_or(&current.colour),
                patch.kind.as_deref().unwrap_or(current.kind.as_str()),
                match patch.monthly_target_sec {
                    Some(v) => v,
                    None => current.monthly_target_sec,
                },
                patch.is_archived.unwrap_or(current.is_archived) as i64,
                self.now()
            ],
        )?;
        self.life_area(id)
    }

    /// Built-in areas archive rather than delete, and an area still holding
    /// entries refuses — orphaning a month of records to tidy a list is not a
    /// trade anyone would make knowingly.
    pub fn delete_life_area(&mut self, id: &str) -> Result<UndoToken> {
        validate_id(id, "life area")?;
        let area = self.life_area(id)?;
        if area.is_builtin {
            return Err(AppError::invalid(
                "Built-in areas can be archived but not deleted — an imported workbook maps onto them.",
            ));
        }
        let entries: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM life_entry WHERE life_area_id = ?1 AND deleted_at IS NULL",
            [id],
            |r| r.get(0),
        )?;
        if entries > 0 {
            return Err(AppError::invalid(format!(
                "'{}' still has {entries} entries. Move or delete them first, or archive the area instead.",
                area.name
            )));
        }
        let now = self.now();
        self.conn.execute(
            "UPDATE life_area SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(UndoToken {
            entity: "life_area".into(),
            id: id.to_string(),
            label: format!("Deleted '{}'", area.name),
            at: now,
        })
    }

    pub(crate) fn life_area(&self, id: &str) -> Result<LifeAreaRow> {
        self.conn
            .query_row(
                &format!("SELECT {} FROM life_area WHERE id = ?1", Self::AREA_COLS),
                [id],
                Self::map_area,
            )
            .map_err(|_| AppError::NotFound("life area"))
    }

    // ─── entries ───────────────────────────────────────────────────────

    pub(crate) const ENTRY_COLS: &'static str = "e.id, e.life_area_id, a.name, a.colour, a.kind, \
         e.label, e.started_at, e.ended_at, e.local_date, e.tz, e.is_private, e.note";

    pub(crate) fn map_entry(r: &Row) -> rusqlite::Result<LifeEntryRow> {
        let kind: String = r.get(4)?;
        Ok(LifeEntryRow {
            id: r.get(0)?,
            life_area_id: r.get(1)?,
            area_name: r.get(2)?,
            area_colour: r.get(3)?,
            area_kind: AreaKind::parse(&kind).unwrap_or(AreaKind::Other),
            label: r.get(5)?,
            started_at: r.get(6)?,
            ended_at: r.get(7)?,
            local_date: r.get(8)?,
            tz: r.get(9)?,
            is_private: r.get::<_, i64>(10)? == 1,
            note: r.get(11)?,
        })
    }

    /// Every entry overlapping the local day, not merely those that started on
    /// it — a night's sleep starts on the day before and would otherwise vanish
    /// from the morning it actually covers.
    pub fn life_entries_on(&self, date: &str, tz: &str) -> Result<Vec<LifeEntryRow>> {
        let zone_ = zone(tz)?;
        let day = crate::time::parse_date(date)?;
        let (from, to) = (
            crate::time::day_start(day, &zone_),
            crate::time::day_end(day, &zone_),
        );
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM life_entry e
               JOIN life_area a ON a.id = e.life_area_id
              WHERE e.deleted_at IS NULL AND e.started_at < ?2 AND e.ended_at > ?1
              ORDER BY e.started_at",
            Self::ENTRY_COLS
        ))?;
        let rows = stmt.query_map(params![from, to], Self::map_entry)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Records confirmed non-work time.
    ///
    /// `replace_existing` clears whatever confirmed record already covers the
    /// interval. It is never the default: replacing is destructive, so the
    /// caller has to have shown the user what is about to go (M9).
    pub fn add_life_entry(&mut self, input: NewLifeEntry) -> Result<LifeEntryRow> {
        validate_id(&input.life_area_id, "life area")?;
        self.life_area(&input.life_area_id)?;
        if input.ended_at <= input.started_at {
            return Err(AppError::invalid("A life entry has to end after it starts."));
        }
        // 24h is not arbitrary: a longer entry cannot be drawn on any day view,
        // and every real case (a weekend away) is better as one entry per day.
        if input.ended_at - input.started_at > 24 * 3600 * 1000 {
            return Err(AppError::invalid(
                "That's longer than a day. Split it into one entry per day.",
            ));
        }
        let zone_ = zone(&input.tz)?;
        let local = local_date(input.started_at, &zone_);

        if input.replace_existing {
            self.clear_interval(input.started_at, input.ended_at)?;
        }

        let id = new_id();
        let now = self.now();
        self.conn.execute(
            "INSERT INTO life_entry
               (id, life_area_id, label, started_at, ended_at, local_date, tz,
                is_private, note, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
            params![
                id,
                input.life_area_id,
                input.label.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                input.started_at,
                input.ended_at,
                local,
                input.tz,
                input.is_private as i64,
                input.note.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                self.device_id,
                now
            ],
        )?;
        self.life_entry(&id)
    }

    /// Removes confirmed records covering an interval, so a correction can take
    /// the ground back. Sessions are trimmed rather than deleted where they only
    /// partly overlap — losing an hour of tracked work to fix a ten-minute
    /// mistake is not a correction, it is a second mistake.
    fn clear_interval(&mut self, from: Millis, to: Millis) -> Result<()> {
        let now = self.now();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE life_entry SET deleted_at = ?3, updated_at = ?3
              WHERE deleted_at IS NULL AND started_at >= ?1 AND ended_at <= ?2",
            params![from, to, now],
        )?;
        tx.execute(
            "UPDATE life_entry SET ended_at = ?1, updated_at = ?3
              WHERE deleted_at IS NULL AND started_at < ?1 AND ended_at > ?1 AND ended_at <= ?2",
            params![from, to, now],
        )?;
        tx.execute(
            "UPDATE life_entry SET started_at = ?2, updated_at = ?3
              WHERE deleted_at IS NULL AND started_at >= ?1 AND started_at < ?2 AND ended_at > ?2",
            params![from, to, now],
        )?;
        // A closed session wholly inside the interval goes; one that straddles
        // it is left alone, because trimming an elapsed accumulator would make
        // the row's duration disagree with its own endpoints.
        tx.execute(
            "DELETE FROM time_session
              WHERE ended_at IS NOT NULL AND started_at >= ?1 AND ended_at <= ?2",
            params![from, to],
        )?;
        crate::db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_life_entry(&mut self, id: &str, patch: LifeEntryPatch) -> Result<LifeEntryRow> {
        validate_id(id, "life entry")?;
        let current = self.life_entry(id)?;
        let started = patch.started_at.unwrap_or(current.started_at);
        let ended = patch.ended_at.unwrap_or(current.ended_at);
        if ended <= started {
            return Err(AppError::invalid("A life entry has to end after it starts."));
        }
        if let Some(area) = patch.life_area_id.as_deref() {
            self.life_area(area)?;
        }
        let zone_ = zone(&current.tz)?;
        self.conn.execute(
            "UPDATE life_entry SET life_area_id = ?2, label = ?3, started_at = ?4, ended_at = ?5,
                                   local_date = ?6, is_private = ?7, note = ?8, updated_at = ?9
              WHERE id = ?1",
            params![
                id,
                patch.life_area_id.as_deref().unwrap_or(&current.life_area_id),
                patch.label.as_deref().or(current.label.as_deref()),
                started,
                ended,
                local_date(started, &zone_),
                patch.is_private.unwrap_or(current.is_private) as i64,
                patch.note.as_deref().or(current.note.as_deref()),
                self.now()
            ],
        )?;
        self.life_entry(id)
    }

    pub fn delete_life_entry(&mut self, id: &str) -> Result<UndoToken> {
        validate_id(id, "life entry")?;
        let entry = self.life_entry(id)?;
        let now = self.now();
        self.conn.execute(
            "UPDATE life_entry SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(UndoToken {
            entity: "life_entry".into(),
            id: id.to_string(),
            label: format!(
                "Deleted {}",
                entry.label.as_deref().unwrap_or(&entry.area_name)
            ),
            at: now,
        })
    }

    pub(crate) fn restore_life_entry(&mut self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE life_entry SET deleted_at = NULL, updated_at = ?2 WHERE id = ?1",
            params![id, self.now()],
        )?;
        Ok(())
    }

    pub(crate) fn restore_life_area(&mut self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE life_area SET deleted_at = NULL, updated_at = ?2 WHERE id = ?1",
            params![id, self.now()],
        )?;
        Ok(())
    }

    pub(crate) fn life_entry(&self, id: &str) -> Result<LifeEntryRow> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {} FROM life_entry e
                       JOIN life_area a ON a.id = e.life_area_id
                      WHERE e.id = ?1 AND e.deleted_at IS NULL",
                    Self::ENTRY_COLS
                ),
                [id],
                Self::map_entry,
            )
            .map_err(|_| AppError::NotFound("life entry"))
    }

    // ─── work contribution (work records only) ─────────────────────────

    /// Sets the contribution mode on a confirmed work session.
    ///
    /// There is no counterpart for life entries, and that is the enforcement:
    /// the only way to express "this personal time was Attend" would be to add
    /// a column, which is a code review rather than a runtime check.
    pub fn set_session_contribution(
        &mut self,
        session_id: &str,
        contribution: Option<Contribution>,
    ) -> Result<SessionRow> {
        validate_id(session_id, "session")?;
        let changed = self.conn.execute(
            "UPDATE time_session SET contribution = ?2, updated_at = ?3 WHERE id = ?1",
            params![
                session_id,
                contribution.map(|c| c.as_str()),
                self.now()
            ],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound("session"));
        }
        self.session_row(session_id)
    }

    /// Converts confirmed *work* into confirmed *life* time.
    ///
    /// This is where the plan's "changing a record from Work to a personal area
    /// clears its contribution mode" is honoured — and it is honoured by
    /// construction, because the destination table has nowhere to put one.
    pub fn convert_session_to_life(
        &mut self,
        session_id: &str,
        life_area_id: &str,
        tz: &str,
    ) -> Result<LifeEntryRow> {
        validate_id(session_id, "session")?;
        let session = self.session_row(session_id)?;
        let Some(ended_at) = session.ended_at else {
            return Err(AppError::invalid(
                "That timer is still running. Stop it first, then convert it.",
            ));
        };
        let entry = self.add_life_entry(NewLifeEntry {
            life_area_id: life_area_id.to_string(),
            label: Some(session.task_title.clone()),
            started_at: session.started_at,
            ended_at,
            tz: tz.to_string(),
            is_private: false,
            note: session.note.clone(),
            replace_existing: false,
        })?;
        // The session goes, contribution and all. Keeping both would double-count
        // the interval the moment the day is totalled.
        self.delete_session(session_id)?;
        Ok(entry)
    }
}
