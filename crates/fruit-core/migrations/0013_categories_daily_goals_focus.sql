-- 0013 — three things a work report needs before it can be written.
--
-- Each is small on its own and none of them is a report. They are here together
-- because they are the same gap seen three ways: the app records *that* you
-- worked and *for how long*, and cannot say *what kind of work it was*, *whether
-- that was enough today*, or *which of those hours were deliberate*.

-- ─────────────────────────────────────────────────────────────────────
-- 1. Task categories — "design", "meeting", "coding"
-- ─────────────────────────────────────────────────────────────────────
--
-- ── Why not tags ──────────────────────────────────────────────────────
-- Tags already exist and are many-to-many, which is exactly what makes them
-- the wrong shape here. "Work split by category" has to add to 100%, and a
-- task tagged #design #urgent #q3 has no single answer — it would be counted
-- three times or arbitrarily once. A category is *one per task*, and that
-- constraint is the whole reason the report is trustworthy.
--
-- Tags stay for everything else. They are for slicing; this is for grouping.
--
-- ── Why a table rather than an enum column ────────────────────────────
-- "design, meeting, coding" are this user's kinds of work, not everyone's. A
-- CHECK constraint would mean a migration every time someone's job changed.
-- The seeds below are a starting point and every one of them is renameable.
CREATE TABLE task_category (
  id         TEXT PRIMARY KEY,
  name       TEXT    NOT NULL COLLATE NOCASE,
  -- Reports draw one bar per category, so each carries its own colour rather
  -- than borrowing a position in a ramp — a category's colour must not change
  -- because another category was added above it.
  colour     TEXT    NOT NULL DEFAULT '#7C7C7C',
  sort_rank  REAL    NOT NULL,
  -- Shipped rather than typed. Renaming one clears the flag: the name is the
  -- user's, and a "built-in" they have renamed is theirs now.
  is_builtin INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0,1)),
  device_id  TEXT    NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER
);

-- One live category per name. Two "Meetings" is a split report and a bug
-- report, never a feature.
CREATE UNIQUE INDEX task_category_name_idx
    ON task_category(name) WHERE deleted_at IS NULL;

-- ON DELETE SET NULL, not CASCADE. Deleting a category must free its tasks,
-- never destroy them — the same rule the labels in Settings already follow,
-- where deleting a label returns its time to unlabelled rather than deleting
-- the time.
ALTER TABLE task ADD COLUMN category_id TEXT
  REFERENCES task_category(id) ON DELETE SET NULL;

-- The report's shape: sum sessions by their task's category. Without this the
-- month split is a full scan of every task.
CREATE INDEX task_category_idx ON task(category_id) WHERE deleted_at IS NULL;

INSERT INTO task_category (id, name, colour, sort_rank, is_builtin, created_at, updated_at)
SELECT * FROM (
  SELECT '01920000-0000-7000-8000-00000000c001' AS id, 'Coding'   AS name, '#0070CC' AS colour, 1.0 AS r, 1 AS b,
         CAST(strftime('%s','now') AS INTEGER) * 1000 AS c, CAST(strftime('%s','now') AS INTEGER) * 1000 AS u
  UNION ALL SELECT '01920000-0000-7000-8000-00000000c002', 'Design',   '#6E399D', 2.0, 1,
         CAST(strftime('%s','now') AS INTEGER) * 1000, CAST(strftime('%s','now') AS INTEGER) * 1000
  UNION ALL SELECT '01920000-0000-7000-8000-00000000c003', 'Meeting',  '#BD3E0C', 3.0, 1,
         CAST(strftime('%s','now') AS INTEGER) * 1000, CAST(strftime('%s','now') AS INTEGER) * 1000
  UNION ALL SELECT '01920000-0000-7000-8000-00000000c004', 'Research', '#267A94', 4.0, 1,
         CAST(strftime('%s','now') AS INTEGER) * 1000, CAST(strftime('%s','now') AS INTEGER) * 1000
  UNION ALL SELECT '01920000-0000-7000-8000-00000000c005', 'Admin',    '#525252', 5.0, 1,
         CAST(strftime('%s','now') AS INTEGER) * 1000, CAST(strftime('%s','now') AS INTEGER) * 1000
);

-- ─────────────────────────────────────────────────────────────────────
-- 2. Daily goals — "at least 6 hours of work, on a weekday"
-- ─────────────────────────────────────────────────────────────────────
--
-- 0008 wrote `period TEXT NOT NULL DEFAULT 'week' CHECK (period IN ('week'))`
-- and a comment saying the month shape is identical and writing it early would
-- be guessing. It is no longer guessing: a daily work target is asked for, and
-- a month is the same query with different bounds.
--
-- SQLite cannot alter a CHECK constraint, so the table is rebuilt. Legacy mode
-- (rename → create → copy → drop) rather than `PRAGMA writable_schema`, which
-- is faster and is how you corrupt a database.
--
-- ── Why a daily goal is not a weekly one divided by five ──────────────
-- A 30h weekly goal with a Mon–Fri mask and a 6h daily goal are different
-- claims. The weekly one is met by a 12-hour Thursday; the daily one is not,
-- and someone who sets a daily target is asking for exactly that distinction.
-- The pro-rating differs too — see `goals.rs`, which does *not* pro-rate a
-- daily goal across the hours of the day, because "you are behind" at 09:40 on
-- a 6h target is the app calling the future a failure.
ALTER TABLE goal RENAME TO goal_old;

CREATE TABLE goal (
  id           TEXT PRIMARY KEY,
  subject_kind TEXT    NOT NULL
                 CHECK (subject_kind IN ('metric','lifeArea','project','category')),
  subject_id   TEXT    NOT NULL,
  direction    TEXT    NOT NULL CHECK (direction IN ('atLeast','atMost')),
  target_sec   INTEGER NOT NULL CHECK (target_sec > 0),
  period       TEXT    NOT NULL DEFAULT 'week'
                 CHECK (period IN ('day','week','month')),
  applies_days INTEGER NOT NULL DEFAULT 127
                 CHECK (applies_days BETWEEN 1 AND 127),
  starts_week  TEXT    NOT NULL,
  ends_week    TEXT,
  device_id    TEXT    NOT NULL DEFAULT '',
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);

INSERT INTO goal (id, subject_kind, subject_id, direction, target_sec, period,
                  applies_days, starts_week, ends_week, device_id, created_at, updated_at)
  SELECT id, subject_kind, subject_id, direction, target_sec, period,
         applies_days, starts_week, ends_week, device_id, created_at, updated_at
    FROM goal_old;

DROP TABLE goal_old;

-- The uniqueness rule changes with the period, and this is the substantive
-- decision in the rebuild. 0008 allowed one live goal per subject; a daily *and*
-- a weekly target on the same subject are not a contradiction — "6h a day, and
-- no more than 32h a week" is a coherent pair of claims — so the index now
-- includes the period. Two goals of the *same* period on one subject is still
-- the contradiction it always was.
CREATE UNIQUE INDEX goal_live_subject_idx
    ON goal(subject_kind, subject_id, period) WHERE ends_week IS NULL;

-- ─────────────────────────────────────────────────────────────────────
-- 3. Focus sessions become countable
-- ─────────────────────────────────────────────────────────────────────
--
-- `start_focus` already creates a `scheduled_block` at the current instant with
-- the intended length — the right decision, and the reason drift, the
-- reconciler and planned-versus-tracked all work on focus sessions for free.
--
-- The cost was that the block is then indistinguishable from one plotted last
-- Tuesday, so "how many focus sessions, for how long" has no query. This is the
-- one bit that was missing.
--
-- Not a value on `intent`: intent says whether an interval was meant to be work,
-- entertainment or life, and a focus session is *work* — the two are orthogonal
-- and folding them together would make "planned entertainment" and "focus
-- session" mutually exclusive for no reason.
ALTER TABLE scheduled_block ADD COLUMN is_focus INTEGER NOT NULL DEFAULT 0
  CHECK (is_focus IN (0,1));

-- The report's shape: focus blocks within a date range.
CREATE INDEX block_focus_idx ON scheduled_block(local_date)
  WHERE deleted_at IS NULL AND is_focus = 1;
