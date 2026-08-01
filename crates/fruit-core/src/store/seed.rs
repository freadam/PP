//! First run (§3.9).
//!
//! The first 90 seconds decide whether a trial converts, so the seed is not a
//! tour — it is one project that *demonstrates* the product. The headline
//! decision here is the pre-seeded overrun: the signature drift rail is on
//! screen before the user has tracked a single minute.

use rusqlite::params;

use super::Store;
use crate::error::Result;
use crate::ids::new_id;
use crate::model::*;
use crate::parser::{parse, DateOrder, ParseCtx};
use crate::time::{day_start, local_date, parse_date, zone};

pub const SEED_FLAG: &str = "app.seeded";

impl Store {
    pub fn is_seeded(&self) -> bool {
        matches!(
            self.get_setting(SEED_FLAG),
            Ok(Some(serde_json::Value::Bool(true)))
        )
    }

    pub fn seed_first_run(&mut self, tz: &str) -> Result<()> {
        if self.is_seeded() {
            return Ok(());
        }
        let zone_ = zone(tz)?;
        let now = self.now();
        let today = local_date(now, &zone_);
        let midnight = day_start(parse_date(&today)?, &zone_);

        let project = self.create_project(NewProject {
            name: "Welcome to Fruit".into(),
            colour: Some("#56C2D6".into()),
            kind: Some("personal".into()),
            weekly_target_sec: Some(2 * 3600),
        })?;

        // 1. The title *is* the parser documentation — so it is genuinely
        //    parsed rather than hand-built.
        let known = vec![(project.name.clone(), project.id.clone())];
        let ctx = ParseCtx {
            now,
            tz: zone_,
            order: DateOrder::DayMonth,
            known_projects: &known,
        };
        let guide = parse(
            "Read the 60-second guide @welcome ~15m !! ^today 17:00",
            &ctx,
        );
        let guide_task = self.create_task(NewTask {
            title: guide.title.clone(),
            project_id: Some(project.id.clone()),
            estimate_sec: guide.estimate_sec,
            due_at: guide.due_at,
            due_date: guide.due_date.clone(),
            priority: Some(guide.priority),
            tags: guide.tags.clone(),
            ..Default::default()
        })?;
        self.save_note(
            &guide_task.id,
            "# Fruit in 60 seconds\n\n\
             **Plot** what you mean to do — drag a task onto the planner, or press `S`.\n\n\
             **Track** what you actually do — `Space` starts and stops the timer.\n\n\
             **Reconcile** the difference — `Cmd+R` opens the day review. \
             Every block carries two traces on its left edge: a dashed cyan one for \
             the plot, a solid amber one for the track. The gap between them is the point.\n\n\
             **Capture grammar:** `#project` `@tag` `~45m` `!!` `^tomorrow 9am`\n",
        )?;

        // 2. A block earlier today with a pre-seeded overrun, and a fabricated
        //    74-minute session under a 60-minute plot.
        let demo_task = self.create_task(NewTask {
            title: "Refactor auth module".into(),
            project_id: Some(project.id.clone()),
            estimate_sec: Some(3600),
            tags: vec!["welcome".into()],
            ..Default::default()
        })?;

        // Prefer 09:00 local; fall back to something safely inside today when
        // the app is first opened in the small hours.
        let nine = midnight + 9 * 3_600_000;
        let block_start = if nine + 3_600_000 <= now {
            nine
        } else {
            (now - 90 * 60_000).max(midnight)
        };
        let block = self.schedule_block(NewBlock {
            task_id: Some(demo_task.id.clone()),
            label: None,
            starts_at: block_start,
            duration_sec: 3600,
            tz: tz.to_string(),
            is_fixed: false,
            rrule: None,
        })?;

        let session_id = new_id();
        let elapsed = 74 * 60;
        self.conn.execute(
            "INSERT INTO time_session
               (id, task_id, block_id, started_at, ended_at, elapsed_sec, heartbeat_at,
                source, is_confirmed, note, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?5,'timer',1,NULL,?7,?8,?8)",
            params![
                session_id,
                demo_task.id,
                block.id,
                block_start,
                block_start + elapsed * 1000,
                elapsed,
                self.device_id,
                now
            ],
        )?;
        {
            let tx = self.conn.transaction()?;
            crate::db::rebuild_tracked_caches(&tx)?;
            tx.commit()?;
        }

        // 3. Cleanup, made obvious and non-threatening.
        self.create_task(NewTask {
            title: "Delete this project when you're done".into(),
            project_id: Some(project.id.clone()),
            estimate_sec: Some(120),
            ..Default::default()
        })?;

        self.set_setting(SEED_FLAG, &serde_json::Value::Bool(true))?;
        self.set_setting(
            "onboarding.railExplained",
            &serde_json::Value::Bool(false),
        )?;
        Ok(())
    }
}
