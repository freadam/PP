//! The Reconcile sheet (§3.7) — the loop-closing feature.
//!
//! Reconciling is the only place the plan learns from the record: an overrun
//! becomes a revised estimate, an unplanned session becomes a retroactive
//! block, and a never-started block either moves or is admitted to be dead.

use rusqlite::params;

use super::Store;
use crate::db;
use crate::error::{AppError, Result};
use crate::model::*;
use crate::parser::human_duration;
use crate::time::{day_end, day_start, local_date, parse_date, zone, Millis};

/// §3.7: gaps shorter than this are life, not drift.
const GAP_THRESHOLD_SEC: i64 = 20 * 60;
const ON_ESTIMATE_TOLERANCE_SEC: i64 = 60;
/// A deferred day stays available for 7 days before being auto-accepted (U11).
pub const DEFER_WINDOW_DAYS: i64 = 7;

impl Store {
    pub fn get_reconcile_items(&self, date: &str, tz: &str) -> Result<Vec<ReconcileItem>> {
        let zone = zone(tz)?;
        let day = parse_date(date)?;
        let from = day_start(day, &zone);
        let to = day_end(day, &zone);
        let now = self.now();
        let mut items = Vec::new();

        // ── blocks: overran, or never started ──────────────────────────
        let mut stmt = self.conn.prepare(
            "SELECT b.id, b.task_id, b.starts_at, b.duration_sec,
                    COALESCE(t.title, b.label), COALESCE(c.tracked_sec, 0), t.estimate_sec
               FROM scheduled_block b
               LEFT JOIN task t ON t.id = b.task_id
               LEFT JOIN block_tracked_cache c ON c.block_id = b.id
              WHERE b.local_date = ?1 AND b.deleted_at IS NULL
              ORDER BY b.starts_at",
        )?;
        let rows = stmt.query_map([date], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?,
            ))
        })?;

        let tomorrow = crate::time::format_date(day.succ_opt().unwrap());
        for row in rows {
            let (block_id, task_id, starts_at, duration_sec, title, tracked_sec, _estimate) = row?;
            let planned = duration_sec;
            let drift = tracked_sec - planned;

            if tracked_sec == 0 {
                let slot = self.next_free_slot(&tomorrow, planned, None, tz)?;
                items.push(ReconcileItem {
                    id: format!("block:{block_id}"),
                    kind: ReconcileKind::NeverStarted,
                    title,
                    block_id: Some(block_id),
                    task_id,
                    session_id: None,
                    planned_sec: planned,
                    tracked_sec: 0,
                    drift_sec: -planned,
                    starts_at: Some(starts_at),
                    ends_at: Some(starts_at + planned * 1000),
                    default_action: ReconcileVerb::MoveToTomorrow,
                    available: vec![
                        ReconcileVerb::MoveToTomorrow,
                        ReconcileVerb::Drop,
                        ReconcileVerb::MarkDone,
                        ReconcileVerb::LeaveUnscheduled,
                    ],
                    suggested_slot: slot,
                    suggested_duration_sec: planned,
                });
            } else if drift > ON_ESTIMATE_TOLERANCE_SEC {
                let remainder = drift;
                let slot = self.next_free_slot(&tomorrow, remainder.max(300), None, tz)?;
                items.push(ReconcileItem {
                    id: format!("block:{block_id}"),
                    kind: ReconcileKind::Overran,
                    title,
                    block_id: Some(block_id),
                    task_id,
                    session_id: None,
                    planned_sec: planned,
                    tracked_sec,
                    drift_sec: drift,
                    starts_at: Some(starts_at),
                    ends_at: Some(starts_at + planned * 1000),
                    // Accepting records the overrun — the honest default.
                    default_action: ReconcileVerb::Accept,
                    available: vec![
                        ReconcileVerb::Accept,
                        ReconcileVerb::RescheduleRemainder,
                        ReconcileVerb::Split,
                        ReconcileVerb::ReviseEstimate,
                    ],
                    suggested_slot: slot,
                    suggested_duration_sec: remainder.max(300),
                });
            }
        }
        drop(stmt);

        // ── sessions with no block: unplanned work ─────────────────────
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.task_id, t.title, s.started_at, s.ended_at, s.elapsed_sec
               FROM time_session s JOIN task t ON t.id = s.task_id
              WHERE s.block_id IS NULL AND s.started_at >= ?1 AND s.started_at < ?2
                AND s.elapsed_sec > 0
              ORDER BY s.started_at",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        for row in rows {
            let (session_id, task_id, title, started_at, ended_at, elapsed) = row?;
            items.push(ReconcileItem {
                id: format!("session:{session_id}"),
                kind: ReconcileKind::UnplannedSession,
                title,
                block_id: None,
                task_id: Some(task_id),
                session_id: Some(session_id),
                planned_sec: 0,
                tracked_sec: elapsed,
                drift_sec: elapsed,
                starts_at: Some(started_at),
                ends_at: ended_at,
                default_action: ReconcileVerb::Accept,
                // Creating the retroactive block is how the plan learns (§3.7).
                available: vec![ReconcileVerb::Accept, ReconcileVerb::CreateRetroBlock],
                suggested_slot: Some(started_at),
                suggested_duration_sec: elapsed.max(300),
            });
        }
        drop(stmt);

        // ── untracked gaps inside planned hours ────────────────────────
        for (gap_from, gap_to) in self.untracked_gaps(date, tz)? {
            let seconds = (gap_to - gap_from) / 1000;
            items.push(ReconcileItem {
                id: format!("gap:{gap_from}"),
                kind: ReconcileKind::UntrackedGap,
                title: format!(
                    "Untracked {} between {} and {}",
                    human_duration(seconds),
                    clock(gap_from, tz),
                    clock(gap_to, tz)
                ),
                block_id: None,
                task_id: None,
                session_id: None,
                planned_sec: 0,
                tracked_sec: 0,
                drift_sec: 0,
                starts_at: Some(gap_from),
                ends_at: Some(gap_to),
                default_action: ReconcileVerb::Ignore,
                available: vec![
                    ReconcileVerb::Ignore,
                    ReconcileVerb::AssignToTask,
                    ReconcileVerb::LogAsBreak,
                ],
                suggested_slot: Some(gap_from),
                suggested_duration_sec: seconds,
            });
        }

        let _ = now;
        Ok(items)
    }

    /// Gaps longer than 20 minutes between the day's first planned start and
    /// its last planned end that no session covers.
    fn untracked_gaps(&self, date: &str, tz: &str) -> Result<Vec<(Millis, Millis)>> {
        let blocks = self.blocks_on(date)?;
        if blocks.is_empty() {
            return Ok(Vec::new());
        }
        let planned_from = blocks.iter().map(|b| b.starts_at).min().unwrap();
        let planned_to = blocks
            .iter()
            .map(|b| b.starts_at + b.duration_sec * 1000)
            .max()
            .unwrap();

        let zone = zone(tz)?;
        let day = parse_date(date)?;
        let mut stmt = self.conn.prepare(
            "SELECT started_at, COALESCE(ended_at, started_at + elapsed_sec * 1000)
               FROM time_session
              WHERE started_at < ?2 AND COALESCE(ended_at, started_at) >= ?1
              ORDER BY started_at",
        )?;
        let rows = stmt.query_map(params![day_start(day, &zone), day_end(day, &zone)], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut covered: Vec<(i64, i64)> = rows.collect::<std::result::Result<_, _>>()?;
        covered.sort();

        let mut gaps = Vec::new();
        let mut cursor = planned_from;
        for (s, e) in covered {
            if s > cursor && (s - cursor) / 1000 > GAP_THRESHOLD_SEC {
                gaps.push((cursor, s.min(planned_to)));
            }
            cursor = cursor.max(e);
        }
        if planned_to > cursor && (planned_to - cursor) / 1000 > GAP_THRESHOLD_SEC {
            gaps.push((cursor, planned_to));
        }
        Ok(gaps)
    }

    /// Applies the day's decisions and writes exactly one `day_review` row (F5).
    pub fn apply_reconcile(
        &mut self,
        date: &str,
        actions: Vec<ReconcileAction>,
        tz: &str,
    ) -> Result<DayReview> {
        parse_date(date)?;
        let zone_ = zone(tz)?;
        let now = self.now();

        for action in &actions {
            let (kind, raw_id) = action
                .item_id
                .split_once(':')
                .ok_or_else(|| AppError::invalid("Malformed reconcile item id."))?;
            match action.verb {
                ReconcileVerb::Accept | ReconcileVerb::Ignore | ReconcileVerb::LeaveUnscheduled => {}

                ReconcileVerb::Drop => {
                    if kind == "block" {
                        self.unschedule_block(raw_id)?;
                    }
                }

                ReconcileVerb::MarkDone => {
                    if let Some(task_id) = &action.task_id {
                        self.set_task_status(task_id, Status::Done)?;
                    } else if kind == "block" {
                        let block = self.block_row(raw_id)?;
                        if let Some(task_id) = block.task_id {
                            self.set_task_status(&task_id, Status::Done)?;
                        }
                    }
                }

                // Move the whole block to tomorrow's first free slot.
                ReconcileVerb::MoveToTomorrow => {
                    if kind != "block" {
                        continue;
                    }
                    let block = self.block_row(raw_id)?;
                    let tomorrow = crate::time::format_date(
                        parse_date(date)?.succ_opt().unwrap(),
                    );
                    let slot = match action.starts_at {
                        Some(at) => at,
                        None => self
                            .next_free_slot(&tomorrow, block.duration_sec, None, tz)?
                            .ok_or_else(|| {
                                AppError::invalid(
                                    "Tomorrow has no free slot that long. Pick a time by hand.",
                                )
                            })?,
                    };
                    self.move_block(raw_id, slot, CollisionPolicy::Overlap)?;
                }

                // The overrun becomes a new block for the remaining time.
                ReconcileVerb::RescheduleRemainder => {
                    if kind != "block" {
                        continue;
                    }
                    let block = self.block_row(raw_id)?;
                    let tracked: i64 = self
                        .conn
                        .query_row(
                            "SELECT COALESCE(tracked_sec, 0) FROM block_tracked_cache WHERE block_id = ?1",
                            [raw_id],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let remainder = action
                        .duration_sec
                        .unwrap_or_else(|| (tracked - block.duration_sec).max(300))
                        .clamp(300, 43_200);
                    let tomorrow =
                        crate::time::format_date(parse_date(date)?.succ_opt().unwrap());
                    let slot = match action.starts_at {
                        Some(at) => at,
                        None => self
                            .next_free_slot(&tomorrow, remainder, None, tz)?
                            .ok_or_else(|| {
                                AppError::invalid(
                                    "No free slot that long tomorrow. Pick a time by hand.",
                                )
                            })?,
                    };
                    self.schedule_block(NewBlock {
                        task_id: block.task_id.clone(),
                        label: block.label.clone(),
                        starts_at: slot,
                        duration_sec: remainder,
                        tz: tz.to_string(),
                        is_fixed: false,
                        rrule: None,
                    })?;
                }

                // Split: the original shrinks to what was planned, the overrun
                // gets its own block right after the tracked time ended.
                ReconcileVerb::Split => {
                    if kind != "block" {
                        continue;
                    }
                    let block = self.block_row(raw_id)?;
                    let tracked: i64 = self
                        .conn
                        .query_row(
                            "SELECT COALESCE(tracked_sec, 0) FROM block_tracked_cache WHERE block_id = ?1",
                            [raw_id],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let overrun = (tracked - block.duration_sec).max(300);
                    let after = block.starts_at + block.duration_sec * 1000;
                    let same_day = crate::time::same_day_span(after, overrun, &zone_).is_ok();
                    let starts_at = if same_day {
                        after
                    } else {
                        let tomorrow =
                            crate::time::format_date(parse_date(date)?.succ_opt().unwrap());
                        self.next_free_slot(&tomorrow, overrun, None, tz)?
                            .ok_or_else(|| {
                                AppError::invalid("No room to split into. Pick a time by hand.")
                            })?
                    };
                    self.schedule_block(NewBlock {
                        task_id: block.task_id.clone(),
                        label: block.label.clone(),
                        starts_at,
                        duration_sec: overrun.min(43_200),
                        tz: tz.to_string(),
                        is_fixed: false,
                        rrule: None,
                    })?;
                }

                ReconcileVerb::ReviseEstimate => {
                    let task_id = match (&action.task_id, kind) {
                        (Some(t), _) => Some(t.clone()),
                        (None, "block") => self.block_row(raw_id)?.task_id,
                        _ => None,
                    };
                    if let (Some(task_id), Some(estimate)) = (task_id, action.estimate_sec) {
                        self.update_task(
                            &task_id,
                            TaskPatch {
                                estimate_sec: Some(Some(estimate)),
                                ..Default::default()
                            },
                        )?;
                    }
                }

                // This is how the plan learns: the record writes a block back
                // into the plot, and the session is attributed to it (F3).
                ReconcileVerb::CreateRetroBlock => {
                    if kind != "session" {
                        continue;
                    }
                    let session = self.session_row(raw_id)?;
                    let duration = action
                        .duration_sec
                        .unwrap_or(session.elapsed_sec)
                        .clamp(300, 43_200);
                    let starts_at = action.starts_at.unwrap_or(session.started_at);
                    let block = self.schedule_block(NewBlock {
                        task_id: Some(session.task_id.clone()),
                        label: None,
                        starts_at,
                        duration_sec: duration,
                        tz: tz.to_string(),
                        is_fixed: false,
                        rrule: None,
                    })?;
                    let tx = self.conn.transaction()?;
                    tx.execute(
                        "UPDATE time_session SET block_id = ?2, updated_at = ?3 WHERE id = ?1",
                        params![session.id, block.id, now],
                    )?;
                    db::rebuild_tracked_caches(&tx)?;
                    tx.commit()?;
                }

                ReconcileVerb::AssignToTask => {
                    if kind != "gap" {
                        continue;
                    }
                    let (Some(task_id), Some(start), Some(dur)) =
                        (&action.task_id, action.starts_at, action.duration_sec)
                    else {
                        continue;
                    };
                    self.add_session(ManualSession {
                        task_id: task_id.clone(),
                        block_id: None,
                        started_at: start,
                        ended_at: start + dur * 1000,
                        note: Some("Assigned during reconcile".into()),
                    })?;
                }

                ReconcileVerb::LogAsBreak => { /* nothing to record — a break is the absence of work */ }
            }
        }

        self.write_day_review(date, tz, &zone_)
    }

    fn write_day_review(
        &mut self,
        date: &str,
        tz: &str,
        zone_: &chrono_tz::Tz,
    ) -> Result<DayReview> {
        let day = parse_date(date)?;
        let (from, to) = (day_start(day, zone_), day_end(day, zone_));

        let (planned_sec, blocks_total): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(duration_sec), 0), COUNT(*)
               FROM scheduled_block WHERE local_date = ?1 AND deleted_at IS NULL",
            [date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let blocks_untouched: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM scheduled_block b
               LEFT JOIN block_tracked_cache c ON c.block_id = b.id
              WHERE b.local_date = ?1 AND b.deleted_at IS NULL
                AND COALESCE(c.tracked_sec, 0) = 0",
            [date],
            |r| r.get(0),
        )?;
        let tracked_sec: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(elapsed_sec), 0) FROM time_session
              WHERE started_at >= ?1 AND started_at < ?2",
            params![from, to],
            |r| r.get(0),
        )?;
        let unplanned_sec: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(elapsed_sec), 0) FROM time_session
              WHERE block_id IS NULL AND started_at >= ?1 AND started_at < ?2",
            params![from, to],
            |r| r.get(0),
        )?;
        let overrun_sec: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(MAX(COALESCE(c.tracked_sec,0) - b.duration_sec, 0)), 0)
               FROM scheduled_block b
               LEFT JOIN block_tracked_cache c ON c.block_id = b.id
              WHERE b.local_date = ?1 AND b.deleted_at IS NULL",
            [date],
            |r| r.get(0),
        )?;

        let calibration_ratio = (planned_sec > 0)
            .then(|| (tracked_sec - unplanned_sec) as f64 / planned_sec as f64);
        let now = self.now();

        // Exactly one row per reconciled local date (F5).
        self.conn.execute(
            "INSERT INTO day_review
               (local_date, reconciled_at, planned_sec, tracked_sec, overrun_sec, unplanned_sec,
                blocks_total, blocks_untouched, calibration_ratio)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(local_date) DO UPDATE SET
               reconciled_at = excluded.reconciled_at,
               planned_sec = excluded.planned_sec,
               tracked_sec = excluded.tracked_sec,
               overrun_sec = excluded.overrun_sec,
               unplanned_sec = excluded.unplanned_sec,
               blocks_total = excluded.blocks_total,
               blocks_untouched = excluded.blocks_untouched,
               calibration_ratio = excluded.calibration_ratio",
            params![
                date,
                now,
                planned_sec,
                tracked_sec,
                overrun_sec,
                unplanned_sec,
                blocks_total,
                blocks_untouched,
                calibration_ratio
            ],
        )?;
        // Computed after the review row is written, so "today" counts.
        let streak_days = self.reconcile_streak(tz)?;

        Ok(DayReview {
            takeaway: takeaway(planned_sec, tracked_sec, unplanned_sec, blocks_untouched),
            streak_days,
            local_date: date.to_string(),
            reconciled_at: now,
            planned_sec,
            tracked_sec,
            overrun_sec,
            unplanned_sec,
            blocks_total,
            blocks_untouched,
            calibration_ratio,
        })
    }

    /// Days with plotted blocks that have not been reconciled yet, oldest
    /// first. Drives the top-bar dot and the tray badge (§3.11).
    pub fn unreconciled_days(&self, before: &str, limit: u32) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT b.local_date
               FROM scheduled_block b
              WHERE b.deleted_at IS NULL AND b.local_date < ?1
                AND NOT EXISTS (SELECT 1 FROM day_review d WHERE d.local_date = b.local_date)
              ORDER BY b.local_date DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![before, limit], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// U11: a deferred day auto-accepts after 7 days rather than nagging forever.
    pub fn auto_accept_stale_days(&mut self, tz: &str) -> Result<Vec<String>> {
        let zone_ = zone(tz)?;
        let today = local_date(self.now(), &zone_);
        let cutoff = crate::time::format_date(
            parse_date(&today)? - chrono::Duration::days(DEFER_WINDOW_DAYS),
        );
        let stale = self.unreconciled_days(&cutoff, 60)?;
        for date in &stale {
            self.write_day_review(date, tz, &zone_)?;
        }
        Ok(stale)
    }

    /// Consecutive reconciled days ending today or yesterday (§2.3 streak).
    pub fn reconcile_streak(&self, tz: &str) -> Result<i64> {
        let zone_ = zone(tz)?;
        let today = parse_date(&local_date(self.now(), &zone_))?;
        let mut streak = 0;
        let mut cursor = today;
        for step in 0..365 {
            let date = crate::time::format_date(cursor);
            let exists: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM day_review WHERE local_date = ?1",
                [&date],
                |r| r.get(0),
            )?;
            if exists == 1 {
                streak += 1;
            } else if step > 0 || streak > 0 {
                break;
            }
            cursor = cursor.pred_opt().unwrap();
        }
        Ok(streak)
    }
}

fn clock(at: Millis, tz: &str) -> String {
    match zone(tz) {
        Ok(z) => {
            use chrono::Timelike;
            let l = crate::time::to_local(at, &z);
            format!("{:02}:{:02}", l.hour(), l.minute())
        }
        Err(_) => "??:??".into(),
    }
}

/// One takeaway line, in plain language (§3.7).
fn takeaway(planned: i64, tracked: i64, unplanned: i64, untouched: i64) -> String {
    if planned == 0 && tracked == 0 {
        return "Nothing plotted, nothing tracked. A clean slate.".into();
    }
    if planned == 0 {
        return format!(
            "{} tracked with nothing plotted. Worth plotting tomorrow?",
            human_duration(tracked)
        );
    }
    let ratio = tracked as f64 / planned as f64;
    let mut line = format!(
        "Plotted {}, tracked {}",
        human_duration(planned),
        human_duration(tracked)
    );
    if ratio >= 1.1 {
        line.push_str(&format!(" — {:.0}% over plan.", (ratio - 1.0) * 100.0));
    } else if ratio <= 0.9 {
        line.push_str(&format!(" — {:.0}% under plan.", (1.0 - ratio) * 100.0));
    } else {
        line.push_str(" — close to plan.");
    }
    if untouched > 0 {
        line.push_str(&format!(
            " {untouched} block{} never started.",
            if untouched == 1 { "" } else { "s" }
        ));
    }
    if unplanned > 0 {
        line.push_str(&format!(" {} was unplanned.", human_duration(unplanned)));
    }
    line
}
