//! Focus intervals and session review (ARCHITECTURE §3.4, §3.6, §4.5, §4.8).
//!
//! Two records the spec names and the schema did not have, and one idea they
//! share: **the machine may measure, but only the user may decide.**
//!
//! A focus interval is a claim about the *quality* of tracked time, so it
//! nests inside a session and is never added to it (invariant 8). A session
//! review is a claim that a tracked interval is an accurate record, so it can
//! only ever come from an explicit user action (invariant 13) — a Productive
//! classification can order the queue and cannot verify anything.

use rusqlite::{params, OptionalExtension};

use super::Store;
use crate::error::{AppError, Result};
use crate::ids::new_id;
use crate::model::*;

impl Store {
    /* ─── focus intervals (§3.4, §4.5) ──────────────────────────────── */

    /// `StartFocus` — open a focus interval inside the running task session.
    ///
    /// Focus cannot run without a task session (§5.3): there is no such thing
    /// as deep work on nothing, and allowing one would create a second kind of
    /// tracked time that the containment invariant could not police.
    pub fn start_focus_interval(&mut self) -> Result<FocusInterval> {
        let now = self.now();
        let Some((session_id, task_id, started_at)) = self.running_session()? else {
            return Err(AppError::invalid(
                "Focus runs inside a task session. Start a task first.",
            ));
        };

        // Idempotent: asking again for the focus that is already running
        // returns it rather than opening a second one. Two open intervals
        // would overlap by construction, which invariant 7 forbids.
        if let Some(open) = self.open_focus_interval()? {
            return Ok(open);
        }

        // The interval cannot begin before its parent (§4.5 containment).
        let starts = now.max(started_at);
        let id = new_id();
        self.conn.execute(
            "INSERT INTO focus_interval (id, session_id, started_at, created_at, updated_at)
                  VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id, session_id, starts, now],
        )?;
        Ok(FocusInterval {
            id,
            session_id,
            task_id,
            started_at: starts,
            ended_at: None,
            interruption_reason: None,
            elapsed_sec: 0,
        })
    }

    /// `StopFocus`. Closing is idempotent — nothing open is not an error,
    /// because stopping a session also stops its focus (§3.4) and the user
    /// pressing the key afterwards has not done anything wrong.
    pub fn stop_focus_interval(&mut self, reason: Option<&str>) -> Result<Option<FocusInterval>> {
        let now = self.now();
        let Some(open) = self.open_focus_interval()? else {
            return Ok(None);
        };
        // Never past the session's own end: a focus interval that outlived its
        // parent would break containment and inflate focus above tracked.
        let bound = self.session_end_bound(&open.session_id)?.unwrap_or(now);
        let end = now.min(bound).max(open.started_at + 1000);
        self.conn.execute(
            "UPDATE focus_interval SET ended_at = ?2, interruption_reason = ?3, updated_at = ?4
              WHERE id = ?1",
            params![open.id, end, reason, now],
        )?;
        Ok(Some(FocusInterval {
            ended_at: Some(end),
            elapsed_sec: (end - open.started_at) / 1000,
            ..open
        }))
    }

    pub fn open_focus_interval(&self) -> Result<Option<FocusInterval>> {
        let row = self
            .conn
            .query_row(
                "SELECT f.id, f.session_id, s.task_id, f.started_at, f.interruption_reason
                   FROM focus_interval f JOIN time_session s ON s.id = f.session_id
                  WHERE f.ended_at IS NULL
                  ORDER BY f.started_at DESC LIMIT 1",
                [],
                |r| {
                    Ok(FocusInterval {
                        id: r.get(0)?,
                        session_id: r.get(1)?,
                        task_id: r.get(2)?,
                        started_at: r.get(3)?,
                        ended_at: None,
                        interruption_reason: r.get(4)?,
                        elapsed_sec: 0,
                    })
                },
            )
            .optional()?;
        let now = self.now();
        Ok(row.map(|f| FocusInterval {
            elapsed_sec: ((now - f.started_at) / 1000).max(0),
            ..f
        }))
    }

    /// Focus seconds inside one session. Clamped to the session's own bounds,
    /// so this can never exceed tracked time however the rows were written
    /// (invariant 8).
    pub fn focus_sec_for_session(&self, session_id: &str) -> Result<i64> {
        let now = self.now();
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(
                        MIN(COALESCE(f.ended_at, ?2), COALESCE(s.ended_at, ?2))
                      - MAX(f.started_at, s.started_at)
                    ), 0) / 1000
               FROM focus_interval f JOIN time_session s ON s.id = f.session_id
              WHERE f.session_id = ?1
                AND MIN(COALESCE(f.ended_at, ?2), COALESCE(s.ended_at, ?2))
                  > MAX(f.started_at, s.started_at)",
            params![session_id, now],
            |r| r.get(0),
        )?)
    }

    pub fn focus_intervals_for_session(&self, session_id: &str) -> Result<Vec<FocusInterval>> {
        let now = self.now();
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.session_id, s.task_id, f.started_at, f.ended_at,
                    f.interruption_reason
               FROM focus_interval f JOIN time_session s ON s.id = f.session_id
              WHERE f.session_id = ?1 ORDER BY f.started_at",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            let started_at: i64 = r.get(3)?;
            let ended_at: Option<i64> = r.get(4)?;
            Ok(FocusInterval {
                id: r.get(0)?,
                session_id: r.get(1)?,
                task_id: r.get(2)?,
                started_at,
                ended_at,
                interruption_reason: r.get(5)?,
                elapsed_sec: ((ended_at.unwrap_or(now) - started_at) / 1000).max(0),
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn running_session(&self) -> Result<Option<(String, String, Millis)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, task_id, started_at FROM time_session WHERE ended_at IS NULL
                  ORDER BY started_at DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    fn session_end_bound(&self, session_id: &str) -> Result<Option<Millis>> {
        Ok(self
            .conn
            .query_row(
                "SELECT ended_at FROM time_session WHERE id = ?1",
                params![session_id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten())
    }

    /* ─── session review (§3.6, §4.8, §7.1) ─────────────────────────── */

    /// `VerifySession` — the only thing that can make a session verified
    /// (invariant 13).
    ///
    /// Records the decision *and* the evidence the user was looking at, then
    /// supersedes any previous decision rather than overwriting it.
    pub fn review_session(
        &mut self,
        session_id: &str,
        decision: ReviewDecision,
        note: Option<&str>,
    ) -> Result<SessionReview> {
        let now = self.now();
        let evidence = self.session_evidence(session_id)?;
        let previous: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM session_current_review WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .optional()?;

        let id = new_id();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO session_review
                 (id, session_id, supersedes_review_id, decision, reviewed_at,
                  observed_work_sec, observed_other_sec, idle_sec, unobserved_sec,
                  note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?5)",
            params![
                id,
                session_id,
                previous,
                decision.as_str(),
                now,
                evidence.observed_work_sec,
                evidence.observed_other_sec,
                evidence.idle_sec,
                evidence.unobserved_sec,
                note,
            ],
        )?;
        // The flag stays in step so every existing query that reads it — the
        // week allocation's verified/pending split among them — keeps working
        // without being taught about the review chain.
        tx.execute(
            "UPDATE time_session SET is_confirmed = ?2, updated_at = ?3 WHERE id = ?1",
            params![
                session_id,
                (decision == ReviewDecision::Verified) as i64,
                now
            ],
        )?;
        tx.commit()?;

        // Invariant 21 — a decision about a session reopens the day it touches.
        self.reopen_days_touching(session_id)?;

        Ok(SessionReview {
            id,
            session_id: session_id.to_string(),
            supersedes_review_id: previous,
            decision,
            reviewed_at: now,
            observed_work_sec: evidence.observed_work_sec,
            observed_other_sec: evidence.observed_other_sec,
            idle_sec: evidence.idle_sec,
            unobserved_sec: evidence.unobserved_sec,
            note: note.map(str::to_string),
        })
    }

    /// Every decision ever made about a session, newest first. The audit trail
    /// §4.8 asks for: a superseded review is history, not a deleted row.
    pub fn session_reviews(&self, session_id: &str) -> Result<Vec<SessionReview>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, supersedes_review_id, decision, reviewed_at,
                    observed_work_sec, observed_other_sec, idle_sec, unobserved_sec, note
               FROM session_review WHERE session_id = ?1 ORDER BY reviewed_at DESC",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(SessionReview {
                id: r.get(0)?,
                session_id: r.get(1)?,
                supersedes_review_id: r.get(2)?,
                decision: ReviewDecision::parse(&r.get::<_, String>(3)?)
                    .unwrap_or(ReviewDecision::Pending),
                reviewed_at: r.get(4)?,
                observed_work_sec: r.get(5)?,
                observed_other_sec: r.get(6)?,
                idle_sec: r.get(7)?,
                unobserved_sec: r.get(8)?,
                note: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// The evidence for one session, and the neutral recommendation it earns.
    pub fn session_evidence(&self, session_id: &str) -> Result<SessionEvidence> {
        let (task_id, task_title, started_at, ended_at, elapsed_sec) = self
            .conn
            .query_row(
                "SELECT s.task_id, t.title, s.started_at, s.ended_at, s.elapsed_sec
                   FROM time_session s JOIN task t ON t.id = s.task_id
                  WHERE s.id = ?1",
                params![session_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                },
            )
            .map_err(|_| AppError::NotFound("session"))?;

        let now = self.now();
        let end = ended_at.unwrap_or(now);
        let duration_sec = elapsed_sec.max(0);

        // Observation overlap, split by what the classifier made of it. Work
        // means Productive; everything else observed is "other". This is
        // evidence, and the names say so.
        /* `counts_as` is the stable roll-up (migration 0007): Work and Study
           are `core`, which is what §16.5 calls PRODUCTIVE. Reading the label
           name instead would break the moment someone renamed a category, and
           reading a `kind` column would break immediately — it does not exist.

           Deliberately NOT wrapped in `unwrap_or(zero)`. A query that cannot
           run must not report "no evidence": that is indistinguishable from a
           quiet hour, and it would put a session at the top of the review
           queue for a reason that is a bug. */
        let (work_ms, other_ms, idle_ms) = self.conn.query_row(
            "SELECT
               COALESCE(SUM(CASE WHEN a.is_idle = 0 AND c.counts_as = 'core'
                                 THEN MIN(a.ended_at, ?2) - MAX(a.started_at, ?1) END), 0),
               COALESCE(SUM(CASE WHEN a.is_idle = 0
                                  AND (c.counts_as IS NULL OR c.counts_as <> 'core')
                                 THEN MIN(a.ended_at, ?2) - MAX(a.started_at, ?1) END), 0),
               COALESCE(SUM(CASE WHEN a.is_idle = 1
                                 THEN MIN(a.ended_at, ?2) - MAX(a.started_at, ?1) END), 0)
             FROM activity_span a
             LEFT JOIN observation_category c ON c.id = a.category_id
            WHERE a.started_at < ?2 AND a.ended_at > ?1",
            params![started_at, end],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )?;

        let observed_work_sec = (work_ms / 1000).max(0);
        let observed_other_sec = (other_ms / 1000).max(0);
        let idle_sec = (idle_ms / 1000).max(0);
        let unobserved_sec =
            (duration_sec - observed_work_sec - observed_other_sec - idle_sec).max(0);

        let current: Option<(String, Millis)> = self
            .conn
            .query_row(
                "SELECT decision, reviewed_at FROM session_current_review WHERE session_id = ?1",
                params![session_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        Ok(SessionEvidence {
            strength: strength(duration_sec, observed_work_sec, observed_other_sec, unobserved_sec + idle_sec),
            focus_sec: self.focus_sec_for_session(session_id)?,
            decision: current.as_ref().and_then(|(d, _)| ReviewDecision::parse(d)),
            reviewed_at: current.map(|(_, at)| at),
            session_id: session_id.to_string(),
            task_id,
            task_title,
            started_at,
            ended_at,
            duration_sec,
            observed_work_sec,
            observed_other_sec,
            idle_sec,
            unobserved_sec,
        })
    }

    /// Invariant 21 — editing or judging a session reopens every closed day
    /// review it touches.
    fn reopen_days_touching(&mut self, session_id: &str) -> Result<()> {
        let bounds: Option<(Millis, Option<Millis>)> = self
            .conn
            .query_row(
                "SELECT started_at, ended_at FROM time_session WHERE id = ?1",
                params![session_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((from, to)) = bounds else { return Ok(()) };
        let tz = self.tz_or("UTC");
        let zone = crate::time::zone(&tz)?;
        let first = crate::time::local_date(from, &zone);
        let last = crate::time::local_date(to.unwrap_or(from), &zone);
        self.conn.execute(
            "DELETE FROM day_review WHERE local_date >= ?1 AND local_date <= ?2",
            params![first, last],
        )?;
        Ok(())
    }
}

/// §7.1 — a neutral recommendation that orders the queue and explains itself.
/// It cannot verify anything; no branch here writes a decision.
fn strength(duration: i64, work: i64, other: i64, thin: i64) -> EvidenceStrength {
    if duration <= 0 {
        return EvidenceStrength::Little;
    }
    let ratio = |v: i64| v as f64 / duration as f64;
    if ratio(thin) > 0.5 {
        EvidenceStrength::Little
    } else if ratio(other) > 0.5 && ratio(other) > ratio(work) {
        EvidenceStrength::Conflicting
    } else if ratio(work) >= 0.6 {
        EvidenceStrength::Strong
    } else {
        EvidenceStrength::Mixed
    }
}
