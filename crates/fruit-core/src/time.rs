//! Time rules (§6.1).
//!
//! 1. Instants are `i64` milliseconds since the Unix epoch, UTC.
//! 2. Calendar dates are `YYYY-MM-DD` **local** text.
//! 3. Durations are `i64` seconds.
//!
//! Everything that converts between the three lives here, so a timezone bug
//! has exactly one place to be.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::error::{AppError, Result};

pub type Millis = i64;

/// Parse an IANA zone name. The renderer supplies this, so it is validated.
pub fn zone(tz: &str) -> Result<Tz> {
    tz.parse::<Tz>()
        .map_err(|_| AppError::invalid(format!("Unknown time zone '{tz}'.")))
}

/// The local calendar date an instant falls on, in `tz`.
pub fn local_date(at: Millis, tz: &Tz) -> String {
    to_local(at, tz).date_naive().format("%Y-%m-%d").to_string()
}

pub fn to_local(at: Millis, tz: &Tz) -> DateTime<Tz> {
    Utc.timestamp_millis_opt(at).unwrap().with_timezone(tz)
}

/// Local wall-clock minutes since local midnight — the planner's y-axis.
pub fn minutes_into_day(at: Millis, tz: &Tz) -> i64 {
    let l = to_local(at, tz);
    l.hour() as i64 * 60 + l.minute() as i64
}

use chrono::Timelike;

pub fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::invalid(format!("'{s}' is not a YYYY-MM-DD date.")))
}

pub fn format_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Resolve a local wall-clock time to a UTC instant.
///
/// §4.4 rule 5: a time that falls in a DST-skipped hour resolves *forward* to
/// the first valid instant rather than erroring. An ambiguous time (the
/// repeated hour in autumn) resolves to the earlier of the two, which is what
/// a person means when they say "01:30".
pub fn resolve_local(naive: NaiveDateTime, tz: &Tz) -> Millis {
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt.timestamp_millis(),
        LocalResult::Ambiguous(earlier, _later) => earlier.timestamp_millis(),
        LocalResult::None => {
            // Spring-forward gap: walk forward a minute at a time to the first
            // instant that exists. The gap is at most a few hours anywhere.
            let mut probe = naive;
            for _ in 0..(6 * 60) {
                probe += Duration::minutes(1);
                if let LocalResult::Single(dt) = tz.from_local_datetime(&probe) {
                    return dt.timestamp_millis();
                }
                if let LocalResult::Ambiguous(dt, _) = tz.from_local_datetime(&probe) {
                    return dt.timestamp_millis();
                }
            }
            // Unreachable for every real zone; fall back to UTC interpretation.
            naive.and_utc().timestamp_millis()
        }
    }
}

/// Local midnight (start of day) for a local date, as a UTC instant.
pub fn day_start(date: NaiveDate, tz: &Tz) -> Millis {
    resolve_local(date.and_hms_opt(0, 0, 0).unwrap(), tz)
}

/// Exclusive end of a local date — i.e. the next day's midnight. On a DST day
/// this is 23 or 25 hours after `day_start`, which is exactly why blocks are
/// validated against *this* rather than `day_start + 86_400_000`.
pub fn day_end(date: NaiveDate, tz: &Tz) -> Millis {
    day_start(date.succ_opt().unwrap(), tz)
}

/// A block must begin and end within one local day (§2.4).
///
/// Returns the block's `local_date` when valid.
pub fn same_day_span(starts_at: Millis, duration_sec: i64, tz: &Tz) -> Result<String> {
    let start_local = to_local(starts_at, tz);
    let date = start_local.date_naive();
    let ends_at = starts_at + duration_sec * 1000;
    if ends_at > day_end(date, tz) {
        return Err(AppError::CrossesMidnight);
    }
    Ok(format_date(date))
}

/// ±10 years of now — the sanity window for any instant crossing the IPC
/// boundary (§6.8 "argument validation happens in Rust").
pub fn check_plausible(at: Millis, now: Millis) -> Result<()> {
    const TEN_YEARS_MS: i64 = 10 * 365 * 24 * 60 * 60 * 1000;
    if (at - now).abs() > TEN_YEARS_MS {
        return Err(AppError::invalid(
            "That date is more than ten years from now. Check the year.",
        ));
    }
    Ok(())
}

/// Inclusive range of local dates, used by the week query and reports.
pub fn date_series(from: &str, to: &str) -> Result<Vec<String>> {
    let (a, b) = (parse_date(from)?, parse_date(to)?);
    if b < a {
        return Err(AppError::invalid("Range ends before it starts."));
    }
    if (b - a).num_days() > 366 {
        return Err(AppError::invalid("Ranges are limited to one year."));
    }
    let mut out = Vec::new();
    let mut d = a;
    while d <= b {
        out.push(format_date(d));
        d = d.succ_opt().unwrap();
    }
    Ok(out)
}

/// Monday-based start of the ISO week containing `date`.
pub fn week_start(date: NaiveDate) -> NaiveDate {
    let dow = date.weekday().num_days_from_monday() as i64;
    date - Duration::days(dow)
}

/// The instants bounding the local calendar month containing `date`.
///
/// Month is the plan's default reporting horizon, so this is the range every
/// target-versus-actual figure is measured over. Both ends are real local
/// midnights, which is what keeps a month honest across a DST transition inside
/// it — the month is not 30 × 24 hours, and pretending otherwise loses an hour
/// twice a year.
pub fn month_bounds(date: &str, tz: &Tz) -> Result<(Millis, Millis)> {
    let d = parse_date(date)?;
    let first = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).ok_or_else(|| AppError::invalid("That date is out of range."))?;
    let next = if d.month() == 12 {
        NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
    }
    .ok_or_else(|| AppError::invalid("That date is out of range."))?;
    Ok((day_start(first, tz), day_start(next, tz)))
}

pub fn now_ms() -> Millis {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tz() -> Tz {
        "Europe/London".parse().unwrap()
    }

    #[test]
    fn local_date_uses_the_zone_not_utc() {
        // 2025-07-30T23:30Z is already the 31st in Addis Ababa (UTC+3).
        let at = Utc
            .with_ymd_and_hms(2025, 7, 30, 23, 30, 0)
            .unwrap()
            .timestamp_millis();
        let addis: Tz = "Africa/Addis_Ababa".parse().unwrap();
        assert_eq!(local_date(at, &addis), "2025-07-31");
        assert_eq!(local_date(at, &Tz::UTC), "2025-07-30");
    }

    #[test]
    fn spring_forward_gap_resolves_forward() {
        // 2025-03-30 01:30 does not exist in London; the clock jumps 01:00→02:00.
        let naive = NaiveDate::from_ymd_opt(2025, 3, 30)
            .unwrap()
            .and_hms_opt(1, 30, 0)
            .unwrap();
        let ms = resolve_local(naive, &tz());
        let back = to_local(ms, &tz());
        assert_eq!(back.hour(), 2, "resolved forward out of the gap");
        assert_eq!(back.minute(), 0);
    }

    #[test]
    fn autumn_ambiguous_hour_takes_the_earlier_instant() {
        let naive = NaiveDate::from_ymd_opt(2025, 10, 26)
            .unwrap()
            .and_hms_opt(1, 30, 0)
            .unwrap();
        let ms = resolve_local(naive, &tz());
        // The earlier 01:30 is BST (UTC+1) → 00:30Z.
        assert_eq!(Utc.timestamp_millis_opt(ms).unwrap().hour(), 0);
    }

    #[test]
    fn day_length_follows_dst() {
        let spring = NaiveDate::from_ymd_opt(2025, 3, 30).unwrap();
        let normal = NaiveDate::from_ymd_opt(2025, 3, 31).unwrap();
        assert_eq!(day_end(spring, &tz()) - day_start(spring, &tz()), 23 * 3_600_000);
        assert_eq!(day_end(normal, &tz()) - day_start(normal, &tz()), 24 * 3_600_000);
    }

    #[test]
    fn blocks_may_not_cross_midnight() {
        let start = resolve_local(
            NaiveDate::from_ymd_opt(2025, 7, 30)
                .unwrap()
                .and_hms_opt(23, 0, 0)
                .unwrap(),
            &tz(),
        );
        assert!(same_day_span(start, 3600, &tz()).is_ok());
        assert!(matches!(
            same_day_span(start, 3601, &tz()),
            Err(AppError::CrossesMidnight)
        ));
    }

    #[test]
    fn a_23_hour_day_still_admits_a_block_ending_at_midnight() {
        let start = resolve_local(
            NaiveDate::from_ymd_opt(2025, 3, 30)
                .unwrap()
                .and_hms_opt(22, 0, 0)
                .unwrap(),
            &tz(),
        );
        assert_eq!(same_day_span(start, 2 * 3600, &tz()).unwrap(), "2025-03-30");
    }
}
