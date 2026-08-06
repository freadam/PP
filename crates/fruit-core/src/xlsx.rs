//! Reading a workbook (M13, §4.8 *Import*).
//!
//! Deliberately separate from `store::import`: this module knows about
//! spreadsheets and nothing about Fruit's records, and the store module knows
//! about records and nothing about `.xlsx`. The seam is `Matrix` — a grid of
//! labels with a date per column and a time per row, which is what both the
//! client's workbook and Fruit's own export are once you stop looking at their
//! formatting.
//!
//! ── Why it detects rather than assumes ────────────────────────────────
//! Open question 6 of the spec is still open: *"Which historical month is the
//! accepted Excel import/export reference?"* Nobody has handed us the client's
//! workbook. An importer that assumed a layout would work perfectly on the one
//! file it was written against and mangle every other one, silently, which is
//! the exact failure §4.8 forbids.
//!
//! So this module **finds candidates and reports what it found**. Which header
//! row, which time column, which day columns, and every distinct label with a
//! count — and then a human confirms it. Detection makes the mapping step one
//! click instead of twenty; it never makes it invisible.
//!
//! ── The source file is never written ──────────────────────────────────
//! §4.8: *"never alters the source file"*. Everything here opens for reading and
//! there is no code path in this module that can write.

use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};

use crate::error::{AppError, Result};

/// A cell's text, as the importer sees it.
///
/// Numbers become their plain rendering and dates their ISO form, so a label
/// map keyed on text behaves the same whether the workbook stored "Sleep" as a
/// string or a shared-string reference.
fn cell_text(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            if (f.fract()).abs() < f64::EPSILON {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
        Data::Error(e) => format!("#{e:?}"),
    }
}

/// Minutes past midnight, if this cell reads as a time of day.
///
/// Three spellings, because all three occur in real workbooks: `09:30` as text,
/// an Excel serial time (a fraction of a day), and an ISO datetime.
fn cell_minutes(d: &Data) -> Option<i64> {
    match d {
        Data::Float(f) => {
            // A whole number here is a day count, not a time — `1` is not
            // 00:00, it is a date, and reading it as midnight would silently
            // put a day's worth of rows on one slot.
            let frac = f.fract();
            if *f >= 0.0 && *f < 1.0001 && frac.abs() > f64::EPSILON {
                Some((frac * 24.0 * 60.0).round() as i64 % (24 * 60))
            } else {
                None
            }
        }
        Data::DateTime(dt) => {
            let f = dt.as_f64();
            let frac = f.fract();
            if frac.abs() > f64::EPSILON {
                Some((frac * 24.0 * 60.0).round() as i64 % (24 * 60))
            } else if f < 1.0001 {
                Some(0)
            } else {
                None
            }
        }
        other => parse_clock(&cell_text(other)),
    }
}

/// `9:00`, `09:00`, `09:00:00`, `9.00`, `9am`, `9 AM`.
fn parse_clock(s: &str) -> Option<i64> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    let (body, shift) = if let Some(rest) = s.strip_suffix("am") {
        (rest.trim().to_string(), 0)
    } else if let Some(rest) = s.strip_suffix("pm") {
        (rest.trim().to_string(), 12 * 60)
    } else {
        (s, -1)
    };
    // A bare number is only a time when am/pm says so. `9` on its own is far
    // more likely to be a day column than nine in the morning, and guessing
    // wrong there mis-shapes the whole sheet.
    let (h, m): (i64, i64) = match body.find([':', '.']) {
        Some(sep) => {
            let h = body[..sep].trim().parse().ok()?;
            let m = body[sep + 1..].split([':', '.']).next()?.trim().parse().ok()?;
            (h, m)
        }
        None if shift >= 0 => (body.trim().parse().ok()?, 0),
        None => return None,
    };
    if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
        return None;
    }
    let mut total = h * 60 + m;
    if shift > 0 && h < 12 {
        total += shift;
    }
    if shift == 0 && h == 12 {
        total -= 12 * 60;
    }
    Some(total)
}

/// The day of the month a column header names, if it names one.
///
/// `1`, `01`, `1 Mon`, `Mon 1`, `2025-08-01`, `1 Aug`. Anything else is not a
/// day column, and saying so is better than guessing.
pub fn header_day(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // An ISO date first: it is unambiguous, and `2025-08-01` starts with digits
    // that would otherwise read as the year.
    if s.len() >= 10 {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d") {
            return Some(chrono::Datelike::day(&d));
        }
    }
    for token in s.split(|c: char| !c.is_ascii_digit()) {
        if token.is_empty() {
            continue;
        }
        if let Ok(n) = token.parse::<u32>() {
            if (1..=31).contains(&n) {
                return Some(n);
            }
        }
    }
    None
}

/// One sheet, described rather than interpreted.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetShape {
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    /// The row that looks like day headers, zero-based. `None` when nothing in
    /// the sheet looks like one — which is a finding, not a failure.
    pub header_row: Option<usize>,
    /// The column that looks like times of day, zero-based.
    pub time_col: Option<usize>,
    /// Columns under `header_row` whose text names a day of the month.
    pub day_columns: Vec<DayColumn>,
    /// The first time found in `time_col`, and the gap between successive ones.
    /// A workbook at half-hour resolution says 30 here.
    pub first_minute: Option<i64>,
    pub slot_minutes: Option<i64>,
    /// Every distinct non-empty label in the matrix, most slots first. This is
    /// the list the user maps, and its length is the honest measure of how much
    /// work the mapping step is.
    pub labels: Vec<LabelCount>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayColumn {
    pub col: usize,
    pub header: String,
    pub day_of_month: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelCount {
    pub label: String,
    /// Slots, not hours — the sheet's own unit. Hours need the slot length,
    /// which is a mapping decision rather than a fact about the file.
    pub slots: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookInspection {
    pub path: String,
    pub sheets: Vec<SheetShape>,
}

/// A sheet reduced to what the importer needs: `cells[row][day]`.
pub struct Matrix {
    pub days: Vec<DayColumn>,
    /// Minutes past midnight for each row.
    pub minutes: Vec<i64>,
    /// `rows[row][day_index]`.
    pub rows: Vec<Vec<String>>,
    pub slot_minutes: i64,
}

fn open(path: &Path) -> Result<calamine::Sheets<std::io::BufReader<std::fs::File>>> {
    if !path.exists() {
        return Err(AppError::Import(format!(
            "There is no file at {}.",
            path.display()
        )));
    }
    open_workbook_auto(path).map_err(|e| {
        AppError::Import(format!(
            "Couldn't read {} as a workbook: {e}. Save it as .xlsx and try again.",
            path.display()
        ))
    })
}

/// Read-only. Reports what each sheet looks like; decides nothing.
pub fn inspect(path: &Path) -> Result<WorkbookInspection> {
    let mut wb = open(path)?;
    let names: Vec<String> = wb.sheet_names().to_vec();
    let mut sheets = Vec::new();

    for name in names {
        let Ok(range) = wb.worksheet_range(&name) else {
            continue;
        };
        let (rows, cols) = (range.height(), range.width());

        // The header row is the one with the most day-like cells, searched only
        // near the top: a "12" in the middle of a table is data, not a header.
        let mut header_row = None;
        let mut best = 1usize;
        for r in 0..rows.min(12) {
            let hits = (0..cols)
                .filter(|c| {
                    range
                        .get((r, *c))
                        .map(|d| header_day(&cell_text(d)).is_some())
                        .unwrap_or(false)
                })
                .count();
            if hits > best {
                best = hits;
                header_row = Some(r);
            }
        }

        // The time column is the one with the most time-like cells below the
        // header. Same discipline: most, not first.
        let start = header_row.map(|r| r + 1).unwrap_or(0);
        let mut time_col = None;
        let mut best_times = 3usize;
        for c in 0..cols.min(6) {
            let hits = (start..rows)
                .filter(|r| range.get((*r, c)).and_then(cell_minutes).is_some())
                .count();
            if hits > best_times {
                best_times = hits;
                time_col = Some(c);
            }
        }

        let day_columns = match header_row {
            Some(hr) => (0..cols)
                .filter(|c| Some(*c) != time_col)
                .filter_map(|c| {
                    let header = range.get((hr, c)).map(cell_text).unwrap_or_default();
                    header_day(&header).map(|day_of_month| DayColumn {
                        col: c,
                        header,
                        day_of_month,
                    })
                })
                .collect(),
            None => Vec::new(),
        };

        let times: Vec<i64> = match time_col {
            Some(tc) => (start..rows)
                .filter_map(|r| range.get((r, tc)).and_then(cell_minutes))
                .collect(),
            None => Vec::new(),
        };
        let first_minute = times.first().copied();
        // The modal gap, not the first: a merged cell or a blank row makes one
        // gap wrong, and the mode is unbothered by it.
        let slot_minutes = {
            let mut gaps: std::collections::HashMap<i64, usize> = Default::default();
            for w in times.windows(2) {
                let g = w[1] - w[0];
                if g > 0 {
                    *gaps.entry(g).or_insert(0) += 1;
                }
            }
            gaps.into_iter().max_by_key(|(_, n)| *n).map(|(g, _)| g)
        };

        let mut counts: std::collections::HashMap<String, i64> = Default::default();
        for r in start..rows {
            for d in &day_columns {
                let text = range.get((r, d.col)).map(cell_text).unwrap_or_default();
                if !text.is_empty() {
                    *counts.entry(text).or_insert(0) += 1;
                }
            }
        }
        let mut labels: Vec<LabelCount> = counts
            .into_iter()
            .map(|(label, slots)| LabelCount { label, slots })
            .collect();
        labels.sort_by(|a, b| b.slots.cmp(&a.slots).then_with(|| a.label.cmp(&b.label)));

        sheets.push(SheetShape {
            name,
            rows,
            cols,
            header_row,
            time_col,
            day_columns,
            first_minute,
            slot_minutes,
            labels,
        });
    }

    Ok(WorkbookInspection {
        path: path.to_string_lossy().to_string(),
        sheets,
    })
}

/// Pulls one sheet into the shape the importer works on, using a mapping the
/// user has confirmed.
pub fn matrix(
    path: &Path,
    sheet: &str,
    header_row: usize,
    time_col: usize,
    columns: &[DayColumn],
    slot_minutes: i64,
) -> Result<Matrix> {
    let mut wb = open(path)?;
    let range = wb
        .worksheet_range(sheet)
        .map_err(|_| AppError::Import(format!("There is no sheet called \"{sheet}\" in that file.")))?;

    let mut minutes = Vec::new();
    let mut rows = Vec::new();
    for r in (header_row + 1)..range.height() {
        let Some(m) = range.get((r, time_col)).and_then(cell_minutes) else {
            // A row without a time is a spacer or a total. Skipping it is right:
            // there is no interval to attribute it to.
            continue;
        };
        minutes.push(m);
        rows.push(
            columns
                .iter()
                .map(|d| range.get((r, d.col)).map(cell_text).unwrap_or_default())
                .collect(),
        );
    }

    Ok(Matrix {
        days: columns.to_vec(),
        minutes,
        rows,
        slot_minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_parse_the_way_workbooks_write_them() {
        assert_eq!(parse_clock("09:00"), Some(540));
        assert_eq!(parse_clock("9:00"), Some(540));
        assert_eq!(parse_clock("09:30:00"), Some(570));
        assert_eq!(parse_clock("9.30"), Some(570));
        assert_eq!(parse_clock("9am"), Some(540));
        assert_eq!(parse_clock("9 PM"), Some(21 * 60));
        assert_eq!(parse_clock("12am"), Some(0));
        assert_eq!(parse_clock("12pm"), Some(12 * 60));
        assert_eq!(parse_clock("Sleep"), None);
        assert_eq!(parse_clock("25:00"), None);
        assert_eq!(parse_clock(""), None);
    }

    #[test]
    fn day_headers_are_read_the_way_people_write_them() {
        assert_eq!(header_day("1"), Some(1));
        assert_eq!(header_day("01"), Some(1));
        assert_eq!(header_day("1 Mon"), Some(1));
        assert_eq!(header_day("Mon 12"), Some(12));
        assert_eq!(header_day("2025-08-04"), Some(4));
        assert_eq!(header_day("Time"), None);
        assert_eq!(header_day(""), None);
        // 32 is not a day, and a header that only contains one is not a day
        // column — better to leave it out and say so than to import a month
        // one column wide.
        assert_eq!(header_day("32"), None);
    }

    /// An Excel serial time is a fraction of a day. A whole number is a *date*,
    /// and reading it as midnight would silently pile a day of rows onto one
    /// slot.
    #[test]
    fn a_whole_serial_number_is_a_date_not_midnight() {
        assert_eq!(cell_minutes(&Data::Float(0.5)), Some(12 * 60));
        assert_eq!(cell_minutes(&Data::Float(0.375)), Some(9 * 60));
        assert_eq!(cell_minutes(&Data::Float(45000.0)), None);
        assert_eq!(cell_minutes(&Data::Float(1.0)), None);
    }

    #[test]
    fn a_number_reads_as_its_plain_text() {
        assert_eq!(cell_text(&Data::Float(3.0)), "3");
        assert_eq!(cell_text(&Data::Int(7)), "7");
        assert_eq!(cell_text(&Data::String("  Sleep  ".into())), "Sleep");
        assert_eq!(cell_text(&Data::Empty), "");
    }
}
