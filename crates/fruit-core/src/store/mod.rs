//! The command layer (§6.8).
//!
//! Commands are **intent-based, not CRUD**, which is what makes the invariants
//! in §6.5 enforceable: `start_timer` is one transaction that stops any running
//! session, opens a new one and updates the singleton — three writes that must
//! never land separately.
//!
//! Nothing above this layer holds SQL.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Connection;

use crate::clock::{Clock, SystemClock};
use crate::db;
use crate::error::{AppError, Result};
use crate::model::*;
use crate::time::now_ms;

mod activity;
mod blocks;
mod data;
mod day;
mod projects;
mod reconcile;
mod life;
mod recurrence;
mod reports;
mod search;
mod seed;
mod tasks;
mod timer;
mod week;

pub use activity::{ENABLED as ACTIVITY_ENABLED, PAUSED as ACTIVITY_PAUSED, SAMPLE_INTERVAL_MS};
pub use day::{resolve_day, Segment, SegmentOwner};
pub use recurrence::{IcsImportSummary, SeriesScope, HORIZON_DAYS};
pub use timer::{IdleReport, TimerRuntime};

pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) device_id: String,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) timer: TimerRuntime,
    pub(crate) db_path: Option<PathBuf>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = db::open(path)?;
        db::migrate(&mut conn, Some(path))?;
        Self::finish(conn, Some(path.to_path_buf()), Arc::new(SystemClock::default()))
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::in_memory_with_clock(Arc::new(SystemClock::default()))
    }

    pub fn in_memory_with_clock(clock: Arc<dyn Clock>) -> Result<Self> {
        let mut conn = db::open_in_memory()?;
        db::migrate(&mut conn, None)?;
        Self::finish(conn, None, clock)
    }

    fn finish(conn: Connection, db_path: Option<PathBuf>, clock: Arc<dyn Clock>) -> Result<Self> {
        let device_id = db::ensure_device_id(&conn)?;
        let mut store = Store {
            conn,
            device_id,
            clock,
            timer: TimerRuntime::default(),
            db_path,
        };
        store.repair_local_dates()?;
        // Idempotent, and deliberately not inside `seed_first_run`: a database
        // created before migration 0005 has no life areas and would otherwise
        // never acquire them, leaving the Day view with nowhere to file
        // non-work time.
        store.seed_life_areas()?;
        Ok(store)
    }

    pub fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    // ─── settings (§6.8 typed key/value) ───────────────────────────────

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .ok();
        Ok(match raw {
            Some(s) => Some(serde_json::from_str(&s)?),
            None => None,
        })
    }

    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO setting (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, value.to_string(), self.now()],
        )?;
        Ok(())
    }

    pub fn all_settings(&self) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM setting")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut map = serde_json::Map::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, serde_json::from_str(&v).unwrap_or(serde_json::Value::Null));
        }
        Ok(map)
    }

    /// Preferred timezone: the setting if the user pinned one, otherwise the
    /// zone the shell reports. Blocks always store the zone they were made in.
    pub fn tz_or(&self, fallback: &str) -> String {
        match self.get_setting("general.tz") {
            Ok(Some(serde_json::Value::String(s))) => s,
            _ => fallback.to_string(),
        }
    }

    // ─── soft delete / undo (§4.6, §6.1 rule 6) ────────────────────────

    /// `restore` is the inverse half of every `UndoToken` the delete commands
    /// hand back. One entry point, so the undo stack has one shape.
    pub fn restore(&mut self, token: &UndoToken) -> Result<()> {
        let table = match token.entity.as_str() {
            "task" => "task",
            "project" => "project",
            "block" => "scheduled_block",
            "series" => return self.restore_series(&token.id),
            "life_entry" => return self.restore_life_entry(&token.id),
            "life_area" => return self.restore_life_area(&token.id),
            "session" => return self.restore_session(&token.id),
            other => return Err(AppError::invalid(format!("Cannot restore a {other}."))),
        };
        let now = self.now();
        let n = self.conn.execute(
            &format!("UPDATE {table} SET deleted_at = NULL, updated_at = ?2 WHERE id = ?1"),
            rusqlite::params![token.id, now],
        )?;
        if n == 0 {
            return Err(AppError::NotFound("row"));
        }
        if table == "scheduled_block" || table == "task" {
            let tx = self.conn.transaction()?;
            db::rebuild_tracked_caches(&tx)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Sessions are hard rows (a deleted session must not keep counting), so
    /// undo re-inserts from the tombstone kept in the undo payload.
    fn restore_session(&mut self, id: &str) -> Result<()> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM setting WHERE key = ?1",
                [format!("undo.session.{id}")],
                |r| r.get(0),
            )
            .ok();
        let raw = raw.ok_or(AppError::NotFound("deleted session"))?;
        let v: serde_json::Value = serde_json::from_str(&raw)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO time_session
               (id, task_id, block_id, started_at, ended_at, elapsed_sec, heartbeat_at,
                source, is_confirmed, note, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            rusqlite::params![
                v["id"].as_str(),
                v["task_id"].as_str(),
                v["block_id"].as_str(),
                v["started_at"].as_i64(),
                v["ended_at"].as_i64(),
                v["elapsed_sec"].as_i64().unwrap_or(0),
                v["heartbeat_at"].as_i64(),
                v["source"].as_str().unwrap_or("manual"),
                v["is_confirmed"].as_i64().unwrap_or(1),
                v["note"].as_str(),
                v["device_id"].as_str().unwrap_or(""),
                v["created_at"].as_i64().unwrap_or(0),
                now_ms(),
            ],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        self.conn.execute(
            "DELETE FROM setting WHERE key = ?1",
            [format!("undo.session.{id}")],
        )?;
        Ok(())
    }

    /// Recently deleted (§3.2), 30-day window.
    pub fn deleted_rows(&self) -> Result<Vec<DeletedRow>> {
        let mut out = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT 'task', id, title, deleted_at FROM task WHERE deleted_at IS NOT NULL
             UNION ALL
             SELECT 'project', id, name, deleted_at FROM project WHERE deleted_at IS NOT NULL
             UNION ALL
             SELECT 'block', id, COALESCE(label, 'Scheduled block'), deleted_at
               FROM scheduled_block WHERE deleted_at IS NOT NULL
             ORDER BY 4 DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DeletedRow {
                entity: r.get(0)?,
                id: r.get(1)?,
                title: r.get(2)?,
                deleted_at: r.get(3)?,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Nightly purge (§6.7): soft-deleted rows older than 30 days.
    pub fn purge_expired(&mut self) -> Result<i64> {
        let now = self.now();
        let cutoff = now - 30 * 24 * 60 * 60 * 1000;
        let tx = self.conn.transaction()?;
        let mut n = 0;
        n += tx.execute("DELETE FROM scheduled_block WHERE deleted_at < ?1", [cutoff])?;
        n += tx.execute("DELETE FROM task WHERE deleted_at < ?1", [cutoff])?;
        n += tx.execute("DELETE FROM project WHERE deleted_at < ?1", [cutoff])?;
        tx.execute(
            "UPDATE app_state SET last_purge_at = ?1 WHERE id = 1",
            [now],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        Ok(n as i64)
    }

    // ─── boot repair ───────────────────────────────────────────────────

    /// §6.5: `local_date` must always agree with `starts_at` in `tz`. The
    /// command layer computes it on write; this verifies it on boot, because a
    /// row written by an older build (or hand-edited) would otherwise render on
    /// the wrong day forever.
    fn repair_local_dates(&mut self) -> Result<usize> {
        let rows: Vec<(String, i64, String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, starts_at, tz, local_date FROM scheduled_block WHERE deleted_at IS NULL",
            )?;
            let mapped = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?;
            mapped.collect::<std::result::Result<_, _>>()?
        };
        let mut fixed = 0;
        let tx = self.conn.transaction()?;
        for (id, starts_at, tz, stored) in rows {
            let Ok(zone) = crate::time::zone(&tz) else {
                continue;
            };
            let actual = crate::time::local_date(starts_at, &zone);
            if actual != stored {
                tx.execute(
                    "UPDATE scheduled_block SET local_date = ?2 WHERE id = ?1",
                    rusqlite::params![id, actual],
                )?;
                fixed += 1;
            }
        }
        tx.commit()?;
        Ok(fixed)
    }
}
