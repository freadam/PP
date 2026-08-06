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

use chrono::{Datelike, Duration};
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
            rrule: None,
            intent: None,
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
        rrule: None,
        intent: None,
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

    // A repeating block, so the preview shows the ↻ marker and the series that
    // makes ⌫ ask which occurrences it means (§4.3, P2).
    store.schedule_recurring(
        NewBlock {
            task_id: None,
            label: Some("Daily stand-up".into()),
            starts_at: day_start_ms(0, 9),
            duration_sec: 900,
            tz: tz.clone(),
            is_fixed: true,
            rrule: None,
            intent: None,
        },
        "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
    )?;

    // Activity (§3.5, P2). Enabled here only so the preview has something to
    // draw; it is off in a real first run, and the fixture's own settings
    // object below says `enabled: true` because that is genuinely what this
    // recorded database had.
    store.set_activity_setting(
        fruit_core::store::ACTIVITY_ENABLED,
        Value::Bool(true),
    )?;
    store.set_activity_setting("activity.titlesEnabled", Value::Bool(true))?;
    store.set_activity_setting("activity.domainsEnabled", Value::Bool(true))?;
    // On *today*, because that is the day Activity opens on, and because the
    // first-run seed puts a plotted block there for the correlation panel to
    // compare against.
    let activity_base = fruit_core::time::day_start(today_date, &zone_) + 9 * 3_600_000;
    // Minutes after 09:00, how long, and what was in front. The 09:00 block on
    // today is the first-run seed's "Refactor auth module" — so the preview's
    // correlation panel has a real intention to hold the observation against,
    // and it is not a flattering comparison, which is the point.
    for (from_min, minutes, app_id, title) in [
        (0i64, 22i64, "code.exe", "auth.rs — fruit"),
        (22, 18, "slack.exe", "#eng-planning"),
        // Both observers run at once in life, and the window title is the only
        // thing that can say *which* video: the connector sends a domain and
        // deliberately never a page title. These two overlap the YouTube
        // stretches below so the preview shows a stretch you can tell apart.
        (35, 22, "chrome.exe", "Rust lifetimes explained — YouTube"),
        (40, 25, "code.exe", "auth.rs — fruit"),
        (65, 10, "chrome.exe", "OAuth 2.1 draft"),
        (120, 55, "code.exe", "activity.rs — fruit"),
        (185, 30, "slack.exe", "#design-review"),
        (240, 70, "code.exe", "Activity.tsx — fruit"),
        (300, 35, "chrome.exe", "Lo-fi beats to relax to — YouTube"),
        (330, 25, "chrome.exe", "RFC 5545 — RRULE"),
    ] {
        // One sample every 20 seconds, exactly as the shell's loop does: each
        // covers a nominal interval, and `record_activity` coalesces a run of
        // them into one span. Recording it any other way would be a fixture of
        // something the app never produces.
        let start = activity_base + from_min * 60_000;
        for step in 0..(minutes * 3) {
            store.record_activity(ActivitySample {
                app_id: app_id.into(),
                window_title: Some(title.into()),
                domain: None,
                at: start + step * 20_000,
            })?;
        }
    }

    // Browser observations, so the preview's "Work + distraction" row lights up
    // for the reason the real app would light it up rather than because the
    // fixture asserted it. The classification is genuinely computed — these
    // rows carry `category: "entertainment"` because the seeded YouTube rule
    // said so at write time.
    //
    // Recorded through `record_activity` with a domain, the same way the app
    // rows above are, rather than through `record_browser_sample`. That entry
    // point deliberately refuses a sample older than six hours — a queued
    // observation from this morning is not evidence of anything — and a fixture
    // laying out a whole day is exactly the case it refuses. Going through the
    // shared write path keeps the classification and the exclusions honest
    // while letting the fixture choose its own clock.
    //
    // The two stretches of YouTube are placed to produce the two *different*
    // states the Day view distinguishes, because a fixture that only produces
    // one of them leaves the other rendering path unexercised:
    //
    //   09:35 falls inside the confirmed morning session → **Work + distraction**.
    //     The work keeps the whole interval; the entertainment is evidence, not
    //     a second duration (M8). This is the finding the product exists for.
    //   14:00 falls in an unclaimed stretch → **Observed entertainment**, which
    //     is an item the reconciler will ask about rather than a fact.
    for (from_min, minutes, domain) in [
        (65i64, 10i64, "developer.mozilla.org"),
        (35, 22, "www.youtube.com"),
        (200, 12, "github.com"),
        (300, 35, "youtube.com"),
    ] {
        let start = activity_base + from_min * 60_000;
        for step in 0..(minutes * 3) {
            store.record_activity(ActivitySample {
                app_id: "chrome.exe".into(),
                window_title: None,
                domain: Some(domain.into()),
                at: start + step * 20_000,
            })?;
        }
    }

    // Confirmed work on *today*, deliberately.
    //
    // Every other session here is seeded relative to Monday, so unless the
    // preview happened to be recorded on a Monday the Day view opened with no
    // confirmed work at all — and "Work + distraction", the finding this whole
    // product exists to surface, had nothing to fire against. It covers the
    // 09:35 YouTube stretch below, so the row is demonstrated rather than
    // asserted.
    store.add_session(ManualSession {
        task_id: firefight.id.clone(),
        block_id: None,
        started_at: activity_base,
        ended_at: activity_base + 75 * 60_000,
        note: None,
    })?;

    // Confirmed non-work time, so the Day view's primary screen has all four
    // record types on it: a night's sleep that starts the previous evening, a
    // lunch, and an evening the user marked private.
    let sleep = store
        .get_life_areas(&tz, false)?
        .into_iter()
        .find(|a| a.name == "Sleep/Rest")
        .expect("seeded");
    let wellbeing = store
        .get_life_areas(&tz, false)?
        .into_iter()
        .find(|a| a.name == "Wellbeing")
        .expect("seeded");
    for (name, hours) in [("Wellbeing", 20i64), ("Family", 40), ("Personal Development", 15)] {
        if let Some(a) = store.get_life_areas(&tz, false)?.into_iter().find(|a| a.name == name) {
            store.update_life_area(
                &a.id,
                LifeAreaPatch {
                    monthly_target_sec: Some(Some(hours * 3600)),
                    ..Default::default()
                },
            )?;
        }
    }

    let today_start = fruit_core::time::day_start(today_date, &zone_);
    for (area_id, label, from_h, to_h, private) in [
        (sleep.id.clone(), Some("Sleep"), -1.0f64, 6.5, false),
        (wellbeing.id.clone(), Some("Lunch and a walk"), 12.5, 13.5, false),
        (wellbeing.id.clone(), None, 19.0, 20.0, true),
    ] {
        store.add_life_entry(NewLifeEntry {
            life_area_id: area_id,
            label: label.map(str::to_string),
            started_at: today_start + (from_h * 3_600_000.0) as i64,
            ended_at: today_start + (to_h * 3_600_000.0) as i64,
            tz: tz.clone(),
            is_private: private,
            note: None,
            replace_existing: false,
        })?;
    }

    // A month's worth of sleep and a scatter of life entries, so the month
    // dashboard has a real shape rather than one populated week against thirty
    // empty days. Sleep every night; the rest on a rotation.
    {
        let month_first = today_date.with_day(1).unwrap();
        let areas = store.get_life_areas(&tz, false)?;
        let pick = |name: &str| areas.iter().find(|a| a.name == name).map(|a| a.id.clone()).unwrap();
        let rota = [
            ("Wellbeing", 18.0f64, 1.0f64),
            ("Family", 19.0, 2.0),
            ("Fun", 20.5, 1.5),
            ("Personal Development", 7.5, 1.0),
            ("Friendship", 19.5, 2.0),
        ];
        let mut d = month_first;
        let mut i = 0usize;
        while d.month() == month_first.month() && d <= today_date {
            let midnight = fruit_core::time::day_start(d, &zone_);
            store.add_life_entry(NewLifeEntry {
                life_area_id: pick("Sleep/Rest"),
                label: None,
                started_at: midnight,
                ended_at: midnight + (6.5 * 3_600_000.0) as i64,
                tz: tz.clone(),
                is_private: false,
                note: None,
                replace_existing: false,
            })?;
            if i % 2 == 0 {
                let (name, from_h, len_h) = rota[(i / 2) % rota.len()];
                // An evening plotted for a film — the second half of M11, and
                // the reason the dashboard's dashed line is no longer flat.
                //
                // Deliberately only *some* of the entertainment evenings get a
                // window, and the window is longer than the evening that fills
                // it. That produces all three readings the chart has to be able
                // to draw: an evening you planned and took, an evening you
                // planned and did not use, and an evening that just happened.
                if name == "Fun" && i % 4 == 0 {
                    store.schedule_block(NewBlock {
                        task_id: None,
                        label: Some("Film".into()),
                        starts_at: midnight + (20.0 * 3_600_000.0) as i64,
                        duration_sec: 2 * 3600,
                        tz: tz.clone(),
                        is_fixed: false,
                        rrule: None,
                        intent: Some(BlockIntent::Entertainment),
                    })?;
                }
                store.add_life_entry(NewLifeEntry {
                    life_area_id: pick(name),
                    label: None,
                    started_at: midnight + (from_h * 3_600_000.0) as i64,
                    ended_at: midnight + ((from_h + len_h) * 3_600_000.0) as i64,
                    tz: tz.clone(),
                    is_private: false,
                    note: None,
                    replace_existing: false,
                })?;
            }
            let Some(next) = d.succ_opt() else { break };
            d = next;
            i += 1;
        }
    }

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
    out.insert("get_day".into(), json!(store.get_day(&today, &tz, None)?));
    out.insert("get_month".into(), json!(store.get_month(&today[..7], &tz)?));
    out.insert("get_life_areas".into(), json!(store.get_life_areas(&tz, false)?));
    out.insert(
        "preview_excel".into(),
        json!(store.preview_excel(
            &today[..7],
            &tz,
            &ExcelOptions {
                include_observed: true,
                include_unaccounted: true,
                include_private_labels: false,
            }
        )?),
    );
    out.insert("suggest_excel_path".into(), json!("~/Downloads/Tracking.xlsx"));
    out.insert(
        "get_life_entries".into(),
        json!(store.life_entries_on(&today, &tz)?),
    );
    out.insert(
        "get_activity_day".into(),
        json!(store.get_activity_day(&today, &tz)?),
    );
    // `support` and `supportNote` are the shell's answer, not the core's — the
    // browser preview has no platform hook at all, so they are stated here
    // rather than recorded. Everything under `settings` is the real thing.
    out.insert(
        "get_activity_settings".into(),
        json!({
            "support": "full",
            "supportNote": "Browser preview. In the desktop app this sentence comes from the platform itself.",
            "settings": store.activity_settings()?,
        }),
    );
    // Two goals, one in each direction, because the *pair* is the point: a
    // ceiling you succeed at by staying under it has to read differently from a
    // target you fill, and a preview showing only one of them proves nothing.
    for (metric, direction, hours, weekdays) in [
        (fruit_core::model::metric::ALL_WORK, GoalDirection::AtMost, 30, true),
        (fruit_core::model::metric::ENTERTAINMENT, GoalDirection::AtMost, 7, false),
        (fruit_core::model::metric::SLEEP, GoalDirection::AtLeast, 52, false),
    ] {
        store.set_goal(
            NewGoal {
                subject_kind: Some(GoalSubject::Metric),
                subject_id: metric.into(),
                direction: Some(direction),
                target_sec: hours * 3600,
                applies_days: weekdays.then_some(0b0011111),
            },
            &today,
        )?;
    }
    out.insert(
        "get_week_review".into(),
        json!(store.get_week_review(&today, &tz)?),
    );
    out.insert("get_goals".into(), json!(store.get_goals(false)?));
    // W9's report, and the card that offers it. The card fixture is recorded
    // for *last* week deliberately: `due_week_report` is only ever about a week
    // that has finished, and a fixture of this week would show the preview a
    // card the real app would never draw.
    out.insert(
        "get_week_report".into(),
        json!(store.week_report(&today, &tz)?),
    );
    out.insert(
        "due_week_report".into(),
        json!(store.due_week_report(&tz)?),
    );
    out.insert(
        "get_goal_templates".into(),
        json!(store.get_goal_templates(&today, &tz)?),
    );
    out.insert("get_activity_rules".into(), json!(store.get_activity_rules()?));
    out.insert(
        "get_categories".into(),
        json!(store.get_categories(Some((&today, &today)), &tz)?),
    );
    out.insert(
        "get_unlabelled".into(),
        json!(store.get_unlabelled(&today, &today, &tz, 12)?),
    );
    out.insert(
        "get_domain_totals".into(),
        json!(store.domain_totals(&today, &tz)?),
    );
    // Paths are the shell's answer, like `support` above. The preview cannot
    // know where the executable lives, and inventing a plausible one would be
    // the kind of fixture that reads as real and is not.
    out.insert(
        "get_connector_status".into(),
        json!({
            "enabled": true,
            "hostManifestName": "app.fruit.connector",
            "extensionDir": "(the desktop app prints the real path here)",
            "exePath": "(the desktop app prints the real path here)",
            "queuedSamples": 0,
        }),
    );
    out.insert("get_rrule_presets".into(), json!(fruit_core::rrule::presets()));
    out.insert("extend_series_to".into(), json!(0));
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
