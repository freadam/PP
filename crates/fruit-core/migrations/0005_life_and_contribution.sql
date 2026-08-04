-- 0005_life_and_contribution.sql — the life-time model (Plan Rev 3 §7).
--
-- The product stops being about work alone. A day is now accounted for across
-- four record types that never merge:
--
--   scheduled_block  what I meant to do            (the plan — never actual time)
--   time_session     confirmed work                (actual)
--   life_entry       confirmed non-work            (actual)   ← new here
--   activity_span    what the machine observed     (observed, not confirmed)
--
-- Overlap between them is resolved on read by precedence, never by mutating a
-- row. See store/day.rs and docs/ARCHITECTURE.md.

-- ─── life areas ───────────────────────────────────────────────────────
-- The workbook's categories, as a table rather than a column of cell colours.
-- `kind` is what the summaries group by: the workbook's "core vs entertainment"
-- split, plus rest, which is neither.
CREATE TABLE life_area (
  id                 TEXT PRIMARY KEY,
  name               TEXT    NOT NULL,
  colour             TEXT    NOT NULL,
  kind               TEXT    NOT NULL DEFAULT 'core'
                       CHECK (kind IN ('core','entertainment','rest','other')),
  monthly_target_sec INTEGER CHECK (monthly_target_sec IS NULL OR monthly_target_sec > 0),
  sort_rank          REAL    NOT NULL,
  -- A built-in area may be renamed or retargeted but not deleted: the seeded
  -- set is what an imported workbook maps onto, and a missing area would turn
  -- an import into a silent data loss.
  is_builtin         INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0,1)),
  is_archived        INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0,1)),
  device_id          TEXT    NOT NULL DEFAULT '',
  created_at         INTEGER NOT NULL,
  updated_at         INTEGER NOT NULL,
  deleted_at         INTEGER
);

-- ─── confirmed non-work time ──────────────────────────────────────────
-- Unlike time_session this has no accumulator and no open state: a life entry
-- is always a closed interval the user asserts after the fact. There is no
-- timer for sleeping.
CREATE TABLE life_entry (
  id           TEXT    PRIMARY KEY,
  life_area_id TEXT    NOT NULL REFERENCES life_area(id) ON DELETE RESTRICT,
  label        TEXT,
  started_at   INTEGER NOT NULL,
  ended_at     INTEGER NOT NULL,
  local_date   TEXT    NOT NULL
                 CHECK (local_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  tz           TEXT    NOT NULL,
  -- "Intentionally untracked": the interval is accounted for — it is not a gap
  -- the reconciler should nag about — but nothing is recorded about it. This is
  -- a flag rather than an area so that privacy never costs a category.
  is_private   INTEGER NOT NULL DEFAULT 0 CHECK (is_private IN (0,1)),
  note         TEXT,
  device_id    TEXT    NOT NULL DEFAULT '',
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  deleted_at   INTEGER,
  CHECK (ended_at > started_at)
);

CREATE INDEX life_entry_date_idx ON life_entry(local_date) WHERE deleted_at IS NULL;
CREATE INDEX life_entry_area_idx ON life_entry(life_area_id) WHERE deleted_at IS NULL;
CREATE INDEX life_entry_time_idx ON life_entry(started_at)  WHERE deleted_at IS NULL;

-- ─── work contribution mode ───────────────────────────────────────────
-- Work records only (Plan Rev 3 §7). A time_session is always work, so this
-- belongs here and deliberately has no counterpart on life_entry — the schema
-- is what makes "contribution modes never apply to personal time" true, rather
-- than a rule the UI is trusted to remember.
--
-- NULL and 'none' are different: NULL is "not recorded", 'none' is "recorded,
-- and the answer is none".
ALTER TABLE time_session ADD COLUMN contribution TEXT
  CHECK (contribution IS NULL
         OR contribution IN ('none','attend','support','own','assist'));

-- ─── observed browser domains ─────────────────────────────────────────
-- The connector is not built yet, but the columns land now: adding them later
-- would mean a migration that has to backfill classification across a table
-- that is by then the largest in the database.
--
-- `domain` is the observed fact (`youtube.com`). `category` is the conclusion
-- drawn at write time from the rules in force then — stored, not derived on
-- read, because a rule added in September must not silently reclassify August.
ALTER TABLE activity_span ADD COLUMN domain TEXT;
ALTER TABLE activity_span ADD COLUMN category TEXT
  CHECK (category IS NULL OR category IN ('core','entertainment','other'));

CREATE INDEX activity_domain_idx ON activity_span(domain, started_at)
  WHERE domain IS NOT NULL;
