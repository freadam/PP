//! §6.3: "Add an `EXPLAIN QUERY PLAN` assertion test over the five queries in
//! §6.10 so a future refactor cannot silently reintroduce a table scan."
//!
//! These assert on the *index chosen*, not on wall-clock time, so they are
//! stable on a loaded CI box. The budgets themselves live in §7.1 and are
//! measured separately.

use fruit_core::{db, Store};
use rusqlite::Connection;

fn plan(conn: &Connection, sql: &str) -> String {
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    rows.join(" | ")
}

fn seeded() -> Store {
    let mut store = Store::open_in_memory().unwrap();
    store.seed_first_run("Europe/London").unwrap();
    store
}

/// Week load — blocks for seven `local_date` values (§6.10, budget 5ms).
#[test]
fn week_load_uses_block_date_idx() {
    let store = seeded();
    let p = plan(
        store.connection(),
        "SELECT b.id, COALESCE(c.tracked_sec, 0)
           FROM scheduled_block b
           LEFT JOIN task t ON t.id = b.task_id
           LEFT JOIN block_tracked_cache c ON c.block_id = b.id
          WHERE b.local_date IN ('2025-07-28','2025-07-29','2025-07-30')
            AND b.deleted_at IS NULL",
    );
    assert!(p.contains("block_date_idx"), "no index on the week query: {p}");
    assert!(!p.contains("SCAN scheduled_block"), "week query scans: {p}");
}

/// Backlog — open tasks bucketed by due date (§6.10, budget 5ms).
#[test]
fn backlog_uses_a_task_index() {
    let store = seeded();
    let p = plan(
        store.connection(),
        "SELECT id FROM task
          WHERE deleted_at IS NULL AND status = 'open' AND due_date IS NOT NULL
          ORDER BY due_date",
    );
    assert!(
        p.contains("task_due_date_idx"),
        "backlog fell back to a scan: {p}"
    );
}

/// Task list page — filtered, sorted, one page (§6.10, budget 3ms).
#[test]
fn task_page_uses_the_rank_index() {
    let store = seeded();
    let p = plan(
        store.connection(),
        "SELECT id FROM task WHERE deleted_at IS NULL ORDER BY sort_rank LIMIT 100",
    );
    assert!(p.contains("task_rank_idx"), "task page sorts by scan: {p}");
}

/// Calibration — done tasks with estimates in a 30-day window (budget 20ms).
#[test]
fn calibration_uses_a_status_index() {
    let store = seeded();
    let p = plan(
        store.connection(),
        "SELECT id FROM task
          WHERE deleted_at IS NULL AND status = 'done' AND completed_at >= 0
            AND estimate_sec IS NOT NULL",
    );
    assert!(
        p.contains("task_due_at_idx") || p.contains("task_due_date_idx"),
        "calibration has no status index to lean on: {p}"
    );
}

/// Day activity — spans overlapping a local date (budget 10ms @ 1M spans).
#[test]
fn day_activity_uses_activity_time_idx() {
    let store = seeded();
    let p = plan(
        store.connection(),
        "SELECT id FROM activity_span WHERE started_at >= 0 AND started_at < 1",
    );
    assert!(p.contains("activity_time_idx"), "activity scans: {p}");
}

/// The open-session lookup runs on every boot and every start/stop; a scan here
/// would be a scan of the largest table in the database.
#[test]
fn open_session_lookup_uses_the_partial_index() {
    let store = seeded();
    let p = plan(
        store.connection(),
        "SELECT id FROM time_session WHERE ended_at IS NULL",
    );
    assert!(p.contains("session_open_idx"), "open-session lookup scans: {p}");
}

/// Every index §6.3 names is actually present — the cheapest possible guard
/// against a migration that forgets one.
#[test]
fn every_declared_index_exists() {
    let store = seeded();
    let mut stmt = store
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%'")
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for expected in [
        "task_project_idx",
        "task_due_date_idx",
        "task_due_at_idx",
        "task_parent_idx",
        "task_rank_idx",
        "block_date_idx",
        "block_task_idx",
        "block_series_idx",
        "session_task_idx",
        "session_block_idx",
        "session_open_idx",
        "activity_time_idx",
        "tag_name_uq",
    ] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }
    assert_eq!(db::schema_version(), db::MIGRATIONS.len() as i64);
}
