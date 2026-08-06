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
        applies_days: r.get(5)?,
        starts_week: r.get(6)?,
        ends_week: r.get(7)?,
    })
}

const GOAL_COLS: &str =
    "id, subject_kind, subject_id, direction, target_sec, applies_days, starts_week, ends_week";

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

        let week = iso_week(today)?;
        let now = self.now();
        let tx = self.conn.transaction()?;
        // Close the outgoing goal at the week the new one starts. Same shape as
        // every other "the record keeps what it was told" rule here.
        tx.execute(
            "UPDATE goal SET ends_week = ?3, updated_at = ?4
              WHERE subject_kind = ?1 AND subject_id = ?2 AND ends_week IS NULL",
            params![kind.as_str(), input.subject_id, week, now],
        )?;
        let id = new_id();
        tx.execute(
            "INSERT INTO goal
               (id, subject_kind, subject_id, direction, target_sec, applies_days,
                starts_week, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
            params![
                id,
                kind.as_str(),
                input.subject_id,
                direction.as_str(),
                input.target_sec,
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

        Ok(WeekReview {
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
        let expected_sec = if applicable > 0 {
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
fn hm(sec: i64) -> String {
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
        let (mut s, _) = store_on(2, 12);
        let today = local_date(s.now(), &zone(TZ).unwrap());
        let review = s.get_week_review(&today, TZ).unwrap();

        let summed: i64 = review.days.iter().map(|d| d.day_sec).sum();
        assert_eq!(review.totals.day_sec, summed);
        assert_eq!(review.days.len(), 7);
        assert_eq!(review.from, "2026-08-03");
        assert_eq!(review.to, "2026-08-09");
        assert_eq!(review.week, "2026-W32");
    }

    #[test]
    fn iso_weeks_sort_as_strings() {
        assert_eq!(iso_week("2026-08-05").unwrap(), "2026-W32");
        assert!(iso_week("2026-01-05").unwrap() < iso_week("2026-08-05").unwrap());
        // The week a year straddles belongs to whichever year owns it in ISO.
        assert_eq!(iso_week("2026-12-31").unwrap(), "2026-W53");
    }
}
