//! Connection setup, migrations, snapshots and integrity (§6.2, §6.6, §6.7).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, Transaction};

use crate::error::{AppError, Result};
use crate::ids::new_id;
use crate::time::now_ms;

/// Forward-only. Never edit a shipped migration; append a new one (§6.6).
pub const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_tracked_cache.sql"),
    include_str!("../migrations/0003_rollover.sql"),
    include_str!("../migrations/0004_recurrence_and_activity.sql"),
    include_str!("../migrations/0005_life_and_contribution.sql"),
    include_str!("../migrations/0006_domain_rules.sql"),
    include_str!("../migrations/0007_observation_categories.sql"),
];

pub fn schema_version() -> i64 {
    MIGRATIONS.len() as i64
}

/// PRAGMAs that must be set on **every** connection (§6.2).
///
/// `foreign_keys` in particular is per-connection and off by default in
/// SQLite, so this is not optional housekeeping — it is invariant enforcement.
pub fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    Ok(())
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    configure(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    // WAL is meaningless for :memory:, the rest still matters.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

/// `VACUUM INTO` — atomic and consistent, unlike a file copy of a live WAL
/// database (§6.7).
pub fn snapshot(conn: &Connection, to: &Path) -> Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if to.exists() {
        std::fs::remove_file(to)?;
    }
    conn.execute("VACUUM INTO ?1", [to.to_string_lossy().as_ref()])?;
    Ok(())
}

pub fn backups_dir(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("backups")
}

fn pre_migration_backup_path(db_path: &Path, from_version: i64) -> PathBuf {
    backups_dir(db_path).join(format!("fruit-pre-v{from_version}-{}.db", now_ms()))
}

/// Apply outstanding migrations, one transaction each, snapshot first.
pub fn migrate(conn: &mut Connection, db_path: Option<&Path>) -> Result<i64> {
    let mut v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let target = schema_version();

    if v > target {
        // The user downgraded. Refuse rather than corrupt (§6.6, D3).
        return Err(AppError::NewerSchema {
            on_disk: v,
            supported: target,
        });
    }
    if v == target {
        return Ok(v);
    }

    // Unconditional snapshot before the first migration, named with the
    // outgoing user_version.
    if let Some(path) = db_path {
        if v > 0 {
            snapshot(conn, &pre_migration_backup_path(path, v))?;
        }
    }

    while v < target {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATIONS[v as usize])?;
        tx.pragma_update(None, "user_version", v + 1)?;
        tx.commit()?;
        v += 1;
    }
    Ok(v)
}

/// Called once after migration: stamps this device's id into the singleton.
pub fn ensure_device_id(conn: &Connection) -> Result<String> {
    let current: String = conn.query_row("SELECT device_id FROM app_state WHERE id = 1", [], |r| {
        r.get(0)
    })?;
    if !current.is_empty() {
        return Ok(current);
    }
    let id = new_id();
    conn.execute("UPDATE app_state SET device_id = ?1 WHERE id = 1", [&id])?;
    Ok(id)
}

#[derive(Debug, serde::Serialize)]
pub struct IntegrityReport {
    pub quick_check: String,
    pub foreign_key_violations: i64,
    pub orphan_open_sessions: i64,
    pub caches_repaired: i64,
    pub blocks_crossing_midnight: i64,
    pub page_count: i64,
    pub db_bytes: i64,
    pub checked_at: i64,
}

pub fn quick_check(conn: &Connection) -> Result<String> {
    Ok(conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))?)
}

pub fn foreign_key_violations(conn: &Connection) -> Result<i64> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let n = stmt.query_map([], |_| Ok(()))?.count();
    Ok(n as i64)
}

/// Rebuild every `*_cache` table from the views. A cache that cannot be
/// rebuilt on demand is not a cache, it is a second truth (§6.4).
pub fn rebuild_tracked_caches(tx: &Transaction) -> Result<i64> {
    let now = now_ms();
    tx.execute("DELETE FROM block_tracked_cache", [])?;
    let blocks = tx.execute(
        "INSERT INTO block_tracked_cache (block_id, tracked_sec, computed_at)
         SELECT block_id, tracked_sec, ?1 FROM block_tracked",
        [now],
    )?;
    tx.execute("DELETE FROM task_tracked_cache", [])?;
    let tasks = tx.execute(
        "INSERT INTO task_tracked_cache (task_id, tracked_sec, computed_at)
         SELECT task_id, tracked_sec, ?1 FROM task_tracked",
        [now],
    )?;
    Ok((blocks + tasks) as i64)
}

pub fn checkpoint(conn: &Connection) -> Result<()> {
    let _: std::result::Result<i64, _> =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_from_empty_to_head() {
        let mut c = open_in_memory().unwrap();
        assert_eq!(migrate(&mut c, None).unwrap(), schema_version());
        // Idempotent.
        assert_eq!(migrate(&mut c, None).unwrap(), schema_version());
    }

    #[test]
    fn refuses_a_newer_schema() {
        let mut c = open_in_memory().unwrap();
        migrate(&mut c, None).unwrap();
        c.pragma_update(None, "user_version", schema_version() + 3)
            .unwrap();
        match migrate(&mut c, None) {
            Err(AppError::NewerSchema { on_disk, supported }) => {
                assert_eq!(on_disk, schema_version() + 3);
                assert_eq!(supported, schema_version());
            }
            other => panic!("expected NewerSchema, got {other:?}"),
        }
    }

    #[test]
    fn foreign_keys_are_on_for_every_connection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fruit.db");
        let mut c = open(&path).unwrap();
        migrate(&mut c, Some(&path)).unwrap();
        drop(c);
        for _ in 0..3 {
            let c = open(&path).unwrap();
            let on: i64 = c.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
            assert_eq!(on, 1);
            let journal: String = c
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(journal.to_lowercase(), "wal");
        }
    }

    #[test]
    fn snapshot_produces_a_usable_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fruit.db");
        let mut c = open(&path).unwrap();
        migrate(&mut c, Some(&path)).unwrap();
        let snap = dir.path().join("backups/snap.db");
        snapshot(&c, &snap).unwrap();
        let restored = open(&snap).unwrap();
        let v: i64 = restored
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, schema_version());
    }
}
