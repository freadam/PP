//! Activity (§3.5, P2) — "was I actually doing the thing the timer said?"
//!
//! Off by default, and the privacy contract is enforced here rather than only
//! described in the UI:
//!
//! - App-level tracking and window-title tracking are **separate** switches, and
//!   titles stay off even when apps are on.
//! - A per-app exclusion list, plus patterns that suppress titles.
//! - Retention of 30 / 90 days / forever, with an automatic purge and a
//!   next-purge date the user can see.
//! - Pause is a stored setting, so it survives a restart.
//!
//! The shell samples; this module decides what may be written. Putting the
//! filtering below the IPC boundary means a bug in the sampler cannot record
//! something the user excluded — the exclusion is applied on the way *in*, not
//! on the way out to the screen.

use rusqlite::params;
use serde_json::Value;

use super::Store;
use crate::error::{AppError, Result};
use crate::model::*;
use crate::time::{day_end, day_start, parse_date, zone, Millis};

pub const ENABLED: &str = "activity.enabled";
pub const TITLES_ENABLED: &str = "activity.titlesEnabled";
pub const PAUSED: &str = "activity.paused";
pub const EXCLUDED_APPS: &str = "activity.excludedApps";
pub const TITLE_PATTERNS: &str = "activity.excludedTitlePatterns";
pub const RETENTION_DAYS: &str = "activity.retentionDays";
pub const LAST_PURGE: &str = "activity.lastPurgeAt";

/// The contract between the shell's sampler and this module: a sample means
/// "this app was frontmost for the interval starting now". Without a nominal
/// length a lone sample would be a zero-duration span — present in the table,
/// invisible in every total, and worse than not recording it.
pub const SAMPLE_INTERVAL_MS: i64 = 20_000;

/// A sample arriving within this long of the previous span's end continues it.
/// Comfortably more than the interval, so an occasional late tick does not
/// shatter one stretch of work into a dozen rows.
const COALESCE_GAP_MS: i64 = 90_000;
/// A single span longer than this is a stuck sampler, not a work session.
const MAX_SPAN_SEC: i64 = 4 * 3600;

/// Longest first, ties broken by name.
///
/// The tie-break is not cosmetic: two apps *will* come out equal — a day split
/// evenly between an editor and a chat window is the most ordinary result this
/// feature produces — and without it the winner is decided by `HashMap`'s
/// per-process random seed. Which app "you were mostly in" would then change
/// between two reads of the same day.
fn ranked(totals: std::collections::HashMap<String, i64>) -> Vec<AppTotal> {
    let mut out: Vec<AppTotal> = totals
        .into_iter()
        .map(|(app_id, seconds)| AppTotal { app_id, seconds })
        .collect();
    out.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.app_id.cmp(&b.app_id)));
    out
}

impl Store {
    pub fn activity_settings(&self) -> Result<ActivitySettings> {
        let flag = |key: &str, default: bool| {
            matches!(self.get_setting(key), Ok(Some(Value::Bool(v))) if v) || (default && matches!(self.get_setting(key), Ok(None)))
        };
        let list = |key: &str| -> Vec<String> {
            match self.get_setting(key) {
                Ok(Some(Value::Array(items))) => items
                    .into_iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            }
        };
        let retention = match self.get_setting(RETENTION_DAYS) {
            Ok(Some(Value::Number(n))) => n.as_i64().unwrap_or(90),
            _ => 90,
        };
        Ok(ActivitySettings {
            enabled: flag(ENABLED, false),
            titles_enabled: flag(TITLES_ENABLED, false),
            // Its own switch, off even when apps are on: which *website* is a
            // materially bigger claim on someone's privacy than which
            // application. Same shape as titles, for the same reason.
            domains_enabled: flag(super::domain_rules::DOMAINS_ENABLED, false),
            paused: flag(PAUSED, false),
            excluded_apps: list(EXCLUDED_APPS),
            excluded_domains: list(super::domain_rules::EXCLUDED_DOMAINS),
            excluded_title_patterns: list(TITLE_PATTERNS),
            retention_days: retention,
            next_purge_at: self.next_purge_at(retention)?,
        })
    }

    fn next_purge_at(&self, retention_days: i64) -> Result<Option<Millis>> {
        if retention_days <= 0 {
            return Ok(None); // "forever" has no next purge
        }
        let last = match self.get_setting(LAST_PURGE) {
            Ok(Some(Value::Number(n))) => n.as_i64().unwrap_or(0),
            _ => 0,
        };
        Ok(Some(last.max(self.now()) + 24 * 3600 * 1000))
    }

    /// Records one observation. Returns false when the privacy contract
    /// suppressed it, so the caller can count what it dropped.
    ///
    /// Coalesces into the previous span when it is the same app and the gap is
    /// small, which is what keeps a day's worth of sampling to a few dozen rows.
    pub fn record_activity(&mut self, sample: ActivitySample) -> Result<bool> {
        let settings = self.activity_settings()?;
        if !settings.enabled || settings.paused {
            return Ok(false);
        }
        let app_id = sample.app_id.trim();
        if app_id.is_empty() || app_id.len() > 200 {
            return Ok(false);
        }
        // The exclusion list is applied on the way in: an excluded app is never
        // written, so it cannot leak later through export or a query.
        if settings
            .excluded_apps
            .iter()
            .any(|a| a.eq_ignore_ascii_case(app_id))
        {
            return Ok(false);
        }

        // A domain is stripped the way a title is: the surrounding record still
        // stands, only the more sensitive field is withheld. Doing it here and
        // not only in `record_browser_sample` means no future caller can write a
        // domain past the switch by taking a different route in.
        let domain = sample
            .domain
            .as_deref()
            .filter(|_| self.domains_enabled())
            .and_then(crate::connector::registrable_domain)
            .filter(|d| !self.domain_is_excluded(d));
        // Stamped now, from the rules in force now — never re-derived on read.
        // See migrations/0006_domain_rules.sql.
        let category = match &domain {
            Some(d) => self.classify_domain(d)?.map(|c| c.as_str()),
            None => None,
        };

        let title = if settings.titles_enabled {
            sample.window_title.as_deref().and_then(|t| {
                let hit = settings
                    .excluded_title_patterns
                    .iter()
                    .any(|p| !p.is_empty() && t.to_lowercase().contains(&p.to_lowercase()));
                if hit {
                    None
                } else {
                    Some(t.chars().take(300).collect::<String>())
                }
            })
        } else {
            None
        };

        let at = sample.at;
        let previous: Option<(i64, i64, String, Option<String>, Option<String>)> = self
            .conn
            .query_row(
                "SELECT id, ended_at, app_id, window_title, domain FROM activity_span
                  ORDER BY ended_at DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .ok();

        if let Some((id, ended_at, prev_app, prev_title, prev_domain)) = previous {
            // The domain is part of "same thing", not incidental to it. A
            // browser is one app, so without this ten minutes of YouTube and ten
            // of GitHub coalesce into one span labelled with whichever arrived
            // first — and the entertainment figure the product exists to reduce
            // would be measuring the wrong interval.
            let same = prev_app == app_id
                && prev_title.as_deref() == title.as_deref()
                && prev_domain.as_deref() == domain.as_deref();
            let contiguous = at >= ended_at && at - ended_at <= COALESCE_GAP_MS;
            let short_enough = (at - ended_at) / 1000 < MAX_SPAN_SEC;
            if same && contiguous && short_enough {
                self.conn.execute(
                    "UPDATE activity_span SET ended_at = ?2 WHERE id = ?1",
                    params![id, at + SAMPLE_INTERVAL_MS],
                )?;
                return Ok(true);
            }
        }

        self.conn.execute(
            "INSERT INTO activity_span
               (started_at, ended_at, app_id, window_title, domain, category, is_idle)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![at, at + SAMPLE_INTERVAL_MS, app_id, title, domain, category],
        )?;
        Ok(true)
    }

    /// The day timeline (§3.5), on the same time axis as the Planner so a
    /// scheduled block and the actual app usage under it read as one picture.
    pub fn get_activity_day(&self, date: &str, tz: &str) -> Result<ActivityDay> {
        let zone_ = zone(tz)?;
        let day = parse_date(date)?;
        let (from, to) = (day_start(day, &zone_), day_end(day, &zone_));

        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, app_id, window_title
               FROM activity_span
              WHERE started_at < ?2 AND ended_at >= ?1
              ORDER BY started_at",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok(ActivitySpanRow {
                id: r.get(0)?,
                // Clipped to the day, so a span running over midnight draws
                // inside the column instead of past it.
                started_at: r.get::<_, i64>(1)?.max(from),
                ended_at: r.get::<_, i64>(2)?.min(to),
                app_id: r.get(3)?,
                window_title: r.get(4)?,
            })
        })?;
        let spans: Vec<ActivitySpanRow> = rows.collect::<std::result::Result<_, _>>()?;

        // By app, longest first — the question is "where did the day go".
        let mut totals: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for s in &spans {
            *totals.entry(s.app_id.clone()).or_insert(0) += (s.ended_at - s.started_at) / 1000;
        }
        let by_app = ranked(totals);

        Ok(ActivityDay {
            local_date: date.to_string(),
            correlations: self.correlate(date, &spans)?,
            tracked_sec: spans.iter().map(|s| (s.ended_at - s.started_at) / 1000).sum(),
            spans,
            by_app,
        })
    }

    /// Activity correlation (§2.3 CALIBRATE, P2): for each plotted block, which
    /// apps were actually in front of you while it ran.
    ///
    /// This is the only place Fruit compares an intention against an
    /// *observation* rather than against its own record — which makes it the
    /// one report that can say "you were in Slack for the hour you plotted for
    /// the refactor", and the reason Activity is worth having at all.
    fn correlate(&self, date: &str, spans: &[ActivitySpanRow]) -> Result<Vec<BlockCorrelation>> {
        let mut out = Vec::new();
        for block in self.blocks_on(date)? {
            let start = block.starts_at;
            let end = start + block.duration_sec * 1000;
            let mut totals: std::collections::HashMap<String, i64> =
                std::collections::HashMap::new();
            for s in spans {
                let overlap = (s.ended_at.min(end) - s.started_at.max(start)) / 1000;
                if overlap > 0 {
                    *totals.entry(s.app_id.clone()).or_insert(0) += overlap;
                }
            }
            if totals.is_empty() {
                continue;
            }
            let mut apps = ranked(totals);
            let title = match &block.task_id {
                Some(task_id) => self
                    .conn
                    .query_row("SELECT title FROM task WHERE id = ?1", [task_id], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap_or_else(|_| "Untitled".into()),
                None => block.label.clone().unwrap_or_else(|| "Untitled".into()),
            };
            apps.truncate(5);
            out.push(BlockCorrelation {
                block_id: block.id,
                title,
                starts_at: start,
                duration_sec: block.duration_sec,
                top_apps: apps,
            });
        }
        Ok(out)
    }

    /// Automatic purge at the retention setting. `0` means forever.
    pub fn purge_activity(&mut self) -> Result<i64> {
        let settings = self.activity_settings()?;
        if settings.retention_days <= 0 {
            return Ok(0);
        }
        let cutoff = self.now() - settings.retention_days * 24 * 3600 * 1000;
        let removed = self
            .conn
            .execute("DELETE FROM activity_span WHERE ended_at < ?1", [cutoff])?;
        self.set_setting(LAST_PURGE, &Value::from(self.now()))?;
        Ok(removed as i64)
    }

    /// Deleting everything recorded so far. The privacy contract is worth
    /// nothing if you cannot get out of it.
    pub fn clear_activity(&mut self) -> Result<i64> {
        Ok(self.conn.execute("DELETE FROM activity_span", [])? as i64)
    }

    pub fn set_activity_setting(&mut self, key: &str, value: Value) -> Result<ActivitySettings> {
        // A closed allow-list: the renderer cannot invent an activity key and
        // have it silently persisted as a flag nothing reads.
        const ALLOWED: [&str; 8] = [
            ENABLED,
            TITLES_ENABLED,
            super::domain_rules::DOMAINS_ENABLED,
            PAUSED,
            EXCLUDED_APPS,
            super::domain_rules::EXCLUDED_DOMAINS,
            TITLE_PATTERNS,
            RETENTION_DAYS,
        ];
        if !ALLOWED.contains(&key) {
            return Err(AppError::invalid(format!("'{key}' isn't an Activity setting.")));
        }
        // Turning app tracking off turns titles and domains off with it. Leaving
        // either "on" underneath a disabled switch is how a re-enable surprises
        // someone.
        if key == ENABLED && value == Value::Bool(false) {
            self.set_setting(TITLES_ENABLED, &Value::Bool(false))?;
            self.set_setting(super::domain_rules::DOMAINS_ENABLED, &Value::Bool(false))?;
        }
        self.set_setting(key, &value)?;
        self.activity_settings()
    }
}
