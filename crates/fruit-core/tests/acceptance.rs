//! The acceptance criteria from §8 that live below the UI.
//!
//! Each test names its criterion. Anything in §8 that needs a rendered window
//! (I1–I8, U1–U5, U9, U10, D13) is checked in the frontend or by hand and is
//! listed in docs/ACCEPTANCE.md rather than silently dropped.

use std::sync::Arc;

use chrono::TimeZone;
use chrono_tz::Tz;
use fruit_core::clock::TestClock;
use fruit_core::model::*;
use fruit_core::store::IdleReport;
use fruit_core::time::{local_date, to_local};
use fruit_core::{db, AppError, Store};

const TZ: &str = "Europe/London";

fn london() -> Tz {
    TZ.parse().unwrap()
}

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
    london()
        .with_ymd_and_hms(y, mo, d, h, mi, 0)
        .unwrap()
        .timestamp_millis()
}

fn store_at(ms: i64) -> (Store, TestClock) {
    let clock = TestClock::new(ms);
    let store = Store::in_memory_with_clock(Arc::new(clock.clone())).unwrap();
    (store, clock)
}

fn task(store: &mut Store, title: &str) -> TaskRow {
    store
        .create_task(NewTask {
            title: title.into(),
            ..Default::default()
        })
        .unwrap()
}

fn block(store: &mut Store, task_id: &str, starts_at: i64, minutes: i64) -> BlockRow {
    store
        .schedule_block(NewBlock {
            task_id: Some(task_id.to_string()),
            label: None,
            starts_at,
            duration_sec: minutes * 60,
            tz: TZ.into(),
            is_fixed: false,
        })
        .unwrap()
}

fn week(store: &Store, date: &str) -> WeekView {
    store
        .get_week(
            &DateRange {
                from: date.into(),
                to: date.into(),
            },
            TZ,
        )
        .unwrap()
}

// ─── F: features ───────────────────────────────────────────────────────

/// F1 — a task can be created, estimated, tagged, scheduled, tracked,
/// reconciled and completed. (The *keyboard* half of F1 is a UI criterion;
/// this covers that every step exists as a command.)
#[test]
fn f1_full_loop_through_the_command_layer() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = store
        .create_task(NewTask {
            title: "Refactor auth module".into(),
            estimate_sec: Some(3600),
            tags: vec!["dev".into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(t.tags.len(), 1);

    let b = block(&mut store, &t.id, at(2025, 7, 30, 9, 0), 60);
    store.start_timer(&t.id, Some(&b.id)).unwrap();
    clock.advance(74 * 60_000);
    let state = store.stop_timer().unwrap();
    assert_eq!(state.phase, TimerPhase::Idle);

    let items = store.get_reconcile_items("2025-07-30", TZ).unwrap();
    let overran = items
        .iter()
        .find(|i| i.kind == ReconcileKind::Overran)
        .expect("the overrun shows up in reconcile");
    let review = store
        .apply_reconcile(
            "2025-07-30",
            vec![ReconcileAction {
                item_id: overran.id.clone(),
                verb: ReconcileVerb::Accept,
                starts_at: None,
                duration_sec: None,
                task_id: None,
                estimate_sec: None,
            }],
            TZ,
        )
        .unwrap();
    assert_eq!(review.planned_sec, 3600);
    assert_eq!(review.tracked_sec, 74 * 60);

    let done = store.set_task_status(&t.id, Status::Done).unwrap();
    assert_eq!(done.status, Status::Done);
    assert!(done.completed_at.is_some(), "the CHECK constraint pairs them");
}

/// F2 — a block started from the planner links its sessions to that block, and
/// drift is computed per block, not per task.
#[test]
fn f2_drift_is_per_block() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 8, 0));
    let t = task(&mut store, "Write intro");
    let morning = block(&mut store, &t.id, at(2025, 7, 30, 9, 0), 60);
    let afternoon = block(&mut store, &t.id, at(2025, 7, 30, 14, 0), 60);

    store.start_timer(&t.id, Some(&morning.id)).unwrap();
    clock.advance(74 * 60_000);
    store.stop_timer().unwrap();
    store.start_timer(&t.id, Some(&afternoon.id)).unwrap();
    clock.advance(20 * 60_000);
    store.stop_timer().unwrap();

    let day = &week(&store, "2025-07-30").days[0];
    let a = day.blocks.iter().find(|b| b.block.id == morning.id).unwrap();
    let b = day
        .blocks
        .iter()
        .find(|b| b.block.id == afternoon.id)
        .unwrap();

    assert_eq!(a.drift_sec, 14 * 60);
    assert_eq!(a.drift_state, DriftState::Overrun);
    assert_eq!(b.drift_sec, -40 * 60);
    // The task's own total is the sum; the blocks keep their own truth.
    assert_eq!(store.get_task(&t.id).unwrap().tracked_sec, 94 * 60);
}

/// F3 — an unplanned session with no block appears in Reconcile and can be
/// converted into a retroactive block. This is how the plan learns.
#[test]
fn f3_unplanned_session_becomes_a_retroactive_block() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 11, 0));
    let t = task(&mut store, "Unplanned firefight");
    let running = store.start_timer(&t.id, None).unwrap();
    assert!(running.session.as_ref().unwrap().block_id.is_none());
    clock.advance(35 * 60_000);
    let stopped = store.stop_timer().unwrap();
    assert_eq!(stopped.phase, TimerPhase::Idle);
    assert!(stopped.session.is_none(), "the timer chip goes away");

    let items = store.get_reconcile_items("2025-07-30", TZ).unwrap();
    let item = items
        .iter()
        .find(|i| i.kind == ReconcileKind::UnplannedSession)
        .expect("unplanned work surfaces");
    assert!(item.available.contains(&ReconcileVerb::CreateRetroBlock));

    store
        .apply_reconcile(
            "2025-07-30",
            vec![ReconcileAction {
                item_id: item.id.clone(),
                verb: ReconcileVerb::CreateRetroBlock,
                starts_at: None,
                duration_sec: None,
                task_id: None,
                estimate_sec: None,
            }],
            TZ,
        )
        .unwrap();

    let day = &week(&store, "2025-07-30").days[0];
    assert_eq!(day.blocks.len(), 1, "a block now exists for the work");
    assert_eq!(day.blocks[0].tracked_sec, 35 * 60);
    assert_eq!(
        day.unplanned_sec, 0,
        "and the session is no longer unplanned"
    );
}

/// F4 — a subtask can be scheduled and tracked independently, and rolls up
/// into its parent's totals.
#[test]
fn f4_subtasks_are_real_tasks() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let parent = task(&mut store, "Ship the release");
    let child = store
        .create_task(NewTask {
            title: "Write the changelog".into(),
            parent_id: Some(parent.id.clone()),
            estimate_sec: Some(1800),
            ..Default::default()
        })
        .unwrap();

    let b = block(&mut store, &child.id, at(2025, 7, 30, 10, 0), 30);
    store.start_timer(&child.id, Some(&b.id)).unwrap();
    clock.advance(30 * 60_000);
    store.stop_timer().unwrap();

    let detail = store.get_task_detail(&parent.id).unwrap();
    assert_eq!(detail.subtasks.len(), 1);
    assert_eq!(detail.subtasks[0].tracked_sec, 1800);
    assert_eq!(detail.task.subtask_total, 1);
    assert_eq!(detail.task.subtask_done, 0);

    store.set_task_status(&child.id, Status::Done).unwrap();
    assert_eq!(store.get_task(&parent.id).unwrap().subtask_done, 1);
}

/// §6.5 — subtask depth is capped at 3.
#[test]
fn subtask_depth_is_capped() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    let a = task(&mut store, "level 1");
    let b = store
        .create_task(NewTask {
            title: "level 2".into(),
            parent_id: Some(a.id.clone()),
            ..Default::default()
        })
        .unwrap();
    let c = store
        .create_task(NewTask {
            title: "level 3".into(),
            parent_id: Some(b.id.clone()),
            ..Default::default()
        })
        .unwrap();
    let too_deep = store.create_task(NewTask {
        title: "level 4".into(),
        parent_id: Some(c.id.clone()),
        ..Default::default()
    });
    assert!(matches!(too_deep, Err(AppError::SubtaskTooDeep)));
}

/// F5 — reconciling a day writes exactly one `day_review` row and produces a
/// takeaway line.
#[test]
fn f5_reconciling_writes_one_row_and_a_takeaway() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Refactor auth module");
    let b = block(&mut store, &t.id, at(2025, 7, 30, 9, 0), 60);
    store.start_timer(&t.id, Some(&b.id)).unwrap();
    clock.advance(74 * 60_000);
    store.stop_timer().unwrap();

    for _ in 0..3 {
        let review = store.apply_reconcile("2025-07-30", vec![], TZ).unwrap();
        assert!(!review.takeaway.is_empty());
        assert!(review.takeaway.contains("over plan"), "{}", review.takeaway);
    }
    let rows: i64 = store
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM day_review WHERE local_date = '2025-07-30'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1, "reconciling repeatedly still writes exactly one row");
}

/// F6 — calibration reports a bucket only at n ≥ 5 and uses median, not mean.
#[test]
fn f6_calibration_needs_five_samples_and_uses_the_median() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));

    // Four 1h tasks, each tracked for 1h — not enough to report.
    let make = |minutes: i64, estimate_sec: i64, store: &mut Store| {
        let t = store
            .create_task(NewTask {
                title: format!("task {minutes}"),
                estimate_sec: Some(estimate_sec),
                ..Default::default()
            })
            .unwrap();
        store.start_timer(&t.id, None).unwrap();
        clock.advance(minutes * 60_000);
        store.stop_timer().unwrap();
        store.set_task_status(&t.id, Status::Done).unwrap();
    };
    for _ in 0..4 {
        make(60, 3600, &mut store);
    }
    let report = store.calibration(TZ, None).unwrap();
    let one_hour = report.buckets.iter().find(|b| b.bucket == "1h").unwrap();
    assert_eq!(one_hour.n, 4);
    assert!(!one_hour.is_reportable, "four samples is not a signal");

    // A fifth, wildly abandoned one: the mean would be ruined, the median holds.
    make(600, 3600, &mut store);
    let report = store.calibration(TZ, None).unwrap();
    let one_hour = report.buckets.iter().find(|b| b.bucket == "1h").unwrap();
    assert_eq!(one_hour.n, 5);
    assert!(one_hour.is_reportable);
    assert!(
        (one_hour.median_ratio - 1.0).abs() < 0.01,
        "median {} shrugs off the outlier (the mean would be ~2.8)",
        one_hour.median_ratio
    );
    assert!(report.headline.contains("estimates"), "{}", report.headline);
}

/// F7 — manual session entry exists and is distinguishable from timer-recorded
/// sessions.
#[test]
fn f7_manual_sessions_are_flagged() {
    let (mut store, _) = store_at(at(2025, 7, 30, 18, 0));
    let t = task(&mut store, "Forgot to start the timer");
    let s = store
        .add_session(ManualSession {
            task_id: t.id.clone(),
            block_id: None,
            started_at: at(2025, 7, 30, 14, 0),
            ended_at: at(2025, 7, 30, 15, 30),
            note: Some("from memory".into()),
        })
        .unwrap();
    assert_eq!(s.source, "manual");
    assert_eq!(s.elapsed_sec, 90 * 60);
    assert_eq!(store.get_task(&t.id).unwrap().tracked_sec, 90 * 60);

    let bad = store.add_session(ManualSession {
        task_id: t.id.clone(),
        block_id: None,
        started_at: at(2025, 7, 30, 15, 0),
        ended_at: at(2025, 7, 30, 14, 0),
        note: None,
    });
    assert!(bad.is_err(), "a session cannot end before it starts");
}

// ─── U: UX criteria expressible below the UI ───────────────────────────

/// U4 — dropping on occupied time follows the stated collision policy, and
/// fixed blocks are never moved.
#[test]
fn u4_collision_policies() {
    let (mut store, _) = store_at(at(2025, 7, 30, 8, 0));
    let a = task(&mut store, "A");
    let b = task(&mut store, "B");

    // Overlap (default): both stay where they are put.
    let first = block(&mut store, &a.id, at(2025, 7, 30, 9, 0), 60);
    let second = block(&mut store, &b.id, at(2025, 7, 30, 11, 0), 60);
    store
        .move_block(&second.id, at(2025, 7, 30, 9, 30), CollisionPolicy::Overlap)
        .unwrap();
    let day = &week(&store, "2025-07-30").days[0];
    assert_eq!(day.blocks.len(), 2);
    assert_eq!(day.blocks[0].lanes, 2, "overlapping blocks share the group");
    assert_ne!(day.blocks[0].lane, day.blocks[1].lane);

    // Push: the later, non-fixed block moves down.
    store
        .move_block(&second.id, at(2025, 7, 30, 13, 0), CollisionPolicy::Overlap)
        .unwrap();
    let touched = store
        .move_block(&first.id, at(2025, 7, 30, 12, 30), CollisionPolicy::Push)
        .unwrap();
    assert_eq!(touched.len(), 2);
    let pushed = touched.iter().find(|b| b.id == second.id).unwrap();
    assert_eq!(pushed.starts_at, at(2025, 7, 30, 13, 30));

    // Fixed blocks are never pushed.
    store.set_block_fixed(&second.id, true).unwrap();
    let err = store.move_block(&first.id, at(2025, 7, 30, 13, 0), CollisionPolicy::Push);
    assert!(err.is_err(), "a push into a fixed block stops");
    assert_eq!(
        store.blocks_on("2025-07-30").unwrap()
            .iter()
            .find(|b| b.id == second.id)
            .unwrap()
            .starts_at,
        at(2025, 7, 30, 13, 30),
        "and leaves it exactly where it was"
    );

    // Shrink: the dropped block shortens to fit the gap.
    let c = task(&mut store, "C");
    let third = block(&mut store, &c.id, at(2025, 7, 30, 16, 0), 120);
    store
        .move_block(&third.id, at(2025, 7, 30, 13, 0), CollisionPolicy::Shrink)
        .unwrap();
    let shortened = store.blocks_on("2025-07-30").unwrap()
        .into_iter()
        .find(|b| b.id == third.id)
        .unwrap();
    assert_eq!(shortened.duration_sec, 30 * 60);
}

/// U6 — idle detection defaults to discarding the idle span, and the exact
/// span is shown.
#[test]
fn u6_idle_defaults_to_discard_and_names_the_span() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Deep work");
    store.start_timer(&t.id, None).unwrap();

    // Ten minutes of work, then twenty with no input.
    clock.advance(10 * 60_000);
    let last_input = clock.now();
    clock.advance(20 * 60_000);
    let state = store
        .tick(Some(IdleReport {
            last_input_at: last_input,
        }))
        .unwrap();

    assert_eq!(state.phase, TimerPhase::IdleChallenge);
    assert_eq!(state.idle_from, Some(last_input));
    assert_eq!(state.idle_to, Some(last_input + 20 * 60_000));
    assert_eq!(
        state.elapsed_sec,
        10 * 60,
        "the accumulator is already trimmed back to the last input"
    );

    // Discarding is the honest default and changes nothing further…
    let discarded = store.resolve_idle(IdleAction::Discard).unwrap();
    assert_eq!(discarded.phase, TimerPhase::Running);
    assert_eq!(discarded.elapsed_sec, 10 * 60);

    // …and keeping is one keystroke away.
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Deep work");
    store.start_timer(&t.id, None).unwrap();
    clock.advance(10 * 60_000);
    let last_input = clock.now();
    clock.advance(20 * 60_000);
    store
        .tick(Some(IdleReport {
            last_input_at: last_input,
        }))
        .unwrap();
    let kept = store.resolve_idle(IdleAction::Keep).unwrap();
    assert_eq!(kept.elapsed_sec, 30 * 60);
}

/// U7 — recovery after `kill -9` trims to the last heartbeat by default, ±30s.
#[test]
fn u7_recovery_trims_to_the_last_heartbeat() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Long session");
    store.start_timer(&t.id, None).unwrap();

    // Run for 25 minutes with heartbeats, then the process dies. Wall time
    // marches on for three hours before the next launch.
    for _ in 0..50 {
        clock.advance(30_000);
        store.tick(None).unwrap();
    }
    let died_at = clock.now();
    clock.advance(3 * 3600 * 1000);

    // Boot: the open session blocks the timer until it is resolved.
    let state = store.recover_on_boot().unwrap();
    assert_eq!(state.phase, TimerPhase::Recovering);
    let orphan = state.recovery_session_id.clone().unwrap();
    assert!(matches!(
        store.start_timer(&t.id, None),
        Err(AppError::RecoveryPending)
    ));

    let resolved = store
        .resolve_recovery(&orphan, RecoveryAction::TrimToHeartbeat)
        .unwrap();
    assert_eq!(resolved.phase, TimerPhase::Idle);

    let tracked = store.get_task(&t.id).unwrap().tracked_sec;
    assert!(
        (tracked - 25 * 60).abs() <= 30,
        "trimmed to the last heartbeat (±30s), got {tracked}s"
    );
    let session = store.get_task_detail(&t.id).unwrap().sessions.remove(0);
    assert_eq!(session.source, "recovered");
    assert!(!session.is_confirmed, "flagged until the user confirms it");
    assert!(session.ended_at.unwrap() <= died_at + 30_000);
}

/// U11 — Reconcile never blocks the app; a deferred day auto-accepts after 7.
#[test]
fn u11_deferred_days_auto_accept_after_a_week() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Old plan");
    block(&mut store, &t.id, at(2025, 7, 10, 9, 0), 60);
    block(&mut store, &t.id, at(2025, 7, 29, 9, 0), 60);

    assert_eq!(
        store.unreconciled_days("2025-07-30", 10).unwrap().len(),
        2,
        "both days are waiting, neither blocks anything"
    );
    let accepted = store.auto_accept_stale_days(TZ).unwrap();
    assert_eq!(accepted, vec!["2025-07-10".to_string()]);
    assert_eq!(
        store.unreconciled_days("2025-07-30", 10).unwrap(),
        vec!["2025-07-29".to_string()],
        "yesterday is still the user's call"
    );
}

/// U8 — first run seeds a project containing one already-drifted block.
#[test]
fn u8_first_run_seeds_a_visible_drift_rail() {
    let (mut store, _) = store_at(at(2025, 7, 30, 15, 0));
    store.seed_first_run(TZ).unwrap();
    store.seed_first_run(TZ).unwrap(); // idempotent

    let projects = store.get_projects(TZ).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "Welcome to Fruit");

    let day = &week(&store, "2025-07-30").days[0];
    let drifted = day
        .blocks
        .iter()
        .find(|b| b.drift_state == DriftState::Overrun)
        .expect("the signature rail is visible before the user tracks anything");
    assert_eq!(drifted.planned_sec, 3600);
    assert_eq!(drifted.tracked_sec, 74 * 60);
    assert_eq!(drifted.drift_sec, 14 * 60);
}

// ─── D: data criteria ──────────────────────────────────────────────────

/// D1 — `foreign_keys=ON`, and no orphan rows after a scripted fuzz.
/// D7 — tracked totals from the views equal the caches after the same fuzz.
/// D11 — at most one session has `ended_at IS NULL` at any moment.
#[test]
fn d1_d7_d11_fuzz_leaves_the_database_consistent() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 6, 0));
    let mut rng = Lcg::new(0xF2017);
    let mut tasks: Vec<String> = Vec::new();
    let mut blocks: Vec<String> = Vec::new();

    for step in 0..10_000u32 {
        clock.advance(1_000 + (rng.next() % 60_000) as i64);
        match rng.next() % 10 {
            0..=2 => {
                let t = store
                    .create_task(NewTask {
                        title: format!("fuzz task {step}"),
                        estimate_sec: Some(((rng.next() % 8 + 1) * 900) as i64),
                        ..Default::default()
                    })
                    .unwrap();
                tasks.push(t.id);
            }
            3..=4 if !tasks.is_empty() => {
                let t = &tasks[(rng.next() as usize) % tasks.len()];
                let hour = 6 + (rng.next() % 12) as u32;
                if let Ok(b) = store.schedule_block(NewBlock {
                    task_id: Some(t.clone()),
                    label: None,
                    starts_at: at(2025, 7, 30, hour, 0),
                    duration_sec: ((rng.next() % 4 + 1) * 900) as i64,
                    tz: TZ.into(),
                    is_fixed: false,
                }) {
                    blocks.push(b.id);
                }
            }
            5..=6 if !tasks.is_empty() => {
                let t = tasks[(rng.next() as usize) % tasks.len()].clone();
                let b = (!blocks.is_empty() && rng.next() % 2 == 0)
                    .then(|| blocks[(rng.next() as usize) % blocks.len()].clone());
                store.start_timer(&t, b.as_deref()).unwrap();

                // At most one open session, always (D11).
                let open: i64 = store
                    .connection()
                    .query_row(
                        "SELECT COUNT(*) FROM time_session WHERE ended_at IS NULL",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(open, 1, "exactly one running session at step {step}");
            }
            7 => {
                store.stop_timer().unwrap();
            }
            8 if !tasks.is_empty() => {
                let i = (rng.next() as usize) % tasks.len();
                let id = tasks.remove(i);
                store.stop_timer().unwrap();
                store.delete_task(&id).unwrap();
            }
            _ if !blocks.is_empty() => {
                let id = blocks[(rng.next() as usize) % blocks.len()].clone();
                let hour = 6 + (rng.next() % 12) as u32;
                let _ = store.move_block(&id, at(2025, 7, 30, hour, 0), CollisionPolicy::Overlap);
            }
            _ => {}
        }
    }
    store.stop_timer().unwrap();

    let open: i64 = store
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM time_session WHERE ended_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 0, "D11: nothing left running");

    assert_eq!(
        db::foreign_key_violations(store.connection()).unwrap(),
        0,
        "D1: no orphan rows"
    );
    assert_eq!(db::quick_check(store.connection()).unwrap(), "ok");

    // D7: the caches agree with the views they are derived from.
    let drifted: i64 = store
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM block_tracked v
               JOIN block_tracked_cache c ON c.block_id = v.block_id
              WHERE c.tracked_sec <> v.tracked_sec",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(drifted, 0, "D7: block caches match the view");

    let report = store.run_integrity_check().unwrap();
    assert_eq!(report.quick_check, "ok");
    assert_eq!(report.foreign_key_violations, 0);
    assert_eq!(report.orphan_open_sessions, 0);
    assert_eq!(report.blocks_crossing_midnight, 0);
}

/// D2 — a previous-version fixture migrates, passes `quick_check`, and leaves a
/// pre-migration snapshot behind.
#[test]
fn d2_migration_from_the_previous_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fruit.db");

    // Build a v1 database: the first migration only.
    {
        let conn = db::open(&path).unwrap();
        conn.execute_batch(db::MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1i64).unwrap();
        conn.execute(
            "INSERT INTO task (id, title, status, sort_rank, created_at, updated_at)
             VALUES ('018f0000-0000-7000-8000-000000000001', 'legacy', 'open', 1, 0, 0)",
            [],
        )
        .unwrap();
    }

    let started = std::time::Instant::now();
    let store = Store::open(&path).unwrap();
    let elapsed = started.elapsed();
    assert!(elapsed.as_secs() < 3, "migrated in {elapsed:?}");

    assert_eq!(db::quick_check(store.connection()).unwrap(), "ok");
    let version: i64 = store
        .connection()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, db::schema_version());

    let snapshots: Vec<_> = std::fs::read_dir(db::backups_dir(&path))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("fruit-pre-v1-"))
        .collect();
    assert_eq!(snapshots.len(), 1, "pre-migration snapshot exists");

    // The legacy row survived and picked up its cache entry.
    let cached: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM task_tracked_cache", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cached, 1);
}

/// D3 — opening a database with a higher `user_version` refuses cleanly.
#[test]
fn d3_refuses_a_database_from_the_future() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fruit.db");
    {
        let store = Store::open(&path).unwrap();
        store
            .connection()
            .pragma_update(None, "user_version", db::schema_version() + 1)
            .unwrap();
    }
    match Store::open(&path) {
        Err(AppError::NewerSchema { on_disk, supported }) => {
            assert_eq!(on_disk, supported + 1);
        }
        Err(other) => panic!("expected a clean refusal, got {other:?}"),
        Ok(_) => panic!("expected a clean refusal, the database opened"),
    }
}

/// D5 — deleting the live database and restoring the newest snapshot yields a
/// working app with all the data.
#[test]
fn d5_restore_from_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fruit.db");
    let snapshot = dir.path().join("backups/manual.db");
    {
        let mut store = Store::open(&path).unwrap();
        store.seed_first_run(TZ).unwrap();
        db::snapshot(store.connection(), &snapshot).unwrap();
    }
    std::fs::remove_file(&path).unwrap();
    std::fs::copy(&snapshot, &path).unwrap();

    let store = Store::open(&path).unwrap();
    assert!(store.is_seeded());
    assert_eq!(store.get_projects(TZ).unwrap().len(), 1);
}

/// D6 — export → wipe → import reproduces every entity, ids included.
#[test]
fn d6_export_import_round_trips_exactly() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    store.seed_first_run(TZ).unwrap();
    let t = task(&mut store, "Extra task");
    let b = block(&mut store, &t.id, at(2025, 7, 30, 14, 0), 45);
    store.start_timer(&t.id, Some(&b.id)).unwrap();
    clock.advance(50 * 60_000);
    store.stop_timer().unwrap();
    store.apply_reconcile("2025-07-30", vec![], TZ).unwrap();

    let before = store.export_json(TZ).unwrap();

    // Wipe and reload into a *fresh* database, ids and all.
    let (mut fresh, _) = store_at(at(2025, 8, 1, 9, 0));
    let summary = fresh.import_json(&before, ImportMode::Replace).unwrap();
    assert!(summary.tasks >= 4);

    let after = fresh.export_json(TZ).unwrap();
    for key in [
        "projects", "tasks", "tags", "taskTags", "notes", "blocks", "sessions", "dayReviews",
    ] {
        assert_eq!(
            before[key], after[key],
            "{key} did not round-trip identically"
        );
    }

    // …and re-importing the same file is idempotent under `merge`.
    let again = fresh.import_json(&before, ImportMode::Merge).unwrap();
    assert!(again.skipped > 0);
    assert_eq!(fresh.export_json(TZ).unwrap()["tasks"], before["tasks"]);
}

/// §7.3 — import is untrusted input.
#[test]
fn imports_reject_junk_before_opening_a_transaction() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "existing");

    let not_ours = serde_json::json!({ "format": "todoist.export", "version": 1 });
    assert!(store.import_json(&not_ours, ImportMode::Merge).is_err());

    let from_the_future =
        serde_json::json!({ "format": "fruit.export", "version": 99, "tasks": [] });
    assert!(store.import_json(&from_the_future, ImportMode::Merge).is_err());

    let malformed =
        serde_json::json!({ "format": "fruit.export", "version": 2, "tasks": ["not an object"] });
    assert!(store.import_json(&malformed, ImportMode::Merge).is_err());

    assert_eq!(
        store.get_task(&t.id).unwrap().title,
        "existing",
        "a rejected import changes nothing"
    );
}

/// D8 — a 09:00 block renders at 09:00 before and after a DST transition, and a
/// block at 02:30 on a spring-forward date resolves without error.
#[test]
fn d8_dst_and_timezone_correctness() {
    use chrono::Timelike;
    let (mut store, _) = store_at(at(2025, 3, 25, 9, 0));
    let t = task(&mut store, "Standup");

    // Before the transition (GMT) and after it (BST).
    let before = block(&mut store, &t.id, at(2025, 3, 25, 9, 0), 30);
    let after = block(&mut store, &t.id, at(2025, 4, 1, 9, 0), 30);
    assert_eq!(before.local_date, "2025-03-25");
    assert_eq!(after.local_date, "2025-04-01");
    for b in [&before, &after] {
        assert_eq!(to_local(b.starts_at, &london()).hour(), 9);
        assert_eq!(to_local(b.starts_at, &london()).minute(), 0);
    }

    // 02:30 on the spring-forward date exists (the gap is 01:00–02:00 here).
    let spring = block(&mut store, &t.id, at(2025, 3, 30, 2, 30), 30);
    assert_eq!(spring.local_date, "2025-03-30");

    // Blocks may not cross midnight, on a 23-hour day either.
    let crosses = store.schedule_block(NewBlock {
        task_id: Some(t.id.clone()),
        label: None,
        starts_at: at(2025, 3, 30, 23, 30),
        duration_sec: 3600,
        tz: TZ.into(),
        is_fixed: false,
    });
    assert!(matches!(crosses, Err(AppError::CrossesMidnight)));

    // Reading the same rows from another zone keeps them on their own day.
    let view = store
        .get_week(
            &DateRange {
                from: "2025-03-25".into(),
                to: "2025-04-01".into(),
            },
            "Africa/Addis_Ababa",
        )
        .unwrap();
    let total: usize = view.days.iter().map(|d| d.blocks.len()).sum();
    assert_eq!(total, 3, "no block vanished when the viewing zone changed");
}

/// D9 — moving the clock back one hour mid-session never produces a negative or
/// decreasing `elapsed_sec`.
#[test]
fn d9_a_backwards_clock_never_rewinds_the_timer() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Long haul");
    store.start_timer(&t.id, None).unwrap();

    let mut last = 0;
    for i in 0..120 {
        clock.advance(30_000);
        if i == 40 {
            clock.shift_wall(-3_600_000); // the user fixes their timezone by hand
        }
        let state = store.tick(None).unwrap();
        assert!(
            state.elapsed_sec >= last,
            "elapsed went backwards: {} then {}",
            last,
            state.elapsed_sec
        );
        last = state.elapsed_sec;
    }
    assert_eq!(last, 60 * 60, "monotonic counting is unaffected by the shift");
    store.stop_timer().unwrap();
    assert_eq!(store.get_task(&t.id).unwrap().tracked_sec, 3600);
}

/// D10 — sleeping 45 minutes mid-session does not count the sleep by default,
/// and offers the choice.
#[test]
fn d10_sleep_is_not_counted_by_default() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Interrupted");
    store.start_timer(&t.id, None).unwrap();

    clock.advance(10 * 60_000);
    store.tick(None).unwrap();
    clock.sleep(45 * 60_000); // wall time passes, monotonic does not
    let state = store.tick(None).unwrap();

    assert_eq!(state.phase, TimerPhase::IdleChallenge, "the app asks");
    assert_eq!(
        state.idle_to.unwrap() - state.idle_from.unwrap(),
        45 * 60_000,
        "and names the exact span"
    );
    assert_eq!(state.elapsed_sec, 10 * 60, "the sleep is not counted");

    store.resolve_idle(IdleAction::Discard).unwrap();
    store.stop_timer().unwrap();
    assert_eq!(store.get_task(&t.id).unwrap().tracked_sec, 10 * 60);
}

/// D12's data half — a note with an injection payload is stored verbatim and
/// never interpreted. (The rendering half is a frontend criterion.)
#[test]
fn d12_notes_are_stored_verbatim_not_executed() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Pasted from the web");
    let payload = r#"<img src=x onerror="alert(1)"> and '; DROP TABLE task; --"#;
    store.save_note(&t.id, payload).unwrap();
    assert_eq!(store.get_task_detail(&t.id).unwrap().note, payload);
    let still_there: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM task", [], |r| r.get(0))
        .unwrap();
    assert_eq!(still_there, 1);
}

/// §6.5 — `local_date` always agrees with `starts_at` in `tz`; boot repairs it.
#[test]
fn boot_repairs_a_disagreeing_local_date() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fruit.db");
    let block_id;
    {
        let mut store = Store::open(&path).unwrap();
        let t = task(&mut store, "Standup");
        let b = store
            .schedule_block(NewBlock {
                task_id: Some(t.id),
                label: None,
                starts_at: fruit_core::time::now_ms(),
                duration_sec: 1800,
                tz: TZ.into(),
                is_fixed: false,
            })
            .unwrap();
        block_id = b.id.clone();
        store
            .connection()
            .execute(
                "UPDATE scheduled_block SET local_date = '1999-01-01' WHERE id = ?1",
                [&block_id],
            )
            .unwrap();
    }
    let store = Store::open(&path).unwrap();
    let repaired = store.blocks_on(&local_date(fruit_core::time::now_ms(), &london())).unwrap();
    assert_eq!(repaired.len(), 1);
    assert_eq!(repaired[0].id, block_id);
}

/// §4.6 — deletes are soft, and undo puts the row back.
#[test]
fn deletes_are_soft_and_reversible() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Delete me");
    let child = store
        .create_task(NewTask {
            title: "…and my subtask".into(),
            parent_id: Some(t.id.clone()),
            ..Default::default()
        })
        .unwrap();

    let token = store.delete_task(&t.id).unwrap();
    assert!(token.label.contains("Delete me"));
    assert_eq!(store.get_tasks(TaskQuery::default()).unwrap().total, 0);
    assert!(
        store
            .get_tasks(TaskQuery {
                scope: Some("deleted".into()),
                include_subtasks: true,
                ..Default::default()
            })
            .unwrap()
            .total
            >= 2,
        "the subtask went with it"
    );
    assert_eq!(store.deleted_rows().unwrap().len(), 2);

    store.restore(&token).unwrap();
    assert_eq!(store.get_tasks(TaskQuery::default()).unwrap().total, 1);
    let _ = child;
}

/// §3.7 — the suggested slot is the first gap big enough, respecting blocks.
#[test]
fn next_free_slot_finds_the_first_real_gap() {
    let (mut store, _) = store_at(at(2025, 7, 30, 7, 0));
    let t = task(&mut store, "Filler");
    block(&mut store, &t.id, at(2025, 7, 30, 8, 0), 120); // 08:00–10:00
    block(&mut store, &t.id, at(2025, 7, 30, 10, 30), 60); // 10:30–11:30

    // A 30-minute job fits the 10:00–10:30 gap exactly.
    assert_eq!(
        store
            .next_free_slot("2025-07-30", 30 * 60, None, TZ)
            .unwrap(),
        Some(at(2025, 7, 30, 10, 0))
    );
    // An hour does not, so it lands after the second block.
    assert_eq!(
        store.next_free_slot("2025-07-30", 3600, None, TZ).unwrap(),
        Some(at(2025, 7, 30, 11, 30))
    );
}

// ─── a tiny deterministic RNG, so a fuzz failure is reproducible ────────

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed)
    }
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
}

// ─── the estimate scale, and the completed tail ────────────────────────

/// Rollover is the top of the estimate scale — "doesn't fit one sitting" — and
/// is a different state from "not estimated yet". The two never coexist.
#[test]
fn rollover_and_an_estimate_are_mutually_exclusive() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));

    let both = store.create_task(NewTask {
        title: "confused".into(),
        estimate_sec: Some(3600),
        is_rollover: true,
        ..Default::default()
    });
    assert!(both.is_err(), "a task cannot be estimated *and* a rollover");

    let t = store
        .create_task(NewTask {
            title: "Rewrite the parser".into(),
            is_rollover: true,
            ..Default::default()
        })
        .unwrap();
    assert!(t.is_rollover);
    assert_eq!(t.estimate_sec, None);

    // Choosing an estimate clears the rollover…
    let t = store
        .update_task(
            &t.id,
            TaskPatch {
                estimate_sec: Some(Some(2 * 3600)),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!t.is_rollover);
    assert_eq!(t.estimate_sec, Some(7200));

    // …and choosing rollover clears the estimate.
    let t = store
        .update_task(
            &t.id,
            TaskPatch {
                is_rollover: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(t.is_rollover);
    assert_eq!(t.estimate_sec, None, "no number is left behind to confuse drift");

    // An unestimated task is distinguishable from a rollover — the whole
    // reason this is a column and not a NULL.
    let plain = task(&mut store, "Not thought about yet");
    assert!(!plain.is_rollover);
    assert_eq!(plain.estimate_sec, None);
}

/// A rollover task carries no estimate, so calibration has nothing to compare
/// and must leave it out rather than counting it as a zero.
#[test]
fn rollover_tasks_stay_out_of_calibration() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = store
        .create_task(NewTask {
            title: "Open-ended".into(),
            is_rollover: true,
            ..Default::default()
        })
        .unwrap();
    store.start_timer(&t.id, None).unwrap();
    clock.advance(3 * 3600 * 1000);
    store.stop_timer().unwrap();
    store.set_task_status(&t.id, Status::Done).unwrap();

    let report = store.calibration(TZ, None).unwrap();
    assert_eq!(report.sample_count, 0, "no estimate means no ratio");
}

/// §3.2 — completed tasks belong at the bottom of the project, not gone.
/// A finished project that renders empty is lying about what it cost.
#[test]
fn completed_tasks_land_in_a_group_at_the_bottom() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let project = store
        .create_project(NewProject {
            name: "Ship it".into(),
            colour: None,
            kind: None,
            weekly_target_sec: None,
        })
        .unwrap();

    let mut make = |title: &str, done: bool, store: &mut Store| {
        let t = store
            .create_task(NewTask {
                title: title.into(),
                project_id: Some(project.id.clone()),
                estimate_sec: Some(1800),
                ..Default::default()
            })
            .unwrap();
        if done {
            clock.advance(60_000);
            store.set_task_status(&t.id, Status::Done).unwrap();
        }
        t
    };
    make("Still open", false, &mut store);
    let first = make("Finished first", true, &mut store);
    let last = make("Finished last", true, &mut store);

    let view = store
        .get_backlog(
            BacklogFilter {
                project_id: Some(project.id.clone()),
                ..Default::default()
            },
            TZ,
        )
        .unwrap();

    let keys: Vec<&str> = view.groups.iter().map(|g| g.key.as_str()).collect();
    assert_eq!(
        keys.last(),
        Some(&"done"),
        "Completed is the last group, so it reads as a tail"
    );

    let completed = view.groups.iter().find(|g| g.key == "done").unwrap();
    assert_eq!(completed.label, "Completed");
    assert_eq!(completed.count, 2);
    assert_eq!(completed.estimate_sec, 3600, "totals still add up");
    assert_eq!(
        completed.task_ids,
        vec![last.id.clone(), first.id.clone()],
        "most recently finished first"
    );

    // The open task is untouched by any of this.
    let open: i64 = view
        .groups
        .iter()
        .filter(|g| g.key != "done")
        .map(|g| g.count)
        .sum();
    assert_eq!(open, 1);
}

// ─── sessions are contiguous awake intervals ───────────────────────────

/// The meeting case: the laptop sleeps mid-session. No session row may span
/// the sleep — a row reading 09:00–12:10 for twenty minutes of work is worse
/// than no row, because the start and end times are what you read back later.
#[test]
fn sleep_splits_the_session_instead_of_spanning_it() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Write the proposal");
    store.start_timer(&t.id, None).unwrap();

    // Twenty minutes of real work, with heartbeats.
    for _ in 0..40 {
        clock.advance(30_000);
        store.tick(None).unwrap();
    }
    // Lid closes for a three-hour meeting.
    clock.sleep(3 * 3600 * 1000);
    let state = store.tick(None).unwrap();
    assert_eq!(state.phase, TimerPhase::IdleChallenge);
    assert!(state.session.is_none(), "nothing is recording during the gap");

    // Back at the desk: discard the sleep (the default) and carry on.
    store.resolve_idle(IdleAction::Discard).unwrap();
    for _ in 0..20 {
        clock.advance(30_000);
        store.tick(None).unwrap();
    }
    store.stop_timer().unwrap();

    let sessions = store.get_task_detail(&t.id).unwrap().sessions;
    assert_eq!(sessions.len(), 2, "one segment either side of the sleep");
    for s in &sessions {
        let wall = (s.ended_at.unwrap() - s.started_at) / 1000;
        assert!(
            (wall - s.elapsed_sec).abs() <= 60,
            "a segment's wall span must match what it counted: {}s wall vs {}s counted",
            wall,
            s.elapsed_sec,
        );
        assert!(
            wall < 3600,
            "no segment swallowed the three-hour sleep ({wall}s)"
        );
    }

    // Both ends of every segment are real system-clock instants.
    let first = &sessions[1];
    let second = &sessions[0];
    assert!(second.started_at >= first.ended_at.unwrap());
    assert!(
        second.started_at - first.ended_at.unwrap() >= 3 * 3600 * 1000 - 60_000,
        "the gap in the record is the sleep, and it is visible as a gap"
    );

    // And the total is still only the time actually worked.
    assert!((store.get_task(&t.id).unwrap().tracked_sec - 30 * 60).abs() <= 60);
}

/// Keeping the span is the other half: the split is undone, so the record
/// shows one interval rather than a suspicious pair.
#[test]
fn keeping_an_idle_span_rejoins_the_segment() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Reading a spec");
    store.start_timer(&t.id, None).unwrap();

    clock.advance(10 * 60_000);
    let last_input = clock.now();
    clock.advance(20 * 60_000);
    store
        .tick(Some(IdleReport {
            last_input_at: last_input,
        }))
        .unwrap();

    let kept = store.resolve_idle(IdleAction::Keep).unwrap();
    assert_eq!(kept.phase, TimerPhase::Running);
    assert_eq!(kept.elapsed_sec, 30 * 60, "the span counts");
    store.stop_timer().unwrap();

    let sessions = store.get_task_detail(&t.id).unwrap().sessions;
    assert_eq!(sessions.len(), 1, "kept means one interval, not two");
    assert_eq!(sessions[0].elapsed_sec, 30 * 60);
}

/// Start and stop both take the wall clock, so a session's endpoints are the
/// times a person would recognise — never a figure derived from the counter.
#[test]
fn session_endpoints_come_from_the_system_clock() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 14, 0));
    let t = task(&mut store, "Pairing");
    store.start_timer(&t.id, None).unwrap();
    clock.advance(45 * 60_000);
    store.stop_timer().unwrap();

    let s = &store.get_task_detail(&t.id).unwrap().sessions[0];
    assert_eq!(s.started_at, at(2025, 7, 30, 14, 0));
    assert_eq!(s.ended_at, Some(at(2025, 7, 30, 14, 45)));

    // …and the task carries those bounds for the completed list to show.
    let row = store.get_task(&t.id).unwrap();
    assert_eq!(row.first_session_at, Some(at(2025, 7, 30, 14, 0)));
    assert_eq!(row.last_session_at, Some(at(2025, 7, 30, 14, 45)));
}

/// Switching tasks closes the old run at the real instant it stopped, and
/// never leaves two sessions open (§6.5).
#[test]
fn switching_tasks_closes_the_previous_segment_cleanly() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let a = task(&mut store, "Write intro");
    let b = task(&mut store, "Refactor auth");

    store.start_timer(&a.id, None).unwrap();
    clock.advance(24 * 60_000);
    store.start_timer(&b.id, None).unwrap();

    let open: i64 = store
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM time_session WHERE ended_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 1);

    let first = &store.get_task_detail(&a.id).unwrap().sessions[0];
    assert_eq!(first.ended_at, Some(at(2025, 7, 30, 9, 24)));
    assert_eq!(first.elapsed_sec, 24 * 60);

    clock.advance(60_000);
    store.stop_timer().unwrap();
}

/// A segment with nothing in it is deleted rather than kept — an empty
/// interval in the Sessions tab is noise, not a record.
#[test]
fn zero_length_segments_are_not_recorded() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Started by mistake");
    store.start_timer(&t.id, None).unwrap();
    store.stop_timer().unwrap();
    assert!(store.get_task_detail(&t.id).unwrap().sessions.is_empty());
}
