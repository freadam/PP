//! The unified day (Plan Rev 3 §7, §8.1) — four record types, one timeline.
//!
//! The product stores four different kinds of claim about the same hour, and
//! they are not interchangeable:
//!
//! | Table | The claim | Made by |
//! |---|---|---|
//! | `scheduled_block` | "I mean to do this" | the user, in advance |
//! | `time_session` | "I did this work" | the timer, or a correction |
//! | `life_entry` | "I did this non-work thing" | the user |
//! | `activity_span` | "this app was in front" | the machine |
//!
//! The tempting design is one table with a `kind` column. It fails immediately:
//! if the timer says the auth refactor ran 09:00–10:00 and the observer says
//! Slack was frontmost 09:20–09:40, one table forces a choice between
//! overwriting a fact and counting eighty minutes in a sixty-minute hour.
//!
//! So the tables stay separate and **overlap resolves on read**:
//!
//!   1. confirmed `life_entry`
//!   2. confirmed `time_session`
//!   3. observed `activity_span`
//!   4. empty
//!
//! `resolve_day` cuts the day at every boundary any source introduces and gives
//! each resulting segment **exactly one** owner. Totals sum segments, not rows,
//! which is why a ten-minute session inside a thirty-minute slot contributes
//! ten minutes. The slot grid is a lens for the eye; the segments are the
//! arithmetic.
//!
//! The plan is deliberately absent from that list. An intention that silently
//! becomes actual time is how a planner starts lying to you, so a block renders
//! as a separate overlay and the gap between the layers is the drift.

use std::collections::{BTreeSet, HashMap};

use rusqlite::params;

use super::Store;
use crate::error::{AppError, Result};
use crate::model::*;
use crate::store::week::drift_state;
use crate::time::{day_end, day_start, local_date, parse_date, zone, Millis};

/// Slot sizes the Day view offers. A lens over the same stored precision —
/// changing it never rounds a record (§8.1).
pub const SLOT_CHOICES: &[i64] = &[5, 15, 30, 60];
pub const DEFAULT_SLOT_MINUTES: i64 = 30;

/// One source interval, before precedence is applied.
pub struct Claim {
    pub from: Millis,
    pub to: Millis,
    pub owner: SlotOwner,
}

/// Re-exported so tests and future callers can drive the resolver without a
/// database — it is a pure function over intervals and deserves to be reachable
/// as one.
pub type Segment = DaySegment;
pub type SegmentOwner = SlotOwner;

impl Store {
    /// The Day view (§8.1): the primary operational screen.
    pub fn get_day(&self, date: &str, tz: &str, slot_minutes: Option<i64>) -> Result<DayView> {
        let zone_ = zone(tz)?;
        let day = parse_date(date)?;
        let (from, to) = (day_start(day, &zone_), day_end(day, &zone_));
        let slot_minutes = slot_minutes.unwrap_or(DEFAULT_SLOT_MINUTES);
        if !SLOT_CHOICES.contains(&slot_minutes) {
            return Err(AppError::invalid(format!(
                "{slot_minutes}-minute slots aren't one of the choices (5, 15, 30, 60)."
            )));
        }

        let claims = self.collect_claims(from, to)?;
        let spans = self.observed_spans(from, to)?;
        let segments = resolve_day(from, to, claims, &spans);
        let plans = self.day_plans(date, &zone_)?;

        let totals = self.totals(from, to, &segments, &spans, &plans, tz)?;
        let slots = build_slots(from, to, slot_minutes, &segments, &plans);
        let now = self.now();

        Ok(DayView {
            fragmentation: fragmentation(&segments, &plans, self.fragment_threshold_sec()),
            local_date: date.to_string(),
            tz: tz.to_string(),
            slot_minutes,
            starts_at: from,
            ends_at: to,
            is_reconciled: !self.reconciled_dates(&[date.to_string()])?.is_empty(),
            is_today: local_date(now, &zone_) == date,
            slots,
            segments,
            totals,
            now,
        })
    }

    /// Everything that claims an interval of the day, in no particular order —
    /// `resolve_day` sorts out who wins where.
    fn collect_claims(&self, from: Millis, to: Millis) -> Result<Vec<Claim>> {
        let mut claims = Vec::new();

        // 1. Confirmed life time.
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.life_area_id, a.name, a.colour, a.kind, e.label, e.is_private,
                    e.started_at, e.ended_at
               FROM life_entry e
               JOIN life_area a ON a.id = e.life_area_id
              WHERE e.deleted_at IS NULL AND e.started_at < ?2 AND e.ended_at > ?1",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            let kind: String = r.get(4)?;
            Ok(Claim {
                from: r.get::<_, i64>(7)?,
                to: r.get::<_, i64>(8)?,
                owner: SlotOwner::Life {
                    entry_id: r.get(0)?,
                    area_id: r.get(1)?,
                    area_name: r.get(2)?,
                    area_colour: r.get(3)?,
                    area_kind: AreaKind::parse(&kind).unwrap_or(AreaKind::Other),
                    label: r.get(5)?,
                    is_private: r.get::<_, i64>(6)? == 1,
                },
            })
        })?;
        for row in rows {
            claims.push(row?);
        }
        drop(stmt);

        // 2. Confirmed work. An open session is claimed only up to now: the
        //    future is not tracked time just because a timer is running.
        let now = self.now();
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.task_id, t.title, t.project_id, p.colour, s.contribution,
                    s.started_at, COALESCE(s.ended_at, ?3), p.name, s.source
               FROM time_session s
               JOIN task t         ON t.id = s.task_id
               LEFT JOIN project p ON p.id = t.project_id
              WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?3) > ?1",
        )?;
        let rows = stmt.query_map(params![from, to, now], |r| {
            let contribution: Option<String> = r.get(5)?;
            Ok(Claim {
                from: r.get::<_, i64>(6)?,
                to: r.get::<_, i64>(7)?,
                owner: SlotOwner::Work {
                    session_id: r.get(0)?,
                    task_id: r.get(1)?,
                    task_title: r.get(2)?,
                    project_id: r.get(3)?,
                    project_name: r.get(8)?,
                    project_colour: r.get(4)?,
                    contribution: contribution.as_deref().and_then(Contribution::parse),
                    source: r.get(9)?,
                },
            })
        })?;
        for row in rows {
            claims.push(row?);
        }
        drop(stmt);

        // 3. Observation.
        for span in self.observed_spans(from, to)? {
            claims.push(Claim {
                from: span.started_at,
                to: span.ended_at,
                owner: if span.is_idle {
                    SlotOwner::Idle
                } else {
                    SlotOwner::Observed {
                        app_id: span.app_id.clone(),
                        domain: span.domain.clone(),
                        category: span.category.clone(),
                    }
                },
            });
        }

        Ok(claims)
    }

    /// The one place observation is read. The Day view, the Activity screen,
    /// the category totals and the uncategorised list all come through here, so
    /// the short-span floor cannot apply to some of them and not others.
    pub(crate) fn observed_spans(&self, from: Millis, to: Millis) -> Result<Vec<ObservedSpan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, app_id, window_title, domain, category,
                    category_id, is_idle
               FROM activity_span
              WHERE started_at < ?2 AND ended_at > ?1
              ORDER BY started_at",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok(ObservedSpan {
                id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                app_id: r.get(3)?,
                window_title: r.get(4)?,
                domain: r.get(5)?,
                category: r.get(6)?,
                category_id: r.get(7)?,
                is_idle: r.get::<_, i64>(8)? == 1,
            })
        })?;
        let spans: Vec<ObservedSpan> = rows.collect::<std::result::Result<_, _>>()?;
        // Order matters: remove the double-count first, then apply the floor —
        // so a two-second remainder left by subtraction is dropped as the noise
        // it is rather than surviving into a total.
        Ok(apply_min_span(
            dedupe_browser_overlap(spans),
            self.min_span_sec(),
        ))
    }

    /// The same, for one local date. Clipped to the day so a span running over
    /// midnight is counted in each day it actually occupied.
    pub(crate) fn labelled_spans(&self, date: &str, tz: &str) -> Result<Vec<ObservedSpan>> {
        let zone_ = zone(tz)?;
        let day = parse_date(date)?;
        let (from, to) = (day_start(day, &zone_), day_end(day, &zone_));
        Ok(self
            .observed_spans(from, to)?
            .into_iter()
            .map(|mut s| {
                s.started_at = s.started_at.max(from);
                s.ended_at = s.ended_at.min(to);
                s
            })
            .filter(|s| s.ended_at > s.started_at)
            .collect())
    }

    /// The plan overlay — never part of the precedence order.
    fn day_plans(&self, date: &str, _zone: &chrono_tz::Tz) -> Result<Vec<DayPlan>> {
        let today = local_date(self.now(), _zone);
        let is_past = date < today.as_str();
        let mut stmt = self.conn.prepare(
            "SELECT b.id, COALESCE(t.title, b.label, 'Untitled'), p.colour,
                    b.starts_at, b.duration_sec, COALESCE(c.tracked_sec, 0),
                    b.is_fixed, b.series_id, b.intent, b.task_id
               FROM scheduled_block b
               LEFT JOIN task t    ON t.id = b.task_id
               LEFT JOIN project p ON p.id = t.project_id
               LEFT JOIN block_tracked_cache c ON c.block_id = b.id
              WHERE b.local_date = ?1 AND b.deleted_at IS NULL
              ORDER BY b.starts_at",
        )?;
        let rows = stmt.query_map([date], |r| {
            let duration_sec: i64 = r.get(4)?;
            let tracked_sec: i64 = r.get(5)?;
            Ok(DayPlan {
                block_id: r.get(0)?,
                task_id: r.get(9)?,
                title: r.get(1)?,
                project_colour: r.get(2)?,
                starts_at: r.get(3)?,
                duration_sec,
                tracked_sec,
                drift_sec: tracked_sec - duration_sec,
                drift_state: drift_state(tracked_sec, duration_sec, is_past),
                is_fixed: r.get::<_, i64>(6)? == 1,
                series_id: r.get(7)?,
                intent: BlockIntent::parse(&r.get::<_, String>(8)?).unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    fn totals(
        &self,
        from: Millis,
        to: Millis,
        segments: &[DaySegment],
        spans: &[ObservedSpan],
        plans: &[DayPlan],
        tz: &str,
    ) -> Result<DayTotals> {
        let mut t = DayTotals {
            day_sec: (to - from) / 1000,
            planned_sec: plans.iter().map(|p| p.duration_sec).sum(),
            planned_entertainment_sec: plans
                .iter()
                .filter(|p| p.intent == BlockIntent::Entertainment)
                .map(|p| p.duration_sec)
                .sum(),
            confirmed_work_sec: 0,
            confirmed_life_sec: 0,
            sleep_sec: 0,
            private_sec: 0,
            observed_only_sec: 0,
            idle_sec: 0,
            empty_sec: 0,
            entertainment_sec: 0,
            entertainment_in_window_sec: 0,
            // Deliberately overlaps the layers above: "how much of this day was
            // at the PC" is a different question from "how was it spent".
            pc_sec: merged_seconds(spans.iter().filter(|s| !s.is_idle).map(|s| (s.started_at, s.ended_at))),
            by_area: Vec::new(),
            by_project: Vec::new(),
            by_app: Vec::new(),
            by_contribution: Vec::new(),
            by_domain: Vec::new(),
        };

        let mut areas: HashMap<String, (String, String, AreaKind, i64)> = HashMap::new();
        let mut projects: HashMap<Option<String>, (String, Option<String>, i64)> = HashMap::new();
        // Where entertainment actually happened, so it can be checked against
        // where it was *meant* to happen.
        let mut ent_intervals: Vec<(Millis, Millis)> = Vec::new();
        // Work by involvement. Keyed on the option itself, so "no mode
        // recorded" is a row rather than a silently dropped one — §5.8 makes
        // that distinct from non-work time, which has no such field at all.
        let mut contributions: HashMap<Option<Contribution>, i64> = HashMap::new();

        for seg in segments {
            let sec = (seg.to - seg.from) / 1000;
            match &seg.owner {
                SlotOwner::Life {
                    area_id,
                    area_name,
                    area_colour,
                    area_kind,
                    is_private,
                    ..
                } => {
                    if *is_private {
                        t.private_sec += sec;
                    } else {
                        t.confirmed_life_sec += sec;
                        if *area_kind == AreaKind::Rest {
                            t.sleep_sec += sec;
                        }
                    }
                    if *area_kind == AreaKind::Entertainment {
                        t.entertainment_sec += sec;
                        ent_intervals.push((seg.from, seg.to));
                    }
                    let e = areas.entry(area_id.clone()).or_insert((
                        area_name.clone(),
                        area_colour.clone(),
                        *area_kind,
                        0,
                    ));
                    e.3 += sec;
                }
                SlotOwner::Work {
                    project_id,
                    project_colour,
                    contribution,
                    ..
                } => {
                    t.confirmed_work_sec += sec;
                    *contributions.entry(*contribution).or_insert(0) += sec;
                    let name = match project_id {
                        Some(id) => self
                            .conn
                            .query_row("SELECT name FROM project WHERE id = ?1", [id], |r| r.get(0))
                            .unwrap_or_else(|_| "Unknown".to_string()),
                        None => "No project".to_string(),
                    };
                    let e = projects
                        .entry(project_id.clone())
                        .or_insert((name, project_colour.clone(), 0));
                    e.2 += sec;
                }
                SlotOwner::Observed { category, .. } => {
                    t.observed_only_sec += sec;
                    if category.as_deref() == Some("entertainment") {
                        t.entertainment_sec += sec;
                        ent_intervals.push((seg.from, seg.to));
                    }
                }
                SlotOwner::Idle => t.idle_sec += sec,
                SlotOwner::Empty => t.empty_sec += sec,
            }
        }

        // M11's reconciliation. `entertainment_sec` splits cleanly in two:
        // the part that fell inside a window you plotted, and the rest.
        let windows: Vec<(Millis, Millis)> = plans
            .iter()
            .filter(|p| p.intent == BlockIntent::Entertainment)
            .map(|p| (p.starts_at, p.starts_at + p.duration_sec * 1000))
            .collect();
        t.entertainment_in_window_sec = intersect_seconds(&ent_intervals, &windows);

        t.by_area = areas
            .into_iter()
            .map(|(area_id, (name, colour, kind, seconds))| AreaTotal {
                area_id,
                name,
                colour,
                kind,
                seconds,
                monthly_target_sec: None,
            })
            .collect();
        t.by_area.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.name.cmp(&b.name)));

        t.by_project = projects
            .into_iter()
            .map(|(project_id, (name, colour, seconds))| ProjectTotal {
                project_id,
                name,
                colour,
                seconds,
            })
            .collect();
        t.by_project.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.name.cmp(&b.name)));

        t.by_contribution = {
            let mut v: Vec<ContributionTotal> = contributions
                .into_iter()
                .map(|(contribution, seconds)| ContributionTotal {
                    contribution,
                    seconds,
                })
                .collect();
            // Longest first, then by the enum's own order, so two reads of the
            // same day cannot come back differently ordered from a hash seed.
            v.sort_by(|a, b| {
                b.seconds.cmp(&a.seconds).then_with(|| {
                    a.contribution
                        .map(|c| c.as_str())
                        .cmp(&b.contribution.map(|c| c.as_str()))
                })
            });
            v
        };

        // Domains come from the spans, like apps do, and for the same reason:
        // a site seen during a confirmed session still answers "what was on
        // screen". It is evidence, and it is not a second duration.
        t.by_domain = {
            let mut by: HashMap<String, i64> = HashMap::new();
            for s in spans.iter().filter(|s| !s.is_idle) {
                let Some(domain) = s.domain.clone() else { continue };
                let clipped = s.ended_at.min(to) - s.started_at.max(from);
                if clipped > 0 {
                    *by.entry(domain).or_insert(0) += clipped / 1000;
                }
            }
            let mut v: Vec<DomainSeconds> = by
                .into_iter()
                .map(|(domain, seconds)| DomainSeconds { domain, seconds })
                .collect();
            v.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.domain.cmp(&b.domain)));
            v
        };

        // App totals come from the spans themselves, not from segments: an app
        // seen during a confirmed session still counts toward "where was the
        // machine", which is the question this list answers.
        let mut by_app: HashMap<String, i64> = HashMap::new();
        for s in spans.iter().filter(|s| !s.is_idle) {
            let clipped = s.ended_at.min(to) - s.started_at.max(from);
            if clipped > 0 {
                *by_app.entry(s.app_id.clone()).or_insert(0) += clipped / 1000;
            }
        }
        t.by_app = by_app
            .into_iter()
            .map(|(app_id, seconds)| AppTotal { app_id, seconds })
            .collect();
        t.by_app
            .sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.app_id.cmp(&b.app_id)));

        let _ = tz;
        Ok(t)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservedSpan {
    pub id: i64,
    pub started_at: Millis,
    pub ended_at: Millis,
    pub app_id: String,
    pub window_title: Option<String>,
    pub domain: Option<String>,
    /// The roll-up — `core` / `entertainment` / `other`. What the Day view and
    /// the month dashboard have keyed off since 0006.
    pub category: Option<String>,
    /// The specific label, since 0007. `None` means *nobody has said*, which is
    /// what the uncategorised list is built on.
    pub category_id: Option<String>,
    pub is_idle: bool,
}

impl ObservedSpan {
    pub fn seconds(&self) -> i64 {
        (self.ended_at - self.started_at) / 1000
    }

    /// What makes two spans "the same thing", for absorption below.
    fn subject(&self) -> (bool, &str, Option<&str>) {
        (self.is_idle, self.app_id.as_str(), self.domain.as_deref())
    }
}

/// Observation shorter than this is noise rather than activity, and is not
/// reported. Alt-tabbing to check a message is not a context switch worth a row
/// in a day's account.
///
/// Two minutes by default, at the user's request, and overridable per install.
pub const DEFAULT_MIN_SPAN_SEC: i64 = 120;

/// Removes the double-count where the foreground sampler and the browser
/// connector both describe the same interval.
///
/// Both write to `activity_span`, and while Chrome is frontmost both are
/// correct: the sampler says `chrome.exe`, the connector says `chrome.exe` on
/// `youtube.com`. Two rows, same seconds. `resolve_day` is unaffected — it picks
/// one owner per segment by precedence — but anything that walks spans directly
/// (per-app totals, category totals, the unlabelled list) would count the hour
/// twice.
///
/// The rule: **where a domain-bearing span covers the same app at the same
/// time, the app-only span gives way.** It is the same claim, less precisely
/// stated.
///
/// Subtraction rather than deletion, because the remainder is real: Chrome open
/// on `chrome://settings` records no domain, and that time genuinely is
/// app-only. Dropping the whole app span would lose it.
pub fn dedupe_browser_overlap(spans: Vec<ObservedSpan>) -> Vec<ObservedSpan> {
    // Per app, the intervals a domain was seen on.
    let mut covered: HashMap<&str, Vec<(Millis, Millis)>> = HashMap::new();
    for s in spans.iter().filter(|s| s.domain.is_some() && !s.is_idle) {
        covered
            .entry(s.app_id.as_str())
            .or_default()
            .push((s.started_at, s.ended_at));
    }
    if covered.is_empty() {
        return spans;
    }

    // The window title lives on the *sampler's* span, never on the connector's:
    // the extension sends a domain and nothing else, by design. So a
    // domain-bearing span knows it was youtube.com and not which video, while
    // the app span being subtracted underneath it holds "Video Name - YouTube -
    // Google Chrome" — the one detail that tells two stretches apart.
    //
    // Carry it across. It is the same application at the same instant, so this
    // is not an inference; it is the same fact recorded by the other of two
    // observers. Titles remain opt-in, so this is empty unless the user asked
    // for them.
    let mut titles: Vec<(Millis, Millis, &str, &str)> = spans
        .iter()
        .filter(|s| s.domain.is_none() && !s.is_idle)
        .filter_map(|s| {
            s.window_title
                .as_deref()
                .map(|t| (s.started_at, s.ended_at, s.app_id.as_str(), t))
        })
        .collect();
    titles.sort();

    let mut out = Vec::with_capacity(spans.len());
    for span in &spans {
        if span.domain.is_some() || span.is_idle {
            let mut span = span.clone();
            if span.window_title.is_none() {
                // The title that covered the most of this interval, so a stretch
                // spanning a title change is named after the larger part rather
                // than after whatever happened to start first.
                span.window_title = titles
                    .iter()
                    .filter(|(_, _, app, _)| *app == span.app_id)
                    .filter_map(|(a, b, _, t)| {
                        let overlap = (*b).min(span.ended_at) - (*a).max(span.started_at);
                        (overlap > 0).then_some((overlap, *t))
                    })
                    .max_by_key(|(overlap, _)| *overlap)
                    .map(|(_, t)| t.to_string());
            }
            out.push(span);
            continue;
        }
        let Some(ranges) = covered.get(span.app_id.as_str()) else {
            out.push(span.clone());
            continue;
        };
        // Walk left to right, emitting whatever is not already described.
        let mut cursor = span.started_at;
        let mut sorted: Vec<(Millis, Millis)> = ranges
            .iter()
            .copied()
            .filter(|(a, b)| *b > span.started_at && *a < span.ended_at)
            .collect();
        sorted.sort();
        for (a, b) in sorted {
            if a > cursor {
                out.push(ObservedSpan {
                    ended_at: a.min(span.ended_at),
                    ..span.clone()
                });
                out.last_mut().unwrap().started_at = cursor;
            }
            cursor = cursor.max(b);
            if cursor >= span.ended_at {
                break;
            }
        }
        if cursor < span.ended_at {
            let mut tail = span.clone();
            tail.started_at = cursor;
            out.push(tail);
        }
    }
    out.sort_by_key(|s| (s.started_at, s.id));
    out
}

/// Drops observation below the floor, closing the hole where the same subject
/// sits on both sides.
///
/// The absorption is the part worth arguing about. Simply deleting short spans
/// would leave a thirty-second gap in the middle of two hours of one editor —
/// and on untimed time that gap becomes **Unaccounted**, which is a worse lie
/// than the one it was trying to avoid. So:
///
/// - short span flanked by the *same* app-and-domain → absorbed into the run,
///   because that is what it was: one stretch with a blip in it;
/// - anything else → dropped, and the interval falls to whatever else owns it,
///   or to Unaccounted, which is honest — nothing worth recording happened.
///
/// Bridging is bounded by the floor itself, so this closes a blip and never a
/// genuine ten-minute absence.
///
/// **Nothing is deleted.** The floor is applied on read, so raising it to five
/// minutes and lowering it back recovers every row. The record is the record.
pub fn apply_min_span(spans: Vec<ObservedSpan>, min_sec: i64) -> Vec<ObservedSpan> {
    if min_sec <= 0 {
        return spans;
    }
    let min_ms = min_sec * 1000;
    let mut kept: Vec<ObservedSpan> = spans
        .into_iter()
        .filter(|s| s.ended_at - s.started_at >= min_ms)
        .collect();

    let mut out: Vec<ObservedSpan> = Vec::with_capacity(kept.len());
    for span in kept.drain(..) {
        match out.last_mut() {
            Some(prev)
                if prev.subject() == span.subject()
                    && span.started_at >= prev.ended_at
                    && span.started_at - prev.ended_at <= min_ms =>
            {
                prev.ended_at = span.ended_at;
            }
            _ => out.push(span),
        }
    }
    out
}

/// Confirmed work in a run shorter than this counts as fragmented. Fifteen
/// minutes: long enough to have done something, short enough that a run of them
/// is a day that got away from you.
pub const DEFAULT_FRAGMENT_SEC: i64 = 15 * 60;

/// How close a switch has to fall to a block's edge to count as **planned**.
///
/// Two minutes, because a person acting on their own plan does not act on the
/// second. Tighter and every deliberate switch reads as an interruption, which
/// would make the distinction worthless; looser and an interruption that happens
/// to land near a boundary gets excused.
const BOUNDARY_TOLERANCE_MS: i64 = 2 * 60_000;

/// The components of how broken up a day was. See [`Fragmentation`] for why this
/// returns four numbers and not a score.
///
/// Pure and database-free, like `resolve_day` itself: this is arithmetic over
/// segments the Day view already renders, so it can be checked against what is
/// on screen and tested without a store.
pub fn fragmentation(
    segments: &[DaySegment],
    plans: &[DayPlan],
    fragment_threshold_sec: i64,
) -> Fragmentation {
    // Every instant a plotted block starts or ends. A switch landing on one is
    // you executing your intention.
    let mut edges: Vec<Millis> = Vec::with_capacity(plans.len() * 2);
    for p in plans {
        edges.push(p.starts_at);
        edges.push(p.starts_at + p.duration_sec * 1000);
    }
    let on_edge = |at: Millis| edges.iter().any(|e| (at - e).abs() <= BOUNDARY_TOLERANCE_MS);

    let task_of = |seg: &DaySegment| match &seg.owner {
        SlotOwner::Work { task_id, .. } => Some(task_id.clone()),
        _ => None,
    };

    let mut out = Fragmentation {
        fragment_threshold_sec,
        ..Default::default()
    };

    // Runs of confirmed work on one task. A switch between two tasks ends a run
    // even though both are work — "unbroken" means unbroken *on the thing*.
    let mut run: Option<(String, Millis, Millis)> = None;
    let close = |run: &mut Option<(String, Millis, Millis)>, out: &mut Fragmentation| {
        if let Some((_, from, to)) = run.take() {
            let sec = (to - from) / 1000;
            out.stretches += 1;
            out.longest_stretch_sec = out.longest_stretch_sec.max(sec);
            if sec < fragment_threshold_sec {
                out.fragmented_sec += sec;
            }
        }
    };

    for (i, seg) in segments.iter().enumerate() {
        match task_of(seg) {
            Some(task) => match &mut run {
                Some((current, _, end)) if *current == task && *end == seg.from => {
                    *end = seg.to;
                }
                _ => {
                    close(&mut run, &mut out);
                    run = Some((task, seg.from, seg.to));
                }
            },
            None => close(&mut run, &mut out),
        }

        // A switch is a boundary between two things that both happened. Empty
        // and idle are not things you switched *to* — nothing was going on.
        if i + 1 < segments.len() {
            let next = &segments[i + 1];
            let real = |o: &SlotOwner| !matches!(o, SlotOwner::Empty | SlotOwner::Idle);
            if real(&seg.owner) && real(&next.owner) {
                if on_edge(seg.to) {
                    out.planned_switches += 1;
                } else {
                    out.unplanned_switches += 1;
                }
            }
        }
    }
    close(&mut run, &mut out);
    out
}

/// Adds two days' worth. Longest stretch is a **maximum**, not a sum — a week
/// with one three-hour stretch is not a week with a twenty-one-hour one.
pub fn add_fragmentation(a: &Fragmentation, b: &Fragmentation) -> Fragmentation {
    Fragmentation {
        longest_stretch_sec: a.longest_stretch_sec.max(b.longest_stretch_sec),
        stretches: a.stretches + b.stretches,
        planned_switches: a.planned_switches + b.planned_switches,
        unplanned_switches: a.unplanned_switches + b.unplanned_switches,
        fragmented_sec: a.fragmented_sec + b.fragmented_sec,
        fragment_threshold_sec: a.fragment_threshold_sec.max(b.fragment_threshold_sec),
    }
}

/// Cuts `[from, to)` at every boundary any claim introduces and gives each
/// resulting segment exactly one owner, by precedence.
///
/// Pure and database-free on purpose: the counting invariant is the product's
/// central promise, and a promise that can only be tested through SQL is a
/// promise tested less often.
///
/// Guarantees, all asserted in the tests below:
/// - segments tile `[from, to)` with no gaps and no overlaps;
/// - their durations sum to `to - from`, exactly, once;
/// - adjacent segments with equal owners are merged, so the output is minimal.
pub fn resolve_day(
    from: Millis,
    to: Millis,
    claims: Vec<Claim>,
    spans: &[ObservedSpan],
) -> Vec<DaySegment> {
    if to <= from {
        return Vec::new();
    }

    let mut cuts: BTreeSet<Millis> = BTreeSet::new();
    cuts.insert(from);
    cuts.insert(to);
    for c in &claims {
        if c.from > from && c.from < to {
            cuts.insert(c.from);
        }
        if c.to > from && c.to < to {
            cuts.insert(c.to);
        }
    }

    let bounds: Vec<Millis> = cuts.into_iter().collect();
    let mut out: Vec<DaySegment> = Vec::with_capacity(bounds.len());

    for pair in bounds.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        // The winner is the lowest-ranked claim covering this whole slice.
        // Claims were cut at every boundary, so "covers the midpoint" and
        // "covers the slice" are the same test.
        let winner = claims
            .iter()
            .filter(|c| c.from <= a && c.to >= b)
            .min_by_key(|c| c.owner.rank())
            .map(|c| c.owner.clone())
            .unwrap_or(SlotOwner::Empty);

        match out.last_mut() {
            Some(last) if last.owner == winner && last.to == a => last.to = b,
            _ => out.push(DaySegment {
                from: a,
                to: b,
                owner: winner,
                evidence: Vec::new(),
                has_distraction: false,
            }),
        }
    }

    attach_evidence(&mut out, spans);
    out
}

/// Hangs observed applications off each segment as **evidence, not duration**.
///
/// This is what makes M8 true: a timer overlapping PC activity is enriched with
/// what the machine saw, and the day still sums to a day. The seconds here are
/// deliberately *not* added to any total.
fn attach_evidence(segments: &mut [DaySegment], spans: &[ObservedSpan]) {
    for seg in segments.iter_mut() {
        // An observed segment already *is* the observation; repeating it as
        // evidence would render the same fact twice in the same row.
        if matches!(seg.owner, SlotOwner::Observed { .. } | SlotOwner::Idle) {
            continue;
        }
        let mut totals: HashMap<String, i64> = HashMap::new();
        for s in spans.iter().filter(|s| !s.is_idle) {
            let overlap = s.ended_at.min(seg.to) - s.started_at.max(seg.from);
            if overlap > 0 {
                *totals.entry(s.app_id.clone()).or_insert(0) += overlap / 1000;
                // Entertainment observed *inside* confirmed work. Not a
                // duration — the work keeps the whole interval — but it is the
                // finding the day view exists to surface.
                if s.category.as_deref() == Some("entertainment") {
                    seg.has_distraction = true;
                }
            }
        }
        let mut evidence: Vec<AppTotal> = totals
            .into_iter()
            .map(|(app_id, seconds)| AppTotal { app_id, seconds })
            .collect();
        evidence.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.app_id.cmp(&b.app_id)));
        evidence.truncate(3);
        seg.evidence = evidence;
    }
}

/// Total covered time across possibly-overlapping intervals, counted once.
fn merged_seconds(intervals: impl Iterator<Item = (Millis, Millis)>) -> i64 {
    let mut v: Vec<(Millis, Millis)> = intervals.filter(|(a, b)| b > a).collect();
    v.sort_unstable();
    let mut total = 0;
    let mut cursor: Option<(Millis, Millis)> = None;
    for (a, b) in v {
        match cursor {
            Some((ca, cb)) if a <= cb => cursor = Some((ca, cb.max(b))),
            Some((ca, cb)) => {
                total += cb - ca;
                cursor = Some((a, b));
            }
            None => cursor = Some((a, b)),
        }
    }
    if let Some((ca, cb)) = cursor {
        total += cb - ca;
    }
    total / 1000
}

/// Seconds where `a` and `b` overlap, counting any instant once however many
/// intervals cover it.
///
/// This is what makes M11's reconciliation checkable rather than asserted:
/// entertainment that happened inside a window you plotted, plus entertainment
/// that did not, is all the entertainment there was.
fn intersect_seconds(a: &[(Millis, Millis)], b: &[(Millis, Millis)]) -> i64 {
    let mut pieces: Vec<(Millis, Millis)> = Vec::new();
    for &(a0, a1) in a {
        for &(b0, b1) in b {
            let lo = a0.max(b0);
            let hi = a1.min(b1);
            if hi > lo {
                pieces.push((lo, hi));
            }
        }
    }
    merged_seconds(pieces.into_iter())
}

/// Projects segments onto the display grid. The grid never changes the
/// arithmetic — a slot simply lists whatever segments touch it.
fn build_slots(
    from: Millis,
    to: Millis,
    slot_minutes: i64,
    segments: &[DaySegment],
    plans: &[DayPlan],
) -> Vec<DaySlot> {
    let step = slot_minutes * 60_000;
    let mut slots = Vec::new();
    let mut index = 0;
    let mut cursor = from;

    while cursor < to {
        let end = (cursor + step).min(to);
        let mut overlapping: Vec<DaySegment> = segments
            .iter()
            .filter(|s| s.from < end && s.to > cursor)
            .cloned()
            .collect();
        // Longest first: the slot's headline is whatever most of it was.
        overlapping.sort_by_key(|s| -((s.to.min(end) - s.from.max(cursor)) as i64));

        let slot_plans: Vec<DayPlan> = plans
            .iter()
            .filter(|p| p.starts_at < end && p.starts_at + p.duration_sec * 1000 > cursor)
            .cloned()
            .collect();

        // The state answers "what is this row, at a glance", so it reports the
        // longest **non-empty** owner. Reporting the longest owner outright
        // would label a slot holding twenty minutes of work "Unaccounted"
        // whenever the gap beside it was larger — true arithmetic, and a
        // contradiction of the chips rendered next to it.
        //
        // `Empty` therefore means *nothing at all happened here*, which is the
        // only reading that makes the word actionable.
        let dominant = overlapping
            .iter()
            .find(|s| s.owner != SlotOwner::Empty)
            .map(|s| &s.owner);
        let state = match dominant {
            Some(SlotOwner::Life { is_private: true, .. }) => SlotState::Private,
            Some(SlotOwner::Life { .. }) => SlotState::ConfirmedLife,
            Some(SlotOwner::Work { .. }) => SlotState::ConfirmedWork,
            Some(SlotOwner::Observed { .. }) => SlotState::ObservedOnly,
            Some(SlotOwner::Idle) => SlotState::Idle,
            // Empty with a block over it is the most actionable state on the
            // screen: it is the difference between intending and doing.
            Some(SlotOwner::Empty) | None => {
                if slot_plans.is_empty() {
                    SlotState::Empty
                } else {
                    SlotState::PlannedNotStarted
                }
            }
        };

        slots.push(DaySlot {
            index,
            starts_at: cursor,
            ends_at: end,
            segments: overlapping,
            plans: slot_plans,
            state,
        });
        index += 1;
        cursor = end;
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: Millis = 3_600_000;

    fn life(from: Millis, to: Millis, private: bool) -> Claim {
        Claim {
            from,
            to,
            owner: SlotOwner::Life {
                entry_id: "e".into(),
                area_id: "a".into(),
                area_name: "Sleep/Rest".into(),
                area_colour: "#000".into(),
                area_kind: AreaKind::Rest,
                label: None,
                is_private: private,
            },
        }
    }

    fn work(from: Millis, to: Millis) -> Claim {
        Claim {
            from,
            to,
            owner: SlotOwner::Work {
                session_id: "s".into(),
                task_id: "t".into(),
                task_title: "Refactor".into(),
                project_id: None,
                project_name: None,
                project_colour: None,
                contribution: None,
                source: "timer".into(),
            },
        }
    }

    fn observed(from: Millis, to: Millis) -> Claim {
        Claim {
            from,
            to,
            owner: SlotOwner::Observed {
                app_id: "code.exe".into(),
                domain: None,
                category: None,
            },
        }
    }

    fn sums_to_day(segments: &[DaySegment], from: Millis, to: Millis) {
        let total: i64 = segments.iter().map(|s| s.to - s.from).sum();
        assert_eq!(total, to - from, "segments must sum to the day exactly once");
        // …and tile it: no gaps, no overlaps.
        let mut cursor = from;
        for s in segments {
            assert_eq!(s.from, cursor, "gap or overlap before {}", s.from);
            cursor = s.to;
        }
        assert_eq!(cursor, to);
    }

    #[test]
    fn an_empty_day_is_one_empty_segment() {
        let segments = resolve_day(0, 24 * H, vec![], &[]);
        sums_to_day(&segments, 0, 24 * H);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].owner, SlotOwner::Empty);
    }

    // ─── fragmentation (W6) ────────────────────────────────────────────

    fn work_on(task: &str, from: Millis, to: Millis) -> Claim {
        Claim {
            from,
            to,
            owner: SlotOwner::Work {
                session_id: "s".into(),
                task_id: task.into(),
                task_title: task.into(),
                project_id: None,
                project_name: None,
                project_colour: None,
                contribution: None,
                source: "timer".into(),
            },
        }
    }

    fn plan(from: Millis, minutes: i64) -> DayPlan {
        DayPlan {
            block_id: "b".into(),
            task_id: None,
            title: "Plotted".into(),
            project_colour: None,
            starts_at: from,
            duration_sec: minutes * 60,
            tracked_sec: 0,
            drift_sec: 0,
            drift_state: DriftState::OnEstimate,
            is_fixed: false,
            series_id: None,
            intent: BlockIntent::Work,
        }
    }

    const M: Millis = 60_000;

    /// A day of one unbroken session is the simplest case, and the one every
    /// other number has to be read against.
    #[test]
    fn one_unbroken_session_is_one_stretch_and_no_switches() {
        let segments = resolve_day(9 * H, 12 * H, vec![work(9 * H, 12 * H)], &[]);
        let f = fragmentation(&segments, &[], DEFAULT_FRAGMENT_SEC);
        assert_eq!(f.stretches, 1);
        assert_eq!(f.longest_stretch_sec, 3 * 3600);
        assert_eq!(f.planned_switches, 0);
        assert_eq!(f.unplanned_switches, 0);
        assert_eq!(f.fragmented_sec, 0);
    }

    /// **The claim an app that only watches window focus cannot make.** The
    /// same two switches, and the *plan* underneath decides what they mean.
    #[test]
    fn a_switch_on_a_block_boundary_is_you_executing_your_intention() {
        // Two tasks, back to back at 10:00.
        let segments = resolve_day(
            9 * H,
            11 * H,
            vec![work_on("a", 9 * H, 10 * H), work_on("b", 10 * H, 11 * H)],
            &[],
        );

        // Nothing plotted: the switch was an interruption as far as anyone knows.
        let f = fragmentation(&segments, &[], DEFAULT_FRAGMENT_SEC);
        assert_eq!((f.planned_switches, f.unplanned_switches), (0, 1));

        // Plotted to end exactly there: the switch was the plan working.
        let f = fragmentation(&segments, &[plan(9 * H, 60)], DEFAULT_FRAGMENT_SEC);
        assert_eq!((f.planned_switches, f.unplanned_switches), (1, 0));

        // A minute either side is still the plan. A person acting on their own
        // intention does not act on the second.
        let f = fragmentation(&segments, &[plan(9 * H, 61)], DEFAULT_FRAGMENT_SEC);
        assert_eq!((f.planned_switches, f.unplanned_switches), (1, 0));
        // Twenty minutes late is not.
        let f = fragmentation(&segments, &[plan(9 * H, 80)], DEFAULT_FRAGMENT_SEC);
        assert_eq!((f.planned_switches, f.unplanned_switches), (0, 1));
    }

    /// Empty and idle are not things you switched *to* — nothing was going on.
    /// Counting them would make every lunch break two interruptions.
    #[test]
    fn a_gap_is_not_a_switch() {
        let segments = resolve_day(
            9 * H,
            12 * H,
            vec![work_on("a", 9 * H, 10 * H), work_on("a", 11 * H, 12 * H)],
            &[],
        );
        let f = fragmentation(&segments, &[], DEFAULT_FRAGMENT_SEC);
        assert_eq!(f.unplanned_switches, 0, "an hour of nothing is not a switch");
        assert_eq!(f.stretches, 2, "but it does end the stretch");
        assert_eq!(f.longest_stretch_sec, 3600);
    }

    /// Time that counts and accomplished little. The threshold rides along with
    /// the figure so it can never be read without the rule that made it.
    #[test]
    fn work_in_short_runs_is_reported_as_fragmented() {
        let claims = vec![
            work_on("a", 9 * H, 9 * H + 6 * M),
            work_on("b", 9 * H + 6 * M, 9 * H + 12 * M),
            work_on("c", 9 * H + 12 * M, 11 * H),
        ];
        let segments = resolve_day(9 * H, 11 * H, claims, &[]);
        let f = fragmentation(&segments, &[], DEFAULT_FRAGMENT_SEC);

        assert_eq!(f.stretches, 3);
        assert_eq!(f.fragmented_sec, 12 * 60, "the two six-minute runs");
        assert_eq!(f.longest_stretch_sec, 108 * 60);
        assert_eq!(f.fragment_threshold_sec, DEFAULT_FRAGMENT_SEC);
    }

    /// A week's longest stretch is a maximum, not a sum. Adding them would make
    /// five three-hour days look like a fifteen-hour one.
    #[test]
    fn adding_days_takes_the_longest_stretch_rather_than_summing_it() {
        let a = Fragmentation {
            longest_stretch_sec: 3 * 3600,
            stretches: 2,
            unplanned_switches: 1,
            fragmented_sec: 300,
            ..Default::default()
        };
        let b = Fragmentation {
            longest_stretch_sec: 2 * 3600,
            stretches: 3,
            unplanned_switches: 4,
            fragmented_sec: 600,
            ..Default::default()
        };
        let sum = add_fragmentation(&a, &b);
        assert_eq!(sum.longest_stretch_sec, 3 * 3600);
        assert_eq!(sum.stretches, 5);
        assert_eq!(sum.unplanned_switches, 5);
        assert_eq!(sum.fragmented_sec, 900);
    }

    #[test]
    fn precedence_is_life_then_work_then_observed() {
        // All three claim 09:00–10:00; life wins the whole hour.
        let segments = resolve_day(
            8 * H,
            11 * H,
            vec![observed(9 * H, 10 * H), work(9 * H, 10 * H), life(9 * H, 10 * H, false)],
            &[],
        );
        sums_to_day(&segments, 8 * H, 11 * H);
        let hour = segments.iter().find(|s| s.from == 9 * H).unwrap();
        assert!(matches!(hour.owner, SlotOwner::Life { .. }));
        assert_eq!(hour.to, 10 * H);
    }

    #[test]
    fn a_partial_overlap_splits_rather_than_double_counting() {
        // Work 09:00–10:00, observation 09:20–10:30. The observation only owns
        // the half hour nobody confirmed.
        let segments = resolve_day(
            9 * H,
            11 * H,
            vec![work(9 * H, 10 * H), observed(9 * H + 20 * 60_000, 10 * H + 30 * 60_000)],
            &[],
        );
        sums_to_day(&segments, 9 * H, 11 * H);

        let work_sec: i64 = segments
            .iter()
            .filter(|s| matches!(s.owner, SlotOwner::Work { .. }))
            .map(|s| (s.to - s.from) / 1000)
            .sum();
        let observed_sec: i64 = segments
            .iter()
            .filter(|s| matches!(s.owner, SlotOwner::Observed { .. }))
            .map(|s| (s.to - s.from) / 1000)
            .sum();
        assert_eq!(work_sec, 3600, "the confirmed hour keeps all of it");
        assert_eq!(observed_sec, 1800, "and the observation keeps only the rest");
    }

    #[test]
    fn adjacent_equal_owners_merge() {
        // Two touching life entries of the same shape produce one segment, not
        // two — the output is minimal so the Day view isn't rendering seams.
        let segments = resolve_day(
            0,
            4 * H,
            vec![life(H, 2 * H, false), life(2 * H, 3 * H, false)],
            &[],
        );
        sums_to_day(&segments, 0, 4 * H);
        assert_eq!(segments.len(), 3, "empty · life · empty");
        assert_eq!(segments[1].from, H);
        assert_eq!(segments[1].to, 3 * H);
    }

    #[test]
    fn claims_outside_the_day_are_clipped_not_counted() {
        // A night's sleep starting the previous evening.
        let segments = resolve_day(0, 24 * H, vec![life(-3 * H, 7 * H, false)], &[]);
        sums_to_day(&segments, 0, 24 * H);
        let life_sec: i64 = segments
            .iter()
            .filter(|s| matches!(s.owner, SlotOwner::Life { .. }))
            .map(|s| (s.to - s.from) / 1000)
            .sum();
        assert_eq!(life_sec, 7 * 3600, "only the part inside the day counts");
    }

    #[test]
    fn a_short_day_still_sums_to_itself() {
        // A spring-forward day is 23 hours. The invariant is "the day", not "24h".
        let segments = resolve_day(0, 23 * H, vec![work(H, 5 * H)], &[]);
        sums_to_day(&segments, 0, 23 * H);
    }

    /// The counting invariant, over randomly overlapping records (M2, M4).
    #[test]
    fn overlapping_records_never_double_count() {
        let mut seed: u64 = 0x5DEECE66D;
        let mut next = |n: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };

        for _ in 0..200 {
            let mut claims = Vec::new();
            for _ in 0..next(12) {
                let start = (next(24 * 60) as i64) * 60_000;
                let len = ((next(180) + 1) as i64) * 60_000;
                claims.push(match next(3) {
                    0 => life(start, start + len, next(2) == 0),
                    1 => work(start, start + len),
                    _ => observed(start, start + len),
                });
            }
            let segments = resolve_day(0, 24 * H, claims, &[]);
            sums_to_day(&segments, 0, 24 * H);
        }
    }

    #[test]
    fn merged_seconds_counts_overlap_once() {
        assert_eq!(merged_seconds([(0, H), (0, H)].into_iter()), 3600);
        assert_eq!(merged_seconds([(0, H), (H / 2, 2 * H)].into_iter()), 7200);
        assert_eq!(merged_seconds([(0, H), (2 * H, 3 * H)].into_iter()), 7200);
        assert_eq!(merged_seconds([].into_iter()), 0);
    }
}
