//! Recurrence (§2.3 PLAN — "Recurring blocks (RRULE)", the v1 spec's mystery
//! `rec` column, specified properly).
//!
//! A deliberately small subset of RFC 5545. Fruit needs "every weekday at 9",
//! "every other Tuesday", "the 1st of the month" — it does not need `BYSETPOS`
//! or `BYYEARDAY`, and a full RRULE implementation is a library-sized problem
//! that would earn its place only if someone asked for it.
//!
//! ```text
//! FREQ=DAILY|WEEKLY|MONTHLY   required
//! INTERVAL=n                  default 1
//! BYDAY=MO,TU,WE,TH,FR        WEEKLY only
//! BYMONTHDAY=n                MONTHLY only, default: the day DTSTART falls on
//! COUNT=n | UNTIL=<utc>       optional; capped by the caller's horizon either way
//! ```
//!
//! Anything else in the string is rejected with a sentence rather than
//! silently ignored — a recurrence that quietly does something other than what
//! it says is worse than one that refuses.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone)]
pub struct Rrule {
    pub freq: Freq,
    pub interval: u32,
    /// WEEKLY only. Empty means "the weekday DTSTART falls on".
    pub by_day: Vec<Weekday>,
    /// MONTHLY only. `None` means "the day of the month DTSTART falls on".
    pub by_month_day: Option<u32>,
    pub count: Option<u32>,
    /// Inclusive last local date.
    pub until: Option<NaiveDate>,
}

/// Nothing is generated beyond this many instances in one expansion, whatever
/// the rule says. `FREQ=DAILY;COUNT=100000` is a typo, not a plan.
pub const MAX_INSTANCES: usize = 400;

impl Rrule {
    pub fn parse(text: &str) -> Result<Rrule> {
        let body = text.trim().strip_prefix("RRULE:").unwrap_or(text.trim());
        if body.is_empty() {
            return Err(AppError::invalid("An empty recurrence rule."));
        }

        let mut freq = None;
        let mut interval = 1u32;
        let mut by_day = Vec::new();
        let mut by_month_day = None;
        let mut count = None;
        let mut until = None;

        for part in body.split(';').filter(|p| !p.is_empty()) {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| AppError::invalid(format!("'{part}' isn't a NAME=VALUE pair.")))?;
            match key.to_ascii_uppercase().as_str() {
                "FREQ" => {
                    freq = Some(match value.to_ascii_uppercase().as_str() {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        other => {
                            return Err(AppError::invalid(format!(
                                "Fruit repeats daily, weekly or monthly — not {other}."
                            )))
                        }
                    })
                }
                "INTERVAL" => {
                    interval = value
                        .parse()
                        .map_err(|_| AppError::invalid("INTERVAL has to be a number."))?;
                    if interval == 0 || interval > 52 {
                        return Err(AppError::invalid("INTERVAL runs from 1 to 52."));
                    }
                }
                "BYDAY" => {
                    for day in value.split(',').filter(|d| !d.is_empty()) {
                        by_day.push(weekday(day)?);
                    }
                }
                "BYMONTHDAY" => {
                    let d: u32 = value
                        .parse()
                        .map_err(|_| AppError::invalid("BYMONTHDAY has to be a number."))?;
                    if !(1..=31).contains(&d) {
                        return Err(AppError::invalid("BYMONTHDAY runs from 1 to 31."));
                    }
                    by_month_day = Some(d);
                }
                "COUNT" => {
                    count = Some(
                        value
                            .parse()
                            .map_err(|_| AppError::invalid("COUNT has to be a number."))?,
                    )
                }
                "UNTIL" => until = Some(parse_until(value)?),
                // WKST only changes where a week boundary falls for BYSETPOS-style
                // rules, which this subset doesn't have. Accept and ignore it,
                // because real calendars emit it constantly.
                "WKST" => {}
                other => {
                    return Err(AppError::invalid(format!(
                        "Fruit doesn't understand {other} in a recurrence rule."
                    )))
                }
            }
        }

        let freq = freq.ok_or_else(|| AppError::invalid("A recurrence rule needs a FREQ."))?;
        if !by_day.is_empty() && freq != Freq::Weekly {
            return Err(AppError::invalid("BYDAY only applies to a weekly rule."));
        }
        if by_month_day.is_some() && freq != Freq::Monthly {
            return Err(AppError::invalid(
                "BYMONTHDAY only applies to a monthly rule.",
            ));
        }
        if count.is_some() && until.is_some() {
            return Err(AppError::invalid(
                "A rule ends with COUNT or UNTIL, not both.",
            ));
        }

        Ok(Rrule {
            freq,
            interval,
            by_day,
            by_month_day,
            count,
            until,
        })
    }

    /// The local dates this rule lands on, starting at `start` and stopping at
    /// whichever comes first: COUNT, UNTIL, `horizon`, or `MAX_INSTANCES`.
    ///
    /// Dates rather than instants: the *time* of day comes from the seed
    /// block's local wall clock, so a series stays at 09:00 across a DST
    /// boundary instead of sliding to 08:00 (§6.1 rule 2, the same reason
    /// `local_date` exists at all).
    pub fn dates(&self, start: NaiveDate, horizon: NaiveDate) -> Vec<NaiveDate> {
        let mut out = Vec::new();
        let limit = self.count.map(|c| c as usize).unwrap_or(MAX_INSTANCES);
        let stop = match self.until {
            Some(u) => u.min(horizon),
            None => horizon,
        };

        match self.freq {
            Freq::Daily => {
                let mut d = start;
                while d <= stop && out.len() < limit && out.len() < MAX_INSTANCES {
                    out.push(d);
                    d += Duration::days(self.interval as i64);
                }
            }
            Freq::Weekly => {
                let days = if self.by_day.is_empty() {
                    vec![start.weekday()]
                } else {
                    self.by_day.clone()
                };
                // Walk week by week from the week DTSTART falls in, so
                // INTERVAL=2 means "every other week", not "every 14 days from
                // whichever weekday happened to be first".
                let week0 = start - Duration::days(start.weekday().num_days_from_monday() as i64);
                let mut week = week0;
                while week <= stop && out.len() < limit && out.len() < MAX_INSTANCES {
                    for wd in &days {
                        let d = week + Duration::days(wd.num_days_from_monday() as i64);
                        if d >= start && d <= stop && out.len() < limit {
                            out.push(d);
                        }
                    }
                    week += Duration::weeks(self.interval as i64);
                }
                out.sort();
            }
            Freq::Monthly => {
                let day_of_month = self.by_month_day.unwrap_or_else(|| start.day());
                let (mut y, mut m) = (start.year(), start.month());
                while out.len() < limit && out.len() < MAX_INSTANCES {
                    // A 31st simply doesn't occur in a 30-day month. Skipping is
                    // what RFC 5545 says, and it beats silently moving it to the
                    // 30th, which would put a "monthly on the 31st" block on a
                    // different date than the user asked for.
                    if let Some(d) = NaiveDate::from_ymd_opt(y, m, day_of_month) {
                        if d > stop {
                            break;
                        }
                        if d >= start {
                            out.push(d);
                        }
                    }
                    let advance = self.interval;
                    let total = (m - 1) + advance;
                    y += (total / 12) as i32;
                    m = (total % 12) + 1;
                    if NaiveDate::from_ymd_opt(y, m, 1).is_none_or(|d| d > stop) {
                        break;
                    }
                }
            }
        }
        out.truncate(limit.min(MAX_INSTANCES));
        out
    }

    /// Plain language for the UI. A rule nobody can read is a rule nobody trusts.
    pub fn describe(&self) -> String {
        let every = match self.interval {
            1 => String::new(),
            2 => "other ".into(),
            n => format!("{n} "),
        };
        let base = match self.freq {
            Freq::Daily => format!("Every {every}day"),
            Freq::Weekly if self.by_day.is_empty() => format!("Every {every}week"),
            Freq::Weekly => {
                let names: Vec<&str> = self.by_day.iter().map(short_name).collect();
                if self.interval == 1 {
                    format!("Every {}", names.join(", "))
                } else {
                    format!("Every {every}week on {}", names.join(", "))
                }
            }
            Freq::Monthly => match self.by_month_day {
                Some(d) => format!("Every {every}month on the {}", ordinal(d)),
                None => format!("Every {every}month"),
            },
        };
        match (self.count, self.until) {
            (Some(n), _) => format!("{base}, {n} times"),
            (_, Some(u)) => format!("{base}, until {u}"),
            _ => base,
        }
    }
}

fn weekday(code: &str) -> Result<Weekday> {
    // A leading ordinal (`2MO` = second Monday) is BYSETPOS territory, which
    // this subset doesn't cover — say so rather than dropping the "2".
    let trimmed = code.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '+');
    if trimmed.len() != code.len() {
        return Err(AppError::invalid(
            "Fruit doesn't do 'the second Tuesday' yet — use a plain weekday.",
        ));
    }
    Ok(match code.to_ascii_uppercase().as_str() {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        other => return Err(AppError::invalid(format!("'{other}' isn't a weekday code."))),
    })
}

fn parse_until(value: &str) -> Result<NaiveDate> {
    let date_part = value.split('T').next().unwrap_or(value);
    if date_part.len() != 8 {
        return Err(AppError::invalid("UNTIL should look like 20250731T000000Z."));
    }
    let (y, m, d) = (
        date_part[0..4].parse::<i32>(),
        date_part[4..6].parse::<u32>(),
        date_part[6..8].parse::<u32>(),
    );
    match (y, m, d) {
        (Ok(y), Ok(m), Ok(d)) => NaiveDate::from_ymd_opt(y, m, d)
            .ok_or_else(|| AppError::invalid(format!("UNTIL names no real date: {value}"))),
        _ => Err(AppError::invalid(format!("Couldn't read UNTIL: {value}"))),
    }
}

fn short_name(w: &Weekday) -> &'static str {
    match w {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The presets the UI offers. Anything typed by hand still goes through `parse`.
pub const PRESETS: &[(&str, &str)] = &[
    ("Every day", "FREQ=DAILY"),
    ("Every weekday", "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"),
    ("Every week", "FREQ=WEEKLY"),
    ("Every other week", "FREQ=WEEKLY;INTERVAL=2"),
    ("Every month", "FREQ=MONTHLY"),
];

/// One preset, ready to render. The renderer never holds its own copy of this
/// list: a second list is a list that drifts, and the day it disagrees the UI
/// offers a rule the parser refuses.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RrulePreset {
    pub label: String,
    pub rrule: String,
    /// `describe()` of the parsed rule — the same sentence shown on a block.
    pub description: String,
}

pub fn presets() -> Vec<RrulePreset> {
    PRESETS
        .iter()
        .filter_map(|(label, rule)| {
            let parsed = Rrule::parse(rule).ok()?;
            Some(RrulePreset {
                label: (*label).to_string(),
                rrule: (*rule).to_string(),
                description: parsed.describe(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn daily_with_interval() {
        let r = Rrule::parse("FREQ=DAILY;INTERVAL=3").unwrap();
        let dates = r.dates(d(2025, 7, 30), d(2025, 8, 10));
        assert_eq!(
            dates,
            vec![d(2025, 7, 30), d(2025, 8, 2), d(2025, 8, 5), d(2025, 8, 8)]
        );
    }

    #[test]
    fn every_weekday() {
        // 2025-07-30 is a Wednesday.
        let r = Rrule::parse("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR").unwrap();
        let dates = r.dates(d(2025, 7, 30), d(2025, 8, 5));
        assert_eq!(
            dates,
            vec![
                d(2025, 7, 30), // Wed
                d(2025, 7, 31), // Thu
                d(2025, 8, 1),  // Fri
                d(2025, 8, 4),  // Mon
                d(2025, 8, 5),  // Tue
            ],
            "no weekend, and nothing before the start date"
        );
    }

    #[test]
    fn fortnightly_means_every_other_week_not_every_fourteen_days() {
        let r = Rrule::parse("FREQ=WEEKLY;INTERVAL=2;BYDAY=TU,TH").unwrap();
        let dates = r.dates(d(2025, 7, 29), d(2025, 8, 30));
        // Tue/Thu in the starting week, then skip a week, and so on.
        assert_eq!(
            dates,
            vec![
                d(2025, 7, 29),
                d(2025, 7, 31),
                d(2025, 8, 12),
                d(2025, 8, 14),
                d(2025, 8, 26),
                d(2025, 8, 28),
            ]
        );
    }

    #[test]
    fn monthly_skips_months_without_that_day() {
        let r = Rrule::parse("FREQ=MONTHLY;BYMONTHDAY=31").unwrap();
        let dates = r.dates(d(2025, 1, 31), d(2025, 6, 30));
        assert_eq!(
            dates,
            vec![d(2025, 1, 31), d(2025, 3, 31), d(2025, 5, 31)],
            "February, April and June have no 31st, so nothing is invented"
        );
    }

    #[test]
    fn count_and_until_bound_the_series() {
        let r = Rrule::parse("FREQ=DAILY;COUNT=3").unwrap();
        assert_eq!(r.dates(d(2025, 7, 30), d(2026, 1, 1)).len(), 3);

        let r = Rrule::parse("FREQ=DAILY;UNTIL=20250802T000000Z").unwrap();
        assert_eq!(r.dates(d(2025, 7, 30), d(2026, 1, 1)).len(), 4);
    }

    #[test]
    fn the_horizon_always_wins() {
        let r = Rrule::parse("FREQ=DAILY;COUNT=1000").unwrap();
        let dates = r.dates(d(2025, 7, 30), d(2025, 8, 3));
        assert_eq!(dates.len(), 5, "COUNT cannot outrun the requested horizon");
    }

    #[test]
    fn a_runaway_rule_is_capped() {
        let r = Rrule::parse("FREQ=DAILY;COUNT=100000").unwrap();
        assert_eq!(r.dates(d(2025, 1, 1), d(2030, 1, 1)).len(), MAX_INSTANCES);
    }

    #[test]
    fn unsupported_parts_are_refused_not_ignored() {
        // Silently dropping BYSETPOS would produce a series that does something
        // other than what the rule says.
        assert!(Rrule::parse("FREQ=MONTHLY;BYSETPOS=-1;BYDAY=FR").is_err());
        assert!(Rrule::parse("FREQ=YEARLY").is_err());
        assert!(Rrule::parse("FREQ=WEEKLY;BYDAY=2MO").is_err());
        assert!(Rrule::parse("FREQ=DAILY;COUNT=3;UNTIL=20250801T000000Z").is_err());
        assert!(Rrule::parse("").is_err());
        // …but WKST is everywhere in real calendars and changes nothing here.
        assert!(Rrule::parse("FREQ=WEEKLY;WKST=SU;BYDAY=MO").is_ok());
    }

    #[test]
    fn descriptions_are_readable() {
        assert_eq!(
            Rrule::parse("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR")
                .unwrap()
                .describe(),
            "Every Mon, Tue, Wed, Thu, Fri"
        );
        assert_eq!(
            Rrule::parse("FREQ=WEEKLY;INTERVAL=2").unwrap().describe(),
            "Every other week"
        );
        assert_eq!(
            Rrule::parse("FREQ=MONTHLY;BYMONTHDAY=1;COUNT=6")
                .unwrap()
                .describe(),
            "Every month on the 1st, 6 times"
        );
    }

    #[test]
    fn every_preset_parses() {
        for (label, rule) in PRESETS {
            let parsed = Rrule::parse(rule).unwrap_or_else(|e| panic!("{label}: {e}"));
            assert!(!parsed.describe().is_empty());
        }
    }
}
