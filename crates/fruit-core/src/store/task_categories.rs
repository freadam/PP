//! Kinds of work — "coding", "design", "meeting" (migration 0013).
//!
//! Every other axis Fruit already had answers a different question. A *project*
//! says what the work was for. A *tag* slices across anything. A *life area*
//! says what non-work time was. None of them answers "what kind of work was
//! it", which is the split a working week is usually reviewed by: four hours of
//! meetings and two of coding is a different Tuesday from the reverse, on the
//! same project, with the same total.
//!
//! # One per task, on purpose
//!
//! The constraint is the feature. A category report has to add to the work
//! total exactly once, and a task carrying three tags has no single answer —
//! it would be counted three times or arbitrarily once, and either way the
//! percentages stop meaning anything. Tags remain for everything else.
//!
//! # Deleting frees, never destroys
//!
//! `ON DELETE SET NULL`, so removing a category returns its tasks to
//! uncategorised rather than deleting them — the same rule the observation
//! labels already follow, and for the same reason: a taxonomy edit must not be
//! able to destroy a record.

use rusqlite::{params, Row};

use super::Store;
use crate::error::{AppError, Result};
use crate::ids::{new_id, validate_id};
use crate::model::*;

impl Store {
    const CATEGORY_COLS: &'static str =
        "id, name, colour, sort_rank, is_builtin, created_at, updated_at";

    fn map_category(row: &Row) -> rusqlite::Result<TaskCategoryRow> {
        Ok(TaskCategoryRow {
            id: row.get(0)?,
            name: row.get(1)?,
            colour: row.get(2)?,
            sort_rank: row.get(3)?,
            is_builtin: row.get::<_, i64>(4)? == 1,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            task_count: 0,
        })
    }

    /// Every live category, with how many open tasks carry it.
    ///
    /// The count is here so the picker can be ordered by what you actually use
    /// and so deleting one can say what it will free, rather than asking the
    /// user to guess.
    pub fn get_task_categories(&self) -> Result<Vec<TaskCategoryRow>> {
        let sql = format!(
            "SELECT {}, (SELECT COUNT(*) FROM task t
                          WHERE t.category_id = c.id AND t.deleted_at IS NULL)
               FROM task_category c
              WHERE c.deleted_at IS NULL
              ORDER BY c.sort_rank, c.name",
            Self::CATEGORY_COLS
                .split(", ")
                .map(|c| format!("c.{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            let mut row = Self::map_category(r)?;
            row.task_count = r.get(7)?;
            Ok(row)
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    fn task_category(&self, id: &str) -> Result<TaskCategoryRow> {
        self.get_task_categories()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(AppError::NotFound("category"))
    }

    pub fn create_task_category(&mut self, name: &str, colour: Option<&str>) -> Result<TaskCategoryRow> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::invalid("A category needs a name."));
        }
        // Checked here as well as by the unique index, so the user gets the
        // sentence rather than a constraint violation.
        if self
            .get_task_categories()?
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(name))
        {
            return Err(AppError::invalid(format!(
                "There is already a category called {name}."
            )));
        }
        let id = new_id();
        let now = self.now();
        let rank: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_rank), 0) + 1 FROM task_category",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1.0);
        self.conn.execute(
            "INSERT INTO task_category
               (id, name, colour, sort_rank, is_builtin, device_id, created_at, updated_at)
             VALUES (?1,?2,?3,?4,0,?5,?6,?6)",
            params![
                id,
                name,
                colour.unwrap_or("#7C7C7C"),
                rank,
                self.device_id,
                now
            ],
        )?;
        self.task_category(&id)
    }

    /// Rename or recolour.
    ///
    /// Renaming clears `is_builtin`. A category the user has renamed is theirs,
    /// and continuing to call it built-in would be the app claiming ownership
    /// of a word it did not choose.
    pub fn update_task_category(
        &mut self,
        id: &str,
        name: Option<&str>,
        colour: Option<&str>,
    ) -> Result<TaskCategoryRow> {
        validate_id(id, "category")?;
        let now = self.now();
        if let Some(name) = name {
            let name = name.trim();
            if name.is_empty() {
                return Err(AppError::invalid("A category needs a name."));
            }
            if self
                .get_task_categories()?
                .iter()
                .any(|c| c.id != id && c.name.eq_ignore_ascii_case(name))
            {
                return Err(AppError::invalid(format!(
                    "There is already a category called {name}."
                )));
            }
            self.conn.execute(
                "UPDATE task_category SET name = ?2, is_builtin = 0, updated_at = ?3 WHERE id = ?1",
                params![id, name, now],
            )?;
        }
        if let Some(colour) = colour {
            self.conn.execute(
                "UPDATE task_category SET colour = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, colour, now],
            )?;
        }
        self.task_category(id)
    }

    /// Remove a category. Its tasks become uncategorised; none of them is
    /// touched otherwise, and no recorded time moves.
    ///
    /// Returns how many tasks were freed, so the confirmation can say it.
    pub fn delete_task_category(&mut self, id: &str) -> Result<i64> {
        validate_id(id, "category")?;
        let now = self.now();
        let tx = self.conn.transaction()?;
        let freed: i64 = tx.query_row(
            "SELECT COUNT(*) FROM task WHERE category_id = ?1 AND deleted_at IS NULL",
            [id],
            |r| r.get(0),
        )?;
        tx.execute("UPDATE task SET category_id = NULL, updated_at = ?2 WHERE category_id = ?1",
            params![id, now])?;
        // Soft, like every other taxonomy row: a session recorded last March
        // was against a task that had this category, and a hard delete would
        // make that month unreadable.
        tx.execute(
            "UPDATE task_category SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        tx.commit()?;
        Ok(freed)
    }

    /// File a task under a category, or clear it with `None`.
    pub fn set_task_category(&mut self, task_id: &str, category_id: Option<&str>) -> Result<()> {
        validate_id(task_id, "task")?;
        if let Some(id) = category_id {
            validate_id(id, "category")?;
            self.task_category(id)?;
        }
        let now = self.now();
        let changed = self.conn.execute(
            "UPDATE task SET category_id = ?2, updated_at = ?3
              WHERE id = ?1 AND deleted_at IS NULL",
            params![task_id, category_id, now],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound("task"));
        }
        Ok(())
    }
}
