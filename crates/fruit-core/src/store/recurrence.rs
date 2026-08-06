//! Recurring blocks and `.ics` import (§2.3 PLAN, both P2).
//!
//! **Instances are materialised, not computed on read.** That is the load-bearing
//! decision here, and it follows from §6.1 rule 7: a `scheduled_block` is an
//! intention, and a `time_session` links to one by id. A virtual instance has no
//! id, so it cannot be tracked against, cannot carry drift, and cannot appear in
//! Reconcile — which would make a recurring block a second-class block that
//! silently does less. Real rows, sharing a `series_id`, keep every existing
//! feature working without knowing recurrence exists.
//!
//! The cost is a horizon: rows exist for 90 days ahead and are topped up
//! whenever the planner is asked for a week beyond them.

use chrono::{Duration, NaiveDate};
use rusqlite::params;

use super::Store;
use crate::error::{AppError, Result};
use crate::ics;
use crate::ids::{new_id, validate_id};
use crate::model::*;
use crate::rrule::Rrule;
use crate::time::{format_date, local_date, parse_date, same_day_span, to_local, zone};

/// How far ahead instances are materialised. Long enough that nobody scrolls
/// past the edge in normal use, short enough that a daily rule costs ~90 rows.
pub const HORIZON_DAYS: i64 = 90;

impl Store {
    /// Creates a repeating series and materialises its instances.
    ///
    /// The seed block carries the `rrule`; every instance carries the
    /// `series_id`, so "edit the rule" always has one row to read it from.
    pub fn schedule_recurring(&mut self, input: NewBlock, rule_text: &str) -> Result<Vec<BlockRow>> {
        let rule = Rrule::parse(rule_text)?;
        let zone_ = zone(&input.tz)?;
        // Validate the seed the same way any other block is validated, so a
        // recurring block can never be created in a state a single one couldn't.
        let seed = self.schedule_block(input.clone())?;
        let series_id = new_id();
        let start = parse_date(&seed.local_date)?;

        self.conn.execute(
            "UPDATE scheduled_block SET series_id = ?2, rrule = ?3, updated_at = ?4 WHERE id = ?1",
            params![seed.id, series_id, rule_text, self.now()],
        )?;

        let horizon = local_today(self, &zone_)? + Duration::days(HORIZON_DAYS);
        let created = self.materialise(&series_id, &seed, &rule, start, horizon)?;
        let mut out = vec![self.block_row(&seed.id)?];
        out.extend(created);
        Ok(out)
    }

    /// Makes an **existing** block repeat (§4.3).
    ///
    /// The gesture in the Planner is "make this repeat", not "throw this away
    /// and create a repeating one somewhere else": the block already carries a
    /// task, a duration and possibly tracked time, and re-creating it would
    /// orphan all three. So the seed is the block you selected, in place.
    pub fn repeat_block(&mut self, block_id: &str, rule_text: &str) -> Result<Vec<BlockRow>> {
        validate_id(block_id, "block")?;
        let rule = Rrule::parse(rule_text)?;
        let seed = self.block_row(block_id)?;
        if self.series_of(block_id)?.is_some() {
            // Changing a live rule means deciding what happens to instances
            // people may already have tracked against. Refusing is honest;
            // silently rewriting the future would not be.
            return Err(AppError::invalid(
                "This block already repeats. Remove the repeat first, then set a new one.",
            ));
        }

        let zone_ = zone(&seed.tz)?;
        let series_id = new_id();
        self.conn.execute(
            "UPDATE scheduled_block SET series_id = ?2, rrule = ?3, updated_at = ?4 WHERE id = ?1",
            params![seed.id, series_id, rule_text, self.now()],
        )?;

        let start = parse_date(&seed.local_date)?;
        let horizon = local_today(self, &zone_)? + Duration::days(HORIZON_DAYS);
        let seed = self.block_row(block_id)?;
        let created = self.materialise(&series_id, &seed, &rule, start, horizon)?;
        let mut out = vec![seed];
        out.extend(created);
        Ok(out)
    }

    /// Tops a series up to the horizon. Idempotent: an instance that already
    /// exists on a date is left alone, so calling this on every week load is
    /// safe and cheap.
    fn materialise(
        &mut self,
        series_id: &str,
        seed: &BlockRow,
        rule: &Rrule,
        from: NaiveDate,
        horizon: NaiveDate,
    ) -> Result<Vec<BlockRow>> {
        let zone_ = zone(&seed.tz)?;
        // The *local wall clock* of the seed, not its instant: a 09:00 series
        // stays at 09:00 across a DST boundary instead of sliding to 08:00.
        let seed_local = to_local(seed.starts_at, &zone_);
        let time = seed_local.time();

        let existing: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT local_date FROM scheduled_block WHERE series_id = ?1 AND deleted_at IS NULL",
            )?;
            let rows = stmt.query_map([series_id], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };

        let now = self.now();
        let mut created = Vec::new();
        let tx = self.conn.transaction()?;
        for date in rule.dates(from, horizon) {
            let local_date = format_date(date);
            if existing.contains(&local_date) {
                continue;
            }
            let starts_at = crate::time::resolve_local(date.and_time(time), &zone_);
            // A DST-shortened day can leave no room for the instance. Skipping
            // one date is better than writing a block that crosses midnight.
            if same_day_span(starts_at, seed.duration_sec, &zone_).is_err() {
                continue;
            }
            let id = new_id();
            tx.execute(
                "INSERT INTO scheduled_block
                   (id, task_id, label, starts_at, duration_sec, local_date, tz, is_fixed,
                    series_id, intent, device_id, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12)",
                params![
                    id,
                    seed.task_id,
                    seed.label,
                    starts_at,
                    seed.duration_sec,
                    local_date,
                    seed.tz,
                    seed.is_fixed as i64,
                    series_id,
                    // A standing Friday film night is entertainment on every
                    // instance, not just the seed.
                    seed.intent.as_str(),
                    self.device_id,
                    now
                ],
            )?;
            tx.execute(
                "INSERT INTO block_tracked_cache (block_id, tracked_sec, computed_at)
                 VALUES (?1, 0, ?2)",
                params![id, now],
            )?;
            created.push(id);
        }
        tx.commit()?;
        created.iter().map(|id| self.block_row(id)).collect()
    }

    /// Called before a week load: extends any series whose last instance falls
    /// short of the range being viewed.
    pub fn extend_series_to(&mut self, through: &str) -> Result<usize> {
        let target = parse_date(through)?;
        let series: Vec<(String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT series_id, rrule FROM scheduled_block
                  WHERE series_id IS NOT NULL AND rrule IS NOT NULL AND deleted_at IS NULL",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<std::result::Result<_, _>>()?
        };

        let mut added = 0;
        for (series_id, rule_text) in series {
            let Ok(rule) = Rrule::parse(&rule_text) else {
                continue;
            };
            let last: Option<String> = self
                .conn
                .query_row(
                    "SELECT MAX(local_date) FROM scheduled_block
                      WHERE series_id = ?1 AND deleted_at IS NULL",
                    [&series_id],
                    |r| r.get(0),
                )
                .unwrap_or(None);
            let Some(last) = last else { continue };
            if parse_date(&last)? >= target {
                continue;
            }
            let seed = self.series_seed(&series_id)?;
            added += self
                .materialise(&series_id, &seed, &rule, parse_date(&last)?, target)?
                .len();
        }
        Ok(added)
    }

    fn series_seed(&self, series_id: &str) -> Result<BlockRow> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {} FROM scheduled_block
                      WHERE series_id = ?1 AND deleted_at IS NULL
                      ORDER BY starts_at LIMIT 1",
                    Self::BLOCK_COLS
                ),
                [series_id],
                Self::map_block,
            )
            .map_err(|_| AppError::NotFound("series"))
    }

    /// Removing part or all of a series (§4.3 — editing a recurrence is the
    /// operation people get wrong, so the scope is explicit rather than implied).
    pub fn unschedule_series(&mut self, block_id: &str, scope: SeriesScope) -> Result<UndoToken> {
        validate_id(block_id, "block")?;
        let block = self.block_row(block_id)?;
        let Some(series_id) = self.series_of(block_id)? else {
            // Not part of a series: the scope is meaningless, do the single thing.
            return self.unschedule_block(block_id);
        };

        let now = self.now();
        let (sql, label) = match scope {
            SeriesScope::Instance => return self.unschedule_block(block_id),
            SeriesScope::Future => (
                "UPDATE scheduled_block SET deleted_at = ?3, updated_at = ?3
                  WHERE series_id = ?1 AND starts_at >= ?2 AND deleted_at IS NULL",
                "Removed this and later occurrences",
            ),
            SeriesScope::All => (
                "UPDATE scheduled_block SET deleted_at = ?3, updated_at = ?3
                  WHERE series_id = ?1 AND ?2 = ?2 AND deleted_at IS NULL",
                "Removed the whole series",
            ),
        };

        let tx = self.conn.transaction()?;
        tx.execute(sql, params![series_id, block.starts_at, now])?;
        // Sessions keep their record; they only stop being attributed to a block.
        tx.execute(
            "UPDATE time_session SET block_id = NULL, updated_at = ?2
              WHERE block_id IN (SELECT id FROM scheduled_block
                                  WHERE series_id = ?1 AND deleted_at IS NOT NULL)",
            params![series_id, now],
        )?;
        crate::db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;

        Ok(UndoToken {
            entity: "series".into(),
            id: series_id,
            label: label.into(),
            at: now,
        })
    }

    pub(crate) fn series_of(&self, block_id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT series_id FROM scheduled_block WHERE id = ?1",
                [block_id],
                |r| r.get(0),
            )
            .unwrap_or(None))
    }

    /// Restores a whole series, for the undo token above.
    pub fn restore_series(&mut self, series_id: &str) -> Result<()> {
        let now = self.now();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE scheduled_block SET deleted_at = NULL, updated_at = ?2 WHERE series_id = ?1",
            params![series_id, now],
        )?;
        crate::db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        Ok(())
    }

    // ─── .ics import ───────────────────────────────────────────────────

    /// Reads a local calendar file and turns its events into fixed blocks.
    ///
    /// Imported blocks are `is_fixed` because that is what an external meeting
    /// *is*: something you do not get to move (§4.3 — fixed blocks are never
    /// pushed and never auto-shortened). They carry no task; they are the
    /// obligations the rest of the day has to fit around.
    pub fn import_ics(&mut self, path: &std::path::Path, tz: &str) -> Result<IcsImportSummary> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| AppError::Import(format!("Couldn't read that file. {e}")))?;
        self.import_ics_text(&text, tz)
    }

    pub fn import_ics_text(&mut self, text: &str, tz: &str) -> Result<IcsImportSummary> {
        let zone_ = zone(tz)?;
        let parsed = ics::parse(text, &zone_)?;
        let horizon = local_today(self, &zone_)? + Duration::days(HORIZON_DAYS);

        let mut summary = IcsImportSummary {
            imported: 0,
            updated: 0,
            recurring_instances: 0,
            skipped: parsed.skipped_total() as i64,
            note: parsed.summary_line(),
        };

        for event in &parsed.events {
            // Re-importing the same calendar updates in place rather than
            // duplicating: UID is the external identity, and a second import
            // that doubles every meeting is worse than no import.
            let existing: Option<String> = self
                .conn
                .query_row(
                    "SELECT id FROM scheduled_block
                      WHERE external_uid = ?1 AND starts_at = ?2 AND deleted_at IS NULL",
                    params![event.uid, event.starts_at],
                    |r| r.get(0),
                )
                .ok();

            if let Some(id) = existing {
                self.conn.execute(
                    "UPDATE scheduled_block SET label = ?2, duration_sec = ?3, updated_at = ?4
                      WHERE id = ?1",
                    params![id, event.summary, event.duration_sec, self.now()],
                )?;
                summary.updated += 1;
                continue;
            }

            let block = self.schedule_block(NewBlock {
                task_id: None,
                label: Some(event.summary.clone()),
                starts_at: event.starts_at,
                duration_sec: event.duration_sec,
                tz: tz.to_string(),
                is_fixed: true,
                rrule: None,
                // A `.ics` file carries no notion of intent. Calling an
                // imported meeting work is a guess, but it is the guess that
                // matches every calendar anyone imports; the block can be
                // reclassified afterwards.
                intent: None,
            })?;
            self.conn.execute(
                "UPDATE scheduled_block SET external_uid = ?2 WHERE id = ?1",
                params![block.id, event.uid],
            )?;
            summary.imported += 1;

            // A repeating meeting is repeating in Fruit too, using the same
            // engine as a hand-made series.
            if let Some(rule_text) = &event.rrule {
                let Ok(rule) = Rrule::parse(rule_text) else {
                    continue; // an unsupported rule imports as a single event
                };
                let series_id = new_id();
                self.conn.execute(
                    "UPDATE scheduled_block SET series_id = ?2, rrule = ?3 WHERE id = ?1",
                    params![block.id, series_id, rule_text],
                )?;
                let seed = self.block_row(&block.id)?;
                let start = parse_date(&seed.local_date)?;
                summary.recurring_instances +=
                    self.materialise(&series_id, &seed, &rule, start, horizon)?.len() as i64;
            }
        }
        Ok(summary)
    }
}

fn local_today(store: &Store, zone_: &chrono_tz::Tz) -> Result<NaiveDate> {
    parse_date(&local_date(store.now(), zone_))
}

/// Which occurrences an edit applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeriesScope {
    Instance,
    Future,
    All,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IcsImportSummary {
    pub imported: i64,
    pub updated: i64,
    pub recurring_instances: i64,
    pub skipped: i64,
    /// The human sentence, including what was skipped and why.
    pub note: String,
}
