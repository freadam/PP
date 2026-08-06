-- 0007_observation_categories.sql — labelling what the machine observed.
--
-- Migration 0006 gave a domain one of three fixed verdicts. That is not enough
-- to answer the question people actually have about their own time: an hour on
-- Coursera and an hour on Instagram are both "not work", and collapsing them
-- loses the only distinction worth drawing.
--
-- So the fixed enum becomes a table, and rules stop being domain-only.
--
--   observation_category   Work · Study · Distraction · Life · Other, and any
--                          the user adds. Renameable, recolourable, deletable
--                          (except the two the schema needs).
--   activity_rule          app_id OR domain → category. One table, because
--                          "Instagram is a distraction" is the same statement
--                          whether Instagram is a website or a phone-mirroring
--                          app, and two tables would need two UIs.
--
-- ── Why `counts_as` exists ────────────────────────────────────────────
-- Every existing report keys off the three-way core/entertainment/other split:
-- the Day view's "Work + distraction", the month dashboard's Entertainment
-- card, the entertainment trend. A user adding "Study" must not silently move
-- any of those numbers.
--
-- So a category declares which of the three it rolls up into, and
-- `activity_span` keeps **both**: `category_id` is the specific label, and
-- `category` stays as the roll-up the existing queries already read. Adding a
-- category can therefore never change a total that was already on screen.
--
-- ── Why the ids are fixed ─────────────────────────────────────────────
-- Built-in categories use stable, hardcoded UUIDs. That is what lets this
-- migration seed them and backfill `category_id` from the old text column in
-- the same transaction, rather than leaving a backfill to run later in Rust
-- against rows whose meaning has already been lost.

CREATE TABLE observation_category (
  id         TEXT PRIMARY KEY,
  name       TEXT    NOT NULL UNIQUE,
  colour     TEXT    NOT NULL,
  -- The roll-up. Never user-editable for built-ins; chosen once for a custom
  -- category and then fixed, because changing it *would* move a past total.
  counts_as  TEXT    NOT NULL CHECK (counts_as IN ('core','entertainment','other')),
  -- Work and Other cannot be deleted: Other is where unmatched observation
  -- lands, and Work is what the "is this work?" question resolves against.
  is_builtin INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0,1)),
  sort_rank  REAL    NOT NULL,
  device_id  TEXT    NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

-- The four the user named, plus the bucket for everything not yet labelled.
-- 0 is deliberately absent from the low bits so these read as obviously
-- synthetic next to a real UUIDv7.
INSERT INTO observation_category (id, name, colour, counts_as, is_builtin, sort_rank, created_at, updated_at)
VALUES
  ('00000000-0000-7000-8000-000000000001','Work',       '#3FA8BD','core',         1,0,0,0),
  ('00000000-0000-7000-8000-000000000002','Study',      '#7FA440','core',         1,1,0,0),
  ('00000000-0000-7000-8000-000000000003','Distraction','#C9603F','entertainment',1,2,0,0),
  ('00000000-0000-7000-8000-000000000004','Life',       '#C2609B','other',        1,3,0,0),
  ('00000000-0000-7000-8000-000000000005','Other',      '#5B6472','other',        1,4,0,0);

CREATE TABLE activity_rule (
  id          TEXT PRIMARY KEY,
  -- 'app'    — matched against activity_span.app_id, case-insensitively.
  -- 'domain' — matched against the registrable domain, already reduced.
  match_kind  TEXT NOT NULL CHECK (match_kind IN ('app','domain')),
  match_value TEXT NOT NULL,
  category_id TEXT NOT NULL REFERENCES observation_category(id) ON DELETE CASCADE,
  is_builtin  INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0,1)),
  device_id   TEXT NOT NULL DEFAULT '',
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  -- One verdict per thing. A precedence contest between two rules for the same
  -- domain is a bug waiting to be argued about, not a feature.
  UNIQUE (match_kind, match_value)
);

CREATE INDEX activity_rule_category_idx ON activity_rule(category_id);

-- Carry 0006's rules forward. `core` mapped to Work rather than to Study
-- because Work is what 0006's three-way split meant by it.
INSERT INTO activity_rule (id, match_kind, match_value, category_id, is_builtin, device_id, created_at, updated_at)
SELECT id, 'domain', domain,
       CASE category
         WHEN 'entertainment' THEN '00000000-0000-7000-8000-000000000003'
         WHEN 'core'          THEN '00000000-0000-7000-8000-000000000001'
         ELSE                      '00000000-0000-7000-8000-000000000005'
       END,
       is_builtin, device_id, created_at, updated_at
  FROM domain_rule;

DROP TABLE domain_rule;

-- The seed the user asked for by name. Deliberately short: a long shipped list
-- is a long list of small wrong guesses, and the uncategorised surface is how
-- the rest gets built — by the person whose time it is.
--
-- Note what is *not* here. `google.com` is absent because Docs, Sheets, Search
-- and Images share it, and a registrable domain cannot tell them apart —
-- `docs.google.com` reduces to `google.com`. Guessing "work" for all of Google
-- would mislabel more time than it labelled. See docs/PLAN-WEEKLY-GOALS.md.
INSERT OR IGNORE INTO activity_rule (id, match_kind, match_value, category_id, is_builtin, created_at, updated_at)
VALUES
  -- Distraction by default, and changeable per interval. Fruit sees a
  -- registrable domain and deliberately never the URL or the page title, so it
  -- cannot tell a lecture from a music video. Defaulting to the common case and
  -- letting the person who watched it say otherwise (`set_span_category`) is the
  -- only honest arrangement available.
  ('00000000-0000-7000-8000-0000000000f1','domain','youtube.com',  '00000000-0000-7000-8000-000000000003',1,0,0),
  ('00000000-0000-7000-8000-0000000000f2','domain','youtu.be',     '00000000-0000-7000-8000-000000000003',1,0,0),
  ('00000000-0000-7000-8000-0000000000f3','domain','twitch.tv',    '00000000-0000-7000-8000-000000000003',1,0,0),
  ('00000000-0000-7000-8000-000000000101','domain','instagram.com','00000000-0000-7000-8000-000000000003',1,0,0),
  ('00000000-0000-7000-8000-000000000102','domain','tiktok.com',   '00000000-0000-7000-8000-000000000003',1,0,0),
  ('00000000-0000-7000-8000-000000000103','domain','facebook.com', '00000000-0000-7000-8000-000000000003',1,0,0),
  ('00000000-0000-7000-8000-000000000104','domain','reddit.com',   '00000000-0000-7000-8000-000000000003',1,0,0),
  ('00000000-0000-7000-8000-000000000105','domain','netflix.com',  '00000000-0000-7000-8000-000000000003',1,0,0),
  ('00000000-0000-7000-8000-000000000106','domain','coursera.org', '00000000-0000-7000-8000-000000000002',1,0,0),
  ('00000000-0000-7000-8000-000000000107','domain','udemy.com',    '00000000-0000-7000-8000-000000000002',1,0,0),
  ('00000000-0000-7000-8000-000000000108','domain','edx.org',      '00000000-0000-7000-8000-000000000002',1,0,0),
  ('00000000-0000-7000-8000-000000000109','domain','khanacademy.org','00000000-0000-7000-8000-000000000002',1,0,0),
  ('00000000-0000-7000-8000-00000000010a','domain','github.com',   '00000000-0000-7000-8000-000000000001',1,0,0),
  ('00000000-0000-7000-8000-00000000010b','domain','stackoverflow.com','00000000-0000-7000-8000-000000000001',1,0,0);

-- ── the specific label on the observation ─────────────────────────────
-- Alongside `category`, not instead of it. See the note at the top.
--
-- Nullable on purpose: NULL means "no rule matched", which is a real answer and
-- the one the uncategorised surface is built on. A default of 'Other' would
-- make unlabelled time indistinguishable from time someone decided was Other.
ALTER TABLE activity_span ADD COLUMN category_id TEXT REFERENCES observation_category(id);

UPDATE activity_span SET category_id = CASE category
    WHEN 'entertainment' THEN '00000000-0000-7000-8000-000000000003'
    WHEN 'core'          THEN '00000000-0000-7000-8000-000000000001'
    WHEN 'other'         THEN '00000000-0000-7000-8000-000000000005'
  END
 WHERE category IS NOT NULL;

CREATE INDEX activity_span_category_idx ON activity_span(category_id)
  WHERE category_id IS NOT NULL;
