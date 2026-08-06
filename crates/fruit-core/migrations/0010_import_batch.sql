-- 0010_import_batch.sql — where an imported record came from (M13).
--
-- M13 asks that "a historical workbook month imports through a preview with no
-- silent loss and a variance report". Two of those words drive this migration.
--
-- ── "no silent loss" means reversible ─────────────────────────────────
-- An import that cannot be undone is one nobody dares run twice, and a user who
-- will not run it twice will not run it once. So every record an import writes
-- carries the batch that wrote it, and removing a batch removes exactly what it
-- added and nothing else. That is a stronger promise than "we were careful".
--
-- ── The export has to be able to say where a row came from ────────────
-- §4.8: the workbook carries "a note identifying which records are confirmed,
-- observed or imported". Without this, an imported hour and an hour someone
-- typed in by hand are the same row, and the note would have to be a guess.
--
-- ── Why not a `source = 'imported'` enum value ────────────────────────
-- `time_session.source` already answers a different question — *how was this
-- recorded in Fruit*: timer, pomodoro, manual, recovered. An imported session
-- was recorded by none of those; it was not recorded in Fruit at all. Widening
-- that CHECK would also mean rebuilding the table, and it would say less: this
-- records not merely "imported" but imported from which file, which sheet and
-- which month, which is what someone reconciling a variance six months later
-- actually needs.
CREATE TABLE import_batch (
  id           TEXT    PRIMARY KEY,
  -- The path as the user chose it. Kept verbatim rather than canonicalised:
  -- what they need to recognise is the name they picked, not a resolved path.
  source_path  TEXT    NOT NULL,
  sheet        TEXT    NOT NULL,
  month        TEXT    NOT NULL
                 CHECK (month GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]'),
  tz           TEXT    NOT NULL,
  -- What the commit actually wrote, so the undo affordance can say how much it
  -- is about to remove before removing it.
  sessions_written    INTEGER NOT NULL DEFAULT 0,
  life_entries_written INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL
);

-- Nullable, and NULL for every row that already exists — which is correct:
-- nothing in the database before this migration came from a workbook.
ALTER TABLE life_entry   ADD COLUMN import_batch_id TEXT REFERENCES import_batch(id);
ALTER TABLE time_session ADD COLUMN import_batch_id TEXT REFERENCES import_batch(id);

-- Partial, because the overwhelming majority of rows will never be imported and
-- an index over a column that is almost always NULL should not pay for them.
CREATE INDEX life_entry_import_idx   ON life_entry(import_batch_id)
  WHERE import_batch_id IS NOT NULL;
CREATE INDEX time_session_import_idx ON time_session(import_batch_id)
  WHERE import_batch_id IS NOT NULL;
