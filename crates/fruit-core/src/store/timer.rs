//! The timer state machine (§4.5).
//!
//! ```text
//!   boot ──▶ [recovering] ──resolve──▶ [idle] ──start──▶ [running]
//!                                        ▲                 │
//!                            resolve ── [idle_challenge] ◀──┘
//!                          phase_end ──▶ [break] ──phase_end──▶ [running]
//! ```
//!
//! Elapsed time is counted on the monotonic clock and only *displayed* from
//! the wall clock. That is what makes "the user changed the system clock" and
//! "the laptop slept for 45 minutes" produce honest numbers instead of a
//! tracker that silently keeps three hours of sleep.

use rusqlite::params;

use super::Store;
use crate::db;
use crate::error::{AppError, Result};
use crate::ids::{new_id, validate_id};
use crate::model::*;
use crate::time::Millis;

const HEARTBEAT_EVERY_MS: i64 = 30_000;
/// A wall/monotonic divergence larger than this is a suspend, not scheduler jitter.
const SLEEP_THRESHOLD_MS: i64 = 60_000;

#[derive(Debug, Clone)]
pub struct IdleReport {
    /// Wall-clock instant of the user's last input.
    pub last_input_at: Millis,
}

#[derive(Debug)]
pub struct TimerRuntime {
    pub phase: TimerPhase,
    pub session_id: Option<String>,
    /// Counted milliseconds that are already banked.
    accumulated_ms: i64,
    /// Monotonic reading at which the current counting stretch began.
    resumed_mono: Option<i64>,
    last_wall: i64,
    last_mono: i64,
    last_heartbeat: i64,
    /// The span awaiting a keep/discard decision, and whether it was counted.
    pending_span: Option<(Millis, Millis)>,
    pending_ms: i64,
    recovery_session_id: Option<String>,
    pomodoro: Option<PomodoroState>,
    /// §6.5: elapsed never decreases *while running*. A user-authorised trim
    /// (idle discard, recovery) resets this baseline explicitly.
    floor_sec: i64,
}

impl Default for TimerRuntime {
    fn default() -> Self {
        TimerRuntime {
            phase: TimerPhase::Idle,
            session_id: None,
            accumulated_ms: 0,
            resumed_mono: None,
            last_wall: 0,
            last_mono: 0,
            last_heartbeat: 0,
            pending_span: None,
            pending_ms: 0,
            recovery_session_id: None,
            pomodoro: None,
            floor_sec: 0,
        }
    }
}

impl TimerRuntime {
    fn elapsed_ms(&self, mono_now: i64) -> i64 {
        self.accumulated_ms
            + match self.resumed_mono {
                Some(r) => (mono_now - r).max(0),
                None => 0,
            }
    }

    fn pause(&mut self, mono_now: i64) {
        if let Some(r) = self.resumed_mono.take() {
            self.accumulated_ms += (mono_now - r).max(0);
        }
    }

    fn resume(&mut self, mono_now: i64) {
        if self.resumed_mono.is_none() {
            self.resumed_mono = Some(mono_now);
        }
    }
}

impl Store {
    // ─── state ─────────────────────────────────────────────────────────

    pub fn timer_state(&self) -> Result<TimerState> {
        let mono = self.clock.mono_ms();
        let elapsed_sec = (self.timer.elapsed_ms(mono) / 1000).max(self.timer.floor_sec);
        let session = match &self.timer.session_id {
            Some(id) => self.session_row(id).ok(),
            None => None,
        };
        Ok(TimerState {
            phase: self.timer.phase,
            session,
            elapsed_sec: if self.timer.session_id.is_some() {
                elapsed_sec
            } else {
                0
            },
            idle_from: self.timer.pending_span.map(|(a, _)| a),
            idle_to: self.timer.pending_span.map(|(_, b)| b),
            recovery_session_id: self.timer.recovery_session_id.clone(),
            pomodoro: self.timer.pomodoro,
        })
    }

    /// Boot: an open session means the last run did not end cleanly. No new
    /// timer may start until the user rules on it (§4.5 `recovering`).
    pub fn recover_on_boot(&mut self) -> Result<TimerState> {
        let open: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM time_session WHERE ended_at IS NULL ORDER BY started_at LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok();
        match open {
            Some(id) => {
                self.timer.phase = TimerPhase::Recovering;
                self.timer.recovery_session_id = Some(id);
            }
            None => {
                self.timer = TimerRuntime::default();
                self.conn.execute(
                    "UPDATE app_state SET running_session_id = NULL WHERE id = 1",
                    [],
                )?;
            }
        }
        self.timer_state()
    }

    pub fn resolve_recovery(&mut self, id: &str, action: RecoveryAction) -> Result<TimerState> {
        validate_id(id, "session")?;
        let (started_at, heartbeat_at, elapsed_sec): (i64, Option<i64>, i64) = self
            .conn
            .query_row(
                "SELECT started_at, heartbeat_at, elapsed_sec FROM time_session WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|_| AppError::NotFound("session"))?;
        let now = self.now();

        let tx = self.conn.transaction()?;
        match action {
            RecoveryAction::TrimToHeartbeat => {
                // Default (U7): believe the last proof-of-life, not the wall
                // clock. Silently keeping three hours of sleep is how a tracker
                // permanently loses trust.
                let end = heartbeat_at.unwrap_or(started_at).max(started_at);
                let trimmed = elapsed_sec.min((end - started_at) / 1000);
                tx.execute(
                    "UPDATE time_session
                        SET ended_at = ?2, elapsed_sec = ?3, source = 'recovered',
                            is_confirmed = 0, updated_at = ?4
                      WHERE id = ?1",
                    params![id, end, trimmed, now],
                )?;
            }
            RecoveryAction::KeepAll => {
                let end = now;
                tx.execute(
                    "UPDATE time_session
                        SET ended_at = ?2, elapsed_sec = ?3, source = 'recovered',
                            is_confirmed = 0, updated_at = ?4
                      WHERE id = ?1",
                    params![id, end, (end - started_at) / 1000, now],
                )?;
            }
            RecoveryAction::Discard => {
                tx.execute("DELETE FROM time_session WHERE id = ?1", [id])?;
            }
        }
        tx.execute(
            "UPDATE app_state SET running_session_id = NULL WHERE id = 1",
            [],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;

        self.timer = TimerRuntime::default();
        self.timer_state()
    }

    // ─── start / stop ──────────────────────────────────────────────────

    /// One transaction: stop whatever was running, open the new session, update
    /// the singleton. Three writes that must never land separately (§6.8).
    pub fn start_timer(&mut self, task_id: &str, block_id: Option<&str>) -> Result<TimerState> {
        validate_id(task_id, "task")?;
        if let Some(b) = block_id {
            validate_id(b, "block")?;
        }
        if self.timer.phase == TimerPhase::Recovering {
            return Err(AppError::RecoveryPending);
        }
        let title: String = self
            .conn
            .query_row(
                "SELECT title FROM task WHERE id = ?1 AND deleted_at IS NULL",
                [task_id],
                |r| r.get(0),
            )
            .map_err(|_| AppError::NotFound("task"))?;
        let _ = title;

        let now = self.now();
        let mono = self.clock.mono_ms();
        let elapsed_ms = self.timer.elapsed_ms(mono);
        let previous = self.timer.session_id.clone();

        let id = new_id();
        let tx = self.conn.transaction()?;
        if let Some(prev) = &previous {
            close_session(&tx, prev, now, elapsed_ms / 1000)?;
        }
        tx.execute(
            "INSERT INTO time_session
               (id, task_id, block_id, started_at, ended_at, elapsed_sec, heartbeat_at,
                source, is_confirmed, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,NULL,0,?4,'timer',1,?5,?4,?4)",
            params![id, task_id, block_id, now, self.device_id],
        )?;
        tx.execute(
            "UPDATE app_state SET running_session_id = ?1 WHERE id = 1",
            [&id],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;

        self.timer = TimerRuntime {
            phase: TimerPhase::Running,
            session_id: Some(id),
            resumed_mono: Some(mono),
            last_wall: now,
            last_mono: mono,
            last_heartbeat: now,
            pomodoro: self.timer.pomodoro,
            ..TimerRuntime::default()
        };
        self.timer_state()
    }

    pub fn stop_timer(&mut self) -> Result<TimerState> {
        let Some(id) = self.timer.session_id.clone() else {
            return self.timer_state();
        };
        let now = self.now();
        let mono = self.clock.mono_ms();
        self.timer.pause(mono);
        let elapsed_sec = (self.timer.accumulated_ms / 1000).max(self.timer.floor_sec);

        let tx = self.conn.transaction()?;
        close_session(&tx, &id, now, elapsed_sec)?;
        tx.execute(
            "UPDATE app_state SET running_session_id = NULL WHERE id = 1",
            [],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;

        let pomodoro = self.timer.pomodoro;
        self.timer = TimerRuntime {
            pomodoro,
            ..TimerRuntime::default()
        };
        self.timer_state()
    }

    /// Called once a second by the shell while running — never by the renderer
    /// (§6.9). Writes a heartbeat every 30s and detects suspend.
    pub fn tick(&mut self, idle: Option<IdleReport>) -> Result<TimerState> {
        if self.timer.phase != TimerPhase::Running {
            return self.timer_state();
        }
        let now = self.now();
        let mono = self.clock.mono_ms();

        // Suspend detection: wall time ran on, monotonic time did not (§4.5
        // `sleep_resume`). The accumulator already excluded the gap, so the
        // default of "not counted" costs nothing — we only have to ask.
        let wall_delta = now - self.timer.last_wall;
        let mono_delta = mono - self.timer.last_mono;
        let divergence = wall_delta - mono_delta;
        self.timer.last_wall = now;
        self.timer.last_mono = mono;

        if divergence > SLEEP_THRESHOLD_MS {
            self.timer.pause(mono);
            self.timer.pending_span = Some((now - divergence, now));
            self.timer.pending_ms = divergence;
            self.timer.phase = TimerPhase::IdleChallenge;
            return self.timer_state();
        }

        // Input idle: the accumulator *has* been counting, so entering the
        // challenge trims it back to the last input and offers to add it again.
        if let Some(report) = idle {
            let idle_ms = now - report.last_input_at;
            let threshold = self.idle_threshold_ms();
            if idle_ms >= threshold {
                self.timer.pause(mono);
                self.timer.accumulated_ms = (self.timer.accumulated_ms - idle_ms).max(0);
                self.timer.pending_span = Some((report.last_input_at, now));
                self.timer.pending_ms = idle_ms;
                self.timer.phase = TimerPhase::IdleChallenge;
                self.timer.floor_sec = 0; // an authorised trim resets the floor
                return self.timer_state();
            }
        }

        let elapsed_sec = (self.timer.elapsed_ms(mono) / 1000).max(self.timer.floor_sec);
        self.timer.floor_sec = elapsed_sec;

        if now - self.timer.last_heartbeat >= HEARTBEAT_EVERY_MS {
            if let Some(id) = &self.timer.session_id {
                self.conn.execute(
                    "UPDATE time_session SET heartbeat_at = ?2, elapsed_sec = ?3, updated_at = ?2
                      WHERE id = ?1",
                    params![id, now, elapsed_sec],
                )?;
                let tx = self.conn.transaction()?;
                db::rebuild_tracked_caches(&tx)?;
                tx.commit()?;
            }
            self.timer.last_heartbeat = now;
        }
        self.timer_state()
    }

    fn idle_threshold_ms(&self) -> i64 {
        match self.get_setting("timer.idleThresholdSec") {
            Ok(Some(serde_json::Value::Number(n))) => n.as_i64().unwrap_or(300) * 1000,
            _ => 300_000,
        }
    }

    /// §4.5: discarding is the honest default; keeping is one keystroke.
    pub fn resolve_idle(&mut self, action: IdleAction) -> Result<TimerState> {
        if self.timer.phase != TimerPhase::IdleChallenge {
            return self.timer_state();
        }
        let mono = self.clock.mono_ms();
        match action {
            IdleAction::Keep => {
                self.timer.accumulated_ms += self.timer.pending_ms;
                self.timer.phase = TimerPhase::Running;
                self.timer.resume(mono);
            }
            IdleAction::Discard => {
                self.timer.phase = TimerPhase::Running;
                self.timer.resume(mono);
            }
            IdleAction::AssignToBreak => {
                self.timer.phase = TimerPhase::Break;
            }
        }
        self.timer.pending_span = None;
        self.timer.pending_ms = 0;
        self.timer.last_wall = self.now();
        self.timer.last_mono = mono;
        self.timer.floor_sec = (self.timer.elapsed_ms(mono) / 1000).max(0);
        self.timer_state()
    }

    pub fn resume_from_break(&mut self) -> Result<TimerState> {
        if self.timer.phase == TimerPhase::Break && self.timer.session_id.is_some() {
            self.timer.phase = TimerPhase::Running;
            self.timer.resume(self.clock.mono_ms());
        }
        self.timer_state()
    }

    // ─── pomodoro ──────────────────────────────────────────────────────

    pub fn start_pomodoro(&mut self, work_sec: i64, cycles_before_long: i64) -> Result<TimerState> {
        self.timer.pomodoro = Some(PomodoroState {
            phase: PomodoroPhase::Work,
            cycle: 1,
            cycles_before_long,
            phase_ends_at: self.now() + work_sec * 1000,
        });
        self.timer_state()
    }

    /// Advances the cycle when the current phase has run out. Returns the new
    /// phase when it changed, so the shell can fire exactly one notification
    /// (§3.11 — Fruit may interrupt only for something the user armed).
    pub fn pomodoro_tick(
        &mut self,
        work_sec: i64,
        short_sec: i64,
        long_sec: i64,
    ) -> Result<Option<PomodoroPhase>> {
        let now = self.now();
        let Some(state) = self.timer.pomodoro else {
            return Ok(None);
        };
        if now < state.phase_ends_at {
            return Ok(None);
        }
        let (next_phase, duration, cycle) = match state.phase {
            PomodoroPhase::Work => {
                if state.cycle % state.cycles_before_long == 0 {
                    (PomodoroPhase::LongBreak, long_sec, state.cycle)
                } else {
                    (PomodoroPhase::ShortBreak, short_sec, state.cycle)
                }
            }
            _ => (PomodoroPhase::Work, work_sec, state.cycle + 1),
        };
        self.timer.pomodoro = Some(PomodoroState {
            phase: next_phase,
            cycle,
            cycles_before_long: state.cycles_before_long,
            phase_ends_at: now + duration * 1000,
        });
        Ok(Some(next_phase))
    }

    // ─── sessions as records ───────────────────────────────────────────

    pub(crate) fn session_row(&self, id: &str) -> Result<SessionRow> {
        self.conn
            .query_row(
                "SELECT s.id, s.task_id, t.title, s.block_id, s.started_at, s.ended_at,
                        s.elapsed_sec, s.heartbeat_at, s.source, s.is_confirmed, s.note
                   FROM time_session s JOIN task t ON t.id = s.task_id
                  WHERE s.id = ?1",
                [id],
                map_session,
            )
            .map_err(|_| AppError::NotFound("session"))
    }

    pub(crate) fn sessions_for_task(&self, task_id: &str) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.task_id, t.title, s.block_id, s.started_at, s.ended_at,
                    s.elapsed_sec, s.heartbeat_at, s.source, s.is_confirmed, s.note
               FROM time_session s JOIN task t ON t.id = s.task_id
              WHERE s.task_id = ?1 ORDER BY s.started_at DESC",
        )?;
        let rows = stmt.query_map([task_id], map_session)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Users *will* forget to start the timer. Without manual entry the record
    /// is untrustworthy and the whole loop dies (§2.3).
    pub fn add_session(&mut self, input: ManualSession) -> Result<SessionRow> {
        validate_id(&input.task_id, "task")?;
        if let Some(b) = &input.block_id {
            validate_id(b, "block")?;
        }
        if input.ended_at < input.started_at {
            return Err(AppError::invalid("A session can't end before it starts."));
        }
        let duration = (input.ended_at - input.started_at) / 1000;
        if duration > 24 * 3600 {
            return Err(AppError::invalid(
                "Sessions are capped at 24 hours. Split it into days.",
            ));
        }
        let now = self.now();
        crate::time::check_plausible(input.started_at, now)?;
        let id = new_id();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO time_session
               (id, task_id, block_id, started_at, ended_at, elapsed_sec, heartbeat_at,
                source, is_confirmed, note, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,NULL,'manual',1,?7,?8,?9,?9)",
            params![
                id,
                input.task_id,
                input.block_id,
                input.started_at,
                input.ended_at,
                duration,
                input.note,
                self.device_id,
                now
            ],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        self.session_row(&id)
    }

    pub fn update_session(&mut self, id: &str, patch: SessionPatch) -> Result<SessionRow> {
        validate_id(id, "session")?;
        let current = self.session_row(id)?;
        if self.timer.session_id.as_deref() == Some(id) {
            return Err(AppError::invalid(
                "That session is still running. Stop the timer before editing it.",
            ));
        }
        let started_at = patch.started_at.unwrap_or(current.started_at);
        let ended_at = patch.ended_at.or(current.ended_at);
        if let Some(end) = ended_at {
            if end < started_at {
                return Err(AppError::invalid("A session can't end before it starts."));
            }
        }
        let elapsed = ended_at
            .map(|e| (e - started_at) / 1000)
            .unwrap_or(current.elapsed_sec);
        let now = self.now();

        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE time_session
                SET started_at = ?2, ended_at = ?3, elapsed_sec = ?4, updated_at = ?5,
                    is_confirmed = ?6
              WHERE id = ?1",
            params![
                id,
                started_at,
                ended_at,
                elapsed,
                now,
                patch.is_confirmed.unwrap_or(true) as i64
            ],
        )?;
        if let Some(block_id) = &patch.block_id {
            tx.execute(
                "UPDATE time_session SET block_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, block_id, now],
            )?;
        }
        if let Some(note) = &patch.note {
            tx.execute(
                "UPDATE time_session SET note = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, note, now],
            )?;
        }
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        self.session_row(id)
    }

    pub fn delete_session(&mut self, id: &str) -> Result<UndoToken> {
        validate_id(id, "session")?;
        if self.timer.session_id.as_deref() == Some(id) {
            return Err(AppError::invalid(
                "That session is still running. Stop the timer first.",
            ));
        }
        let row: serde_json::Value = self.conn.query_row(
            "SELECT json_object(
                'id', id, 'task_id', task_id, 'block_id', block_id, 'started_at', started_at,
                'ended_at', ended_at, 'elapsed_sec', elapsed_sec, 'heartbeat_at', heartbeat_at,
                'source', source, 'is_confirmed', is_confirmed, 'note', note,
                'device_id', device_id, 'created_at', created_at)
             FROM time_session WHERE id = ?1",
            [id],
            |r| {
                let s: String = r.get(0)?;
                Ok(serde_json::from_str(&s).unwrap_or(serde_json::Value::Null))
            },
        )?;
        let now = self.now();
        // Sessions are hard rows — a soft-deleted session would keep counting in
        // the views. The tombstone lives in `setting` for the undo window.
        self.set_setting(&format!("undo.session.{id}"), &row)?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM time_session WHERE id = ?1", [id])?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        Ok(UndoToken {
            entity: "session".into(),
            id: id.to_string(),
            label: "Deleted session".into(),
            at: now,
        })
    }
}

fn close_session(
    tx: &rusqlite::Transaction,
    id: &str,
    now: i64,
    elapsed_sec: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE time_session
            SET ended_at = ?2, elapsed_sec = MAX(?3, 0), heartbeat_at = ?2, updated_at = ?2
          WHERE id = ?1 AND ended_at IS NULL",
        params![id, now, elapsed_sec],
    )?;
    Ok(())
}

fn map_session(r: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        task_id: r.get(1)?,
        task_title: r.get(2)?,
        block_id: r.get(3)?,
        started_at: r.get(4)?,
        ended_at: r.get(5)?,
        elapsed_sec: r.get(6)?,
        heartbeat_at: r.get(7)?,
        source: r.get(8)?,
        is_confirmed: r.get::<_, i64>(9)? == 1,
        note: r.get(10)?,
    })
}
