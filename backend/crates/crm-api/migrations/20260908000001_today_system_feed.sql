-- Slice 011d §3: the three built-in Today rules (unanswered inquiry, client
-- replied, call outcome needed) become per-Organization system feeds in the
-- filter vocabulary. `today_system_feed` is ordinary relational
-- configuration (like `saved_list`), not history; `today_feed_changed` is
-- the D-015 append-only audit fact for admin edits.

CREATE TABLE today_system_feed (
    organization_id      UUID NOT NULL REFERENCES organization (id),
    feed_key             TEXT NOT NULL CHECK (feed_key IN
                           ('unanswered_inquiry', 'client_replied', 'call_outcome_needed')),
    enabled              BOOLEAN NOT NULL DEFAULT true,
    -- NULL means "use the canonical default regenerated from code"
    -- (spec §3) — an unedited Organization tracks future canonical changes.
    filter               JSONB NULL CHECK (filter IS NULL OR jsonb_typeof(filter) = 'object'),
    -- NULL means "use the canonical 24 hours" for the two person-state
    -- feeds; always NULL for call_outcome_needed (no freshness window).
    fresh_within_hours   INTEGER NULL CHECK (fresh_within_hours BETWEEN 1 AND 8760),
    revision             BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_by_user_id   UUID NULL REFERENCES app_user (id),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, feed_key),
    CHECK (feed_key <> 'call_outcome_needed' OR fresh_within_hours IS NULL)
);

-- Deliberately no DELETE/TRUNCATE privilege (spec §3): feeds are seeded and
-- reverted, never removed. Membership deactivation, list deletion and
-- Person changes never touch this table.
GRANT SELECT, INSERT, UPDATE ON today_system_feed TO crm_app;

-- Backfill (spec §3): one row per feed for every existing Organization, with
-- NULL definition columns, so the migration embeds no definition. Ordering
-- and the `ON CONFLICT` clause are defensive (`create_organization` also
-- seeds these rows going forward with the same `ON CONFLICT DO NOTHING`,
-- but this migration runs once against whatever Organizations already
-- exist).
INSERT INTO today_system_feed (organization_id, feed_key)
SELECT o.id, k.feed_key
FROM organization o
CROSS JOIN (VALUES ('unanswered_inquiry'), ('client_replied'), ('call_outcome_needed')) AS k(feed_key)
ON CONFLICT (organization_id, feed_key) DO NOTHING;

-- --------------------------------------------------------------------
-- today_feed_changed: append-only audit fact (D-015 §2 standard envelope,
-- identical shape to migrations/20260821000004_inquiry_and_facts.sql).
-- PII-free: ids, clause JSON (ids and tokens only) and integers. Seeding
-- and backfill above write no fact — only admin commands do (a later
-- round).
-- --------------------------------------------------------------------

CREATE TABLE today_feed_changed (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id UUID REFERENCES app_user (id),
    on_behalf_of_user_id UUID REFERENCES app_user (id),
    origin TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,
    causation_id UUID,
    corrects_id UUID REFERENCES today_feed_changed (id),
    feed_key TEXT NOT NULL CHECK (feed_key IN
                       ('unanswered_inquiry', 'client_replied', 'call_outcome_needed')),
    change TEXT NOT NULL CHECK (change IN ('updated', 'reverted', 'enabled', 'disabled')),
    from_revision BIGINT NOT NULL,
    to_revision BIGINT NOT NULL,
    enabled_after BOOLEAN NOT NULL,
    -- NULL = canonical (spec §4: a hand-typed default also collapses to
    -- NULL here, same rule as the table above).
    filter_after JSONB NULL CHECK (filter_after IS NULL OR jsonb_typeof(filter_after) = 'object'),
    fresh_within_hours_after INTEGER NULL CHECK (fresh_within_hours_after BETWEEN 1 AND 8760),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX today_feed_changed_org_occurred_idx ON today_feed_changed (organization_id, occurred_at);
CREATE INDEX today_feed_changed_org_correlation_idx ON today_feed_changed (organization_id, correlation_id);

GRANT SELECT, INSERT ON today_feed_changed TO crm_app;

-- Append-only enforcement, same two-layer pattern (grant + trigger) as
-- every other fact table; `reject_mutation()` already exists (migration
-- 20260821000004).

CREATE TRIGGER today_feed_changed_append_only
    BEFORE UPDATE OR DELETE ON today_feed_changed
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER today_feed_changed_no_truncate
    BEFORE TRUNCATE ON today_feed_changed
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
