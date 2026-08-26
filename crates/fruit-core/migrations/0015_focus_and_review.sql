-- 0015_focus_and_review.sql — the two records the spec names and the schema
-- did not have (ARCHITECTURE §4.5, §4.8; PRODUCT-SPEC FR-FOC-001, FR-REV-001).
-- Forward-only. Never edit a shipped migration.

-- ─── focus intervals: deep work INSIDE a tracked session (§3.4, §4.5) ───
--
-- Fruit already had a "focus session": a scheduled block marked is_focus plus
-- a timer deadline. That is a *plan* — an intention to work for 45 minutes.
-- It cannot answer "how much of that session was actually deep work", because
-- a block is not a record of anything (invariant 5).
--
-- This is the record. It nests inside a session, and the invariants that make
-- it meaningful are enforced here where they cannot be forgotten:
--   * it belongs to exactly one session (FK, cascade);
--   * a closed interval has positive duration; and
--   * containment and non-overlap are checked in the domain, because SQLite
--     cannot express "within my parent's bounds" or "disjoint from my
--     siblings" as a column constraint.
CREATE TABLE focus_interval (
  id                  TEXT PRIMARY KEY,
  session_id          TEXT NOT NULL REFERENCES time_session(id) ON DELETE CASCADE,
  started_at          INTEGER NOT NULL,
  ended_at            INTEGER,
  -- Why it stopped early, when the user says. Never inferred.
  interruption_reason TEXT,
  created_at          INTEGER NOT NULL,
  updated_at          INTEGER NOT NULL,
  CHECK (ended_at IS NULL OR ended_at > started_at)
);

CREATE INDEX focus_session_idx ON focus_interval(session_id, started_at);
-- The one open interval, found without scanning history.
CREATE INDEX focus_open_idx ON focus_interval(ended_at) WHERE ended_at IS NULL;

-- ─── session review: the verification decision, with its evidence (§4.8) ─
--
-- `time_session.is_confirmed` is a flag: it can say a session needs looking
-- at, and nothing else. It cannot say when the decision was made, what the
-- user was shown when they made it, or what an earlier decision said before
-- this one replaced it — all of which §4.8 wants, and all of which is the
-- difference between a record you trust and a checkbox.
--
-- A new decision SUPERSEDES rather than deletes: `supersedes_review_id`
-- chains them, so re-verifying an edited session keeps the history of what was
-- believed and when. The coverage columns are an audit snapshot of the
-- evidence at review time; the observations themselves stay where they are, so
-- a later reclassification is visibly a later reclassification.
CREATE TABLE session_review (
  id                   TEXT PRIMARY KEY,
  session_id           TEXT NOT NULL REFERENCES time_session(id) ON DELETE CASCADE,
  supersedes_review_id TEXT REFERENCES session_review(id) ON DELETE SET NULL,
  decision             TEXT NOT NULL CHECK (decision IN ('verified','pending','voided')),
  reviewed_at          INTEGER NOT NULL,
  observed_work_sec    INTEGER NOT NULL DEFAULT 0 CHECK (observed_work_sec >= 0),
  observed_other_sec   INTEGER NOT NULL DEFAULT 0 CHECK (observed_other_sec >= 0),
  idle_sec             INTEGER NOT NULL DEFAULT 0 CHECK (idle_sec >= 0),
  unobserved_sec       INTEGER NOT NULL DEFAULT 0 CHECK (unobserved_sec >= 0),
  note                 TEXT,
  created_at           INTEGER NOT NULL
);

CREATE INDEX review_session_idx ON session_review(session_id, reviewed_at);

-- The current decision for a session: the one review nothing supersedes.
-- A view rather than a column, so "current" cannot drift from the chain.
CREATE VIEW session_current_review AS
SELECT r.*
  FROM session_review r
 WHERE NOT EXISTS (
       SELECT 1 FROM session_review s WHERE s.supersedes_review_id = r.id
 );
