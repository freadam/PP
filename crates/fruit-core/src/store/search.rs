//! Full-text search over tasks, notes and project names (§2.3), plus the
//! command palette's fuzzy source.
//!
//! LIKE over an indexed-by-rank table is well inside the 50ms budget in §7.1 at
//! the reference database size; FTS5 is the escape hatch if that stops holding.

use rusqlite::params;

use super::Store;
use crate::error::Result;
use crate::model::{SearchHit, SearchResults};

impl Store {
    pub fn search(&self, query: &str, limit: u32) -> Result<SearchResults> {
        let q = query.trim();
        let mut hits = Vec::new();
        if q.is_empty() {
            return Ok(SearchResults {
                query: q.to_string(),
                hits,
            });
        }
        let needle = format!("%{q}%");
        let limit = limit.clamp(1, 200);

        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.title, p.name
               FROM task t LEFT JOIN project p ON p.id = t.project_id
              WHERE t.deleted_at IS NULL AND t.title LIKE ?1 COLLATE NOCASE
              ORDER BY t.status ASC, t.updated_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![needle, limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in rows {
            let (id, title, project) = row?;
            let (from, to) = highlight(&title, q);
            hits.push(SearchHit {
                kind: "task".into(),
                id,
                title,
                subtitle: project,
                match_from: from,
                match_to: to,
            });
        }
        drop(stmt);

        let mut stmt = self.conn.prepare(
            "SELECT id, name FROM project
              WHERE deleted_at IS NULL AND name LIKE ?1 COLLATE NOCASE LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![needle, limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, name) = row?;
            let (from, to) = highlight(&name, q);
            hits.push(SearchHit {
                kind: "project".into(),
                id,
                title: name,
                subtitle: Some("Project".into()),
                match_from: from,
                match_to: to,
            });
        }
        drop(stmt);

        let mut stmt = self.conn.prepare(
            "SELECT n.task_id, t.title, substr(n.body, 1, 120)
               FROM note n JOIN task t ON t.id = n.task_id
              WHERE t.deleted_at IS NULL AND n.body LIKE ?1 COLLATE NOCASE LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![needle, limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, title, snippet) = row?;
            hits.push(SearchHit {
                kind: "note".into(),
                id,
                title,
                subtitle: Some(snippet.replace('\n', " ")),
                match_from: None,
                match_to: None,
            });
        }

        hits.truncate(limit as usize);
        Ok(SearchResults {
            query: q.to_string(),
            hits,
        })
    }
}

/// Byte offsets of the first case-insensitive occurrence, for rendering the
/// matched substring in `--plot` (§5.7 command palette).
fn highlight(haystack: &str, needle: &str) -> (Option<u32>, Option<u32>) {
    match haystack.to_lowercase().find(&needle.to_lowercase()) {
        Some(idx) => (Some(idx as u32), Some((idx + needle.len()) as u32)),
        None => (None, None),
    }
}
