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
    /// The open segment, if one is open. `None` during an idle challenge or a
    /// break — the run continues, but nothing is being recorded.
    pub session_id: Option<String>,
    /// What this run is timing. Held across segment boundaries so a resume
    /// knows what to reopen.
    task_id: Option<String>,
    block_id: Option<String>,
    /// Counted milliseconds banked for the whole run (all segments).
    run_ms: i64,
    /// Counted milliseconds banked for the *current* segment only. This is
    /// what gets written to the session row.
    segment_ms: i64,
    /// Monotonic reading at which the current counting stretch began.
    resumed_mono: Option<i64>,
    last_wall: i64,
    last_mono: i64,
    last_heartbeat: i64,
    /// The span awaiting a keep/discard decision.
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
            task_id: None,
            block_id: None,
            run_ms: 0,
            segment_ms: 0,
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
    /// Counted time for the whole run — what the timer chip shows, so it does
    /// not reset to zero when a segment closes.
    fn run_elapsed_ms(&self, mono_now: i64) -> i64 {
        self.run_ms + self.live_ms(mono_now)
    }

    /// Counted time for the open segment — what gets written to its row.
    fn segment_elapsed_ms(&self, mono_now: i64) -> i64 {
        self.segment_ms + self.live_ms(mono_now)
    }

    fn live_ms(&self, mono_now: i64) -> i64 {
        match self.resumed_mono {
            Some(r) => (mono_now - r).max(0),
            None => 0,
        }
    }

    fn pause(&mut self, mono_now: i64) {
        if let Some(r) = self.resumed_mono.take() {
            let live = (mono_now - r).max(0);
            self.run_ms += live;
            self.segment_ms += live;
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
        let running = self.timer.task_id.is_some();
        let run_sec = (self.timer.run_elapsed_ms(mono) / 1000).max(self.timer.floor_sec);
        let session = match &self.timer.session_id {
            Some(id) => self.session_row(id).ok(),
            None => None,
        };
        Ok(TimerState {
            phase: self.timer.phase,
            run_task_id: self.timer.task_id.clone(),
            task_title: self.timer.task_id.as_ref().and_then(|id| {
                self.conn
                    .query_row("SELECT title FROM task WHERE id = ?1", [id], |r| r.get(0))
                    .ok()
            }),
            session,
            // The run total, not the segment: a segment boundary is a
            // bookkeeping detail, and a chip that resets to 00:00 when you come
            // back from a meeting is a chip nobody trusts.
            elapsed_sec: if running { run_sec } else { 0 },
            segment_elapsed_sec: if running {
                self.timer.segment_elapsed_ms(mono) / 1000
            } else {
                0
            },
            idle_from: self.timer.pending_span.map(|(a, _)| a),
            idle_to: self.timer.pending_span.map(|(_, b)| b),
            recovery_session_id: self.timer.recovery_session_id.clone(),
            pomodoro: self.timer.pomodoro,
            focus_ends_at: if running { self.focus_ends_at() } else { None },
        })
    }

    /// Opens a new segment for the current run and marks it running.
    ///
    /// Every segment is one contiguous *awake* interval, so its `started_at`
    /// and `ended_at` are real system-clock instants with no sleep inside them.
    fn open_segment(&mut self, now: Millis) -> Result<()> {
        let (Some(task_id), block_id) = (self.timer.task_id.clone(), self.timer.block_id.clone())
        else {
            return Ok(());
        };
        let id = new_id();
        let tx = self.conn.transaction()?;
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
        tx.commit()?;
        self.timer.session_id = Some(id);
        self.timer.segment_ms = 0;
        self.timer.last_heartbeat = now;
        self.timer.resume(self.clock.mono_ms());
        self.timer.phase = TimerPhase::Running;
        Ok(())
    }

    /// Closes the open segment at `end`, a wall-clock instant the machine is
    /// known to have been awake for.
    ///
    /// A segment with nothing counted in it is deleted rather than kept: an
    /// empty interval in the Sessions tab is noise, not a record.
    fn close_segment(&mut self, end: Millis) -> Result<()> {
        let Some(id) = self.timer.session_id.take() else {
            return Ok(());
        };
        self.timer.pause(self.clock.mono_ms());
        let elapsed_sec = self.timer.segment_ms / 1000;

        let tx = self.conn.transaction()?;
        if elapsed_sec <= 0 {
            tx.execute("DELETE FROM time_session WHERE id = ?1", [&id])?;
        } else {
            tx.execute(
                "UPDATE time_session
                    SET ended_at = ?2, elapsed_sec = ?3, heartbeat_at = ?2, updated_at = ?2
                  WHERE id = ?1",
                params![id, end.max(0), elapsed_sec],
            )?;
        }
        tx.execute(
            "UPDATE app_state SET running_session_id = NULL WHERE id = 1",
            [],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        self.timer.segment_ms = 0;
        Ok(())
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
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM task WHERE id = ?1 AND deleted_at IS NULL",
            [task_id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(AppError::NotFound("task"));
        }

        let now = self.now();
        // Close whatever was running at the real instant it stopped, then begin
        // a new run. The singleton is cleared and re-set inside each half, so
        // there is never a moment with two open sessions (§6.5).
        self.close_segment(now)?;

        let pomodoro = self.timer.pomodoro;
        self.timer = TimerRuntime {
            phase: TimerPhase::Running,
            task_id: Some(task_id.to_string()),
            block_id: block_id.map(str::to_string),
            last_wall: now,
            last_mono: self.clock.mono_ms(),
            pomodoro,
            ..TimerRuntime::default()
        };
        self.open_segment(now)?;
        self.timer_state()
    }

    pub fn stop_timer(&mut self) -> Result<TimerState> {
        if self.timer.task_id.is_none() {
            return self.timer_state();
        }
        // The end of a session is a system-clock instant, always — never a
        // number derived from the counter.
        self.close_segment(self.now())?;
        // An intended length belongs to the run that had it. Leaving it behind
        // would have the next timer inherit a deadline nobody set.
        self.clear_focus()?;
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
            // The machine slept. Close the segment at the last moment we know
            // it was awake, so no session ever spans the gap — a session
            // reading 09:00–12:10 for twenty minutes of work is exactly the
            // "wrong data" a meeting-shaped sleep used to produce.
            let slept_from = now - divergence;
            self.timer.pause(mono);
            self.close_segment(self.timer.last_heartbeat.max(slept_from))?;
            self.timer.pending_span = Some((slept_from, now));
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
                // The counter *has* been running through the idle span, so it
                // is rolled back first; then the segment closes at the last
                // input, which is the last instant the record can vouch for.
                self.timer.pause(mono);
                self.timer.run_ms = (self.timer.run_ms - idle_ms).max(0);
                self.timer.segment_ms = (self.timer.segment_ms - idle_ms).max(0);
                self.timer.floor_sec = 0; // an authorised trim resets the floor
                self.close_segment(report.last_input_at)?;
                self.timer.pending_span = Some((report.last_input_at, now));
                self.timer.pending_ms = idle_ms;
                self.timer.phase = TimerPhase::IdleChallenge;
                return self.timer_state();
            }
        }

        let run_sec = (self.timer.run_elapsed_ms(mono) / 1000).max(self.timer.floor_sec);
        self.timer.floor_sec = run_sec;
        let segment_sec = self.timer.segment_elapsed_ms(mono) / 1000;

        if now - self.timer.last_heartbeat >= HEARTBEAT_EVERY_MS {
            if let Some(id) = &self.timer.session_id {
                self.conn.execute(
                    "UPDATE time_session SET heartbeat_at = ?2, elapsed_sec = ?3, updated_at = ?2
                      WHERE id = ?1",
                    params![id, now, segment_sec],
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
        let now = self.now();
        let mono = self.clock.mono_ms();
        let pending = self.timer.pending_ms;

        match action {
            // The span counts after all: reopen the segment that was closed at
            // the boundary and fold the span back into it, so the record shows
            // one continuous interval rather than a suspicious pair.
            IdleAction::Keep => {
                // Reopen first: closing the segment banked its counted time
                // into the row and zeroed the runtime figure, so the reopen has
                // to restore it before the kept span is added on top.
                self.reopen_last_segment()?;
                self.timer.run_ms += pending;
                self.timer.segment_ms += pending;
                self.timer.phase = TimerPhase::Running;
                self.timer.resume(mono);
            }
            // The honest default. A fresh segment starts now, so the gap is
            // visible in the record as a gap.
            IdleAction::Discard => {
                self.open_segment(now)?;
            }
            IdleAction::AssignToBreak => {
                self.timer.phase = TimerPhase::Break;
            }
        }
        self.timer.pending_span = None;
        self.timer.pending_ms = 0;
        self.timer.last_wall = now;
        self.timer.last_mono = mono;
        self.timer.last_heartbeat = now;
        self.timer.floor_sec = (self.timer.run_elapsed_ms(mono) / 1000).max(0);
        self.timer_state()
    }

    /// Undoes the most recent split for this run, for `IdleAction::Keep`.
    fn reopen_last_segment(&mut self) -> Result<()> {
        let Some(task_id) = self.timer.task_id.clone() else {
            return Ok(());
        };
        let last: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM time_session
                  WHERE task_id = ?1 AND source = 'timer'
                  ORDER BY started_at DESC LIMIT 1",
                [&task_id],
                |r| r.get(0),
            )
            .ok();
        match last {
            Some(id) => {
                let banked: i64 = self
                    .conn
                    .query_row(
                        "SELECT elapsed_sec FROM time_session WHERE id = ?1",
                        [&id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                self.timer.segment_ms = banked * 1000;
                self.conn.execute(
                    "UPDATE time_session SET ended_at = NULL, updated_at = ?2 WHERE id = ?1",
                    params![id, self.now()],
                )?;
                self.conn.execute(
                    "UPDATE app_state SET running_session_id = ?1 WHERE id = 1",
                    [&id],
                )?;
                self.timer.session_id = Some(id);
                Ok(())
            }
            // The segment was empty and got deleted; just start a new one.
            None => self.open_segment(self.now() - self.timer.pending_ms),
        }
    }

    pub fn resume_from_break(&mut self) -> Result<TimerState> {
        if self.timer.phase == TimerPhase::Break && self.timer.task_id.is_some() {
            self.open_segment(self.now())?;
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
                        s.elapsed_sec, s.heartbeat_at, s.source, s.is_confirmed, s.note,
                        s.contribution
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
                    s.elapsed_sec, s.heartbeat_at, s.source, s.is_confirmed, s.note,
                        s.contribution
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
        if input.ended_at <= input.started_at {
            // Equal used to be allowed and produced a zero-length row: invisible
            // on the Day view, counted nowhere, and impossible to select in
            // order to delete. Nothing wants one.
            return Err(AppError::invalid("A session has to end after it starts."));
        }
        let duration = (input.ended_at - input.started_at) / 1000;
        if duration > 24 * 3600 {
            return Err(AppError::invalid(
                "Sessions are capped at 24 hours. Split it into days.",
            ));
        }
        let now = self.now();
        crate::time::check_plausible(input.started_at, now)?;

        // Before the insert, not after: clearing afterwards would delete the
        // row that was just written, since it is itself inside the interval.
        if input.replace_existing {
            self.clear_interval(input.started_at, input.ended_at)?;
        }

        let id = new_id();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO time_session
               (id, task_id, block_id, started_at, ended_at, elapsed_sec, heartbeat_at,
                source, is_confirmed, note, contribution, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,NULL,'manual',1,?7,?8,?9,?10,?10)",
            params![
                id,
                input.task_id,
                input.block_id,
                input.started_at,
                input.ended_at,
                duration,
                input.note,
                input.contribution.map(|c| c.as_str()),
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
        if let Some(task_id) = &patch.task_id {
            validate_id(task_id, "task")?;
            let exists: i64 = tx.query_row(
                "SELECT COUNT(*) FROM task WHERE id = ?1 AND deleted_at IS NULL",
                [task_id],
                |r| r.get(0),
            )?;
            if exists == 0 {
                return Err(AppError::invalid("That task doesn't exist."));
            }
            tx.execute(
                "UPDATE time_session SET task_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, task_id, now],
            )?;
            // A block belongs to a task, and `block_tracked` sums every session
            // carrying its id — so a session moved to task B while still
            // attached to task A's block would credit B's minutes to A's plot
            // and put a drift rail on a block nobody worked. Moving to a
            // different task detaches the block unless the block is the new
            // task's own.
            if task_id != &current.task_id {
                tx.execute(
                    "UPDATE time_session
                        SET block_id = NULL, updated_at = ?2
                      WHERE id = ?1
                        AND block_id IS NOT NULL
                        AND block_id NOT IN (SELECT id FROM scheduled_block WHERE task_id = ?3)",
                    params![id, now, task_id],
                )?;
            }
        }
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
        // Work records only (Plan Rev 3 §7). `life_entry` has no counterpart,
        // which is what makes "never on personal time" structural.
        contribution: r
            .get::<_, Option<String>>(11)?
            .as_deref()
            .and_then(Contribution::parse),
    })
}

/// Focus sessions (PLAN-WEEKLY-GOALS.md W3).
///
/// Fruit's timer runs until stopped. Starting **"45 minutes on this"** is a
/// different act from starting a stopwatch, and it is the act people perform
/// dozens of times a week.
///
/// # The intended length is a plot, and the overrun is real
///
/// Starting a focus session creates a block at the current instant. That is not
/// bookkeeping — it is the only honest place to put an intention in an app that
/// separates plan from record. Everything downstream then works already: the
/// drift rail, the reconciler, the week's planned-versus-tracked.
///
/// # Extending does not enlarge the plan
///
/// This is the decision worth defending. Rize lets you extend a session with one
/// click, and its extension is free because Rize has no plan to diverge from.
/// Here, growing the block would quietly rewrite what you meant to do — and
/// tomorrow's report would say you planned ninety minutes when you planned
/// forty-five and kept going.
///
/// So `extend_focus` moves **only the moment Fruit next mentions it**. The block
/// keeps its length, the record keeps growing, and drift shows the overrun,
/// which is exactly what happened. One key, no dialog: the moment you are most
/// productive is the moment you least want a decision.
impl Store {
    /// When Fruit next mentions that the intended length has run out.
    pub(crate) fn focus_ends_at(&self) -> Option<Millis> {
        match self.get_setting(FOCUS_ENDS_AT) {
            Ok(Some(serde_json::Value::Number(n))) => n.as_i64(),
            _ => None,
        }
    }

    /// Starts a timed run with an intended length.
    ///
    /// `minutes` of `None` is an ordinary open-ended timer, which is still the
    /// right thing when you genuinely do not know.
    pub fn start_focus(&mut self, task_id: &str, minutes: Option<i64>) -> Result<TimerState> {
        let Some(minutes) = minutes.filter(|m| *m > 0) else {
            self.clear_focus()?;
            return self.start_timer(task_id, None);
        };
        let minutes = minutes.min(8 * 60);
        let now = self.now();
        let tz = self.tz_or("UTC");

        // A block at this instant. `schedule_block` refuses one that crosses
        // midnight, and a focus session started at 23:50 is exactly that — so
        // fall back to an open-ended run rather than refusing to start at all.
        // Someone pressing "start" at midnight wants to work, not to be taught
        // about local dates.
        let block = self
            .schedule_block(NewBlock {
                task_id: Some(task_id.to_string()),
                starts_at: now,
                duration_sec: minutes * 60,
                tz: tz.clone(),
                ..Default::default()
            })
            .ok();
        // Marked, so "how many focus sessions this week, for how long" has a
        // query. Stamped after the insert rather than threaded through
        // `NewBlock`: a focus block is an ordinary block in every way the
        // planner, the reconciler and the drift rail care about, and the only
        // thing that differs is how it was started (migration 0013).
        if let Some(block) = &block {
            self.conn.execute(
                "UPDATE scheduled_block SET is_focus = 1 WHERE id = ?1",
                [&block.id],
            )?;
        }

        let state = self.start_timer(task_id, block.as_ref().map(|b| b.id.as_str()))?;
        self.set_setting(
            FOCUS_ENDS_AT,
            &serde_json::Value::from(now + minutes * 60_000),
        )?;
        self.timer_state().map(|mut s| {
            s.focus_ends_at = state.focus_ends_at.or(Some(now + minutes * 60_000));
            s
        })
    }

    /// "Keep going" — one key, no dialog.
    ///
    /// Moves when Fruit next mentions the session and **nothing else**. The
    /// block is not resized: see the note above.
    pub fn extend_focus(&mut self, minutes: i64) -> Result<TimerState> {
        let minutes = minutes.clamp(1, 8 * 60);
        let from = self.focus_ends_at().unwrap_or_else(|| self.now()).max(self.now());
        self.set_setting(FOCUS_ENDS_AT, &serde_json::Value::from(from + minutes * 60_000))?;
        self.timer_state()
    }

    pub(crate) fn clear_focus(&mut self) -> Result<()> {
        self.set_setting(FOCUS_ENDS_AT, &serde_json::Value::Null)
    }
}

/// When the running focus session's intended length runs out. Cleared when the
/// timer stops.
pub const FOCUS_ENDS_AT: &str = "timer.focusEndsAt";
