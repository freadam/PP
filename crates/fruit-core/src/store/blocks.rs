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
            // A row written before 0009 reads back as `work`, which is what it
            // was; an unrecognised string can only mean a newer schema, and the
            // migrator has already refused that, so `work` is the safe floor.
            intent: BlockIntent::parse(&row.get::<_, String>(11)?).unwrap_or_default(),
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }

    pub(crate) const BLOCK_COLS: &'static str =
        "id, task_id, label, starts_at, duration_sec, local_date, tz, is_fixed,
         series_id, rrule, external_uid, intent, created_at, updated_at";

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
                intent, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
            params![
                id,
                input.task_id,
                input.label.as_ref().map(|l| l.trim()),
                input.starts_at,
                input.duration_sec,
                local_date,
                input.tz,
                input.is_fixed as i64,
                input.intent.unwrap_or_default().as_str(),
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
            intent: Some(b.intent),
        })
    }

    /// Reclassify a plotted interval — the evening you blocked out for work and
    /// then decided to spend on a film. Changing this changes what the month
    /// dashboard counts as *planned* entertainment from that moment on; it does
    /// not touch anything already recorded against the block.
    pub fn set_block_intent(&mut self, id: &str, intent: BlockIntent) -> Result<BlockRow> {
        validate_id(id, "block")?;
        let n = self.conn.execute(
            "UPDATE scheduled_block SET intent = ?2, updated_at = ?3
              WHERE id = ?1 AND deleted_at IS NULL",
            params![id, intent.as_str(), self.now()],
        )?;
        if n == 0 {
            return Err(AppError::NotFound("block"));
        }
        self.block_row(id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;
    use crate::time::local_date;
    use std::sync::Arc;

    /// Monday 2026-08-03, 09:00 UTC — the same anchor the goals tests use, so a
    /// failure here and a failure there are talking about the same week.
    const MONDAY: i64 = 1_785_747_600_000;
    const TZ: &str = "UTC";

    fn store() -> Store {
        Store::in_memory_with_clock(Arc::new(TestClock::new(MONDAY))).unwrap()
    }

    fn plot(s: &mut Store, hour: i64, hours: i64, intent: Option<BlockIntent>) -> BlockRow {
        s.schedule_block(NewBlock {
            task_id: None,
            label: Some("Something".into()),
            starts_at: MONDAY + (hour - 9) * 3_600_000,
            duration_sec: hours * 3600,
            tz: TZ.into(),
            is_fixed: false,
            rrule: None,
            intent,
        })
        .unwrap()
    }

    /// The reason the column defaults rather than being required: every block
    /// plotted before 0009 existed was work, and reading one back must not
    /// reclassify it.
    #[test]
    fn a_block_plotted_without_an_intent_is_work() {
        let mut s = store();
        let b = plot(&mut s, 10, 1, None);
        assert_eq!(b.intent, BlockIntent::Work);
        assert_eq!(s.block_row(&b.id).unwrap().intent, BlockIntent::Work);
    }

    #[test]
    fn an_entertainment_window_survives_the_round_trip() {
        let mut s = store();
        let b = plot(&mut s, 20, 2, Some(BlockIntent::Entertainment));
        assert_eq!(s.block_row(&b.id).unwrap().intent, BlockIntent::Entertainment);
        // And through the Day view's plan overlay, which reads its own
        // column list rather than `BLOCK_COLS`.
        let today = local_date(s.now(), &crate::time::zone(TZ).unwrap());
        let day = s.get_day(&today, TZ, None).unwrap();
        let plan = day
            .slots
            .iter()
            .flat_map(|slot| slot.plans.iter())
            .find(|p| p.block_id == b.id)
            .expect("the window is on the day");
        assert_eq!(plan.intent, BlockIntent::Entertainment);

        // And through the week query, whose column list is written by hand.
        let week = s
            .get_week(&DateRange { from: today.clone(), to: today.clone() }, TZ)
            .unwrap();
        let view = week
            .days
            .iter()
            .flat_map(|d| d.blocks.iter())
            .find(|v| v.block.id == b.id)
            .expect("the window is in the week");
        assert_eq!(view.block.intent, BlockIntent::Entertainment);
    }

    /// M11's arithmetic: an evening you *meant* to spend on a film is planned
    /// entertainment, and a morning of work is not — even though both are
    /// planned time.
    #[test]
    fn planned_entertainment_counts_only_entertainment_windows() {
        let mut s = store();
        plot(&mut s, 9, 3, None); // work
        plot(&mut s, 20, 2, Some(BlockIntent::Entertainment));
        plot(&mut s, 18, 1, Some(BlockIntent::Life)); // dinner: planned, not entertainment

        let today = local_date(s.now(), &crate::time::zone(TZ).unwrap());
        let t = s.get_day(&today, TZ, None).unwrap().totals;
        assert_eq!(t.planned_sec, 6 * 3600);
        assert_eq!(t.planned_entertainment_sec, 2 * 3600);

        // And the month is the same arithmetic, summed.
        let month = s.get_month(&today, TZ).unwrap();
        assert_eq!(month.totals.planned_entertainment_sec, 2 * 3600);
        let day = month
            .days
            .iter()
            .find(|d| d.local_date == today)
            .expect("today is in this month");
        assert_eq!(day.planned_entertainment_sec, 2 * 3600);
    }

    /// Planned time is an overlay, not a layer of the timeline — so adding an
    /// entertainment window must not move a single second of the day's
    /// accounting. This is the counting invariant, guarded from the one
    /// direction 0009 could have broken it.
    #[test]
    fn planning_a_window_does_not_touch_what_the_day_counts() {
        let mut s = store();
        let today = local_date(s.now(), &crate::time::zone(TZ).unwrap());
        let before = s.get_day(&today, TZ, None).unwrap().totals;

        plot(&mut s, 20, 2, Some(BlockIntent::Entertainment));
        let after = s.get_day(&today, TZ, None).unwrap().totals;

        assert_eq!(after.entertainment_sec, before.entertainment_sec);
        assert_eq!(after.empty_sec, before.empty_sec);
        assert_eq!(after.day_sec, before.day_sec);
        // Only the overlay moved.
        assert_eq!(after.planned_entertainment_sec, 2 * 3600);
    }

    #[test]
    fn a_window_can_be_reclassified_after_the_fact() {
        let mut s = store();
        let b = plot(&mut s, 20, 2, None);
        let changed = s.set_block_intent(&b.id, BlockIntent::Entertainment).unwrap();
        assert_eq!(changed.intent, BlockIntent::Entertainment);

        let today = local_date(s.now(), &crate::time::zone(TZ).unwrap());
        let t = s.get_day(&today, TZ, None).unwrap().totals;
        assert_eq!(t.planned_entertainment_sec, 2 * 3600);

        // A well-formed id that names nothing is not found, rather than
        // silently succeeding against zero rows.
        assert!(matches!(
            s.set_block_intent(&new_id(), BlockIntent::Work),
            Err(AppError::NotFound("block"))
        ));

        // And a soft-deleted block is gone for this purpose too.
        s.unschedule_block(&b.id).unwrap();
        assert!(matches!(
            s.set_block_intent(&b.id, BlockIntent::Work),
            Err(AppError::NotFound("block"))
        ));
    }

    /// M11, stated as arithmetic: entertainment that happened inside a window
    /// you plotted, plus entertainment that did not, is all the entertainment
    /// there was. The dashboard's two lines are that split, so they cannot
    /// drift from the total they came from.
    #[test]
    fn entertainment_reconciles_to_planned_and_unplanned() {
        let mut s = store();
        let tv = s
            .create_life_area(NewLifeArea {
                name: "Television".into(),
                colour: Some("#8b5cf6".into()),
                kind: Some("entertainment".into()),
                monthly_target_sec: None,
            })
            .unwrap();

        // A two-hour window plotted for the evening…
        plot(&mut s, 20, 2, Some(BlockIntent::Entertainment));
        // …an hour of which was used, plus an unplanned hour at lunchtime.
        let mut watch = |hour: i64, hours: i64| {
            s.add_life_entry(NewLifeEntry {
                life_area_id: tv.id.clone(),
                label: Some("Watching".into()),
                started_at: MONDAY + (hour - 9) * 3_600_000,
                ended_at: MONDAY + (hour - 9 + hours) * 3_600_000,
                tz: TZ.into(),
                is_private: false,
                note: None,
                replace_existing: false,
            })
            .unwrap();
        };
        watch(20, 1);
        watch(13, 1);

        let today = local_date(s.now(), &crate::time::zone(TZ).unwrap());
        let t = s.get_day(&today, TZ, None).unwrap().totals;

        assert_eq!(t.entertainment_sec, 2 * 3600);
        assert_eq!(t.entertainment_in_window_sec, 3600, "one hour was inside the window");
        assert_eq!(
            t.entertainment_sec - t.entertainment_in_window_sec,
            3600,
            "and one hour was not"
        );
        // The window was two hours; only one of them was used. Planned time is
        // an intention, not a record, and the two are never merged.
        assert_eq!(t.planned_entertainment_sec, 2 * 3600);
    }

    /// The split is a partition, never an overcount: an hour of entertainment
    /// under two overlapping windows is still one hour.
    #[test]
    fn overlapping_windows_do_not_double_count() {
        let mut s = store();
        let tv = s
            .create_life_area(NewLifeArea {
                name: "Television".into(),
                colour: Some("#8b5cf6".into()),
                kind: Some("entertainment".into()),
                monthly_target_sec: None,
            })
            .unwrap();
        plot(&mut s, 20, 2, Some(BlockIntent::Entertainment));
        plot(&mut s, 21, 2, Some(BlockIntent::Entertainment));
        s.add_life_entry(NewLifeEntry {
            life_area_id: tv.id.clone(),
            label: Some("Watching".into()),
            started_at: MONDAY + 11 * 3_600_000, // 20:00
            ended_at: MONDAY + 14 * 3_600_000,   // 23:00
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();

        let today = local_date(s.now(), &crate::time::zone(TZ).unwrap());
        let t = s.get_day(&today, TZ, None).unwrap().totals;
        assert_eq!(t.entertainment_sec, 3 * 3600);
        // 20:00–23:00 watched; windows cover 20:00–23:00 between them.
        assert_eq!(t.entertainment_in_window_sec, 3 * 3600);
    }

    /// A standing Friday film night is entertainment every Friday, not just the
    /// first one.
    #[test]
    fn every_instance_of_a_series_inherits_the_seeds_intent() {
        let mut s = store();
        let created = s
            .schedule_recurring(
                NewBlock {
                    task_id: None,
                    label: Some("Film night".into()),
                    starts_at: MONDAY + 11 * 3_600_000, // 20:00
                    duration_sec: 2 * 3600,
                    tz: TZ.into(),
                    is_fixed: false,
                    rrule: None,
                    intent: Some(BlockIntent::Entertainment),
                },
                "FREQ=WEEKLY;BYDAY=MO;COUNT=4",
            )
            .unwrap();
        assert_eq!(created.len(), 4);
        assert!(created.iter().all(|b| b.intent == BlockIntent::Entertainment));
    }

    /// The database refuses a value the enum cannot name, so a bad write is a
    /// failed write rather than a row nobody can classify.
    #[test]
    fn the_column_refuses_an_intent_that_is_not_one() {
        let mut s = store();
        let b = plot(&mut s, 10, 1, None);
        let bad = s.conn.execute(
            "UPDATE scheduled_block SET intent = 'leisure' WHERE id = ?1",
            [&b.id],
        );
        assert!(bad.is_err(), "the CHECK constraint should have refused this");
    }
}
