-- 0008_goals.sql — weekly goals (Plan Rev 3, PLAN-WEEKLY-GOALS.md W1/W2).
--
-- Fruit has three horizons and only two of them work. The day is the primary
-- screen and has a ritual. The month is the dashboard. The week — the horizon
-- where a person can still change the outcome — has nothing that says, on
-- Wednesday afternoon, whether it is going the way they meant it to.
--
-- ── A goal is a property of you, not of a category ────────────────────
-- `life_area.monthly_target_sec` and `project.weekly_target_sec` already exist,
-- and they are attributes of the *thing*: "Wellbeing is meant to get 8 hours a
-- month." A goal is different — "I am working a 30-hour week." You change goals
-- without re-categorising your life, and two people sharing a taxonomy do not
-- share intentions. So this is its own table, and those columns stay where they
-- are, still feeding the month dashboard's target-versus-actual bars.
--
-- ── Direction is first-class, not a sign convention ───────────────────
-- The product's primary outcome is a *reduction*, so "at most 5h of
-- entertainment" has to be a goal you succeed at by being under it. A progress
-- bar that turns red when you do the right thing teaches people to ignore
-- progress bars. `at_most` closes M11: an entertainment budget is exactly this.

CREATE TABLE goal (
  id           TEXT PRIMARY KEY,

  -- What is being measured.
  --   metric    — a column of the day's own totals: allWork, life, sleep,
  --               entertainment, unaccounted. `subject_id` names which.
  --   lifeArea  — confirmed non-work time in one area.
  --   project   — confirmed work on one project.
  --   category  — observed time carrying one label (migration 0007).
  subject_kind TEXT    NOT NULL
                 CHECK (subject_kind IN ('metric','lifeArea','project','category')),
  subject_id   TEXT    NOT NULL,

  direction    TEXT    NOT NULL CHECK (direction IN ('atLeast','atMost')),
  target_sec   INTEGER NOT NULL CHECK (target_sec > 0),

  -- Only `week` for now. The shape is identical for a month, and writing the
  -- column now costs nothing; writing the month code before anyone has used a
  -- weekly goal would be guessing.
  period       TEXT    NOT NULL DEFAULT 'week' CHECK (period IN ('week')),

  -- Bitmask, Monday = 1 … Sunday = 64. 127 is every day.
  --
  -- This is Rize's per-weekday work-hours target, generalised, and it does more
  -- than scale the number: a Mon–Fri goal must not report you *behind* on a
  -- Sunday morning, which is what a flat seven-day pro-rate would do.
  applies_days INTEGER NOT NULL DEFAULT 127
                 CHECK (applies_days BETWEEN 1 AND 127),

  -- ISO week, `YYYY-Www`. A goal is closed, never deleted: last month's review
  -- has to still show the goal that was actually in force, for the same reason
  -- `activity_span.category_id` is stamped at write time. A goal edited into a
  -- new number would retroactively rewrite how a month went, and reviews would
  -- stop meaning anything.
  starts_week  TEXT    NOT NULL,
  ends_week    TEXT,

  device_id    TEXT    NOT NULL DEFAULT '',
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);

-- One live goal per subject. Two goals arguing about the same quantity is a
-- contradiction the UI would have to explain, not a feature.
CREATE UNIQUE INDEX goal_live_subject_idx
    ON goal(subject_kind, subject_id) WHERE ends_week IS NULL;
