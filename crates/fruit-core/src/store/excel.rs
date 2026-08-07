//! Excel export (Plan Rev 3 §10, wireframe screen 5).
//!
//! Excel is the client's primary exchange format, and the export exists to
//! replace a workbook they maintain by hand. Two rules follow from that, and
//! they are the whole design:
//!
//! **1. Colour communicates; structured values compute.** The old workbook
//! totalled its month by counting cell fills, which is why its numbers drifted
//! from its own table. Here the month sheet carries a text label per slot and a
//! fill for legibility, and every total is a real `SUMIF` over those labels. Open
//! the file, change a cell, and the totals move — which is what "auditable in
//! Excel" has to mean.
//!
//! **2. The preview is the sheet.** `preview_excel` and `write_excel` render
//! from the same matrix, so the screen cannot promise a layout the file doesn't
//! have.
//!
//! The reconciliation table is the third thing, and it is the reason to trust
//! either: it puts a figure from the app beside the same figure recomputed from
//! the sheet's own cells. Success measure 7 is "exports reconcile with no
//! unexplained variance" — this is what makes that checkable.

use std::path::Path;

use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};

use super::Store;
use crate::error::Result;
use crate::model::*;
use crate::time::{day_start, parse_date, zone};

/// Half-hour rows, as the workbook has. Not configurable: the sheet is a
/// familiar artefact, and a month exported at 5-minute resolution is 288 rows
/// nobody can read.
const SLOT_MINUTES: i64 = 30;

impl Store {
    /// The matrix behind both the preview and the file.
    pub fn preview_excel(
        &self,
        month: &str,
        tz: &str,
        options: &ExcelOptions,
    ) -> Result<ExcelPreview> {
        let view = self.get_month(month, tz)?;
        let zone_ = zone(tz)?;

        let mut day_headers = Vec::new();
        let mut columns: Vec<Vec<ExcelCell>> = Vec::new();
        for d in &view.days {
            let date = parse_date(&d.local_date)?;
            day_headers.push(format!(
                "{} {}",
                d.day_of_month,
                weekday_short(&date.format("%a").to_string())
            ));
            columns.push(self.day_column(&d.local_date, tz, &zone_, options)?);
        }

        let slots = columns.iter().map(Vec::len).max().unwrap_or(48);
        let slot_labels = (0..slots)
            .map(|i| {
                let m = i as i64 * SLOT_MINUTES;
                format!("{:02}:{:02}", m / 60, m % 60)
            })
            .collect();

        // Transposed to `rows[slot][day]`, which is the workbook's shape: time
        // down the side, days across the top.
        let mut rows = Vec::with_capacity(slots);
        for slot in 0..slots {
            rows.push(
                columns
                    .iter()
                    .map(|col| {
                        col.get(slot).cloned().unwrap_or(ExcelCell {
                            label: String::new(),
                            kind: "gap".into(),
                            colour: None,
                        })
                    })
                    .collect(),
            );
        }

        Ok(ExcelPreview {
            variances: variances(&view, &rows),
            unreconciled_days: view.unreconciled_days,
            source_note: source_note(&view, options),
            file_name: format!("{} Tracking.xlsx", view.label),
            month: view.month.clone(),
            label: view.label.clone(),
            day_headers,
            slot_labels,
            rows,
        })
    }

    /// One day's column, one cell per half-hour.
    ///
    /// A slot takes the label of whatever owns most of it. That is a lossy view
    /// of a precise record, which is exactly what a printed month table is —
    /// the variance table below reports what the rounding cost, rather than
    /// hiding it.
    fn day_column(
        &self,
        date: &str,
        tz: &str,
        zone_: &chrono_tz::Tz,
        options: &ExcelOptions,
    ) -> Result<Vec<ExcelCell>> {
        let day = self.get_day(date, tz, Some(SLOT_MINUTES))?;
        let _ = day_start(parse_date(date)?, zone_);
        Ok(day
            .slots
            .iter()
            .map(|slot| {
                let dominant = slot
                    .segments
                    .iter()
                    .find(|s| !matches!(s.owner, SlotOwner::Empty));
                match dominant.map(|s| &s.owner) {
                    Some(SlotOwner::Life {
                        area_name,
                        area_colour,
                        area_kind,
                        is_private,
                        label,
                        ..
                    }) => {
                        if *is_private && !options.include_private_labels {
                            // The duration is always exported; only the area's
                            // name is withheld. "Private" is what the workbook
                            // shows, which is the promise Settings makes.
                            ExcelCell {
                                label: "Private".into(),
                                kind: "private".into(),
                                colour: None,
                            }
                        } else {
                            ExcelCell {
                                label: label.clone().unwrap_or_else(|| area_name.clone()),
                                kind: match area_kind {
                                    AreaKind::Entertainment => "entertainment",
                                    AreaKind::Rest => "rest",
                                    _ => "life",
                                }
                                .into(),
                                colour: Some(area_colour.clone()),
                            }
                        }
                    }
                    Some(SlotOwner::Work {
                        task_title,
                        project_colour,
                        ..
                    }) => ExcelCell {
                        label: task_title.clone(),
                        kind: "work".into(),
                        colour: project_colour.clone(),
                    },
                    Some(SlotOwner::Observed { app_id, domain, .. }) if options.include_observed => {
                        ExcelCell {
                            label: format!(
                                "Observed: {}",
                                domain.clone().unwrap_or_else(|| app_id.clone())
                            ),
                            kind: "observed".into(),
                            colour: None,
                        }
                    }
                    _ => ExcelCell {
                        label: if options.include_unaccounted {
                            "Gap".into()
                        } else {
                            String::new()
                        },
                        kind: "gap".into(),
                        colour: None,
                    },
                }
            })
            .collect())
    }

    /// Work in the month, grouped project → task, ordered longest first.
    ///
    /// Titles rather than ids, because the export's figures are `COUNTIF`s over
    /// the month sheet and the month sheet writes a task's **title** into each
    /// work slot. Two tasks sharing a title would therefore share a row, which
    /// is a real limitation of counting a printed grid — and is why the row is
    /// a formula the reader can check rather than a number they must believe.
    fn month_work_breakdown(&self, view: &MonthView, tz: &str) -> Result<Vec<ProjectWork>> {
        use std::collections::HashMap;

        // Keyed on (project name, task title): the same pair the sheet's cells
        // can be counted by.
        let mut by: HashMap<(String, String), i64> = HashMap::new();
        for day in &view.days {
            let d = self.get_day(&day.local_date, tz, None)?;
            for seg in &d.segments {
                if let SlotOwner::Work {
                    task_title,
                    project_name,
                    ..
                } = &seg.owner
                {
                    let project = project_name.clone().unwrap_or_else(|| "No project".into());
                    *by.entry((project, task_title.clone())).or_insert(0) +=
                        (seg.to - seg.from) / 1000;
                }
            }
        }
        let mut projects: HashMap<String, Vec<TaskWork>> = HashMap::new();
        for ((project, title), seconds) in by {
            projects.entry(project).or_default().push(TaskWork { title, seconds });
        }
        let mut out: Vec<ProjectWork> = projects
            .into_iter()
            .map(|(name, mut tasks)| {
                tasks.sort_by(|a, b| {
                    b.seconds.cmp(&a.seconds).then_with(|| a.title.cmp(&b.title))
                });
                let seconds = tasks.iter().map(|t| t.seconds).sum();
                ProjectWork { name, seconds, tasks }
            })
            .collect();
        out.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.name.cmp(&b.name)));
        Ok(out)
    }

    /// Writes the workbook. Three sheets, as the wireframe's tags say: the
    /// month table, a summary, and a source mapping that records which records
    /// were confirmed, observed or imported.
    pub fn write_excel(
        &self,
        month: &str,
        tz: &str,
        path: &Path,
        options: &ExcelOptions,
    ) -> Result<ExcelExportResult> {
        let preview = self.preview_excel(month, tz, options)?;
        let view = self.get_month(month, tz)?;

        let mut book = Workbook::new();
        let header = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0xD5D8DA))
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Center);
        let time_col = Format::new()
            .set_border(FormatBorder::Thin)
            .set_background_color(Color::RGB(0xF3F4F5));
        let plain = Format::new().set_border(FormatBorder::Thin);
        let gap = Format::new()
            .set_border(FormatBorder::Thin)
            .set_background_color(Color::RGB(0xEDEDED))
            .set_italic();
        let bold = Format::new().set_bold();
        let hours = Format::new().set_num_format("0.0");
        let pct = Format::new().set_num_format("0%");

        // ─── 1. the month table ─────────────────────────────────────────
        let sheet = book.add_worksheet().set_name(&preview.label)?;
        sheet.write_string_with_format(0, 0, "Time", &header)?;
        for (i, h) in preview.day_headers.iter().enumerate() {
            sheet.write_string_with_format(0, i as u16 + 1, h, &header)?;
        }
        sheet.set_column_width(0, 8)?;
        for (r, label) in preview.slot_labels.iter().enumerate() {
            let row = r as u32 + 1;
            sheet.write_string_with_format(row, 0, label, &time_col)?;
            for (c, cell) in preview.rows[r].iter().enumerate() {
                let fmt = if cell.kind == "gap" { &gap } else { &plain };
                sheet.write_string_with_format(row, c as u16 + 1, &cell.label, fmt)?;
            }
        }
        let last_row = preview.slot_labels.len() as u32;
        sheet.set_freeze_panes(1, 1)?;

        // ─── 2. the summary ─────────────────────────────────────────────
        //
        // Every figure is a formula over the month sheet, not a number this
        // program computed and pasted. That is the difference between a report
        // and a screenshot: change a cell on sheet 1 and these move.
        let summary = book.add_worksheet().set_name("Summary")?;
        summary.set_column_width(0, 28)?;
        summary.set_column_width(1, 14)?;
        summary.write_string_with_format(0, 0, "Measure", &header)?;
        summary.write_string_with_format(0, 1, "Half-hour slots", &header)?;
        summary.write_string_with_format(0, 2, "Hours", &header)?;

        summary.set_column_width(3, 12)?;
        summary.set_column_width(4, 10)?;
        summary.set_column_width(5, 16)?;
        summary.write_string_with_format(0, 3, "Target h", &header)?;
        summary.write_string_with_format(0, 4, "Of target", &header)?;
        // §4.8 says totals must be formulas over this sheet. Most are. Some
        // cannot be: the month grid carries a label per slot, and a label
        // cannot say whether an hour of entertainment fell inside a window you
        // plotted, or that a session was one you attended rather than owned.
        // Those come from the record. Which is which is stated per row rather
        // than left for the reader to work out, because a figure you cannot
        // trace is a figure you cannot check.
        summary.write_string_with_format(0, 5, "Source", &header)?;

        let sheet_ref = quote_sheet(&preview.label);
        let last_col = col_letter(preview.day_headers.len());
        let range = format!("{sheet_ref}!B2:{last_col}{}", last_row + 1);
        let mut row = 1u32;

        // A row whose figure the sheet can recount for itself.
        //
        // The formula carries a **cached result** as well as its text. Excel
        // recomputes on open and the cache is discarded, which is the point of
        // writing formulas at all; but a *library* reading the file — including
        // Fruit's own importer, which uses `calamine` — never evaluates
        // anything and would otherwise see zero in every total. A file whose
        // numbers depend on which program opens it is not an export.
        let from_sheet = |s: &mut rust_xlsxwriter::Worksheet,
                          r: u32,
                          name: &str,
                          slots: &str,
                          seconds: i64|
         -> Result<()> {
            let cached_slots = seconds as f64 / (SLOT_MINUTES as f64 * 60.0);
            s.write_string(r, 0, name)?;
            s.write_formula(
                r,
                1,
                rust_xlsxwriter::Formula::new(slots).set_result(cached_slots.to_string()),
            )?;
            s.write_formula_with_format(
                r,
                2,
                rust_xlsxwriter::Formula::new(&format!("B{}/2", r + 1))
                    .set_result((seconds as f64 / 3600.0).to_string()),
                &hours,
            )?;
            s.write_string(r, 5, "sheet formula")?;
            Ok(())
        };
        // A row the sheet cannot express, written from the record and labelled.
        let from_record = |s: &mut rust_xlsxwriter::Worksheet,
                           r: u32,
                           name: &str,
                           seconds: i64|
         -> Result<()> {
            s.write_string(r, 0, name)?;
            s.write_number_with_format(r, 2, seconds as f64 / 3600.0, &hours)?;
            s.write_string(r, 5, "from the record")?;
            Ok(())
        };

        // The cached figure for each is what the *sheet* holds, counted from
        // the same cells the formula counts — not the app's own total. The two
        // differ by the half-hour rounding, and the variance table is where
        // that difference is reported rather than hidden.
        let sheet_sec = |kind: &str| -> i64 {
            preview.rows.iter().flatten().filter(|c| c.kind == kind).count() as i64
                * SLOT_MINUTES
                * 60
        };
        let sheet_work = sheet_sec("work")
            + sheet_sec("life")
            + sheet_sec("rest")
            + sheet_sec("entertainment");
        for (name, formula, cached) in [
            ("Work", format!("COUNTIFS({range},\"<>\")-COUNTIF({range},\"Gap\")-COUNTIF({range},\"Observed: *\")-COUNTIF({range},\"Private\")"), sheet_work),
            ("Unaccounted", format!("COUNTIF({range},\"Gap\")"), sheet_sec("gap")),
            ("Observed only", format!("COUNTIF({range},\"Observed: *\")"), sheet_sec("observed")),
            ("Private", format!("COUNTIF({range},\"Private\")"), sheet_sec("private")),
        ] {
            from_sheet(summary, row, name, &formula, cached)?;
            row += 1;
        }

        // ── weekly totals ───────────────────────────────────────────────
        //
        // Day columns run in date order, so a week is a contiguous span of
        // them and each week's figure stays a real formula over its own cells.
        row += 1;
        summary.write_string_with_format(row, 0, "By week", &bold)?;
        row += 1;
        for week in week_spans(&view) {
            let cols = format!(
                "{sheet_ref}!{}2:{}{}",
                col_letter(week.first_day),
                col_letter(week.last_day),
                last_row + 1
            );
            // Counted from the same cells the formula counts: this week's day
            // columns, confirmed kinds only.
            let cached: i64 = preview
                .rows
                .iter()
                .flat_map(|r| r.iter().enumerate())
                .filter(|(i, _)| *i + 1 >= week.first_day && *i + 1 <= week.last_day)
                .filter(|(_, c)| {
                    matches!(c.kind.as_str(), "work" | "life" | "rest" | "entertainment")
                })
                .count() as i64
                * SLOT_MINUTES
                * 60;
            from_sheet(
                summary,
                row,
                &week.label,
                &format!("COUNTIFS({cols},\"<>\")-COUNTIF({cols},\"Gap\")-COUNTIF({cols},\"Observed: *\")-COUNTIF({cols},\"Private\")"),
                cached,
            )?;
            row += 1;
        }

        // ── work by project and task ────────────────────────────────────
        //
        // A work slot carries its task's title, so each task is one COUNTIF
        // and a project is the sum of its tasks' rows. Still the sheet's own
        // arithmetic, which is why it is worth doing this way rather than
        // pasting numbers that would drift the first time a cell was edited.
        row += 1;
        summary.write_string_with_format(row, 0, "Work by project and task", &bold)?;
        row += 1;
        for project in self.month_work_breakdown(&view, tz)? {
            summary.write_string_with_format(row, 0, &project.name, &bold)?;
            let first_task_row = row + 2;
            let project_row = row;
            let mut project_slots = 0i64;
            row += 1;
            for task in &project.tasks {
                summary.write_string(row, 0, &format!("    {}", task.title))?;
                // Cached as the **sheet's** count, not the record's exact
                // seconds. `COUNTIF` returns a whole number of cells; caching
                // 2.47 there would mean a file that says one thing to a library
                // and another the moment Excel recalculates, which is worse
                // than caching nothing. The gap between the two is the
                // half-hour rounding, and the variance table reports it.
                let slots = preview
                    .rows
                    .iter()
                    .flatten()
                    .filter(|c| c.kind == "work" && c.label == task.title)
                    .count() as i64;
                summary.write_formula(
                    row,
                    1,
                    rust_xlsxwriter::Formula::new(&format!(
                        "COUNTIF({range},\"{}\")",
                        escape_criteria(&task.title)
                    ))
                    .set_result(slots.to_string()),
                )?;
                summary.write_formula_with_format(
                    row,
                    2,
                    rust_xlsxwriter::Formula::new(&format!("B{}/2", row + 1))
                        .set_result((slots as f64 / 2.0).to_string()),
                    &hours,
                )?;
                project_slots += slots;
                summary.write_string(row, 5, "sheet formula")?;
                row += 1;
            }
            if !project.tasks.is_empty() {
                summary.write_formula_with_format(
                    project_row,
                    2,
                    rust_xlsxwriter::Formula::new(&format!("SUM(C{first_task_row}:C{})", row))
                        .set_result((project_slots as f64 / 2.0).to_string()),
                    &hours,
                )?;
                summary.write_string(project_row, 5, "sheet formula")?;
            }
        }

        // ── work by contribution ────────────────────────────────────────
        //
        // From the record: a slot label is a task title, and no label can say
        // whether you attended that hour or owned it. Life areas never appear
        // here because `life_entry` has no contribution column at all — the
        // guarantee is structural, not a filter applied on the way out.
        row += 1;
        summary.write_string_with_format(row, 0, "Work by contribution", &bold)?;
        row += 1;
        for c in &view.totals.by_contribution {
            let name = match c.contribution {
                Some(k) => contribution_label(k),
                None => "Not set",
            };
            from_record(summary, row, name, c.seconds)?;
            row += 1;
        }

        // ── the entertainment split, which is the point of the product ──
        row += 1;
        summary.write_string_with_format(row, 0, "Entertainment", &bold)?;
        row += 1;
        let unplanned =
            view.totals.entertainment_sec - view.totals.entertainment_in_window_sec;
        for (name, seconds) in [
            ("Core", core_seconds(&view)),
            ("Entertainment, total", view.totals.entertainment_sec),
            ("  planned — inside a window you plotted", view.totals.entertainment_in_window_sec),
            ("  unplanned", unplanned),
        ] {
            from_record(summary, row, name, seconds)?;
            row += 1;
        }
        for domain in ["youtube.com", "www.youtube.com", "youtu.be", "twitch.tv", "www.twitch.tv"] {
            let seconds: i64 = view
                .totals
                .by_domain
                .iter()
                .filter(|d| d.domain == domain)
                .map(|d| d.seconds)
                .sum();
            if seconds > 0 {
                from_record(summary, row, &format!("  {domain}"), seconds)?;
                row += 1;
            }
        }

        // ── life areas against their targets ────────────────────────────
        row += 1;
        summary.write_string_with_format(row, 0, "Life areas", &bold)?;
        row += 1;
        for a in view.totals.by_area.iter() {
            summary.write_string(row, 0, &a.name)?;
            summary.write_number_with_format(row, 2, a.seconds as f64 / 3600.0, &hours)?;
            summary.write_string(row, 5, "from the record")?;
            if let Some(target) = a.monthly_target_sec {
                summary.write_number_with_format(row, 3, target as f64 / 3600.0, &hours)?;
                summary.write_formula_with_format(
                    row,
                    4,
                    format!("IF(D{r}=0,\"\",C{r}/D{r})", r = row + 1).as_str(),
                    &pct,
                )?;
            }
            row += 1;
        }

        // ─── 3. source mapping ──────────────────────────────────────────
        let mapping = book.add_worksheet().set_name("Source mapping")?;
        mapping.set_column_width(0, 14)?;
        mapping.set_column_width(1, 60)?;
        mapping.write_string_with_format(0, 0, "Field", &header)?;
        mapping.write_string_with_format(0, 1, "What it means", &header)?;
        for (i, (k, v)) in [
            ("Work", "A confirmed timer session against a project task."),
            ("Life areas", "A confirmed life entry you recorded by hand."),
            ("Private", "Accounted for on purpose; nothing recorded about it."),
            ("Observed:", "The machine saw this application. Nobody confirmed what it was."),
            ("Gap", "Unaccounted. Neither recorded nor observed."),
            ("Totals", "Formulas over the month sheet — change a cell and they move."),
            (
                "Source: sheet formula",
                "Counted from this workbook's own cells. Edit the month sheet and the figure moves.",
            ),
            (
                "Source: from the record",
                "Taken from Fruit's records, because the month sheet cannot express it — a slot \
                 label cannot say whether an hour of entertainment fell inside a window you \
                 plotted, or whether a session was one you attended rather than owned.",
            ),
            (
                "Rounding",
                "The sheet is a half-hour grid and the record is to the second, so a 20-minute \
                 session fills one slot and reads as 30 minutes. The variance table reports the \
                 difference rather than hiding it.",
            ),
        ]
        .iter()
        .enumerate()
        {
            mapping.write_string(i as u32 + 1, 0, *k)?;
            mapping.write_string(i as u32 + 1, 1, *v)?;
        }
        mapping.write_string(12, 0, "Export note")?;
        mapping.write_string(12, 1, &preview.source_note)?;

        book.save(path)?;

        Ok(ExcelExportResult {
            path: path.display().to_string(),
            sheets: vec![preview.label.clone(), "Summary".into(), "Source mapping".into()],
            rows_written: last_row as i64,
            variances: preview.variances,
        })
    }
}

/// App figures beside the same figures recounted from the sheet's own cells.
///
/// The variance is real and is expected to be non-zero: the sheet is a
/// half-hour grid and the record is to the second, so a 20-minute session
/// occupies one slot and reads as 30 minutes. Reporting that is the point —
/// "no *unexplained* variance" is the measure, and rounding is explained.
fn variances(view: &MonthView, rows: &[Vec<ExcelCell>]) -> Vec<ExcelVariance> {
    let slot = SLOT_MINUTES * 60;
    let count = |kind: &str| -> i64 {
        rows.iter()
            .flatten()
            .filter(|c| c.kind == kind)
            .count() as i64
            * slot
    };
    let sheet_confirmed =
        count("work") + count("life") + count("rest") + count("entertainment") + count("private");
    let app_confirmed = view.totals.confirmed_work_sec
        + view.totals.confirmed_life_sec
        + view.totals.private_sec;

    vec![
        ExcelVariance {
            measure: "Accounted".into(),
            app_sec: app_confirmed,
            sheet_sec: sheet_confirmed,
            variance_sec: sheet_confirmed - app_confirmed,
        },
        ExcelVariance {
            measure: "Observed only".into(),
            app_sec: view.totals.observed_only_sec,
            sheet_sec: count("observed"),
            variance_sec: count("observed") - view.totals.observed_only_sec,
        },
        ExcelVariance {
            measure: "Unaccounted".into(),
            app_sec: view.totals.empty_sec,
            sheet_sec: count("gap"),
            variance_sec: count("gap") - view.totals.empty_sec,
        },
    ]
}

fn source_note(view: &MonthView, options: &ExcelOptions) -> String {
    let mut parts = vec![format!(
        "{} · exported from Fruit. Confirmed records, machine observations and gaps are labelled \
         separately; totals are formulas over this sheet, never cell colours.",
        view.label
    )];
    if view.unreconciled_days > 0 {
        parts.push(format!(
            "{} day{} in this month were never reconciled — their observed and unaccounted slots \
             are exported as they stand.",
            view.unreconciled_days,
            if view.unreconciled_days == 1 { "" } else { "s" }
        ));
    }
    if !options.include_private_labels {
        parts.push(
            "Private time is included by duration; its area is not named.".into(),
        );
    }
    parts.join(" ")
}

fn weekday_short(s: &str) -> String {
    s.to_string()
}

struct ProjectWork {
    name: String,
    seconds: i64,
    tasks: Vec<TaskWork>,
}

struct TaskWork {
    title: String,
    seconds: i64,
}

/// One week's slice of the month's day columns.
struct WeekSpan {
    label: String,
    /// 1-based day-column indices, the same numbering `col_letter` takes.
    first_day: usize,
    last_day: usize,
}

/// The month's days grouped into ISO weeks, as column spans.
///
/// Day columns are written in date order, so a week is always contiguous and
/// its total stays a formula over its own cells rather than a pasted number.
/// Partial weeks at either end are real weeks and are labelled with the dates
/// they actually cover — a "week" of two days reported as a full one is how a
/// month's first row ends up looking like a collapse in output.
fn week_spans(view: &MonthView) -> Vec<WeekSpan> {
    let mut out: Vec<WeekSpan> = Vec::new();
    let mut current: Option<(String, usize, usize)> = None;

    for (i, day) in view.days.iter().enumerate() {
        let col = i + 1; // 1-based: column 1 is the first day, written at B
        let week = match crate::store::goals::iso_week(&day.local_date) {
            Ok(w) => w,
            Err(_) => continue,
        };
        match &mut current {
            Some((w, _, last)) if *w == week => *last = col,
            Some((w, first, last)) => {
                out.push(WeekSpan {
                    label: format!("{w}  ({}–{})", view.days[*first - 1].local_date, view.days[*last - 1].local_date),
                    first_day: *first,
                    last_day: *last,
                });
                current = Some((week, col, col));
            }
            None => current = Some((week, col, col)),
        }
    }
    if let Some((w, first, last)) = current {
        out.push(WeekSpan {
            label: format!("{w}  ({}–{})", view.days[first - 1].local_date, view.days[last - 1].local_date),
            first_day: first,
            last_day: last,
        });
    }
    out
}

/// Seconds in life areas whose kind is `core` — the workbook's own word for
/// the time that is neither entertainment nor rest.
fn core_seconds(view: &MonthView) -> i64 {
    view.totals
        .by_area
        .iter()
        .filter(|a| a.kind == AreaKind::Core)
        .map(|a| a.seconds)
        .sum()
}

fn contribution_label(c: Contribution) -> &'static str {
    match c {
        // `Contribution::None` is the explicit "work, no mode" the user chose,
        // which §5.8 distinguishes from `Option::None` — never having been
        // asked. Both are reported; they are different answers.
        Contribution::None => "None — recorded as work with no mode",
        Contribution::Attend => "Attend — you were present",
        Contribution::Support => "Support — you helped someone else's work",
        Contribution::Own => "Own — the outcome was yours",
        Contribution::Assist => "Assist — you did part of it, someone else led",
    }
}

/// Escapes a task title for use inside a `COUNTIF` criteria string.
///
/// A `"` in a title would end the criteria early and produce a formula Excel
/// refuses to open — the file would look written and be unopenable. Doubling it
/// is the escape Excel uses. `*` and `?` are wildcards in a criteria and are
/// left alone deliberately: a title containing one is rare, and matching more
/// than intended is visible in the total, whereas a broken formula is not.
fn escape_criteria(title: &str) -> String {
    title.replace('"', "\"\"")
}

/// A1-style column letter for a 1-based data column (column 1 is B, after Time).
fn col_letter(days: usize) -> String {
    let mut n = days + 1; // +1 for the Time column
    let mut out = String::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        out.insert(0, (b'A' + rem as u8) as char);
        n = (n - 1) / 26;
    }
    out
}

/// Sheet names with spaces need quoting inside a formula reference.
fn quote_sheet(name: &str) -> String {
    if name.contains(' ') {
        format!("'{name}'")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::col_letter;

    #[test]
    fn column_letters_cover_a_long_month() {
        assert_eq!(col_letter(1), "B");
        assert_eq!(col_letter(25), "Z");
        assert_eq!(col_letter(26), "AA");
        assert_eq!(col_letter(31), "AF");
    }
}

#[cfg(test)]
mod summary_tests {
    use super::*;
    use crate::clock::TestClock;
    use std::sync::Arc;

    /// Monday 2026-08-03, 09:00 UTC.
    const MONDAY: i64 = 1_785_747_600_000;
    const TZ: &str = "UTC";

    fn store() -> Store {
        Store::in_memory_with_clock(Arc::new(TestClock::new(MONDAY))).unwrap()
    }

    /// Reads the Summary sheet back out of the written file.
    ///
    /// The tests below assert on the **file**, not on the struct that produced
    /// it. An export is the one artefact whose correctness is entirely about
    /// what ends up on disk, and a test that checks the intermediate value
    /// would pass while writing nothing.
    fn summary_rows(path: &Path) -> Vec<Vec<String>> {
        use calamine::{open_workbook_auto, Reader};
        let mut wb = open_workbook_auto(path).unwrap();
        let range = wb.worksheet_range("Summary").unwrap();
        (0..range.height())
            .map(|r| {
                (0..range.width())
                    .map(|c| {
                        range
                            .get((r, c))
                            .map(|d| d.to_string())
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect()
    }

    fn has_row(rows: &[Vec<String>], label: &str) -> bool {
        rows.iter().any(|r| r.first().is_some_and(|c| c.contains(label)))
    }

    fn seeded(dir: &Path) -> (Store, std::path::PathBuf) {
        let mut s = store();
        let project = s
            .create_project(NewProject {
                name: "Deep work".into(),
                colour: Some("#7E8CF0".into()),
                kind: Some("work".into()),
                weekly_target_sec: None,
            })
            .unwrap();
        let task = s
            .create_task(NewTask {
                title: "Refactor auth".into(),
                project_id: Some(project.id.clone()),
                ..Default::default()
            })
            .unwrap();
        s.add_session(ManualSession {
            task_id: task.id.clone(),
            block_id: None,
            started_at: MONDAY,
            ended_at: MONDAY + 2 * 3_600_000,
            note: None,
            contribution: Some(Contribution::Attend),
            replace_existing: false,
        })
        .unwrap();

        // An evening plotted for a film, and an evening of it that was not.
        let fun = s
            .create_life_area(NewLifeArea {
                name: "Telly".into(),
                colour: Some("#C9603F".into()),
                kind: Some("entertainment".into()),
                monthly_target_sec: None,
            })
            .unwrap();
        s.schedule_block(NewBlock {
            task_id: None,
            label: Some("Film".into()),
            starts_at: MONDAY + 11 * 3_600_000,
            duration_sec: 2 * 3600,
            tz: TZ.into(),
            is_fixed: false,
            rrule: None,
            intent: Some(BlockIntent::Entertainment),
        })
        .unwrap();
        for (from, to) in [(11i64, 12i64), (14, 15)] {
            s.add_life_entry(NewLifeEntry {
                life_area_id: fun.id.clone(),
                label: None,
                started_at: MONDAY + from * 3_600_000,
                ended_at: MONDAY + to * 3_600_000,
                tz: TZ.into(),
                is_private: false,
                note: None,
                replace_existing: false,
            })
            .unwrap();
        }

        let path = dir.join("august.xlsx");
        s.write_excel("2026-08", TZ, &path, &ExcelOptions::default())
            .unwrap();
        (s, path)
    }

    /// §4.8 enumerates what the export contains. Seven of those were missing;
    /// this is the list, checked against the file itself.
    #[test]
    fn the_summary_carries_everything_the_spec_lists() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, path) = seeded(dir.path());
        let rows = summary_rows(&path);

        for required in [
            "By week",
            "Work by project and task",
            "Work by contribution",
            "Core",
            "Entertainment, total",
            "planned",
            "unplanned",
            "Life areas",
        ] {
            assert!(
                has_row(&rows, required),
                "the export is missing the {required:?} block that §4.8 requires"
            );
        }
    }

    /// The provenance of every figure is stated, because §4.8 requires totals
    /// to be formulas over the sheet and some genuinely cannot be. A reader has
    /// to be able to tell which is which without opening the source.
    #[test]
    fn every_figure_says_where_it_came_from() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, path) = seeded(dir.path());
        let rows = summary_rows(&path);

        assert!(rows[0].iter().any(|c| c == "Source"), "the column is headed");

        let sources: Vec<&String> = rows
            .iter()
            .skip(1)
            .filter_map(|r| r.get(5))
            .filter(|c| !c.is_empty())
            .collect();
        assert!(!sources.is_empty());
        for s in &sources {
            assert!(
                *s == "sheet formula" || *s == "from the record",
                "unexpected provenance {s:?}"
            );
        }
        assert!(sources.iter().any(|s| *s == "sheet formula"));
        assert!(
            sources.iter().any(|s| *s == "from the record"),
            "the entertainment split cannot be a formula and must say so"
        );
    }

    /// A week is a contiguous run of day columns, so its total stays a real
    /// formula rather than a pasted number. August 2026 starts on a Saturday
    /// and ends on a Monday, so both ends are partial weeks — which are real
    /// weeks and are labelled with the dates they cover.
    #[test]
    fn weeks_are_contiguous_column_spans_including_the_partial_ones() {
        let (s, _) = (store(), ());
        let view = s.get_month("2026-08", TZ).unwrap();
        let weeks = week_spans(&view);

        assert!(weeks.len() >= 5, "August 2026 touches at least five ISO weeks");
        assert_eq!(weeks[0].first_day, 1, "the first week starts at the first day");
        assert_eq!(
            weeks.last().unwrap().last_day,
            view.days.len(),
            "the last week ends at the last day"
        );
        for pair in weeks.windows(2) {
            assert_eq!(
                pair[1].first_day,
                pair[0].last_day + 1,
                "weeks are contiguous and cover every column exactly once"
            );
        }
        // The label carries the dates, so a two-day week cannot be mistaken for
        // a full one.
        assert!(weeks[0].label.contains("2026-08-01"));
    }

    /// A cached formula result has to equal what Excel will compute when it
    /// recalculates, or the file says one thing to a library and another to a
    /// spreadsheet — which is worse than caching nothing at all.
    ///
    /// The trap here is specific: `COUNTIF` returns a whole number of *cells*,
    /// and the record holds exact seconds. Caching the record's figure in a
    /// slot-count cell produced 2.47 where Excel would produce 3.
    #[test]
    fn cached_results_are_what_the_formula_will_recompute() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, path) = seeded(dir.path());
        let rows = summary_rows(&path);

        // Every slot-count cell holds a whole number, because that is what
        // COUNTIF returns.
        for r in rows.iter().skip(1) {
            let (Some(slots), Some(source)) = (r.get(1), r.get(5)) else { continue };
            if source != "sheet formula" || slots.is_empty() {
                continue;
            }
            let n: f64 = slots.parse().unwrap_or(0.0);
            assert_eq!(
                n.fract(),
                0.0,
                "a COUNTIF cache must be a whole number of cells, got {slots}"
            );
        }

        // And hours are always exactly half the slot count, since a slot is
        // half an hour — the relationship the "B/2" formula encodes.
        for r in rows.iter().skip(1) {
            let (Some(slots), Some(hours), Some(source)) = (r.get(1), r.get(2), r.get(5)) else {
                continue;
            };
            if source != "sheet formula" || slots.is_empty() || hours.is_empty() {
                continue;
            }
            let (n, h): (f64, f64) = (slots.parse().unwrap(), hours.parse().unwrap());
            assert!((h - n / 2.0).abs() < 1e-9, "{slots} slots should be {} hours, cached {hours}", n / 2.0);
        }
    }

    /// A quotation mark in a task title would end a COUNTIF criteria early and
    /// produce a formula Excel refuses to open — the file looks written and is
    /// unopenable.
    #[test]
    fn a_quotation_mark_in_a_title_cannot_break_the_formula() {
        assert_eq!(escape_criteria(r#"Fix the "auth" bug"#), r#"Fix the ""auth"" bug"#);
        assert_eq!(escape_criteria("Ordinary title"), "Ordinary title");
    }

    /// Contribution is work-only by construction: `life_entry` has no such
    /// column, so a life area can never appear in this block however the export
    /// is written.
    #[test]
    fn the_contribution_block_reports_work_and_only_work() {
        let dir = tempfile::tempdir().unwrap();
        let (s, path) = seeded(dir.path());
        let rows = summary_rows(&path);

        assert!(has_row(&rows, "Attend"), "the seeded session was an Attend");
        assert!(
            !has_row(&rows, "Telly — you were present"),
            "a life area can never carry a contribution"
        );

        let view = s.get_month("2026-08", TZ).unwrap();
        let by_contribution: i64 = view.totals.by_contribution.iter().map(|c| c.seconds).sum();
        assert_eq!(
            by_contribution, view.totals.confirmed_work_sec,
            "the split accounts for all confirmed work and nothing else"
        );
    }

    /// The two entertainment rows partition the total, so a reader can add them
    /// up and get the figure above them.
    #[test]
    fn planned_and_unplanned_entertainment_sum_to_the_total() {
        let dir = tempfile::tempdir().unwrap();
        let (s, _path) = seeded(dir.path());
        let view = s.get_month("2026-08", TZ).unwrap();

        let planned = view.totals.entertainment_in_window_sec;
        let unplanned = view.totals.entertainment_sec - planned;
        assert_eq!(planned, 3600, "one hour fell inside the plotted window");
        assert_eq!(unplanned, 3600, "and one hour did not");
        assert_eq!(planned + unplanned, view.totals.entertainment_sec);
    }
}
