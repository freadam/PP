//! Quick-capture grammar (§4.4).
//!
//! ```text
//! capture    := element*
//! element    := token | text
//! token      := project | tag | estimate | priority | due
//! project    := '#' ident
//! tag        := '@' ident
//! estimate   := '~' duration
//! duration   := int ('m'|'h') [ int 'm' ]
//! priority   := '!' | '!!' | '!!!'
//! due        := '^' ( date [ ws time ] | time )
//! ident      := [A-Za-z0-9_-]+ | '"' [^"]+ '"'
//! escape     := '\' sigil
//! ```
//!
//! Parsing lives in Rust rather than the renderer for three reasons: the
//! resolution rules are where the bugs are and they need tests; the same
//! grammar is reused by the estimate field and the command palette; and the
//! renderer is supposed to hold no business logic (§6.9).

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
use chrono_tz::Tz;

use crate::model::{CaptureChip, CapturePreview};
use crate::time::{format_date, local_date, resolve_local, to_local, Millis};

/// `d/m` vs `m/d` follows the OS locale (§4.4 rule 3). The shell resolves the
/// locale once and passes it down; ISO `YYYY-MM-DD` ignores this entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    DayMonth,
    MonthDay,
}

impl Default for DateOrder {
    fn default() -> Self {
        DateOrder::DayMonth
    }
}

pub struct ParseCtx<'a> {
    pub now: Millis,
    pub tz: Tz,
    pub order: DateOrder,
    /// Existing project names, lower-cased, with their ids — decides whether a
    /// `#chip` reads as "use" or "create" (rule 7).
    pub known_projects: &'a [(String, String)],
}

const SIGILS: [char; 5] = ['#', '@', '~', '!', '^'];

#[derive(Debug, Clone)]
enum Matched {
    Project(String),
    Tag(String),
    Estimate(i64),
    Priority(i64),
    DueDate(NaiveDate),
    DueAt(Millis),
}

pub fn parse(input: &str, ctx: &ParseCtx) -> CapturePreview {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;
    let mut title = String::new();
    let mut chips: Vec<CaptureChip> = Vec::new();
    let mut hits: Vec<(Matched, usize, usize)> = Vec::new();

    while i < chars.len() {
        let c = chars[i];

        // escape := '\' sigil  → the sigil survives as literal text.
        if c == '\\' && i + 1 < chars.len() && SIGILS.contains(&chars[i + 1]) {
            title.push(chars[i + 1]);
            i += 2;
            continue;
        }

        let at_token_start = i == 0 || chars[i - 1].is_whitespace();
        if at_token_start && SIGILS.contains(&c) {
            if let Some((m, end)) = match_token(&chars, i, ctx) {
                hits.push((m, i, end));
                i = end;
                // Swallow one trailing space so the title does not keep a
                // double gap where the token was (rule 8).
                if i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                continue;
            }
        }

        title.push(c);
        i += 1;
    }

    // Rule 6: last token of a kind wins. Walk in order and let later hits
    // overwrite earlier ones; tags accumulate because they are a set.
    let mut project: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut estimate: Option<i64> = None;
    let mut priority = 0i64;
    let mut due_date: Option<NaiveDate> = None;
    let mut due_at: Option<Millis> = None;

    for (m, from, to) in &hits {
        let raw: String = chars[*from..*to].iter().collect();
        let (kind, display, creates) = match m {
            Matched::Project(name) => {
                project = Some(name.clone());
                let exists = ctx
                    .known_projects
                    .iter()
                    .any(|(n, _)| n.eq_ignore_ascii_case(name));
                ("project", name.clone(), !exists)
            }
            Matched::Tag(name) => {
                if !tags.iter().any(|t| t.eq_ignore_ascii_case(name)) {
                    tags.push(name.clone());
                }
                ("tag", name.clone(), false)
            }
            Matched::Estimate(sec) => {
                estimate = Some(*sec);
                ("estimate", human_duration(*sec), false)
            }
            Matched::Priority(p) => {
                priority = *p;
                (
                    "priority",
                    match p {
                        3 => "high",
                        2 => "medium",
                        _ => "low",
                    }
                    .to_string(),
                    false,
                )
            }
            // Rule 4: a date sets due_date, a date-with-time sets due_at, never both.
            Matched::DueDate(d) => {
                due_date = Some(*d);
                due_at = None;
                ("due", format_date(*d), false)
            }
            Matched::DueAt(at) => {
                due_at = Some(*at);
                due_date = None;
                ("due", human_instant(*at, &ctx.tz), false)
            }
        };
        chips.push(CaptureChip {
            kind: kind.to_string(),
            raw,
            display,
            from: *from as u32,
            to: *to as u32,
            creates,
        });
    }

    let project_id = project.as_ref().and_then(|name| {
        ctx.known_projects
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, id)| id.clone())
    });
    let project_creates = project.is_some() && project_id.is_none();

    CapturePreview {
        raw: input.to_string(),
        title: collapse_whitespace(&title),
        project_name: project,
        project_id,
        project_creates,
        tags,
        estimate_sec: estimate,
        priority,
        due_date: due_date.map(format_date),
        due_at,
        chips,
    }
}

fn match_token(chars: &[char], start: usize, ctx: &ParseCtx) -> Option<(Matched, usize)> {
    match chars[start] {
        '#' => ident(chars, start + 1).map(|(name, end)| (Matched::Project(name), end)),
        '@' => ident(chars, start + 1).map(|(name, end)| (Matched::Tag(name), end)),
        '~' => duration(chars, start + 1).map(|(sec, end)| (Matched::Estimate(sec), end)),
        '!' => {
            let mut end = start;
            while end < chars.len() && chars[end] == '!' {
                end += 1;
            }
            let n = (end - start) as i64;
            // Only a bare run of 1–3 bangs, and nothing glued to the end.
            if n > 3 || chars.get(end).is_some_and(|c| !c.is_whitespace()) {
                return None;
            }
            Some((Matched::Priority(n), end))
        }
        '^' => due(chars, start + 1, ctx),
        _ => None,
    }
}

fn ident(chars: &[char], start: usize) -> Option<(String, usize)> {
    if start >= chars.len() {
        return None;
    }
    if chars[start] == '"' {
        let mut end = start + 1;
        let mut out = String::new();
        while end < chars.len() && chars[end] != '"' {
            out.push(chars[end]);
            end += 1;
        }
        if end >= chars.len() || out.trim().is_empty() {
            return None; // unterminated quote stays literal
        }
        return Some((out.trim().to_string(), end + 1));
    }
    let mut end = start;
    while end < chars.len() && (chars[end].is_ascii_alphanumeric() || matches!(chars[end], '_' | '-'))
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    Some((chars[start..end].iter().collect(), end))
}

fn take_int(chars: &[char], start: usize) -> Option<(i64, usize)> {
    let mut end = start;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if end == start || end - start > 6 {
        return None;
    }
    let n: i64 = chars[start..end].iter().collect::<String>().parse().ok()?;
    Some((n, end))
}

/// `duration := int ('m'|'h') [ int 'm' ]` — 30m · 2h · 1h30m.
fn duration(chars: &[char], start: usize) -> Option<(i64, usize)> {
    let (n, mut i) = take_int(chars, start)?;
    let unit = *chars.get(i)?;
    match unit {
        'm' | 'M' => Some((n * 60, i + 1)),
        'h' | 'H' => {
            i += 1;
            if let Some((mins, j)) = take_int(chars, i) {
                if matches!(chars.get(j), Some('m') | Some('M')) && mins < 60 {
                    return Some((n * 3600 + mins * 60, j + 1));
                }
            }
            Some((n * 3600, i))
        }
        _ => None,
    }
}

/// The estimate field in Task detail is free text (§1.5) and forgiving:
/// `45`, `45m`, `1.5h`, `1h 30m`, `2 h` all land.
pub fn parse_duration_loose(s: &str) -> Option<i64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    if let Ok(bare) = s.parse::<f64>() {
        return positive_seconds((bare * 60.0).round() as i64); // bare number = minutes
    }
    let mut total = 0f64;
    let mut num = String::new();
    let mut saw_unit = false;
    for c in s.chars() {
        match c {
            '0'..='9' | '.' => num.push(c),
            'h' => {
                total += num.parse::<f64>().ok()? * 3600.0;
                num.clear();
                saw_unit = true;
            }
            'm' => {
                total += num.parse::<f64>().ok()? * 60.0;
                num.clear();
                saw_unit = true;
            }
            ' ' => {}
            _ => return None,
        }
    }
    if !num.is_empty() {
        total += num.parse::<f64>().ok()? * 60.0;
        saw_unit = true;
    }
    if !saw_unit {
        return None;
    }
    positive_seconds(total.round() as i64)
}

fn positive_seconds(sec: i64) -> Option<i64> {
    (sec > 0 && sec <= 12 * 3600 * 30).then_some(sec)
}

fn due(chars: &[char], start: usize, ctx: &ParseCtx) -> Option<(Matched, usize)> {
    let today = to_local(ctx.now, &ctx.tz).date_naive();

    // Try `date [ws time]` first, then bare `time`.
    if let Some((date, after_date)) = date_token(chars, start, ctx, today) {
        // One space then a time is still the same token.
        if chars.get(after_date) == Some(&' ') {
            if let Some((t, after_time)) = time_token(chars, after_date + 1) {
                let resolved = resolve_weekday_with_time(date, t, chars, start, ctx, today);
                return Some((Matched::DueAt(resolved), after_time));
            }
        }
        // Rule 1: a weekday naming today with no time means *next* week.
        let date = bump_weekday_if_today(date, chars, start, today, None);
        return Some((Matched::DueDate(date), after_date));
    }

    if let Some((t, end)) = time_token(chars, start) {
        // Rule 2: bare time is today when still ahead, otherwise tomorrow.
        let candidate = resolve_local(today.and_time(t), &ctx.tz);
        let at = if candidate > ctx.now {
            candidate
        } else {
            resolve_local(today.succ_opt()?.and_time(t), &ctx.tz)
        };
        return Some((Matched::DueAt(at), end));
    }

    None
}

/// True when the `^token` names a weekday (as opposed to `today`, an ISO date…).
fn is_weekday_token(chars: &[char], start: usize) -> bool {
    let word: String = chars[start..]
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    weekday_from(&word.to_lowercase()).is_some()
}

fn bump_weekday_if_today(
    date: NaiveDate,
    chars: &[char],
    start: usize,
    today: NaiveDate,
    time_is_future: Option<bool>,
) -> NaiveDate {
    if date == today && is_weekday_token(chars, start) && time_is_future != Some(true) {
        return date + Duration::days(7);
    }
    date
}

fn resolve_weekday_with_time(
    date: NaiveDate,
    t: NaiveTime,
    chars: &[char],
    start: usize,
    ctx: &ParseCtx,
    today: NaiveDate,
) -> Millis {
    let first = resolve_local(date.and_time(t), &ctx.tz);
    let date = bump_weekday_if_today(date, chars, start, today, Some(first > ctx.now));
    resolve_local(date.and_time(t), &ctx.tz)
}

fn date_token(
    chars: &[char],
    start: usize,
    ctx: &ParseCtx,
    today: NaiveDate,
) -> Option<(NaiveDate, usize)> {
    // '+' int ('d'|'w')
    if chars.get(start) == Some(&'+') {
        let (n, i) = take_int(chars, start + 1)?;
        return match chars.get(i) {
            Some('d') | Some('D') => Some((today + Duration::days(n), i + 1)),
            Some('w') | Some('W') => Some((today + Duration::weeks(n), i + 1)),
            _ => None,
        };
    }

    // Alphabetic keywords and weekday names.
    let word: String = chars[start..]
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if !word.is_empty() {
        let end = start + word.len();
        let w = word.to_lowercase();
        let date = match w.as_str() {
            "today" | "tod" => Some(today),
            "tomorrow" | "tmr" | "tom" => Some(today + Duration::days(1)),
            _ => weekday_from(&w).map(|wd| next_weekday(today, wd)),
        };
        return date.map(|d| (d, end));
    }

    // Numeric: ISO `YYYY-MM-DD`, or `d/m` (`m/d` under a US locale).
    let (a, i) = take_int(chars, start)?;
    match chars.get(i) {
        Some('-') => {
            let (m, j) = take_int(chars, i + 1)?;
            if chars.get(j) != Some(&'-') {
                return None;
            }
            let (d, k) = take_int(chars, j + 1)?;
            NaiveDate::from_ymd_opt(a as i32, m as u32, d as u32).map(|date| (date, k))
        }
        Some('/') => {
            let (b, j) = take_int(chars, i + 1)?;
            let (day, month) = match ctx.order {
                DateOrder::DayMonth => (a, b),
                DateOrder::MonthDay => (b, a),
            };
            // A day/month pair with no year means the next occurrence.
            let this_year =
                NaiveDate::from_ymd_opt(today.year(), month as u32, day as u32)?;
            let date = if this_year < today {
                NaiveDate::from_ymd_opt(today.year() + 1, month as u32, day as u32)?
            } else {
                this_year
            };
            Some((date, j))
        }
        _ => None,
    }
}

fn weekday_from(w: &str) -> Option<Weekday> {
    Some(match w {
        "mon" | "monday" => Weekday::Mon,
        "tue" | "tues" | "tuesday" => Weekday::Tue,
        "wed" | "weds" | "wednesday" => Weekday::Wed,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thu,
        "fri" | "friday" => Weekday::Fri,
        "sat" | "saturday" => Weekday::Sat,
        "sun" | "sunday" => Weekday::Sun,
        _ => return None,
    })
}

/// The next occurrence, allowing today — rule 1 decides afterwards whether
/// today survives.
fn next_weekday(today: NaiveDate, wd: Weekday) -> NaiveDate {
    let delta = (wd.num_days_from_monday() as i64
        - today.weekday().num_days_from_monday() as i64)
        .rem_euclid(7);
    today + Duration::days(delta)
}

/// `time := int [':' int] [ 'am'|'pm' ]`
fn time_token(chars: &[char], start: usize) -> Option<(NaiveTime, usize)> {
    let (h, mut i) = take_int(chars, start)?;
    let mut m = 0i64;
    if chars.get(i) == Some(&':') {
        let (mm, j) = take_int(chars, i + 1)?;
        if mm > 59 {
            return None;
        }
        m = mm;
        i = j;
    }
    let mut hour = h;
    let suffix: String = chars[i..]
        .iter()
        .take(2)
        .collect::<String>()
        .to_lowercase();
    if suffix == "am" || suffix == "pm" {
        if !(1..=12).contains(&hour) {
            return None;
        }
        if suffix == "pm" && hour != 12 {
            hour += 12;
        }
        if suffix == "am" && hour == 12 {
            hour = 0;
        }
        i += 2;
    } else if hour > 23 {
        return None;
    }
    // A bare integer with no colon and no meridiem is only a time if it could
    // be an hour — `^9` is 09:00.
    NaiveTime::from_hms_opt(hour as u32, m as u32, 0).map(|t| (t, i))
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn human_duration(sec: i64) -> String {
    let h = sec / 3600;
    let m = (sec % 3600) / 60;
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

fn human_instant(at: Millis, tz: &Tz) -> String {
    let l = to_local(at, tz);
    format!("{} {:02}:{:02}", local_date(at, tz), l.hour(), l.minute())
}

#[allow(dead_code)]
fn debug_naive(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Tz;

    fn ctx_at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> (ParseCtx<'static>, Millis) {
        static PROJECTS: &[(String, String)] = &[];
        let tz: Tz = "Europe/London".parse().unwrap();
        let now = tz
            .with_ymd_and_hms(y, mo, d, h, mi, 0)
            .unwrap()
            .timestamp_millis();
        (
            ParseCtx {
                now,
                tz,
                order: DateOrder::DayMonth,
                known_projects: PROJECTS,
            },
            now,
        )
    }

    #[test]
    fn the_readme_example_parses() {
        // §3.9: the seeded task title *is* the parser documentation.
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("Read the 60-second guide #welcome ~15m !! ^today 17:00", &ctx);
        assert_eq!(p.title, "Read the 60-second guide");
        assert_eq!(p.project_name.as_deref(), Some("welcome"));
        assert_eq!(p.estimate_sec, Some(15 * 60));
        assert_eq!(p.priority, 2);
        assert!(p.due_at.is_some());
        assert_eq!(p.due_date, None, "rule 4: never both");
        assert!(p.project_creates, "rule 7: unknown project is create-on-confirm");
        assert_eq!(p.chips.len(), 4);
    }

    #[test]
    fn tokens_are_removed_and_whitespace_collapsed() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("Fix   login bug #work ~45m !!", &ctx);
        assert_eq!(p.title, "Fix login bug");
    }

    #[test]
    fn durations_cover_the_grammar() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        for (input, want) in [
            ("a ~30m", 1800),
            ("a ~2h", 7200),
            ("a ~1h30m", 5400),
            ("a ~90m", 5400),
        ] {
            assert_eq!(parse(input, &ctx).estimate_sec, Some(want), "{input}");
        }
    }

    #[test]
    fn rule_6_last_token_of_a_kind_wins() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("thing ~30m ~1h !!! !", &ctx);
        assert_eq!(p.estimate_sec, Some(3600));
        assert_eq!(p.priority, 1);
    }

    #[test]
    fn rule_1_weekday_naming_today_jumps_a_week() {
        // 2025-07-30 is a Wednesday.
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("thing ^wed", &ctx);
        assert_eq!(p.due_date.as_deref(), Some("2025-08-06"));

        // …unless it names today and the time is still ahead.
        let p = parse("thing ^wed 17:00", &ctx);
        assert_eq!(local_date(p.due_at.unwrap(), &ctx.tz), "2025-07-30");

        // A time already gone rolls to next week.
        let (late, _) = ctx_at(2025, 7, 30, 18, 0);
        let p = parse("thing ^wed 17:00", &late);
        assert_eq!(local_date(p.due_at.unwrap(), &late.tz), "2025-08-06");
    }

    #[test]
    fn rule_2_bare_time_is_today_then_tomorrow() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("thing ^17:00", &ctx);
        assert_eq!(local_date(p.due_at.unwrap(), &ctx.tz), "2025-07-30");

        let (late, _) = ctx_at(2025, 7, 30, 18, 0);
        let p = parse("thing ^17:00", &late);
        assert_eq!(local_date(p.due_at.unwrap(), &late.tz), "2025-07-31");
    }

    #[test]
    fn rule_3_slash_dates_follow_the_locale() {
        let tz: Tz = "Europe/London".parse().unwrap();
        let now = tz
            .with_ymd_and_hms(2025, 7, 30, 9, 0, 0)
            .unwrap()
            .timestamp_millis();
        let dmy = ParseCtx {
            now,
            tz,
            order: DateOrder::DayMonth,
            known_projects: &[],
        };
        let mdy = ParseCtx {
            order: DateOrder::MonthDay,
            ..dmy
        };
        assert_eq!(parse("x ^3/8", &dmy).due_date.as_deref(), Some("2025-08-03"));
        assert_eq!(parse("x ^3/8", &mdy).due_date.as_deref(), Some("2026-03-08"));
        // ISO is unambiguous under either.
        assert_eq!(
            parse("x ^2025-12-01", &mdy).due_date.as_deref(),
            Some("2025-12-01")
        );
    }

    #[test]
    fn rule_4_date_only_sets_due_date() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("x ^tomorrow", &ctx);
        assert_eq!(p.due_date.as_deref(), Some("2025-07-31"));
        assert!(p.due_at.is_none());
    }

    #[test]
    fn rule_5_dst_gap_resolves_forward() {
        // 2025-03-30 01:30 does not exist in London.
        let (ctx, _) = ctx_at(2025, 3, 20, 9, 0);
        let p = parse("x ^2025-03-30 01:30", &ctx);
        let at = p.due_at.expect("resolved rather than erroring");
        assert_eq!(to_local(at, &ctx.tz).hour(), 2);
    }

    #[test]
    fn rule_8_raw_is_retained_for_undo() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let raw = "Ship it #dev ~2h";
        assert_eq!(parse(raw, &ctx).raw, raw);
    }

    #[test]
    fn escapes_keep_sigils_literal() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse(r"Read \#hashtag docs \~approx", &ctx);
        assert_eq!(p.title, "Read #hashtag docs ~approx");
        assert!(p.project_name.is_none());
        assert!(p.estimate_sec.is_none());
    }

    #[test]
    fn unparseable_tokens_stay_literal() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("email bob@example.com about ~stuff and #!", &ctx);
        assert_eq!(p.title, "email bob@example.com about ~stuff and #!");
        assert!(p.chips.is_empty());
    }

    #[test]
    fn mid_word_sigils_are_not_tokens() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("Fix the bug! It's C#", &ctx);
        assert_eq!(p.priority, 0);
        assert_eq!(p.title, "Fix the bug! It's C#");
    }

    #[test]
    fn quoted_idents_allow_spaces() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("Plan #\"Deep Work\" @\"next week\"", &ctx);
        assert_eq!(p.project_name.as_deref(), Some("Deep Work"));
        assert_eq!(p.tags, vec!["next week".to_string()]);
        assert_eq!(p.title, "Plan");
    }

    #[test]
    fn known_project_does_not_offer_to_create() {
        let projects = vec![("Welcome to Fruit".to_string(), "p1".to_string())];
        let tz: Tz = "Europe/London".parse().unwrap();
        let ctx = ParseCtx {
            now: tz
                .with_ymd_and_hms(2025, 7, 30, 9, 0, 0)
                .unwrap()
                .timestamp_millis(),
            tz,
            order: DateOrder::DayMonth,
            known_projects: &projects,
        };
        let p = parse("x #\"welcome to fruit\"", &ctx);
        assert_eq!(p.project_id.as_deref(), Some("p1"));
        assert!(!p.project_creates);
    }

    #[test]
    fn relative_offsets() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        assert_eq!(parse("x ^+3d", &ctx).due_date.as_deref(), Some("2025-08-02"));
        assert_eq!(parse("x ^+2w", &ctx).due_date.as_deref(), Some("2025-08-13"));
    }

    #[test]
    fn meridiem_times() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let p = parse("x ^today 5pm", &ctx);
        assert_eq!(to_local(p.due_at.unwrap(), &ctx.tz).hour(), 17);
        let p = parse("x ^today 12am", &ctx);
        assert_eq!(to_local(p.due_at.unwrap(), &ctx.tz).hour(), 0);
    }

    #[test]
    fn loose_estimate_field() {
        assert_eq!(parse_duration_loose("45"), Some(2700));
        assert_eq!(parse_duration_loose("45m"), Some(2700));
        assert_eq!(parse_duration_loose("1.5h"), Some(5400));
        assert_eq!(parse_duration_loose("1h 30m"), Some(5400));
        assert_eq!(parse_duration_loose(""), None);
        assert_eq!(parse_duration_loose("soon"), None);
    }

    #[test]
    fn chips_carry_offsets_for_in_place_highlighting() {
        let (ctx, _) = ctx_at(2025, 7, 30, 9, 0);
        let input = "Ship it #dev";
        let p = parse(input, &ctx);
        let chip = &p.chips[0];
        assert_eq!(&input[chip.from as usize..chip.to as usize], "#dev");
    }
}
