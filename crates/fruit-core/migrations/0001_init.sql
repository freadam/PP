-- 0001_init.sql — Fruit base schema (spec §6.2, §6.3, §6.4)
-- Forward-only. Never edit a shipped migration.

-- ─── projects ─────────────────────────────────────────────────────────
CREATE TABLE project (
  id                TEXT PRIMARY KEY,
  name              TEXT    NOT NULL,
  colour            TEXT    NOT NULL,
  icon              TEXT,
  kind              TEXT    NOT NULL DEFAULT 'work',
  sort_rank         REAL    NOT NULL,
  is_archived       INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0,1)),
  weekly_target_sec INTEGER CHECK (weekly_target_sec IS NULL OR weekly_target_sec > 0),
  device_id         TEXT    NOT NULL DEFAULT '',
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL,
  deleted_at        INTEGER
);

-- ─── tasks (subtasks are tasks) ────────────────────────────────────────
CREATE TABLE task (
  id           TEXT PRIMARY KEY,
  project_id   TEXT REFERENCES project(id) ON DELETE SET NULL,
  parent_id    TEXT REFERENCES task(id)    ON DELETE CASCADE,
  title        TEXT    NOT NULL CHECK (length(trim(title)) > 0),
  status       TEXT    NOT NULL DEFAULT 'open'
                 CHECK (status IN ('open','done','cancelled')),
  estimate_sec INTEGER CHECK (estimate_sec IS NULL OR estimate_sec > 0),
  -- NOTE: the spec writes an underscore GLOB pattern for dates. In SQLite `_` is a
  -- LIKE wildcard; GLOB uses `?`, so that pattern matches a literal string and
  -- rejects every real date. The character class below is what it meant, and is
  -- stricter besides — it enforces digits. See docs/SPEC-DEVIATIONS.md.
  due_date     TEXT    CHECK (due_date IS NULL OR due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  due_at       INTEGER,
  priority     INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 3),
  energy       TEXT    CHECK (energy IS NULL OR energy IN ('low','mid','high')),
  sort_rank    REAL    NOT NULL,
  completed_at INTEGER,
  device_id    TEXT    NOT NULL DEFAULT '',
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  deleted_at   INTEGER,
  CHECK (NOT (due_date IS NOT NULL AND due_at IS NOT NULL)),
  CHECK ((status = 'done') = (completed_at IS NOT NULL))
);

-- ─── tags: queryable and renamable, unlike a JSON column ───────────────
CREATE TABLE tag (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL COLLATE NOCASE,
  colour     TEXT,
  created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX tag_name_uq ON tag(name);

CREATE TABLE task_tag (
  task_id TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
  tag_id  TEXT NOT NULL REFERENCES tag(id)  ON DELETE CASCADE,
  PRIMARY KEY (task_id, tag_id)
) WITHOUT ROWID;

-- ─── notes ─────────────────────────────────────────────────────────────
CREATE TABLE note (
  task_id    TEXT PRIMARY KEY REFERENCES task(id) ON DELETE CASCADE,
  markdown   TEXT    NOT NULL DEFAULT '',
  updated_at INTEGER NOT NULL
);

-- ─── the plot: what you intended ───────────────────────────────────────
CREATE TABLE scheduled_block (
  id           TEXT PRIMARY KEY,
  task_id      TEXT REFERENCES task(id) ON DELETE CASCADE,
  label        TEXT,
  starts_at    INTEGER NOT NULL,
  duration_sec INTEGER NOT NULL
                 CHECK (duration_sec BETWEEN 300 AND 43200),
  local_date   TEXT    NOT NULL CHECK (local_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  tz           TEXT    NOT NULL,
  is_fixed     INTEGER NOT NULL DEFAULT 0 CHECK (is_fixed IN (0,1)),
  rrule        TEXT,
  series_id    TEXT,
  device_id    TEXT    NOT NULL DEFAULT '',
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  deleted_at   INTEGER,
  CHECK (task_id IS NOT NULL OR label IS NOT NULL)
);

-- ─── the track: what actually happened ─────────────────────────────────
CREATE TABLE time_session (
  id           TEXT PRIMARY KEY,
  task_id      TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
  block_id     TEXT REFERENCES scheduled_block(id) ON DELETE SET NULL,
  started_at   INTEGER NOT NULL,
  ended_at     INTEGER,
  elapsed_sec  INTEGER NOT NULL DEFAULT 0 CHECK (elapsed_sec >= 0),
  heartbeat_at INTEGER,
  source       TEXT    NOT NULL DEFAULT 'timer'
                 CHECK (source IN ('timer','pomodoro','manual','recovered')),
  is_confirmed INTEGER NOT NULL DEFAULT 1 CHECK (is_confirmed IN (0,1)),
  note         TEXT,
  device_id    TEXT    NOT NULL DEFAULT '',
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  CHECK (ended_at IS NULL OR ended_at >= started_at)
);

-- ─── singleton: enforces "at most one running timer" in the schema ─────
CREATE TABLE app_state (
  id                 INTEGER PRIMARY KEY CHECK (id = 1),
  running_session_id TEXT REFERENCES time_session(id) ON DELETE SET NULL,
  device_id          TEXT NOT NULL,
  last_purge_at      INTEGER
);
INSERT INTO app_state (id, device_id) VALUES (1, '');

-- ─── reconciliation outcome, one row per reconciled local date ─────────
CREATE TABLE day_review (
  local_date        TEXT PRIMARY KEY CHECK (local_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  reconciled_at     INTEGER NOT NULL,
  planned_sec       INTEGER NOT NULL,
  tracked_sec       INTEGER NOT NULL,
  overrun_sec       INTEGER NOT NULL,
  unplanned_sec     INTEGER NOT NULL,
  blocks_total      INTEGER NOT NULL,
  blocks_untouched  INTEGER NOT NULL,
  calibration_ratio REAL
);

-- ─── activity observation (P2, opt-in) ─────────────────────────────────
CREATE TABLE activity_span (
  id           INTEGER PRIMARY KEY,
  started_at   INTEGER NOT NULL,
  ended_at     INTEGER NOT NULL,
  app_id       TEXT    NOT NULL,
  window_title TEXT,
  is_idle      INTEGER NOT NULL DEFAULT 0 CHECK (is_idle IN (0,1)),
  CHECK (ended_at >= started_at)
);

-- ─── typed key/value settings ──────────────────────────────────────────
CREATE TABLE setting (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

-- ─── indexes (§6.3) ────────────────────────────────────────────────────
CREATE INDEX task_project_idx  ON task(project_id)             WHERE deleted_at IS NULL;
CREATE INDEX task_due_date_idx ON task(status, due_date)       WHERE deleted_at IS NULL;
CREATE INDEX task_due_at_idx   ON task(status, due_at)         WHERE deleted_at IS NULL;
CREATE INDEX task_parent_idx   ON task(parent_id)              WHERE parent_id IS NOT NULL;
CREATE INDEX task_rank_idx     ON task(sort_rank)              WHERE deleted_at IS NULL;
CREATE INDEX block_date_idx    ON scheduled_block(local_date)  WHERE deleted_at IS NULL;
CREATE INDEX block_task_idx    ON scheduled_block(task_id);
CREATE INDEX block_series_idx  ON scheduled_block(series_id)   WHERE series_id IS NOT NULL;
CREATE INDEX session_task_idx  ON time_session(task_id, started_at);
CREATE INDEX session_block_idx ON time_session(block_id)       WHERE block_id IS NOT NULL;
CREATE INDEX session_open_idx  ON time_session(ended_at)       WHERE ended_at IS NULL;
CREATE INDEX activity_time_idx ON activity_span(started_at);

-- ─── derived data (§6.4) — views are the truth ─────────────────────────
CREATE VIEW block_tracked AS
SELECT b.id AS block_id,
       COALESCE(SUM(s.elapsed_sec), 0) AS tracked_sec
FROM scheduled_block b
LEFT JOIN time_session s ON s.block_id = b.id
WHERE b.deleted_at IS NULL
GROUP BY b.id;

CREATE VIEW task_tracked AS
SELECT t.id AS task_id,
       COALESCE(SUM(s.elapsed_sec), 0) AS tracked_sec,
       MAX(s.ended_at)                 AS last_tracked_at
FROM task t
LEFT JOIN time_session s ON s.task_id = t.id
WHERE t.deleted_at IS NULL
GROUP BY t.id;
