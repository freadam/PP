//! Merge — two adjacent records of the same thing becoming one.
//!
//! The last item on `WIREFRAME-GAP.md`, and the smallest. It exists because
//! recording is interrupted for reasons that have nothing to do with the work:
//! a timer stopped to take a call, a life entry split by a day boundary that
//! was not really a boundary, a session recovered in two halves after a crash.
//! What is left is two rows describing one thing, and every report that groups
//! by row — the drift rail, the session list, the reconciler's item count —
//! counts it twice.
//!
//! ── What a merge asserts, and what it costs ───────────────────────────
//! Merging says "the gap between these was part of the same thing". That is a
//! claim about time nobody recorded, so it is bounded and it is reported: at
//! most [`MERGE_MAX_GAP_SEC`] between consecutive members, and the result says
//! how many seconds it absorbed. A merge that silently annexed forty minutes
//! would be indistinguishable from a bug.
//!
//! ── Why the span and the elapsed stay equal ───────────────────────────
//! A `time_session` carries `elapsed_sec` as well as its endpoints, and idle
//! time can make them differ. The merged session sets `elapsed_sec` to its own
//! span rather than to the sum of its parts, because everything downstream —
//! `resolve_day`'s segments on one side, `block_tracked` on the other — reads
//! one or the other, and a row whose two durations disagree would put a
//! different number in the Day view than in the drift rail. The absorbed
//! seconds are reported instead of hidden in the difference.
//!
//! ── What it refuses ───────────────────────────────────────────────────
//! Different subjects, a running session, a gap past the bound, and fewer than
//! two records. None of those is a merge; each of them is a different edit,
//! and doing it quietly on a merge's behalf is how someone loses an afternoon.

use rusqlite::params;

use super::Store;
use crate::db;
use crate::error::{AppError, Result};
use crate::ids::validate_id;
use crate::model::*;
use crate::time::{local_date, zone};

/// The largest gap a merge will bridge, in seconds.
///
/// Five minutes: long enough to cover a timer stopped for a phone call or two
/// halves of a recovered session, short enough that it cannot swallow a break
/// anybody would describe as a break. It is a judgement, so it is a constant
/// with a name rather than a number inside a comparison.
pub const MERGE_MAX_GAP_SEC: i64 = 300;

impl Store {
    /// Merges two or more adjacent life entries in the same area.
    pub fn merge_life_entries(&mut self, ids: &[String]) -> Result<MergeResult> {
        if ids.len() < 2 {
            return Err(AppError::invalid("Pick at least two records to merge."));
        }
        let mut rows = Vec::new();
        for id in ids {
            validate_id(id, "life entry")?;
            rows.push(self.life_entry_row(id)?);
        }
        rows.sort_by_key(|r| r.started_at);

        let first = &rows[0];
        if rows
            .iter()
            .any(|r| r.life_area_id != first.life_area_id || r.is_private != first.is_private)
        {
            return Err(AppError::invalid(
                "These aren't the same kind of time. Merge only joins records of the same life area.",
            ));
        }
        let absorbed = check_adjacency(rows.iter().map(|r| (r.started_at, r.ended_at)))?;

        let start = first.started_at;
        let end = rows.iter().map(|r| r.ended_at).max().unwrap();
        // A day is the cap on a life entry everywhere else; a merge is not the
        // place to make an exception to it.
        if end - start > 24 * 3600 * 1000 {
            return Err(AppError::invalid(
                "That would be longer than a day. Merge these in smaller runs.",
            ));
        }

        let zone_ = zone(&first.tz)?;
        let now = self.now();
        let keep = first.id.clone();
        // The earliest row survives, so anything already pointing at it — a
        // reconcile decision, an undo token in the session — still points at
        // something.
        let label = rows.iter().find_map(|r| r.label.clone());
        let note = rows.iter().find_map(|r| r.note.clone());

        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE life_entry SET started_at = ?2, ended_at = ?3, local_date = ?4,
                                   label = ?5, note = ?6, updated_at = ?7
              WHERE id = ?1",
            params![keep, start, end, local_date(start, &zone_), label, note, now],
        )?;
        for r in rows.iter().skip(1) {
            tx.execute(
                "UPDATE life_entry SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![r.id, now],
            )?;
        }
        tx.commit()?;

        Ok(MergeResult {
            id: keep,
            merged: rows.len() as i64,
            seconds: (end - start) / 1000,
            absorbed_sec: absorbed,
        })
    }

    /// Merges two or more adjacent sessions on the same task.
    pub fn merge_sessions(&mut self, ids: &[String]) -> Result<MergeResult> {
        if ids.len() < 2 {
            return Err(AppError::invalid("Pick at least two sessions to merge."));
        }
        let mut rows = Vec::new();
        for id in ids {
            validate_id(id, "session")?;
            let row = self.session_row(id)?;
            if row.ended_at.is_none() {
                return Err(AppError::invalid(
                    "One of those is still running. Stop the timer before merging.",
                ));
            }
            if self.timer.session_id.as_deref() == Some(id.as_str()) {
                return Err(AppError::invalid(
                    "That session is the running one. Stop the timer before merging.",
                ));
            }
            rows.push(row);
        }
        rows.sort_by_key(|r| r.started_at);

        let first = &rows[0];
        if rows.iter().any(|r| r.task_id != first.task_id) {
            return Err(AppError::invalid(
                "These are on different tasks. Merge only joins sessions of the same task.",
            ));
        }
        let absorbed =
            check_adjacency(rows.iter().map(|r| (r.started_at, r.ended_at.unwrap_or(r.started_at))))?;

        let start = first.started_at;
        let end = rows.iter().filter_map(|r| r.ended_at).max().unwrap();
        if end - start > 24 * 3600 * 1000 {
            return Err(AppError::invalid(
                "Sessions are capped at 24 hours. Merge these in smaller runs.",
            ));
        }

        let now = self.now();
        let keep = first.id.clone();
        // The block attribution of the earliest survives. Picking any other
        // would silently re-attribute tracked time to a different plan.
        let note = rows.iter().find_map(|r| r.note.clone());

        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE time_session SET ended_at = ?2, elapsed_sec = ?3, note = ?4, updated_at = ?5
              WHERE id = ?1",
            params![keep, end, (end - start) / 1000, note, now],
        )?;
        for r in rows.iter().skip(1) {
            tx.execute("DELETE FROM time_session WHERE id = ?1", [&r.id])?;
        }
        db::rebuild_tracked_caches(&tx)?;
        tx.commit()?;

        Ok(MergeResult {
            id: keep,
            merged: rows.len() as i64,
            seconds: (end - start) / 1000,
            absorbed_sec: absorbed,
        })
    }

    fn life_entry_row(&self, id: &str) -> Result<LifeEntryRow> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {} FROM life_entry e
                       JOIN life_area a ON a.id = e.life_area_id
                      WHERE e.id = ?1 AND e.deleted_at IS NULL",
                    Self::ENTRY_COLS
                ),
                [id],
                Self::map_entry,
            )
            .map_err(|_| AppError::NotFound("life entry"))
    }
}

/// Total seconds between consecutive records that nothing had claimed.
///
/// Errors if any single gap is past the bound. Overlaps are fine and contribute
/// nothing — two records of the same thing that overlap are already one thing.
fn check_adjacency(spans: impl Iterator<Item = (Millis, Millis)>) -> Result<i64> {
    let mut absorbed = 0;
    let mut frontier: Option<Millis> = None;
    for (from, to) in spans {
        if let Some(end) = frontier {
            let gap = (from - end) / 1000;
            if gap > MERGE_MAX_GAP_SEC {
                return Err(AppError::invalid(format!(
                    "There's a {} gap between two of those. Merge joins records that run \
                     together — anything longer is a break, and merging would claim it as \
                     part of the same thing.",
                    super::goals::hm(gap)
                )));
            }
            if gap > 0 {
                absorbed += gap;
            }
        }
        frontier = Some(frontier.map_or(to, |e: Millis| e.max(to)));
    }
    Ok(absorbed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;
    use std::sync::Arc;

    /// Monday 2026-08-03, 09:00 UTC.
    const MONDAY: i64 = 1_785_747_600_000;
    const TZ: &str = "UTC";

    fn store() -> Store {
        Store::in_memory_with_clock(Arc::new(TestClock::new(MONDAY))).unwrap()
    }

    fn area(s: &Store, name: &str) -> String {
        s.get_life_areas(TZ, false)
            .unwrap()
            .into_iter()
            .find(|a| a.name == name)
            .unwrap()
            .id
    }

    fn life(s: &mut Store, area_id: &str, from_min: i64, to_min: i64) -> String {
        s.add_life_entry(NewLifeEntry {
            life_area_id: area_id.to_string(),
            label: None,
            started_at: MONDAY + from_min * 60_000,
            ended_at: MONDAY + to_min * 60_000,
            tz: TZ.into(),
            is_private: false,
            note: None,
            replace_existing: false,
        })
        .unwrap()
        .id
    }

    fn task(s: &mut Store) -> String {
        s.create_task(NewTask {
            title: "Work".into(),
            ..Default::default()
        })
        .unwrap()
        .id
    }

    fn session(s: &mut Store, task_id: &str, from_min: i64, to_min: i64) -> String {
        s.add_session(ManualSession {
            task_id: task_id.to_string(),
            block_id: None,
            started_at: MONDAY + from_min * 60_000,
            ended_at: MONDAY + to_min * 60_000,
            note: None,
        })
        .unwrap()
        .id
    }

    /// The case the feature exists for: one thing recorded as two rows.
    #[test]
    fn two_touching_entries_become_one() {
        let mut s = store();
        let wellbeing = area(&s, "Wellbeing");
        let a = life(&mut s, &wellbeing, 0, 30);
        let b = life(&mut s, &wellbeing, 30, 60);

        let r = s.merge_life_entries(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(r.merged, 2);
        assert_eq!(r.seconds, 3600);
        assert_eq!(r.absorbed_sec, 0, "they touched, so nothing was claimed");
        assert_eq!(r.id, a, "the earliest row survives");

        let on_the_day = s.life_entries_on("2026-08-03", TZ).unwrap();
        assert_eq!(on_the_day.len(), 1);
        assert_eq!(on_the_day[0].ended_at - on_the_day[0].started_at, 3600 * 1000);
    }

    /// A merge asserts that the gap was part of the same thing. That is a claim
    /// about time nobody recorded, so it is reported rather than absorbed
    /// quietly — a merge that silently annexed time is indistinguishable from a
    /// bug.
    #[test]
    fn a_small_gap_is_bridged_and_said_out_loud() {
        let mut s = store();
        let wellbeing = area(&s, "Wellbeing");
        let a = life(&mut s, &wellbeing, 0, 30);
        let b = life(&mut s, &wellbeing, 33, 60);

        let r = s.merge_life_entries(&[a, b]).unwrap();
        assert_eq!(r.seconds, 3600);
        assert_eq!(r.absorbed_sec, 3 * 60, "three minutes nothing had claimed");
    }

    #[test]
    fn a_gap_past_the_bound_is_a_break_and_is_refused() {
        let mut s = store();
        let wellbeing = area(&s, "Wellbeing");
        let a = life(&mut s, &wellbeing, 0, 30);
        let b = life(&mut s, &wellbeing, 90, 120);

        let err = s.merge_life_entries(&[a, b]).unwrap_err();
        assert!(err.to_string().contains("gap"), "got {err}");
        // And nothing moved.
        assert_eq!(s.life_entries_on("2026-08-03", TZ).unwrap().len(), 2);
    }

    #[test]
    fn records_of_different_things_are_not_merged() {
        let mut s = store();
        let (wellbeing, family) = (area(&s, "Wellbeing"), area(&s, "Family"));
        let a = life(&mut s, &wellbeing, 0, 30);
        let b = life(&mut s, &family, 30, 60);
        assert!(s.merge_life_entries(&[a, b]).is_err());

        let t1 = task(&mut s);
        let t2 = s
            .create_task(NewTask {
                title: "Other".into(),
                ..Default::default()
            })
            .unwrap()
            .id;
        let x = session(&mut s, &t1, 120, 150);
        let y = session(&mut s, &t2, 150, 180);
        assert!(s.merge_sessions(&[x, y]).is_err());
    }

    /// Sessions carry `elapsed_sec` as well as endpoints, and everything
    /// downstream reads one or the other. A merged row whose two durations
    /// disagreed would put a different number in the Day view than in the drift
    /// rail.
    #[test]
    fn a_merged_sessions_span_and_elapsed_agree() {
        let mut s = store();
        let t = task(&mut s);
        let a = session(&mut s, &t, 0, 30);
        let b = session(&mut s, &t, 34, 60);

        let r = s.merge_sessions(&[a.clone(), b]).unwrap();
        assert_eq!(r.merged, 2);
        assert_eq!(r.absorbed_sec, 4 * 60);

        let row = s.session_row(&a).unwrap();
        assert_eq!(row.ended_at.unwrap() - row.started_at, 3600 * 1000);
        assert_eq!(row.elapsed_sec, 3600, "span and elapsed agree");

        // And the day agrees with the row.
        let day = s.get_day("2026-08-03", TZ, None).unwrap();
        assert_eq!(day.totals.confirmed_work_sec, 3600);
    }

    #[test]
    fn merging_needs_at_least_two() {
        let mut s = store();
        let wellbeing = area(&s, "Wellbeing");
        let a = life(&mut s, &wellbeing, 0, 30);
        assert!(s.merge_life_entries(std::slice::from_ref(&a)).is_err());
        assert!(s.merge_life_entries(&[]).is_err());
    }

    /// Two records of the same thing that overlap are already one thing, so an
    /// overlap contributes nothing to the absorbed total.
    #[test]
    fn an_overlap_claims_nothing() {
        let mut s = store();
        let wellbeing = area(&s, "Wellbeing");
        let a = life(&mut s, &wellbeing, 0, 40);
        let b = life(&mut s, &wellbeing, 30, 60);

        let r = s.merge_life_entries(&[a, b]).unwrap();
        assert_eq!(r.seconds, 3600);
        assert_eq!(r.absorbed_sec, 0);
    }

    /// Three at a time, out of order, and the run is checked between each
    /// consecutive pair rather than end to end — otherwise a merge of three
    /// rows could bridge a gap the bound forbids.
    #[test]
    fn a_run_is_checked_pair_by_pair() {
        let mut s = store();
        let wellbeing = area(&s, "Wellbeing");
        let a = life(&mut s, &wellbeing, 0, 30);
        let b = life(&mut s, &wellbeing, 32, 60);
        let c = life(&mut s, &wellbeing, 61, 90);

        let r = s
            .merge_life_entries(&[c.clone(), a.clone(), b.clone()])
            .unwrap();
        assert_eq!(r.merged, 3);
        assert_eq!(r.id, a, "the earliest survives whatever order they arrive in");
        assert_eq!(r.absorbed_sec, 3 * 60);

        // A run whose middle gap is fine but whose last gap is not: refused.
        let d = life(&mut s, &wellbeing, 120, 150);
        let e = life(&mut s, &wellbeing, 200, 230);
        let f = life(&mut s, &wellbeing, 231, 260);
        assert!(s.merge_life_entries(&[d, e, f]).is_err());
    }
}
