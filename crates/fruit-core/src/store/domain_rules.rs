//! Domain rules — turning `youtube.com` into "entertainment" (Plan Rev 3 §5.4).
//!
//! This is the half of the browser connector that lives in the database. The
//! other half — the protocol — is in `crate::connector`, and the two meet in
//! [`Store::record_browser_sample`].
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

use rusqlite::{params, Row};


use super::Store;
use crate::connector::{registrable_domain, Sample};
use crate::error::{AppError, Result};
use crate::ids::new_id;
use crate::model::*;

/// Whether domains may be recorded at all. Off by default, and independent of
/// `activity.enabled` — see the module note.
pub const DOMAINS_ENABLED: &str = "activity.domainsEnabled";
/// Registrable domains that are never written.
pub const EXCLUDED_DOMAINS: &str = "activity.excludedDomains";

/// The rules that make the primary outcome measurable on first run.
///
/// Short on purpose. A long shipped list is a long list of small wrong guesses:
/// `reddit.com` is entertainment for most people and a work tool for some, and
/// being wrong about it teaches the user to distrust the whole classification.
/// These three are the ones the plan names, and the reconciler is how the rest
/// of the list gets built — by the person whose time it is.
pub const DEFAULT_RULES: &[(&str, DomainCategory)] = &[
    ("youtube.com", DomainCategory::Entertainment),
    ("youtu.be", DomainCategory::Entertainment),
    ("twitch.tv", DomainCategory::Entertainment),
];

fn map_rule(r: &Row) -> rusqlite::Result<DomainRuleRow> {
    let category: String = r.get(2)?;
    Ok(DomainRuleRow {
        id: r.get(0)?,
        domain: r.get(1)?,
        category: DomainCategory::parse(&category).unwrap_or(DomainCategory::Other),
        life_area_id: r.get(3)?,
        is_builtin: r.get::<_, i64>(4)? == 1,
    })
}

impl Store {
    /// Idempotent, and called on every launch for the same reason
    /// `seed_life_areas` is: a database that predates this migration would
    /// otherwise never acquire the defaults.
    ///
    /// Seeds only when the table is empty. A user who deletes the YouTube rule
    /// has said something, and re-adding it every launch would be arguing.
    pub fn seed_domain_rules(&mut self) -> Result<usize> {
        let existing: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM domain_rule", [], |r| r.get(0))?;
        if existing > 0 {
            return Ok(0);
        }
        let now = self.now();
        let tx = self.conn.transaction()?;
        for (domain, category) in DEFAULT_RULES {
            tx.execute(
                "INSERT INTO domain_rule
                   (id, domain, category, is_builtin, device_id, created_at, updated_at)
                 VALUES (?1,?2,?3,1,?4,?5,?5)",
                params![new_id(), domain, category.as_str(), self.device_id, now],
            )?;
        }
        tx.commit()?;
        Ok(DEFAULT_RULES.len())
    }

    pub fn list_domain_rules(&self) -> Result<Vec<DomainRuleRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, domain, category, life_area_id, is_builtin
               FROM domain_rule ORDER BY domain",
        )?;
        let rows = stmt.query_map([], map_rule)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Creates or replaces the rule for a domain.
    ///
    /// This is what the reconciler's "apply my choice to future activity in this
    /// context" calls: one decision, made once, that stops the same question
    /// being asked tomorrow. Editing a built-in rule makes it the user's — the
    /// row stops being built-in rather than being shadowed by a second one.
    pub fn set_domain_rule(
        &mut self,
        domain: &str,
        category: DomainCategory,
        life_area_id: Option<String>,
    ) -> Result<DomainRuleRow> {
        let domain = registrable_domain(domain)
            .ok_or_else(|| AppError::invalid(format!("'{domain}' isn't a website address.")))?;
        if let Some(area) = &life_area_id {
            let exists: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM life_area WHERE id = ?1 AND deleted_at IS NULL",
                [area],
                |r| r.get(0),
            )?;
            if exists == 0 {
                return Err(AppError::invalid("That life area no longer exists."));
            }
        }
        let now = self.now();
        self.conn.execute(
            "INSERT INTO domain_rule
               (id, domain, category, life_area_id, is_builtin, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,0,?5,?6,?6)
             ON CONFLICT(domain) DO UPDATE SET
               category = excluded.category,
               life_area_id = excluded.life_area_id,
               is_builtin = 0,
               updated_at = excluded.updated_at",
            params![
                new_id(),
                domain,
                category.as_str(),
                life_area_id,
                self.device_id,
                now
            ],
        )?;
        self.conn
            .query_row(
                "SELECT id, domain, category, life_area_id, is_builtin
                   FROM domain_rule WHERE domain = ?1",
                [&domain],
                map_rule,
            )
            .map_err(Into::into)
    }

    pub fn delete_domain_rule(&mut self, id: &str) -> Result<()> {
        let n = self
            .conn
            .execute("DELETE FROM domain_rule WHERE id = ?1", [id])?;
        if n == 0 {
            return Err(AppError::invalid("That rule no longer exists."));
        }
        Ok(())
    }

    /// The verdict for a domain, or `None` when no rule covers it.
    ///
    /// `None` is a real answer, not a failure: an unclassified domain shows up
    /// as observed-only time in the reconciler, which is exactly where the user
    /// gets asked what it was. Guessing a default here would replace a question
    /// with a wrong answer nobody is prompted to correct.
    pub fn classify_domain(&self, domain: &str) -> Result<Option<DomainCategory>> {
        let Some(domain) = registrable_domain(domain) else {
            return Ok(None);
        };
        let found: Option<String> = self
            .conn
            .query_row(
                "SELECT category FROM domain_rule WHERE domain = ?1",
                [&domain],
                |r| r.get(0),
            )
            .ok();
        Ok(found.as_deref().and_then(DomainCategory::parse))
    }

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
    /// longest first. The Day view's evidence column and the month dashboard's
    /// entertainment trend both read this.
    pub fn domain_totals(&self, date: &str, tz: &str) -> Result<Vec<DomainTotal>> {
        use crate::time::{day_end, day_start, parse_date, zone};
        let zone_ = zone(tz)?;
        let day = parse_date(date)?;
        let (from, to) = (day_start(day, &zone_), day_end(day, &zone_));

        let mut stmt = self.conn.prepare(
            // Clipped to the day on both ends, so a span running over midnight
            // is counted once in each day it actually occupied rather than
            // wholly in the one it started in.
            "SELECT domain,
                    category,
                    SUM(MIN(ended_at, ?2) - MAX(started_at, ?1)) / 1000
               FROM activity_span
              WHERE domain IS NOT NULL AND started_at < ?2 AND ended_at >= ?1
              GROUP BY domain, category
              ORDER BY 3 DESC, 1 ASC",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            let category: Option<String> = r.get(1)?;
            Ok(DomainTotal {
                domain: r.get(0)?,
                category: category.as_deref().and_then(DomainCategory::parse),
                seconds: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

/// Seconds spent on one domain, with the verdict recorded at the time.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainTotal {
    pub domain: String,
    pub category: Option<DomainCategory>,
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
        (s, clock)
    }

    /// Stamped with the clock the store is reading, because that is what the
    /// native-messaging host does before the sample reaches the spool — the app
    /// keeps that stamp rather than re-stamping on drain.
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

    #[test]
    fn defaults_classify_the_domains_the_plan_names() {
        let (s, _) = store();
        assert_eq!(
            s.classify_domain("www.youtube.com").unwrap(),
            Some(DomainCategory::Entertainment)
        );
        assert_eq!(
            s.classify_domain("youtu.be").unwrap(),
            Some(DomainCategory::Entertainment)
        );
        // Unknown is a question for the reconciler, not a guess.
        assert_eq!(s.classify_domain("example.com").unwrap(), None);
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
        assert!(spans(&s, "domain").is_empty(), "a domain was written with the switch off");
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
        assert!(!s.record_browser_sample(sample(&clock, "secure.bank.co.uk")).unwrap());
        clock.advance(SAMPLE_INTERVAL_MS);
        assert!(s.record_browser_sample(sample(&clock, "youtube.com")).unwrap());

        assert_eq!(spans(&s, "domain"), vec![Some("youtube.com".to_string())]);
    }

    #[test]
    fn a_recorded_sample_carries_the_verdict_in_force_when_it_was_written() {
        let (mut s, clock) = store();
        s.record_browser_sample(sample(&clock, "www.youtube.com")).unwrap();
        assert_eq!(spans(&s, "domain"), vec![Some("youtube.com".to_string())]);
        assert_eq!(
            spans(&s, "category"),
            vec![Some("entertainment".to_string())]
        );
    }

    /// The reason `activity_span.category` is stored rather than joined: a rule
    /// written today must not rewrite what last week said you were doing.
    #[test]
    fn changing_a_rule_does_not_reclassify_what_is_already_recorded() {
        let (mut s, clock) = store();
        s.record_browser_sample(sample(&clock, "youtube.com")).unwrap();
        s.set_domain_rule("youtube.com", DomainCategory::Core, None)
            .unwrap();
        // A new observation gets the new verdict...
        clock.advance(10 * 60_000);
        s.record_browser_sample(sample(&clock, "youtube.com")).unwrap();

        // ...and the old one keeps the one it was given.
        assert_eq!(
            spans(&s, "category"),
            vec![
                Some("entertainment".to_string()),
                Some("core".to_string())
            ]
        );
    }

    #[test]
    fn a_user_rule_replaces_the_builtin_rather_than_competing_with_it() {
        let (mut s, _) = store();
        let rule = s
            .set_domain_rule("www.youtube.com", DomainCategory::Core, None)
            .unwrap();
        assert_eq!(rule.domain, "youtube.com", "the rule is stored reduced");
        assert!(!rule.is_builtin, "an edited rule belongs to the user");

        let for_youtube: Vec<_> = s
            .list_domain_rules()
            .unwrap()
            .into_iter()
            .filter(|r| r.domain == "youtube.com")
            .collect();
        assert_eq!(for_youtube.len(), 1, "two rules would need a tie-break");
        assert_eq!(
            s.classify_domain("m.youtube.com").unwrap(),
            Some(DomainCategory::Core)
        );
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
        assert_eq!(spans(&s, "domain").len(), 1, "one stretch of watching is one row");

        // And it is as long as the watching was, not as long as one sample.
        let totals = s.domain_totals("2026-08-04", "UTC").unwrap();
        assert_eq!(totals[0].seconds, 5 * SAMPLE_INTERVAL_MS / 1000);
    }

    #[test]
    fn domain_totals_rank_by_time_and_carry_the_category() {
        let (mut s, clock) = store();
        for _ in 0..6 {
            s.record_browser_sample(sample(&clock, "youtube.com")).unwrap();
            clock.advance(SAMPLE_INTERVAL_MS);
        }
        for _ in 0..2 {
            s.record_browser_sample(sample(&clock, "github.com")).unwrap();
            clock.advance(SAMPLE_INTERVAL_MS);
        }

        let totals = s.domain_totals("2026-08-04", "UTC").unwrap();
        assert_eq!(totals.len(), 2);
        assert_eq!(totals[0].domain, "youtube.com");
        assert_eq!(totals[0].category, Some(DomainCategory::Entertainment));
        assert!(totals[0].seconds > totals[1].seconds);
        assert_eq!(totals[1].category, None, "github has no rule, and says so");
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
        assert_eq!(totals[0].domain, "youtube.com");
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

    #[test]
    fn a_rule_can_only_be_written_for_something_that_is_a_website() {
        let (mut s, _) = store();
        assert!(s
            .set_domain_rule("not a domain", DomainCategory::Other, None)
            .is_err());
        assert!(s
            .set_domain_rule("youtube.com", DomainCategory::Other, Some("nope".into()))
            .is_err());
    }
}
