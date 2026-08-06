//! `get_week` — one command, one composed DTO (§6.10).
//!
//! Not one query per day, and emphatically not N+1 per block: the frontend
//! issuing its own SQL made this a per-render fan-out in v1.

use std::collections::HashMap;

use rusqlite::params;

use super::Store;
use crate::error::Result;
use crate::model::*;
use crate::time::{date_series, day_end, day_start, local_date, parse_date, zone};

/// Tolerance band for "on estimate" — a minute either way is not drift.
const ON_ESTIMATE_TOLERANCE_SEC: i64 = 60;

impl Store {
    pub fn get_week(&self, range: &DateRange, tz: &str) -> Result<WeekView> {
        let zone = zone(tz)?;
        let now = self.now();
        let today = local_date(now, &zone);
        let dates = date_series(&range.from, &range.to)?;

        let placeholders = vec!["?"; dates.len()].join(",");
        let sql = format!(
            "SELECT b.id, b.task_id, b.label, b.starts_at, b.duration_sec, b.local_date, b.tz,
                    b.is_fixed, b.series_id, b.rrule, b.external_uid, b.intent,
                    b.created_at, b.updated_at,
                    t.title, t.status, t.project_id, p.colour,
                    COALESCE(c.tracked_sec, 0),
                    EXISTS(SELECT 1 FROM time_session s
                            WHERE s.block_id = b.id AND s.ended_at IS NULL)
               FROM scheduled_block b
               LEFT JOIN task t    ON t.id = b.task_id
               LEFT JOIN project p ON p.id = t.project_id
               LEFT JOIN block_tracked_cache c ON c.block_id = b.id
              WHERE b.local_date IN ({placeholders}) AND b.deleted_at IS NULL
              ORDER BY b.starts_at, b.created_at"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(dates.iter()), |r| {
            let block = Self::map_block(r)?;
            let title: Option<String> = r.get(14)?;
            let status: Option<String> = r.get(15)?;
            Ok((
                BlockView {
                    title: title
                        .or_else(|| block.label.clone())
                        .unwrap_or_else(|| "Untitled".into()),
                    project_id: r.get(16)?,
                    project_colour: r.get(17)?,
                    task_status: status.as_deref().and_then(Status::parse),
                    planned_sec: block.duration_sec,
                    tracked_sec: r.get(18)?,
                    drift_sec: 0,
                    drift_state: DriftState::NotStarted,
                    is_running: r.get::<_, i64>(19)? == 1,
                    lane: 0,
                    lanes: 1,
                    block,
                },
                (),
            ))
        })?;

        let mut by_date: HashMap<String, Vec<BlockView>> = HashMap::new();
        for row in rows {
            let (mut view, _) = row?;
            view.drift_sec = view.tracked_sec - view.planned_sec;
            by_date
                .entry(view.block.local_date.clone())
                .or_default()
                .push(view);
        }

        let unplanned = self.unplanned_by_date(&dates, tz)?;
        let reconciled = self.reconciled_dates(&dates)?;

        let mut days = Vec::with_capacity(dates.len());
        for date in dates {
            let is_past = date.as_str() < today.as_str();
            let is_today = date == today;
            let mut blocks = by_date.remove(&date).unwrap_or_default();
            for b in blocks.iter_mut() {
                b.drift_state = drift_state(b.tracked_sec, b.planned_sec, is_past);
            }
            assign_lanes(&mut blocks);
            days.push(DayColumn {
                planned_sec: blocks.iter().map(|b| b.planned_sec).sum(),
                tracked_sec: blocks.iter().map(|b| b.tracked_sec).sum(),
                unplanned_sec: *unplanned.get(&date).unwrap_or(&0),
                is_reconciled: reconciled.contains(&date),
                is_today,
                is_past,
                blocks,
                local_date: date,
            });
        }

        Ok(WeekView {
            from: range.from.clone(),
            to: range.to.clone(),
            tz: tz.to_string(),
            days,
            now,
        })
    }

    /// Sessions with no block, bucketed by the local date they started on —
    /// the unplanned work Reconcile turns into retroactive blocks (§3.7).
    pub(crate) fn unplanned_by_date(
        &self,
        dates: &[String],
        tz: &str,
    ) -> Result<HashMap<String, i64>> {
        let zone = zone(tz)?;
        let mut out = HashMap::new();
        if dates.is_empty() {
            return Ok(out);
        }
        let first = parse_date(&dates[0])?;
        let last = parse_date(dates.last().unwrap())?;
        let from = day_start(first, &zone);
        let to = day_end(last, &zone);
        let mut stmt = self.conn.prepare(
            "SELECT started_at, elapsed_sec FROM time_session
              WHERE block_id IS NULL AND started_at >= ?1 AND started_at < ?2",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (started_at, elapsed) = row?;
            *out.entry(local_date(started_at, &zone)).or_insert(0) += elapsed;
        }
        Ok(out)
    }

    pub(crate) fn reconciled_dates(&self, dates: &[String]) -> Result<Vec<String>> {
        if dates.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; dates.len()].join(",");
        let mut stmt = self.conn.prepare(&format!(
            "SELECT local_date FROM day_review WHERE local_date IN ({placeholders})"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(dates.iter()), |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

/// §5.6 redundancy table, computed once so the planner block, the compact rail
/// in a task row and the report bar can never disagree.
pub fn drift_state(tracked_sec: i64, planned_sec: i64, day_is_past: bool) -> DriftState {
    if tracked_sec == 0 {
        return DriftState::NotStarted;
    }
    let drift = tracked_sec - planned_sec;
    if drift > ON_ESTIMATE_TOLERANCE_SEC {
        DriftState::Overrun
    } else if drift.abs() <= ON_ESTIMATE_TOLERANCE_SEC {
        DriftState::OnEstimate
    } else if day_is_past {
        DriftState::UnderrunPast
    } else {
        DriftState::InProgress
    }
}

/// Collision groups: overlapping blocks lay out side by side in equal columns
/// (§3.1). Blocks are pre-sorted by start.
fn assign_lanes(blocks: &mut [BlockView]) {
    let mut group: Vec<usize> = Vec::new();
    let mut group_end: i64 = i64::MIN;

    let mut i = 0;
    while i <= blocks.len() {
        let starts_new_group = i == blocks.len() || blocks[i].block.starts_at >= group_end;
        if starts_new_group && !group.is_empty() {
            let lanes = group.iter().map(|&j| blocks[j].lane).max().unwrap_or(0) + 1;
            for &j in &group {
                blocks[j].lanes = lanes;
            }
            group.clear();
            group_end = i64::MIN;
        }
        if i == blocks.len() {
            break;
        }

        // First free lane among the blocks this one actually overlaps.
        let start = blocks[i].block.starts_at;
        let end = start + blocks[i].block.duration_sec * 1000;
        let mut taken: Vec<i64> = Vec::new();
        for &j in &group {
            let j_end = blocks[j].block.starts_at + blocks[j].block.duration_sec * 1000;
            if j_end > start {
                taken.push(blocks[j].lane);
            }
        }
        let mut lane = 0;
        while taken.contains(&lane) {
            lane += 1;
        }
        blocks[i].lane = lane;
        group.push(i);
        group_end = group_end.max(end);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_states_cover_the_redundancy_table() {
        assert_eq!(drift_state(0, 3600, false), DriftState::NotStarted);
        assert_eq!(drift_state(2700, 3600, false), DriftState::InProgress);
        assert_eq!(drift_state(3600, 3600, false), DriftState::OnEstimate);
        assert_eq!(drift_state(4440, 3600, false), DriftState::Overrun);
        assert_eq!(drift_state(2280, 3600, true), DriftState::UnderrunPast);
        // A minute either way is not drift.
        assert_eq!(drift_state(3630, 3600, true), DriftState::OnEstimate);
    }
}
