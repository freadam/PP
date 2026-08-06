//! The month dashboard (Plan Rev 3 §8.5, wireframe screen 3).
//!
//! Month is the plan's default reporting horizon, and this is what it opens to.
//!
//! **It aggregates `resolve_day`, day by day, rather than writing a second
//! query over the same tables.** That costs 28–31 passes instead of one clever
//! SQL statement, and it is the right trade: the month's headline figure and
//! the day's headline figure are then the same arithmetic by construction, and
//! the counting invariant — that a day's layers sum to the day — extends to the
//! month for free. A separate month query would be a second implementation of
//! the precedence rules, drifting from the first the day either changed.
//!
//! The §12 budget is 250ms for a populated 31-day month; each day is a handful
//! of indexed range scans, so the passes are cheap and the guard is
//! `month_load_stays_inside_its_budget`.

use crate::error::Result;
use crate::model::*;
use crate::time::{format_date, local_date, month_bounds, parse_date, zone};

use super::Store;

/// What one pass over a range of days produced.
///
/// Deliberately not a DTO: it never crosses the IPC boundary. The month view
/// and the week review each shape it into their own answer, and sharing the
/// *arithmetic* rather than the *presentation* is the whole point.
pub(crate) struct RangeTotals {
    pub days: Vec<MonthDay>,
    /// Summed across the range. The longest stretch is a **maximum**, not a
    /// sum — a week with one three-hour stretch is not a week with a
    /// twenty-one-hour one.
    pub fragmentation: Fragmentation,
    pub totals: DayTotals,
    /// Days that finished without being reviewed. A day that has not finished
    /// cannot be un-reconciled — nobody was ever going to review tomorrow.
    pub unreconciled: i64,
    /// How much of the range has actually elapsed, and how much of *that* is
    /// unaccounted for. The pair that agree, and the reason a fresh month does
    /// not report itself as 6% accounted.
    pub elapsed_sec: i64,
    pub elapsed_empty_sec: i64,
}

impl Store {
    /// Sums `get_day` over an inclusive local-date range.
    ///
    /// The month dashboard and the week review are the same arithmetic over
    /// different bounds, and this is where that is made true rather than
    /// promised. A second aggregate — a weekly `GROUP BY`, say — would be a
    /// second implementation of the precedence rules, drifting from the first
    /// the day either changed.
    pub(crate) fn aggregate_range(
        &self,
        from: &str,
        to: &str,
        tz: &str,
    ) -> Result<RangeTotals> {
        let mut cursor = parse_date(from)?;
        let last_local = to.to_string();

        let mut days = Vec::new();
        let mut totals = DayTotals {
            day_sec: 0,
            planned_sec: 0,
            confirmed_work_sec: 0,
            confirmed_life_sec: 0,
            sleep_sec: 0,
            private_sec: 0,
            observed_only_sec: 0,
            idle_sec: 0,
            empty_sec: 0,
            entertainment_sec: 0,
            pc_sec: 0,
            by_area: Vec::new(),
            by_project: Vec::new(),
            by_app: Vec::new(),
        };
        let mut areas: std::collections::HashMap<String, AreaTotal> = Default::default();
        let mut projects: std::collections::HashMap<Option<String>, ProjectTotal> =
            Default::default();
        let mut apps: std::collections::HashMap<String, i64> = Default::default();
        let mut unreconciled = 0;
        // Only days that have actually happened count toward "accounted".
        // Measuring against the whole month would report a fresh August as 6%
        // accounted on the 4th — arithmetically true, and a useless headline:
        // the missing 27 days are the future, not a gap in the record.
        let mut elapsed_sec = 0;
        let mut elapsed_empty_sec = 0;
        let mut frag = Fragmentation::default();
        let now = self.now();

        while format_date(cursor) <= last_local {
            let date = format_date(cursor);
            let day = self.get_day(&date, tz, None)?;
            let d = &day.totals;

            totals.day_sec += d.day_sec;
            totals.planned_sec += d.planned_sec;
            totals.confirmed_work_sec += d.confirmed_work_sec;
            totals.confirmed_life_sec += d.confirmed_life_sec;
            totals.sleep_sec += d.sleep_sec;
            totals.private_sec += d.private_sec;
            totals.observed_only_sec += d.observed_only_sec;
            totals.idle_sec += d.idle_sec;
            totals.empty_sec += d.empty_sec;
            totals.entertainment_sec += d.entertainment_sec;
            totals.pc_sec += d.pc_sec;

            for a in &d.by_area {
                let e = areas.entry(a.area_id.clone()).or_insert_with(|| AreaTotal {
                    seconds: 0,
                    ..a.clone()
                });
                e.seconds += a.seconds;
            }
            for p in &d.by_project {
                let e = projects
                    .entry(p.project_id.clone())
                    .or_insert_with(|| ProjectTotal {
                        seconds: 0,
                        ..p.clone()
                    });
                e.seconds += p.seconds;
            }
            for a in &d.by_app {
                *apps.entry(a.app_id.clone()).or_insert(0) += a.seconds;
            }
            // A day that has not finished cannot be un-reconciled — nobody was
            // ever going to review tomorrow.
            if !day.is_reconciled && day.ends_at <= now {
                unreconciled += 1;
            }

            frag = super::day::add_fragmentation(&frag, &day.fragmentation);

            let accounted = d.day_sec - d.empty_sec;
            if day.starts_at < now {
                // Today counts only as far as the clock has got — and its empty
                // seconds have to be clipped to the same window. Taking the
                // whole day's `empty_sec` and capping it at the elapsed length
                // would charge this morning for tonight's blank evening.
                let horizon = day.ends_at.min(now);
                elapsed_sec += (horizon - day.starts_at) / 1000;
                elapsed_empty_sec += day
                    .segments
                    .iter()
                    .filter(|s| s.owner == SlotOwner::Empty)
                    .map(|s| (s.to.min(horizon) - s.from).max(0) / 1000)
                    .sum::<i64>();
            }
            days.push(MonthDay {
                day_of_month: cursor.day() as i64,
                day_sec: d.day_sec,
                confirmed_work_sec: d.confirmed_work_sec,
                confirmed_life_sec: d.confirmed_life_sec,
                sleep_sec: d.sleep_sec,
                private_sec: d.private_sec,
                observed_only_sec: d.observed_only_sec,
                idle_sec: d.idle_sec,
                empty_sec: d.empty_sec,
                entertainment_sec: d.entertainment_sec,
                // Entertainment you meant to have. Nothing can produce this yet
                // — see `planned_entertainment_note` below.
                planned_entertainment_sec: 0,
                is_reconciled: day.is_reconciled,
                has_happened: day.ends_at <= now,
                accounted_ratio: if d.day_sec > 0 {
                    accounted as f64 / d.day_sec as f64
                } else {
                    0.0
                },
                local_date: date,
            });

            let Some(next) = cursor.succ_opt() else { break };
            cursor = next;
        }

        totals.by_area = {
            let mut v: Vec<AreaTotal> = areas.into_values().collect();
            v.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.name.cmp(&b.name)));
            v
        };
        totals.by_project = {
            let mut v: Vec<ProjectTotal> = projects.into_values().collect();
            v.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.name.cmp(&b.name)));
            v
        };
        totals.by_app = {
            let mut v: Vec<AppTotal> = apps
                .into_iter()
                .map(|(app_id, seconds)| AppTotal { app_id, seconds })
                .collect();
            v.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.app_id.cmp(&b.app_id)));
            v
        };

        Ok(RangeTotals {
            days,
            fragmentation: frag,
            totals,
            unreconciled,
            elapsed_sec,
            elapsed_empty_sec,
        })
    }

    /// `month` is `YYYY-MM`. Anything parseable as a date in that month works
    /// too, so the caller can pass a day and get its month.
    pub fn get_month(&self, month: &str, tz: &str) -> Result<MonthView> {
        let zone_ = zone(tz)?;
        let anchor = if month.len() == 7 {
            parse_date(&format!("{month}-01"))?
        } else {
            parse_date(month)?
        };
        let first_date = anchor.with_day(1).unwrap_or(anchor);
        let first = format_date(first_date);
        let (_from_ms, to_ms) = month_bounds(&first, &zone_)?;
        let last_local = local_date(to_ms - 1, &zone_);

        let RangeTotals {
            days,
            mut totals,
            fragmentation: _,
            unreconciled,
            elapsed_sec,
            elapsed_empty_sec,
        } = self.aggregate_range(&first, &last_local, tz)?;

        // Monthly targets belong on the area rows here rather than on the day's,
        // because a target is a monthly quantity and showing it against one
        // day's total would invite exactly the wrong comparison.
        let targets = self.get_life_areas(tz, false)?;
        for a in totals.by_area.iter_mut() {
            if let Some(t) = targets.iter().find(|t| t.id == a.area_id) {
                a.monthly_target_sec = t.monthly_target_sec;
            }
        }
        // An area with a target and no time at all still belongs on the chart —
        // a zero against a target is the most actionable row there is.
        for t in &targets {
            if t.monthly_target_sec.is_some()
                && !totals.by_area.iter().any(|a| a.area_id == t.id)
            {
                totals.by_area.push(AreaTotal {
                    area_id: t.id.clone(),
                    name: t.name.clone(),
                    colour: t.colour.clone(),
                    kind: t.kind,
                    seconds: 0,
                    monthly_target_sec: t.monthly_target_sec,
                });
            }
        }

        let accounted_ratio = if elapsed_sec > 0 {
            (elapsed_sec - elapsed_empty_sec) as f64 / elapsed_sec as f64
        } else {
            0.0
        };

        Ok(MonthView {
            month: first[..7].to_string(),
            label: month_label(&first),
            from: first.clone(),
            to: last_local,
            tz: tz.to_string(),
            findings: self.month_findings(&totals, unreconciled, &days, elapsed_empty_sec)?,
            planned_entertainment_note: Some(
                "Planned entertainment is always zero: there is no way to plan an \
                 entertainment window yet, so every minute of it is unplanned by \
                 definition. The dashed line becomes real when windows land."
                    .into(),
            ),
            totals,
            accounted_ratio,
            elapsed_sec,
            elapsed_empty_sec,
            unreconciled_days: unreconciled,
            days,
        })
    }

    /// The "Monthly findings" panel. Judgement about what counts as a finding
    /// lives here rather than in the renderer — it is the panel's whole content,
    /// and a second copy of it would be a second opinion.
    fn month_findings(
        &self,
        totals: &DayTotals,
        unreconciled: i64,
        days: &[MonthDay],
        elapsed_empty_sec: i64,
    ) -> Result<Vec<MonthFinding>> {
        let mut out = Vec::new();

        if totals.entertainment_sec > 0 {
            let worst = days
                .iter()
                .max_by_key(|d| d.entertainment_sec)
                .filter(|d| d.entertainment_sec > 0);
            out.push(MonthFinding {
                key: "entertainment".into(),
                label: "Entertainment, all unplanned".into(),
                value: fmt_hm(totals.entertainment_sec),
                detail: worst.map(|d| {
                    format!(
                        "Worst day: {} on the {}",
                        fmt_hm(d.entertainment_sec),
                        d.day_of_month
                    )
                }),
                is_warning: true,
            });
        }

        // The largest project overrun, as a percentage, from plotted blocks.
        if let Some((name, pct)) = self.largest_overrun(&days[0].local_date, days.last().map(|d| d.local_date.as_str()).unwrap_or(""))? {
            out.push(MonthFinding {
                key: "overrun".into(),
                label: "Largest project overrun".into(),
                value: format!("{pct:+.0}%"),
                detail: Some(name),
                is_warning: pct > 25.0,
            });
        }

        if totals.observed_only_sec > 0 {
            out.push(MonthFinding {
                key: "observedOnly".into(),
                label: "Observed, never confirmed".into(),
                value: fmt_hm(totals.observed_only_sec),
                detail: Some("The machine saw it; nobody said what it was.".into()),
                is_warning: true,
            });
        }

        out.push(MonthFinding {
            key: "unaccounted".into(),
            // Days that have happened, not the whole month: a future day is not
            // unaccounted for, it simply has not arrived.
            label: "Unaccounted so far".into(),
            value: fmt_hm(elapsed_empty_sec),
            detail: (unreconciled > 0).then(|| {
                format!(
                    "{unreconciled} day{} not reconciled",
                    if unreconciled == 1 { "" } else { "s" }
                )
            }),
            is_warning: elapsed_empty_sec * 4 > totals.day_sec,
        });

        Ok(out)
    }

    /// Project with the worst tracked-versus-plotted ratio over the range.
    /// Returns `None` when nothing was plotted — a percentage of zero is not a
    /// finding, it is a division by zero wearing a hat.
    fn largest_overrun(&self, from: &str, to: &str) -> Result<Option<(String, f64)>> {
        let row: Option<(String, i64, i64)> = self
            .conn
            .query_row(
                "SELECT COALESCE(p.name, 'No project'),
                        SUM(b.duration_sec),
                        SUM(COALESCE(c.tracked_sec, 0))
                   FROM scheduled_block b
                   LEFT JOIN task t    ON t.id = b.task_id
                   LEFT JOIN project p ON p.id = t.project_id
                   LEFT JOIN block_tracked_cache c ON c.block_id = b.id
                  WHERE b.deleted_at IS NULL AND b.local_date >= ?1 AND b.local_date <= ?2
                  GROUP BY p.id
                 HAVING SUM(b.duration_sec) > 0
                  ORDER BY (CAST(SUM(COALESCE(c.tracked_sec, 0)) AS REAL)
                            / SUM(b.duration_sec)) DESC
                  LIMIT 1",
                [from, to],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        Ok(row.map(|(name, planned, tracked)| {
            (name, (tracked as f64 / planned as f64 - 1.0) * 100.0)
        }))
    }
}

fn fmt_hm(seconds: i64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    if h == 0 {
        format!("{m}m")
    } else if m == 0 {
        format!("{h}h")
    } else {
        format!("{h}h {m}m")
    }
}

fn month_label(first: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    let m: usize = first[5..7].parse().unwrap_or(1);
    format!("{} {}", MONTHS[(m - 1).min(11)], &first[..4])
}

use chrono::Datelike;
