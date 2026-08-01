-- 0004_recurrence_and_activity.sql — the P2 features (§2.3).
--
-- `rrule` and `series_id` already exist on scheduled_block from 0001; this adds
-- what the two importers need and the indexes the Activity view depends on.

-- ─── .ics import ──────────────────────────────────────────────────────
-- The VEVENT's UID. Re-importing the same calendar has to update the blocks it
-- created rather than duplicating them, and UID is the only stable identity an
-- external calendar gives us.
ALTER TABLE scheduled_block ADD COLUMN external_uid TEXT;

CREATE UNIQUE INDEX block_external_uid_uq
  ON scheduled_block(external_uid, starts_at)
  WHERE external_uid IS NOT NULL;

-- ─── activity (§3.5) ──────────────────────────────────────────────────
-- 0001 indexed activity_span by time for the day query. Aggregating a day by
-- app is the other half of that view, and at the retention limits in §3.5 the
-- table is the largest in the database.
CREATE INDEX activity_app_idx ON activity_span(app_id, started_at);
