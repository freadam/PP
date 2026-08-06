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

/// How a confirmed segment reads on the evidence panel.
fn owner_label(owner: &SlotOwner) -> String {
    match owner {
        SlotOwner::Life { label, area_name, is_private, .. } => {
            if *is_private {
                "Private".into()
            } else {
                label.clone().unwrap_or_else(|| area_name.clone())
            }
        }
        SlotOwner::Work { task_title, .. } => task_title.clone(),
        _ => String::new(),
    }
}

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
                    explanation: format!(
                        "You plotted {} and never started it. Nothing was tracked against this block.",
                        human_duration(planned)
                    ),
                    recommendation: Some(
                        "Moving it keeps the intention; dropping it admits the plan was wrong. Both are honest — leaving it is what isn't.".into(),
                    ),
                    evidence: None,
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
                    explanation: format!(
                        "Plotted {}, tracked {} — {} over. The estimate is the thing that was wrong, not the work.",
                        human_duration(planned),
                        human_duration(tracked_sec),
                        human_duration(drift.abs())
                    ),
                    recommendation: Some(
                        "Accepting records what happened. Revising the estimate is what makes the next one better.".into(),
                    ),
                    evidence: None,
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
                explanation: format!(
                    "{} of work with nothing plotted against it. Real work the plan didn't know about.",
                    human_duration(elapsed)
                ),
                recommendation: Some(
                    "A retroactive block is how the plan learns this kind of work exists.".into(),
                ),
                evidence: None,
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
                explanation: format!(
                    "{} inside your planned hours with no session against it.",
                    human_duration(seconds)
                ),
                recommendation: None,
                evidence: None,
                default_action: ReconcileVerb::Ignore,
                available: vec![
                    ReconcileVerb::Ignore,
                    ReconcileVerb::AssignToTask,
                    ReconcileVerb::LogAsBreak,
                    ReconcileVerb::RecordAsLife,
                    ReconcileVerb::MarkPrivate,
                ],
                suggested_slot: Some(gap_from),
                suggested_duration_sec: seconds,
            });
        }

        // ── observed-only, and hours nobody accounted for (M10) ────────
        //
        // These come from `resolve_day`, not from a query of their own: the
        // reconciler must be asking about exactly the intervals the Day view
        // shows, or the two screens disagree about what is left to decide.
        let areas = self.get_life_areas(tz, false)?;
        let entertainment = areas.iter().find(|a| a.kind == AreaKind::Entertainment);
        for segment in self.get_day(date, tz, None)?.segments {
            let seconds = (segment.to - segment.from) / 1000;
            if seconds < GAP_THRESHOLD_SEC {
                continue; // a few minutes is life, not drift (§3.7)
            }
            match &segment.owner {
                SlotOwner::Observed { app_id, domain, category } => {
                    let subject = domain.clone().unwrap_or_else(|| app_id.clone());
                    let is_entertainment = category.as_deref() == Some("entertainment");
                    items.push(ReconcileItem {
                        id: format!("observed:{}", segment.from),
                        kind: ReconcileKind::ObservedOnly,
                        title: format!(
                            "{subject} · {}–{}",
                            clock(segment.from, tz),
                            clock(segment.to, tz)
                        ),
                        block_id: None,
                        task_id: None,
                        session_id: None,
                        planned_sec: 0,
                        tracked_sec: 0,
                        drift_sec: 0,
                        starts_at: Some(segment.from),
                        ends_at: Some(segment.to),
                        explanation: format!(
                            "{} observed {subject} in the foreground for {}. No confirmed activity covers this time.",
                            if domain.is_some() { "The browser connector" } else { "Fruit" },
                            human_duration(seconds)
                        ),
                        recommendation: Some(if is_entertainment {
                            "Recommended from the default rule for this domain.".into()
                        } else {
                            "Attaching it to a task keeps the observation as evidence rather than replacing it.".into()
                        }),
                        evidence: Some(ReconcileEvidence {
                            source: if domain.is_some() {
                                "Browser connector".into()
                            } else {
                                "Foreground window".into()
                            },
                            subject,
                            confidence: if domain.is_some() {
                                "High · active foreground tab".into()
                            } else {
                                "High · frontmost application".into()
                            },
                            adjacent: self.adjacent_labels(segment.from, segment.to, date, tz)?,
                            // The privacy promise, restated at the moment it
                            // matters — which is the moment someone is looking
                            // at a record of what they did.
                            storage: if domain.is_some() {
                                "Domain only. No full URL or page title.".into()
                            } else {
                                "Application name only. No window title unless you enabled titles.".into()
                            },
                            domain: domain.clone(),
                        }),
                        default_action: if is_entertainment {
                            ReconcileVerb::RecordAsLife
                        } else {
                            ReconcileVerb::AssignToTask
                        },
                        available: vec![
                            ReconcileVerb::RecordAsLife,
                            ReconcileVerb::AssignToTask,
                            ReconcileVerb::MarkPrivate,
                            ReconcileVerb::Ignore,
                        ],
                        suggested_slot: Some(segment.from),
                        suggested_duration_sec: seconds,
                    });
                }
                SlotOwner::Empty => {
                    items.push(ReconcileItem {
                        id: format!("empty:{}", segment.from),
                        kind: ReconcileKind::Empty,
                        title: format!(
                            "Unaccounted · {}–{}",
                            clock(segment.from, tz),
                            clock(segment.to, tz)
                        ),
                        block_id: None,
                        task_id: None,
                        session_id: None,
                        planned_sec: 0,
                        tracked_sec: 0,
                        drift_sec: 0,
                        starts_at: Some(segment.from),
                        ends_at: Some(segment.to),
                        explanation: format!(
                            "{} with no record and nothing observed. The machine wasn't watching and neither was the timer.",
                            human_duration(seconds)
                        ),
                        recommendation: Some(
                            "Filling it is what makes the month's account trustworthy. Marking it private accounts for it without recording anything.".into(),
                        ),
                        evidence: None,
                        default_action: ReconcileVerb::RecordAsLife,
                        available: vec![
                            ReconcileVerb::RecordAsLife,
                            ReconcileVerb::AssignToTask,
                            ReconcileVerb::MarkPrivate,
                            ReconcileVerb::Ignore,
                        ],
                        suggested_slot: Some(segment.from),
                        suggested_duration_sec: seconds,
                    });
                }
                _ => {}
            }
        }
        let _ = entertainment;

        let _ = now;
        Ok(items)
    }

    /// What sits either side of an interval, for the evidence panel. Being able
    /// to see "10:30 Team call · 12:00 Lunch" is usually enough to remember what
    /// the hour between them was.
    fn adjacent_labels(
        &self,
        from: Millis,
        to: Millis,
        date: &str,
        tz: &str,
    ) -> Result<Vec<String>> {
        let day = self.get_day(date, tz, None)?;
        let mut out = Vec::new();
        if let Some(before) = day
            .segments
            .iter()
            .filter(|s| s.to <= from && s.owner.is_confirmed())
            .next_back()
        {
            out.push(format!("{} {}", clock(before.from, tz), owner_label(&before.owner)));
        }
        if let Some(after) = day
            .segments
            .iter()
            .find(|s| s.from >= to && s.owner.is_confirmed())
        {
            out.push(format!("{} {}", clock(after.from, tz), owner_label(&after.owner)));
        }
        Ok(out)
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

            // "Apply my choice to future activity in this context" (wireframe 4).
            //
            // The rule is written *before* the decision it came from, and it is
            // deliberately prospective only: `activity_span.category` is stamped
            // at write time, so a rule made today classifies tomorrow and leaves
            // every day already reconciled exactly as the user left it. A rule
            // that reached backwards would silently rewrite a month someone has
            // already signed off.
            if let Some(domain) = &action.rule_for_domain {
                let category_id = match &action.rule_category_id {
                    Some(id) => id.clone(),
                    // No explicit label, so the verb decides. Two cases only —
                    // anything else is not a verdict about the site, and
                    // inventing one would put a rule behind a shrug.
                    None => match action.verb {
                        // Just called this interval personal time. What that
                        // means for a *site* is "not work", which is the bucket
                        // the reduction target is measured in.
                        ReconcileVerb::RecordAsLife => category::DISTRACTION.to_string(),
                        // Attached to a task: this is how work gets done.
                        ReconcileVerb::AssignToTask => category::WORK.to_string(),
                        _ => continue,
                    },
                };
                self.set_activity_rule(MatchKind::Domain, domain, &category_id)?;
            }
            match action.verb {
                ReconcileVerb::Accept | ReconcileVerb::Ignore | ReconcileVerb::LeaveUnscheduled => {}

                // Turns an observation, or an hour of nothing, into a record.
                ReconcileVerb::RecordAsLife | ReconcileVerb::MarkPrivate => {
                    let (Some(started_at), Some(seconds)) =
                        (action.starts_at, action.duration_sec)
                    else {
                        return Err(AppError::invalid(
                            "Recording an interval needs its start and length.",
                        ));
                    };
                    let area = match &action.life_area_id {
                        Some(id) => id.clone(),
                        // Private has to land somewhere; the flag is what
                        // matters, and the area is never shown for it.
                        None => self
                            .get_life_areas(tz, false)?
                            .first()
                            .map(|a| a.id.clone())
                            .ok_or_else(|| AppError::invalid("No life areas exist."))?,
                    };
                    self.add_life_entry(NewLifeEntry {
                        life_area_id: area,
                        label: None,
                        started_at,
                        ended_at: started_at + seconds * 1000,
                        tz: tz.to_string(),
                        is_private: action.verb == ReconcileVerb::MarkPrivate,
                        note: None,
                        // The interval being reconciled is by definition not
                        // confirmed, so there is nothing to replace.
                        replace_existing: false,
                    })?;
                    let _ = (kind, raw_id);
                }

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
