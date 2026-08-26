//! The Today workspace — the default launch route (ARCHITECTURE §3.0, §7.5).
//!
//! This is a **composition read model**, not a new source of truth. Everything
//! on the launch screen already exists somewhere: the week's allocation comes
//! from the Day Ledger projection, the task column from daily plan items plus
//! tasks, the schedule from scheduled blocks, and the running session from the
//! timer. Nothing here recomputes any of them.
//!
//! The one thing it *does* compute is the weekday tracking-memory cue (§7.6),
//! and it computes it from distinct task ids rather than from sessions or
//! durations: multiple sessions on one task still make one tracked task
//! (invariant 27), and machine observation never increments the count
//! (invariant 28). The cue exists to remind someone to start a timer, so a day
//! full of observed activity and no session has to keep looking untracked.

use std::collections::{HashMap, HashSet};

use chrono::Duration;
use rusqlite::params;

use super::Store;
use crate::error::{AppError, Result};
use crate::ids::new_id;
use crate::model::*;
use crate::time::{
    date_series, day_end, day_start, format_date, local_date, minutes_into_day, parse_date,
    week_start, zone,
};

/// Where the timeboxing canvas opens when the selected day is not today.
/// Deliberately a *setting* with a default rather than a constant: 07:00 is the
/// wireframe's first row, and a night worker's day does not start there.
const DAY_START_HOUR_KEY: &str = "today.dayStartHour";
const DEFAULT_DAY_START_HOUR: i64 = 7;

/// Invariant 24 — a local date may hold at most three items in `top`.
pub const TOP_LIMIT: usize = 3;

/// Rank step for appending. Fractional ranks let a reorder rewrite one row
/// instead of renumbering the list, which is what keeps a drag cheap.
const RANK_STEP: f64 = 1024.0;

impl Store {
    fn day_start_hour(&self) -> i64 {
        self.get_setting(DAY_START_HOUR_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.as_i64())
            .filter(|h| (0..24).contains(h))
            .unwrap_or(DEFAULT_DAY_START_HOUR)
    }

    /// The launch screen for one selected local date (§7.5).
    pub fn today_workspace(&self, date: &str, tz: &str) -> Result<TodayWorkspace> {
        let zone = zone(tz)?;
        let now = self.now();
        let today = local_date(now, &zone);
        let selected = parse_date(date)?;
        let tomorrow = format_date(selected + Duration::days(1));

        // §7.6 — the strip is the week containing the *selected* date, not the
        // week containing today. Navigating to last Thursday has to show last
        // Thursday's week, or the selection lands in a strip that cannot show it.
        let monday = week_start(selected);
        let sunday = monday + Duration::days(6);
        let dates = date_series(&format_date(monday), &format_date(sunday))?;

        let week_days = self.week_day_cells(&dates, date, &today, tz)?;
        let week_allocation = self.week_allocation(&format_date(monday), &format_date(sunday), tz)?;

        let mut selected_tasks = self.planned_tasks(date, tz)?;
        let completed_today = selected_tasks
            .iter()
            .filter(|t| t.task.status == Status::Done)
            .cloned()
            .collect::<Vec<_>>();
        selected_tasks.retain(|t| t.task.status != Status::Done);
        let (top_three, more_today): (Vec<_>, Vec<_>) = selected_tasks
            .into_iter()
            .partition(|t| t.section == DaySection::Top);

        // The Tomorrow preview is read-only and short: it is orientation, not a
        // second list to maintain (§7.4). Completed tasks are dropped from it
        // for the same reason — tomorrow has no completions worth previewing.
        let mut tomorrow_preview = self.planned_tasks(&tomorrow, tz)?;
        tomorrow_preview.retain(|t| t.task.status != Status::Done);

        Ok(TodayWorkspace {
            selected_date: date.to_string(),
            schedule: self.day_schedule(date, &today, tz)?,
            today,
            tz: tz.to_string(),
            now,
            week_days,
            week_allocation,
            top_three,
            more_today,
            completed_today,
            tomorrow_preview,
            tomorrow_date: tomorrow,
            timer: self.timer_state()?,
        })
    }

    /* ─── the weekday strip (§7.6) ──────────────────────────────────── */

    fn week_day_cells(
        &self,
        dates: &[String],
        selected: &str,
        today: &str,
        tz: &str,
    ) -> Result<Vec<WeekDayCell>> {
        let zone = zone(tz)?;
        let placeholders = vec!["?"; dates.len()].join(",");

        // Planned: a task is present on a date through a daily plan item, an
        // intended date, or a scheduled block — the three ways §7.6 says a task
        // can be "on" a day. UNION, not OR across joins, so a task that is
        // present twice over still counts once.
        let mut planned: HashMap<String, HashSet<String>> = HashMap::new();
        {
            let sql = format!(
                "SELECT local_date, task_id FROM (
                   SELECT p.local_date AS local_date, p.task_id AS task_id
                     FROM daily_plan_item p JOIN task t ON t.id = p.task_id
                    WHERE p.local_date IN ({placeholders}) AND t.deleted_at IS NULL
                      AND t.status IN ('open','done')
                   UNION
                   SELECT t.due_date, t.id FROM task t
                    WHERE t.due_date IN ({placeholders}) AND t.deleted_at IS NULL
                      AND t.status IN ('open','done')
                   UNION
                   SELECT b.local_date, b.task_id FROM scheduled_block b
                     JOIN task t ON t.id = b.task_id
                    WHERE b.local_date IN ({placeholders}) AND b.deleted_at IS NULL
                      AND t.deleted_at IS NULL AND t.status IN ('open','done')
                 )"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let args: Vec<&String> = dates.iter().chain(dates).chain(dates).collect();
            let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (date, task) = row?;
                planned.entry(date).or_default().insert(task);
            }
        }

        // Tracked: distinct tasks with a positive-duration session overlapping
        // the local date. Grouped in Rust by the session's own local date so a
        // session that starts before midnight lands where it started, exactly
        // as `unplanned_by_date` already does.
        let mut tracked: HashMap<String, HashSet<String>> = HashMap::new();
        let mut completed: HashMap<String, i64> = HashMap::new();
        {
            let first = parse_date(&dates[0])?;
            let last = parse_date(dates.last().unwrap())?;
            let from = day_start(first, &zone);
            let to = day_end(last, &zone);

            let mut stmt = self.conn.prepare(
                "SELECT s.started_at, s.task_id FROM time_session s
                   JOIN task t ON t.id = s.task_id
                  WHERE s.started_at >= ?1 AND s.started_at < ?2
                    AND s.elapsed_sec > 0 AND t.deleted_at IS NULL",
            )?;
            let rows = stmt.query_map(params![from, to], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (started_at, task) = row?;
                tracked
                    .entry(local_date(started_at, &zone))
                    .or_default()
                    .insert(task);
            }

            let mut stmt = self.conn.prepare(
                "SELECT completed_at FROM task
                  WHERE status = 'done' AND deleted_at IS NULL
                    AND completed_at >= ?1 AND completed_at < ?2",
            )?;
            let rows = stmt.query_map(params![from, to], |r| r.get::<_, i64>(0))?;
            for row in rows {
                *completed.entry(local_date(row?, &zone)).or_insert(0) += 1;
            }
        }

        let reconciled: HashSet<String> = self.reconciled_dates(dates)?.into_iter().collect();
        let now = self.now();

        let mut out = Vec::with_capacity(dates.len());
        for date in dates {
            let parsed = parse_date(date)?;
            let planned_set = planned.get(date).cloned().unwrap_or_default();
            let tracked_set = tracked.get(date).cloned().unwrap_or_default();
            out.push(WeekDayCell {
                weekday: parsed.format("%a").to_string(),
                day_of_month: parsed.format("%-d").to_string().parse().unwrap_or(0),
                is_today: date == today,
                is_selected: date == selected,
                has_happened: day_end(parsed, &zone) <= now,
                planned_task_count: planned_set.len() as i64,
                tracked_task_count: tracked_set.len() as i64,
                completed_task_count: completed.get(date).copied().unwrap_or(0),
                untracked_planned_count: planned_set.difference(&tracked_set).count() as i64,
                is_reconciled: reconciled.contains(date),
                local_date: date.clone(),
            });
        }
        Ok(out)
    }

    /* ─── the week allocation (§7.5, invariant 18) ──────────────────── */

    /// The week's six mutually exclusive accounting states.
    ///
    /// Counted off `resolve_day`'s own tiling segments rather than from a
    /// second aggregate query, so the donut and the Day view cannot disagree
    /// and the six states sum to the exact local week — including the 167- and
    /// 169-hour weeks a daylight-saving transition produces.
    pub fn week_allocation(&self, from: &str, to: &str, tz: &str) -> Result<WeekAllocation> {
        let zone = zone(tz)?;
        let now = self.now();
        let dates = date_series(from, to)?;

        // Sessions still awaiting an explicit decision — a recovered timer, or
        // one edited back to pending. Tracked time is a deliberate claim, which
        // is stronger than passive observation and still short of verified, so
        // it gets its own state rather than being folded into either.
        let pending: HashSet<String> = {
            let first = parse_date(&dates[0])?;
            let last = parse_date(dates.last().unwrap())?;
            let mut stmt = self.conn.prepare(
                "SELECT id FROM time_session
                  WHERE is_confirmed = 0 AND started_at >= ?1 AND started_at < ?2",
            )?;
            let rows = stmt.query_map(
                params![day_start(first, &zone), day_end(last, &zone)],
                |r| r.get::<_, String>(0),
            )?;
            rows.collect::<std::result::Result<HashSet<_>, _>>()?
        };

        let mut a = WeekAllocation {
            from: from.to_string(),
            to: to.to_string(),
            period_sec: 0,
            verified_work_sec: 0,
            verified_life_sec: 0,
            sleep_rest_sec: 0,
            private_sec: 0,
            tracked_pending_sec: 0,
            observed_unconfirmed_sec: 0,
            unaccounted_sec: 0,
            elapsed_sec: 0,
            accounted_ratio: 0.0,
            reviewed_days: 0,
            elapsed_days: 0,
        };

        for date in &dates {
            let day = self.get_day(date, tz, None)?;
            a.period_sec += day.totals.day_sec;

            let parsed = parse_date(date)?;
            let ends_at = day_end(parsed, &zone);
            let elapsed = (now.min(ends_at) - day_start(parsed, &zone)).max(0) / 1000;
            a.elapsed_sec += elapsed;
            if ends_at <= now {
                a.elapsed_days += 1;
                if day.is_reconciled {
                    a.reviewed_days += 1;
                }
            }

            for seg in &day.segments {
                let sec = (seg.to - seg.from) / 1000;
                match &seg.owner {
                    // Private is checked before area kind: an entry marked
                    // private in a Rest area is private time, not sleep the
                    // report may name.
                    SlotOwner::Life { is_private: true, .. } => a.private_sec += sec,
                    SlotOwner::Life { area_kind: AreaKind::Rest, .. } => a.sleep_rest_sec += sec,
                    SlotOwner::Life { .. } => a.verified_life_sec += sec,
                    SlotOwner::Work { session_id, .. } => {
                        if pending.contains(session_id) {
                            a.tracked_pending_sec += sec;
                        } else {
                            a.verified_work_sec += sec;
                        }
                    }
                    SlotOwner::Observed { .. } => a.observed_unconfirmed_sec += sec,
                    // Idle is the machine saying nobody was here, which is not
                    // an account of the time — it is the absence of one.
                    SlotOwner::Idle | SlotOwner::Empty => a.unaccounted_sec += sec,
                }
            }
        }

        let confirmed = a.verified_work_sec + a.verified_life_sec + a.sleep_rest_sec + a.private_sec;
        a.accounted_ratio = if a.elapsed_sec > 0 {
            (confirmed as f64 / a.elapsed_sec as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Ok(a)
    }

    /* ─── the selected day's task column (§7.5) ─────────────────────── */

    /// Daily plan items, plus tasks whose intended date or scheduled block
    /// lands on the date without one.
    ///
    /// §4.15: a task present only through its date or a block appears in **More
    /// for today** in a deterministic default order, and promoting it is what
    /// creates its plan item. That is why `plan_item_id` is optional here — the
    /// row exists before the record does.
    fn planned_tasks(&self, date: &str, tz: &str) -> Result<Vec<PlannedTask>> {
        let zone = zone(tz)?;
        let parsed = parse_date(date)?;
        let (from, to) = (day_start(parsed, &zone), day_end(parsed, &zone));
        let now = self.now();

        let sql = format!(
            "SELECT {cols},
                    p.id, p.section, p.rank,
                    pr.name, pr.colour,
                    COALESCE((SELECT SUM(s.elapsed_sec) FROM time_session s
                               WHERE s.task_id = t.id
                                 AND s.started_at >= ?2 AND s.started_at < ?3), 0),
                    (SELECT MIN(b.starts_at) FROM scheduled_block b
                      WHERE b.task_id = t.id AND b.local_date = ?4 AND b.deleted_at IS NULL),
                    COALESCE((SELECT SUM(b.duration_sec) FROM scheduled_block b
                               WHERE b.task_id = t.id AND b.local_date = ?4
                                 AND b.deleted_at IS NULL), 0),
                    (SELECT COUNT(*) FROM scheduled_block b
                      WHERE b.task_id = t.id AND b.local_date = ?4 AND b.deleted_at IS NULL),
                    EXISTS(SELECT 1 FROM time_session s
                            WHERE s.task_id = t.id AND s.ended_at IS NULL)
               FROM task t
               LEFT JOIN daily_plan_item p ON p.task_id = t.id AND p.local_date = ?4
               LEFT JOIN project pr ON pr.id = t.project_id
              WHERE t.deleted_at IS NULL
                AND t.status IN ('open','done')
                AND t.parent_id IS NULL
                AND (
                     p.id IS NOT NULL
                  OR t.due_date = ?4
                  OR EXISTS(SELECT 1 FROM scheduled_block b
                             WHERE b.task_id = t.id AND b.local_date = ?4
                               AND b.deleted_at IS NULL)
                )",
            cols = super::tasks::task_cols(1),
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![now, from, to, date], |r| {
            let task = Self::map_task(r)?;
            let plan_item_id: Option<String> = r.get(22)?;
            let section: Option<String> = r.get(23)?;
            let rank: Option<f64> = r.get(24)?;
            let block_count: i64 = r.get(30)?;
            // A plan item is a choice, an intended date is a plan, and a block
            // is a booking. The row reads differently for each, so the source
            // travels with it rather than being guessed at in the renderer.
            let source = if plan_item_id.is_some() {
                PlanSource::Planned
            } else if task.due_date.is_some() {
                PlanSource::IntendedDate
            } else if block_count > 0 {
                PlanSource::Scheduled
            } else {
                PlanSource::IntendedDate
            };
            Ok(PlannedTask {
                section: section
                    .as_deref()
                    .and_then(DaySection::parse)
                    .unwrap_or(DaySection::More),
                source,
                // No plan item means no explicit rank. Sorting these last, and
                // then by the fallbacks below, is what makes "More for today"
                // an order rather than a heap.
                rank: rank.unwrap_or(f64::MAX),
                plan_item_id,
                project_name: r.get(25)?,
                project_colour: r.get(26)?,
                tracked_today_sec: r.get(27)?,
                scheduled_starts_at: r.get(28)?,
                scheduled_sec: r.get(29)?,
                scheduled_block_count: block_count,
                is_running: r.get::<_, i64>(31)? == 1,
                task,
            })
        })?;

        let mut out = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        // Deterministic default order for rows with no plan item (§4.15):
        // ranked items first in their rank, then the day's scheduled work in
        // clock order, then everything else by priority and title. "Whatever
        // SQLite returned" is not an order a user can learn.
        out.sort_by(|a, b| {
            a.rank
                .partial_cmp(&b.rank)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| match (a.scheduled_starts_at, b.scheduled_starts_at) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
                .then_with(|| b.task.priority.cmp(&a.task.priority))
                .then_with(|| a.task.title.to_lowercase().cmp(&b.task.title.to_lowercase()))
        });

        let mut tasks: Vec<TaskRow> = out.iter().map(|p| p.task.clone()).collect();
        self.attach_tags(&mut tasks)?;
        for (row, task) in out.iter_mut().zip(tasks) {
            row.task = task;
        }
        Ok(out)
    }

    /* ─── the selected day's schedule (§7.5) ────────────────────────── */

    fn day_schedule(&self, date: &str, today: &str, tz: &str) -> Result<DaySchedule> {
        let zone = zone(tz)?;
        let week = self.get_week(
            &DateRange {
                from: date.to_string(),
                to: date.to_string(),
            },
            tz,
        )?;
        let column = week.days.into_iter().next();
        let is_today = date == today;
        let current_minute = is_today.then(|| minutes_into_day(self.now(), &zone));
        let configured = self.day_start_hour() * 60;

        Ok(DaySchedule {
            local_date: date.to_string(),
            // Open near the current hour on today, at the configured day start
            // otherwise — and never *past* the day start, so an early morning
            // does not open the canvas below the first thing on it (§3.0).
            visible_day_start_minute: match current_minute {
                Some(m) => configured.min((m - 60).max(0)),
                None => configured,
            },
            planned_sec: column.as_ref().map(|c| c.planned_sec).unwrap_or(0),
            unplanned_sec: column.as_ref().map(|c| c.unplanned_sec).unwrap_or(0),
            blocks: column.map(|c| c.blocks).unwrap_or_default(),
            current_minute,
        })
    }

    /* ─── commands (§8.2) ───────────────────────────────────────────── */

    /// `AddTaskToDay` — place a task in one local day's list.
    ///
    /// Idempotent on the (task, date) pair: calling it again moves the existing
    /// item's section rather than creating a second one, because invariant 23
    /// says a task appears at most once per date and a command that can violate
    /// it by being retried is a command that will.
    pub fn add_task_to_day(
        &mut self,
        task_id: &str,
        date: &str,
        section: DaySection,
    ) -> Result<()> {
        parse_date(date)?;
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM task WHERE id = ?1 AND deleted_at IS NULL",
            params![task_id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(AppError::NotFound("task"));
        }
        if section == DaySection::Top {
            self.assert_top_has_room(task_id, date)?;
        }

        let now = self.now();
        let rank: f64 = self.conn.query_row(
            "SELECT COALESCE(MAX(rank), 0) + ?3 FROM daily_plan_item
              WHERE local_date = ?1 AND section = ?2",
            params![date, section.as_str(), RANK_STEP],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO daily_plan_item (id, task_id, local_date, section, rank, created_at, updated_at)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(task_id, local_date) DO UPDATE
                    SET section = excluded.section, rank = excluded.rank, updated_at = excluded.updated_at",
            params![new_id(), task_id, date, section.as_str(), rank, now],
        )?;
        Ok(())
    }

    /// `RemoveTaskFromDay`. Invariant 25: this touches the plan item only — the
    /// task keeps its estimate, blocks, sessions and completion history, and a
    /// task with an intended date on this day stays on the list, demoted to
    /// **More for today**, because removing it from the day would mean editing
    /// the date it does not own.
    pub fn remove_task_from_day(&mut self, task_id: &str, date: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM daily_plan_item WHERE task_id = ?1 AND local_date = ?2",
            params![task_id, date],
        )?;
        Ok(())
    }

    /// `ReorderDayTasks` — rewrite one section's order for one date.
    ///
    /// Tasks with no plan item acquire one here: an explicit position *is* a
    /// plan item, and a reorder that silently failed to persist for half the
    /// list would be worse than one that refused.
    pub fn reorder_day_tasks(
        &mut self,
        date: &str,
        section: DaySection,
        task_ids: &[String],
    ) -> Result<()> {
        parse_date(date)?;
        if section == DaySection::Top && task_ids.len() > TOP_LIMIT {
            return Err(AppError::invalid(
                "Top 3 holds three tasks. Move one down to make room.",
            ));
        }
        let now = self.now();
        let tx = self.conn.transaction()?;
        for (i, task_id) in task_ids.iter().enumerate() {
            let rank = (i as f64 + 1.0) * RANK_STEP;
            tx.execute(
                "INSERT INTO daily_plan_item (id, task_id, local_date, section, rank, created_at, updated_at)
                      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(task_id, local_date) DO UPDATE
                        SET section = excluded.section, rank = excluded.rank, updated_at = excluded.updated_at",
                params![new_id(), task_id, date, section.as_str(), rank, now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Invariant 24, checked before the write rather than repaired after it.
    fn assert_top_has_room(&self, task_id: &str, date: &str) -> Result<()> {
        let occupied: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM daily_plan_item
              WHERE local_date = ?1 AND section = 'top' AND task_id <> ?2",
            params![date, task_id],
            |r| r.get(0),
        )?;
        if occupied as usize >= TOP_LIMIT {
            return Err(AppError::invalid(
                "Top 3 already holds three tasks. Move one down to make room.",
            ));
        }
        Ok(())
    }
}
