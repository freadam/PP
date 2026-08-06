-- 0009_block_intent.sql — planning time that is not work (M11, second half).
--
-- The month dashboard has drawn a dashed "planned entertainment" line since it
-- was built, and it has been flat zero the whole time — correctly, because
-- there was no way to plan an entertainment window. The chart said so
-- underneath rather than leaving a mystery line at the axis, which was honest
-- and unsatisfying.
--
-- This is the missing half. M11 asks that "entertainment budgets and
-- planned/unplanned totals reconcile to the underlying intervals": budgets
-- landed with weekly goals in 0008, and *planned* is this.
--
-- ── Why a column on the block, not a new table ────────────────────────
-- An evening plotted for a film is a plan for an interval, which is exactly
-- what `scheduled_block` is. Giving it its own table would mean a second thing
-- the Planner has to draw, the Day view has to lay out and the reconciler has to
-- ask about — three implementations of "an interval you intended", drifting
-- apart the day any of them changed.
--
-- ── Why `work` is the default ─────────────────────────────────────────
-- Every block that exists was plotted before this column did, and every one of
-- them was work. Defaulting to anything else would retroactively reclassify a
-- year of plans.
ALTER TABLE scheduled_block ADD COLUMN intent TEXT NOT NULL DEFAULT 'work'
  CHECK (intent IN ('work','entertainment','life'));

-- The dashboard's per-day series reads this, so it is worth an index on the
-- shape that query has: one day, filtered by intent.
CREATE INDEX block_intent_idx ON scheduled_block(local_date, intent)
  WHERE deleted_at IS NULL AND intent <> 'work';
