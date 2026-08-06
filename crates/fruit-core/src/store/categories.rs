//! Labelling what the machine observed (migration 0007).
//!
//! Fruit records two kinds of observation: which application was frontmost, and
//! — through the browser connector — which website. This module is how either
//! one acquires a meaning. `chrome.exe` is not an answer to "what was I doing";
//! `instagram.com → Distraction` and `coursera.org → Study` are.
//!
//! Four decisions live here.
//!
//! # One rule table, not two
//!
//! "Instagram is a distraction" is the same statement whether Instagram is a
//! website or an app, and splitting it across a `domain_rule` and an `app_rule`
//! would mean two tables, two commands and two lists in Settings for one idea.
//! [`MatchKind`] is a column instead.
//!
//! # The domain wins over the app
//!
//! Chrome is one application and the whole point of the connector is that the
//! tab inside it is the thing that matters. So a rule on `instagram.com` beats a
//! rule on `chrome.exe`: the more specific claim wins, always, and there is no
//! configuration for it. The alternative — a browser labelled Work making every
//! site inside it work — is precisely the failure the connector was built to
//! fix.
//!
//! # The label is stamped at write time
//!
//! `activity_span.category_id` records the verdict in force when the span was
//! written, exactly as `category` has since 0006. A rule made today classifies
//! tomorrow. A rule cannot rewrite a month someone has already reconciled, and
//! that is the property with the acceptance test attached.
//!
//! # An unlabelled span stays NULL
//!
//! Not "Other". *Nobody has said* and *someone decided it was nothing in
//! particular* are different facts, and collapsing them would empty the
//! uncategorised list — which is the surface the whole feature is usable from.

use rusqlite::{params, Row};

use super::Store;
use crate::connector::registrable_domain;
use crate::error::{AppError, Result};
use crate::ids::{new_id, validate_id};
use crate::model::*;

/// Categories the schema depends on and therefore refuses to delete.
///
/// `Other` is where a user files something deliberately unremarkable, and the
/// uncategorised list would have nowhere to send it. `Work` is what "was this
/// work?" resolves against. Everything else — including Study, Distraction and
/// Life — is the user's to rename, recolour or remove.
const UNDELETABLE: [&str; 2] = [category::WORK, category::OTHER];

/// What a new category is coloured when the caller does not say.
///
/// Here rather than in the renderer for two reasons. A colour is a *stored
/// value*, so the component that happens to create the row is the wrong owner —
/// and `check-ui.mjs`'s I1 rule forbids literal colours in components precisely
/// so that a palette cannot fork between screens.
///
/// Cycled by how many categories already exist, so the first few additions are
/// visibly different from each other and from the built-ins.
/// How many individual stretches ride along with each unlabelled row.
///
/// Enough to cover a normal day's visits to one site; past that the aggregate is
/// the useful object and a scrolling list of forty rows is not. A day that
/// genuinely has more is a day for a rule, which is the row's own action.
const MAX_STRETCHES: usize = 12;

const NEW_CATEGORY_COLOURS: [&str; 6] = [
    "#8B7FD4", "#4FA374", "#C98A2E", "#5B8FC4", "#56C2D6", "#C2609B",
];

fn map_category(r: &Row) -> rusqlite::Result<ObservationCategory> {
    let counts_as: String = r.get(3)?;
    Ok(ObservationCategory {
        id: r.get(0)?,
        name: r.get(1)?,
        colour: r.get(2)?,
        counts_as: DomainCategory::parse(&counts_as).unwrap_or(DomainCategory::Other),
        is_builtin: r.get::<_, i64>(4)? == 1,
        sort_rank: r.get(5)?,
        seconds: 0,
    })
}

fn map_rule(r: &Row) -> rusqlite::Result<ActivityRuleRow> {
    let kind: String = r.get(1)?;
    Ok(ActivityRuleRow {
        id: r.get(0)?,
        match_kind: MatchKind::parse(&kind).unwrap_or(MatchKind::App),
        match_value: r.get(2)?,
        category_id: r.get(3)?,
        category_name: r.get(4)?,
        category_colour: r.get(5)?,
        is_builtin: r.get::<_, i64>(6)? == 1,
        backfilled: 0,
    })
}

/// Normalises whatever the user typed into something a rule can match.
///
/// A domain is reduced the same way the connector reduces it, so a rule typed
/// as `https://www.instagram.com/explore` matches the `instagram.com` that
/// actually gets stored. An app is lowercased and trimmed and nothing else —
/// executable names are already the shape they arrive in.
fn normalise_match(kind: MatchKind, value: &str) -> Result<String> {
    match kind {
        MatchKind::Domain => registrable_domain(value)
            .ok_or_else(|| AppError::invalid(format!("'{value}' isn't a website address."))),
        MatchKind::App => {
            let v = value.trim().to_lowercase();
            if v.is_empty() || v.len() > 200 {
                return Err(AppError::invalid("That isn't an application name."));
            }
            Ok(v)
        }
    }
}

impl Store {
    // ─── categories ────────────────────────────────────────────────────

    /// Every category, with observed seconds over an optional local-date range.
    ///
    /// The totals ride along rather than living in a second command because the
    /// list is never useful without them: "Distraction" on its own is a word,
    /// and "Distraction · 6h 20m" is the reason anyone opened the screen.
    pub fn get_categories(
        &self,
        range: Option<(&str, &str)>,
        tz: &str,
    ) -> Result<Vec<ObservationCategory>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, colour, counts_as, is_builtin, sort_rank
               FROM observation_category ORDER BY sort_rank, name",
        )?;
        let mut cats: Vec<ObservationCategory> =
            stmt.query_map([], map_category)?
                .collect::<std::result::Result<_, _>>()?;

        if let Some((from, to)) = range {
            let totals = self.category_seconds(from, to, tz)?;
            for c in &mut cats {
                c.seconds = totals.get(&c.id).copied().unwrap_or(0);
            }
        }
        Ok(cats)
    }

    fn category_seconds(
        &self,
        from: &str,
        to: &str,
        tz: &str,
    ) -> Result<std::collections::HashMap<String, i64>> {
        let mut out = std::collections::HashMap::new();
        for date in crate::time::date_series(from, to)? {
            for span in self.labelled_spans(&date, tz)? {
                if let Some(id) = &span.category_id {
                    *out.entry(id.clone()).or_insert(0) += (span.ended_at - span.started_at) / 1000;
                }
            }
        }
        Ok(out)
    }

    pub fn create_category(
        &mut self,
        name: &str,
        colour: Option<&str>,
        counts_as: DomainCategory,
    ) -> Result<ObservationCategory> {
        let name = name.trim();
        if name.is_empty() || name.len() > 60 {
            return Err(AppError::invalid("A category needs a name."));
        }
        let now = self.now();
        let existing: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM observation_category", [], |r| r.get(0))?;
        let colour = colour
            .unwrap_or(NEW_CATEGORY_COLOURS[existing as usize % NEW_CATEGORY_COLOURS.len()]);
        let rank: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_rank), 0) + 1 FROM observation_category",
                [],
                |r| r.get(0),
            )
            .unwrap_or(100.0);
        let id = new_id();
        self.conn
            .execute(
                "INSERT INTO observation_category
                   (id, name, colour, counts_as, is_builtin, sort_rank, device_id, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,0,?5,?6,?7,?7)",
                params![id, name, colour, counts_as.as_str(), rank, self.device_id, now],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
                    AppError::invalid(format!("A category called '{name}' already exists."))
                }
                other => other.into(),
            })?;
        self.category(&id)
    }

    /// Rename and recolour only.
    ///
    /// `counts_as` is deliberately not editable. It is the roll-up every
    /// existing report reads, so changing it would move totals on months that
    /// have already been reviewed and exported — the one thing this schema is
    /// arranged to prevent. Getting it wrong means deleting the category and
    /// making a new one, which is visible.
    pub fn update_category(
        &mut self,
        id: &str,
        name: Option<&str>,
        colour: Option<&str>,
    ) -> Result<ObservationCategory> {
        validate_id(id, "category")?;
        let now = self.now();
        if let Some(name) = name {
            let name = name.trim();
            if name.is_empty() || name.len() > 60 {
                return Err(AppError::invalid("A category needs a name."));
            }
            self.conn.execute(
                "UPDATE observation_category SET name = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, name, now],
            )?;
        }
        if let Some(colour) = colour {
            self.conn.execute(
                "UPDATE observation_category SET colour = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, colour, now],
            )?;
        }
        self.category(id)
    }

    /// Deletes a category and everything that pointed at it.
    ///
    /// Rules cascade. Spans do **not**: their `category_id` is set to NULL and
    /// their roll-up cleared, so the time reverts to unlabelled and reappears in
    /// the uncategorised list. Deleting a label must not delete the observation
    /// it was on — that would be a privacy feature turning into data loss.
    pub fn delete_category(&mut self, id: &str) -> Result<()> {
        validate_id(id, "category")?;
        if UNDELETABLE.contains(&id) {
            return Err(AppError::invalid(
                "Work and Other can be renamed but not deleted — everything unlabelled has to land somewhere.",
            ));
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE activity_span SET category_id = NULL, category = NULL WHERE category_id = ?1",
            [id],
        )?;
        let n = tx.execute("DELETE FROM observation_category WHERE id = ?1", [id])?;
        tx.commit()?;
        if n == 0 {
            return Err(AppError::invalid("That category no longer exists."));
        }
        Ok(())
    }

    fn category(&self, id: &str) -> Result<ObservationCategory> {
        self.conn
            .query_row(
                "SELECT id, name, colour, counts_as, is_builtin, sort_rank
                   FROM observation_category WHERE id = ?1",
                [id],
                map_category,
            )
            .map_err(Into::into)
    }

    // ─── rules ─────────────────────────────────────────────────────────

    pub fn get_activity_rules(&self) -> Result<Vec<ActivityRuleRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.id, r.match_kind, r.match_value, r.category_id, c.name, c.colour, r.is_builtin
               FROM activity_rule r
               JOIN observation_category c ON c.id = r.category_id
              ORDER BY r.match_kind, r.match_value",
        )?;
        let rows = stmt.query_map([], map_rule)?;
        let out: Vec<ActivityRuleRow> = rows.collect::<std::result::Result<_, _>>()?;
        Ok(out)
    }

    /// Creates or replaces the rule for one app or domain.
    ///
    /// This is what "label this as Study" calls, from Settings, from the Day
    /// view's detail panel and from the reconciler's *apply to future activity*
    /// checkbox. Editing a built-in makes it the user's — the row stops being
    /// built-in rather than being shadowed by a second one.
    pub fn set_activity_rule(
        &mut self,
        kind: MatchKind,
        value: &str,
        category_id: &str,
    ) -> Result<ActivityRuleRow> {
        validate_id(category_id, "category")?;
        let value = normalise_match(kind, value)?;
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM observation_category WHERE id = ?1",
            [category_id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(AppError::invalid("That category no longer exists."));
        }
        let now = self.now();
        self.conn.execute(
            "INSERT INTO activity_rule
               (id, match_kind, match_value, category_id, is_builtin, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,0,?5,?6,?6)
             ON CONFLICT(match_kind, match_value) DO UPDATE SET
               category_id = excluded.category_id,
               is_builtin = 0,
               updated_at = excluded.updated_at",
            params![new_id(), kind.as_str(), value, category_id, self.device_id, now],
        )?;
        let backfilled = self.fill_unlabelled(kind, &value, category_id)?;
        let mut row: ActivityRuleRow = self.conn.query_row(
            "SELECT r.id, r.match_kind, r.match_value, r.category_id, c.name, c.colour, r.is_builtin
               FROM activity_rule r
               JOIN observation_category c ON c.id = r.category_id
              WHERE r.match_kind = ?1 AND r.match_value = ?2",
            params![kind.as_str(), value],
            map_rule,
        )?;
        row.backfilled = backfilled;
        Ok(row)
    }

    /// Applies a new rule to observation that has **no label yet**.
    ///
    /// This is narrower than it first looks, and the narrowness is the point.
    ///
    /// The rule everything else here obeys is that a verdict is stamped at write
    /// time and a later rule cannot rewrite it — otherwise relabelling a domain
    /// in September silently changes what August said you were doing, and a month
    /// someone has reviewed and exported stops meaning anything.
    ///
    /// An **unlabelled** span has no verdict to protect. Filling it in is not
    /// rewriting history; it is answering a question that was left open. Nothing
    /// is overwritten, no reconciled day changes its story, and — the reason this
    /// exists — labelling something on the "not labelled yet" list makes it
    /// leave the list, which is the only behaviour a person would predict.
    ///
    /// `category` is written alongside `category_id` so the three-way roll-up
    /// every older report reads stays in step.
    fn fill_unlabelled(
        &self,
        kind: MatchKind,
        value: &str,
        category_id: &str,
    ) -> Result<i64> {
        let counts_as: String = self.conn.query_row(
            "SELECT counts_as FROM observation_category WHERE id = ?1",
            [category_id],
            |r| r.get(0),
        )?;
        let n = match kind {
            MatchKind::Domain => self.conn.execute(
                "UPDATE activity_span SET category_id = ?2, category = ?3
                  WHERE category_id IS NULL AND domain = ?1",
                params![value, category_id, counts_as],
            )?,
            // App rules must not claim a browser tab. A span carrying a domain
            // is a question about the *site*, and answering it with the
            // browser's own label is the precedence failure the connector was
            // built to fix — applied retroactively, which is worse.
            MatchKind::App => self.conn.execute(
                "UPDATE activity_span SET category_id = ?2, category = ?3
                  WHERE category_id IS NULL AND domain IS NULL
                    AND LOWER(app_id) = ?1",
                params![value, category_id, counts_as],
            )?,
        };
        Ok(n as i64)
    }

    pub fn delete_activity_rule(&mut self, id: &str) -> Result<()> {
        validate_id(id, "rule")?;
        let n = self
            .conn
            .execute("DELETE FROM activity_rule WHERE id = ?1", [id])?;
        if n == 0 {
            return Err(AppError::invalid("That rule no longer exists."));
        }
        Ok(())
    }

    /// The verdict for one observation, or `None` when nothing covers it.
    ///
    /// **The domain wins.** See the module note: a browser is one application
    /// and the tab inside it is the thing being asked about.
    pub fn classify(
        &self,
        app_id: &str,
        domain: Option<&str>,
    ) -> Result<Option<(String, DomainCategory)>> {
        if let Some(domain) = domain {
            if let Some(hit) = self.rule_for(MatchKind::Domain, domain)? {
                return Ok(Some(hit));
            }
        }
        self.rule_for(MatchKind::App, app_id)
    }

    fn rule_for(&self, kind: MatchKind, value: &str) -> Result<Option<(String, DomainCategory)>> {
        let Ok(value) = normalise_match(kind, value) else {
            return Ok(None);
        };
        let found: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT r.category_id, c.counts_as
                   FROM activity_rule r
                   JOIN observation_category c ON c.id = r.category_id
                  WHERE r.match_kind = ?1 AND r.match_value = ?2",
                params![kind.as_str(), value],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        Ok(found.and_then(|(id, counts)| DomainCategory::parse(&counts).map(|c| (id, c))))
    }

    // ─── relabelling one interval ──────────────────────────────────────

    /// Relabels a single observed span without touching any rule.
    ///
    /// The case the whole feature turns on: YouTube defaults to Distraction, and
    /// *this* video was a lecture. Fruit cannot tell the difference — it sees a
    /// registrable domain and deliberately never the URL or the page title — so
    /// the only honest design is to let the person who watched it say so, for
    /// that interval alone.
    ///
    /// Passing `None` returns the span to unlabelled, which puts it back on the
    /// uncategorised list rather than silently filing it as Other.
    pub fn set_span_category(&mut self, span_id: i64, category_id: Option<&str>) -> Result<()> {
        let counts_as = match category_id {
            Some(id) => {
                validate_id(id, "category")?;
                let found: Option<String> = self
                    .conn
                    .query_row(
                        "SELECT counts_as FROM observation_category WHERE id = ?1",
                        [id],
                        |r| r.get(0),
                    )
                    .ok();
                Some(found.ok_or_else(|| AppError::invalid("That category no longer exists."))?)
            }
            None => None,
        };
        let n = self.conn.execute(
            "UPDATE activity_span SET category_id = ?2, category = ?3 WHERE id = ?1",
            params![span_id, category_id, counts_as],
        )?;
        if n == 0 {
            return Err(AppError::invalid("That interval no longer exists."));
        }
        Ok(())
    }

    // ─── what has no label yet ─────────────────────────────────────────

    /// Observed apps and domains that no rule covers, ranked by time.
    ///
    /// The surface the labelling feature is actually usable from. An empty
    /// taxonomy plus a text field is a chore; "these four things took eleven
    /// hours between them and have no name" is a question people answer.
    ///
    /// Respects the short-span floor, so the list is not forty one-minute
    /// utilities ahead of the thing that actually ate the afternoon.
    pub fn get_unlabelled(&self, from: &str, to: &str, tz: &str, limit: u32) -> Result<Vec<UnlabelledRow>> {
        use std::collections::HashMap;
        let mut totals: HashMap<(MatchKind, String), (i64, i64, Vec<ObservedInterval>)> =
            HashMap::new();

        for date in crate::time::date_series(from, to)? {
            for span in self.labelled_spans(&date, tz)? {
                if span.category_id.is_some() || span.is_idle {
                    continue;
                }
                let seconds = span.seconds();
                // A browser with an unlabelled tab is a question about the
                // *site*, never about the browser. Attributing it to chrome.exe
                // would put one row at the top of this list forever and hide
                // every site under it.
                let key = match &span.domain {
                    Some(d) => (MatchKind::Domain, d.clone()),
                    None => (MatchKind::App, span.app_id.to_lowercase()),
                };
                let entry = totals.entry(key).or_insert((0, 0, Vec::new()));
                entry.0 += seconds;
                entry.1 += 1;
                entry.2.push(ObservedInterval {
                    id: span.id,
                    started_at: span.started_at,
                    ended_at: span.ended_at,
                    seconds,
                    app_id: span.app_id.clone(),
                    domain: span.domain.clone(),
                    window_title: span.window_title.clone(),
                    category_id: span.category_id.clone(),
                });
            }
        }

        let mut out: Vec<UnlabelledRow> = totals
            .into_iter()
            .map(
                |((match_kind, match_value), (seconds, occurrences, mut stretches))| {
                    // Longest first here too: the stretch worth naming is the
                    // one that took the afternoon, not the one that happened
                    // earliest.
                    stretches.sort_by(|a, b| {
                        b.seconds.cmp(&a.seconds).then_with(|| a.started_at.cmp(&b.started_at))
                    });
                    stretches.truncate(MAX_STRETCHES);
                    UnlabelledRow {
                        match_kind,
                        match_value,
                        seconds,
                        occurrences,
                        stretches,
                    }
                },
            )
            .collect();
        // Longest first, ties broken by name — the same reason `ranked()` does
        // it in activity.rs: without a tie-break the order comes from a hash
        // seed and changes between two reads of the same day.
        out.sort_by(|a, b| {
            b.seconds
                .cmp(&a.seconds)
                .then_with(|| a.match_value.cmp(&b.match_value))
        });
        out.truncate(limit as usize);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::activity::{ENABLED, MIN_SPAN_SEC, SAMPLE_INTERVAL_MS};
    use crate::clock::TestClock;
    use serde_json::json;
    use std::sync::Arc;

    /// 2026-08-04, 09:00 UTC.
    const NINE_AM: i64 = 1_785_834_000_000;
    const DATE: &str = "2026-08-04";

    fn store() -> (Store, TestClock) {
        let clock = TestClock::new(NINE_AM);
        let mut s = Store::in_memory_with_clock(Arc::new(clock.clone())).unwrap();
        s.set_activity_setting(ENABLED, json!(true)).unwrap();
        s.set_activity_setting("activity.domainsEnabled", json!(true))
            .unwrap();
        (s, clock)
    }

    /// Writes `minutes` of observation starting at the clock, and leaves the
    /// clock where it ended — the same shape the shell's 20-second loop produces.
    fn observe(s: &mut Store, clock: &TestClock, app: &str, domain: Option<&str>, minutes: i64) {
        for _ in 0..(minutes * 60_000 / SAMPLE_INTERVAL_MS) {
            s.record_activity(ActivitySample {
                app_id: app.into(),
                window_title: None,
                domain: domain.map(str::to_string),
                at: clock.now(),
            })
            .unwrap();
            clock.advance(SAMPLE_INTERVAL_MS);
        }
    }

    fn named(s: &Store, name: &str) -> String {
        s.get_categories(None, "UTC")
            .unwrap()
            .into_iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no category called {name}"))
            .id
    }

    fn totals(s: &Store) -> Vec<(String, i64)> {
        s.get_categories(Some((DATE, DATE)), "UTC")
            .unwrap()
            .into_iter()
            .filter(|c| c.seconds > 0)
            .map(|c| (c.name, c.seconds))
            .collect()
    }

    /// The four labels the product now ships with, plus the bucket for
    /// "deliberately nothing in particular".
    #[test]
    fn the_seeded_categories_are_the_ones_a_person_asked_for() {
        let (s, _) = store();
        let names: Vec<String> = s
            .get_categories(None, "UTC")
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["Work", "Study", "Distraction", "Life", "Other"]);
    }

    /// The headline: the same browser, four sites, four different answers.
    /// Before this, all of it was `chrome.exe`.
    #[test]
    fn one_browser_four_sites_four_labels() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("instagram.com"), 10);
        observe(&mut s, &clock, "chrome.exe", Some("coursera.org"), 20);
        observe(&mut s, &clock, "chrome.exe", Some("github.com"), 30);
        observe(&mut s, &clock, "chrome.exe", Some("youtube.com"), 15);

        assert_eq!(
            totals(&s),
            [
                ("Work".to_string(), 30 * 60),
                ("Study".to_string(), 20 * 60),
                // Instagram and YouTube, both distraction, summed.
                ("Distraction".to_string(), 25 * 60),
            ]
        );
    }

    /// The specific case: YouTube defaults to Distraction, and this one was a
    /// lecture. Fruit cannot see the video — no URL, no title — so the only
    /// honest design is to let the person who watched it say so, **for that
    /// interval alone**, without touching the rule.
    #[test]
    fn a_youtube_lecture_can_be_relabelled_without_changing_the_rule() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("youtube.com"), 40);
        assert_eq!(totals(&s), [("Distraction".to_string(), 40 * 60)]);

        let span_id: i64 = s
            .conn
            .query_row("SELECT id FROM activity_span LIMIT 1", [], |r| r.get(0))
            .unwrap();
        s.set_span_category(span_id, Some(&named(&s, "Study"))).unwrap();
        assert_eq!(totals(&s), [("Study".to_string(), 40 * 60)]);

        // The rule is untouched, so tomorrow's YouTube is still Distraction —
        // relabelling one video must not silently retrain the classifier.
        clock.advance(24 * 3600 * 1000);
        assert_eq!(
            s.classify("chrome.exe", Some("youtube.com"))
                .unwrap()
                .map(|(id, _)| id),
            Some(named(&s, "Distraction"))
        );
    }

    /// The rule that has no configuration, because getting it wrong defeats the
    /// connector entirely: a browser labelled Work would make every site inside
    /// it work.
    #[test]
    fn the_site_beats_the_browser_it_was_open_in() {
        let (mut s, _) = store();
        s.set_activity_rule(MatchKind::App, "chrome.exe", &named(&s, "Work"))
            .unwrap();

        assert_eq!(
            s.classify("chrome.exe", Some("instagram.com"))
                .unwrap()
                .map(|(id, _)| id),
            Some(named(&s, "Distraction")),
            "the app rule won, which is the failure the connector exists to fix"
        );
        // With no site in view the app rule is all there is, and it applies.
        assert_eq!(
            s.classify("chrome.exe", None).unwrap().map(|(id, _)| id),
            Some(named(&s, "Work"))
        );
    }

    /// Labelling is not domain-only: an application is a thing you spend hours
    /// in too, and the user asked for both.
    #[test]
    fn an_application_can_be_labelled_as_well_as_a_website() {
        let (mut s, clock) = store();
        s.set_activity_rule(MatchKind::App, "Code.exe", &named(&s, "Work"))
            .unwrap();
        s.set_activity_rule(MatchKind::App, "steam.exe", &named(&s, "Distraction"))
            .unwrap();

        observe(&mut s, &clock, "Code.exe", None, 45);
        observe(&mut s, &clock, "steam.exe", None, 25);

        assert_eq!(
            totals(&s),
            [
                ("Work".to_string(), 45 * 60),
                ("Distraction".to_string(), 25 * 60)
            ]
        );
    }

    /// Case and shape are normalised on both sides, so a rule typed as a URL
    /// still matches the reduced domain that actually gets stored.
    #[test]
    fn a_rule_matches_however_it_was_typed() {
        let (mut s, _) = store();
        let study = named(&s, "Study");
        let rule = s
            .set_activity_rule(MatchKind::Domain, "https://www.Coursera.org/learn/x", &study)
            .unwrap();
        assert_eq!(rule.match_value, "coursera.org");

        s.set_activity_rule(MatchKind::App, "  Code.EXE ", &named(&s, "Work"))
            .unwrap();
        assert_eq!(
            s.classify("code.exe", None).unwrap().map(|(id, _)| id),
            Some(named(&s, "Work"))
        );
    }

    /// The narrow, correct version of "a rule cannot rewrite history".
    ///
    /// A rule fills in observation that has **no label yet**, whatever its date.
    /// That is not rewriting: nothing is overwritten, no reconciled day changes
    /// its story, and — the reason this matters — labelling something on the
    /// "not labelled yet" list makes it leave the list, which is the only
    /// behaviour anyone would predict.
    #[test]
    fn a_rule_fills_in_what_had_no_label_and_says_how_much() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("news.example"), 30);
        assert!(totals(&s).is_empty(), "unlabelled, and honestly so");
        assert_eq!(s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap().len(), 1);

        let rule = s
            .set_activity_rule(MatchKind::Domain, "news.example", &named(&s, "Distraction"))
            .unwrap();
        assert_eq!(rule.backfilled, 1, "the app has to be able to say so");
        assert_eq!(totals(&s), [("Distraction".to_string(), 30 * 60)]);
        assert!(
            s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap().is_empty(),
            "labelling it left it on the list of things needing a label"
        );
    }

    /// And the half that is genuinely protected: a span that already carries a
    /// verdict keeps it. Relabelling a domain in September must not change what
    /// August said you were doing, or a month someone reviewed and exported
    /// stops meaning anything.
    #[test]
    fn a_new_rule_never_moves_a_label_that_is_already_there() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("youtube.com"), 30);
        assert_eq!(totals(&s), [("Distraction".to_string(), 30 * 60)]);

        let rule = s
            .set_activity_rule(MatchKind::Domain, "youtube.com", &named(&s, "Study"))
            .unwrap();
        assert_eq!(rule.backfilled, 0, "there was nothing unlabelled to fill");
        assert_eq!(
            totals(&s),
            [("Distraction".to_string(), 30 * 60)],
            "the rule reached backwards into a day already lived"
        );

        // Forwards, it is Study.
        clock.advance(10 * 60_000);
        observe(&mut s, &clock, "chrome.exe", Some("youtube.com"), 10);
        assert_eq!(
            totals(&s),
            [
                ("Study".to_string(), 10 * 60),
                ("Distraction".to_string(), 30 * 60)
            ]
        );
    }

    /// An app rule must not claim a browser tab, retroactively least of all.
    /// A span carrying a domain is a question about the *site*, and answering it
    /// with the browser's own label is the precedence failure the connector was
    /// built to fix.
    #[test]
    fn labelling_the_browser_does_not_backfill_its_tabs() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("news.example"), 30);
        clock.advance(10 * 60_000);
        observe(&mut s, &clock, "chrome.exe", None, 20);

        let rule = s
            .set_activity_rule(MatchKind::App, "chrome.exe", &named(&s, "Work"))
            .unwrap();
        assert_eq!(rule.backfilled, 1, "the app-only stretch, and not the tab");
        assert_eq!(totals(&s), [("Work".to_string(), 20 * 60)]);

        let left = s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].match_value, "news.example");
    }

    /// The surface the whole feature is usable from. An empty taxonomy plus a
    /// text field is a chore; a ranked list of what actually ate the day is a
    /// question people answer.
    #[test]
    fn the_unlabelled_list_ranks_by_time_and_names_the_site_not_the_browser() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("news.example"), 40);
        observe(&mut s, &clock, "chrome.exe", Some("wiki.example"), 12);
        observe(&mut s, &clock, "someapp.exe", None, 25);
        // Labelled, so it must not appear.
        observe(&mut s, &clock, "chrome.exe", Some("github.com"), 60);

        let rows = s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| (r.match_kind, r.match_value.as_str(), r.seconds))
                .collect::<Vec<_>>(),
            [
                (MatchKind::Domain, "news.example", 40 * 60),
                (MatchKind::App, "someapp.exe", 25 * 60),
                (MatchKind::Domain, "wiki.example", 12 * 60),
            ],
            "a browser with an unlabelled tab is a question about the site"
        );
    }

    /// The list is not only a total: each stretch comes with it, so a visit can
    /// be labelled on its own without first committing to a rule about the site.
    #[test]
    fn every_stretch_comes_with_the_row_so_each_can_be_labelled_alone() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("news.example"), 30);
        clock.advance(20 * 60_000);
        observe(&mut s, &clock, "chrome.exe", Some("news.example"), 12);
        clock.advance(20 * 60_000);
        observe(&mut s, &clock, "chrome.exe", Some("news.example"), 45);

        let rows = s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap();
        assert_eq!(rows.len(), 1, "one row for the site");
        assert_eq!(rows[0].occurrences, 3);
        // Longest first — the stretch worth naming is the one that took the
        // afternoon, not the one that happened earliest.
        assert_eq!(
            rows[0].stretches.iter().map(|x| x.seconds).collect::<Vec<_>>(),
            [45 * 60, 30 * 60, 12 * 60]
        );

        // And each carries the id `set_span_category` needs.
        let one = &rows[0].stretches[1];
        s.set_span_category(one.id, Some(&named(&s, "Study"))).unwrap();
        assert_eq!(totals(&s), [("Study".to_string(), 30 * 60)]);
        // The other two are untouched and still on the list.
        let rows = s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap();
        assert_eq!(rows[0].occurrences, 2);
    }

    /// The detail that tells two stretches of the same site apart.
    ///
    /// The connector sends a domain and never a page title — that is the privacy
    /// design. The *window* title is a different, already-opt-in channel, and
    /// Chrome puts the video's name in it. Subtracting the app span underneath
    /// the domain span must not throw that away.
    #[test]
    fn a_stretch_keeps_the_window_title_that_says_which_video_it_was() {
        let (mut s, clock) = store();
        s.set_activity_setting("activity.titlesEnabled", json!(true))
            .unwrap();
        let start = clock.now();

        // The sampler, which sees the window title.
        for _ in 0..90 {
            s.record_activity(ActivitySample {
                app_id: "chrome.exe".into(),
                window_title: Some("Rust lifetimes explained - YouTube".into()),
                domain: None,
                at: clock.now(),
            })
            .unwrap();
            clock.advance(SAMPLE_INTERVAL_MS);
        }
        // The connector, which sees the site and deliberately not the title.
        clock.set(start);
        observe(&mut s, &clock, "chrome.exe", Some("youtube.com"), 30);

        let spans = s.labelled_spans(DATE, "UTC").unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].domain.as_deref(), Some("youtube.com"));
        assert_eq!(
            spans[0].window_title.as_deref(),
            Some("Rust lifetimes explained - YouTube"),
            "the one detail that distinguishes a lecture from a music video"
        );
    }

    /// Deleting a label must not delete the observation it was on. The time
    /// returns to unlabelled and reappears on the list, rather than vanishing.
    #[test]
    fn deleting_a_category_frees_its_time_rather_than_destroying_it() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("coursera.org"), 30);
        assert_eq!(totals(&s), [("Study".to_string(), 30 * 60)]);

        s.delete_category(&named(&s, "Study")).unwrap();
        assert!(totals(&s).is_empty());
        let rows = s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap();
        assert_eq!(rows[0].match_value, "coursera.org");
        assert_eq!(rows[0].seconds, 30 * 60, "the time survived the label");
    }

    /// Rules are the user's to edit and remove — including the ones shipped
    /// with the app. A "shipped" rule you cannot argue with is a rule that
    /// mislabels your month and tells you to live with it.
    #[test]
    fn a_shipped_rule_can_be_repointed_and_deleted() {
        let (mut s, _) = store();
        fn youtube(s: &Store) -> Option<ActivityRuleRow> {
            s.get_activity_rules()
                .unwrap()
                .into_iter()
                .find(|r| r.match_value == "youtube.com")
        }
        let before = youtube(&s).expect("youtube ships as a rule");
        assert!(before.is_builtin);
        assert_eq!(before.category_name, "Distraction");

        // Repoint it. Editing a shipped rule makes it the user's — one row, not
        // a second one shadowing the first.
        let study = named(&s, "Study");
        s.set_activity_rule(MatchKind::Domain, "youtube.com", &study)
            .unwrap();
        let after = youtube(&s).unwrap();
        assert_eq!(after.category_name, "Study");
        assert!(!after.is_builtin, "an edited rule belongs to the user");
        assert_eq!(
            s.get_activity_rules()
                .unwrap()
                .iter()
                .filter(|r| r.match_value == "youtube.com")
                .count(),
            1
        );

        // And remove it entirely.
        s.delete_activity_rule(&after.id).unwrap();
        assert!(youtube(&s).is_none());
        assert_eq!(s.classify("chrome.exe", Some("youtube.com")).unwrap(), None);
    }

    /// Typing a site by hand is the path that works before the browser
    /// extension is installed — and the one that lets someone set up their
    /// labels in advance rather than waiting for a day of observation.
    #[test]
    fn a_site_can_be_labelled_before_it_has_ever_been_observed() {
        let (mut s, clock) = store();
        let study = named(&s, "Study");
        s.set_activity_rule(MatchKind::Domain, "docs.rs", &study).unwrap();

        observe(&mut s, &clock, "chrome.exe", Some("docs.rs"), 25);
        assert_eq!(totals(&s), [("Study".to_string(), 25 * 60)]);
    }

    #[test]
    fn the_two_categories_the_schema_needs_cannot_be_deleted() {
        let (mut s, _) = store();
        assert!(s.delete_category(category::WORK).is_err());
        assert!(s.delete_category(category::OTHER).is_err());
        // Everything else is the user's, including the ones shipped built-in.
        assert!(s.delete_category(category::LIFE).is_ok());
    }

    /// The promise `counts_as` exists to keep: a user adding a category must
    /// never move a figure that was already on screen.
    #[test]
    fn adding_a_category_does_not_move_an_existing_total() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("youtube.com"), 30);
        let before = s.get_day(DATE, "UTC", None).unwrap().totals.entertainment_sec;

        let mine = s
            .create_category("Doomscrolling", None, DomainCategory::Entertainment)
            .unwrap();
        s.set_activity_rule(MatchKind::Domain, "youtube.com", &mine.id)
            .unwrap();

        assert_eq!(
            s.get_day(DATE, "UTC", None).unwrap().totals.entertainment_sec,
            before,
            "the dashboard's arithmetic moved under a rename"
        );
    }

    #[test]
    fn a_category_needs_a_name_and_cannot_duplicate_one() {
        let (mut s, _) = store();
        assert!(s.create_category("  ", None, DomainCategory::Other).is_err());
        assert!(s
            .create_category("Study", None, DomainCategory::Core)
            .is_err());
    }

    #[test]
    fn a_rule_needs_a_real_category_and_a_real_subject() {
        let (mut s, _) = store();
        let work = named(&s, "Work");
        assert!(s
            .set_activity_rule(MatchKind::Domain, "not a domain", &work)
            .is_err());
        assert!(s
            .set_activity_rule(MatchKind::App, "code.exe", "00000000-0000-7000-8000-0000000000ff")
            .is_err());
    }

    // ─── the short-observation floor ───────────────────────────────────

    /// The user's request, and the reason it needs a rule rather than a filter:
    /// deleting the blip alone would leave a hole in the middle of one stretch
    /// of work, and on untimed time that hole reads as **Unaccounted**.
    #[test]
    fn a_blip_between_two_stretches_of_the_same_thing_is_absorbed() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "Code.exe", None, 30);
        observe(&mut s, &clock, "slack.exe", None, 1); // under the floor
        observe(&mut s, &clock, "Code.exe", None, 30);

        let spans = s.labelled_spans(DATE, "UTC").unwrap();
        assert_eq!(spans.len(), 1, "one stretch with a blip in it");
        assert_eq!(spans[0].app_id, "Code.exe");
        assert_eq!(spans[0].seconds(), 61 * 60, "including the minute it covered");
    }

    /// The other half of the rule. Bridging two *different* things across a
    /// dropped span would be a fabrication, so the interval is simply left to
    /// whatever else owns it.
    #[test]
    fn a_blip_between_two_different_things_is_dropped_not_bridged() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "Code.exe", None, 30);
        observe(&mut s, &clock, "slack.exe", None, 1);
        observe(&mut s, &clock, "figma.exe", None, 30);

        let spans = s.labelled_spans(DATE, "UTC").unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].seconds(), 30 * 60, "not stretched over the gap");
        assert_eq!(spans[1].seconds(), 30 * 60);
    }

    #[test]
    fn short_observation_stays_out_of_the_totals_and_the_unlabelled_list() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("news.example"), 1);
        clock.advance(10 * 60_000);
        observe(&mut s, &clock, "chrome.exe", Some("wiki.example"), 20);

        let rows = s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].match_value, "wiki.example");
    }

    /// The floor is applied on read, never at the write. Raising it and lowering
    /// it back has to recover every row — the record is the record.
    #[test]
    fn the_floor_hides_rows_and_never_deletes_them() {
        let (mut s, clock) = store();
        observe(&mut s, &clock, "chrome.exe", Some("news.example"), 1);
        clock.advance(10 * 60_000);

        assert!(s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap().is_empty());
        s.set_activity_setting(MIN_SPAN_SEC, json!(0)).unwrap();
        assert_eq!(s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap().len(), 1);
    }

    // ─── the sampler and the connector describing the same hour ────────

    /// Both are running, both are correct, and both write. Without the
    /// subtraction an hour in Chrome is an hour of `chrome.exe` **plus** an hour
    /// of youtube.com, and every total on the screen is doubled.
    #[test]
    fn the_browser_and_the_sampler_do_not_both_bill_the_same_hour() {
        let (mut s, clock) = store();
        let start = clock.now();
        // The foreground sampler: one long chrome.exe span.
        observe(&mut s, &clock, "chrome.exe", None, 60);
        // The connector, describing the same hour more precisely. Rewound
        // because in life the two interleave — and interleaving is exactly what
        // `record_activity` has to survive without shattering into 20-second rows.
        clock.set(start);
        observe(&mut s, &clock, "chrome.exe", Some("youtube.com"), 30);
        observe(&mut s, &clock, "chrome.exe", Some("github.com"), 30);

        let spans = s.labelled_spans(DATE, "UTC").unwrap();
        let total: i64 = spans.iter().map(|x| x.seconds()).sum();
        assert_eq!(total, 60 * 60, "the hour was billed twice");
        assert_eq!(
            totals(&s),
            [
                ("Work".to_string(), 30 * 60),
                ("Distraction".to_string(), 30 * 60)
            ]
        );
    }

    /// The remainder is real time and must survive. Chrome open on
    /// `chrome://settings` records no domain, and that is genuinely app-only.
    #[test]
    fn app_only_time_the_connector_could_not_see_is_kept() {
        let (mut s, clock) = store();
        let start = clock.now();
        observe(&mut s, &clock, "chrome.exe", None, 60);
        // Only the middle twenty minutes were on a site.
        clock.set(start + 20 * 60_000);
        observe(&mut s, &clock, "chrome.exe", Some("github.com"), 20);

        let spans = s.labelled_spans(DATE, "UTC").unwrap();
        assert_eq!(spans.iter().map(|x| x.seconds()).sum::<i64>(), 60 * 60);
        assert_eq!(totals(&s), [("Work".to_string(), 20 * 60)]);
        // The forty minutes either side are still there, still unlabelled.
        let rows = s.get_unlabelled(DATE, DATE, "UTC", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].match_value, "chrome.exe");
        assert_eq!(rows[0].seconds, 40 * 60);
    }

    #[test]
    fn the_floor_is_two_minutes_unless_someone_says_otherwise() {
        let (mut s, _) = store();
        assert_eq!(s.min_span_sec(), super::super::day::DEFAULT_MIN_SPAN_SEC);
        assert_eq!(s.min_span_sec(), 120);
        s.set_activity_setting(MIN_SPAN_SEC, json!(300)).unwrap();
        assert_eq!(s.min_span_sec(), 300);
    }
}
