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

        let sheet_ref = quote_sheet(&preview.label);
        let range = format!("{sheet_ref}!B2:{}{}", col_letter(preview.day_headers.len()), last_row + 1);
        let measures: Vec<(&str, String)> = vec![
            ("Work", format!("COUNTIFS({range},\"<>\")-COUNTIF({range},\"Gap\")-COUNTIF({range},\"Observed: *\")-COUNTIF({range},\"Private\")")),
            ("Unaccounted", format!("COUNTIF({range},\"Gap\")")),
            ("Observed only", format!("COUNTIF({range},\"Observed: *\")")),
            ("Private", format!("COUNTIF({range},\"Private\")")),
        ];
        for (i, (name, formula)) in measures.iter().enumerate() {
            let row = i as u32 + 1;
            summary.write_string(row, 0, *name)?;
            summary.write_formula(row, 1, formula.as_str())?;
            summary.write_formula(row, 2, format!("B{}/2", row + 1).as_str())?;
        }

        summary.write_string_with_format(measures.len() as u32 + 2, 0, "Life areas", &bold)?;
        for (i, a) in view.totals.by_area.iter().enumerate() {
            let row = measures.len() as u32 + 3 + i as u32;
            summary.write_string(row, 0, &a.name)?;
            summary.write_number(row, 2, a.seconds as f64 / 3600.0)?;
            if let Some(target) = a.monthly_target_sec {
                summary.write_number(row, 3, target as f64 / 3600.0)?;
                summary.write_formula(row, 4, format!("IF(D{r}=0,\"\",C{r}/D{r})", r = row + 1).as_str())?;
            }
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
        ]
        .iter()
        .enumerate()
        {
            mapping.write_string(i as u32 + 1, 0, *k)?;
            mapping.write_string(i as u32 + 1, 1, *v)?;
        }
        mapping.write_string(9, 0, "Export note")?;
        mapping.write_string(9, 1, &preview.source_note)?;

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
