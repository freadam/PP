//! The work report — five questions over one range (migration 0013).
//!
//! *How many hours did I work? On what kind of work? On which projects? How
//! much of it was deliberate? What was actually on screen?*
//!
//! # Why one function and not five
//!
//! They are always read together, on one screen, against one range. Five
//! commands would mean five range parses, five timezone resolutions and five
//! chances for the panels on the same screen to disagree about which week it
//! is. The cost of the bundle is that a screen showing one panel pays for all
//! five; at a month of data that is a handful of indexed sums, and the
//! alternative is a class of bug that is very hard to see.
//!
//! # Confirmed work only
//!
//! Every figure here except the app list comes from `time_session` — the
//! *confirmed* record. Observed time is not work until somebody says it was,
//! and a work-hours graph that quietly included "Chrome was in front" would be
//! the exact dishonesty this app is built to avoid.
//!
//! The app list is the one panel that reports observation, and it is labelled
//! as such. It answers "what was on screen", which is a different question from
//! "what did you work on" and must never be added to the same total.

use std::collections::HashMap;

use rusqlite::params;

use super::Store;
use crate::error::Result;
use crate::model::*;
use chrono::Datelike;

use crate::time::{day_end, day_start, format_date, local_date, parse_date, zone};

/// Longest first, and short enough to read. The tail of an app list is a
/// hundred one-minute entries that answer nothing.
const MAX_APPS: usize = 12;

impl Store {
    /// The five reports, for `date`'s day, ISO week, or calendar month.
    ///
    /// `period` is `"day"`, `"week"` or `"month"`; anything else is a day,
    /// because the narrowest answer is the safest one to give by mistake.
    pub fn get_work_report(&self, date: &str, period: &str, tz: &str) -> Result<WorkReport> {
        let zone_ = zone(tz)?;
        let day = parse_date(date)?;
        let (from, to) = match period {
            "week" => {
                let monday = crate::time::week_start(day);
                (monday, monday + chrono::Duration::days(6))
            }
            "month" => {
                let first = day.with_day(1).unwrap_or(day);
                let next = if first.month() == 12 {
                    chrono::NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
                } else {
                    chrono::NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
                }
                .unwrap_or(first);
                (first, next - chrono::Duration::days(1))
            }
            _ => (day, day),
        };
        let (from_s, to_s) = (format_date(from), format_date(to));
        let (start_ms, end_ms) = (day_start(from, &zone_), day_end(to, &zone_));

        // The daily target, if one is live. `applies_days` is a Monday=1 …
        // Sunday=64 bitmask, so a Mon–Fri target does not report a shortfall on
        // a Sunday — which is the whole reason the mask exists.
        let target = self.daily_work_target()?;

        let mut days = Vec::new();
        let mut cursor = from;
        while cursor <= to {
            let date = format_date(cursor);
            let (d0, d1) = (day_start(cursor, &zone_), day_end(cursor, &zone_));
            let work_sec = self.confirmed_work_between(d0, d1)?;
            let focus_sec = self.focus_planned_between(&date)?;
            let applies = target
                .as_ref()
                .map(|g| weekday_applies(cursor, g.applies_days))
                .unwrap_or(false);
            days.push(WorkDay {
                date,
                work_sec,
                target_applies: applies,
                focus_sec,
            });
            cursor += chrono::Duration::days(1);
        }

        let total_work_sec = days.iter().map(|d| d.work_sec).sum();

        // Only days that have *happened* count towards "met". A Friday that has
        // not arrived is not a day you failed to hit six hours on — the same
        // rule the month dashboard already applies to its accounted percentage.
        let today = local_date(self.now(), &zone_);
        let (met, applicable) = match &target {
            Some(g) => {
                let mut met = 0;
                let mut applicable = 0;
                for d in &days {
                    if d.target_applies && d.date.as_str() <= today.as_str() {
                        applicable += 1;
                        if d.work_sec >= g.target_sec {
                            met += 1;
                        }
                    }
                }
                (Some(met), Some(applicable))
            }
            None => (None, None),
        };

        Ok(WorkReport {
            by_category: self.work_by_category(start_ms, end_ms, total_work_sec)?,
            by_project: self.work_by_project(start_ms, end_ms, total_work_sec)?,
            focus: self.focus_summary(&from_s, &to_s)?,
            apps: self.apps_used(&from_s, &to_s, tz)?,
            from: from_s,
            to: to_s,
            tz: tz.to_string(),
            period: match period {
                "week" | "month" => period.to_string(),
                _ => "day".to_string(),
            },
            total_work_sec,
            target_sec: target.as_ref().map(|g| g.target_sec),
            target_days_met: met,
            target_days_applicable: applicable,
            days,
        })
    }

    /// The live `atLeast` daily goal on all work, if there is one.
    pub fn daily_work_target(&self) -> Result<Option<GoalRow>> {
        Ok(self
            .get_goals(false)?
            .into_iter()
            .find(|g| g.period == "day" && g.subject_id == "allWork"))
    }

    /// Confirmed work overlapping the window, clipped to it.
    ///
    /// Clipped rather than counted-if-it-starts-inside: a session running
    /// 23:40–00:20 belongs to both days, for the minutes it spent in each, or
    /// the days of a week stop summing to the week.
    fn confirmed_work_between(&self, from: Millis, to: Millis) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM((MIN(COALESCE(ended_at, ?2), ?2)
                                - MAX(started_at, ?1)) / 1000), 0)
               FROM time_session
              WHERE started_at < ?2 AND COALESCE(ended_at, ?2) > ?1",
            params![from, to],
            |r| r.get(0),
        )?)
    }

    fn focus_planned_between(&self, date: &str) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(duration_sec), 0) FROM scheduled_block
              WHERE local_date = ?1 AND deleted_at IS NULL AND is_focus = 1",
            [date],
            |r| r.get(0),
        )?)
    }

    /// Work split by kind. The uncategorised bucket is always present when it
    /// has time in it — dropping it would make the shares add to less than the
    /// total with nothing on screen explaining where the rest went.
    fn work_by_category(&self, from: Millis, to: Millis, total: i64) -> Result<Vec<WorkSlice>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.colour,
                    SUM((MIN(COALESCE(s.ended_at, ?2), ?2) - MAX(s.started_at, ?1)) / 1000)
               FROM time_session s
               JOIN task t ON t.id = s.task_id
               LEFT JOIN task_category c ON c.id = t.category_id
              WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?2) > ?1
              GROUP BY c.id
              ORDER BY 4 DESC",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        Ok(slices(rows, total, "Uncategorised")?)
    }

    fn work_by_project(&self, from: Millis, to: Millis, total: i64) -> Result<Vec<WorkSlice>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.colour,
                    SUM((MIN(COALESCE(s.ended_at, ?2), ?2) - MAX(s.started_at, ?1)) / 1000)
               FROM time_session s
               JOIN task t ON t.id = s.task_id
               LEFT JOIN project p ON p.id = t.project_id
              WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?2) > ?1
              GROUP BY p.id
              ORDER BY 4 DESC",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        Ok(slices(rows, total, "No project")?)
    }

    /// Focus sessions in the range: how many, plotted for how long, and what
    /// they actually cost.
    ///
    /// `completed` counts the sessions that ran at least their intended length.
    /// Not a pass mark — a session cut short because the work finished is a
    /// good outcome — but "six started, two ran their length" is the honest
    /// shape of a week, and averaging it away would hide it.
    fn focus_summary(&self, from: &str, to: &str) -> Result<FocusSummary> {
        let mut stmt = self.conn.prepare(
            "SELECT b.duration_sec,
                    COALESCE((SELECT SUM(s.elapsed_sec) FROM time_session s
                               WHERE s.block_id = b.id), 0)
               FROM scheduled_block b
              WHERE b.local_date BETWEEN ?1 AND ?2
                AND b.deleted_at IS NULL AND b.is_focus = 1",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;

        let mut summary = FocusSummary {
            sessions: 0,
            planned_sec: 0,
            tracked_sec: 0,
            longest_sec: 0,
            completed: 0,
        };
        for row in rows {
            let (planned, tracked) = row?;
            summary.sessions += 1;
            summary.planned_sec += planned;
            summary.tracked_sec += tracked;
            summary.longest_sec = summary.longest_sec.max(tracked);
            if tracked >= planned {
                summary.completed += 1;
            }
        }
        Ok(summary)
    }

    /// What was actually on screen, longest first.
    ///
    /// Observation, not record — read through the same span reader the Day view
    /// uses, so the minimum-span floor and the exclusions apply here exactly as
    /// they do there. Two screens disagreeing about the same afternoon is the
    /// failure that indirection exists to prevent.
    fn apps_used(&self, from: &str, to: &str, tz: &str) -> Result<Vec<AppUsage>> {
        let mut totals: HashMap<String, (i64, Option<String>)> = HashMap::new();
        let mut cursor = parse_date(from)?;
        let last = parse_date(to)?;
        while cursor <= last {
            for span in self.labelled_spans(&format_date(cursor), tz)? {
                let seconds = (span.ended_at - span.started_at) / 1000;
                let key = span.domain.clone().unwrap_or_else(|| span.app_id.clone());
                let entry = totals.entry(key).or_insert((0, span.category_id.clone()));
                entry.0 += seconds;
                // First verdict wins rather than last: the categories are
                // stamped at write time and a later span's label is not more
                // authoritative than an earlier one's.
                if entry.1.is_none() {
                    entry.1 = span.category_id.clone();
                }
            }
            cursor += chrono::Duration::days(1);
        }

        let total: i64 = totals.values().map(|(s, _)| *s).sum();
        let mut out: Vec<AppUsage> = totals
            .into_iter()
            .map(|(app_id, (seconds, category))| AppUsage {
                name: friendly_app_name(&app_id),
                app_id,
                seconds,
                share: share_of(seconds, total),
                category,
            })
            .collect();
        out.sort_by(|a, b| b.seconds.cmp(&a.seconds).then(a.name.cmp(&b.name)));
        out.truncate(MAX_APPS);
        Ok(out)
    }
}

/// Monday = 1 … Sunday = 64, matching `goal.applies_days`.
fn weekday_applies(date: chrono::NaiveDate, mask: i64) -> bool {
    let bit = 1 << date.weekday().num_days_from_monday();
    mask & bit != 0
}

fn share_of(part: i64, total: i64) -> f64 {
    if total <= 0 {
        0.0
    } else {
        (part as f64 / total as f64 * 1000.0).round() / 1000.0
    }
}

/// `"code.exe"` → `"code"`. The same reduction the Day view already applies, so
/// the two screens name the same application the same way.
fn friendly_app_name(app_id: &str) -> String {
    let trimmed = app_id
        .strip_suffix(".exe")
        .or_else(|| app_id.strip_suffix(".EXE"))
        .or_else(|| app_id.strip_suffix(".app"))
        .unwrap_or(app_id);
    trimmed.to_string()
}

/// Shared shaping for the two splits. `fallback` names the bucket for rows with
/// no category or no project, which is always kept: the shares have to add up.
fn slices<I>(rows: I, total: i64, fallback: &str) -> Result<Vec<WorkSlice>>
where
    I: Iterator<Item = rusqlite::Result<(Option<String>, Option<String>, Option<String>, i64)>>,
{
    let mut out = Vec::new();
    for row in rows {
        let (id, name, colour, seconds) = row?;
        if seconds <= 0 {
            continue;
        }
        out.push(WorkSlice {
            name: name.unwrap_or_else(|| fallback.to_string()),
            // The uncategorised bucket is deliberately a neutral grey rather
            // than a hue: it is an absence, and giving it a colour would put it
            // in the same visual class as a real category.
            colour: colour.unwrap_or_else(|| "#C7C7C7".to_string()),
            id,
            seconds,
            share: share_of(seconds, total),
        });
    }
    Ok(out)
}
