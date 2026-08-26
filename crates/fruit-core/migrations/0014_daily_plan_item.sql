-- 0014_daily_plan_item.sql — the Top 3 (ARCHITECTURE §4.15, PRODUCT-SPEC §7.4)
-- Forward-only. Never edit a shipped migration.
--
-- A daily plan item places a task in ONE local day's ordered working list. It
-- is deliberately its own row rather than a column on `task`:
--
--   * a task's emphasis changes daily, its identity does not (invariant 25 —
--     promoting a task must not touch its estimate, blocks, sessions or
--     completion history);
--   * `due_date` already means "the date I currently expect to work on this",
--     which is a different claim from "this is one of today's three"; and
--   * the Tomorrow preview is a projection of the next date's items, not a
--     second copy of the task (§4.15).

CREATE TABLE daily_plan_item (
  id         TEXT PRIMARY KEY,
  task_id    TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
  local_date TEXT NOT NULL
               CHECK (local_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  -- §4.15: `top` is the protected three, `more` is the rest of the day's list.
  section    TEXT NOT NULL CHECK (section IN ('top','more')),
  rank       REAL NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Invariant 23: a task may have at most one daily plan item per local date.
-- In the schema, not in a service, because "appears twice in today's list" is
-- the kind of bug that survives every code path that forgot to check.
CREATE UNIQUE INDEX daily_plan_task_date_uq ON daily_plan_item(task_id, local_date);
CREATE INDEX daily_plan_date_idx ON daily_plan_item(local_date, section, rank);
