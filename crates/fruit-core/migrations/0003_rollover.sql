-- 0003_rollover.sql — the "Rollover" estimate.
--
-- Rollover sits at the top of the estimate scale: work that does not fit one
-- sitting and carries across days. It is *not* the same as an unestimated
-- task — "I haven't thought about it yet" and "I have thought about it and it
-- doesn't fit in a number" are different states, and collapsing them into a
-- single NULL would make the backlog unreadable.
--
-- So it gets its own column rather than overloading estimate_sec. The pairing
-- rule — rollover implies no estimate, an estimate clears rollover — is
-- enforced in the command layer, because SQLite cannot add a table-level CHECK
-- to an existing table.

ALTER TABLE task ADD COLUMN is_rollover INTEGER NOT NULL DEFAULT 0
  CHECK (is_rollover IN (0, 1));

-- Calibration already skips rows with no estimate (§6.4), so rollover tasks
-- stay out of the ratio automatically — there is no estimate to compare against.
