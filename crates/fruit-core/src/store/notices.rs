//! Notices (PLAN-WEEKLY-GOALS.md W4/W5).
//!
//! Three things Fruit will tell you about, all off by default, all one Settings
//! group. Bundled deliberately: the source review's sharpest criticism of this
//! class of tool is that configuring it becomes the work, so three switches
//! under one heading rather than three sections.
//!
//! # A notice, never a nag and never a block
//!
//! It does not interrupt a timer, it does not dim the screen, and crossing a
//! threshold twice produces one notice. An app that talks too much gets muted,
//! and a muted app cannot tell you the thing that mattered.
//!
//! Firing is recorded here rather than in the shell, so "once per crossing" is a
//! property with a test rather than a hope about a loop.
//!
//! # What does *not* count toward a break
//!
//! The good idea in Rize's break reminders is what they exclude: only focused
//! work counts. In Fruit that is not a rule to remember, it is the schema —
//!
//! - `life_entry` has no accumulator and never counts toward a work streak;
//! - `activity_span` is observed, not confirmed, and never counts either;
//! - `time_session` counts, **except** where `contribution = 'attend'`. Sitting
//!   in a two-hour review is not two hours heads-down, and Fruit already records
//!   the difference.
//!
//! # Why the off-plan notice is not blocking
//!
//! `PRODUCT-SPEC.md` §1 puts blocking out of scope, and the connector ships with
//! no `host_permissions` — it cannot touch a page, and that absence *is* the
//! privacy argument. A notice needs no such permission.
//!
//! It is also *better placed* than a blocker. Rize must guess what counts as a
//! distraction from a category alone; Fruit knows **what you plotted for this
//! hour**. "Entertainment during plotted work" is a far sharper signal than
//! "entertainment", and it produces fewer of the false positives that make a
//! nudge something people learn to dismiss. Two rules follow from that, and both
//! come from the reviewer's own caveat:
//!
//! - **Never during unplotted time.** If you did not plan the hour, Fruit has no
//!   standing to have an opinion about it — which removes most false positives,
//!   because his were all on time nobody had claimed.
//! - **Silenceable for the session**, exactly as Rize does it.

use rusqlite::params;
use serde_json::Value;

use super::Store;
use crate::error::Result;
use crate::model::*;
use crate::time::{day_start, local_date, parse_date, zone, Millis};

/// Minutes of unbroken confirmed work before Fruit mentions it. 0 is off.
pub const CONTINUOUS_SEC: &str = "notices.continuousWorkSec";
/// A day's work ceiling. 0 is off. Rize's overwork notification.
pub const CEILING_SEC: &str = "notices.dailyCeilingSec";
/// Whether to mention entertainment observed during a plotted block.
pub const OFF_PLAN: &str = "notices.offPlan";
/// Which notice keys have already fired. One row, a JSON array.
const FIRED: &str = "notices.fired";
/// Silenced until this instant — the reviewer's "don't warn me again this
/// session", which is what keeps a nudge from becoming something people learn
/// to dismiss.
const SILENT_UNTIL: &str = "notices.silentUntil";

/// A gap this long ends a run of work. Shorter than a lunch break, longer than
/// standing up — a stretch is not broken by fetching coffee.
const BREAK_GAP_MS: i64 = 10 * 60_000;

impl Store {
    fn setting_sec(&self, key: &str) -> i64 {
        match self.get_setting(key) {
            Ok(Some(Value::Number(n))) => n.as_i64().unwrap_or(0),
            _ => 0,
        }
        .max(0)
    }

    fn fired(&self) -> Vec<String> {
        match self.get_setting(FIRED) {
            Ok(Some(Value::Array(v))) => v
                .into_iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Silences every notice for a while. The reviewer's own caveat: his false
    /// positives were real and common, and a nudge you cannot quiet becomes a
    /// nudge you learn to ignore.
    pub fn silence_notices(&mut self, minutes: i64) -> Result<()> {
        let until = self.now() + minutes.clamp(1, 24 * 60) * 60_000;
        self.set_setting(SILENT_UNTIL, &Value::from(until))
    }

    fn silenced(&self) -> bool {
        matches!(self.get_setting(SILENT_UNTIL), Ok(Some(Value::Number(n)))
            if n.as_i64().unwrap_or(0) > self.now())
    }

    /// The current unbroken run of **confirmed, heads-down** work, in seconds.
    ///
    /// Meetings do not count: a `contribution = 'attend'` session ends the run
    /// rather than extending it, because two hours in a review is not two hours
    /// heads-down and the schema already knows which is which.
    pub fn continuous_work_sec(&self) -> Result<i64> {
        let now = self.now();
        let mut stmt = self.conn.prepare(
            "SELECT started_at, COALESCE(ended_at, ?1), contribution
               FROM time_session
              WHERE started_at > ?2
              ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map(params![now, now - 24 * 3600 * 1000], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;

        let mut run_from: Option<Millis> = None;
        let mut edge = now;
        for row in rows {
            let (started, ended, contribution) = row?;
            // A meeting is not heads-down time, so it ends the run rather than
            // extending it. This is the whole of Rize's "meetings don't count"
            // rule, expressed in a column that already existed.
            if contribution.as_deref() == Some("attend") {
                break;
            }
            if ended < edge - BREAK_GAP_MS {
                break;
            }
            run_from = Some(started);
            edge = started;
        }
        Ok(run_from.map(|from| (now - from) / 1000).unwrap_or(0))
    }

    /// Notices that are due now, marking each as fired so it cannot repeat.
    ///
    /// Returns them rather than delivering them: the shell owns notification,
    /// this owns *whether there is anything to say*. That split is what makes
    /// "once per crossing" testable.
    pub fn due_notices(&mut self, tz: &str) -> Result<Vec<Notice>> {
        if self.silenced() {
            return Ok(Vec::new());
        }
        let zone_ = zone(tz)?;
        let now = self.now();
        let today = local_date(now, &zone_);
        let mut fired = self.fired();
        let mut out = Vec::new();

        let mut claim = |key: String, kind: NoticeKind, title: String, body: String| {
            if fired.contains(&key) {
                return;
            }
            fired.push(key.clone());
            out.push(Notice {
                key,
                kind,
                title,
                body,
            });
        };

        // ── continuous work ────────────────────────────────────────────
        let threshold = self.setting_sec(CONTINUOUS_SEC);
        if threshold > 0 {
            let run = self.continuous_work_sec()?;
            if run >= threshold {
                // Keyed by how many multiples have passed, so a long stretch
                // says something at two hours and again at four — and never
                // twice at the same crossing.
                let step = run / threshold;
                claim(
                    format!("continuous:{today}:{step}"),
                    NoticeKind::ContinuousWork,
                    format!("{} without a break", fmt_hm(run)),
                    "Meetings and personal time are not counted toward this.".into(),
                );
            }
        }

        // ── the day's ceiling ──────────────────────────────────────────
        let ceiling = self.setting_sec(CEILING_SEC);
        if ceiling > 0 {
            let worked = self.get_day(&today, tz, None)?.totals.confirmed_work_sec;
            if worked >= ceiling {
                claim(
                    format!("ceiling:{today}"),
                    NoticeKind::DailyCeiling,
                    format!("{} of work today", fmt_hm(worked)),
                    format!("Past the {} you set for a day.", fmt_hm(ceiling)),
                );
            }
        }

        // ── off plan ───────────────────────────────────────────────────
        if matches!(self.get_setting(OFF_PLAN), Ok(Some(Value::Bool(true)))) {
            if let Some((what, plotted)) = self.off_plan_now(&today, tz)? {
                // Keyed by the block, so one wander produces one notice and
                // returning to the same block later can produce another.
                claim(
                    format!("offPlan:{today}:{plotted}:{what}"),
                    NoticeKind::OffPlan,
                    format!("{what} during {plotted}"),
                    "You plotted this hour for something else.".into(),
                );
            }
        }

        self.set_setting(FIRED, &Value::from(fired))?;
        Ok(out)
    }

    /// `(what you are on, what you plotted)` when entertainment is being
    /// observed inside a plotted block, right now.
    ///
    /// `None` during unplotted time, always: if you did not plan the hour, Fruit
    /// has no standing to have an opinion about it.
    fn off_plan_now(&self, today: &str, tz: &str) -> Result<Option<(String, String)>> {
        let now = self.now();
        let zone_ = zone(tz)?;
        let start = day_start(parse_date(today)?, &zone_);

        let plotted: Option<String> = self
            .conn
            .query_row(
                "SELECT COALESCE(t.title, b.label, 'a plotted block')
                   FROM scheduled_block b
                   LEFT JOIN task t ON t.id = b.task_id
                  WHERE b.deleted_at IS NULL AND b.local_date = ?1
                    AND b.starts_at <= ?2 AND b.starts_at + b.duration_sec * 1000 > ?2
                  LIMIT 1",
                params![today, now],
                |r| r.get(0),
            )
            .ok();
        let Some(plotted) = plotted else {
            return Ok(None);
        };

        // What is on screen right now, by the label recorded when it was
        // written. `counts_as` rather than the category's name, so a user's own
        // "Doomscrolling" fires this too.
        let observed: Option<(Option<String>, String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT s.domain, s.app_id, c.counts_as
                   FROM activity_span s
                   LEFT JOIN observation_category c ON c.id = s.category_id
                  WHERE s.ended_at >= ?1 AND s.started_at <= ?2 AND s.is_idle = 0
                  ORDER BY s.ended_at DESC LIMIT 1",
                params![now - BREAK_GAP_MS, now],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        let _ = start;

        Ok(match observed {
            Some((domain, app, Some(counts))) if counts == "entertainment" => {
                Some((domain.unwrap_or(app), plotted))
            }
            _ => None,
        })
    }
}

fn fmt_hm(sec: i64) -> String {
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
    use crate::clock::TestClock;
    use serde_json::json;
    use std::sync::Arc;

    /// 2026-08-04, 09:00 UTC.
    const NINE: i64 = 1_785_834_000_000;
    const TZ: &str = "UTC";
    const DATE: &str = "2026-08-04";

    fn store() -> (Store, TestClock) {
        let clock = TestClock::new(NINE);
        let s = Store::in_memory_with_clock(Arc::new(clock.clone())).unwrap();
        (s, clock)
    }

    fn task(s: &mut Store, title: &str) -> String {
        s.create_task(NewTask {
            title: title.into(),
            ..Default::default()
        })
        .unwrap()
        .id
    }

    fn session(s: &mut Store, task_id: &str, from: i64, minutes: i64) -> String {
        s.add_session(ManualSession {
            task_id: task_id.into(),
            block_id: None,
            started_at: from,
            ended_at: from + minutes * 60_000,
            note: None,
        })
        .unwrap()
        .id
    }

    /// **The rule worth having.** Sitting in a two-hour review is not two hours
    /// heads-down, and the schema already records the difference.
    #[test]
    fn a_meeting_does_not_count_toward_going_without_a_break() {
        let (mut s, clock) = store();
        let t = task(&mut s, "Refactor");

        session(&mut s, &t, NINE, 60);
        clock.advance(60 * 60_000);
        assert_eq!(s.continuous_work_sec().unwrap(), 3600);

        // An hour of meeting, then an hour of work. The run is one hour, not
        // three: the meeting ended it.
        let meeting = session(&mut s, &t, NINE + 60 * 60_000, 60);
        s.set_session_contribution(&meeting, Some(Contribution::Attend))
            .unwrap();
        session(&mut s, &t, NINE + 120 * 60_000, 60);
        clock.advance(120 * 60_000);

        assert_eq!(
            s.continuous_work_sec().unwrap(),
            3600,
            "the meeting was counted as heads-down time"
        );
    }

    #[test]
    fn a_real_break_ends_the_run_and_a_coffee_does_not() {
        let (mut s, clock) = store();
        let t = task(&mut s, "Refactor");

        // Two hours with a five-minute gap in the middle.
        session(&mut s, &t, NINE, 60);
        session(&mut s, &t, NINE + 65 * 60_000, 55);
        clock.advance(120 * 60_000);
        assert_eq!(s.continuous_work_sec().unwrap(), 2 * 3600);

        // A twenty-minute gap is a break.
        let (mut s, clock) = store();
        let t = task(&mut s, "Refactor");
        session(&mut s, &t, NINE, 60);
        session(&mut s, &t, NINE + 80 * 60_000, 40);
        clock.advance(120 * 60_000);
        assert_eq!(s.continuous_work_sec().unwrap(), 40 * 60);
    }

    /// Off by default. Every notice is, and this is the switch that proves it.
    #[test]
    fn nothing_is_said_until_a_threshold_is_set() {
        let (mut s, clock) = store();
        let t = task(&mut s, "Refactor");
        session(&mut s, &t, NINE, 240);
        clock.advance(240 * 60_000);

        assert!(s.due_notices(TZ).unwrap().is_empty());
    }

    /// A notice, never a nag: crossing a threshold produces exactly one.
    #[test]
    fn a_threshold_is_mentioned_once_per_crossing() {
        let (mut s, clock) = store();
        s.set_setting(CONTINUOUS_SEC, &json!(2 * 3600)).unwrap();
        let t = task(&mut s, "Refactor");
        session(&mut s, &t, NINE, 150);
        clock.advance(150 * 60_000);

        let first = s.due_notices(TZ).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].kind, NoticeKind::ContinuousWork);
        assert!(first[0].title.starts_with("2h 30m"), "{}", first[0].title);
        assert!(s.due_notices(TZ).unwrap().is_empty(), "it said it twice");

        // Four hours is a second crossing, and worth saying again.
        s.add_session(ManualSession {
            task_id: t.clone(),
            block_id: None,
            started_at: NINE + 150 * 60_000,
            ended_at: NINE + 250 * 60_000,
            note: None,
        })
        .unwrap();
        clock.advance(100 * 60_000);
        assert_eq!(s.due_notices(TZ).unwrap().len(), 1);
    }

    #[test]
    fn the_days_ceiling_is_mentioned_once() {
        let (mut s, clock) = store();
        s.set_setting(CEILING_SEC, &json!(6 * 3600)).unwrap();
        let t = task(&mut s, "Refactor");
        session(&mut s, &t, NINE, 400);
        clock.advance(400 * 60_000);

        let notices = s.due_notices(TZ).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, NoticeKind::DailyCeiling);
        assert!(notices[0].body.contains("6h 00m"), "{}", notices[0].body);
        assert!(s.due_notices(TZ).unwrap().is_empty());
    }

    /// **Never during unplotted time.** If you did not plan the hour, Fruit has
    /// no standing to have an opinion about it — and that removes most of the
    /// false positives a category-only blocker produces.
    #[test]
    fn the_off_plan_nudge_says_nothing_about_an_hour_you_did_not_plan() {
        let (mut s, clock) = store();
        s.set_setting(OFF_PLAN, &json!(true)).unwrap();
        s.set_activity_setting(super::super::activity::ENABLED, json!(true))
            .unwrap();
        s.set_activity_setting("activity.domainsEnabled", json!(true))
            .unwrap();

        for _ in 0..30 {
            s.record_activity(ActivitySample {
                app_id: "chrome.exe".into(),
                window_title: None,
                domain: Some("youtube.com".into()),
                at: clock.now(),
            })
            .unwrap();
            clock.advance(super::super::activity::SAMPLE_INTERVAL_MS);
        }

        assert!(
            s.due_notices(TZ).unwrap().is_empty(),
            "nothing was plotted, so there was nothing to be off"
        );

        // Plot the hour, and the same observation now has something to be
        // measured against.
        let refactor = task(&mut s, "Refactor auth");
        s.schedule_block(NewBlock {
            task_id: Some(refactor),
            starts_at: NINE,
            duration_sec: 3600,
            tz: TZ.into(),
            ..Default::default()
        })
        .unwrap();

        let notices = s.due_notices(TZ).unwrap();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert_eq!(notices[0].kind, NoticeKind::OffPlan);
        assert!(notices[0].title.contains("youtube.com"), "{}", notices[0].title);
        assert!(notices[0].title.contains("Refactor auth"), "{}", notices[0].title);
    }

    /// The reviewer's own caveat: his false positives were real and common, and
    /// a nudge you cannot quiet becomes a nudge you learn to ignore.
    #[test]
    fn everything_can_be_silenced_for_a_while() {
        let (mut s, clock) = store();
        s.set_setting(CEILING_SEC, &json!(1)).unwrap();
        let t = task(&mut s, "Refactor");
        session(&mut s, &t, NINE, 60);
        clock.advance(60 * 60_000);

        s.silence_notices(30).unwrap();
        assert!(s.due_notices(TZ).unwrap().is_empty());
        clock.advance(31 * 60_000);
        assert_eq!(s.due_notices(TZ).unwrap().len(), 1, "silence outlived itself");
    }
}
