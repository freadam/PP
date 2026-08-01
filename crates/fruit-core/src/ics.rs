//! Reading a local `.ics` file (§2.3 PLAN — "Local `.ics` import (read-only)",
//! "External meetings, fully offline").
//!
//! Read-only in both directions of the phrase: Fruit parses a file you already
//! have on disk, and it never subscribes to a URL, never authenticates against
//! a calendar server, and never writes back. §1.4 rules out third-party
//! calendar *write* access, and the OFFLINE badge has to stay honest.
//!
//! The parser handles what real exports actually contain — folded lines,
//! escaped text, `TZID` parameters, `DTEND` or `DURATION`, `RRULE` — and
//! refuses the rest by counting it rather than guessing.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;

use crate::error::{AppError, Result};
use crate::time::{resolve_local, Millis};

/// One VEVENT, reduced to what a `scheduled_block` needs.
#[derive(Debug, Clone, PartialEq)]
pub struct IcsEvent {
    pub uid: String,
    pub summary: String,
    pub starts_at: Millis,
    pub duration_sec: i64,
    pub rrule: Option<String>,
}

/// What a file yielded, and — just as important — what it didn't.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct IcsParse {
    pub events: Vec<IcsEvent>,
    /// All-day events: a block is 5min–12h and same-day (§2.4), so a `VALUE=DATE`
    /// event has no honest representation here.
    pub skipped_all_day: u32,
    /// Events crossing local midnight, for the same reason.
    pub skipped_multi_day: u32,
    /// Events too short or too long for a block.
    pub skipped_out_of_range: u32,
    /// Events Fruit could not read at all.
    pub skipped_unreadable: u32,
}

impl IcsParse {
    pub fn skipped_total(&self) -> u32 {
        self.skipped_all_day
            + self.skipped_multi_day
            + self.skipped_out_of_range
            + self.skipped_unreadable
    }

    /// One sentence for the import summary. Silence about skipped events is how
    /// someone ends up believing their calendar imported cleanly when a third
    /// of it didn't.
    pub fn summary_line(&self) -> String {
        let mut parts = Vec::new();
        if self.skipped_all_day > 0 {
            parts.push(format!("{} all-day", self.skipped_all_day));
        }
        if self.skipped_multi_day > 0 {
            parts.push(format!("{} spanning midnight", self.skipped_multi_day));
        }
        if self.skipped_out_of_range > 0 {
            parts.push(format!("{} outside 5min–12h", self.skipped_out_of_range));
        }
        if self.skipped_unreadable > 0 {
            parts.push(format!("{} unreadable", self.skipped_unreadable));
        }
        match parts.len() {
            0 => format!("{} events.", self.events.len()),
            _ => format!("{} events. Skipped {}.", self.events.len(), parts.join(", ")),
        }
    }
}

const MIN_DURATION: i64 = 300;
const MAX_DURATION: i64 = 43_200;
/// Import is untrusted input (§7.3): cap before allocating anything large.
const MAX_LINES: usize = 2_000_000;
const MAX_EVENTS: usize = 20_000;

/// `fallback_tz` is used for floating times — a `DTSTART` with neither `Z` nor
/// a `TZID`, which RFC 5545 says means "local wherever you are".
pub fn parse(text: &str, fallback_tz: &Tz) -> Result<IcsParse> {
    let lines = unfold(text)?;
    let mut out = IcsParse::default();
    let mut current: Option<Vec<(String, String)>> = None;

    for line in lines {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("BEGIN:VEVENT") {
            current = Some(Vec::new());
            continue;
        }
        if upper.starts_with("END:VEVENT") {
            if let Some(props) = current.take() {
                if out.events.len() >= MAX_EVENTS {
                    return Err(AppError::Import(format!(
                        "That calendar has more than {MAX_EVENTS} events. Split it, or import a narrower range."
                    )));
                }
                match event_from(&props, fallback_tz) {
                    Ok(Some(event)) => out.events.push(event),
                    Ok(None) => {}
                    Err(Skip::AllDay) => out.skipped_all_day += 1,
                    Err(Skip::MultiDay) => out.skipped_multi_day += 1,
                    Err(Skip::OutOfRange) => out.skipped_out_of_range += 1,
                    Err(Skip::Unreadable) => out.skipped_unreadable += 1,
                }
            }
            continue;
        }
        if let Some(props) = current.as_mut() {
            if let Some((name, value)) = line.split_once(':') {
                props.push((name.to_string(), value.to_string()));
            }
        }
    }

    if out.events.is_empty() && out.skipped_total() == 0 {
        return Err(AppError::Import(
            "No events in that file. It should be a .ics calendar export containing VEVENTs."
                .into(),
        ));
    }
    Ok(out)
}

enum Skip {
    AllDay,
    MultiDay,
    OutOfRange,
    Unreadable,
}

fn event_from(props: &[(String, String)], fallback: &Tz) -> std::result::Result<Option<IcsEvent>, Skip> {
    let find = |name: &str| {
        props
            .iter()
            .find(|(k, _)| k.to_ascii_uppercase() == name || k.to_ascii_uppercase().starts_with(&format!("{name};")))
    };

    let (start_key, start_value) = find("DTSTART").ok_or(Skip::Unreadable)?;
    // `VALUE=DATE` marks an all-day event: a date with no time at all.
    if start_key.to_ascii_uppercase().contains("VALUE=DATE")
        && !start_key.to_ascii_uppercase().contains("DATE-TIME")
    {
        return Err(Skip::AllDay);
    }
    let (start_ms, start_naive, zone) =
        instant(start_key, start_value, fallback).ok_or(Skip::Unreadable)?;

    let duration_sec = if let Some((end_key, end_value)) = find("DTEND") {
        let (end_ms, _, _) = instant(end_key, end_value, fallback).ok_or(Skip::Unreadable)?;
        (end_ms - start_ms) / 1000
    } else if let Some((_, value)) = find("DURATION") {
        iso_duration(value).ok_or(Skip::Unreadable)?
    } else {
        // RFC 5545: a DTSTART with no end is a zero-length instant.
        return Err(Skip::OutOfRange);
    };

    if duration_sec < MIN_DURATION || duration_sec > MAX_DURATION {
        return Err(Skip::OutOfRange);
    }
    // Blocks never cross midnight (§2.4), and an event that does cannot be one.
    let end_naive = start_naive + chrono::Duration::seconds(duration_sec);
    if end_naive.date() != start_naive.date()
        && end_naive.time() != NaiveTime::from_hms_opt(0, 0, 0).unwrap()
    {
        return Err(Skip::MultiDay);
    }
    let _ = zone;

    let summary = find("SUMMARY")
        .map(|(_, v)| unescape(v))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Untitled event".to_string());

    let uid = find("UID")
        .map(|(_, v)| v.trim().to_string())
        .filter(|s| !s.is_empty())
        // No UID is legal-ish and common in hand-made files. Synthesising one
        // from the content keeps re-import idempotent, which is the only thing
        // the UID is used for here.
        .unwrap_or_else(|| format!("fruit-synth-{start_ms}-{}", summary.len()));

    let rrule = find("RRULE").map(|(_, v)| v.trim().to_string());

    Ok(Some(IcsEvent {
        uid,
        summary: summary.chars().take(500).collect(),
        starts_at: start_ms,
        duration_sec,
        rrule,
    }))
}

/// Resolves a DTSTART/DTEND, honouring `Z` (UTC), `TZID=` and floating times.
fn instant(key: &str, value: &str, fallback: &Tz) -> Option<(Millis, NaiveDateTime, Tz)> {
    let value = value.trim();
    let (date_part, time_part) = value.split_once('T')?;
    if date_part.len() != 8 || time_part.len() < 6 {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(
        date_part[0..4].parse().ok()?,
        date_part[4..6].parse().ok()?,
        date_part[6..8].parse().ok()?,
    )?;
    let time = NaiveTime::from_hms_opt(
        time_part[0..2].parse().ok()?,
        time_part[2..4].parse().ok()?,
        time_part[4..6].parse().ok()?,
    )?;
    let naive = date.and_time(time);

    if value.ends_with('Z') {
        return Some((naive.and_utc().timestamp_millis(), naive, chrono_tz::UTC));
    }
    let zone = key
        .to_ascii_uppercase()
        .split(';')
        .find_map(|p| p.strip_prefix("TZID=").map(|s| s.to_string()))
        .and_then(|name| {
            // TZID casing is not normalised by exporters, so match the IANA
            // name case-insensitively rather than failing on `EUROPE/LONDON`.
            chrono_tz::TZ_VARIANTS
                .iter()
                .find(|tz| tz.name().eq_ignore_ascii_case(&name))
                .copied()
        })
        .unwrap_or(*fallback);
    Some((resolve_local(naive, &zone), naive, zone))
}

/// `PT1H30M`, `P1DT2H`. Weeks and days included because exporters emit them.
fn iso_duration(value: &str) -> Option<i64> {
    let v = value.trim().to_ascii_uppercase();
    let v = v.strip_prefix('P')?;
    let (date_part, time_part) = match v.split_once('T') {
        Some((d, t)) => (d, t),
        None => (v, ""),
    };
    let mut total = 0i64;
    let mut num = String::new();
    for c in date_part.chars() {
        match c {
            '0'..='9' => num.push(c),
            'W' => {
                total += num.parse::<i64>().ok()? * 604_800;
                num.clear();
            }
            'D' => {
                total += num.parse::<i64>().ok()? * 86_400;
                num.clear();
            }
            _ => return None,
        }
    }
    num.clear();
    for c in time_part.chars() {
        match c {
            '0'..='9' => num.push(c),
            'H' => {
                total += num.parse::<i64>().ok()? * 3600;
                num.clear();
            }
            'M' => {
                total += num.parse::<i64>().ok()? * 60;
                num.clear();
            }
            'S' => {
                total += num.parse::<i64>().ok()?;
                num.clear();
            }
            _ => return None,
        }
    }
    Some(total)
}

/// RFC 5545 folds long lines by breaking them and prefixing the continuation
/// with a space or tab. Every real export does this, so unfolding is not
/// optional — an un-unfolded parser truncates every long SUMMARY.
fn unfold(text: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut count = 0usize;
    for raw in text.split('\n') {
        count += 1;
        if count > MAX_LINES {
            return Err(AppError::Import("That file is too large to import.".into()));
        }
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.push_str(&line[1..]);
                continue;
            }
        }
        out.push(line.to_string());
    }
    Ok(out)
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(',') => out.push(','),
            Some(';') => out.push(';'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use crate::time::to_local;

    fn london() -> Tz {
        "Europe/London".parse().unwrap()
    }

    const SAMPLE: &str = "\
BEGIN:VCALENDAR\r
VERSION:2.0\r
BEGIN:VEVENT\r
UID:abc-123\r
DTSTART;TZID=Europe/London:20250730T090000\r
DTEND;TZID=Europe/London:20250730T093000\r
SUMMARY:Stand-up\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:def-456\r
DTSTART:20250730T140000Z\r
DURATION:PT1H\r
SUMMARY:Design review with a very long title that the exporter had to f\r
 old across two lines\r
RRULE:FREQ=WEEKLY;BYDAY=WE\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:all-day\r
DTSTART;VALUE=DATE:20250731\r
DTEND;VALUE=DATE:20250801\r
SUMMARY:Company holiday\r
END:VEVENT\r
END:VCALENDAR\r
";

    #[test]
    fn reads_a_realistic_export() {
        let parsed = parse(SAMPLE, &london()).unwrap();
        assert_eq!(parsed.events.len(), 2);
        assert_eq!(parsed.skipped_all_day, 1);

        let standup = &parsed.events[0];
        assert_eq!(standup.summary, "Stand-up");
        assert_eq!(standup.duration_sec, 1800);
        assert_eq!(to_local(standup.starts_at, &london()).hour(), 9);

        let review = &parsed.events[1];
        assert_eq!(
            review.summary,
            "Design review with a very long title that the exporter had to fold across two lines",
            "folded lines are rejoined, not truncated"
        );
        assert_eq!(review.duration_sec, 3600);
        assert_eq!(review.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=WE"));
    }

    #[test]
    fn tzid_casing_does_not_matter() {
        let ics = "BEGIN:VEVENT\nUID:x\nDTSTART;TZID=EUROPE/LONDON:20250730T090000\n\
                   DTEND;TZID=EUROPE/LONDON:20250730T100000\nSUMMARY:x\nEND:VEVENT";
        let parsed = parse(ics, &chrono_tz::UTC).unwrap();
        assert_eq!(to_local(parsed.events[0].starts_at, &london()).hour(), 9);
    }

    #[test]
    fn a_floating_time_uses_the_fallback_zone() {
        let ics = "BEGIN:VEVENT\nUID:x\nDTSTART:20250730T090000\nDURATION:PT30M\n\
                   SUMMARY:Floating\nEND:VEVENT";
        let parsed = parse(ics, &london()).unwrap();
        assert_eq!(to_local(parsed.events[0].starts_at, &london()).hour(), 9);
    }

    #[test]
    fn events_a_block_cannot_represent_are_counted_not_dropped_silently() {
        let ics = "\
BEGIN:VEVENT\nUID:a\nDTSTART:20250730T230000Z\nDURATION:PT4H\nSUMMARY:Overnight\nEND:VEVENT
BEGIN:VEVENT\nUID:b\nDTSTART:20250730T090000Z\nDURATION:PT2M\nSUMMARY:Blink\nEND:VEVENT
BEGIN:VEVENT\nUID:c\nDTSTART:20250730T090000Z\nDURATION:PT20H\nSUMMARY:Marathon\nEND:VEVENT
BEGIN:VEVENT\nUID:d\nDTSTART:nonsense\nSUMMARY:Broken\nEND:VEVENT";
        let parsed = parse(ics, &chrono_tz::UTC).unwrap();
        assert!(parsed.events.is_empty());
        assert_eq!(parsed.skipped_multi_day, 1);
        assert_eq!(parsed.skipped_out_of_range, 2);
        assert_eq!(parsed.skipped_unreadable, 1);
        assert_eq!(
            parsed.summary_line(),
            "0 events. Skipped 1 spanning midnight, 2 outside 5min–12h, 1 unreadable."
        );
    }

    #[test]
    fn escaped_text_is_unescaped() {
        let ics = "BEGIN:VEVENT\nUID:x\nDTSTART:20250730T090000Z\nDURATION:PT30M\n\
                   SUMMARY:Budget\\, Q3\\; and hiring\nEND:VEVENT";
        let parsed = parse(ics, &chrono_tz::UTC).unwrap();
        assert_eq!(parsed.events[0].summary, "Budget, Q3; and hiring");
    }

    #[test]
    fn iso_durations() {
        assert_eq!(iso_duration("PT1H30M"), Some(5400));
        assert_eq!(iso_duration("PT45M"), Some(2700));
        assert_eq!(iso_duration("P1D"), Some(86_400));
        assert_eq!(iso_duration("PT30S"), Some(30));
        assert_eq!(iso_duration("garbage"), None);
    }

    #[test]
    fn a_file_that_is_not_a_calendar_is_refused_clearly() {
        let err = parse("just some text", &chrono_tz::UTC).unwrap_err();
        assert!(err.to_string().contains(".ics"), "{err}");
    }
}
