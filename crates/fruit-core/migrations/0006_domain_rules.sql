-- 0006_domain_rules.sql — how an observed domain becomes a classification.
--
-- Migration 0005 added `activity_span.domain` and `activity_span.category` and
-- said the category is "the conclusion drawn at write time from the rules in
-- force then". This is that rules table.
--
-- The design decision worth writing down: `category` on the span is *stored*,
-- not derived by joining to this table on read. Joining would be less data and
-- more obviously correct-looking, and it would be wrong — adding a rule in
-- September would rewrite what August said you were doing. A record of what was
-- observed must not move under a user who later changes their mind about a
-- domain. The rule decides new spans; the old spans keep their verdict.

CREATE TABLE domain_rule (
  id           TEXT PRIMARY KEY,
  -- A registrable domain, lowercase: `youtube.com`, never a URL and never a
  -- subdomain. `connector::registrable_domain` reduces both sides, so
  -- `music.youtube.com` and `www.youtube.com` match this one row.
  domain       TEXT    NOT NULL UNIQUE,
  category     TEXT    NOT NULL
                 CHECK (category IN ('core','entertainment','other')),
  -- Optional: what "Record as life time" should pre-select when this domain
  -- turns up in the reconciler. NULL means "classify it, but don't presume
  -- where it files".
  life_area_id TEXT    REFERENCES life_area(id) ON DELETE SET NULL,
  -- Shipped with the app rather than typed by the user. Editing a built-in rule
  -- clears this flag rather than adding a second row: one domain, one verdict,
  -- and the user's is the one that counts. That is also why `domain` is UNIQUE —
  -- a precedence contest between two rules for the same domain is a bug waiting
  -- to be argued about, not a feature.
  is_builtin   INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0,1)),
  device_id    TEXT    NOT NULL DEFAULT '',
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);

CREATE INDEX domain_rule_category_idx ON domain_rule(category);
