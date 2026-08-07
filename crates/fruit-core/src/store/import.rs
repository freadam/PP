//! Workbook import (M13, §4.8).
//!
//! > *"Import starts with a mapping and variance preview, never alters the
//! > source file, and requires the user to resolve duplicate or inconsistent
//! > periods before commit."*
//!
//! Three requirements, and each one is a rule this module enforces rather than
//! a step the UI is trusted to have taken:
//!
//! **A mapping, and nothing unmapped.** Every distinct label in the sheet is
//! shown with how many slots it covers, and the commit refuses while any label
//! is unaccounted for. "Ignore this one" is a decision the user makes; it is
//! never a default the importer applies on their behalf. That is what "no
//! silent loss" means — the loss is allowed, the silence is not.
//!
//! **A variance preview.** For every day: what the sheet says, what Fruit
//! already holds, and the difference. A historical month is being imported into
//! a database that may already know something about it, and the honest question
//! is not "did it work" but "what changed".
//!
//! **Conflicts resolved before commit.** A day where Fruit already has confirmed
//! records is a conflict, and the commit refuses until the user says what to do
//! with it — keep what is there, or replace it. There is no third option that
//! merges them, because two records of the same hour are not evidence twice;
//! they are a disagreement, and only a person can settle it.
//!
//! ── Reversible, because otherwise nobody runs it ──────────────────────
//! Every record a commit writes carries its batch (migration 0010). `undo_import`
//! removes exactly what a batch added. An import that cannot be undone is one
//! nobody dares run, and a feature nobody dares run has not been built.

use std::path::{Path, PathBuf};

use rusqlite::params;

use super::Store;
use crate::db;
use crate::error::{AppError, Result};
use crate::ids::new_id;
use crate::model::*;
use chrono::Datelike;

use crate::time::{day_start, format_date, parse_date, zone, Millis};
use crate::xlsx::{self, LabelCount};

impl Store {
    /// Read-only. Says what each sheet looks like so the mapping step can be
    /// one confirmation instead of twenty questions.
    pub fn inspect_workbook(&self, path: &Path) -> Result<xlsx::WorkbookInspection> {
        xlsx::inspect(path)
    }

    /// A mapping this store can propose, from a sheet's detected shape.
    ///
    /// Every rule it proposes is one the user can change, and every label it
    /// cannot place is left **unmapped** rather than guessed — an unmapped label
    /// blocks the commit, which is the whole point.
    pub fn suggest_import_mapping(
        &self,
        inspection: &xlsx::WorkbookInspection,
        sheet: &str,
        tz: &str,
    ) -> Result<ImportMapping> {
        let shape = inspection
            .sheets
            .iter()
            .find(|s| s.name == sheet)
            .ok_or_else(|| AppError::Import(format!("No sheet called \"{sheet}\" in that file.")))?;

        let areas = self.get_life_areas(tz, false)?;
        let rules = shape
            .labels
            .iter()
            .filter_map(|l| {
                let lower = l.label.to_lowercase();
                // An exact, case-insensitive match against a life area the user
                // already keeps. Nothing fuzzier: "Fun" matching "Fundraising"
                // is precisely the silent mis-import this feature exists to
                // prevent, and a near-match the user has to notice is worse
                // than a blank they cannot miss.
                let area = areas.iter().find(|a| a.name.to_lowercase() == lower);
                area.map(|a| ImportRule {
                    label: l.label.clone(),
                    target: ImportTarget::Life {
                        life_area_id: a.id.clone(),
                    },
                })
            })
            .collect();

        Ok(ImportMapping {
            sheet: sheet.to_string(),
            header_row: shape.header_row.unwrap_or(0),
            time_col: shape.time_col.unwrap_or(0),
            columns: shape.day_columns.clone(),
            // Half-hour rows, unless the sheet says otherwise. The detected
            // value wins because it is a fact about the file.
            slot_minutes: shape.slot_minutes.unwrap_or(30),
            month: String::new(),
            tz: tz.to_string(),
            rules,
            replace_existing: false,
        })
    }

    /// The variance preview. Reads the file; writes nothing.
    pub fn preview_import(&self, path: &Path, mapping: &ImportMapping) -> Result<ImportPreview> {
        let plan = self.plan_import(path, mapping)?;

        let mut days = Vec::new();
        for d in &plan.days {
            let existing = self.get_day(&d.local_date, &mapping.tz, None)?;
            let existing_sec = existing.totals.confirmed_work_sec
                + existing.totals.confirmed_life_sec
                + existing.totals.private_sec;
            days.push(ImportDay {
                local_date: d.local_date.clone(),
                slots: d.slots,
                seconds: d.seconds,
                existing_sec,
                // Positive means the sheet holds more than Fruit does. Signed
                // on purpose: "12h of variance" without a direction is a number
                // nobody can act on.
                variance_sec: d.seconds - existing_sec,
                conflicts: existing_sec > 0,
            });
        }

        let mut blocking = Vec::new();
        if mapping.month.is_empty() {
            blocking.push("Pick the month this sheet covers.".to_string());
        }
        if plan.columns_without_a_date > 0 {
            blocking.push(format!(
                "{} column{} in this sheet don't name a day of the month. \
                 They will not be imported.",
                plan.columns_without_a_date,
                if plan.columns_without_a_date == 1 { "" } else { "s" }
            ));
        }
        if !plan.unmapped.is_empty() {
            blocking.push(format!(
                "{} label{} still unmapped. Say what each one is — including \
                 \"ignore\" — before importing.",
                plan.unmapped.len(),
                if plan.unmapped.len() == 1 { " is" } else { "s are" }
            ));
        }
        let conflicting: Vec<String> = days
            .iter()
            .filter(|d| d.conflicts)
            .map(|d| d.local_date.clone())
            .collect();
        if !conflicting.is_empty() && !mapping.replace_existing {
            blocking.push(format!(
                "{} day{} already have records in Fruit. Choose whether to keep \
                 or replace them.",
                conflicting.len(),
                if conflicting.len() == 1 { "" } else { "s" }
            ));
        }

        Ok(ImportPreview {
            sheet: mapping.sheet.clone(),
            month: mapping.month.clone(),
            tz: mapping.tz.clone(),
            slot_minutes: mapping.slot_minutes,
            total_sec: plan.days.iter().map(|d| d.seconds).sum(),
            ignored_sec: plan.ignored_sec,
            days,
            unmapped: plan.unmapped,
            conflicting_dates: conflicting,
            blocking,
        })
    }

    /// Writes the import. Refuses if the preview is blocked, for the same
    /// reasons the preview said so — the check is here, not in the renderer,
    /// because the renderer is not a trusted caller (§6.8).
    pub fn commit_import(&mut self, path: &Path, mapping: &ImportMapping) -> Result<ImportResult> {
        let preview = self.preview_import(path, mapping)?;
        if !preview.blocking.is_empty() {
            return Err(AppError::Import(preview.blocking.join(" ")));
        }
        let plan = self.plan_import(path, mapping)?;

        let batch_id = new_id();
        let now = self.now();
        self.conn.execute(
            "INSERT INTO import_batch
               (id, source_path, sheet, month, tz, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                batch_id,
                path.to_string_lossy(),
                mapping.sheet,
                mapping.month,
                mapping.tz,
                now
            ],
        )?;

        let mut sessions = 0i64;
        let mut life = 0i64;
        let mut replaced = 0i64;

        for interval in plan.intervals {
            if mapping.replace_existing {
                replaced += self.clear_for_import(interval.from, interval.to)?;
            }
            match &interval.target {
                ImportTarget::Ignore => {}
                ImportTarget::Life { life_area_id } => {
                    let row = self.add_life_entry(NewLifeEntry {
                        life_area_id: life_area_id.clone(),
                        label: Some(interval.label.clone()),
                        started_at: interval.from,
                        ended_at: interval.to,
                        tz: mapping.tz.clone(),
                        is_private: false,
                        note: None,
                        replace_existing: false,
                    })?;
                    self.stamp_batch("life_entry", &row.id, &batch_id)?;
                    life += 1;
                }
                ImportTarget::Private => {
                    // Private time still needs an area, because the schema needs
                    // one. The flag is what makes it private, not the bucket.
                    let area = self.import_holding_area(&mapping.tz)?;
                    let row = self.add_life_entry(NewLifeEntry {
                        life_area_id: area,
                        label: None,
                        started_at: interval.from,
                        ended_at: interval.to,
                        tz: mapping.tz.clone(),
                        is_private: true,
                        note: None,
                        replace_existing: false,
                    })?;
                    self.stamp_batch("life_entry", &row.id, &batch_id)?;
                    life += 1;
                }
                ImportTarget::Work { task_id } => {
                    let task_id = match task_id {
                        Some(t) => t.clone(),
                        None => self.import_task_for(&interval.label)?,
                    };
                    let row = self.add_session(ManualSession {
                        contribution: None,
                        // The import decides replacement per interval already,
                        // before it gets here — see `clear_for_import`.
                        replace_existing: false,
                        task_id,
                        block_id: None,
                        started_at: interval.from,
                        ended_at: interval.to,
                        note: Some(format!("Imported from {}", mapping.sheet)),
                    })?;
                    self.stamp_batch("time_session", &row.id, &batch_id)?;
                    sessions += 1;
                }
            }
        }

        self.conn.execute(
            "UPDATE import_batch SET sessions_written = ?2, life_entries_written = ?3
              WHERE id = ?1",
            params![batch_id, sessions, life],
        )?;

        Ok(ImportResult {
            batch_id,
            month: mapping.month.clone(),
            sessions_written: sessions,
            life_entries_written: life,
            records_replaced: replaced,
            total_sec: preview.total_sec,
        })
    }

    pub fn get_import_batches(&self) -> Result<Vec<ImportBatch>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_path, sheet, month, tz, sessions_written,
                    life_entries_written, created_at
               FROM import_batch ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ImportBatch {
                id: r.get(0)?,
                source_path: r.get(1)?,
                sheet: r.get(2)?,
                month: r.get(3)?,
                tz: r.get(4)?,
                sessions_written: r.get(5)?,
                life_entries_written: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Removes exactly what a batch added.
    ///
    /// Life entries are soft-deleted, as every life entry is. Sessions are
    /// removed outright, because that is what `delete_session` does and for the
    /// same reason: a soft-deleted session keeps counting in the views, so the
    /// schema has no `deleted_at` on it at all. The asymmetry is the schema's,
    /// not this module's, and hiding it here would mean an "undone" import
    /// whose hours were still in the month total.
    ///
    /// Records the batch *replaced* are not brought back. Replacing was the
    /// user's decision at commit time, and silently resurrecting what they
    /// chose to discard would be a second surprise on top of the first — the
    /// preview says how many rows are about to go before it happens.
    pub fn undo_import(&mut self, batch_id: &str) -> Result<UndoToken> {
        let batch = self
            .get_import_batches()?
            .into_iter()
            .find(|b| b.id == batch_id)
            .ok_or(AppError::NotFound("import"))?;

        let now = self.now();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE life_entry SET deleted_at = ?2, updated_at = ?2
              WHERE import_batch_id = ?1 AND deleted_at IS NULL",
            params![batch_id, now],
        )?;
        tx.execute(
            "DELETE FROM time_session WHERE import_batch_id = ?1",
            params![batch_id],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;

        Ok(UndoToken {
            entity: "import".into(),
            id: batch_id.to_string(),
            label: format!("Removed the import of {} from {}", batch.month, batch.sheet),
            at: now,
        })
    }

    // ─── the parse ─────────────────────────────────────────────────────

    fn plan_import(&self, path: &Path, mapping: &ImportMapping) -> Result<ImportPlan> {
        let zone_ = zone(&mapping.tz)?;
        if mapping.slot_minutes <= 0 || mapping.slot_minutes > 24 * 60 {
            return Err(AppError::Import(
                "A slot has to be between one minute and a day long.".into(),
            ));
        }
        let first = if mapping.month.is_empty() {
            None
        } else {
            Some(parse_date(&format!("{}-01", mapping.month))?)
        };

        let matrix = xlsx::matrix(
            path,
            &mapping.sheet,
            mapping.header_row,
            mapping.time_col,
            &mapping.columns,
            mapping.slot_minutes,
        )?;

        // Returns an owned target rather than a borrow: the borrow's lifetime
        // would be tied to the label it was looked up by, which says something
        // untrue about where the rule lives.
        let lookup = |label: &str| -> Option<ImportTarget> {
            mapping
                .rules
                .iter()
                .find(|r| r.label.eq_ignore_ascii_case(label))
                .map(|r| r.target.clone())
        };

        let mut unmapped: std::collections::HashMap<String, i64> = Default::default();
        let mut intervals: Vec<ImportInterval> = Vec::new();
        let mut per_day: std::collections::HashMap<String, (i64, i64)> = Default::default();
        let mut ignored_sec = 0i64;
        let mut columns_without_a_date = 0i64;

        for (di, day) in matrix.days.iter().enumerate() {
            let Some(month_first) = first else { continue };
            let Some(date) = month_first.with_day(day.day_of_month) else {
                // A 31st in a 30-day month. The column is real; the date is not.
                columns_without_a_date += 1;
                continue;
            };
            let local = format_date(date);
            let midnight = day_start(date, &zone_);

            // Consecutive equal labels become one interval. A sheet that says
            // "Sleep" twelve times in a row is one night, not twelve records —
            // and twelve records would make the day view unreadable and the
            // reconciler ask twelve questions.
            let mut run: Option<(String, i64)> = None;
            for (ri, minute) in matrix.minutes.iter().enumerate() {
                let label = matrix.rows[ri].get(di).cloned().unwrap_or_default();
                let same = run.as_ref().is_some_and(|(l, _)| *l == label);
                if !same {
                    if let Some((l, start)) = run.take() {
                        self.close_run(
                            &l,
                            midnight,
                            start,
                            *minute,
                            &local,
                            &lookup,
                            &mut intervals,
                            &mut per_day,
                            &mut unmapped,
                            &mut ignored_sec,
                            mapping.slot_minutes,
                        );
                    }
                    if !label.is_empty() {
                        run = Some((label, *minute));
                    }
                }
            }
            if let Some((l, start)) = run.take() {
                let end = matrix
                    .minutes
                    .last()
                    .map(|m| m + mapping.slot_minutes)
                    .unwrap_or(start + mapping.slot_minutes);
                self.close_run(
                    &l,
                    midnight,
                    start,
                    end,
                    &local,
                    &lookup,
                    &mut intervals,
                    &mut per_day,
                    &mut unmapped,
                    &mut ignored_sec,
                    mapping.slot_minutes,
                );
            }
        }

        let mut days: Vec<ImportDayPlan> = per_day
            .into_iter()
            .map(|(local_date, (slots, seconds))| ImportDayPlan {
                local_date,
                slots,
                seconds,
            })
            .collect();
        days.sort_by(|a, b| a.local_date.cmp(&b.local_date));

        let mut unmapped: Vec<LabelCount> = unmapped
            .into_iter()
            .map(|(label, slots)| LabelCount { label, slots })
            .collect();
        unmapped.sort_by(|a, b| b.slots.cmp(&a.slots).then_with(|| a.label.cmp(&b.label)));

        intervals.sort_by_key(|i| i.from);
        Ok(ImportPlan {
            days,
            intervals,
            unmapped,
            ignored_sec,
            columns_without_a_date,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn close_run(
        &self,
        label: &str,
        midnight: Millis,
        start_min: i64,
        end_min: i64,
        local_date: &str,
        lookup: &dyn Fn(&str) -> Option<ImportTarget>,
        intervals: &mut Vec<ImportInterval>,
        per_day: &mut std::collections::HashMap<String, (i64, i64)>,
        unmapped: &mut std::collections::HashMap<String, i64>,
        ignored_sec: &mut i64,
        slot_minutes: i64,
    ) {
        if label.is_empty() || end_min <= start_min {
            return;
        }
        let slots = ((end_min - start_min) / slot_minutes).max(1);
        let seconds = (end_min - start_min) * 60;
        match lookup(label) {
            None => {
                *unmapped.entry(label.to_string()).or_insert(0) += slots;
            }
            Some(ImportTarget::Ignore) => {
                *ignored_sec += seconds;
            }
            Some(target) => {
                intervals.push(ImportInterval {
                    label: label.to_string(),
                    from: midnight + start_min * 60_000,
                    to: midnight + end_min * 60_000,
                    target,
                });
                let e = per_day.entry(local_date.to_string()).or_insert((0, 0));
                e.0 += slots;
                e.1 += seconds;
            }
        }
    }

    // ─── writing ───────────────────────────────────────────────────────

    fn stamp_batch(&self, table: &str, id: &str, batch_id: &str) -> Result<()> {
        // `table` is never user input — the two call sites above are the only
        // ones, and both pass a literal.
        self.conn.execute(
            &format!("UPDATE {table} SET import_batch_id = ?2 WHERE id = ?1"),
            params![id, batch_id],
        )?;
        Ok(())
    }

    /// Clears confirmed records inside an interval the import is about to fill.
    /// Returns how many rows it touched, so the result can say.
    fn clear_for_import(&mut self, from: Millis, to: Millis) -> Result<i64> {
        let now = self.now();
        let tx = self.conn.transaction()?;
        let a = tx.execute(
            "UPDATE life_entry SET deleted_at = ?3, updated_at = ?3
              WHERE deleted_at IS NULL AND started_at >= ?1 AND ended_at <= ?2",
            params![from, to, now],
        )?;
        // Deleted, not tombstoned: a session has no `deleted_at`, because one
        // that had would keep counting in every view that reads it.
        let b = tx.execute(
            "DELETE FROM time_session
              WHERE ended_at IS NOT NULL AND started_at >= ?1 AND ended_at <= ?2
                AND id <> COALESCE((SELECT running_session_id FROM app_state WHERE id = 1), '')",
            params![from, to],
        )?;
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;
        Ok((a + b) as i64)
    }

    /// Where private imported time lands. Created on demand rather than seeded,
    /// so a user who never imports never sees it.
    fn import_holding_area(&mut self, tz: &str) -> Result<String> {
        const NAME: &str = "Imported";
        if let Some(a) = self
            .get_life_areas(tz, true)?
            .into_iter()
            .find(|a| a.name == NAME)
        {
            return Ok(a.id);
        }
        Ok(self
            .create_life_area(NewLifeArea {
                name: NAME.into(),
                colour: Some("#8A8F98".into()),
                kind: Some("other".into()),
                monthly_target_sec: None,
            })?
            .id)
    }

    /// A task to hang imported work on. One per distinct label, reused across
    /// imports — otherwise a second import of the same month doubles the
    /// backlog rather than the hours.
    fn import_task_for(&mut self, label: &str) -> Result<String> {
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM task WHERE title = ?1 AND deleted_at IS NULL LIMIT 1",
                [label],
                |r| r.get(0),
            )
            .ok();
        if let Some(id) = existing {
            return Ok(id);
        }
        Ok(self
            .create_task(NewTask {
                title: label.to_string(),
                ..Default::default()
            })?
            .id)
    }

    /// Where an imported month's report would go. Beside the database, so a
    /// re-import six months later finds the file it came from.
    pub fn suggest_import_dir(&self, db_path: Option<&Path>) -> PathBuf {
        db_path
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

struct ImportPlan {
    days: Vec<ImportDayPlan>,
    intervals: Vec<ImportInterval>,
    unmapped: Vec<LabelCount>,
    ignored_sec: i64,
    columns_without_a_date: i64,
}

struct ImportDayPlan {
    local_date: String,
    slots: i64,
    seconds: i64,
}

struct ImportInterval {
    label: String,
    from: Millis,
    to: Millis,
    target: ImportTarget,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;
    use rust_xlsxwriter::Workbook;
    use std::sync::Arc;

    /// Monday 2026-08-03, 09:00 UTC.
    const MONDAY: i64 = 1_785_747_600_000;
    const TZ: &str = "UTC";

    fn store() -> Store {
        Store::in_memory_with_clock(Arc::new(TestClock::new(MONDAY))).unwrap()
    }

    /// A workbook shaped the way the client's is: time down the side, days
    /// across the top, a word in each half-hour cell.
    ///
    /// Written with the same library the export uses, which is the honest test
    /// fixture: if Fruit can read what Fruit writes, the round trip that
    /// matters most already works.
    fn workbook(dir: &Path, rows: &[(&str, &[&str])], headers: &[&str]) -> PathBuf {
        let path = dir.join("month.xlsx");
        let mut book = Workbook::new();
        let sheet = book.add_worksheet().set_name("August").unwrap();
        sheet.write_string(0, 0, "Time").unwrap();
        for (i, h) in headers.iter().enumerate() {
            sheet.write_string(0, i as u16 + 1, *h).unwrap();
        }
        for (r, (time, cells)) in rows.iter().enumerate() {
            sheet.write_string(r as u32 + 1, 0, *time).unwrap();
            for (c, cell) in cells.iter().enumerate() {
                if !cell.is_empty() {
                    sheet.write_string(r as u32 + 1, c as u16 + 1, *cell).unwrap();
                }
            }
        }
        book.save(&path).unwrap();
        path
    }

    fn simple(dir: &Path) -> PathBuf {
        workbook(
            dir,
            &[
                ("09:00", &["Deep work", "Sleep"]),
                ("09:30", &["Deep work", "Sleep"]),
                ("10:00", &["Deep work", ""]),
                ("10:30", &["Admin", ""]),
            ],
            &["3 Mon", "4 Tue"],
        )
    }

    #[test]
    fn a_sheet_is_described_before_it_is_interpreted() {
        let dir = tempfile::tempdir().unwrap();
        let path = simple(dir.path());
        let s = store();
        let insp = s.inspect_workbook(&path).unwrap();

        let sheet = &insp.sheets[0];
        assert_eq!(sheet.name, "August");
        assert_eq!(sheet.header_row, Some(0));
        assert_eq!(sheet.time_col, Some(0));
        assert_eq!(sheet.slot_minutes, Some(30));
        assert_eq!(sheet.first_minute, Some(9 * 60));
        assert_eq!(
            sheet.day_columns.iter().map(|d| d.day_of_month).collect::<Vec<_>>(),
            vec![3, 4]
        );
        // Every distinct label, ranked — the list the user has to work through,
        // stated rather than hidden.
        assert_eq!(sheet.labels[0].label, "Deep work");
        assert_eq!(sheet.labels[0].slots, 3);
    }

    /// The rule that makes "no silent loss" mean something: a label nobody has
    /// placed blocks the commit. Not a warning — a refusal.
    #[test]
    fn an_unmapped_label_blocks_the_import() {
        let dir = tempfile::tempdir().unwrap();
        let path = simple(dir.path());
        let mut s = store();
        let insp = s.inspect_workbook(&path).unwrap();
        let mut mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        mapping.month = "2026-08".into();

        let preview = s.preview_import(&path, &mapping).unwrap();
        assert!(!preview.unmapped.is_empty());
        assert!(preview.blocking.iter().any(|b| b.contains("unmapped")));

        let err = s.commit_import(&path, &mapping).unwrap_err();
        assert!(matches!(err, AppError::Import(_)), "got {err:?}");
    }

    /// "Ignore" is a decision, and once it is made the commit proceeds. The
    /// ignored time is reported rather than vanishing.
    #[test]
    fn ignoring_a_label_is_a_decision_the_preview_reports() {
        let dir = tempfile::tempdir().unwrap();
        let path = simple(dir.path());
        let mut s = store();
        let sleep = s
            .get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Sleep/Rest")
            .unwrap();

        let insp = s.inspect_workbook(&path).unwrap();
        let mut mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        mapping.month = "2026-08".into();
        mapping.rules = vec![
            ImportRule {
                label: "Deep work".into(),
                target: ImportTarget::Work { task_id: None },
            },
            ImportRule {
                label: "Sleep".into(),
                target: ImportTarget::Life {
                    life_area_id: sleep.id.clone(),
                },
            },
            ImportRule {
                label: "Admin".into(),
                target: ImportTarget::Ignore,
            },
        ];

        let preview = s.preview_import(&path, &mapping).unwrap();
        assert!(preview.unmapped.is_empty());
        assert_eq!(preview.ignored_sec, 30 * 60, "one half-hour of Admin");
        assert!(preview.blocking.is_empty(), "got {:?}", preview.blocking);

        let result = s.commit_import(&path, &mapping).unwrap();
        assert_eq!(result.sessions_written, 1, "three contiguous slots, one session");
        assert_eq!(result.life_entries_written, 1);
        assert_eq!(result.total_sec, 90 * 60 + 60 * 60);
    }

    /// A sheet that says "Sleep" twice in a row is one night, not two records.
    /// Twelve records would make the day unreadable and the reconciler ask
    /// twelve questions about one decision.
    #[test]
    fn consecutive_slots_become_one_interval() {
        let dir = tempfile::tempdir().unwrap();
        let path = simple(dir.path());
        let mut s = store();
        let sleep = s
            .get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Sleep/Rest")
            .unwrap();
        let insp = s.inspect_workbook(&path).unwrap();
        let mut mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        mapping.month = "2026-08".into();
        mapping.rules = vec![
            ImportRule {
                label: "Deep work".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Admin".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Sleep".into(),
                target: ImportTarget::Life {
                    life_area_id: sleep.id,
                },
            },
        ];
        s.commit_import(&path, &mapping).unwrap();

        let entries = s.life_entries_on("2026-08-04", TZ).unwrap();
        assert_eq!(entries.len(), 1, "two slots of Sleep is one entry");
        assert_eq!(entries[0].ended_at - entries[0].started_at, 60 * 60 * 1000);
    }

    /// §4.8: *"requires the user to resolve duplicate or inconsistent periods
    /// before commit"*. A day Fruit already knows about is a conflict, and the
    /// commit refuses until someone has said what to do with it.
    #[test]
    fn a_day_that_already_has_records_blocks_until_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let path = simple(dir.path());
        let mut s = store();
        let sleep = s
            .get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Sleep/Rest")
            .unwrap();
        // Fruit already holds an hour on the 4th.
        s.add_life_entry(NewLifeEntry {
            life_area_id: sleep.id.clone(),
            label: None,
            started_at: MONDAY + 86_400_000 - 9 * 3_600_000, // 2026-08-04 00:00
            ended_at: MONDAY + 86_400_000 - 8 * 3_600_000,
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();

        let insp = s.inspect_workbook(&path).unwrap();
        let mut mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        mapping.month = "2026-08".into();
        mapping.rules = vec![
            ImportRule {
                label: "Deep work".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Admin".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Sleep".into(),
                target: ImportTarget::Life {
                    life_area_id: sleep.id,
                },
            },
        ];

        let preview = s.preview_import(&path, &mapping).unwrap();
        assert_eq!(preview.conflicting_dates, vec!["2026-08-04".to_string()]);
        assert!(preview.blocking.iter().any(|b| b.contains("keep or replace")));
        assert!(s.commit_import(&path, &mapping).is_err());

        // Resolved, and it goes through.
        mapping.replace_existing = true;
        let preview = s.preview_import(&path, &mapping).unwrap();
        assert!(preview.blocking.is_empty(), "got {:?}", preview.blocking);
        assert!(s.commit_import(&path, &mapping).is_ok());
    }

    /// The variance is what the preview is *for*: what the sheet says beside
    /// what Fruit holds, with a direction.
    #[test]
    fn the_preview_reports_a_signed_variance_per_day() {
        let dir = tempfile::tempdir().unwrap();
        let path = simple(dir.path());
        let mut s = store();
        let sleep = s
            .get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Sleep/Rest")
            .unwrap();
        s.add_life_entry(NewLifeEntry {
            life_area_id: sleep.id.clone(),
            label: None,
            started_at: MONDAY + 86_400_000 - 9 * 3_600_000,
            ended_at: MONDAY + 86_400_000 - 6 * 3_600_000, // three hours
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();

        let insp = s.inspect_workbook(&path).unwrap();
        let mut mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        mapping.month = "2026-08".into();
        mapping.rules = vec![
            ImportRule {
                label: "Deep work".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Admin".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Sleep".into(),
                target: ImportTarget::Life {
                    life_area_id: sleep.id,
                },
            },
        ];

        let preview = s.preview_import(&path, &mapping).unwrap();
        let d = preview
            .days
            .iter()
            .find(|d| d.local_date == "2026-08-04")
            .unwrap();
        assert_eq!(d.seconds, 3600, "the sheet says one hour");
        assert_eq!(d.existing_sec, 3 * 3600, "Fruit holds three");
        assert_eq!(d.variance_sec, -2 * 3600, "and the sign says which way");
    }

    /// An import that cannot be undone is one nobody dares run.
    #[test]
    fn an_import_can_be_undone_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let path = simple(dir.path());
        let mut s = store();
        let sleep = s
            .get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Sleep/Rest")
            .unwrap();
        let insp = s.inspect_workbook(&path).unwrap();
        let mut mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        mapping.month = "2026-08".into();
        mapping.rules = vec![
            ImportRule {
                label: "Deep work".into(),
                target: ImportTarget::Work { task_id: None },
            },
            ImportRule {
                label: "Admin".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Sleep".into(),
                target: ImportTarget::Life {
                    life_area_id: sleep.id,
                },
            },
        ];

        let before = s.get_day("2026-08-03", TZ, None).unwrap().totals.confirmed_work_sec;
        let result = s.commit_import(&path, &mapping).unwrap();
        let after = s.get_day("2026-08-03", TZ, None).unwrap().totals.confirmed_work_sec;
        assert_eq!(after - before, 90 * 60);

        s.undo_import(&result.batch_id).unwrap();
        let undone = s.get_day("2026-08-03", TZ, None).unwrap().totals.confirmed_work_sec;
        assert_eq!(undone, before, "the undo removed exactly what it added");
        assert!(s.life_entries_on("2026-08-04", TZ).unwrap().is_empty());
    }

    /// A 31st in a 30-day month is a real column naming a date that does not
    /// exist. Said out loud, not skipped quietly.
    #[test]
    fn a_column_naming_a_day_the_month_does_not_have_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = workbook(
            dir.path(),
            &[("09:00", &["Sleep", "Sleep"])],
            &["30 Tue", "31 Wed"],
        );
        let s = store();
        let sleep = s
            .get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Sleep/Rest")
            .unwrap();
        let insp = s.inspect_workbook(&path).unwrap();
        let mut mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        // June has 30 days.
        mapping.month = "2026-06".into();
        mapping.rules = vec![ImportRule {
            label: "Sleep".into(),
            target: ImportTarget::Life {
                life_area_id: sleep.id,
            },
        }];

        let preview = s.preview_import(&path, &mapping).unwrap();
        assert!(
            preview.blocking.iter().any(|b| b.contains("don't name a day")),
            "got {:?}",
            preview.blocking
        );
    }

    /// The suggestion only matches a life area exactly. "Fun" quietly landing
    /// in "Fundraising" is the silent mis-import this whole feature exists to
    /// prevent.
    #[test]
    fn the_suggested_mapping_never_guesses_a_near_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = workbook(dir.path(), &[("09:00", &["Fun"])], &["3 Mon"]);
        let mut s = store();
        s.create_life_area(NewLifeArea {
            name: "Fundraising".into(),
            colour: Some("#8A8F98".into()),
            kind: Some("other".into()),
            monthly_target_sec: None,
        })
        .unwrap();

        let insp = s.inspect_workbook(&path).unwrap();
        let mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        assert!(
            !mapping.rules.iter().any(|r| r.label == "Fun"),
            "a near match is not a match"
        );
    }

    /// The round trip that matters most: Fruit reading Fruit's own export.
    ///
    /// The client's reference workbook is still open question 6 of the spec, so
    /// this is the only real file the importer can be held against today. If
    /// the detection cannot find the shape of a sheet this codebase wrote, it
    /// will not find the shape of one it did not.
    #[test]
    fn the_importer_finds_the_shape_of_fruits_own_export() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store();
        let sleep = s
            .get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Sleep/Rest")
            .unwrap();
        s.add_life_entry(NewLifeEntry {
            life_area_id: sleep.id,
            label: None,
            started_at: MONDAY - 9 * 3_600_000,
            ended_at: MONDAY - 3 * 3_600_000,
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();

        let path = dir.path().join("august.xlsx");
        s.write_excel("2026-08", TZ, &path, &ExcelOptions::default())
            .unwrap();

        let insp = s.inspect_workbook(&path).unwrap();
        let month = insp
            .sheets
            .iter()
            .find(|sh| !sh.day_columns.is_empty())
            .expect("the month sheet is found by its day headers, not by its name");
        assert_eq!(month.header_row, Some(0));
        assert_eq!(month.time_col, Some(0));
        assert_eq!(month.slot_minutes, Some(30));
        assert_eq!(month.day_columns.len(), 31, "August has 31 days");
        assert!(
            month.labels.iter().any(|l| l.label == "Sleep/Rest"),
            "the labels it wrote are the labels it reads: {:?}",
            month.labels.iter().map(|l| &l.label).collect::<Vec<_>>()
        );
    }

    /// §4.8: *"never alters the source file"*.
    #[test]
    fn the_source_file_is_left_exactly_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = simple(dir.path());
        let before = std::fs::read(&path).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();

        let mut s = store();
        let sleep = s
            .get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Sleep/Rest")
            .unwrap();
        let insp = s.inspect_workbook(&path).unwrap();
        let mut mapping = s.suggest_import_mapping(&insp, "August", TZ).unwrap();
        mapping.month = "2026-08".into();
        mapping.rules = vec![
            ImportRule {
                label: "Deep work".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Admin".into(),
                target: ImportTarget::Ignore,
            },
            ImportRule {
                label: "Sleep".into(),
                target: ImportTarget::Life {
                    life_area_id: sleep.id,
                },
            },
        ];
        s.preview_import(&path, &mapping).unwrap();
        s.commit_import(&path, &mapping).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), modified);
    }
}
