//! Weekly goals, and pace (migration 0008, PLAN-WEEKLY-GOALS.md W1/W2).
//!
//! Closes **M11**: an entertainment budget is a goal with
//! `direction = atMost` and `subject = metric:entertainment`, so the general
//! mechanism gets the specific requirement for nothing.
//!
//! # Pace is the feature, not the target
//!
//! A target you read on Friday is a report card. The question worth answering is
//! *where should I be right now, and where am I?* So every goal reports four
//! numbers, and the fourth is the one that changes behaviour:
//!
//! - **actual** — the number;
//! - **expected by now** — `target × (elapsed applicable days ÷ all applicable
//!   days)`, today clipped to the clock;
//! - **delta** — ahead or behind, in hours and minutes, never a bare percentage;
//! - **what the rest of the week needs** — *"3h 20m a day for the remaining 3
//!   days"*, which is a decision you can make at breakfast. "62% of target" is a
//!   fact.
//!
//! # The future is never a shortfall
//!
//! The same rule that stopped the month dashboard reporting a fresh August as
//! "6% accounted" on the 4th. A goal at zero on Monday morning is **on pace**,
//! and has to say so — an app that reports the future as a failure is one whose
//! numbers you learn to discount.
//!
//! `applies_days` is what makes that true across a working week: a Mon–Fri goal
//! must not expect progress on Saturday, and must not report you behind on a
//! Sunday morning.

use rusqlite::{params, Row};

use super::Store;
use crate::error::{AppError, Result};
use crate::ids::{new_id, validate_id};
use crate::model::*;
use crate::time::{format_date, local_date, parse_date, week_start, zone};

/// Monday = 1 … Sunday = 64.
pub const ALL_DAYS: i64 = 127;

/// `YYYY-Www`, ISO. The key a goal's lifetime is recorded in — comparable as a
/// string, which is what lets "was this goal in force that week" be a `<=`.
pub fn iso_week(date: &str) -> Result<String> {
    let d = parse_date(date)?;
    let iso = chrono::Datelike::iso_week(&d);
    Ok(format!("{}-W{:02}", iso.year(), iso.week()))
}

fn map_goal(r: &Row) -> rusqlite::Result<GoalRow> {
    let kind: String = r.get(1)?;
    let direction: String = r.get(3)?;
    Ok(GoalRow {
        id: r.get(0)?,
        subject_kind: GoalSubject::parse(&kind).unwrap_or(GoalSubject::Metric),
        subject_id: r.get(2)?,
        subject_name: String::new(),
        direction: GoalDirection::parse(&direction).unwrap_or(GoalDirection::AtLeast),
        target_sec: r.get(4)?,
        period: r.get(5)?,
        applies_days: r.get(6)?,
        starts_week: r.get(7)?,
        ends_week: r.get(8)?,
    })
}

const GOAL_COLS: &str =
    "id, subject_kind, subject_id, direction, target_sec, period, applies_days, starts_week, ends_week";

/// The wording a metric goal is displayed with. In Rust because the renderer
/// re-deriving it would be a second list, drifting the day either changed.
fn metric_name(id: &str) -> &str {
    match id {
        metric::ALL_WORK => "Work",
        metric::LIFE => "Life",
        metric::SLEEP => "Sleep",
        metric::ENTERTAINMENT => "Entertainment",
        metric::UNACCOUNTED => "Unaccounted",
        other => other,
    }
}

impl Store {
    pub fn get_goals(&self, include_ended: bool) -> Result<Vec<GoalRow>> {
        let sql = format!(
            "SELECT {GOAL_COLS} FROM goal {} ORDER BY subject_kind, subject_id",
            if include_ended {
                ""
            } else {
                "WHERE ends_week IS NULL"
            }
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_goal)?;
        let mut goals: Vec<GoalRow> = rows.collect::<std::result::Result<_, _>>()?;
        for g in &mut goals {
            g.subject_name = self.subject_name(g)?;
        }
        Ok(goals)
    }

    fn subject_name(&self, goal: &GoalRow) -> Result<String> {
        Ok(match goal.subject_kind {
            GoalSubject::Metric => metric_name(&goal.subject_id).to_string(),
            GoalSubject::LifeArea => self
                .conn
                .query_row(
                    "SELECT name FROM life_area WHERE id = ?1",
                    [&goal.subject_id],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "Deleted area".into()),
            GoalSubject::Project => self
                .conn
                .query_row(
                    "SELECT name FROM project WHERE id = ?1",
                    [&goal.subject_id],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "Deleted project".into()),
            GoalSubject::Category => self
                .conn
                .query_row(
                    "SELECT name FROM observation_category WHERE id = ?1",
                    [&goal.subject_id],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "Deleted label".into()),
        })
    }

    /// Creates a goal, replacing any live one for the same subject.
    ///
    /// Replacing rather than erroring, because "I meant 20 hours, not 25" is the
    /// commonest edit there is — and the old goal is **closed, not deleted**, so
    /// a review of the week it governed still shows the number that was actually
    /// in force. A goal edited into a new figure retroactively would rewrite how
    /// a month went, and reviews would stop meaning anything.
    pub fn set_goal(&mut self, input: NewGoal, today: &str) -> Result<GoalRow> {
        let kind = input.subject_kind.unwrap_or(GoalSubject::Metric);
        let direction = input.direction.unwrap_or(GoalDirection::AtLeast);
        if input.target_sec <= 0 {
            return Err(AppError::invalid("A goal needs a target above zero."));
        }
        let applies = input.applies_days.unwrap_or(ALL_DAYS);
        if !(1..=ALL_DAYS).contains(&applies) {
            return Err(AppError::invalid("Pick at least one day of the week."));
        }
        self.check_subject(kind, &input.subject_id)?;

        // A goal's period is part of its identity (migration 0013): a daily and
        // a weekly target on the same subject are two coherent claims, so only
        // a same-period goal is closed when a new one is set.
        let period = input.period.as_deref().unwrap_or("week");
        if !matches!(period, "day" | "week" | "month") {
            return Err(AppError::invalid(format!(
                "'{period}' isn't a goal period. Use day, week or month."
            )));
        }

        let week = iso_week(today)?;
        let now = self.now();
        let tx = self.conn.transaction()?;
        // Close the outgoing goal at the week the new one starts. Same shape as
        // every other "the record keeps what it was told" rule here.
        tx.execute(
            "UPDATE goal SET ends_week = ?3, updated_at = ?4
              WHERE subject_kind = ?1 AND subject_id = ?2 AND period = ?5
                AND ends_week IS NULL",
            params![kind.as_str(), input.subject_id, week, now, period],
        )?;
        let id = new_id();
        tx.execute(
            "INSERT INTO goal
               (id, subject_kind, subject_id, direction, target_sec, period, applies_days,
                starts_week, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
            params![
                id,
                kind.as_str(),
                input.subject_id,
                direction.as_str(),
                input.target_sec,
                period,
                applies,
                week,
                self.device_id,
                now
            ],
        )?;
        tx.commit()?;

        let mut goal: GoalRow = self.conn.query_row(
            &format!("SELECT {GOAL_COLS} FROM goal WHERE id = ?1"),
            [&id],
            map_goal,
        )?;
        goal.subject_name = self.subject_name(&goal)?;
        Ok(goal)
    }

    fn check_subject(&self, kind: GoalSubject, id: &str) -> Result<()> {
        let (table, what) = match kind {
            GoalSubject::Metric => {
                return if metric::ALL.contains(&id) {
                    Ok(())
                } else {
                    Err(AppError::invalid(format!("'{id}' isn't something Fruit measures.")))
                }
            }
            GoalSubject::LifeArea => ("life_area", "life area"),
            GoalSubject::Project => ("project", "project"),
            GoalSubject::Category => ("observation_category", "label"),
        };
        validate_id(id, "subject")?;
        let n: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )?;
        if n == 0 {
            return Err(AppError::invalid(format!("That {what} no longer exists.")));
        }
        Ok(())
    }

    /// Ends a goal without deleting it. See `set_goal`.
    pub fn end_goal(&mut self, id: &str, today: &str) -> Result<()> {
        validate_id(id, "goal")?;
        let week = iso_week(today)?;
        let now = self.now();
        let n = self.conn.execute(
            "UPDATE goal SET ends_week = ?2, updated_at = ?3 WHERE id = ?1 AND ends_week IS NULL",
            params![id, week, now],
        )?;
        if n == 0 {
            return Err(AppError::invalid("That goal has already ended."));
        }
        Ok(())
    }

    // ─── the week ──────────────────────────────────────────────────────

    /// The week containing `date`, with every live goal's pace.
    pub fn get_week_review(&self, date: &str, tz: &str) -> Result<WeekReview> {
        let zone_ = zone(tz)?;
        let monday = week_start(parse_date(date)?);
        let sunday = monday + chrono::Duration::days(6);
        let (from, to) = (format_date(monday), format_date(sunday));

        let range = self.aggregate_range(&from, &to, tz)?;
        let today = local_date(self.now(), &zone_);

        let goals = self
            .get_goals(false)?
            .into_iter()
            .map(|goal| self.progress(goal, &range, &from, &today, tz))
            .collect::<Result<Vec<_>>>()?;

        // The week before, for direction of travel. One week's longest stretch
        // in isolation says nothing; "up from 42 minutes" is the whole reading.
        let previous_monday = monday - chrono::Duration::days(7);
        let previous = self.aggregate_range(
            &format_date(previous_monday),
            &format_date(previous_monday + chrono::Duration::days(6)),
            tz,
        )?;

        Ok(WeekReview {
            calibration: self.calibrate(monday, tz)?,
            fragmentation: range.fragmentation.clone(),
            previous_fragmentation: previous.fragmentation,
            week: iso_week(&from)?,
            from,
            to,
            tz: tz.to_string(),
            totals: range.totals.clone(),
            days: range.days.clone(),
            elapsed_sec: range.elapsed_sec,
            elapsed_empty_sec: range.elapsed_empty_sec,
            unreconciled_days: range.unreconciled,
            goals,
        })
    }

    /// Goals whose recent history says the number has stopped being a goal.
    ///
    /// Trailing completed weeks, median, **n ≥ 5** — the same discipline `f6`
    /// already holds the estimate calibration to, and for the same reason: five
    /// samples of noise must not move a recommendation.
    ///
    /// Only *completed* weeks count. Including the week in progress would drag
    /// every median down by however much of it is left, and recommend a cut on
    /// Tuesday every single time.
    fn calibrate(&self, monday: chrono::NaiveDate, tz: &str) -> Result<Vec<GoalCalibration>> {
        const WEEKS: i64 = 6;
        const MIN_SAMPLES: i64 = 5;
        /// Below this, the honest reading is "nothing was recorded", not "your
        /// goal is wrong".
        const MIN_MEDIAN_SEC: i64 = 30 * 60;

        let goals = self.get_goals(false)?;
        if goals.is_empty() {
            return Ok(Vec::new());
        }

        let mut samples: std::collections::HashMap<String, Vec<i64>> = Default::default();
        for back in 1..=WEEKS {
            let start = monday - chrono::Duration::days(7 * back);
            let range = self.aggregate_range(
                &format_date(start),
                &format_date(start + chrono::Duration::days(6)),
                tz,
            )?;
            // A week with nothing recorded in it is not a sample.
            //
            // `aggregate_range` answers for any range, so without this every
            // goal would appear to have six observations from its first day —
            // and the n ≥ 5 rule below would be decoration rather than a
            // threshold. A week you did not use Fruit is silence, not evidence.
            if range.totals.confirmed_work_sec + range.totals.confirmed_life_sec == 0 {
                continue;
            }
            for goal in &goals {
                let actual = match goal.subject_kind {
                    GoalSubject::Category => {
                        let from = format_date(start);
                        let to = format_date(start + chrono::Duration::days(6));
                        self.get_categories(Some((&from, &to)), tz)?
                            .into_iter()
                            .find(|c| c.id == goal.subject_id)
                            .map(|c| c.seconds)
                            .unwrap_or(0)
                    }
                    _ => self.actual_for(goal, &range),
                };
                samples.entry(goal.id.clone()).or_default().push(actual);
            }
        }

        let mut out = Vec::new();
        for goal in goals {
            let Some(weeks) = samples.get(&goal.id) else {
                continue;
            };
            if (weeks.len() as i64) < MIN_SAMPLES {
                continue;
            }
            let mut sorted = weeks.clone();
            sorted.sort_unstable();
            let median = sorted[sorted.len() / 2];

            // Nothing to calibrate against. A median of zero on an "at least"
            // goal means the quantity was never recorded, which is a data
            // problem — and "try 0m?" is not advice, it is an insult dressed as
            // one.
            if median < MIN_MEDIAN_SEC {
                continue;
            }
            let met = weeks
                .iter()
                .filter(|w| match goal.direction {
                    GoalDirection::AtLeast => **w >= goal.target_sec,
                    GoalDirection::AtMost => **w <= goal.target_sec,
                })
                .count() as i64;

            // Nothing to say while the goal is working. "Working" is generous on
            // purpose: a goal met most weeks is a goal, and nagging about the
            // ones it was not is how advice gets ignored.
            if met * 2 > weeks.len() as i64 {
                continue;
            }

            // Move toward what actually happens, and only ever toward it. A
            // suggestion that overshoots reality is the same mistake in a new
            // direction.
            let suggested = match goal.direction {
                GoalDirection::AtLeast => median.min(goal.target_sec),
                GoalDirection::AtMost => median.max(goal.target_sec),
            };
            if suggested == goal.target_sec {
                continue;
            }

            let summary = format!(
                "{}: {} target, {} median over {} weeks. A goal you miss most weeks has stopped being a goal. Try {}?",
                goal.subject_name,
                hm(goal.target_sec),
                hm(median),
                weeks.len(),
                hm(suggested),
            );
            out.push(GoalCalibration {
                goal_id: goal.id.clone(),
                subject_kind: goal.subject_kind,
                subject_id: goal.subject_id.clone(),
                direction: goal.direction,
                subject_name: goal.subject_name.clone(),
                target_sec: goal.target_sec,
                median_sec: median,
                weeks: weeks.len() as i64,
                weeks_met: met,
                suggested_sec: suggested,
                summary,
            });
        }
        Ok(out)
    }

    /// What this goal measured over the week.
    ///
    /// Every branch reads the **same** `DayTotals` the Day view renders, summed
    /// by `aggregate_range`. There is no second query, so a goal's figure and the
    /// day's figure cannot disagree.
    fn actual_for(&self, goal: &GoalRow, range: &super::month::RangeTotals) -> i64 {
        let t = &range.totals;
        match goal.subject_kind {
            GoalSubject::Metric => match goal.subject_id.as_str() {
                metric::ALL_WORK => t.confirmed_work_sec,
                metric::LIFE => t.confirmed_life_sec,
                metric::SLEEP => t.sleep_sec,
                metric::ENTERTAINMENT => t.entertainment_sec,
                metric::UNACCOUNTED => t.empty_sec,
                _ => 0,
            },
            GoalSubject::LifeArea => t
                .by_area
                .iter()
                .find(|a| a.area_id == goal.subject_id)
                .map(|a| a.seconds)
                .unwrap_or(0),
            GoalSubject::Project => t
                .by_project
                .iter()
                .find(|p| p.project_id.as_deref() == Some(goal.subject_id.as_str()))
                .map(|p| p.seconds)
                .unwrap_or(0),
            // Observation carrying one label. `by_app` cannot answer this, so it
            // is the one subject that needs its own read — still over the same
            // spans, through the same reader.
            GoalSubject::Category => 0,
        }
    }

    fn progress(
        &self,
        goal: GoalRow,
        range: &super::month::RangeTotals,
        from: &str,
        today: &str,
        tz: &str,
    ) -> Result<GoalProgress> {
        let actual_sec = match goal.subject_kind {
            GoalSubject::Category => {
                let to = format_date(parse_date(from)? + chrono::Duration::days(6));
                self.get_categories(Some((from, &to)), tz)?
                    .into_iter()
                    .find(|c| c.id == goal.subject_id)
                    .map(|c| c.seconds)
                    .unwrap_or(0)
            }
            _ => self.actual_for(&goal, range),
        };

        let (applicable, elapsed, left) = week_shape(from, today, goal.applies_days)?;
        // The future is never a shortfall: expected is pro-rated over the days
        // that have *happened*, so Monday morning expects nothing.
        //
        // A **daily** goal is deliberately exempt from the pro-rating. There is
        // no honest way to spread six hours of work across a day — nobody works
        // a uniform 25% of every hour — so pro-rating within the day would
        // report "behind" at 09:40 to someone who is simply not finished yet.
        // That is the same mistake as calling a fresh August "6% accounted",
        // which this codebase already refuses to make elsewhere. A daily goal
        // expects its full target by the end of the day and nothing before it.
        let expected_sec = if goal.period == "day" {
            0
        } else if applicable > 0 {
            (goal.target_sec as f64 * (elapsed as f64 / applicable as f64)).round() as i64
        } else {
            0
        };

        // Positive is good in both directions — ahead of pace, or under budget.
        let delta_sec = match goal.direction {
            GoalDirection::AtLeast => actual_sec - expected_sec,
            GoalDirection::AtMost => expected_sec - actual_sec,
        };
        let remaining = goal.target_sec - actual_sec;

        let state = match goal.direction {
            GoalDirection::AtLeast if actual_sec >= goal.target_sec => GoalState::Met,
            GoalDirection::AtLeast if delta_sec < 0 => GoalState::Behind,
            GoalDirection::AtLeast => GoalState::OnPace,
            GoalDirection::AtMost if remaining < 0 => GoalState::Over,
            // The week is out of applicable days and the budget held.
            GoalDirection::AtMost if left == 0 => GoalState::Met,
            GoalDirection::AtMost if delta_sec < 0 => GoalState::AtRisk,
            GoalDirection::AtMost => GoalState::OnPace,
        };

        // What the rest of the week has to look like. `None` once there are no
        // applicable days left — "0h a day for the remaining 0 days" is noise.
        let per_day_needed_sec = match (goal.direction, left) {
            (_, 0) => None,
            (GoalDirection::AtLeast, n) if remaining > 0 => Some((remaining + n - 1) / n),
            (GoalDirection::AtLeast, _) => None,
            (GoalDirection::AtMost, n) if remaining > 0 => Some(remaining / n),
            (GoalDirection::AtMost, _) => None,
        };

        let summary = summarise(&goal, state, remaining, per_day_needed_sec, left);
        Ok(GoalProgress {
            goal,
            actual_sec,
            expected_sec,
            delta_sec,
            state,
            per_day_needed_sec,
            applicable_days: applicable,
            days_left: left,
            summary,
        })
    }
}

/// `(applicable days, elapsed applicable days, applicable days left)` for the
/// week starting `from`, as of `today`.
///
/// "Elapsed" counts whole days that have finished. Today is deliberately *not*
/// counted as elapsed even in part: a goal is a quantity for a day, and being
/// three hours short at 9am is not being behind. Counting today fractionally
/// would put every goal into `Behind` every morning, which is exactly the
/// "reports the future as a failure" problem in a smaller frame.
fn week_shape(from: &str, today: &str, applies_days: i64) -> Result<(i64, i64, i64)> {
    use chrono::Datelike;
    let monday = parse_date(from)?;
    let today_d = parse_date(today)?;

    let (mut applicable, mut elapsed, mut left) = (0, 0, 0);
    for offset in 0..7 {
        let day = monday + chrono::Duration::days(offset);
        let bit = 1 << day.weekday().num_days_from_monday();
        if applies_days & bit == 0 {
            continue;
        }
        applicable += 1;
        if day < today_d {
            elapsed += 1;
        } else {
            // Today counts as still available — there is time left in it.
            left += 1;
        }
    }
    Ok((applicable, elapsed, left))
}

/// The sentence, in Rust, because it differs by direction *and* by state and a
/// renderer deriving it would be a second implementation of the rules.
fn summarise(
    goal: &GoalRow,
    state: GoalState,
    remaining: i64,
    per_day: Option<i64>,
    left: i64,
) -> String {
    let name = &goal.subject_name;
    let days = |n: i64| if n == 1 { "day" } else { "days" };
    match (goal.direction, state) {
        (GoalDirection::AtLeast, GoalState::Met) => format!("{name}: target reached."),
        (GoalDirection::AtLeast, _) => match per_day {
            Some(p) => format!(
                "{name}: {} a day for the remaining {left} {}.",
                hm(p),
                days(left)
            ),
            None => format!("{name}: {} short, and the week is out of days.", hm(remaining)),
        },
        (GoalDirection::AtMost, GoalState::Over) => {
            format!("{name}: over by {}. The rest of the week is already spent.", hm(-remaining))
        }
        (GoalDirection::AtMost, GoalState::Met) => format!("{name}: stayed inside the budget."),
        (GoalDirection::AtMost, _) => format!(
            "{name}: {} left for the week — {} a day over {left} {}.",
            hm(remaining),
            hm(per_day.unwrap_or(0)),
            days(left)
        ),
    }
}

/// `2h 15m`, `45m`, `0m`. Matches the renderer's own duration format so the
/// sentence and the figure beside it never read differently.
pub(crate) fn hm(sec: i64) -> String {
    let sec = sec.max(0);
    let (h, m) = (sec / 3600, (sec % 3600) / 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_week_knows_which_of_its_days_the_goal_applies_to() {
        // Monday 2026-08-03 … Sunday 2026-08-09. "Today" is Wednesday.
        let mon_fri = 0b0011111;
        assert_eq!(week_shape("2026-08-03", "2026-08-05", ALL_DAYS).unwrap(), (7, 2, 5));
        assert_eq!(week_shape("2026-08-03", "2026-08-05", mon_fri).unwrap(), (5, 2, 3));
        // Sunday morning, on a Mon–Fri goal: no applicable days left, and none
        // of them is today.
        assert_eq!(week_shape("2026-08-03", "2026-08-09", mon_fri).unwrap(), (5, 5, 0));
        // Monday morning: nothing has elapsed. This is the case that must not
        // report "behind".
        assert_eq!(week_shape("2026-08-03", "2026-08-03", ALL_DAYS).unwrap(), (7, 0, 7));
    }

    #[test]
    fn durations_read_the_way_the_rest_of_the_app_writes_them() {
        assert_eq!(hm(0), "0m");
        assert_eq!(hm(45 * 60), "45m");
        assert_eq!(hm(2 * 3600 + 15 * 60), "2h 15m");
        // Never a negative duration in a sentence; the wording carries the sign.
        assert_eq!(hm(-600), "0m");
    }

    // ─── the arithmetic, end to end ────────────────────────────────────

    use crate::clock::TestClock;
    use std::sync::Arc;

    /// Monday 2026-08-03, 09:00 UTC.
    const MONDAY: i64 = 1_785_747_600_000;
    const TZ: &str = "UTC";

    fn store_on(day_offset: i64, hour: i64) -> (Store, TestClock) {
        let clock = TestClock::new(MONDAY + day_offset * 86_400_000 + (hour - 9) * 3_600_000);
        let s = Store::in_memory_with_clock(Arc::new(clock.clone())).unwrap();
        (s, clock)
    }

    fn goal(s: &mut Store, direction: GoalDirection, metric: &str, hours: i64) -> GoalRow {
        let today = local_date(s.now(), &zone(TZ).unwrap());
        s.set_goal(
            NewGoal {
                subject_kind: Some(GoalSubject::Metric),
                subject_id: metric.into(),
                period: None,
                direction: Some(direction),
                target_sec: hours * 3600,
                applies_days: None,
            },
            &today,
        )
        .unwrap()
    }

    fn progress(s: &Store) -> GoalProgress {
        let today = local_date(s.now(), &zone(TZ).unwrap());
        s.get_week_review(&today, TZ).unwrap().goals.remove(0)
    }

    /// **The one that matters most.** The same rule that stopped the month
    /// dashboard reporting a fresh August as "6% accounted" on the 4th: an app
    /// that calls the future a failure is one whose numbers you learn to
    /// discount.
    #[test]
    fn a_goal_at_zero_on_monday_morning_is_on_pace_not_behind() {
        let (mut s, _) = store_on(0, 9);
        goal(&mut s, GoalDirection::AtLeast, metric::ALL_WORK, 20);

        let p = progress(&s);
        assert_eq!(p.actual_sec, 0);
        assert_eq!(p.expected_sec, 0, "nothing has elapsed, so nothing is owed");
        assert_eq!(p.state, GoalState::OnPace);
        assert_eq!(p.days_left, 7);
    }

    /// The row that changes behaviour. "62% of target" is a fact; this is a
    /// decision you can make at breakfast.
    #[test]
    fn a_goal_says_what_the_rest_of_the_week_has_to_look_like() {
        // Thursday: three whole days elapsed, four left including today.
        let (mut s, _) = store_on(3, 9);
        goal(&mut s, GoalDirection::AtLeast, metric::ALL_WORK, 14);

        let p = progress(&s);
        assert_eq!(p.expected_sec, 6 * 3600, "3 of 7 days of a 14h target");
        assert_eq!(p.state, GoalState::Behind);
        assert_eq!(p.per_day_needed_sec, Some(14 * 3600 / 4));
        assert_eq!(
            p.summary,
            "Work: 3h 30m a day for the remaining 4 days.",
            "the sentence is built in Rust so it cannot drift from the figures"
        );
    }

    /// **Closes M11.** An entertainment budget is a goal with the direction
    /// reversed, and the wording has to reverse with it — a bar that turns red
    /// when you do the right thing teaches people to ignore bars.
    #[test]
    fn an_at_most_goal_reports_budget_left_rather_than_progress_made() {
        let (mut s, _) = store_on(3, 9);
        goal(&mut s, GoalDirection::AtMost, metric::ENTERTAINMENT, 7);

        let p = progress(&s);
        assert_eq!(p.actual_sec, 0);
        assert_eq!(
            p.delta_sec,
            3 * 3600,
            "under budget is positive in both directions"
        );
        assert_eq!(p.state, GoalState::OnPace);
        assert!(
            p.summary.starts_with("Entertainment: 7h 00m left for the week"),
            "{}",
            p.summary
        );
    }

    /// A Mon–Fri goal must not expect progress on Saturday, and must not report
    /// you behind on a Sunday morning for a week you already worked.
    #[test]
    fn a_weekday_goal_is_not_behind_at_the_weekend() {
        // Sunday.
        let (mut s, _) = store_on(6, 9);
        let today = local_date(s.now(), &zone(TZ).unwrap());
        s.set_goal(
            NewGoal {
                subject_kind: Some(GoalSubject::Metric),
                subject_id: metric::ALL_WORK.into(),
                period: None,
                direction: Some(GoalDirection::AtLeast),
                target_sec: 10 * 3600,
                applies_days: Some(0b0011111),
            },
            &today,
        )
        .unwrap();

        let p = progress(&s);
        assert_eq!(p.applicable_days, 5);
        assert_eq!(p.days_left, 0, "the working week is over");
        assert_eq!(p.per_day_needed_sec, None, "nothing left to spread it over");
        assert!(
            p.summary.contains("out of days"),
            "a goal with no days left says so rather than asking for 0h a day: {}",
            p.summary
        );
    }

    /// A goal is closed, never deleted, so a review of the week it governed
    /// still shows the number that was actually in force.
    #[test]
    fn changing_a_goal_closes_the_old_one_rather_than_erasing_it() {
        let (mut s, _) = store_on(0, 9);
        goal(&mut s, GoalDirection::AtLeast, metric::ALL_WORK, 20);
        goal(&mut s, GoalDirection::AtLeast, metric::ALL_WORK, 25);

        let live = s.get_goals(false).unwrap();
        assert_eq!(live.len(), 1, "one live goal per subject");
        assert_eq!(live[0].target_sec, 25 * 3600);

        let all = s.get_goals(true).unwrap();
        assert_eq!(all.len(), 2);
        let old = all.iter().find(|g| g.target_sec == 20 * 3600).unwrap();
        assert_eq!(old.ends_week.as_deref(), Some("2026-W32"));
    }

    #[test]
    fn a_goal_needs_a_subject_that_exists_and_a_target_above_zero() {
        let (mut s, _) = store_on(0, 9);
        let today = local_date(s.now(), &zone(TZ).unwrap());
        let attempt = |s: &mut Store, id: &str, target: i64, days: Option<i64>| {
            s.set_goal(
                NewGoal {
                    subject_kind: Some(GoalSubject::Metric),
                    subject_id: id.into(),
                period: None,
                    direction: Some(GoalDirection::AtLeast),
                    target_sec: target,
                    applies_days: days,
                },
                &today,
            )
        };
        assert!(attempt(&mut s, "notAMetric", 3600, None).is_err());
        assert!(attempt(&mut s, metric::ALL_WORK, 0, None).is_err());
        assert!(attempt(&mut s, metric::ALL_WORK, 3600, Some(0)).is_err());
        assert!(attempt(&mut s, metric::ALL_WORK, 3600, Some(128)).is_err());
    }

    /// The week and the day are the same arithmetic, by construction — the
    /// review sums `get_day`'s own totals rather than running a second query.
    #[test]
    fn the_week_totals_what_the_days_total() {
        let (s, _) = store_on(2, 12);
        let today = local_date(s.now(), &zone(TZ).unwrap());
        let review = s.get_week_review(&today, TZ).unwrap();

        let summed: i64 = review.days.iter().map(|d| d.day_sec).sum();
        assert_eq!(review.totals.day_sec, summed);
        assert_eq!(review.days.len(), 7);
        assert_eq!(review.from, "2026-08-03");
        assert_eq!(review.to, "2026-08-09");
        assert_eq!(review.week, "2026-W32");
    }

    /// The same discipline `f6` holds estimates to: trailing median, n ≥ 5, so
    /// a bad fortnight cannot move a recommendation.
    ///
    /// A week with nothing recorded is **silence, not evidence**. Without that,
    /// `aggregate_range` would hand every goal six observations on its first
    /// day and the threshold would be decoration.
    #[test]
    fn calibration_says_nothing_from_weeks_that_were_never_recorded() {
        let (mut s, _) = store_on(0, 9);
        goal(&mut s, GoalDirection::AtLeast, metric::ALL_WORK, 20);

        let today = local_date(s.now(), &zone(TZ).unwrap());
        let review = s.get_week_review(&today, TZ).unwrap();
        assert!(
            review.calibration.is_empty(),
            "a fresh install was told its goals are wrong: {:?}",
            review.calibration
        );
    }

    /// The case that matters: five recorded weeks, a goal missed in all of
    /// them, and a suggestion drawn from what actually happened.
    #[test]
    fn a_goal_missed_every_recorded_week_is_offered_a_number_that_happened() {
        let (mut s, clock) = store_on(0, 9);
        let t = s
            .create_task(NewTask {
                title: "Refactor".into(),
                ..Default::default()
            })
            .unwrap();

        // Five earlier weeks, four hours of work in each.
        for back in 1..=5i64 {
            let monday = MONDAY - back * 7 * 86_400_000;
            s.add_session(ManualSession {
                contribution: None,
                replace_existing: false,
                task_id: t.id.clone(),
                block_id: None,
                started_at: monday,
                ended_at: monday + 4 * 3_600_000,
                note: None,
            })
            .unwrap();
        }
        let _ = &clock;

        // A twenty-hour goal against four-hour weeks.
        goal(&mut s, GoalDirection::AtLeast, metric::ALL_WORK, 20);

        let today = local_date(s.now(), &zone(TZ).unwrap());
        let review = s.get_week_review(&today, TZ).unwrap();
        assert_eq!(review.calibration.len(), 1, "{:?}", review.calibration);
        let c = &review.calibration[0];
        assert_eq!(c.weeks, 5, "only the weeks that were actually recorded");
        assert_eq!(c.weeks_met, 0);
        assert_eq!(c.median_sec, 4 * 3600);
        assert_eq!(c.suggested_sec, 4 * 3600, "toward what happened, never past it");
        assert!(c.summary.contains("Try 4h 00m?"), "{}", c.summary);

        // And the suggestion is applicable as it stands. Reaching back through
        // the goal id would make the obvious mistake — passing a goal id where a
        // subject id belongs — fail at the point of use rather than at the
        // boundary.
        s.set_goal(
            NewGoal {
                subject_kind: Some(c.subject_kind),
                subject_id: c.subject_id.clone(),
                period: None,
                direction: Some(c.direction),
                target_sec: c.suggested_sec,
                applies_days: None,
            },
            &today,
        )
        .expect("a suggestion must be applicable without translation");
    }

    /// A suggestion never overshoots reality in the other direction. Raising an
    /// "at least" goal because you beat it is a different feature, and not one
    /// anybody asked for.
    #[test]
    fn a_suggestion_only_ever_moves_toward_what_happened() {
        let (mut s, _) = store_on(0, 9);
        let t = s
            .create_task(NewTask {
                title: "Refactor".into(),
                ..Default::default()
            })
            .unwrap();
        for back in 1..=5i64 {
            let monday = MONDAY - back * 7 * 86_400_000;
            s.add_session(ManualSession {
                contribution: None,
                replace_existing: false,
                task_id: t.id.clone(),
                block_id: None,
                started_at: monday,
                ended_at: monday + 10 * 3_600_000,
                note: None,
            })
            .unwrap();
        }
        // A goal comfortably met every week.
        goal(&mut s, GoalDirection::AtLeast, metric::ALL_WORK, 4);

        let today = local_date(s.now(), &zone(TZ).unwrap());
        let review = s.get_week_review(&today, TZ).unwrap();
        assert!(
            review.calibration.is_empty(),
            "a goal being met was told to change: {:?}",
            review.calibration
        );
    }

    /// A ceiling that is being kept needs no advice. Nagging about a goal that
    /// is working is how advice gets ignored.
    #[test]
    fn calibration_says_nothing_about_a_goal_that_is_working() {
        let (mut s, _) = store_on(0, 9);
        // Nothing recorded, so entertainment is zero every week — comfortably
        // inside an "at most" budget.
        goal(&mut s, GoalDirection::AtMost, metric::ENTERTAINMENT, 7);

        let today = local_date(s.now(), &zone(TZ).unwrap());
        let review = s.get_week_review(&today, TZ).unwrap();
        assert!(
            review.calibration.is_empty(),
            "a budget being kept was told to change: {:?}",
            review.calibration
        );
    }

    /// Direction of travel is the point — one week's longest stretch in
    /// isolation says nothing.
    #[test]
    fn the_review_carries_last_week_to_compare_against() {
        let (s, _) = store_on(2, 12);
        let today = local_date(s.now(), &zone(TZ).unwrap());
        let review = s.get_week_review(&today, TZ).unwrap();
        assert_eq!(review.fragmentation.fragment_threshold_sec, 15 * 60);
        // Empty, but carrying the threshold — the figure is never readable
        // without the rule that produced it, in either week.
        assert_eq!(review.previous_fragmentation.stretches, 0);
        assert_eq!(review.previous_fragmentation.fragment_threshold_sec, 15 * 60);
    }

    /// A template that opens with an invented round number is one people
    /// dismiss. Where there is no history, saying so is the honest move.
    #[test]
    fn a_template_with_no_history_asks_rather_than_guesses() {
        let (s, _) = store_on(0, 9);
        let today = local_date(s.now(), &zone(TZ).unwrap());
        let templates = s.get_goal_templates(&today, TZ).unwrap();

        let shorter = templates.iter().find(|t| t.key == "shorterWeek").unwrap();
        assert_eq!(shorter.target_sec, None, "a fresh install has no median");
        assert!(shorter.rationale.contains("Not enough weeks"), "{}", shorter.rationale);
        assert_eq!(shorter.direction, GoalDirection::AtMost);
        assert_eq!(shorter.applies_days, 0b0011111, "a work ceiling is weekdays");

        // Sleep is the one number not taken from your weeks, and it says so.
        let sleep = templates.iter().find(|t| t.key == "sleep").unwrap();
        assert_eq!(sleep.target_sec, Some(8 * 3600 * 7));
        assert!(sleep.rationale.contains("not drawn from your own history"));
    }

    #[test]
    fn iso_weeks_sort_as_strings() {
        assert_eq!(iso_week("2026-08-05").unwrap(), "2026-W32");
        assert!(iso_week("2026-01-05").unwrap() < iso_week("2026-08-05").unwrap());
        // The week a year straddles belongs to whichever year owns it in ISO.
        assert_eq!(iso_week("2026-12-31").unwrap(), "2026-W53");
    }
}

/// Goal templates (W10) — the numbers come from your own weeks.
///
/// The governing constraint on this whole plan is that configuring the tool must
/// not become the work. A blank goal form is exactly that failure: it asks for a
/// number nobody has, and the number people invent is a round one they do not
/// believe.
///
/// So each template looks at the trailing weeks and proposes something reachable
/// — and where there is not enough history it says so rather than guessing. The
/// same n ≥ 5 discipline as everything else here.
impl Store {
    pub fn get_goal_templates(&self, today: &str, tz: &str) -> Result<Vec<GoalTemplate>> {
        const WEEKS: i64 = 4;
        const MIN_SAMPLES: usize = 2;
        const WEEKDAYS: i64 = 0b0011111;

        let monday = week_start(parse_date(today)?);
        // Completed weeks only. The week in progress would drag every median
        // down by however much of it is left.
        let mut work: Vec<i64> = Vec::new();
        let mut entertainment: Vec<i64> = Vec::new();
        let mut sleep: Vec<i64> = Vec::new();
        for back in 1..=WEEKS {
            let start = monday - chrono::Duration::days(7 * back);
            let range = self.aggregate_range(
                &format_date(start),
                &format_date(start + chrono::Duration::days(6)),
                tz,
            )?;
            work.push(range.totals.confirmed_work_sec);
            entertainment.push(range.totals.entertainment_sec);
            sleep.push(range.totals.sleep_sec);
        }

        let median = |v: &[i64]| -> Option<i64> {
            let mut s: Vec<i64> = v.iter().copied().filter(|n| *n > 0).collect();
            if s.len() < MIN_SAMPLES {
                return None;
            }
            s.sort_unstable();
            Some(s[s.len() / 2])
        };

        let mut out = vec![
            // Rize's "6-hour work day", and the template its reviewer actually
            // chose. A ceiling, because his goal was a 30-hour week.
            template(
                "shorterWeek",
                "The shorter week",
                metric::ALL_WORK,
                GoalDirection::AtMost,
                median(&work).map(|m| (m as f64 * 0.9) as i64),
                WEEKDAYS,
                median(&work),
                "10% under your median week",
            ),
            template(
                "cutEntertainment",
                "Cut entertainment",
                metric::ENTERTAINMENT,
                GoalDirection::AtMost,
                median(&entertainment).map(|m| (m as f64 * 0.8) as i64),
                ALL_DAYS,
                median(&entertainment),
                "20% under your median week",
            ),
            template(
                "sleep",
                "Sleep",
                metric::SLEEP,
                GoalDirection::AtLeast,
                Some(8 * 3600 * 7),
                ALL_DAYS,
                // No basis, because there is none: this is the one number not
                // taken from your weeks, and appending "(56h median)" would
                // dress a constant up as a measurement.
                None,
                "8 hours a night — the only template not drawn from your own history",
            ),
        ];

        // "Off zero": a life area with a monthly target and no time at all is
        // already the most actionable row on the month dashboard.
        for area in self.get_life_areas(tz, false)? {
            let Some(monthly) = area.monthly_target_sec else {
                continue;
            };
            if area.month_tracked_sec > 0 {
                continue;
            }
            out.push(GoalTemplate {
                key: format!("offZero:{}", area.id),
                name: format!("{} — off zero", area.name),
                subject_kind: GoalSubject::LifeArea,
                subject_id: area.id.clone(),
                direction: GoalDirection::AtLeast,
                // A quarter of the monthly target, so a week is a step toward it
                // rather than a restatement of it.
                target_sec: Some((monthly / 4).max(1800)),
                applies_days: ALL_DAYS,
                rationale: format!(
                    "A quarter of its {} monthly target. It has had none this month.",
                    hm(monthly)
                ),
            });
        }
        Ok(out)
    }
}

fn template(
    key: &str,
    name: &str,
    metric_id: &str,
    direction: GoalDirection,
    target_sec: Option<i64>,
    applies_days: i64,
    basis: Option<i64>,
    how: &str,
) -> GoalTemplate {
    GoalTemplate {
        key: key.into(),
        name: name.into(),
        subject_kind: GoalSubject::Metric,
        subject_id: metric_id.into(),
        direction,
        target_sec,
        applies_days,
        rationale: match (target_sec, basis) {
            (Some(_), Some(b)) => format!("{how} ({} median).", hm(b)),
            (Some(_), None) => format!("{how}."),
            // Stated, not hidden. A template with no number is still worth
            // showing — it tells you the app is not guessing.
            (None, _) => "Not enough weeks recorded yet to pick a number. Set one yourself.".into(),
        },
    }
}
