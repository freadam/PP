//! The browser connector's plumbing (Plan Rev 3 §5.4).
//!
//! What may be recorded about a website, and how a queued sample becomes a row.
//! The *meaning* of a domain moved to `categories.rs` in migration 0007 — a rule
//! is no longer domain-only, because "Instagram is a distraction" is the same
//! statement whether Instagram is a website or an app.
//!
//! Three things are settled here.
//!
//! **Domains are off by default, separately from apps.** `activity.enabled`
//! turns on "which application was frontmost". Recording *which website* is a
//! materially bigger claim on someone's privacy, so it is its own switch, off
//! even when apps are on — the same shape as window titles, for the same reason.
//!
//! **Exclusions are applied on the way in.** An excluded domain is never
//! written, so it cannot surface later through the Day view, a report or an
//! export. `activity.excludedDomains` matches on the registrable domain, so
//! excluding `bank.co.uk` excludes every page of it and there is no subdomain
//! that slips past.
//!
//! **The verdict is stamped at write time.** See `0006_domain_rules.sql`: a rule
//! added today decides today's spans, and yesterday's keep what they were given.
//! Anything else means a user reclassifying a domain silently rewrites their own
//! history, which is the one thing a record of what you did must never do.

use super::Store;
use crate::connector::{registrable_domain, Sample};
use crate::error::Result;
use crate::model::*;

/// Whether domains may be recorded at all. Off by default, and independent of
/// `activity.enabled` — see the module note.
pub const DOMAINS_ENABLED: &str = "activity.domainsEnabled";
/// Registrable domains that are never written.
pub const EXCLUDED_DOMAINS: &str = "activity.excludedDomains";

impl Store {
    /// Whether the connector should be listening at all. The extension asks on
    /// connect, and the host answers honestly rather than accepting samples it
    /// intends to throw away.
    pub fn domains_enabled(&self) -> bool {
        let settings = match self.activity_settings() {
            Ok(s) => s,
            Err(_) => return false,
        };
        settings.enabled
            && !settings.paused
            && matches!(
                self.get_setting(DOMAINS_ENABLED),
                Ok(Some(serde_json::Value::Bool(true)))
            )
    }

    pub fn excluded_domains(&self) -> Vec<String> {
        match self.get_setting(EXCLUDED_DOMAINS) {
            Ok(Some(serde_json::Value::Array(items))) => items
                .into_iter()
                .filter_map(|v| v.as_str().and_then(registrable_domain))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Both sides reduced to a registrable domain before comparing, so excluding
    /// `bank.co.uk` excludes `secure.bank.co.uk` and there is no subdomain that
    /// slips past the list.
    pub(crate) fn domain_is_excluded(&self, domain: &str) -> bool {
        self.excluded_domains().iter().any(|d| d == domain)
    }

    /// The connector's entry point: one focused-tab observation becomes at most
    /// one span.
    ///
    /// Returns `false` for everything the privacy contract suppressed — the
    /// switch being off, an excluded domain, an unfocused tab, a non-website —
    /// so the caller can report "n dropped" rather than pretending it recorded.
    pub fn record_browser_sample(&mut self, sample: Sample) -> Result<bool> {
        if !self.domains_enabled() {
            return Ok(false);
        }
        // `accept` re-reduces the domain and drops unfocused tabs regardless of
        // what the extension claimed — the extension is the swappable part, and
        // this is where the promise is kept — and it drops a sample that waited
        // in the spool long enough to stop meaning anything. It keeps the host's
        // timestamp: see `Sample::accept`.
        let now = self.now();
        let Some(sample) = sample.accept(now) else {
            return Ok(false);
        };
        // The exclusion is checked here as well as in `record_activity`, and the
        // two do different jobs. There, an excluded domain is *stripped* and the
        // surrounding app record survives — the same treatment a title gets.
        // Here the domain is the whole observation, so a stripped one would
        // leave a bare "Chrome was frontmost" span that the connector had no
        // business writing and the foreground sampler will write anyway. An
        // excluded domain must cost a row, not gain one.
        if self.domain_is_excluded(&sample.domain) {
            return Ok(false);
        }
        self.record_activity(ActivitySample {
            app_id: sample.browser,
            window_title: None,
            domain: Some(sample.domain),
            at: sample.at,
        })
    }

    /// Drains the connector's spool and records what it held. Returns how many
    /// samples became observations, which is always ≤ what was drained: the
    /// switch, the exclusions and the focus flag all still apply here, at the
    /// write, and not at the host that queued them.
    ///
    /// Also republishes the sentinel, so the file on disk always agrees with the
    /// setting — a crash between "user turns domains off" and "sentinel removed"
    /// otherwise leaves the host writing to a spool nobody drains.
    pub fn drain_browser_spool(&mut self, dir: &std::path::Path) -> Result<usize> {
        let permitted = self.domains_enabled();
        crate::spool::set_spooling_permitted(dir, permitted)?;
        if !permitted {
            return Ok(0);
        }
        let mut recorded = 0;
        for sample in crate::spool::drain(dir)? {
            if self.record_browser_sample(sample)? {
                recorded += 1;
            }
        }
        Ok(recorded)
    }

    /// How much observed browser time each domain accounted for on a local date,
    /// longest first, with the label it was written with.
    ///
    /// Goes through `labelled_spans` rather than a `GROUP BY`, so the
    /// short-observation floor applies here exactly as it does on the Day view.
    /// A SQL aggregate would have been shorter and would have quietly disagreed
    /// with every other screen about the same afternoon.
    pub fn domain_totals(&self, date: &str, tz: &str) -> Result<Vec<DomainTotal>> {
        use std::collections::HashMap;
        let mut totals: HashMap<(String, Option<String>), i64> = HashMap::new();
        for span in self.labelled_spans(date, tz)? {
            let Some(domain) = span.domain.clone() else {
                continue;
            };
            *totals
                .entry((domain, span.category_id.clone()))
                .or_insert(0) += span.seconds();
        }

        let names: HashMap<String, (String, String)> = self
            .get_categories(None, tz)?
            .into_iter()
            .map(|c| (c.id, (c.name, c.colour)))
            .collect();

        let mut out: Vec<DomainTotal> = totals
            .into_iter()
            .map(|((domain, category_id), seconds)| {
                let meta = category_id.as_ref().and_then(|id| names.get(id));
                DomainTotal {
                    domain,
                    category_name: meta.map(|m| m.0.clone()),
                    category_colour: meta.map(|m| m.1.clone()),
                    category_id,
                    seconds,
                }
            })
            .collect();
        // Longest first, ties by name — without the tie-break the order comes
        // from a hash seed and changes between two reads of the same day.
        out.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.domain.cmp(&b.domain)));
        Ok(out)
    }
}

/// Seconds spent on one domain, with the label recorded at the time.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainTotal {
    pub domain: String,
    /// `None` when no rule covered it — a question for the labelling surface,
    /// not a failure.
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub category_colour: Option<String>,
    pub seconds: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::activity::SAMPLE_INTERVAL_MS;
    use crate::clock::TestClock;
    use serde_json::json;
    use std::sync::Arc;

    /// 2026-08-04, 09:00 UTC. A fixed clock rather than the wall clock, because
    /// `Sample::normalise` deliberately discards the browser's timestamp and
    /// stamps the host's — a test that passes its own `at` values is testing a
    /// path that does not exist in the running app.
    const NINE_AM: i64 = 1_785_834_000_000;

    fn store() -> (Store, TestClock) {
        let clock = TestClock::new(NINE_AM);
        let mut s = Store::in_memory_with_clock(Arc::new(clock.clone())).unwrap();
        s.set_activity_setting(super::super::activity::ENABLED, json!(true))
            .unwrap();
        s.set_setting(DOMAINS_ENABLED, &json!(true)).unwrap();
        // The floor exists to keep noise out of a day's account; these tests are
        // about the connector, and a 20-second sample is below it by design.
        s.set_setting(super::super::activity::MIN_SPAN_SEC, &json!(0))
            .unwrap();
        (s, clock)
    }

    fn sample(clock: &TestClock, domain: &str) -> Sample {
        Sample {
            domain: domain.into(),
            browser: "Chrome".into(),
            at: clock.now(),
            focused: true,
        }
    }

    fn spans(s: &Store, column: &str) -> Vec<Option<String>> {
        s.conn
            .prepare(&format!(
                "SELECT {column} FROM activity_span ORDER BY started_at, id"
            ))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    }

    /// The switch the Settings note promises. Apps on, domains off, and nothing
    /// about a website is written — not even a span with the domain stripped,
    /// because the connector's sample *is* the domain.
    #[test]
    fn domains_are_not_recorded_until_their_own_switch_is_on() {
        let clock = TestClock::new(NINE_AM);
        let mut s = Store::in_memory_with_clock(Arc::new(clock.clone())).unwrap();
        s.set_activity_setting(super::super::activity::ENABLED, json!(true))
            .unwrap();
        assert!(!s.domains_enabled());
        assert!(!s.record_browser_sample(sample(&clock, "youtube.com")).unwrap());
        assert!(
            spans(&s, "domain").is_empty(),
            "a domain was written with the switch off"
        );
    }

    /// Turning app tracking off must not leave the domain switch armed
    /// underneath it, ready to surprise someone who turns apps back on.
    #[test]
    fn turning_apps_off_turns_domains_off_with_them() {
        let (mut s, _) = store();
        assert!(s.domains_enabled());
        s.set_activity_setting(super::super::activity::ENABLED, json!(false))
            .unwrap();
        s.set_activity_setting(super::super::activity::ENABLED, json!(true))
            .unwrap();
        assert!(!s.domains_enabled(), "domains came back on by themselves");
    }

    #[test]
    fn an_excluded_domain_is_never_written() {
        let (mut s, clock) = store();
        s.set_setting(EXCLUDED_DOMAINS, &json!(["bank.co.uk"]))
            .unwrap();
        // Excluding the registrable domain excludes every subdomain of it —
        // there is no `secure.` that slips past.
        assert!(!s
            .record_browser_sample(sample(&clock, "secure.bank.co.uk"))
            .unwrap());
        clock.advance(SAMPLE_INTERVAL_MS);
        assert!(s.record_browser_sample(sample(&clock, "youtube.com")).unwrap());

        assert_eq!(spans(&s, "domain"), vec![Some("youtube.com".to_string())]);
    }

    /// Coalescing is app-based, and a browser is one app. Without the domain in
    /// the comparison, ten minutes of YouTube followed by ten of GitHub would
    /// merge into one span labelled with whichever came first — and the
    /// entertainment figure would be measuring the wrong interval.
    #[test]
    fn two_domains_in_one_browser_do_not_merge_into_one_span() {
        let (mut s, clock) = store();
        s.record_browser_sample(sample(&clock, "youtube.com")).unwrap();
        clock.advance(SAMPLE_INTERVAL_MS);
        s.record_browser_sample(sample(&clock, "github.com")).unwrap();

        assert_eq!(
            spans(&s, "domain"),
            vec![
                Some("youtube.com".to_string()),
                Some("github.com".to_string())
            ]
        );
    }

    #[test]
    fn a_continuous_stretch_on_one_domain_stays_one_span() {
        let (mut s, clock) = store();
        for _ in 0..5 {
            s.record_browser_sample(sample(&clock, "youtube.com")).unwrap();
            clock.advance(SAMPLE_INTERVAL_MS);
        }
        assert_eq!(
            spans(&s, "domain").len(),
            1,
            "one stretch of watching is one row"
        );
        let totals = s.domain_totals("2026-08-04", "UTC").unwrap();
        assert_eq!(totals[0].seconds, 5 * SAMPLE_INTERVAL_MS / 1000);
    }

    #[test]
    fn domain_totals_rank_by_time_and_carry_the_label() {
        let (mut s, clock) = store();
        for _ in 0..6 {
            s.record_browser_sample(sample(&clock, "youtube.com")).unwrap();
            clock.advance(SAMPLE_INTERVAL_MS);
        }
        for _ in 0..2 {
            s.record_browser_sample(sample(&clock, "notarule.example"))
                .unwrap();
            clock.advance(SAMPLE_INTERVAL_MS);
        }

        let totals = s.domain_totals("2026-08-04", "UTC").unwrap();
        assert_eq!(totals.len(), 2);
        assert_eq!(totals[0].domain, "youtube.com");
        assert_eq!(totals[0].category_name.as_deref(), Some("Distraction"));
        assert!(totals[0].seconds > totals[1].seconds);
        assert_eq!(
            totals[1].category_name, None,
            "an unlabelled site says so rather than being filed as Other"
        );
    }

    /// The whole hand-off, end to end: the host queues, the app drains, and the
    /// twenty seconds land where the browser actually spent them rather than
    /// where the app happened to notice.
    #[test]
    fn the_spool_delivers_what_the_host_queued_and_only_once() {
        let (mut s, clock) = store();
        let dir = tempfile::tempdir().unwrap();
        // Publishes the sentinel the host checks before writing anything.
        assert_eq!(s.drain_browser_spool(dir.path()).unwrap(), 0);

        for _ in 0..3 {
            let queued = sample(&clock, "youtube.com").normalise(clock.now()).unwrap();
            crate::spool::append(dir.path(), &queued).unwrap();
            clock.advance(SAMPLE_INTERVAL_MS);
        }

        assert_eq!(s.drain_browser_spool(dir.path()).unwrap(), 3);
        assert_eq!(s.drain_browser_spool(dir.path()).unwrap(), 0, "replayed");

        let totals = s.domain_totals("2026-08-04", "UTC").unwrap();
        assert_eq!(totals.len(), 1);
        assert_eq!(
            totals[0].seconds,
            3 * SAMPLE_INTERVAL_MS / 1000,
            "the stamps were collapsed onto the drain instant"
        );
    }

    /// Turning the switch off has to reach the *host*, which cannot read a
    /// setting — it reads a file the app publishes. Draining is where that file
    /// is kept in step, so it cannot drift after a crash.
    #[test]
    fn draining_publishes_whether_the_host_may_write_at_all() {
        let (mut s, _) = store();
        let dir = tempfile::tempdir().unwrap();
        s.drain_browser_spool(dir.path()).unwrap();
        assert!(crate::spool::spooling_permitted(dir.path()));

        s.set_activity_setting(DOMAINS_ENABLED, json!(false)).unwrap();
        s.drain_browser_spool(dir.path()).unwrap();
        assert!(!crate::spool::spooling_permitted(dir.path()));
    }
}
