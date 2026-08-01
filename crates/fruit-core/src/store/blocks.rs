use rusqlite::{params, Row};

use super::Store;
use crate::db;
use crate::error::{AppError, Result};
use crate::ids::{new_id, validate_id};
use crate::model::*;
use crate::time::{
    check_plausible, day_end, day_start, local_date, parse_date, same_day_span, zone, Millis,
};

const MIN_DURATION: i64 = 300; // 5 minutes
const MAX_DURATION: i64 = 43_200; // 12 hours

impl Store {
    pub(crate) fn map_block(row: &Row) -> rusqlite::Result<BlockRow> {
        Ok(BlockRow {
            id: row.get(0)?,
            task_id: row.get(1)?,
            label: row.get(2)?,
            starts_at: row.get(3)?,
            duration_sec: row.get(4)?,
            local_date: row.get(5)?,
            tz: row.get(6)?,
            is_fixed: row.get::<_, i64>(7)? == 1,
            series_id: row.get(8)?,
            rrule: row.get(9)?,
            external_uid: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }

    pub(crate) const BLOCK_COLS: &'static str =
        "id, task_id, label, starts_at, duration_sec, local_date, tz, is_fixed,
         series_id, rrule, external_uid, created_at, updated_at";

    pub fn schedule_block(&mut self, input: NewBlock) -> Result<BlockRow> {
        // A rule on the input means a series; `schedule_recurring` validates the
        // seed through this same function, so there is no second validation path.
        if let Some(rule) = input.rrule.clone() {
            let mut seed = input;
            seed.rrule = None;
            return Ok(self.schedule_recurring(seed, &rule)?.remove(0));
        }
        let now = self.now();
        let zone = zone(&input.tz)?;
        check_plausible(input.starts_at, now)?;
        if !(MIN_DURATION..=MAX_DURATION).contains(&input.duration_sec) {
            return Err(AppError::invalid(
                "A block runs from 5 minutes to 12 hours. Split anything longer.",
            ));
        }
        if input.task_id.is_none() && input.label.as_deref().map(str::trim).unwrap_or("").is_empty()
        {
            return Err(AppError::invalid(
                "A block needs either a task or a label.",
            ));
        }
        if let Some(task_id) = &input.task_id {
            validate_id(task_id, "task")?;
            let exists: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM task WHERE id = ?1 AND deleted_at IS NULL",
                [task_id],
                |r| r.get(0),
            )?;
            if exists == 0 {
                return Err(AppError::NotFound("task"));
            }
        }
        let local_date = same_day_span(input.starts_at, input.duration_sec, &zone)?;

        let id = new_id();
        self.conn.execute(
            "INSERT INTO scheduled_block
               (id, task_id, label, starts_at, duration_sec, local_date, tz, is_fixed,
                device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
            params![
                id,
                input.task_id,
                input.label.as_ref().map(|l| l.trim()),
                input.starts_at,
                input.duration_sec,
                local_date,
                input.tz,
                input.is_fixed as i64,
                self.device_id,
                now
            ],
        )?;
        self.conn.execute(
            "INSERT INTO block_tracked_cache (block_id, tracked_sec, computed_at) VALUES (?1, 0, ?2)",
            params![id, now],
        )?;
        self.block_row(&id)
    }

    /// `None` when the block is gone or soft-deleted — the shape tests want.
    pub fn block_row_public(&self, id: &str) -> Option<BlockRow> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {} FROM scheduled_block WHERE id = ?1 AND deleted_at IS NULL",
                    Self::BLOCK_COLS
                ),
                [id],
                Self::map_block,
            )
            .ok()
    }

    pub(crate) fn block_row(&self, id: &str) -> Result<BlockRow> {
        self.conn
            .query_row(
                &format!("SELECT {} FROM scheduled_block WHERE id = ?1", Self::BLOCK_COLS),
                [id],
                Self::map_block,
            )
            .map_err(|_| AppError::NotFound("block"))
    }

    pub(crate) fn blocks_for_task(&self, task_id: &str) -> Result<Vec<BlockRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM scheduled_block
              WHERE task_id = ?1 AND deleted_at IS NULL ORDER BY starts_at",
            Self::BLOCK_COLS
        ))?;
        let rows = stmt.query_map([task_id], Self::map_block)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn blocks_on(&self, local_date: &str) -> Result<Vec<BlockRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM scheduled_block
              WHERE local_date = ?1 AND deleted_at IS NULL ORDER BY starts_at",
            Self::BLOCK_COLS
        ))?;
        let rows = stmt.query_map([local_date], Self::map_block)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Returns every block the move touched, so the renderer can reconcile a
    /// cascade in one patch instead of refetching the week.
    pub fn move_block(
        &mut self,
        id: &str,
        starts_at: Millis,
        policy: CollisionPolicy,
    ) -> Result<Vec<BlockRow>> {
        validate_id(id, "block")?;
        let block = self.block_row(id)?;
        let now = self.now();
        check_plausible(starts_at, now)?;
        let zone = zone(&block.tz)?;

        let mut duration = block.duration_sec;
        if policy == CollisionPolicy::Shrink {
            let gap = self.gap_after(starts_at, &block.tz, Some(id))?;
            if gap < MIN_DURATION {
                return Err(AppError::invalid(
                    "There isn't 5 minutes free there. Drop it somewhere with more room.",
                ));
            }
            duration = duration.min(gap);
        }
        let new_date = same_day_span(starts_at, duration, &zone)?;

        let tx = self.conn.transaction()?;
        let mut touched = vec![id.to_string()];

        if policy == CollisionPolicy::Push {
            let delta_end = starts_at + duration * 1000;
            let mut stmt = tx.prepare(&format!(
                "SELECT {} FROM scheduled_block
                  WHERE local_date = ?1 AND deleted_at IS NULL AND id <> ?2
                  ORDER BY starts_at",
                Self::BLOCK_COLS
            ))?;
            let rows = stmt.query_map(params![new_date, id], Self::map_block)?;
            let others: Vec<BlockRow> = rows.collect::<std::result::Result<_, _>>()?;
            drop(stmt);

            let mut frontier = delta_end;
            for other in others {
                let other_end = other.starts_at + other.duration_sec * 1000;
                if other_end <= starts_at || other.starts_at >= frontier {
                    continue;
                }
                if other.is_fixed {
                    // Fixed blocks are never pushed and never auto-shortened
                    // (§4.3). Stop rather than quietly overlapping.
                    return Err(AppError::invalid(format!(
                        "'{}' is fixed and won't move. Drop this somewhere else, or unfix it first.",
                        other.label.clone().unwrap_or_else(|| "A fixed block".into())
                    )));
                }
                let pushed_to = frontier;
                let pushed_date = same_day_span(pushed_to, other.duration_sec, &zone)?;
                tx.execute(
                    "UPDATE scheduled_block SET starts_at = ?2, local_date = ?3, updated_at = ?4
                      WHERE id = ?1",
                    params![other.id, pushed_to, pushed_date, now],
                )?;
                frontier = pushed_to + other.duration_sec * 1000;
                touched.push(other.id.clone());
            }
        }

        tx.execute(
            "UPDATE scheduled_block
                SET starts_at = ?2, duration_sec = ?3, local_date = ?4, updated_at = ?5
              WHERE id = ?1",
            params![id, starts_at, duration, new_date, now],
        )?;
        tx.commit()?;

        touched.iter().map(|t| self.block_row(t)).collect()
    }

    pub fn resize_block(&mut self, id: &str, duration_sec: i64) -> Result<BlockRow> {
        validate_id(id, "block")?;
        if !(MIN_DURATION..=MAX_DURATION).contains(&duration_sec) {
            return Err(AppError::invalid(
                "A block runs from 5 minutes to 12 hours. Split anything longer.",
            ));
        }
        let block = self.block_row(id)?;
        let zone = zone(&block.tz)?;
        let local_date = same_day_span(block.starts_at, duration_sec, &zone)?;
        self.conn.execute(
            "UPDATE scheduled_block SET duration_sec = ?2, local_date = ?3, updated_at = ?4
              WHERE id = ?1",
            params![id, duration_sec, local_date, self.now()],
        )?;
        self.block_row(id)
    }

    pub fn set_block_fixed(&mut self, id: &str, is_fixed: bool) -> Result<BlockRow> {
        validate_id(id, "block")?;
        self.conn.execute(
            "UPDATE scheduled_block SET is_fixed = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, is_fixed as i64, self.now()],
        )?;
        self.block_row(id)
    }

    pub fn unschedule_block(&mut self, id: &str) -> Result<UndoToken> {
        validate_id(id, "block")?;
        let block = self.block_row(id)?;
        let now = self.now();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE scheduled_block SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        // Sessions keep their record; they simply stop being attributed to a
        // block. Intentions and records never merge (§6.1 rule 7).
        tx.execute(
            "UPDATE time_session SET block_id = NULL, updated_at = ?2 WHERE block_id = ?1",
            params![id, now],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;

        let label = match &block.task_id {
            Some(task_id) => self
                .conn
                .query_row("SELECT title FROM task WHERE id = ?1", [task_id], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap_or_else(|_| "block".into()),
            None => block.label.clone().unwrap_or_else(|| "block".into()),
        };
        Ok(UndoToken {
            entity: "block".into(),
            id: id.to_string(),
            label: format!("Unscheduled {label}"),
            at: now,
        })
    }

    pub fn duplicate_block(&mut self, id: &str) -> Result<BlockRow> {
        let b = self.block_row(id)?;
        self.schedule_block(NewBlock {
            task_id: b.task_id,
            label: b.label,
            starts_at: b.starts_at + b.duration_sec * 1000,
            duration_sec: b.duration_sec,
            tz: b.tz,
            is_fixed: false,
            rrule: None,
        })
    }

    /// Free seconds between `from` and the next block that starts after it.
    fn gap_after(&self, from: Millis, tz: &str, ignoring: Option<&str>) -> Result<i64> {
        let zone = zone(tz)?;
        let date = local_date(from, &zone);
        let day = parse_date(&date)?;
        let end_of_day = day_end(day, &zone);
        let ignore = ignoring.unwrap_or("");
        let next: Option<i64> = self
            .conn
            .query_row(
                "SELECT MIN(starts_at) FROM scheduled_block
                  WHERE local_date = ?1 AND deleted_at IS NULL AND id <> ?2 AND starts_at > ?3",
                params![date, ignore, from],
                |r| r.get(0),
            )
            .unwrap_or(None);
        let boundary = next.unwrap_or(end_of_day).min(end_of_day);
        Ok(((boundary - from) / 1000).max(0))
    }

    /// §3.7 auto-suggest: the first gap ≥ `duration_sec` on `date`, at or after
    /// `not_before`, respecting every existing block (fixed or not).
    pub fn next_free_slot(
        &self,
        date: &str,
        duration_sec: i64,
        not_before: Option<Millis>,
        tz: &str,
    ) -> Result<Option<Millis>> {
        let zone = zone(tz)?;
        let day = parse_date(date)?;
        let start_of_day = day_start(day, &zone);
        let end_of_day = day_end(day, &zone);
        // Don't suggest the small hours: start no earlier than 08:00 local
        // unless the caller asks for earlier.
        let default_start = start_of_day + 8 * 3_600_000;
        let mut cursor = not_before.unwrap_or(default_start).max(start_of_day);

        let blocks = self.blocks_on(date)?;
        for b in blocks {
            let b_end = b.starts_at + b.duration_sec * 1000;
            if b_end <= cursor {
                continue;
            }
            if b.starts_at - cursor >= duration_sec * 1000 {
                return Ok(Some(cursor));
            }
            cursor = cursor.max(b_end);
        }
        if end_of_day - cursor >= duration_sec * 1000 {
            return Ok(Some(cursor));
        }
        Ok(None)
    }
}
