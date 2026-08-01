//! Records a set of real DTOs for browser preview (`npm run dev`).
//!
//! The point is that the preview shows output from the *actual* command layer
//! rather than from a hand-written JavaScript mock. A mock would be a second
//! implementation of the rules, which is precisely the divergence this
//! architecture exists to prevent — in an app about divergence, no less.
//!
//!     cargo run -p fruit-core --bin dump-fixtures
//!
//! Writes `src/dev/fixtures.json` relative to the repository root.

use std::path::PathBuf;

use chrono::Duration;
use fruit_core::model::*;
use fruit_core::time::{format_date, local_date, now_ms, parse_date, week_start, zone};
use fruit_core::Store;
use serde_json::{json, Map, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tz = std::env::var("TZ").unwrap_or_else(|_| "Europe/London".into());
    let zone_ = zone(&tz)?;
    let mut store = Store::open_in_memory()?;
    store.seed_first_run(&tz)?;

    let today = local_date(now_ms(), &zone_);
    let today_date = parse_date(&today)?;
    let monday = week_start(today_date);
    let sunday = monday + Duration::days(6);

    // A second project with a weekly target, so Reports has something to draw.
    let deep = store.create_project(NewProject {
        name: "Deep work".into(),
        colour: Some("#7E8CF0".into()),
        kind: Some("work".into()),
        weekly_target_sec: Some(10 * 3600),
    })?;

    let titles = [
        ("Refactor the scheduler", 5400i64, 2i64),
        ("Write the migration guide", 3600, 1),
        ("Review the drift rail spec", 1800, 3),
        ("Fix the DST off-by-one", 900, 3),
        ("Answer support mail", 1800, 0),
    ];
    let day_start_ms = |offset_days: i64, hour: i64| {
        let date = monday + Duration::days(offset_days);
        fruit_core::time::day_start(date, &zone_) + hour * 3_600_000
    };

    for (i, (title, estimate, priority)) in titles.iter().enumerate() {
        let task = store.create_task(NewTask {
            title: (*title).into(),
            project_id: Some(deep.id.clone()),
            estimate_sec: Some(*estimate),
            priority: Some(*priority),
            due_date: (i % 2 == 0).then(|| today.clone()),
            tags: vec![if i % 2 == 0 { "dev".into() } else { "writing".into() }],
            ..Default::default()
        })?;

        let offset = (i as i64) % 5;
        let block = store.schedule_block(NewBlock {
            task_id: Some(task.id.clone()),
            label: None,
            starts_at: day_start_ms(offset, 9 + (i as i64 % 3) * 2),
            duration_sec: *estimate,
            tz: tz.clone(),
            is_fixed: i == 4,
        })?;

        // Every drift state from §5.6 gets a representative on screen.
        let tracked = match i {
            0 => estimate + 14 * 60, // overrun
            1 => *estimate,          // on estimate
            2 => estimate / 2,       // in progress / underrun
            3 => 0,                  // never started
            _ => estimate - 22 * 60, // underrun
        };
        if tracked > 0 {
            store.add_session(ManualSession {
                task_id: task.id.clone(),
                block_id: Some(block.id.clone()),
                started_at: block.starts_at,
                ended_at: block.starts_at + tracked * 1000,
                note: None,
            })?;
            store.set_task_status(&task.id, Status::Done)?;
        }
    }

    // One rollover task and a couple of finished ones, so the preview shows
    // the top of the estimate ladder and the greyed Completed tail.
    store.create_task(NewTask {
        title: "Rewrite the sync layer".into(),
        project_id: Some(deep.id.clone()),
        is_rollover: true,
        priority: Some(2),
        tags: vec!["dev".into()],
        ..Default::default()
    })?;
    for title in ["Fix the flaky test", "Reply to the RFC thread"] {
        let t = store.create_task(NewTask {
            title: title.into(),
            project_id: Some(deep.id.clone()),
            estimate_sec: Some(1800),
            ..Default::default()
        })?;
        store.add_session(ManualSession {
            task_id: t.id.clone(),
            block_id: None,
            started_at: day_start_ms(0, 13),
            ended_at: day_start_ms(0, 13) + 25 * 60_000,
            note: None,
        })?;
        store.set_task_status(&t.id, Status::Done)?;
    }

    // A meeting nobody plans for, and one unplanned session.
    store.schedule_block(NewBlock {
        task_id: None,
        label: Some("Standup".into()),
        starts_at: day_start_ms(0, 10),
        duration_sec: 900,
        tz: tz.clone(),
        is_fixed: true,
    })?;
    let firefight = store.create_task(NewTask {
        title: "Production firefight".into(),
        project_id: Some(deep.id.clone()),
        ..Default::default()
    })?;
    store.add_session(ManualSession {
        task_id: firefight.id.clone(),
        block_id: None,
        started_at: day_start_ms(0, 15),
        ended_at: day_start_ms(0, 15) + 40 * 60_000,
        note: None,
    })?;

    let range = DateRange {
        from: format_date(monday),
        to: format_date(sunday),
    };

    let mut out = Map::new();
    out.insert("get_week".into(), json!(store.get_week(&range, &tz)?));
    out.insert(
        "get_backlog".into(),
        json!(store.get_backlog(BacklogFilter::default(), &tz)?),
    );
    out.insert(
        "get_tasks".into(),
        json!(store.get_tasks(TaskQuery {
            limit: Some(200),
            ..Default::default()
        })?),
    );
    out.insert("get_projects".into(), json!(store.get_projects(&tz)?));
    out.insert("get_tags".into(), json!(store.get_tags()?));
    out.insert(
        "get_task_detail".into(),
        json!(store.get_task_detail(&firefight.id)?),
    );
    out.insert(
        "get_reports".into(),
        json!(store.get_reports(
            &DateRange {
                from: format_date(monday - Duration::days(28)),
                to: range.to.clone(),
            },
            &ReportFilter {
                tz: Some(tz.clone()),
                project_id: None,
            }
        )?),
    );
    out.insert(
        "get_reconcile_items".into(),
        json!(store.get_reconcile_items(&today, &tz)?),
    );
    out.insert(
        "get_unreconciled_days".into(),
        json!(store.unreconciled_days(&today, 10)?),
    );
    out.insert("get_timer_state".into(), json!(store.timer_state()?));
    out.insert("get_settings".into(), Value::Object(store.all_settings()?));
    out.insert("get_deleted".into(), json!(store.deleted_rows()?));
    out.insert("search".into(), json!(store.search("draft", 20)?));
    out.insert(
        "parse_capture".into(),
        json!(fruit_core::parser::parse(
            "Fix login bug #dev ~45m !! ^tomorrow 9am",
            &fruit_core::parser::ParseCtx {
                now: now_ms(),
                tz: zone_,
                order: fruit_core::parser::DateOrder::DayMonth,
                known_projects: &[],
            }
        )),
    );

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/dev/fixtures.json")
        .canonicalize()
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/dev/fixtures.json")
        });
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&Value::Object(out))?)?;
    println!("wrote {}", path.display());
    Ok(())
}
