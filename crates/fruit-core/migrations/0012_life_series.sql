-- 0012_life_series.sql — life entries that repeat (M9).
--
-- M9 asks that manual life entries "fill gaps, repeat, replace an interval with
-- confirmation, and are editable later". Three of those shipped; repeat did
-- not, which meant sleep — the single largest block of anyone's week, and the
-- one that is genuinely the same every night — had to be typed in daily.
--
-- ── The same two columns blocks got, for the same reason ──────────────
-- Instances are materialised, not computed on read. That decision is argued in
-- `store/recurrence.rs` and it applies here unchanged: a virtual instance has no
-- id, so it cannot be edited, cannot be deleted on its own, and cannot be the
-- "replace this interval" the reconciler offers. Real rows sharing a
-- `series_id` keep every existing feature working without it knowing recurrence
-- exists.
--
-- The cost is the same too: a horizon. Rows exist 90 days ahead and are topped
-- up when a day past them is asked for.
ALTER TABLE life_entry ADD COLUMN series_id TEXT;
-- The rule itself, carried by the seed instance — so "edit the repeat" always
-- has exactly one row to read it from.
ALTER TABLE life_entry ADD COLUMN rrule TEXT;

CREATE INDEX life_entry_series_idx ON life_entry(series_id)
  WHERE series_id IS NOT NULL AND deleted_at IS NULL;
