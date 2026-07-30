//! Reports (§3.6) — the payoff for the whole drift concept.
//!
//! Three panels, no more: calibration, planned-vs-tracked per project per
//! week, and weekly targets with pace-to-date.

use std::collections::HashMap;

use rusqlite::params;

use super::Store;
use crate::error::Result;
use crate::model::*;
use crate::time::{day_start, format_date, local_date, parse_date, week_start, zone};

/// Report a bucket only at n ≥ 5, so a single outlier doesn't shout (F6).
const MIN_SAMPLES: i64 = 5;
const CALIBRATION_WINDOW_DAYS: i64 = 30;

impl Store {
    pub fn get_reports(&self, range: &DateRange, filter: &ReportFilter) -> Result<ReportBundle> {
        let tz = filter
            .tz
            .clone()
            .unwrap_or_else(|| self.tz_or("UTC"));
        let zone_ = zone(&tz)?;
        let calibration = self.calibration(&tz, filter.project_id.as_deref())?;
        let project_weeks = self.project_weeks(range, &tz, filter.project_id.as_deref())?;
        let weekly_targets = self.weekly_targets(&tz)?;

        let from = day_start(parse_date(&range.from)?, &zone_);
        let to = day_start(
            parse_date(&range.to)?.succ_opt().unwrap(),
            &zone_,
        );
        let total_planned_sec: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(duration_sec), 0) FROM scheduled_block
              WHERE deleted_at IS NULL AND local_date BETWEEN ?1 AND ?2",
            params![range.from, range.to],
            |r| r.get(0),
        )?;
        let total_tracked_sec: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(elapsed_sec), 0) FROM time_session
              WHERE started_at >= ?1 AND started_at < ?2",
            params![from, to],
            |r| r.get(0),
        )?;

        Ok(ReportBundle {
            from: range.from.clone(),
            to: range.to.clone(),
            calibration,
            project_weeks,
            weekly_targets,
            total_planned_sec,
            total_tracked_sec,
            streak_days: self.reconcile_streak(&tz)?,
        })
    }

    /// Trailing 30-day `tracked ÷ estimate`, bucketed by estimate size.
    ///
    /// The median is computed here rather than in SQL because SQLite has no
    /// median aggregate — and mean is the wrong statistic anyway: one
    /// abandoned task ruins it (§6.4).
    pub fn calibration(&self, tz: &str, project_id: Option<&str>) -> Result<CalibrationReport> {
        let zone_ = zone(tz)?;
        let today = parse_date(&local_date(self.now(), &zone_))?;
        let since = day_start(
            today - chrono::Duration::days(CALIBRATION_WINDOW_DAYS),
            &zone_,
        );

        let mut sql = String::from(
            "SELECT CASE WHEN t.estimate_sec <=  900 THEN '<=15m'
                         WHEN t.estimate_sec <= 1800 THEN '30m'
                         WHEN t.estimate_sec <= 3600 THEN '1h'
                         WHEN t.estimate_sec <= 7200 THEN '2h'
                         ELSE '3h+' END,
                    CAST(tt.tracked_sec AS REAL) / t.estimate_sec
               FROM task t
               JOIN task_tracked tt ON tt.task_id = t.id
              WHERE t.status = 'done' AND t.deleted_at IS NULL
                AND t.estimate_sec IS NOT NULL AND tt.tracked_sec > 0
                AND t.completed_at >= ?1",
        );
        if project_id.is_some() {
            sql.push_str(" AND t.project_id = ?2");
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let mapped: Vec<(String, f64)> = match project_id {
            Some(p) => stmt
                .query_map(params![since, p], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?,
            None => stmt
                .query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?,
        };

        let mut by_bucket: HashMap<String, Vec<f64>> = HashMap::new();
        let mut all: Vec<f64> = Vec::new();
        for (bucket, ratio) in mapped {
            by_bucket.entry(bucket).or_default().push(ratio);
            all.push(ratio);
        }

        let order = ["<=15m", "30m", "1h", "2h", "3h+"];
        let buckets: Vec<CalibrationBucket> = order
            .iter()
            .map(|name| {
                let ratios = by_bucket.remove(*name).unwrap_or_default();
                let n = ratios.len() as i64;
                CalibrationBucket {
                    bucket: (*name).to_string(),
                    n,
                    median_ratio: median(ratios).unwrap_or(0.0),
                    is_reportable: n >= MIN_SAMPLES,
                }
            })
            .collect();

        let sample_count = all.len() as i64;
        let overall_median = median(all);
        let worst = buckets
            .iter()
            .filter(|b| b.is_reportable)
            .max_by(|a, b| {
                (a.median_ratio - 1.0)
                    .abs()
                    .partial_cmp(&(b.median_ratio - 1.0).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|b| b.bucket.clone());

        Ok(CalibrationReport {
            headline: headline(overall_median, sample_count, worst.as_deref()),
            worst_bucket: worst,
            overall_median,
            sample_count,
            buckets,
        })
    }

    /// Planned vs tracked, per project per week — drawn with the same plot/track
    /// encoding as the drift rail, rotated horizontal (§3.6).
    fn project_weeks(
        &self,
        range: &DateRange,
        tz: &str,
        project_id: Option<&str>,
    ) -> Result<Vec<ProjectWeekRow>> {
        let zone_ = zone(tz)?;
        let mut rows: HashMap<(Option<String>, String), ProjectWeekRow> = HashMap::new();

        let mut names: HashMap<String, (String, String)> = HashMap::new();
        for p in self.get_projects(tz)? {
            names.insert(p.id.clone(), (p.name.clone(), p.colour.clone()));
        }

        // Planned: blocks live on a local_date already.
        let mut stmt = self.conn.prepare(
            "SELECT t.project_id, b.local_date, b.duration_sec
               FROM scheduled_block b
               LEFT JOIN task t ON t.id = b.task_id
              WHERE b.deleted_at IS NULL AND b.local_date BETWEEN ?1 AND ?2",
        )?;
        let planned = stmt.query_map(params![range.from, range.to], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in planned {
            let (pid, date, dur) = row?;
            if let Some(want) = project_id {
                if pid.as_deref() != Some(want) {
                    continue;
                }
            }
            let ws = format_date(week_start(parse_date(&date)?));
            let entry = rows
                .entry((pid.clone(), ws.clone()))
                .or_insert_with(|| new_row(&pid, &ws, &names));
            entry.planned_sec += dur;
        }
        drop(stmt);

        // Tracked: sessions carry an instant, so they need the zone to land on
        // the right week.
        let from = day_start(parse_date(&range.from)?, &zone_);
        let to = day_start(parse_date(&range.to)?.succ_opt().unwrap(), &zone_);
        let mut stmt = self.conn.prepare(
            "SELECT t.project_id, s.started_at, s.elapsed_sec
               FROM time_session s JOIN task t ON t.id = s.task_id
              WHERE s.started_at >= ?1 AND s.started_at < ?2",
        )?;
        let tracked = stmt.query_map(params![from, to], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in tracked {
            let (pid, started_at, elapsed) = row?;
            if let Some(want) = project_id {
                if pid.as_deref() != Some(want) {
                    continue;
                }
            }
            let date = local_date(started_at, &zone_);
            let ws = format_date(week_start(parse_date(&date)?));
            let entry = rows
                .entry((pid.clone(), ws.clone()))
                .or_insert_with(|| new_row(&pid, &ws, &names));
            entry.tracked_sec += elapsed;
        }

        let mut out: Vec<ProjectWeekRow> = rows.into_values().collect();
        out.sort_by(|a, b| {
            a.week_start
                .cmp(&b.week_start)
                .then(a.project_name.cmp(&b.project_name))
        });
        Ok(out)
    }

    /// Target vs actual per project, with pace-to-date rather than a bare total.
    fn weekly_targets(&self, tz: &str) -> Result<Vec<WeeklyTargetRow>> {
        let zone_ = zone(tz)?;
        let today = parse_date(&local_date(self.now(), &zone_))?;
        let ws = week_start(today);
        let from = day_start(ws, &zone_);
        let days_elapsed = (today - ws).num_days() + 1;

        let mut out = Vec::new();
        for p in self.get_projects(tz)? {
            let Some(target) = p.weekly_target_sec else {
                continue;
            };
            if p.is_archived {
                continue;
            }
            let tracked: i64 = self.conn.query_row(
                "SELECT COALESCE(SUM(s.elapsed_sec), 0)
                   FROM time_session s JOIN task t ON t.id = s.task_id
                  WHERE t.project_id = ?1 AND s.started_at >= ?2",
                params![p.id, from],
                |r| r.get(0),
            )?;
            out.push(WeeklyTargetRow {
                project_id: p.id,
                project_name: p.name,
                project_colour: p.colour,
                target_sec: target,
                tracked_sec: tracked,
                pace_sec: target * days_elapsed / 7,
            });
        }
        Ok(out)
    }

    /// CSV of the rows behind the report (§3.6, §6.12).
    pub fn report_csv(&self, range: &DateRange, filter: &ReportFilter) -> Result<String> {
        let bundle = self.get_reports(range, filter)?;
        let mut out = String::from("week_start,project,planned_sec,tracked_sec,drift_sec\n");
        for row in bundle.project_weeks {
            out.push_str(&format!(
                "{},{},{},{},{}\n",
                row.week_start,
                csv_escape(&row.project_name),
                row.planned_sec,
                row.tracked_sec,
                row.tracked_sec - row.planned_sec
            ));
        }
        Ok(out)
    }
}

fn new_row(
    pid: &Option<String>,
    week_start: &str,
    names: &HashMap<String, (String, String)>,
) -> ProjectWeekRow {
    let (name, colour) = pid
        .as_ref()
        .and_then(|id| names.get(id).cloned())
        .unwrap_or_else(|| ("Inbox".to_string(), "#8B95A3".to_string()));
    ProjectWeekRow {
        project_id: pid.clone(),
        project_name: name,
        project_colour: colour,
        week_start: week_start.to_string(),
        planned_sec: 0,
        tracked_sec: 0,
    }
}

pub fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    })
}

/// Stated in plain language (§3.6): *"You run 1.24× your estimates. Your 3h
/// estimates are the least accurate."*
fn headline(median: Option<f64>, n: i64, worst: Option<&str>) -> String {
    match median {
        None => "Come back after five tracked tasks — calibration needs a few estimates to compare."
            .into(),
        Some(_) if n < MIN_SAMPLES => format!(
            "{n} of 5 estimates tracked so far. Calibration starts at five."
        ),
        Some(m) => {
            let mut s = if m >= 1.05 {
                format!("You run {m:.2}× your estimates.")
            } else if m <= 0.95 {
                format!("You come in at {m:.2}× your estimates — you over-estimate.")
            } else {
                format!("You land within 5% of your estimates ({m:.2}×).")
            };
            if let Some(bucket) = worst {
                s.push_str(&format!(" Your {bucket} estimates are the least accurate."));
            }
            s
        }
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_is_not_mean() {
        // One abandoned task: mean 3.1, median 1.1. The median is the honest one.
        let ratios = vec![1.0, 1.1, 1.2, 1.05, 11.2];
        assert_eq!(median(ratios.clone()).unwrap(), 1.1);
        let mean: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
        assert!(mean > 3.0);
    }

    #[test]
    fn median_of_even_counts_averages_the_middle() {
        assert_eq!(median(vec![1.0, 2.0, 3.0, 4.0]).unwrap(), 2.5);
    }
}
