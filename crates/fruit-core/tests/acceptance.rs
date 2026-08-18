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
use fruit_core::store::{IdleReport, SeriesScope};
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
            rrule: None,
            intent: None,
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
                life_area_id: None,
                rule_for_domain: None,
                rule_category_id: None,
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
                life_area_id: None,
                rule_for_domain: None,
                rule_category_id: None,
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
            contribution: None,
            replace_existing: false,
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
        contribution: None,
        replace_existing: false,
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
                    rrule: None,
                    intent: None,
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

/// The work report: five questions, one range, one round trip.
#[test]
fn the_work_report_answers_all_five_questions() {
    // A Wednesday, so the week has days either side of it.
    let (mut store, clock) = store_at(at(2025, 7, 30, 8, 0));
    let cats = store.get_task_categories().unwrap();
    assert!(
        cats.len() >= 5,
        "migration 0013 should seed the starter categories"
    );
    let coding = cats.iter().find(|c| c.name == "Coding").unwrap().id.clone();
    let meeting = cats.iter().find(|c| c.name == "Meeting").unwrap().id.clone();

    let project = store
        .create_project(NewProject {
            name: "Ledger".into(),
            colour: None,
            kind: None,
            weekly_target_sec: None,
        })
        .unwrap();
    let build = store
        .create_task(NewTask {
            title: "Build the thing".into(),
            project_id: Some(project.id.clone()),
            ..Default::default()
        })
        .unwrap();
    let standup = task(&mut store, "Standup");
    // Deliberately left uncategorised and unassigned to a project, so both
    // fallback buckets are exercised.
    let odds = task(&mut store, "Odds and ends");

    store.set_task_category(&build.id, Some(&coding)).unwrap();
    store.set_task_category(&standup.id, Some(&meeting)).unwrap();

    // 2h coding, 30m meeting, 15m uncategorised — all on the Wednesday.
    for (t, minutes) in [(&build.id, 120), (&standup.id, 30), (&odds.id, 15)] {
        store.start_timer(t, None).unwrap();
        clock.advance(minutes * 60_000);
        store.stop_timer().unwrap();
    }

    // A six-hour weekday target.
    store
        .set_goal(
            NewGoal {
                subject_kind: Some(GoalSubject::Metric),
                subject_id: fruit_core::model::metric::ALL_WORK.into(),
                period: Some("day".into()),
                direction: Some(GoalDirection::AtLeast),
                target_sec: 6 * 3600,
                applies_days: Some(0b0011111),
            },
            "2025-07-30",
        )
        .unwrap();

    // A focus session that ran short of its plotted length.
    store.start_focus(&build.id, Some(45)).unwrap();
    clock.advance(20 * 60_000);
    store.stop_timer().unwrap();

    let day = store.get_work_report("2025-07-30", "day", TZ).unwrap();

    // 1. Work hours, and the target it is measured against.
    assert_eq!(day.period, "day");
    assert_eq!(day.days.len(), 1);
    assert_eq!(day.total_work_sec, (120 + 30 + 15 + 20) * 60);
    assert_eq!(day.target_sec, Some(6 * 3600));
    assert_eq!(day.target_days_applicable, Some(1));
    // 3h05 against a 6h target: applicable, and not met.
    assert_eq!(day.target_days_met, Some(0));
    assert!(day.days[0].target_applies, "Wednesday is a weekday");

    // 2. Split by kind of work. The uncategorised bucket is present, because
    //    the shares have to add up to the total.
    let cat = |name: &str| {
        day.by_category
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.seconds)
    };
    assert_eq!(cat("Coding"), Some((120 + 20) * 60));
    assert_eq!(cat("Meeting"), Some(30 * 60));
    assert_eq!(cat("Uncategorised"), Some(15 * 60));
    let summed: i64 = day.by_category.iter().map(|s| s.seconds).sum();
    assert_eq!(
        summed, day.total_work_sec,
        "the split must account for every second of the total, exactly once"
    );

    // 3. Split by project, with the same rule for work that has none.
    let proj = |name: &str| {
        day.by_project
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.seconds)
    };
    assert_eq!(proj("Ledger"), Some((120 + 20) * 60));
    assert_eq!(proj("No project"), Some((30 + 15) * 60));
    assert_eq!(
        day.by_project.iter().map(|s| s.seconds).sum::<i64>(),
        day.total_work_sec
    );

    // 4. Focus sessions: one, plotted for 45m, run for 20m — and *not* counted
    //    as completed, because it did not run its length.
    assert_eq!(day.focus.sessions, 1);
    assert_eq!(day.focus.planned_sec, 45 * 60);
    assert_eq!(day.focus.tracked_sec, 20 * 60);
    assert_eq!(day.focus.completed, 0);

    // 5. Apps used. Nothing was observed in this fixture, and an empty list is
    //    the honest answer rather than a fabricated one.
    assert!(day.apps.is_empty());

    // The week contains the day, and the days sum to the week's total — the
    // property that makes the graph trustworthy.
    let week = store.get_work_report("2025-07-30", "week", TZ).unwrap();
    assert_eq!(week.days.len(), 7);
    assert_eq!(week.from, "2025-07-28");
    assert_eq!(week.to, "2025-08-03");
    assert_eq!(week.total_work_sec, day.total_work_sec);
    assert_eq!(
        week.days.iter().map(|d| d.work_sec).sum::<i64>(),
        week.total_work_sec
    );
    // Mon–Fri applicable, and only the days that have happened are counted —
    // Thursday and Friday have not, and a day that has not arrived is not a
    // day you failed to work six hours on.
    assert_eq!(week.target_days_applicable, Some(3));
    assert_eq!(week.days.iter().filter(|d| d.target_applies).count(), 5);

    // A month is the same shape with different bounds.
    let month = store.get_work_report("2025-07-30", "month", TZ).unwrap();
    assert_eq!((month.from.as_str(), month.to.as_str()), ("2025-07-01", "2025-07-31"));
    assert_eq!(month.days.len(), 31);
    assert_eq!(month.total_work_sec, day.total_work_sec);

    // December is the rollover case a naive `month + 1` gets wrong.
    let december = store.get_work_report("2025-12-14", "month", TZ).unwrap();
    assert_eq!((december.from.as_str(), december.to.as_str()), ("2025-12-01", "2025-12-31"));
}

/// The analytics the report is built on: the hour histogram, the day span, the
/// four-period baseline, and the score's arithmetic.
#[test]
fn the_work_report_compares_against_history_and_shows_its_arithmetic() {
    let (mut store, clock) = store_at(at(2025, 7, 7, 8, 0));
    let t = task(&mut store, "The work");

    // Four prior weeks with two hours on the Monday of each, then this week
    // with four. The baseline should read 2h and this week should read 4h.
    for week in 0..5 {
        let monday = at(2025, 7, 7, 9, 0) + week * 7 * 86_400_000;
        clock.set(monday);
        store.start_timer(&t.id, None).unwrap();
        clock.advance(if week == 4 { 4 * 3_600_000 } else { 2 * 3_600_000 });
        store.stop_timer().unwrap();
    }

    let report = store.get_work_report("2025-08-04", "week", TZ).unwrap();
    assert_eq!(report.total_work_sec, 4 * 3600);

    let base = report.baseline.clone().expect("four prior weeks all had work in them");
    assert_eq!(base.periods, 4);
    assert_eq!(base.total_work_sec, 2 * 3600);

    // The hour histogram splits at hour boundaries and sums to the total, so a
    // session crossing an hour is counted in both — for the minutes it spent in
    // each, once.
    assert_eq!(report.by_hour.len(), 24);
    assert_eq!(report.by_hour.iter().sum::<i64>(), report.total_work_sec);
    assert_eq!(report.by_hour[9], 3600, "09:00–10:00 is a full hour");
    assert_eq!(report.by_hour[12], 3600, "…and so is 12:00–13:00");
    assert_eq!(report.by_hour[13], 0, "work stopped at 13:00");

    // The span of the day, which is a different fact from its length.
    let monday = report.days.iter().find(|d| d.date == "2025-08-04").unwrap();
    assert_eq!(monday.work_sec, 4 * 3600);
    assert!(monday.first_at.is_some() && monday.last_at.is_some());
    assert_eq!(
        (monday.last_at.unwrap() - monday.first_at.unwrap()) / 1000,
        4 * 3600
    );

    // The score is a weighted sum of components that are each reported, so the
    // panel can print the arithmetic rather than asking to be trusted.
    assert!(report.score.scored);
    let summed: f64 = report
        .score
        .components
        .iter()
        .map(|c| c.value as f64 * c.weight)
        .sum();
    assert_eq!(report.score.value, summed.round() as i64);
    assert_eq!(
        report.score.components.iter().map(|c| c.weight).sum::<f64>(),
        1.0,
        "the weights have to be a whole"
    );
    assert!(!report.score.rating.is_empty());

    // A period with nothing in it is *not scored*, rather than scored zero. A
    // 0/100 for a week on holiday is a verdict the app has no business giving.
    let empty = store.get_work_report("2024-01-08", "week", TZ).unwrap();
    assert!(!empty.score.scored);
    assert_eq!(empty.score.rating, "Not scored");
    // …and it has no baseline either, rather than a fabricated one.
    assert!(empty.baseline.is_none());
    // …and says nothing at all, rather than generating prose about a week that
    // did not happen. Findings invented from absent data are worse than none.
    assert!(empty.findings.is_empty());

    // The findings read the figures back. This week doubled the baseline, so
    // the comparison is stated with both numbers in it.
    let by_id = |r: &WorkReport, id: &str| {
        r.findings.iter().find(|f| f.id == id).cloned()
    };
    let total = by_id(&report, "total-vs-baseline").expect("4h against a 2h baseline is a finding");
    assert!(total.headline.contains("more"), "{}", total.headline);
    assert!(total.detail.contains("4h 00m") && total.detail.contains("2h 00m"));

    // No target, no target finding — rather than one measured against zero.
    assert!(by_id(&report, "target").is_none());

    // The longest day is named by its weekday. "Monday carried 100%" is a
    // sentence; "2025-08-04 carried 100%" is a log line.
    if let Some(longest) = by_id(&report, "longest-day") {
        assert!(longest.headline.starts_with("Mon"), "{}", longest.headline);
    }

    // Nothing was categorised and nothing was a focus session, and the report
    // says both. A blind spot named is worth more than a chart of one colour.
    assert!(by_id(&report, "uncategorised").is_some());
    assert!(by_id(&report, "no-focus").is_some());

    // A change under a tenth is noise and is not reported as a trend — calling
    // it one would teach the reader to ignore the findings that matter.
    let (mut quiet, clock2) = store_at(at(2025, 7, 7, 9, 0));
    let q = task(&mut quiet, "Steady");
    for week in 0..5 {
        clock2.set(at(2025, 7, 7, 9, 0) + week * 7 * 86_400_000);
        quiet.start_timer(&q.id, None).unwrap();
        clock2.advance(2 * 3_600_000);
        quiet.stop_timer().unwrap();
    }
    let steady = quiet.get_work_report("2025-08-04", "week", TZ).unwrap();
    assert_eq!(steady.baseline.as_ref().unwrap().total_work_sec, 2 * 3600);
    assert!(
        steady.findings.iter().all(|f| f.id != "total-vs-baseline"),
        "an identical week is not a trend"
    );
}

/// A category is a taxonomy, so deleting one frees its tasks rather than
/// destroying them — and never touches the time recorded against them.
#[test]
fn deleting_a_category_frees_its_tasks_and_keeps_their_time() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let mine = store.create_task_category("Ops", Some("#123456")).unwrap();
    assert!(!mine.is_builtin);

    let t = task(&mut store, "Rotate the certs");
    store.set_task_category(&t.id, Some(&mine.id)).unwrap();
    store.start_timer(&t.id, None).unwrap();
    clock.advance(40 * 60_000);
    store.stop_timer().unwrap();

    // Two categories cannot share a name — a split report is a bug report.
    assert!(store.create_task_category("ops", None).is_err());

    // Renaming a built-in makes it the user's, so it stops claiming to be ours.
    let coding = store
        .get_task_categories()
        .unwrap()
        .into_iter()
        .find(|c| c.name == "Coding")
        .unwrap();
    let renamed = store
        .update_task_category(&coding.id, Some("Engineering"), None)
        .unwrap();
    assert_eq!(renamed.name, "Engineering");
    assert!(!renamed.is_builtin);

    let freed = store.delete_task_category(&mine.id).unwrap();
    assert_eq!(freed, 1);
    assert!(!store
        .get_task_categories()
        .unwrap()
        .iter()
        .any(|c| c.id == mine.id));

    // The task is still there, still open, and still carries its 40 minutes.
    let report = store.get_work_report("2025-07-30", "day", TZ).unwrap();
    assert_eq!(report.total_work_sec, 40 * 60);
    assert_eq!(
        report
            .by_category
            .iter()
            .find(|s| s.name == "Uncategorised")
            .map(|s| s.seconds),
        Some(40 * 60),
        "freed time lands in the uncategorised bucket, not nowhere"
    );
}

/// Reassigning a record to a different task — the commonest correction there
/// is, and the one the Day view had no control for.
#[test]
fn a_session_can_be_moved_to_another_task() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let wrong = task(&mut store, "Filed against this by mistake");
    let right = task(&mut store, "Where it actually belongs");
    let b = block(&mut store, &wrong.id, at(2025, 7, 30, 9, 0), 60);

    store.start_timer(&wrong.id, Some(&b.id)).unwrap();
    clock.advance(40 * 60_000);
    let session = store
        .timer_state()
        .unwrap()
        .session
        .expect("a running session");
    store.stop_timer().unwrap();
    let session = store
        .set_session_contribution(&session.id, Some(Contribution::Attend))
        .unwrap();

    let moved = store
        .update_session(
            &session.id,
            SessionPatch {
                task_id: Some(right.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(moved.task_id, right.id);
    // The row survives the move: same id, and the contribution mode the user
    // chose deliberately is still there. That is the whole reason this is a
    // reassignment rather than a delete plus a re-add.
    assert_eq!(moved.id, session.id);
    assert_eq!(moved.contribution, Some(Contribution::Attend));
    assert_eq!(moved.elapsed_sec, session.elapsed_sec);

    // The block belonged to the *old* task. `block_tracked` sums every session
    // carrying a block's id, so leaving it attached would credit the new task's
    // minutes to the old task's plot and hang a drift rail on a block nobody
    // worked. It detaches.
    assert_eq!(moved.block_id, None);

    // And the minutes moved with it, in both directions.
    let tasks = store.get_tasks(TaskQuery::default()).unwrap();
    let find = |id: &str| tasks.rows.iter().find(|t| t.id == id).unwrap().tracked_sec;
    assert_eq!(find(&wrong.id), 0);
    assert_eq!(find(&right.id), session.elapsed_sec);

    // A block that *does* belong to the destination task is kept, because there
    // is nothing to misattribute.
    let b2 = block(&mut store, &right.id, at(2025, 7, 30, 14, 0), 60);
    store
        .update_session(
            &moved.id,
            SessionPatch {
                block_id: Some(Some(b2.id.clone())),
                ..Default::default()
            },
        )
        .unwrap();
    let again = store
        .update_session(
            &moved.id,
            SessionPatch {
                task_id: Some(right.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(again.block_id, Some(b2.id));

    // A task that does not exist is refused rather than written.
    assert!(store
        .update_session(
            &moved.id,
            SessionPatch {
                task_id: Some("01890000-0000-7000-8000-000000000000".into()),
                ..Default::default()
            },
        )
        .is_err());
}

/// The testing reset: everything the user recorded goes, the built-in taxonomy
/// and the configuration stay, and the database is left in a state the app can
/// keep running against rather than one that needs a restart.
#[test]
fn reset_empties_user_data_and_keeps_the_builtins() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    store.seed_first_run(TZ).unwrap();
    store.seed_life_areas().unwrap();

    // A full day's worth of every record type, plus a running timer.
    let t = task(&mut store, "Extra task");
    let b = block(&mut store, &t.id, at(2025, 7, 30, 14, 0), 45);
    store.start_timer(&t.id, Some(&b.id)).unwrap();
    clock.advance(50 * 60_000);
    store.stop_timer().unwrap();
    store.apply_reconcile("2025-07-30", vec![], TZ).unwrap();

    let areas = store.get_life_areas(TZ, false).unwrap();
    let builtin_areas = areas.len();
    assert!(builtin_areas > 0, "the fixture needs built-in life areas");
    let mine = store
        .create_life_area(NewLifeArea {
            name: "Something I added".into(),
            ..Default::default()
        })
        .unwrap();

    // Configuration the tester set, which must survive.
    store
        .set_setting("general.hour12", &serde_json::json!(true))
        .unwrap();
    // …and runtime state, which must not.
    store
        .set_setting("notices.fired", &serde_json::json!(["off-plan"]))
        .unwrap();

    // A second timer left running, so the reset has a live pointer to clear.
    store.start_timer(&t.id, None).unwrap();

    let summary = store.reset_all_data().unwrap();
    assert!(summary.projects >= 1, "should report what it destroyed");
    assert!(summary.tasks >= 4);
    assert!(summary.sessions >= 1);

    // Nothing the user recorded is left — including soft-deleted rows, so the
    // next run does not open with a restore list from the last one.
    let export = store.export_json(TZ).unwrap();
    for key in [
        "projects", "tasks", "tags", "taskTags", "notes", "blocks", "sessions", "dayReviews",
    ] {
        assert_eq!(
            export[key].as_array().unwrap().len(),
            0,
            "{key} survived the reset"
        );
    }

    // The built-in taxonomy is schema, not data: no migration will ever create
    // it again, so it stays. What the user added to it goes.
    let after = store.get_life_areas(TZ, true).unwrap();
    assert_eq!(after.len(), builtin_areas, "built-in life areas must survive");
    assert!(!after.iter().any(|a| a.id == mine.id), "user areas must go");
    assert!(
        !store.get_categories(None, TZ).unwrap().is_empty(),
        "built-in observation categories must survive"
    );

    // Configuration survives; runtime state does not.
    assert_eq!(
        store.get_setting("general.hour12").unwrap(),
        Some(serde_json::json!(true))
    );
    assert_eq!(store.get_setting("notices.fired").unwrap(), None);

    // The seed flag stays set: the point of the button is a database that stays
    // empty, not one that grows the demo project back on the next launch.
    assert!(store.is_seeded());
    store.seed_first_run(TZ).unwrap();
    assert_eq!(
        store.export_json(TZ).unwrap()["tasks"].as_array().unwrap().len(),
        0
    );

    // The store is still usable in-process: the timer runtime let go of the
    // session row it was holding, and a fresh day reads as genuinely empty.
    assert!(store.timer_state().unwrap().session.is_none());
    let day = store.get_day("2025-07-30", TZ, Some(30)).unwrap();
    assert_eq!(
        day.slots.iter().filter(|s| s.state != SlotState::Empty).count(),
        0,
        "every slot should read as unaccounted after a reset"
    );
    let t2 = task(&mut store, "After the reset");
    assert_eq!(t2.title, "After the reset");
}

/// The reset's escape hatch: on a real, file-backed database it snapshots
/// first, and that snapshot still holds everything the reset destroyed.
///
/// Worth its own test because the in-memory store takes the other branch
/// entirely — there is no file to copy, so `backup_path` is `None` and the
/// snapshot code never runs. "Cannot be undone" is only acceptable copy if the
/// file it points at is really there and really loads.
#[test]
fn reset_snapshots_first_and_the_snapshot_restores() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fruit.db");

    let backup = {
        let mut store = Store::open(&path).unwrap();
        store.seed_first_run(TZ).unwrap();
        let before = store.export_json(TZ).unwrap();
        let tasks_before = before["tasks"].as_array().unwrap().len();
        assert!(tasks_before > 0);

        let summary = store.reset_all_data().unwrap();
        let backup = summary.backup_path.expect("a file-backed store must snapshot");
        assert!(
            std::path::Path::new(&backup).exists(),
            "the snapshot the UI points the user at has to exist"
        );

        // The live database really is empty…
        assert_eq!(
            store.export_json(TZ).unwrap()["tasks"].as_array().unwrap().len(),
            0
        );
        backup
    };

    // …and the snapshot really does still hold what was there.
    let restored = Store::open(std::path::Path::new(&backup)).unwrap();
    assert!(
        restored.export_json(TZ).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .len()
            > 0,
        "the snapshot should predate the reset, not mirror it"
    );
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
        rrule: None,
        intent: None,
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
                // 09:00 local today, not `now`: a 30-minute block started at
                // the real wall clock crosses midnight when the suite happens
                // to run late in the evening, and this test is about
                // `local_date` repair rather than about midnight.
                starts_at: fruit_core::time::day_start(
                    fruit_core::time::parse_date(&local_date(
                        fruit_core::time::now_ms(),
                        &london(),
                    ))
                    .unwrap(),
                    &london(),
                ) + 9 * 3_600_000,
                duration_sec: 1800,
                tz: TZ.into(),
                is_fixed: false,
                rrule: None,
                intent: None,
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

    let make = |title: &str, done: bool, store: &mut Store| {
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

/// The two observed panels, carried by the work report over its whole range.
///
/// They used to be day-only, on the Activity screen. Moving them onto a report
/// with a day/week/month switcher means they have to answer for a week — so the
/// test plots blocks and records samples on *two* days and asks for the week,
/// which a day-shaped implementation would half-answer.
#[test]
fn the_work_report_carries_observed_labels_and_the_plan_comparison_for_its_whole_range() {
    // Wednesday and Thursday of the same ISO week.
    let (mut store, _clock) = store_at(at(2025, 7, 30, 8, 0));
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();

    let wed = task(&mut store, "Refactor auth");
    let thu = task(&mut store, "Write the migration guide");
    let b_wed = block(&mut store, &wed.id, at(2025, 7, 30, 9, 0), 60);
    let b_thu = block(&mut store, &thu.id, at(2025, 7, 31, 14, 0), 60);

    let mut watch = |app: &str, day_start: i64, minutes: i64| {
        for m in 0..minutes {
            store
                .record_activity(ActivitySample {
                    app_id: app.into(),
                    window_title: None,
                    domain: None,
                    at: day_start + m * 60_000,
                })
                .unwrap();
        }
    };
    watch("code.exe", at(2025, 7, 30, 9, 0), 60);
    watch("slack.exe", at(2025, 7, 31, 14, 0), 60);

    // The day report sees only its own day.
    let day = store.get_work_report("2025-07-30", "day", TZ).unwrap();
    assert_eq!(
        day.correlations.iter().map(|c| c.block_id.as_str()).collect::<Vec<_>>(),
        vec![b_wed.id.as_str()],
        "a day report compares the plan for that day and no other"
    );

    // The week report sees both, oldest first.
    let week = store.get_work_report("2025-07-30", "week", TZ).unwrap();
    assert_eq!(
        week.correlations.iter().map(|c| c.block_id.as_str()).collect::<Vec<_>>(),
        vec![b_wed.id.as_str(), b_thu.id.as_str()],
        "a week report reaches every day in the week, in time order"
    );
    assert_eq!(week.correlations[0].title, "Refactor auth");
    assert_eq!(week.correlations[1].top_apps[0].app_id, "slack.exe");

    // Nothing is labelled yet, so the label panel is empty rather than a column
    // of zeroes — and the report says so by being empty, not by inventing rows.
    assert!(
        week.by_label.is_empty(),
        "an unused label is not news: {:?}",
        week.by_label
    );

    // Label Wednesday's hour, and only that hour shows up.
    let cats = store.get_categories(None, TZ).unwrap();
    let work = cats.iter().find(|c| c.name == "Work").expect("a built-in Work label");
    let spans = store.get_activity_day("2025-07-30", TZ).unwrap().spans;
    // Taken from the span rather than written as a round hour: a span runs from
    // its first sample to its last plus one sampling interval, so sixty samples
    // a minute apart are not sixty minutes. Restating that arithmetic here
    // would make the test a second copy of it.
    let observed = (spans[0].ended_at - spans[0].started_at) / 1000;
    store.set_span_category(spans[0].id, Some(&work.id)).unwrap();

    let week = store.get_work_report("2025-07-30", "week", TZ).unwrap();
    let labelled: Vec<(&str, i64)> =
        week.by_label.iter().map(|c| (c.name.as_str(), c.seconds)).collect();
    assert_eq!(labelled, vec![("Work", observed)], "one label, one span's worth");

    // …and it is *observed* time, which must never have been folded into the
    // confirmed work total. No timer ran in this test at all.
    assert_eq!(
        week.total_work_sec, 0,
        "observed time is not work until somebody confirms it"
    );
}

/// Renaming a project.
///
/// A project's name is not a label sitting beside its id — the capture parser
/// matches `#project` against it (§4.4 rule 7), so a rename changes what typing
/// into the capture bar resolves to. The id is what everything else holds, so
/// the tasks stay attached and nothing is re-pointed.
#[test]
fn a_project_can_be_renamed_and_keeps_everything_attached() {
    let (mut store, _clock) = store_at(at(2025, 7, 30, 9, 0));
    let p = store
        .create_project(NewProject {
            name: "Ledger".into(),
            colour: None,
            kind: None,
            weekly_target_sec: None,
        })
        .unwrap();
    let t = store
        .create_task(NewTask {
            title: "Reconcile June".into(),
            project_id: Some(p.id.clone()),
            ..Default::default()
        })
        .unwrap();

    let renamed = store
        .update_project(&p.id, ProjectPatch { name: Some("  Accounts  ".into()), ..Default::default() })
        .unwrap();

    assert_eq!(renamed.name, "Accounts", "the name is trimmed, as on create");
    assert_eq!(renamed.id, p.id, "a rename is not a new project");
    assert_eq!(renamed.colour, p.colour, "renaming changes the name and nothing else");
    assert_eq!(renamed.open_task_count, 1, "the task is still attached");
    assert_eq!(
        store.get_task_detail(&t.id).unwrap().task.project_id.as_deref(),
        Some(p.id.as_str()),
        "tasks hold the id, so a rename never re-points them"
    );

    // The parser follows the new name, and stops answering to the old one.
    let names = store.project_names().unwrap();
    assert!(names.iter().any(|(n, id)| n == "Accounts" && id == &p.id));
    assert!(!names.iter().any(|(n, _)| n == "Ledger"));
}

/// The same gate on both paths.
///
/// `create_project` has always capped names at 120 characters and rejected
/// blank ones. Renaming used to skip the cap, which made it a cap you could
/// walk straight past by creating a project and then renaming it.
#[test]
fn renaming_a_project_is_held_to_the_same_rules_as_naming_one() {
    let (mut store, _clock) = store_at(at(2025, 7, 30, 9, 0));
    let p = store
        .create_project(NewProject {
            name: "Ledger".into(),
            colour: None,
            kind: None,
            weekly_target_sec: None,
        })
        .unwrap();

    let patch = |name: &str| ProjectPatch { name: Some(name.into()), ..Default::default() };

    assert!(store.update_project(&p.id, patch("")).is_err(), "blank");
    assert!(store.update_project(&p.id, patch("   ")).is_err(), "whitespace is blank");
    assert!(
        store.update_project(&p.id, patch(&"x".repeat(121))).is_err(),
        "past the cap that create enforces"
    );
    assert!(store.update_project(&p.id, patch(&"x".repeat(120))).is_ok(), "at the cap");

    // A rejected rename leaves the project alone rather than half-applying.
    let _ = store.update_project(&p.id, patch(""));
    let after = store.get_projects(TZ).unwrap();
    assert_eq!(after.iter().find(|r| r.id == p.id).unwrap().name, "x".repeat(120));
}

/// Keeping an idle span, then **carrying on working**.
///
/// The existing keep test stops the timer immediately afterwards. The real app
/// ticks once a second and the session runs on for a while, which is the shape
/// a user actually produces — and the shape a report of "the start and end are
/// right but the duration is wrong" points at.
#[test]
fn a_kept_idle_span_survives_the_ticks_that_follow_it() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 11, 33));
    let t = task(&mut store, "Adey - ATI");
    store.start_timer(&t.id, None).unwrap();

    // Seven minutes of work, then fourteen idle.
    clock.advance(7 * 60_000);
    let last_input = clock.now();
    clock.advance(14 * 60_000);
    store.tick(Some(IdleReport { last_input_at: last_input })).unwrap();
    assert_eq!(store.timer_state().unwrap().phase, TimerPhase::IdleChallenge);

    let kept = store.resolve_idle(IdleAction::Keep).unwrap();
    assert_eq!(kept.elapsed_sec, 21 * 60, "the whole span counts");

    // …and then the app goes on ticking, as it does every second in real use.
    for _ in 0..12 {
        clock.advance(5_000);
        store.tick(None).unwrap();
    }
    assert_eq!(
        store.timer_state().unwrap().elapsed_sec,
        22 * 60,
        "a minute of ticks after the keep, on top of the twenty-one"
    );

    store.stop_timer().unwrap();
    let sessions = store.get_task_detail(&t.id).unwrap().sessions;
    assert_eq!(sessions.len(), 1, "kept means one interval, not two");
    let s = &sessions[0];
    // The reported failure: endpoints right, duration wrong. Assert both, and
    // assert they agree — a session whose elapsed does not match its own span
    // is the thing the screenshot showed.
    assert_eq!(s.elapsed_sec, 22 * 60);
    assert_eq!(
        (s.ended_at.unwrap() - s.started_at) / 1000,
        s.elapsed_sec,
        "a kept span means the record is continuous, so the two must agree"
    );
}

/// **Keeping a span that outlasted what was counted inside it.**
///
/// The rollback that opens the challenge is `(run_ms - idle_ms).max(0)`, and a
/// clamp is not reversible by addition. `idle_ms` is wall time since the last
/// input; the accumulator only holds what the *monotonic* clock counted. A
/// laptop on modern standby naps in short bursts — each divergence too small to
/// read as a suspend, so no burst trips the sleep branch — and over a quarter of
/// an hour the two drift a long way apart. `idle_ms` then exceeds everything
/// ever counted, the clamp throws the excess away, and the old
/// `run_ms += pending_ms` could not give it back.
///
/// The session read with **correct endpoints and a short duration**, which is
/// exactly what a kept span must never do, and exactly how it was reported.
#[test]
fn keeping_a_span_longer_than_what_was_counted_returns_all_of_it() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 11, 33));
    let t = task(&mut store, "Adey - ATI");
    store.start_timer(&t.id, None).unwrap();

    // Seven minutes of real work.
    clock.advance(7 * 60_000);
    let last_input = clock.now();

    // Fourteen minutes of the machine dozing: twenty-eight naps of half a
    // minute each, every one of them under the sixty-second suspend threshold,
    // with a tick between so no single divergence is ever large enough to be
    // read as a sleep. Wall time runs on; almost nothing is counted.
    for _ in 0..28 {
        clock.sleep(30_000);
        store.tick(None).unwrap();
    }

    store.tick(Some(IdleReport { last_input_at: last_input })).unwrap();
    assert_eq!(
        store.timer_state().unwrap().phase,
        TimerPhase::IdleChallenge,
        "fourteen minutes without input is an idle challenge"
    );

    let kept = store.resolve_idle(IdleAction::Keep).unwrap();
    assert_eq!(
        kept.elapsed_sec,
        21 * 60,
        "keeping means the whole span counts, not whatever survived the clamp"
    );

    store.stop_timer().unwrap();
    let sessions = store.get_task_detail(&t.id).unwrap().sessions;
    assert_eq!(sessions.len(), 1, "kept means one interval, not two");
    assert_eq!(sessions[0].elapsed_sec, 21 * 60);
    assert_eq!(
        (sessions[0].ended_at.unwrap() - sessions[0].started_at) / 1000,
        sessions[0].elapsed_sec,
        "a kept span is continuous, so its duration must match its own endpoints"
    );
}

/// The other half of the reprieve.
///
/// A challenge closes its segment without deleting it, so that keeping the span
/// has a row to reopen. Answering anything else ends the reprieve: an interval
/// with nothing counted in it is noise, and must not survive in the Sessions
/// tab just because it was once the subject of a question.
#[test]
fn a_challenged_segment_with_nothing_in_it_is_not_kept_by_refusing_it() {
    for action in [IdleAction::Discard, IdleAction::AssignToBreak] {
        let (mut store, clock) = store_at(at(2025, 7, 30, 11, 33));
        let t = task(&mut store, "Adey - ATI");
        store.start_timer(&t.id, None).unwrap();

        // The machine dozes from the moment the timer starts, so the segment
        // ends up holding no counted time at all.
        let last_input = clock.now();
        for _ in 0..28 {
            clock.sleep(30_000);
            store.tick(None).unwrap();
        }
        store.tick(Some(IdleReport { last_input_at: last_input })).unwrap();
        assert_eq!(store.timer_state().unwrap().phase, TimerPhase::IdleChallenge);

        store.resolve_idle(action).unwrap();
        store.stop_timer().unwrap();

        let sessions = store.get_task_detail(&t.id).unwrap().sessions;
        assert!(
            sessions.iter().all(|s| s.elapsed_sec > 0),
            "{action:?} left an empty interval behind: {sessions:?}"
        );
    }
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

// ─── P2: recurring blocks, .ics import, activity ───────────────────────

/// §2.3 — a recurring block is a *series of real blocks*, not a virtual
/// pattern, because a session links to a block by id and drift is per block.
#[test]
fn a_recurring_block_produces_trackable_instances() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 8, 0));
    let t = task(&mut store, "Stand-up");

    let created = store
        .schedule_recurring(
            NewBlock {
                task_id: Some(t.id.clone()),
                starts_at: at(2025, 7, 30, 9, 0),
                duration_sec: 900,
                tz: TZ.into(),
                ..Default::default()
            },
            "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
        )
        .unwrap();
    assert!(created.len() > 40, "90 days of weekdays, got {}", created.len());

    let series_id = created[0].series_id.clone().expect("the seed carries the series");
    assert!(created.iter().all(|b| b.series_id.as_deref() == Some(&series_id)));
    assert_eq!(
        created[0].rrule.as_deref(),
        Some("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"),
        "the rule lives on the seed, so it can always be read back"
    );

    // Every instance is an ordinary block: trackable, and it carries drift.
    let instance = &created[1];
    store.start_timer(&t.id, Some(&instance.id)).unwrap();
    clock.advance(20 * 60_000);
    store.stop_timer().unwrap();

    let day = &week(&store, &instance.local_date).days[0];
    let view = day.blocks.iter().find(|b| b.block.id == instance.id).unwrap();
    assert_eq!(view.drift_state, DriftState::Overrun);
    assert_eq!(view.drift_sec, 5 * 60);

    // Weekends are not in the series.
    assert!(
        !created
            .iter()
            .any(|b| ["2025-08-02", "2025-08-03"].contains(&b.local_date.as_str())),
        "BYDAY=MO..FR means no Saturday or Sunday"
    );
}

/// Editing a series is where calendars get it wrong, so the scope is explicit:
/// this one, this and later, or all of them.
#[test]
fn series_edits_have_an_explicit_scope() {
    let (mut store, _) = store_at(at(2025, 7, 30, 8, 0));
    let t = task(&mut store, "Daily review");
    let created = store
        .schedule_recurring(
            NewBlock {
                task_id: Some(t.id.clone()),
                starts_at: at(2025, 7, 30, 17, 0),
                duration_sec: 900,
                tz: TZ.into(),
                ..Default::default()
            },
            "FREQ=DAILY;COUNT=10",
        )
        .unwrap();
    assert_eq!(created.len(), 10);

    // One instance only.
    store
        .unschedule_series(&created[2].id, SeriesScope::Instance)
        .unwrap();
    assert_eq!(remaining(&store, &created), 9);

    // This one and everything after it.
    let token = store
        .unschedule_series(&created[5].id, SeriesScope::Future)
        .unwrap();
    assert_eq!(
        remaining(&store, &created),
        4,
        "instances 5-9 go with it, leaving 0, 1, 3 and 4"
    );

    // …and undo puts the series back, like every other delete (§4.6).
    store.restore(&token).unwrap();
    assert_eq!(remaining(&store, &created), 10);

    store
        .unschedule_series(&created[0].id, SeriesScope::All)
        .unwrap();
    assert_eq!(remaining(&store, &created), 0);
}

/// §4.3 — "make this repeat" acts on the block you selected, in place.
///
/// The alternative (delete it, create a repeating one) would orphan the tracked
/// time already sitting against it, which is exactly the kind of quiet data loss
/// the block/session split exists to prevent (§6.1 rule 7).
#[test]
fn repeating_an_existing_block_keeps_its_tracked_time() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 8, 0));
    let t = task(&mut store, "Weekly review");
    let block = store
        .schedule_block(NewBlock {
            task_id: Some(t.id.clone()),
            starts_at: at(2025, 7, 30, 16, 0),
            duration_sec: 3600,
            tz: TZ.into(),
            ..Default::default()
        })
        .unwrap();

    store.start_timer(&t.id, Some(&block.id)).unwrap();
    clock.advance(30 * 60_000);
    store.stop_timer().unwrap();

    let created = store.repeat_block(&block.id, "FREQ=WEEKLY;COUNT=4").unwrap();
    assert_eq!(created.len(), 4);
    assert_eq!(created[0].id, block.id, "the seed is the block you selected");
    assert!(created.iter().all(|b| b.series_id == created[0].series_id));

    // The half hour is still attributed to it.
    let day = &week(&store, &block.local_date).days[0];
    let view = day.blocks.iter().find(|b| b.block.id == block.id).unwrap();
    assert_eq!(view.tracked_sec, 30 * 60);

    // Changing a live rule would mean silently rewriting occurrences someone
    // may already have tracked against, so it is refused rather than guessed.
    let again = store.repeat_block(&block.id, "FREQ=DAILY");
    assert!(again.is_err(), "a block that already repeats refuses a second rule");
}

/// The picker's list is generated from the same code that parses it, so it can
/// never offer a rule the engine would refuse (§4.3).
#[test]
fn every_repeat_preset_parses_and_describes_itself() {
    let presets = fruit_core::rrule::presets();
    assert_eq!(presets.len(), fruit_core::rrule::PRESETS.len());
    for p in &presets {
        assert!(
            fruit_core::rrule::Rrule::parse(&p.rrule).is_ok(),
            "{} offers an unparseable rule",
            p.label
        );
        assert!(!p.description.is_empty(), "{} has no description", p.label);
    }
}

fn remaining(store: &Store, created: &[BlockRow]) -> usize {
    created
        .iter()
        .filter(|b| store.block_row_public(&b.id).is_some())
        .count()
}

/// §2.3 — `.ics` import, read-only and fully offline. Imported events become
/// *fixed* blocks: an external meeting is the thing you plan around.
#[test]
fn ics_import_creates_fixed_blocks_and_is_idempotent() {
    let (mut store, _) = store_at(at(2025, 7, 28, 8, 0));
    let ics = "\
BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
UID:standup@corp\r
DTSTART;TZID=Europe/London:20250730T093000\r
DTEND;TZID=Europe/London:20250730T094500\r
SUMMARY:Stand-up\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:1to1@corp\r
DTSTART;TZID=Europe/London:20250731T140000\r
DURATION:PT30M\r
SUMMARY:1:1\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:offsite@corp\r
DTSTART;VALUE=DATE:20250801\r
DTEND;VALUE=DATE:20250802\r
SUMMARY:Offsite\r
END:VEVENT\r
END:VCALENDAR\r
";
    let summary = store.import_ics_text(ics, TZ).unwrap();
    assert_eq!(summary.imported, 2);
    assert_eq!(summary.skipped, 1, "the all-day offsite has no block form");
    assert!(summary.note.contains("all-day"), "{}", summary.note);

    let blocks = store.blocks_on("2025-07-30").unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].label.as_deref(), Some("Stand-up"));
    assert!(blocks[0].is_fixed, "an external meeting is not yours to move");
    assert_eq!(blocks[0].task_id, None);

    // Importing the same file again updates rather than duplicating.
    let again = store.import_ics_text(ics, TZ).unwrap();
    assert_eq!(again.imported, 0);
    assert_eq!(again.updated, 2);
    assert_eq!(store.blocks_on("2025-07-30").unwrap().len(), 1);
}

/// A repeating meeting repeats in Fruit, through the same engine as a
/// hand-made series.
#[test]
fn ics_recurring_events_expand() {
    let (mut store, _) = store_at(at(2025, 7, 28, 8, 0));
    let ics = "BEGIN:VEVENT\nUID:weekly@corp\nDTSTART;TZID=Europe/London:20250730T100000\n\
               DURATION:PT1H\nSUMMARY:Weekly sync\nRRULE:FREQ=WEEKLY;BYDAY=WE;COUNT=4\n\
               END:VEVENT";
    let summary = store.import_ics_text(ics, TZ).unwrap();
    assert_eq!(summary.imported, 1);
    assert_eq!(summary.recurring_instances, 3, "the seed plus three more");

    for date in ["2025-07-30", "2025-08-06", "2025-08-13", "2025-08-20"] {
        assert_eq!(
            store.blocks_on(date).unwrap().len(),
            1,
            "no instance on {date}"
        );
    }
}

/// §3.5 — the privacy contract is enforced where the write happens, not only
/// described in the UI.
#[test]
fn activity_respects_the_privacy_contract() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    // This test is about which fields may be written, not about how long an
    // observation has to last to be reported. The floor has its own tests.
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_MIN_SPAN_SEC, 0.into())
        .unwrap();
    let sample = |app: &str, title: &str, at: i64| ActivitySample {
        app_id: app.into(),
        window_title: Some(title.into()),
        domain: None,
        at,
    };

    // Off by default: nothing is recorded until it is switched on.
    assert!(!store.activity_settings().unwrap().enabled);
    assert!(!store
        .record_activity(sample("code.exe", "main.rs", at(2025, 7, 30, 9, 0)))
        .unwrap());

    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();

    // Titles are a separate switch, and stay off even when apps are on.
    assert!(store
        .record_activity(sample("code.exe", "secret-project.rs", at(2025, 7, 30, 9, 0)))
        .unwrap());
    let day = store.get_activity_day("2025-07-30", TZ).unwrap();
    assert_eq!(day.spans.len(), 1);
    assert_eq!(day.spans[0].window_title, None, "titles are opt-in separately");

    store
        .set_activity_setting("activity.titlesEnabled", true.into())
        .unwrap();
    store
        .record_activity(sample("mail.exe", "Inbox", at(2025, 7, 30, 9, 5)))
        .unwrap();
    let day = store.get_activity_day("2025-07-30", TZ).unwrap();
    assert_eq!(day.spans[1].window_title.as_deref(), Some("Inbox"));

    // An excluded app is never written at all — not hidden at read time.
    store
        .set_activity_setting("activity.excludedApps", serde_json::json!(["bank.exe"]))
        .unwrap();
    assert!(!store
        .record_activity(sample("bank.exe", "Statements", at(2025, 7, 30, 9, 10)))
        .unwrap());
    assert!(!store
        .get_activity_day("2025-07-30", TZ)
        .unwrap()
        .spans
        .iter()
        .any(|s| s.app_id == "bank.exe"));

    // A title pattern suppresses the title, keeping the app.
    store
        .set_activity_setting("activity.excludedTitlePatterns", serde_json::json!(["password"]))
        .unwrap();
    store
        .record_activity(sample("browser.exe", "My password vault", at(2025, 7, 30, 9, 15)))
        .unwrap();
    let day = store.get_activity_day("2025-07-30", TZ).unwrap();
    let browser = day.spans.iter().find(|s| s.app_id == "browser.exe").unwrap();
    assert_eq!(browser.window_title, None);

    // Pause is remembered, and stops recording.
    store.set_activity_setting("activity.paused", true.into()).unwrap();
    assert!(!store
        .record_activity(sample("code.exe", "main.rs", at(2025, 7, 30, 9, 20)))
        .unwrap());

    // Turning apps off turns titles off with it.
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, false.into())
        .unwrap();
    assert!(!store.activity_settings().unwrap().titles_enabled);
}

/// Consecutive samples of the same app coalesce, or a day would be tens of
/// thousands of one-second rows.
#[test]
fn activity_samples_coalesce_into_spans() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();

    for minute in 0..10 {
        store
            .record_activity(ActivitySample {
                app_id: "code.exe".into(),
                window_title: None,
                domain: None,
                at: at(2025, 7, 30, 9, 0) + minute * 60_000,
            })
            .unwrap();
    }
    // Five minutes of a second app, not one tick: a single 20-second sample is
    // below the short-span floor and is deliberately not reported.
    for tick in 0..15 {
        store
            .record_activity(ActivitySample {
                app_id: "mail.exe".into(),
                window_title: None,
                domain: None,
                at: at(2025, 7, 30, 9, 10) + tick * fruit_core::store::SAMPLE_INTERVAL_MS,
            })
            .unwrap();
    }

    let day = store.get_activity_day("2025-07-30", TZ).unwrap();
    assert_eq!(day.spans.len(), 2, "ten samples of one app are one span");
    assert_eq!(day.by_app[0].app_id, "code.exe");
    assert_eq!(
        day.by_app[0].seconds,
        9 * 60 + fruit_core::store::SAMPLE_INTERVAL_MS / 1000,
        "the span runs from the first sample to one interval past the last"
    );
}

/// §2.3 CALIBRATE — activity correlation: what was actually on screen during
/// the block you plotted. The one report that compares an intention against an
/// observation rather than against Fruit's own record.
#[test]
fn activity_correlates_with_the_block_underneath_it() {
    let (mut store, _) = store_at(at(2025, 7, 30, 8, 0));
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();
    let t = task(&mut store, "Refactor auth");
    let b = block(&mut store, &t.id, at(2025, 7, 30, 9, 0), 60);

    // Half the hour in the editor, half in Slack.
    for minute in 0..30 {
        store
            .record_activity(ActivitySample {
                app_id: "code.exe".into(),
                window_title: None,
                domain: None,
                at: at(2025, 7, 30, 9, 0) + minute * 60_000,
            })
            .unwrap();
    }
    for minute in 30..60 {
        store
            .record_activity(ActivitySample {
                app_id: "slack.exe".into(),
                window_title: None,
                domain: None,
                at: at(2025, 7, 30, 9, 0) + minute * 60_000,
            })
            .unwrap();
    }

    let day = store.get_activity_day("2025-07-30", TZ).unwrap();
    let correlation = day
        .correlations
        .iter()
        .find(|c| c.block_id == b.id)
        .expect("the block has activity under it");
    assert_eq!(correlation.title, "Refactor auth");
    assert_eq!(correlation.top_apps.len(), 2);
    // Both halves are the same length, so the ranking is a tie — and a tie has
    // to resolve the same way every run, not by `HashMap` seeding.
    assert_eq!(correlation.top_apps[0].app_id, "code.exe");
    assert_eq!(correlation.top_apps[1].app_id, "slack.exe");
    assert_eq!(
        correlation.top_apps[0].seconds, correlation.top_apps[1].seconds,
        "half an hour each, to the second"
    );
    assert!(correlation.top_apps[0].seconds > 25 * 60);
}

/// Retention is a promise with a mechanism behind it.
#[test]
fn activity_purges_at_the_retention_limit() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();
    store
        .set_activity_setting("activity.retentionDays", 30.into())
        .unwrap();
    store
        .record_activity(ActivitySample {
            app_id: "code.exe".into(),
            window_title: None,
            domain: None,
            at: at(2025, 7, 30, 9, 0),
        })
        .unwrap();

    assert_eq!(store.purge_activity().unwrap(), 0, "nothing is stale yet");
    clock.advance(40 * 24 * 3600 * 1000);
    assert_eq!(store.purge_activity().unwrap(), 1);
    assert!(store.activity_settings().unwrap().next_purge_at.is_some());

    // "Forever" means no purge at all.
    store
        .set_activity_setting("activity.retentionDays", 0.into())
        .unwrap();
    assert_eq!(store.activity_settings().unwrap().next_purge_at, None);
}

// ─── MVP acceptance (Plan Rev 3 §14) — the unified day ─────────────────

fn area(store: &Store, name: &str) -> LifeAreaRow {
    store
        .get_life_areas(TZ, false)
        .unwrap()
        .into_iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("no built-in area called {name}"))
}

fn life(store: &mut Store, area_name: &str, from: i64, to: i64) -> LifeEntryRow {
    let id = area(store, area_name).id;
    store
        .add_life_entry(NewLifeEntry {
            life_area_id: id,
            label: None,
            started_at: from,
            ended_at: to,
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap()
}

/// M1 + M4 — every second of the selected date is accounted for exactly once,
/// including the seconds nothing happened in.
///
/// This is the product's central promise in one assertion. The workbook could
/// not make it (its totals were maintained by hand); a single `time_entry`
/// table could not make it either, because an observation and a session
/// describing the same hour would be two rows and two durations.
#[test]
fn a_day_accounts_for_every_second_exactly_once() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 22, 0));
    let t = task(&mut store, "Refactor auth");

    // 00:00–07:00 asleep, 09:00–10:00 worked, and the rest of the day is empty.
    life(&mut store, "Sleep/Rest", at(2025, 7, 30, 0, 0), at(2025, 7, 30, 7, 0));
    store.start_timer(&t.id, None).unwrap();
    clock.shift_wall(-(13 * 3600 * 1000)); // pretend the run happened at 09:00
    let _ = clock;

    let day = store.get_day("2025-07-30", TZ, None).unwrap();
    let d = &day.totals;

    assert_eq!(d.day_sec, 24 * 3600);
    assert_eq!(
        d.confirmed_work_sec
            + d.confirmed_life_sec
            + d.private_sec
            + d.observed_only_sec
            + d.idle_sec
            + d.empty_sec,
        d.day_sec,
        "the layers must tile the day: {d:?}"
    );
    assert_eq!(d.confirmed_life_sec, 7 * 3600, "seven hours of sleep");

    // M1: every slot is present, empty ones included.
    assert_eq!(day.slots.len(), 48, "24 hours at 30-minute slots");
    assert!(
        day.slots.iter().any(|s| s.state == SlotState::Empty),
        "empty time is a real state, not an absent row"
    );
}

/// M2 + M8 — an observation overlapping a confirmed session **enriches** it.
/// It contributes evidence and no duration, so the day still sums to a day.
#[test]
fn observation_enriches_a_confirmed_session_without_adding_time() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();
    let t = task(&mut store, "Refactor auth");

    store.start_timer(&t.id, None).unwrap();
    // Twenty minutes of the hour were actually spent in Slack.
    for minute in 20..40 {
        store
            .record_activity(ActivitySample {
                app_id: "slack.exe".into(),
                window_title: None,
                domain: None,
                at: at(2025, 7, 30, 9, 0) + minute * 60_000,
            })
            .unwrap();
    }
    clock.advance(60 * 60_000);
    store.stop_timer().unwrap();

    let day = store.get_day("2025-07-30", TZ, None).unwrap();
    let d = &day.totals;

    assert_eq!(d.confirmed_work_sec, 3600, "the hour is work, all of it");
    assert_eq!(
        d.observed_only_sec, 0,
        "the observation is inside a confirmed session, so it owns nothing"
    );
    assert_eq!(
        d.confirmed_work_sec + d.confirmed_life_sec + d.private_sec
            + d.observed_only_sec + d.idle_sec + d.empty_sec,
        d.day_sec,
        "and the day still sums to a day"
    );

    // …but the evidence is there to be read.
    let worked = day
        .segments
        .iter()
        .find(|s| matches!(s.owner, SlotOwner::Work { .. }))
        .expect("the work segment");
    assert!(
        worked.evidence.iter().any(|a| a.app_id == "slack.exe"),
        "the segment carries what the machine saw"
    );
    assert!(d.pc_sec > 0, "and PC time is reported separately");
}

/// M5 — contribution modes are work-only, and converting a record to life time
/// clears one by construction: `life_entry` has nowhere to put it.
#[test]
fn contribution_is_work_only_and_clears_on_conversion() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    let t = task(&mut store, "Sprint review");
    store.start_timer(&t.id, None).unwrap();
    clock.advance(30 * 60_000);
    store.stop_timer().unwrap();
    // Stopping clears the runtime's session; the record is read back from the task.
    let session = store.get_task_detail(&t.id).unwrap().sessions.remove(0);

    let with_mode = store
        .set_session_contribution(&session.id, Some(Contribution::Attend))
        .unwrap();
    assert_eq!(with_mode.contribution, Some(Contribution::Attend));

    // Convert it to family time. There is no contribution field to carry over.
    let family = area(&store, "Family").id;
    let entry = store
        .convert_session_to_life(&session.id, &family, TZ)
        .unwrap();
    assert_eq!(entry.area_name, "Family");
    assert_eq!(entry.started_at, session.started_at);

    let day = store.get_day("2025-07-30", TZ, None).unwrap();
    assert_eq!(
        day.totals.confirmed_work_sec, 0,
        "the work session is gone, not duplicated"
    );
    assert_eq!(day.totals.confirmed_life_sec, 30 * 60);
}

/// M9 — a life entry can take an interval back from a confirmed record, but
/// only when the caller has said so: replacing is destructive.
#[test]
fn life_entries_fill_gaps_and_replace_with_confirmation() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 12, 0));
    let t = task(&mut store, "Deep work");
    store.start_timer(&t.id, None).unwrap();
    clock.advance(60 * 60_000);
    store.stop_timer().unwrap();

    // Without `replace_existing`, both records exist and precedence decides.
    let lunch = area(&store, "Wellbeing").id;
    store
        .add_life_entry(NewLifeEntry {
            life_area_id: lunch.clone(),
            label: Some("Lunch".into()),
            started_at: at(2025, 7, 30, 12, 0),
            ended_at: at(2025, 7, 30, 13, 0),
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();

    let day = store.get_day("2025-07-30", TZ, None).unwrap();
    assert_eq!(
        day.totals.confirmed_life_sec, 3600,
        "life wins the hour by precedence"
    );
    assert_eq!(
        day.totals.confirmed_work_sec, 0,
        "and the session is shadowed rather than counted twice"
    );
    // The session is still there — precedence hid it, it did not delete it.
    assert_eq!(store.get_task_detail(&t.id).unwrap().sessions.len(), 1);

    // With `replace_existing`, the session actually goes.
    store
        .add_life_entry(NewLifeEntry {
            life_area_id: lunch,
            label: Some("Lunch, properly".into()),
            started_at: at(2025, 7, 30, 12, 0),
            ended_at: at(2025, 7, 30, 13, 0),
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: true,
        })
        .unwrap();
    assert_eq!(
        store.get_task_detail(&t.id).unwrap().sessions.len(),
        0,
        "replace clears the interval it takes"
    );
}

/// Private time is accounted for without being recorded — it is not a gap the
/// reconciler should nag about, and it is not in any life-area total either.
#[test]
fn private_time_is_accounted_for_but_not_categorised() {
    let (mut store, _) = store_at(at(2025, 7, 30, 20, 0));
    let id = area(&store, "Personal/Spiritual").id;
    store
        .add_life_entry(NewLifeEntry {
            life_area_id: id,
            label: None,
            started_at: at(2025, 7, 30, 18, 0),
            ended_at: at(2025, 7, 30, 19, 0),
            tz: TZ.into(),
            is_private: true,
            note: None,
            replace_existing: false,
        })
        .unwrap();

    let day = store.get_day("2025-07-30", TZ, None).unwrap();
    assert_eq!(day.totals.private_sec, 3600);
    assert_eq!(day.totals.confirmed_life_sec, 0, "private is its own bucket");
    assert_eq!(day.totals.empty_sec, 23 * 3600, "and it is not a gap");
    assert!(day.slots.iter().any(|s| s.state == SlotState::Private));
}

/// A day the clocks change on is 23 or 25 hours, and the invariant is "the
/// day", never "86,400 seconds".
#[test]
fn the_counting_invariant_holds_across_a_dst_transition() {
    for (date, hours) in [("2025-03-30", 23), ("2025-10-26", 25)] {
        let (store, _) = store_at(at(2025, 6, 1, 12, 0));
        let day = store.get_day(date, TZ, None).unwrap();
        let d = &day.totals;
        assert_eq!(d.day_sec, hours * 3600, "{date} is {hours} hours long");
        assert_eq!(
            d.confirmed_work_sec + d.confirmed_life_sec + d.private_sec
                + d.observed_only_sec + d.idle_sec + d.empty_sec,
            d.day_sec,
            "{date} must still tile exactly"
        );
        assert_eq!(day.slots.len() as i64, hours * 2, "{date} slot count");
    }
}

/// The slot grid is a lens, never a quantisation: changing it must not change
/// a single second of the arithmetic (§8.1).
#[test]
fn slot_size_changes_the_view_not_the_totals() {
    let (mut store, _) = store_at(at(2025, 7, 30, 20, 0));
    // Ten minutes, deliberately not aligned to any slot boundary.
    life(
        &mut store,
        "Fun",
        at(2025, 7, 30, 9, 7),
        at(2025, 7, 30, 9, 17),
    );

    let mut totals = Vec::new();
    for minutes in [5, 15, 30, 60] {
        let day = store.get_day("2025-07-30", TZ, Some(minutes)).unwrap();
        assert_eq!(day.slots.len() as i64, 24 * 60 / minutes);
        totals.push((day.totals.confirmed_life_sec, day.totals.empty_sec));
    }
    assert!(
        totals.windows(2).all(|w| w[0] == w[1]),
        "totals must be identical at every zoom: {totals:?}"
    );
    assert_eq!(totals[0].0, 600, "ten minutes, at every zoom");
    assert_eq!(
        totals[0].1,
        24 * 3600 - 600,
        "and the entertainment hour is not rounded up to a slot"
    );
}

/// A life area holding records refuses to be deleted, and a built-in never
/// deletes at all — an imported workbook maps onto the built-in set.
#[test]
fn life_areas_refuse_to_orphan_their_records() {
    let (mut store, _) = store_at(at(2025, 7, 30, 9, 0));
    let sleep = area(&store, "Sleep/Rest");
    assert!(
        store.delete_life_area(&sleep.id).is_err(),
        "built-in areas archive, they do not delete"
    );

    let custom = store
        .create_life_area(NewLifeArea {
            name: "Sailing".into(),
            kind: Some("core".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(store.delete_life_area(&custom.id).is_ok(), "empty and custom: fine");

    let custom = store
        .create_life_area(NewLifeArea {
            name: "Sailing".into(),
            ..Default::default()
        })
        .unwrap();
    store
        .add_life_entry(NewLifeEntry {
            life_area_id: custom.id.clone(),
            label: None,
            started_at: at(2025, 7, 30, 9, 0),
            ended_at: at(2025, 7, 30, 10, 0),
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();
    assert!(
        store.delete_life_area(&custom.id).is_err(),
        "an area with records refuses rather than orphaning them"
    );
}

/// Every field crossing the IPC boundary is camelCase, including the ones
/// inside struct enum variants.
///
/// This exists because of a real break: `#[serde(rename_all = "camelCase")]`
/// renames an enum's *variants* and not its struct-variant **fields**, so
/// `SlotOwner` shipped `app_id` while every other DTO shipped `appId`. The
/// renderer read `undefined` and the primary screen rendered nothing — and
/// nothing in `cargo test` or `tsc` could see it, because both sides were
/// individually correct.
///
/// Walking the serialised JSON is the only place that mismatch is visible.
#[test]
fn wire_shape_is_camel_case() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 9, 0));
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();

    // One of every owner variant, so every struct variant is actually reached.
    life(&mut store, "Sleep/Rest", at(2025, 7, 30, 0, 0), at(2025, 7, 30, 6, 0));
    let t = task(&mut store, "Refactor auth");
    block(&mut store, &t.id, at(2025, 7, 30, 9, 0), 60);
    store.start_timer(&t.id, None).unwrap();
    clock.advance(30 * 60_000);
    store.stop_timer().unwrap();
    // Ten minutes, not one sample: observation below the short-span floor is
    // not reported, and a fixture built from a single 20-second tick would
    // silently stop producing an `observed` segment at all.
    for tick in 0..30 {
        store
            .record_activity(ActivitySample {
                app_id: "code.exe".into(),
                window_title: None,
                domain: None,
                at: at(2025, 7, 30, 14, 0) + tick * fruit_core::store::SAMPLE_INTERVAL_MS,
            })
            .unwrap();
    }

    let day = store.get_day("2025-07-30", TZ, None).unwrap();
    let json = serde_json::to_value(&day).unwrap();

    let mut offenders = Vec::new();
    walk_keys(&json, &mut |key| {
        if key.contains('_') {
            offenders.push(key.to_string());
        }
    });
    offenders.sort();
    offenders.dedup();
    assert!(
        offenders.is_empty(),
        "these fields cross the boundary as snake_case: {offenders:?}"
    );

    // …and the variants themselves are reached, so the walk above is not
    // passing merely because nothing interesting was in the payload.
    let kinds: Vec<&str> = day
        .segments
        .iter()
        .filter_map(|s| json_kind(&serde_json::to_value(&s.owner).unwrap()))
        .collect();
    for expected in ["life", "work", "observed", "empty"] {
        assert!(
            kinds.contains(&expected),
            "the fixture never produced a {expected} segment, so it proves nothing"
        );
    }
}

fn json_kind(v: &serde_json::Value) -> Option<&'static str> {
    match v.get("kind")?.as_str()? {
        "life" => Some("life"),
        "work" => Some("work"),
        "observed" => Some("observed"),
        "idle" => Some("idle"),
        "empty" => Some("empty"),
        _ => None,
    }
}

fn walk_keys(v: &serde_json::Value, f: &mut impl FnMut(&str)) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, child) in map {
                f(k);
                walk_keys(child, f);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|i| walk_keys(i, f)),
        _ => {}
    }
}

/// The month is the day's arithmetic, 31 times — so a figure on the dashboard
/// and the same figure on a day are the same number by construction (M12).
#[test]
fn a_month_is_the_sum_of_its_days_exactly() {
    let (mut store, clock) = store_at(at(2025, 7, 15, 12, 0));
    let t = task(&mut store, "Refactor auth");

    life(&mut store, "Sleep/Rest", at(2025, 7, 3, 0, 0), at(2025, 7, 3, 7, 0));
    life(&mut store, "Fun", at(2025, 7, 4, 20, 0), at(2025, 7, 4, 22, 0));
    store.start_timer(&t.id, None).unwrap();
    clock.advance(90 * 60_000);
    store.stop_timer().unwrap();

    let month = store.get_month("2025-07", TZ).unwrap();
    assert_eq!(month.days.len(), 31);
    assert_eq!(month.label, "July 2025");

    let m = &month.totals;
    assert_eq!(m.day_sec, 31 * 24 * 3600, "July has 31 whole days");
    assert_eq!(
        m.confirmed_work_sec + m.confirmed_life_sec + m.private_sec
            + m.observed_only_sec + m.idle_sec + m.empty_sec,
        m.day_sec,
        "the month tiles exactly, like each day does"
    );

    // Each per-day figure equals what `get_day` says for that date.
    for d in &month.days {
        let day = store.get_day(&d.local_date, TZ, None).unwrap();
        assert_eq!(d.empty_sec, day.totals.empty_sec, "{}", d.local_date);
        assert_eq!(
            d.confirmed_life_sec, day.totals.confirmed_life_sec,
            "{}",
            d.local_date
        );
    }

    // …and the totals equal the sum of the days.
    let summed: i64 = month.days.iter().map(|d| d.confirmed_life_sec).sum();
    assert_eq!(summed, m.confirmed_life_sec);
    assert_eq!(m.confirmed_life_sec, 9 * 3600, "seven of sleep, two of fun");
    assert_eq!(m.entertainment_sec, 2 * 3600);
    assert_eq!(m.confirmed_work_sec, 90 * 60);
}

/// A month containing a DST transition is not 30 × 24 hours, and the dashboard
/// must not quietly lose or invent the hour.
#[test]
fn a_dst_month_is_measured_in_real_hours() {
    let (store, _) = store_at(at(2025, 6, 1, 12, 0));

    let march = store.get_month("2025-03", TZ).unwrap();
    assert_eq!(
        march.totals.day_sec,
        31 * 24 * 3600 - 3600,
        "March loses an hour to the spring forward"
    );
    let october = store.get_month("2025-10", TZ).unwrap();
    assert_eq!(
        october.totals.day_sec,
        31 * 24 * 3600 + 3600,
        "October gains one back"
    );
    for m in [&march, &october] {
        let t = &m.totals;
        assert_eq!(
            t.confirmed_work_sec + t.confirmed_life_sec + t.private_sec
                + t.observed_only_sec + t.idle_sec + t.empty_sec,
            t.day_sec
        );
    }
}

/// §12 budget: a populated 31-day month renders in under 250ms.
///
/// The month deliberately runs `resolve_day` 31 times rather than writing a
/// second, cleverer query — the guard is that the honest implementation is fast
/// enough, not that it is the fastest possible.
#[test]
fn month_load_stays_inside_its_budget() {
    let (mut store, _) = store_at(at(2025, 7, 15, 12, 0));
    let t = task(&mut store, "Deep work");
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();

    // A full month: work, life and observation on every single day.
    for day in 1..=31 {
        life(&mut store, "Sleep/Rest", at(2025, 7, day, 0, 0), at(2025, 7, day, 7, 0));
        store
            .add_session(ManualSession {
                contribution: None,
                replace_existing: false,
                task_id: t.id.clone(),
                block_id: None,
                started_at: at(2025, 7, day, 9, 0),
                ended_at: at(2025, 7, day, 17, 0),
                note: None,
            })
            .unwrap();
        for minute in (0..120).step_by(5) {
            store
                .record_activity(ActivitySample {
                    app_id: "code.exe".into(),
                    window_title: None,
                    domain: None,
                    at: at(2025, 7, day, 19, 0) + minute * 60_000,
                })
                .unwrap();
        }
    }

    let start = std::time::Instant::now();
    let month = store.get_month("2025-07", TZ).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(month.days.len(), 31);
    assert!(
        elapsed.as_millis() < 250,
        "month load took {elapsed:?}, budget is 250ms"
    );
}

/// "Accounted" measures days that have happened, not the whole month.
///
/// A fresh August on the 4th is 6% of its month recorded — arithmetically true
/// and a useless headline, because the missing 27 days are the future rather
/// than a gap in the record. The same applies to the unreconciled count: nobody
/// was ever going to review tomorrow.
#[test]
fn a_month_in_progress_is_measured_against_the_days_that_happened() {
    // Noon on the 4th: three whole days behind, half of today, 27 ahead.
    let (mut store, _) = store_at(at(2025, 7, 4, 12, 0));
    for day in 1..=4 {
        life(&mut store, "Sleep/Rest", at(2025, 7, day, 0, 0), at(2025, 7, day, 6, 0));
    }

    let month = store.get_month("2025-07", TZ).unwrap();

    // 3 × 24h + 12h elapsed = 84h; 4 × 6h recorded = 24h.
    let elapsed = 84.0 * 3600.0;
    let expected = (elapsed - (elapsed - 24.0 * 3600.0)) / elapsed;
    assert!(
        (month.accounted_ratio - expected).abs() < 0.01,
        "accounted {:.3}, expected about {expected:.3}",
        month.accounted_ratio
    );
    assert!(
        month.accounted_ratio > 0.25,
        "measuring against all 31 days would have given about 0.03"
    );

    assert_eq!(
        month.unreconciled_days, 3,
        "the three finished days, not today and not the 27 to come"
    );

    // The whole-month totals are still whole-month — only the *ratio* is
    // elapsed-relative, because a total is a fact and a ratio is a judgement.
    assert_eq!(month.totals.day_sec, 31 * 24 * 3600);
}

// ─── M12 / M13: Excel export (Plan Rev 3 §10) ──────────────────────────

/// The preview and the file are the same matrix, and the workbook is a real
/// `.xlsx` — a ZIP whose first entries are the OPC parts Excel expects.
#[test]
fn the_month_exports_as_a_real_workbook() {
    let (mut store, clock) = store_at(at(2025, 7, 15, 12, 0));
    let t = task(&mut store, "Refactor auth");
    life(&mut store, "Sleep/Rest", at(2025, 7, 3, 0, 0), at(2025, 7, 3, 7, 0));
    life(&mut store, "Fun", at(2025, 7, 4, 20, 0), at(2025, 7, 4, 22, 0));
    store.start_timer(&t.id, None).unwrap();
    clock.advance(90 * 60_000);
    store.stop_timer().unwrap();

    let options = ExcelOptions {
        include_observed: true,
        include_unaccounted: true,
        include_private_labels: false,
    };
    let preview = store.preview_excel("2025-07", TZ, &options).unwrap();

    assert_eq!(preview.day_headers.len(), 31);
    assert_eq!(preview.slot_labels.len(), 48, "half-hour rows");
    assert_eq!(preview.slot_labels[0], "00:00");
    assert_eq!(preview.slot_labels[47], "23:30");
    assert_eq!(preview.rows.len(), 48);
    assert!(preview.rows.iter().all(|r| r.len() == 31), "a full matrix");
    assert!(preview.file_name.ends_with(".xlsx"));

    // The blank slots are present as cells, not missing (M1 carried into Excel).
    assert!(
        preview.rows.iter().flatten().any(|c| c.kind == "gap"),
        "unaccounted time has to survive the export"
    );
    assert!(preview.rows.iter().flatten().any(|c| c.kind == "rest"));
    assert!(preview.rows.iter().flatten().any(|c| c.kind == "entertainment"));
    assert!(preview.rows.iter().flatten().any(|c| c.kind == "work"));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("July 2025 Tracking.xlsx");
    let result = store.write_excel("2025-07", TZ, &path, &options).unwrap();
    assert_eq!(result.sheets.len(), 3, "month table, summary, source mapping");

    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.len() > 4_000, "a real workbook, not a stub");
    assert_eq!(&bytes[..2], b"PK", "xlsx is a ZIP container");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("[Content_Types].xml") && text.contains("xl/workbook.xml"),
        "the OPC parts Excel needs are present"
    );
}

/// The reconciliation table is the export's whole claim to being trustworthy:
/// the app's figure, the sheet's own figure, and the difference.
///
/// The variance is expected to be non-zero and bounded — the sheet is a
/// half-hour grid and the record is to the second, so a 90-minute session
/// occupies three slots and reads as 90 minutes while a 20-minute one reads as
/// 30. Success measure 7 is "no *unexplained* variance"; rounding is explained,
/// and this asserts it stays within one slot per day.
#[test]
fn the_export_reconciles_against_its_own_cells() {
    let (mut store, _) = store_at(at(2025, 7, 15, 12, 0));
    for day in 1..=10 {
        life(&mut store, "Sleep/Rest", at(2025, 7, day, 0, 0), at(2025, 7, day, 7, 0));
        life(&mut store, "Wellbeing", at(2025, 7, day, 12, 0), at(2025, 7, day, 13, 0));
    }

    let preview = store
        .preview_excel("2025-07", TZ, &ExcelOptions {
            include_observed: true,
            include_unaccounted: true,
            include_private_labels: false,
        })
        .unwrap();

    let accounted = preview
        .variances
        .iter()
        .find(|v| v.measure == "Accounted")
        .unwrap();
    assert_eq!(
        accounted.app_sec, 80 * 3600,
        "ten days of seven hours' sleep plus an hour each"
    );
    assert_eq!(
        accounted.variance_sec, 0,
        "whole-half-hour records round to nothing"
    );

    // Every measure is reported, whether or not it moved.
    assert_eq!(preview.variances.len(), 3);
    assert!(preview.variances.iter().any(|v| v.measure == "Unaccounted"));
    assert!(preview.variances.iter().any(|v| v.measure == "Observed only"));
}

/// Private time is exported by duration and never by name — the promise
/// Settings makes, carried through to the file someone might email.
#[test]
fn the_export_never_names_private_time_unless_asked() {
    let (mut store, _) = store_at(at(2025, 7, 15, 12, 0));
    let area = area(&store, "Personal/Spiritual").id;
    store
        .add_life_entry(NewLifeEntry {
            life_area_id: area,
            label: Some("Therapy".into()),
            started_at: at(2025, 7, 2, 18, 0),
            ended_at: at(2025, 7, 2, 19, 0),
            tz: TZ.into(),
            is_private: true,
            note: None,
            replace_existing: false,
        })
        .unwrap();

    let hidden = store
        .preview_excel("2025-07", TZ, &ExcelOptions {
            include_observed: true,
            include_unaccounted: true,
            include_private_labels: false,
        })
        .unwrap();
    let labels: Vec<&str> = hidden
        .rows
        .iter()
        .flatten()
        .map(|c| c.label.as_str())
        .collect();
    assert!(labels.contains(&"Private"), "the hour is in the sheet");
    assert!(
        !labels.contains(&"Therapy") && !labels.contains(&"Personal/Spiritual"),
        "…but nothing about what it was"
    );
    assert!(hidden.source_note.contains("not named"));

    let shown = store
        .preview_excel("2025-07", TZ, &ExcelOptions {
            include_observed: true,
            include_unaccounted: true,
            include_private_labels: true,
        })
        .unwrap();
    assert!(shown
        .rows
        .iter()
        .flatten()
        .any(|c| c.label == "Therapy"), "only when explicitly asked");
}

/// The browser connector, end to end: an observed domain becomes a reconcile
/// item that names the *site* rather than the browser, and a prospective rule
/// made from that decision classifies tomorrow **without touching yesterday**.
///
/// The backwards half is the one worth a test. `activity_span.category` is
/// stamped at write time exactly so that a rule cannot rewrite a month someone
/// has already signed off, and that is a property nothing in the UI can show.
#[test]
fn a_rule_made_while_reconciling_classifies_forwards_and_never_backwards() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 23, 0));
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_DOMAINS_ENABLED, true.into())
        .unwrap();

    // An hour on a site no rule covers, so the app has no opinion about it yet.
    for minute in 0..60 {
        store
            .record_activity(ActivitySample {
                app_id: "chrome.exe".into(),
                window_title: None,
                domain: Some("news.example.com".into()),
                at: at(2025, 7, 30, 11, 0) + minute * 60_000,
            })
            .unwrap();
    }

    let items = store.get_reconcile_items("2025-07-30", TZ).unwrap();
    let observed = items
        .iter()
        .find(|i| i.kind == ReconcileKind::ObservedOnly)
        .expect("the observed hour is on the list");
    // The site, not the browser. Naming it `chrome.exe` is the exact failure the
    // connector exists to fix.
    assert!(observed.title.contains("example.com"), "{}", observed.title);

    let evidence = observed.evidence.as_ref().unwrap();
    assert_eq!(evidence.source, "Browser connector");
    assert!(evidence.storage.contains("Domain only"));
    assert_eq!(
        evidence.domain.as_deref(),
        Some("example.com"),
        "a rule needs something durable to key on"
    );

    let area = store.get_life_areas(TZ, false).unwrap()[0].id.clone();
    store
        .apply_reconcile(
            "2025-07-30",
            vec![ReconcileAction {
                item_id: observed.id.clone(),
                verb: ReconcileVerb::RecordAsLife,
                starts_at: observed.starts_at,
                duration_sec: Some(3600),
                task_id: None,
                estimate_sec: None,
                life_area_id: Some(area),
                rule_for_domain: evidence.domain.clone(),
                rule_category_id: None,
            }],
            TZ,
        )
        .unwrap();

    // Forwards: tomorrow's visit is classified without being asked about again.
    assert_eq!(
        store
            .classify("chrome.exe", Some("news.example.com"))
            .unwrap()
            .map(|(_, counts)| counts),
        Some(DomainCategory::Entertainment)
    );
    clock.advance(24 * 3600 * 1000);
    for tick in 0..30 {
        store
            .record_activity(ActivitySample {
                app_id: "chrome.exe".into(),
                window_title: None,
                domain: Some("news.example.com".into()),
                at: at(2025, 7, 31, 11, 0) + tick * fruit_core::store::SAMPLE_INTERVAL_MS,
            })
            .unwrap();
    }
    let tomorrow = store.domain_totals("2025-07-31", TZ).unwrap();
    assert_eq!(tomorrow[0].category_name.as_deref(), Some("Distraction"));

    // Backwards: yesterday's *account* is exactly as it was left.
    //
    // This is the promise worth holding, and it is narrower than "nothing
    // changes". The hour was reconciled as life time by hand, and it still is:
    // the life entry owns it, the day still totals the same, and nothing the
    // user decided has moved. The observation underneath — which had no label at
    // all — has acquired one, because that is what a rule is for, and filling a
    // blank is not rewriting an answer.
    let yesterday = store.get_day("2025-07-30", TZ, None).unwrap();
    assert_eq!(
        yesterday.totals.confirmed_life_sec, 3600,
        "the hour someone recorded as life time stopped being life time"
    );
    assert_eq!(
        yesterday.totals.observed_only_sec, 0,
        "the reconciled hour is still owned by the life entry, not the observation"
    );

    // And a span that *did* carry a verdict keeps it. This is the half a later
    // rule must never touch — relabelling a domain in September cannot change
    // what August said you were doing.
    store
        .set_activity_rule(
            fruit_core::model::MatchKind::Domain,
            "news.example.com",
            fruit_core::model::category::STUDY,
        )
        .unwrap();
    let yesterday = store.domain_totals("2025-07-30", TZ).unwrap();
    assert_eq!(
        yesterday[0].category_name.as_deref(),
        Some("Distraction"),
        "the rule reached backwards and reclassified a day already closed"
    );
}

/// M10 — reconciliation covers **observed-only time and empty hours**, not just
/// the three block-shaped problems.
///
/// Those two are what make a day's account trustworthy rather than merely
/// tidy: a day with three reconciled blocks and nine unexplained hours is not
/// reconciled, and until now the sheet had no way to say so.
#[test]
fn reconcile_covers_observed_only_and_empty_hours() {
    let (mut store, clock) = store_at(at(2025, 7, 30, 23, 0));
    store
        .set_activity_setting(fruit_core::store::ACTIVITY_ENABLED, true.into())
        .unwrap();
    let t = task(&mut store, "Refactor auth");

    // An hour of observation nobody confirmed…
    for minute in 0..60 {
        store
            .record_activity(ActivitySample {
                app_id: "chrome.exe".into(),
                window_title: None,
                domain: None,
                at: at(2025, 7, 30, 11, 0) + minute * 60_000,
            })
            .unwrap();
    }
    // …with confirmed work either side, so the evidence panel has neighbours.
    store
        .add_session(ManualSession {
            contribution: None,
            replace_existing: false,
            task_id: t.id.clone(),
            block_id: None,
            started_at: at(2025, 7, 30, 10, 0),
            ended_at: at(2025, 7, 30, 11, 0),
            note: None,
        })
        .unwrap();
    life(&mut store, "Wellbeing", at(2025, 7, 30, 12, 0), at(2025, 7, 30, 13, 0));
    let _ = clock;

    let items = store.get_reconcile_items("2025-07-30", TZ).unwrap();

    let observed = items
        .iter()
        .find(|i| i.kind == ReconcileKind::ObservedOnly)
        .expect("the observed hour is on the list");
    assert!(observed.title.contains("chrome.exe"));
    assert!(observed.explanation.contains("No confirmed activity"));

    // The evidence panel: provenance, and the privacy promise restated.
    let evidence = observed.evidence.as_ref().expect("observed items carry evidence");
    assert_eq!(evidence.source, "Foreground window");
    assert!(evidence.confidence.contains("frontmost"));
    assert!(
        evidence.storage.contains("Application name only"),
        "the storage line is the promise, stated where it counts: {}",
        evidence.storage
    );
    assert_eq!(evidence.adjacent.len(), 2, "what sits either side");
    assert!(evidence.adjacent[0].contains("Refactor auth"));
    assert!(evidence.adjacent[1].contains("Wellbeing"));

    // And the empty hours are items too.
    let empties: Vec<_> = items
        .iter()
        .filter(|i| i.kind == ReconcileKind::Empty)
        .collect();
    assert!(!empties.is_empty(), "unaccounted hours are reconcilable");
    assert!(empties.iter().all(|i| i.starts_at.is_some() && i.ends_at.is_some()));

    // Every new item offers the verbs that can actually resolve it.
    for item in items
        .iter()
        .filter(|i| matches!(i.kind, ReconcileKind::ObservedOnly | ReconcileKind::Empty))
    {
        assert!(item.available.contains(&ReconcileVerb::RecordAsLife));
        assert!(item.available.contains(&ReconcileVerb::MarkPrivate));
        assert!(item.recommendation.is_some(), "the default explains itself");
    }
}

/// Resolving an observed hour writes a real record, and the day's arithmetic
/// moves — which is the whole point of reconciling it.
#[test]
fn reconciling_an_empty_hour_fills_it() {
    let (mut store, _) = store_at(at(2025, 7, 30, 23, 0));
    let before = store.get_day("2025-07-30", TZ, None).unwrap().totals.empty_sec;
    assert_eq!(before, 24 * 3600);

    let family = area(&store, "Family").id;
    store
        .apply_reconcile(
            "2025-07-30",
            vec![ReconcileAction {
                item_id: format!("empty:{}", at(2025, 7, 30, 18, 0)),
                verb: ReconcileVerb::RecordAsLife,
                starts_at: Some(at(2025, 7, 30, 18, 0)),
                duration_sec: Some(2 * 3600),
                task_id: None,
                estimate_sec: None,
                life_area_id: Some(family),
                rule_for_domain: None,
                rule_category_id: None,
            }],
            TZ,
        )
        .unwrap();

    let after = store.get_day("2025-07-30", TZ, None).unwrap().totals;
    assert_eq!(after.confirmed_life_sec, 2 * 3600);
    assert_eq!(after.empty_sec, 22 * 3600, "the hours moved, they did not vanish");
    assert_eq!(
        after.confirmed_work_sec + after.confirmed_life_sec + after.private_sec
            + after.observed_only_sec + after.idle_sec + after.empty_sec,
        after.day_sec,
        "and the day still tiles exactly"
    );

    // Private accounts for time without recording anything about it.
    store
        .apply_reconcile(
            "2025-07-30",
            vec![ReconcileAction {
                item_id: format!("empty:{}", at(2025, 7, 30, 21, 0)),
                verb: ReconcileVerb::MarkPrivate,
                starts_at: Some(at(2025, 7, 30, 21, 0)),
                duration_sec: Some(3600),
                task_id: None,
                estimate_sec: None,
                life_area_id: None,
                rule_for_domain: None,
                rule_category_id: None,
            }],
            TZ,
        )
        .unwrap();
    let after = store.get_day("2025-07-30", TZ, None).unwrap().totals;
    assert_eq!(after.private_sec, 3600);
    assert_eq!(after.confirmed_life_sec, 2 * 3600, "private is its own bucket");
}

// ─── found by running the binary ───────────────────────────────────────
//
// Everything below was found by launching the built application rather than by
// reading it. They are recorded here, in the acceptance suite, because each one
// is a promise the product makes on the very first screen a user sees — and
// because a bug that only appears on a real first run is exactly the kind the
// rest of this suite is blind to.

/// A timezone the app cannot parse must never reach the rest of the app.
///
/// The shipped fallback was `chrono::Local::now().offset().to_string()`, which
/// renders as `"+00:00"`. That is a *UTC offset*, not an IANA zone name, and
/// `zone()` rightly refuses it. On any machine without `TZ` set — which is
/// every ordinary Windows machine, since `TZ` is a Unix convention — the first
/// run therefore failed at its first line.
///
/// The floor is `"UTC"` rather than an error: an app running in the wrong zone
/// can be corrected in Settings, an app that will not start cannot.
#[test]
fn an_unparseable_timezone_never_escapes_tz_or() {
    let (store, _clock) = store_at(at(2026, 8, 3, 9, 0));

    // A UTC offset is not a zone name. This is the exact string that shipped.
    assert_eq!(store.tz_or("+00:00"), "UTC");
    assert_eq!(store.tz_or("+03:00"), "UTC");
    assert_eq!(store.tz_or(""), "UTC");
    assert_eq!(store.tz_or("Mars/Olympus"), "UTC");

    // A real zone name is passed through untouched.
    assert_eq!(store.tz_or("Europe/London"), "Europe/London");
    assert_eq!(store.tz_or("Africa/Addis_Ababa"), "Africa/Addis_Ababa");
}

/// A pinned setting is not trusted either — it is a string in a database that a
/// person or a bad import could have written.
#[test]
fn a_pinned_timezone_is_validated_like_any_other() {
    let (store, _clock) = store_at(at(2026, 8, 3, 9, 0));

    store
        .set_setting("general.tz", &serde_json::json!("Europe/Berlin"))
        .unwrap();
    assert_eq!(store.tz_or("Europe/London"), "Europe/Berlin", "pinned wins");

    store
        .set_setting("general.tz", &serde_json::json!("+02:00"))
        .unwrap();
    assert_eq!(
        store.tz_or("Europe/London"),
        "Europe/London",
        "a broken pin falls through to the caller's zone rather than failing"
    );

    store
        .set_setting("general.tz", &serde_json::json!("+02:00"))
        .unwrap();
    assert_eq!(
        store.tz_or("+01:00"),
        "UTC",
        "and with nothing usable anywhere, UTC"
    );
}

/// The first run must produce the demonstration project in every zone the app
/// can end up in — including the one it falls back to.
#[test]
fn the_first_run_seed_succeeds_in_every_zone_it_can_reach() {
    for tz in ["UTC", "Europe/London", "Africa/Addis_Ababa", "Pacific/Kiritimati"] {
        let (mut store, _clock) = store_at(at(2026, 8, 3, 9, 0));
        store
            .seed_first_run(tz)
            .unwrap_or_else(|e| panic!("seed failed in {tz}: {e:?}"));

        let backlog = store.get_backlog(BacklogFilter::default(), tz).unwrap();
        assert!(
            !backlog.tasks.is_empty(),
            "a first run in {tz} produced no tasks at all"
        );
    }
}

/// The seed runs at whatever time the app happens to be opened, including the
/// small hours, where "09:00 today" is still in the future.
#[test]
fn the_first_run_seed_succeeds_at_any_hour() {
    for hour in [0, 1, 6, 9, 13, 23] {
        let (mut store, _clock) = store_at(at(2026, 8, 3, hour, 30));
        store
            .seed_first_run("Europe/London")
            .unwrap_or_else(|e| panic!("seed failed at {hour}:30 — {e:?}"));
    }
}

/// The note the seed writes is the first prose a user reads. Since M4 removed
/// the Markdown renderer, anything left in Markdown syntax renders as its own
/// punctuation — hashes and asterisks on the welcome screen.
#[test]
fn the_welcome_note_contains_no_markup() {
    let (mut store, _clock) = store_at(at(2026, 8, 3, 9, 0));
    store.seed_first_run("Europe/London").unwrap();

    let notes: Vec<String> = store
        .get_backlog(BacklogFilter::default(), "Europe/London")
        .unwrap()
        .tasks
        .iter()
        .map(|t| store.get_task_detail(&t.id).unwrap().note)
        .filter(|n| !n.is_empty())
        .collect();

    assert!(!notes.is_empty(), "the seed writes at least one note");
    for note in notes {
        for markup in ["**", "# ", "`"] {
            assert!(
                !note.contains(markup),
                "seeded note still contains {markup:?}, which now renders literally:\n{note}"
            );
        }
    }
}

// ─── recording work that happened away from this PC (U1, U2) ───────────
//
// The observer sees one machine. Work on a second computer, an offline meeting
// and a task done on paper produce no `activity_span` at all, so the reconciler
// never offers them — they can *only* ever be entered by hand. That makes the
// manual path load-bearing rather than a fallback, and these are its rules.

/// A meeting entered by hand carries how you were involved, in one write.
///
/// Contribution used to be settable only *after* the fact, on a record that
/// already existed. But the case manual entry exists for is very often a
/// meeting, and "I attended two hours" versus "I did two hours" is the entire
/// distinction contribution was added to draw.
#[test]
fn a_hand_entered_session_can_carry_its_contribution() {
    let (mut store, _clock) = store_at(at(2026, 8, 3, 18, 0));
    let t = task(&mut store, "Design review");

    let session = store
        .add_session(ManualSession {
            task_id: t.id.clone(),
            block_id: None,
            started_at: at(2026, 8, 3, 14, 20),
            ended_at: at(2026, 8, 3, 16, 5),
            note: Some("Offline, in the meeting room".into()),
            contribution: Some(Contribution::Attend),
            replace_existing: false,
        })
        .unwrap();

    assert_eq!(session.contribution, Some(Contribution::Attend));
    assert_eq!(session.elapsed_sec, 105 * 60, "14:20 to 16:05 is 1h 45m");
    // And it lands on the day as confirmed work, like any other session.
    let totals = store.get_day("2026-08-03", TZ, None).unwrap().totals;
    assert_eq!(totals.confirmed_work_sec, 105 * 60);
}

/// Times are arbitrary, not multiples of anything. A meeting that ran 14:20 to
/// 16:05 is stored as 14:20 to 16:05.
#[test]
fn a_hand_entered_session_keeps_the_exact_minutes_given() {
    let (mut store, _clock) = store_at(at(2026, 8, 3, 18, 0));
    let t = task(&mut store, "Standup");

    let session = store
        .add_session(ManualSession {
            task_id: t.id.clone(),
            block_id: None,
            started_at: at(2026, 8, 3, 9, 7),
            ended_at: at(2026, 8, 3, 9, 23),
            note: None,
            contribution: None,
            replace_existing: false,
        })
        .unwrap();

    assert_eq!(session.started_at, at(2026, 8, 3, 9, 7));
    assert_eq!(session.ended_at, Some(at(2026, 8, 3, 9, 23)));
    assert_eq!(session.elapsed_sec, 16 * 60);
}

/// Recording by hand is usually filling a gap, but it is sometimes correcting
/// an hour the app got wrong — and the second case needs the old record gone,
/// or the day holds both. The same flag `NewLifeEntry` has, defaulting the same
/// way: off, because replacing destroys a confirmed record.
#[test]
fn a_hand_entered_session_can_replace_what_was_already_there() {
    let (mut store, _clock) = store_at(at(2026, 8, 3, 18, 0));
    let wrong = task(&mut store, "Mis-tracked");
    let right = task(&mut store, "What actually happened");

    store
        .add_session(ManualSession {
            task_id: wrong.id.clone(),
            block_id: None,
            started_at: at(2026, 8, 3, 10, 0),
            ended_at: at(2026, 8, 3, 11, 0),
            note: None,
            contribution: None,
            replace_existing: false,
        })
        .unwrap();

    // Without the flag, both records exist and the hour is claimed twice.
    store
        .add_session(ManualSession {
            task_id: right.id.clone(),
            block_id: None,
            started_at: at(2026, 8, 3, 10, 0),
            ended_at: at(2026, 8, 3, 11, 0),
            note: None,
            contribution: None,
            replace_existing: false,
        })
        .unwrap();
    assert_eq!(
        store.get_task_detail(&wrong.id).unwrap().sessions.len()
            + store.get_task_detail(&right.id).unwrap().sessions.len(),
        2
    );

    // With it, the interval is cleared first and one record remains.
    store
        .add_session(ManualSession {
            task_id: right.id.clone(),
            block_id: None,
            started_at: at(2026, 8, 3, 10, 0),
            ended_at: at(2026, 8, 3, 11, 0),
            note: None,
            contribution: None,
            replace_existing: true,
        })
        .unwrap();
    assert!(
        store.get_task_detail(&wrong.id).unwrap().sessions.is_empty(),
        "the mis-tracked hour was cleared"
    );
    assert_eq!(
        store.get_task_detail(&right.id).unwrap().sessions.len(),
        1,
        "and exactly one record remains"
    );

    let totals = store.get_day("2026-08-03", TZ, None).unwrap().totals;
    assert_eq!(totals.confirmed_work_sec, 3600, "one hour, claimed once");
}

/// Replacing clears the interval *before* the insert. Doing it after would
/// delete the row just written, because that row is itself inside the interval.
#[test]
fn replacing_does_not_delete_the_record_it_just_wrote() {
    let (mut store, _clock) = store_at(at(2026, 8, 3, 18, 0));
    let t = task(&mut store, "Offline work");

    let session = store
        .add_session(ManualSession {
            task_id: t.id.clone(),
            block_id: None,
            started_at: at(2026, 8, 3, 13, 0),
            ended_at: at(2026, 8, 3, 14, 0),
            note: None,
            contribution: None,
            replace_existing: true,
        })
        .unwrap();

    let survivors = store.get_task_detail(&t.id).unwrap().sessions;
    assert_eq!(survivors.len(), 1, "the new session survived its own replace");
    assert_eq!(survivors[0].id, session.id);
}

/// A zero-length session is invisible on the Day view, counted nowhere, and
/// impossible to select in order to delete. Nothing wants one.
#[test]
fn a_session_that_ends_when_it_starts_is_refused() {
    let (mut store, _clock) = store_at(at(2026, 8, 3, 18, 0));
    let t = task(&mut store, "Nothing");
    let same = at(2026, 8, 3, 10, 0);

    assert!(store
        .add_session(ManualSession {
            task_id: t.id.clone(),
            block_id: None,
            started_at: same,
            ended_at: same,
            note: None,
            contribution: None,
            replace_existing: false,
        })
        .is_err());
}
