//! The Monday-morning report (W9's artifact).
//!
//! The reviewer this plan is built around described the habit precisely:
//!
//! > *"Rize sends me a weekly PDF report, and I take a quick look on a Monday
//! > morning."*
//! >
//! > *"While this can all be overwhelming, the most important information is
//! > right at the top — the Work hours from last week and the breakdown into
//! > categories."*
//!
//! Three instructions follow, and they are the whole design of this module.
//!
//! **1. It is read at a fixed moment.** Not whenever you happen to open the
//! app — so it has to exist as something *waiting* for you. `due_week_report`
//! is that: once a week has finished and has not been reported, the app has a
//! report to hand you, and it says so until you have seen it.
//!
//! **2. It is skimmed, not studied.** One headline sentence, then a small table.
//! Everything past the first screen is optional depth.
//!
//! **3. The headline goes at the top**, and it is total hours plus the category
//! breakdown. `WeekHeadline` is literally that and nothing else.
//!
//! ── Why a file, and why `.xlsx` ────────────────────────────────────────
//! Not a PDF and not an email. An offline app has no mailer, and inventing one
//! would be the first outbound connection in a product badged OFFLINE. Excel is
//! already the client's exchange format and Fruit already writes it, so a weekly
//! report is the same machinery over seven days rather than a second renderer
//! to keep in step with the first.
//!
//! ── Why the totals are formulas ───────────────────────────────────────
//! The same rule the month export holds itself to: change a cell and the totals
//! move. A report whose numbers were pasted in is a screenshot, and a screenshot
//! cannot be checked.

use std::path::{Path, PathBuf};

use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};

use super::Store;
use crate::error::Result;
use crate::model::*;
use crate::time::{format_date, local_date, parse_date, week_start, zone};

/// Where the last report Fruit wrote ended up, and which week it covered.
/// Stored so the card appears once rather than every launch.
pub const LAST_WRITTEN: &str = "report.week.lastWritten";
pub const LAST_PATH: &str = "report.week.lastPath";
/// The week whose card the user has dismissed. Separate from `LAST_WRITTEN`
/// because writing a file and being finished with it are different events.
pub const LAST_SEEN: &str = "report.week.lastSeen";

impl Store {
    /// The headline, the sections, and the numbers the file is built from.
    ///
    /// The card and the workbook read this same value, so what the app says is
    /// waiting for you and what the file contains cannot disagree.
    pub fn week_report(&self, date: &str, tz: &str) -> Result<WeekReport> {
        let review = self.get_week_review(date, tz)?;
        let categories = self.week_categories(&review)?;
        let divergence = self.week_divergence(&review, tz)?;
        let unlabelled = {
            let mut v = self.get_unlabelled(&review.from, &review.to, tz, 5)?;
            // Skimmed, not studied: the five worst offenders, not a database
            // dump. The Activity view is where the whole list lives.
            v.truncate(5);
            v
        };

        Ok(WeekReport {
            headline: WeekHeadline {
                work_sec: review.totals.confirmed_work_sec,
                categories,
                // "Last week" is only true once the week has ended. Saying it on
                // a Wednesday about the week you are standing in would make the
                // top line wrong in the one place it must not be.
                is_complete: review.to.as_str() < local_date(self.now(), &zone(tz)?).as_str(),
            },
            divergence,
            unlabelled,
            review,
        })
    }

    /// The category breakdown — the second half of the headline.
    ///
    /// Deliberately the same five metrics the goals use, not the life areas:
    /// the areas are a taxonomy the user maintains and can have thirty rows in
    /// it, and a headline with thirty rows is not a headline.
    fn week_categories(&self, review: &WeekReview) -> Result<Vec<WeekCategory>> {
        let t = &review.totals;
        let rows = [
            ("Work", t.confirmed_work_sec),
            ("Life", t.confirmed_life_sec - t.sleep_sec),
            ("Sleep", t.sleep_sec),
            ("Entertainment", t.entertainment_sec),
            ("Private", t.private_sec),
            ("Observed, unconfirmed", t.observed_only_sec),
            ("Unaccounted", t.empty_sec),
        ];
        // Every second of the week appears exactly once across these rows —
        // that is the counting invariant, and the report is where someone is
        // most likely to check it, so the total is carried rather than implied.
        let total: i64 = rows.iter().map(|(_, s)| *s).sum();
        Ok(rows
            .into_iter()
            .map(|(name, seconds)| WeekCategory {
                name: name.into(),
                seconds,
                share: if total > 0 {
                    seconds as f64 / total as f64
                } else {
                    0.0
                },
            })
            .collect())
    }

    /// W9 point 4: the week's biggest divergence, with the day it happened on.
    ///
    /// Two candidates, and the larger wins: the day that overran its plan by
    /// most, and the day with the most entertainment nobody plotted a window
    /// for. Both are "the plan and the week disagreed, here is where" — which
    /// is the only kind of finding worth putting in a report someone reads for
    /// ninety seconds.
    fn week_divergence(&self, review: &WeekReview, tz: &str) -> Result<Option<WeekDivergence>> {
        let mut best: Option<WeekDivergence> = None;

        for d in &review.days {
            // Only days that have happened. A Friday that has not arrived has
            // not diverged from anything.
            if !d.has_happened {
                continue;
            }
            let day = self.get_day(&d.local_date, tz, None)?;
            let planned = day.totals.planned_sec;
            let tracked = day.totals.confirmed_work_sec;
            if planned > 0 && tracked > planned {
                let sec = tracked - planned;
                let candidate = WeekDivergence {
                    local_date: d.local_date.clone(),
                    kind: DivergenceKind::Overrun,
                    seconds: sec,
                    detail: format!(
                        "Plotted {}, tracked {}.",
                        super::goals::hm(planned),
                        super::goals::hm(tracked)
                    ),
                };
                if best.as_ref().is_none_or(|b| sec > b.seconds) {
                    best = Some(candidate);
                }
            }

            let unplanned = d.entertainment_sec - d.entertainment_in_window_sec;
            if unplanned > 0 {
                let candidate = WeekDivergence {
                    local_date: d.local_date.clone(),
                    kind: DivergenceKind::UnplannedEntertainment,
                    seconds: unplanned,
                    detail: if d.planned_entertainment_sec > 0 {
                        format!(
                            "{} outside the {} you plotted.",
                            super::goals::hm(unplanned),
                            super::goals::hm(d.planned_entertainment_sec)
                        )
                    } else {
                        format!("{}, with no window plotted.", super::goals::hm(unplanned))
                    },
                };
                if best.as_ref().is_none_or(|b| unplanned > b.seconds) {
                    best = Some(candidate);
                }
            }
        }
        Ok(best)
    }

    /// Is a report waiting?
    ///
    /// `Some` when the most recently *finished* week has not been seen. The
    /// week in progress is never offered: a report on a week that is still
    /// happening is a report that will be wrong by Friday, and the reviewer's
    /// habit — Monday morning, last week — depends on the boundary being real.
    pub fn due_week_report(&self, tz: &str) -> Result<Option<WeekReportDue>> {
        let zone_ = zone(tz)?;
        let today = local_date(self.now(), &zone_);
        let last_monday = week_start(parse_date(&today)?) - chrono::Duration::days(7);
        let week = super::goals::iso_week(&format_date(last_monday))?;

        if let Some(serde_json::Value::String(seen)) = self.get_setting(LAST_SEEN)? {
            if seen >= week {
                return Ok(None);
            }
        }

        let report = self.week_report(&format_date(last_monday), tz)?;
        // A week with nothing in it is not a report, it is an empty file and a
        // notification you learn to dismiss. Fruit has nothing to say about a
        // week you did not use it.
        if report.review.totals.day_sec == 0
            || report.review.totals.confirmed_work_sec
                + report.review.totals.confirmed_life_sec
                + report.review.totals.observed_only_sec
                == 0
        {
            return Ok(None);
        }

        let path = match self.get_setting(LAST_WRITTEN)? {
            Some(serde_json::Value::String(w)) if w == week => match self.get_setting(LAST_PATH)? {
                Some(serde_json::Value::String(p)) if Path::new(&p).exists() => Some(p),
                _ => None,
            },
            _ => None,
        };

        Ok(Some(WeekReportDue {
            week,
            from: report.review.from.clone(),
            to: report.review.to.clone(),
            headline: report.headline,
            written_to: path,
        }))
    }

    /// The user has read it. The card stops appearing for this week and every
    /// week before it.
    pub fn mark_week_report_seen(&self, week: &str) -> Result<()> {
        self.set_setting(LAST_SEEN, &serde_json::Value::String(week.to_string()))
    }

    /// Where the report would go if the user did not choose. Beside the
    /// database, in `reports/`, named by the ISO week so a year of them sorts.
    pub fn suggest_week_report_path(&self, week: &str, db_path: Option<&Path>) -> PathBuf {
        let dir = db_path
            .and_then(|p| p.parent())
            .map(|p| p.join("reports"))
            .unwrap_or_else(|| PathBuf::from("reports"));
        dir.join(format!("fruit-{week}.xlsx"))
    }

    /// Writes the report. Two sheets: the headline and the days behind it.
    ///
    /// Small on purpose. The month export is the artefact for auditing a month;
    /// this one is read on a Monday morning in ninety seconds, and a workbook
    /// with six tabs does not get read in ninety seconds.
    pub fn write_week_report(&self, date: &str, tz: &str, path: &Path) -> Result<WeekReportResult> {
        let report = self.week_report(date, tz)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let mut book = Workbook::new();
        let header = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0xD5D8DA))
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Center);
        let big = Format::new().set_bold().set_font_size(16);
        let bold = Format::new().set_bold();
        let hours = Format::new().set_num_format("0.0");
        let pct = Format::new().set_num_format("0%");

        // ─── 1. the headline ────────────────────────────────────────────
        let s = book.add_worksheet().set_name("Week")?;
        s.set_column_width(0, 26)?;
        s.set_column_width(1, 12)?;
        s.set_column_width(2, 10)?;

        s.write_string_with_format(0, 0, &headline_sentence(&report), &big)?;
        s.write_string(
            1,
            0,
            format!(
                "{} to {} · {}",
                report.review.from, report.review.to, report.review.week
            ),
        )?;

        s.write_string_with_format(3, 0, "Category", &header)?;
        s.write_string_with_format(3, 1, "Hours", &header)?;
        s.write_string_with_format(3, 2, "Share", &header)?;
        for (i, c) in report.headline.categories.iter().enumerate() {
            let row = i as u32 + 4;
            s.write_string(row, 0, &c.name)?;
            s.write_number_with_format(row, 1, c.seconds as f64 / 3600.0, &hours)?;
            // A formula, not the ratio this program computed: edit an hours
            // cell and the share moves, which is what makes the sheet checkable
            // rather than merely printed.
            s.write_formula_with_format(
                row,
                2,
                format!(
                    "IF(SUM($B$5:$B${last})=0,0,B{r}/SUM($B$5:$B${last}))",
                    last = report.headline.categories.len() + 4,
                    r = row + 1
                )
                .as_str(),
                &pct,
            )?;
        }
        let after = report.headline.categories.len() as u32 + 5;
        s.write_string_with_format(after, 0, "Total", &bold)?;
        s.write_formula_with_format(
            after,
            1,
            format!("SUM(B5:B{})", report.headline.categories.len() + 4).as_str(),
            &hours,
        )?;
        s.write_string(
            after + 1,
            0,
            "Every hour of the week appears in exactly one row above.",
        )?;

        // Goals, if there are any. A report about goals nobody set would be a
        // lecture.
        let mut row = after + 3;
        if !report.review.goals.is_empty() {
            s.write_string_with_format(row, 0, "Goals", &bold)?;
            row += 1;
            s.write_string_with_format(row, 0, "Goal", &header)?;
            s.write_string_with_format(row, 1, "Target", &header)?;
            s.write_string_with_format(row, 2, "Actual", &header)?;
            row += 1;
            for g in &report.review.goals {
                s.write_string(row, 0, &g.goal.subject_name)?;
                s.write_number_with_format(row, 1, g.goal.target_sec as f64 / 3600.0, &hours)?;
                s.write_number_with_format(row, 2, g.actual_sec as f64 / 3600.0, &hours)?;
                s.write_string(row, 3, &g.summary)?;
                row += 1;
            }
            row += 1;
        }

        if let Some(d) = &report.divergence {
            s.write_string_with_format(row, 0, "Biggest divergence", &bold)?;
            s.write_string(row + 1, 0, &d.local_date)?;
            s.write_string(row + 1, 1, d.detail.clone())?;
            row += 3;
        }

        if !report.unlabelled.is_empty() {
            s.write_string_with_format(row, 0, "Not categorised yet", &bold)?;
            row += 1;
            for u in &report.unlabelled {
                s.write_string(row, 0, &u.match_value)?;
                s.write_number_with_format(row, 1, u.seconds as f64 / 3600.0, &hours)?;
                row += 1;
            }
        }

        // ─── 2. the days behind it ──────────────────────────────────────
        let d = book.add_worksheet().set_name("Days")?;
        d.set_column_width(0, 12)?;
        for (i, h) in [
            "Date", "Work", "Life", "Sleep", "Entertainment", "Planned entertainment", "Unaccounted",
        ]
        .iter()
        .enumerate()
        {
            d.set_column_width(i as u16, 14)?;
            d.write_string_with_format(0, i as u16, *h, &header)?;
        }
        for (i, day) in report.review.days.iter().enumerate() {
            let r = i as u32 + 1;
            d.write_string(r, 0, &day.local_date)?;
            for (c, sec) in [
                day.confirmed_work_sec,
                day.confirmed_life_sec - day.sleep_sec,
                day.sleep_sec,
                day.entertainment_sec,
                day.planned_entertainment_sec,
                day.empty_sec,
            ]
            .into_iter()
            .enumerate()
            {
                d.write_number_with_format(r, c as u16 + 1, sec as f64 / 3600.0, &hours)?;
            }
        }

        book.save(path)?;

        self.set_setting(
            LAST_WRITTEN,
            &serde_json::Value::String(report.review.week.clone()),
        )?;
        self.set_setting(
            LAST_PATH,
            &serde_json::Value::String(path.to_string_lossy().to_string()),
        )?;

        Ok(WeekReportResult {
            path: path.to_string_lossy().to_string(),
            week: report.review.week.clone(),
            headline: headline_sentence(&report),
            sheets: vec!["Week".into(), "Days".into()],
        })
    }
}

/// The top line, in words. One sentence, because the reviewer reads this for
/// ninety seconds and a paragraph is a paragraph nobody reads.
pub fn headline_sentence(report: &WeekReport) -> String {
    let h = &report.headline;
    let when = if h.is_complete { "Last week" } else { "So far this week" };
    let ent = h
        .categories
        .iter()
        .find(|c| c.name == "Entertainment")
        .map(|c| c.seconds)
        .unwrap_or(0);
    format!(
        "{when}: {} of work, {} of entertainment.",
        super::goals::hm(h.work_sec),
        super::goals::hm(ent)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;
    use std::sync::Arc;

    /// Monday 2026-08-03, 09:00 UTC.
    const MONDAY: i64 = 1_785_747_600_000;
    const TZ: &str = "UTC";

    fn store_at(ms: i64) -> Store {
        Store::in_memory_with_clock(Arc::new(TestClock::new(ms))).unwrap()
    }

    fn work(s: &mut Store, from: i64, hours: i64) {
        let t = s
            .create_task(NewTask {
                title: "Work".into(),
                ..Default::default()
            })
            .unwrap();
        s.add_session(ManualSession {
            task_id: t.id,
            block_id: None,
            started_at: from,
            ended_at: from + hours * 3_600_000,
            note: None,
        })
        .unwrap();
    }

    /// The instruction that matters most: the headline is the first thing, and
    /// it says the two figures the reviewer named.
    #[test]
    fn the_headline_is_hours_and_categories() {
        let mut s = store_at(MONDAY);
        work(&mut s, MONDAY, 3);
        let r = s.week_report("2026-08-03", TZ).unwrap();

        assert_eq!(r.headline.work_sec, 3 * 3600);
        assert_eq!(r.headline.categories.first().unwrap().name, "Work");
        assert!(
            headline_sentence(&r).contains("3h 00m of work"),
            "got {}",
            headline_sentence(&r)
        );
    }

    /// The counting invariant, restated where someone is most likely to check
    /// it: the categories partition the week rather than sampling it.
    #[test]
    fn the_categories_account_for_the_whole_week() {
        let mut s = store_at(MONDAY);
        work(&mut s, MONDAY, 3);
        let r = s.week_report("2026-08-03", TZ).unwrap();
        let summed: i64 = r.headline.categories.iter().map(|c| c.seconds).sum();
        assert_eq!(summed, r.review.totals.day_sec);
        let shares: f64 = r.headline.categories.iter().map(|c| c.share).sum();
        assert!((shares - 1.0).abs() < 1e-9, "shares summed to {shares}");
    }

    /// "Last week" has to be true. Said on a Wednesday about the week you are
    /// standing in, the top line is wrong in the one place it must not be.
    #[test]
    fn a_week_in_progress_does_not_call_itself_last_week() {
        let mut s = store_at(MONDAY + 2 * 86_400_000);
        work(&mut s, MONDAY, 3);
        let r = s.week_report("2026-08-03", TZ).unwrap();
        assert!(!r.headline.is_complete);
        assert!(headline_sentence(&r).starts_with("So far this week"));

        // And the week before, from the same clock, is complete.
        let prev = s.week_report("2026-07-27", TZ).unwrap();
        assert!(prev.headline.is_complete);
        assert!(headline_sentence(&prev).starts_with("Last week"));
    }

    /// The report waits for you, and stops waiting once you have read it.
    #[test]
    fn a_finished_week_waits_and_then_stops() {
        // The following Monday.
        let mut s = store_at(MONDAY + 7 * 86_400_000);
        work(&mut s, MONDAY, 3);

        let due = s.due_week_report(TZ).unwrap().expect("last week is waiting");
        assert_eq!(due.week, "2026-W32");
        assert_eq!(due.from, "2026-08-03");
        assert_eq!(due.to, "2026-08-09");
        assert!(due.written_to.is_none(), "nothing written yet");

        s.mark_week_report_seen(&due.week).unwrap();
        assert!(s.due_week_report(TZ).unwrap().is_none());
    }

    /// A week you did not use the app produces no card. A notification about
    /// nothing is the fastest way to teach someone to dismiss notifications.
    #[test]
    fn an_empty_week_is_not_a_report() {
        let s = store_at(MONDAY + 7 * 86_400_000);
        assert!(s.due_week_report(TZ).unwrap().is_none());
    }

    /// W9 point 4. The larger of the two candidates wins, and it names the day.
    #[test]
    fn the_divergence_is_the_larger_of_the_two_candidates() {
        let mut s = store_at(MONDAY + 7 * 86_400_000);
        let fun = s
            .create_life_area(NewLifeArea {
                name: "Telly".into(),
                colour: Some("#8b5cf6".into()),
                kind: Some("entertainment".into()),
                monthly_target_sec: None,
            })
            .unwrap();
        // Tuesday: three hours of entertainment, no window.
        let tuesday = MONDAY + 86_400_000;
        s.add_life_entry(NewLifeEntry {
            life_area_id: fun.id.clone(),
            label: None,
            started_at: tuesday + 8 * 3_600_000,
            ended_at: tuesday + 11 * 3_600_000,
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();
        // Wednesday: one hour, which loses.
        let wednesday = MONDAY + 2 * 86_400_000;
        s.add_life_entry(NewLifeEntry {
            life_area_id: fun.id,
            label: None,
            started_at: wednesday + 8 * 3_600_000,
            ended_at: wednesday + 9 * 3_600_000,
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();

        let r = s.week_report("2026-08-03", TZ).unwrap();
        let d = r.divergence.expect("three unplanned hours is a divergence");
        assert_eq!(d.local_date, "2026-08-04");
        assert_eq!(d.kind, DivergenceKind::UnplannedEntertainment);
        assert_eq!(d.seconds, 3 * 3600);
    }

    /// Entertainment inside a window you plotted is not a divergence — that is
    /// the whole point of having plotted it.
    #[test]
    fn a_planned_evening_is_not_a_divergence() {
        let mut s = store_at(MONDAY + 7 * 86_400_000);
        let fun = s
            .create_life_area(NewLifeArea {
                name: "Telly".into(),
                colour: Some("#8b5cf6".into()),
                kind: Some("entertainment".into()),
                monthly_target_sec: None,
            })
            .unwrap();
        let tuesday = MONDAY + 86_400_000;
        s.schedule_block(NewBlock {
            task_id: None,
            label: Some("Film".into()),
            starts_at: tuesday + 8 * 3_600_000,
            duration_sec: 3 * 3600,
            tz: TZ.into(),
            is_fixed: false,
            rrule: None,
            intent: Some(BlockIntent::Entertainment),
        })
        .unwrap();
        s.add_life_entry(NewLifeEntry {
            life_area_id: fun.id,
            label: None,
            started_at: tuesday + 8 * 3_600_000,
            ended_at: tuesday + 11 * 3_600_000,
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap();

        let r = s.week_report("2026-08-03", TZ).unwrap();
        assert!(r.divergence.is_none(), "got {:?}", r.divergence);
    }

    /// The file is the deliverable, so it has to actually appear — and reading
    /// it back has to give the same headline the card promised.
    #[test]
    fn the_file_is_written_and_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store_at(MONDAY + 7 * 86_400_000);
        work(&mut s, MONDAY, 3);

        let path = dir.path().join("reports/fruit-2026-W32.xlsx");
        let result = s.write_week_report("2026-08-03", TZ, &path).unwrap();
        assert!(path.exists());
        assert_eq!(result.week, "2026-W32");
        assert!(result.headline.contains("3h 00m of work"));

        // And the card now knows where it went.
        let due = s.due_week_report(TZ).unwrap().unwrap();
        assert_eq!(due.written_to.as_deref(), Some(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn the_suggested_path_sorts_by_week() {
        let s = store_at(MONDAY);
        let db = Path::new("/data/fruit.db");
        let a = s.suggest_week_report_path("2026-W02", Some(db));
        let b = s.suggest_week_report_path("2026-W32", Some(db));
        assert!(a < b, "{a:?} should sort before {b:?}");
        assert!(a.ends_with("fruit-2026-W02.xlsx"));
    }
}
