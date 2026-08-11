//! Export, import, backups and integrity (§6.7, §6.12).
//!
//! Export is a trust feature, not a chore. People commit to local-first tools
//! that visibly let them leave, so the JSON form is lossless, id-preserving and
//! round-trips exactly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Timelike};
use rusqlite::types::ValueRef;
use rusqlite::Transaction;
use serde_json::{json, Map, Value};

use super::Store;
use crate::db;
use crate::error::{AppError, Result};
use crate::ids::new_id;
use crate::model::*;
use crate::time::{to_local, zone};

pub const EXPORT_FORMAT: &str = "fruit.export";
pub const EXPORT_VERSION: i64 = 2;

/// Import is untrusted input (§7.3): every array is capped before the
/// transaction opens.
const MAX_ROWS_PER_TABLE: usize = 500_000;

/// (json key, table name) in dependency order — parents before children, so a
/// load inside one transaction never trips a foreign key.
const TABLES: &[(&str, &str)] = &[
    ("projects", "project"),
    ("tasks", "task"),
    ("tags", "tag"),
    ("taskTags", "task_tag"),
    ("notes", "note"),
    ("blocks", "scheduled_block"),
    ("sessions", "time_session"),
    ("dayReviews", "day_review"),
];

/// The only tables `reset_all_data` leaves alone entirely.
///
/// This is a **keep-list, not a delete-list**, and the direction is the whole
/// point. A delete-list has to be edited every time a migration adds a table,
/// and forgetting is silent: the button still says "cleared everything" and the
/// new table quietly survives every reset. With a keep-list, forgetting wipes
/// something that should have been kept — which is wrong too, but wrong
/// *loudly*, on the first run, instead of hiding inside a test suite that
/// starts from a state nobody expected.
///
/// (The first draft of this was a delete-list. It named `domain_rule`, a table
/// migration 0007 folded into `activity_rule` and dropped, and the reset threw
/// "no such table" on every call. The list was wrong on the day it was written.)
const RESET_KEEP_TABLES: &[&str] = &[
    // Configuration, not data. Wiping it would make a tester reconfigure the
    // app on every run; the handful of runtime keys are removed by name below.
    "setting",
    // The singleton. Its `device_id` identifies the machine, not the data, so
    // the row is edited rather than deleted.
    "app_state",
];

/// Tables where the built-in rows are schema, not data: migrations 0005 and
/// 0007 created them, no migration will create them again, and the counting
/// invariant needs somewhere for every second to land. Only what the *user*
/// added to these is removed.
const RESET_KEEP_BUILTIN: &[&str] = &["activity_rule", "observation_category", "life_area"];

/// Settings that hold runtime state or point at rows that are about to stop
/// existing. Everything else in `setting` is configuration and survives —
/// see the doc comment on `reset_all_data`. Undo tokens are cleared separately
/// by prefix.
const RESET_SETTINGS: &[&str] = &[
    "timer.focusEndsAt",
    "notices.fired",
    "notices.silentUntil",
    "activity.lastPurgeAt",
    "onboarding.railExplained",
];

impl Store {
    // ─── export ────────────────────────────────────────────────────────

    pub fn export_json(&self, tz: &str) -> Result<Value> {
        let mut doc = Map::new();
        doc.insert("format".into(), json!(EXPORT_FORMAT));
        doc.insert("version".into(), json!(EXPORT_VERSION));
        doc.insert("exportedAt".into(), json!(self.now()));
        doc.insert("tz".into(), json!(tz));
        for (key, table) in TABLES {
            doc.insert((*key).to_string(), Value::Array(self.dump_table(table)?));
        }
        doc.insert("settings".into(), Value::Object(self.all_settings()?));
        Ok(Value::Object(doc))
    }

    fn dump_table(&self, table: &str) -> Result<Vec<Value>> {
        let mut stmt = self.conn.prepare(&format!("SELECT * FROM {table}"))?;
        let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut obj = Map::new();
            for (i, name) in columns.iter().enumerate() {
                obj.insert(name.clone(), sqlite_to_json(row.get_ref(i)?));
            }
            out.push(Value::Object(obj));
        }
        Ok(out)
    }

    /// Two files, because both audiences exist: human clock times *and* raw
    /// epoch columns (§6.12).
    pub fn export_csv(&self, tz: &str) -> Result<(String, String)> {
        let zone_ = zone(tz)?;
        let mut tasks = String::from(
            "id,title,project,status,priority,estimate_sec,tracked_sec,due,created_local,created_epoch_ms\n",
        );
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.title, COALESCE(p.name, 'Inbox'), t.status, t.priority,
                    COALESCE(t.estimate_sec, 0), COALESCE(tt.tracked_sec, 0),
                    COALESCE(t.due_date, ''), t.due_at, t.created_at
               FROM task t
               LEFT JOIN project p ON p.id = t.project_id
               LEFT JOIN task_tracked tt ON tt.task_id = t.id
              WHERE t.deleted_at IS NULL ORDER BY t.created_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, i64>(9)?,
            ))
        })?;
        for row in rows {
            let (id, title, project, status, priority, est, tracked, due_date, due_at, created) =
                row?;
            let due = if !due_date.is_empty() {
                due_date
            } else {
                due_at.map(|at| iso_local(at, tz)).unwrap_or_default()
            };
            tasks.push_str(&format!(
                "{id},{},{},{status},{priority},{est},{tracked},{due},{},{created}\n",
                csv(&title),
                csv(&project),
                iso_local(created, tz)
            ));
        }
        drop(stmt);

        let mut sessions = String::from(
            "id,task,project,source,started_local,ended_local,elapsed_sec,block_id,started_epoch_ms,ended_epoch_ms\n",
        );
        let mut stmt = self.conn.prepare(
            "SELECT s.id, t.title, COALESCE(p.name, 'Inbox'), s.source, s.started_at, s.ended_at,
                    s.elapsed_sec, COALESCE(s.block_id, '')
               FROM time_session s
               JOIN task t ON t.id = s.task_id
               LEFT JOIN project p ON p.id = t.project_id
              ORDER BY s.started_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        for row in rows {
            let (id, title, project, source, started, ended, elapsed, block) = row?;
            sessions.push_str(&format!(
                "{id},{},{},{source},{},{},{elapsed},{block},{started},{}\n",
                csv(&title),
                csv(&project),
                iso_local(started, tz),
                ended.map(|e| iso_local(e, tz)).unwrap_or_default(),
                ended.map(|e| e.to_string()).unwrap_or_default()
            ));
        }
        let _ = zone_;
        Ok((tasks, sessions))
    }

    /// Export only — Fruit never writes to an external calendar (§6.12).
    pub fn export_ics(&self) -> Result<String> {
        let mut out = String::from(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Fruit//Planner//EN\r\nCALSCALE:GREGORIAN\r\n",
        );
        let mut stmt = self.conn.prepare(
            "SELECT b.id, COALESCE(t.title, b.label), b.starts_at, b.duration_sec, b.created_at
               FROM scheduled_block b LEFT JOIN task t ON t.id = b.task_id
              WHERE b.deleted_at IS NULL ORDER BY b.starts_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        for row in rows {
            let (id, title, starts_at, duration, created) = row?;
            out.push_str("BEGIN:VEVENT\r\n");
            out.push_str(&format!("UID:{id}@fruit.local\r\n"));
            out.push_str(&format!("DTSTAMP:{}\r\n", ics_utc(created)));
            out.push_str(&format!("DTSTART:{}\r\n", ics_utc(starts_at)));
            out.push_str(&format!(
                "DTEND:{}\r\n",
                ics_utc(starts_at + duration * 1000)
            ));
            out.push_str(&format!("SUMMARY:{}\r\n", ics_escape(&title)));
            out.push_str("END:VEVENT\r\n");
        }
        out.push_str("END:VCALENDAR\r\n");
        Ok(out)
    }

    pub fn export_to(&self, format: ExportFormat, path: &Path, tz: &str) -> Result<ExportSummary> {
        let mut paths = Vec::new();
        match format {
            ExportFormat::Json => {
                let doc = self.export_json(tz)?;
                std::fs::write(path, serde_json::to_vec_pretty(&doc)?)?;
                paths.push(path.display().to_string());
            }
            ExportFormat::Csv => {
                let (tasks, sessions) = self.export_csv(tz)?;
                let dir = if path.is_dir() {
                    path.to_path_buf()
                } else {
                    path.parent().unwrap_or(Path::new(".")).to_path_buf()
                };
                std::fs::create_dir_all(&dir)?;
                let t = dir.join("tasks.csv");
                let s = dir.join("sessions.csv");
                std::fs::write(&t, tasks)?;
                std::fs::write(&s, sessions)?;
                paths.push(t.display().to_string());
                paths.push(s.display().to_string());
            }
            ExportFormat::Ics => {
                std::fs::write(path, self.export_ics()?)?;
                paths.push(path.display().to_string());
            }
        }
        Ok(ExportSummary {
            format,
            paths,
            projects: self.count("project")?,
            tasks: self.count("task")?,
            blocks: self.count("scheduled_block")?,
            sessions: self.count("time_session")?,
        })
    }

    /// Every table this database actually has, asked of the schema rather than
    /// remembered in a list. `sqlite_%` covers SQLite's own internals
    /// (`sqlite_sequence`, `sqlite_stat1`); views are excluded because a view
    /// has nothing to delete.
    fn user_tables(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
              ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn count(&self, table: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
    }

    // ─── import ────────────────────────────────────────────────────────

    pub fn import_json(&mut self, doc: &Value, mode: ImportMode) -> Result<ImportSummary> {
        // Validate the whole payload before the transaction opens (§7.3).
        if doc.get("format").and_then(Value::as_str) != Some(EXPORT_FORMAT) {
            return Err(AppError::Import(
                "That file isn't a Fruit export. Look for a .json written by Settings → Data → Export.".into(),
            ));
        }
        let version = doc.get("version").and_then(Value::as_i64).unwrap_or(0);
        if version > EXPORT_VERSION {
            return Err(AppError::Import(format!(
                "That export came from a newer version of Fruit (format {version}). Update Fruit first."
            )));
        }
        for (key, _) in TABLES {
            if let Some(v) = doc.get(key) {
                let arr = v.as_array().ok_or_else(|| {
                    AppError::Import(format!("'{key}' should be a list of rows."))
                })?;
                if arr.len() > MAX_ROWS_PER_TABLE {
                    return Err(AppError::Import(format!(
                        "'{key}' has {} rows, past the {MAX_ROWS_PER_TABLE} cap.",
                        arr.len()
                    )));
                }
                if arr.iter().any(|r| !r.is_object()) {
                    return Err(AppError::Import(format!("'{key}' has a row that isn't an object.")));
                }
            }
        }

        if mode == ImportMode::Replace {
            if let Some(path) = self.db_path.clone() {
                db::snapshot(&self.conn, &db::backups_dir(&path).join(format!(
                    "fruit-pre-import-{}.db",
                    self.now()
                )))?;
            }
        }

        let mut summary = ImportSummary {
            projects: 0,
            tasks: 0,
            blocks: 0,
            sessions: 0,
            skipped: 0,
        };
        let now = self.now();
        let tx = self.conn.transaction()?;

        if mode == ImportMode::Replace {
            for (_, table) in TABLES.iter().rev() {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
            tx.execute("UPDATE app_state SET running_session_id = NULL WHERE id = 1", [])?;
        }

        // `append` mints fresh ids throughout, so a file can be imported into a
        // database that already contains it without colliding.
        let mut remap: HashMap<String, String> = HashMap::new();
        if mode == ImportMode::Append {
            for (key, _) in TABLES {
                if let Some(rows) = doc.get(key).and_then(Value::as_array) {
                    for row in rows {
                        if let Some(id) = row.get("id").and_then(Value::as_str) {
                            remap.insert(id.to_string(), new_id());
                        }
                    }
                }
            }
        }

        for (key, table) in TABLES {
            let Some(rows) = doc.get(key).and_then(Value::as_array) else {
                continue;
            };
            let columns = table_columns(&tx, table)?;
            for row in rows {
                let obj = row.as_object().unwrap();
                let inserted =
                    insert_row(&tx, table, obj, &columns, &remap, mode, now)?;
                if !inserted {
                    summary.skipped += 1;
                    continue;
                }
                match *table {
                    "project" => summary.projects += 1,
                    "task" => summary.tasks += 1,
                    "scheduled_block" => summary.blocks += 1,
                    "time_session" => summary.sessions += 1,
                    _ => {}
                }
            }
        }

        if let Some(settings) = doc.get("settings").and_then(Value::as_object) {
            for (k, v) in settings {
                if k.starts_with("undo.") {
                    continue;
                }
                tx.execute(
                    "INSERT INTO setting (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![k, v.to_string(), now],
                )?;
            }
        }

        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        Ok(summary)
    }

    pub fn import_file(&mut self, path: &Path, mode: ImportMode) -> Result<ImportSummary> {
        let text = std::fs::read_to_string(path)?;
        let doc: Value = serde_json::from_str(&text)
            .map_err(|e| AppError::Import(format!("That file isn't valid JSON ({e}).")))?;
        self.import_json(&doc, mode)
    }

    // ─── reset ─────────────────────────────────────────────────────────

    /// Empty the database of everything the user ever recorded, so a test run
    /// can start from nothing.
    ///
    /// Three things this deliberately does **not** touch, and each would be a
    /// bug if it did:
    ///
    /// 1. **The built-in taxonomy.** Life areas, observation categories and
    ///    their starter rules are created by migrations 0005/0007 and marked
    ///    `is_builtin = 1`. The counting invariant needs somewhere for every
    ///    second to land, and recreating those rows is the migration's job —
    ///    a command that deleted them would leave a database no migration will
    ///    ever repair. Everything the *user* added to those tables goes.
    /// 2. **Configuration.** Theme, clock format, pomodoro lengths and the
    ///    activity switches are settings, not data; wiping them would make a
    ///    tester reconfigure the app on every run. Only the keys that hold
    ///    *runtime* state or point at now-deleted rows are cleared.
    /// 3. **`app_state.device_id`.** It identifies the machine, not the data.
    ///
    /// Deletes are hard, not soft: "Recently deleted" has to be empty too, or
    /// the next run starts with a restore list from the last one.
    ///
    /// Takes a snapshot first when the store is file-backed. This is
    /// irreversible from inside the app, and the one-key escape hatch costs a
    /// `VACUUM INTO` that nobody will notice.
    pub fn reset_all_data(&mut self) -> Result<ResetSummary> {
        let summary = ResetSummary {
            projects: self.count("project")?,
            tasks: self.count("task")?,
            blocks: self.count("scheduled_block")?,
            sessions: self.count("time_session")?,
            life_entries: self.count("life_entry")?,
            activity_spans: self.count("activity_span")?,
            backup_path: None,
        };

        let backup_path = match self.db_path.clone() {
            Some(path) => {
                let to = db::backups_dir(&path).join(format!("fruit-pre-reset-{}.db", self.now()));
                std::fs::create_dir_all(db::backups_dir(&path))?;
                db::snapshot(&self.conn, &to)?;
                Some(to.display().to_string())
            }
            None => None,
        };

        // The running timer's row is about to be deleted. `ON DELETE SET NULL`
        // clears the database pointer, but the in-process runtime holds its own
        // copy of the session id and would go on ticking against a row that no
        // longer exists — so it is reset too, before the delete.
        self.timer = super::TimerRuntime::default();

        let now = self.now();
        let tables = self.user_tables()?;
        let tx = self.conn.transaction()?;

        // Foreign keys are checked at commit rather than per statement, so the
        // deletes need no dependency ordering — by the time the constraint is
        // evaluated every table involved is already empty. Ordering a list that
        // is discovered at runtime would otherwise mean topologically sorting
        // the schema, which is a lot of machinery to delete nothing.
        tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;

        tx.execute(
            "UPDATE app_state SET running_session_id = NULL, last_purge_at = NULL WHERE id = 1",
            [],
        )?;

        for table in &tables {
            if RESET_KEEP_TABLES.contains(&table.as_str()) {
                continue;
            }
            if RESET_KEEP_BUILTIN.contains(&table.as_str()) {
                tx.execute(&format!("DELETE FROM \"{table}\" WHERE is_builtin = 0"), [])?;
            } else {
                tx.execute(&format!("DELETE FROM \"{table}\""), [])?;
            }
        }

        for key in RESET_SETTINGS {
            tx.execute("DELETE FROM setting WHERE key = ?1", [key])?;
        }
        tx.execute("DELETE FROM setting WHERE key LIKE 'undo.%'", [])?;
        // Left set on purpose. The point of this button is a database that
        // *stays* empty; clearing the flag would hand the next launch the
        // "Welcome to Fruit" demo project instead of nothing.
        tx.execute(
            "INSERT INTO setting (key, value, updated_at) VALUES (?1, 'true', ?2)
             ON CONFLICT(key) DO UPDATE SET value = 'true', updated_at = excluded.updated_at",
            rusqlite::params![super::seed::SEED_FLAG, now],
        )?;

        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;

        Ok(ResetSummary {
            backup_path,
            ..summary
        })
    }

    // ─── integrity and backups ─────────────────────────────────────────

    pub fn run_integrity_check(&mut self) -> Result<db::IntegrityReport> {
        let quick = db::quick_check(&self.conn)?;
        let fk = db::foreign_key_violations(&self.conn)?;
        let orphans: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM time_session WHERE ended_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        let midnight_crossers = self.count_midnight_crossers()?;
        let tx = self.conn.transaction()?;
        let repaired = db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        let page_count: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok(db::IntegrityReport {
            quick_check: quick,
            foreign_key_violations: fk,
            orphan_open_sessions: orphans,
            caches_repaired: repaired,
            blocks_crossing_midnight: midnight_crossers,
            page_count,
            db_bytes: page_count * page_size,
            checked_at: self.now(),
        })
    }

    fn count_midnight_crossers(&self) -> Result<i64> {
        let mut stmt = self.conn.prepare(
            "SELECT starts_at, duration_sec, tz FROM scheduled_block WHERE deleted_at IS NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut bad = 0;
        for row in rows {
            let (starts_at, duration, tz) = row?;
            if let Ok(z) = zone(&tz) {
                if crate::time::same_day_span(starts_at, duration, &z).is_err() {
                    bad += 1;
                }
            }
        }
        Ok(bad)
    }

    /// On launch, if the newest snapshot is older than 24h, take one. Keep 7
    /// daily and 4 weekly (§6.7).
    pub fn rotate_backups(&self) -> Result<Option<PathBuf>> {
        let Some(db_path) = &self.db_path else {
            return Ok(None);
        };
        let dir = db::backups_dir(db_path);
        std::fs::create_dir_all(&dir)?;
        let now = self.now();

        let mut snapshots: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("fruit-daily-")
            })
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        snapshots.sort_by(|a, b| b.0.cmp(&a.0));

        let newest_age = snapshots
            .first()
            .and_then(|(t, _)| t.elapsed().ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(i64::MAX);
        if newest_age < 24 * 3600 * 1000 {
            return Ok(None);
        }

        let stamp = to_local(now, &chrono_tz::UTC);
        let path = dir.join(format!(
            "fruit-daily-{:04}{:02}{:02}-{:02}{:02}.db",
            stamp.year(),
            stamp.month(),
            stamp.day(),
            stamp.hour(),
            stamp.minute()
        ));
        db::snapshot(&self.conn, &path)?;

        for (_, old) in snapshots.into_iter().skip(6) {
            let _ = std::fs::remove_file(old);
        }
        Ok(Some(path))
    }
}

fn table_columns(tx: &Transaction, table: &str) -> Result<Vec<String>> {
    let mut stmt = tx.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Column names come from an untrusted file, so they are matched against the
/// table's real columns rather than interpolated. Anything unrecognised is
/// dropped, which is also what makes a v1 export loadable into a v2 schema.
fn insert_row(
    tx: &Transaction,
    table: &str,
    obj: &Map<String, Value>,
    columns: &[String],
    remap: &HashMap<String, String>,
    mode: ImportMode,
    now: i64,
) -> Result<bool> {
    let id_like = ["id", "task_id", "project_id", "parent_id", "block_id", "tag_id"];
    let mut names: Vec<&str> = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();

    for column in columns {
        // `note.markdown` became `note.body` in migration 0011. An export
        // written before that carries the old key, and skipping it would drop
        // every note on restore — silently, because the row still inserts on
        // its `task_id`. Restoring a backup must not lose what the backup held.
        let raw = match obj.get(column) {
            Some(v) => v,
            None if table == "note" && column == "body" => match obj.get("markdown") {
                Some(v) => v,
                None => continue,
            },
            None => continue,
        };
        let mut value = json_to_sqlite(raw);
        if !remap.is_empty() && id_like.contains(&column.as_str()) {
            if let rusqlite::types::Value::Text(s) = &value {
                if let Some(fresh) = remap.get(s) {
                    value = rusqlite::types::Value::Text(fresh.clone());
                }
            }
        }
        if column == "updated_at" && mode == ImportMode::Append {
            value = rusqlite::types::Value::Integer(now);
        }
        names.push(column);
        values.push(value);
    }
    if names.is_empty() {
        return Ok(false);
    }

    let placeholders = (1..=names.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    // `merge` skips rows that already exist; ids are preserved so a re-import
    // is idempotent (§6.12).
    let verb = if mode == ImportMode::Merge {
        "INSERT OR IGNORE"
    } else {
        "INSERT OR REPLACE"
    };
    let sql = format!(
        "{verb} INTO {table} ({}) VALUES ({placeholders})",
        names.join(",")
    );
    let changed = tx.execute(&sql, rusqlite::params_from_iter(values.iter()))?;
    Ok(changed > 0)
}

fn sqlite_to_json(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
        ValueRef::Blob(b) => json!(b.to_vec()),
    }
}

fn json_to_sqlite(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as S;
    match v {
        Value::Null => S::Null,
        Value::Bool(b) => S::Integer(*b as i64),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                S::Integer(i)
            } else {
                S::Real(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => S::Text(s.clone()),
        other => S::Text(other.to_string()),
    }
}

fn iso_local(at: i64, tz: &str) -> String {
    match zone(tz) {
        Ok(z) => to_local(at, &z).format("%Y-%m-%d %H:%M:%S").to_string(),
        Err(_) => String::new(),
    }
}

fn ics_utc(at: i64) -> String {
    to_local(at, &chrono_tz::UTC)
        .format("%Y%m%dT%H%M%SZ")
        .to_string()
}

fn ics_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
