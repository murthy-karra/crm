-- Slice 009 (docs/specs/SLICE_009.md §4): correspondence capture v1.
-- Five items: capture_address (per-agent token), correspondence_raw
-- (encrypted, NO read surface — structurally separate from raw_payload so
-- client correspondence can never enter the 007f/D-038 extraction sweep),
-- correspondence_captured (envelope-style fact table, the
-- intake_token_rotated template + metadata columns), capture_message (the
-- per-agent held queue, mutable, no DELETE), capture_token_rotated (007g
-- §4 shape). Sole owner: this migration.

-- --------------------------------------------------------------------
-- 1. capture_address: one token per (organization, agent). Backfill-mint
--    for existing active members below; mint-if-absent on activation
--    thereafter (domain/capture/address.rs) — a reactivated member gets
--    their EXISTING row back, since the composite FK/UNIQUE make an
--    unconditional mint an error, never a fresh token (criterion 8).
-- --------------------------------------------------------------------

CREATE TABLE capture_address (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    user_id UUID NOT NULL REFERENCES app_user (id),
    -- Plaintext, kept for re-display (GET /api/capture/address), exactly
    -- like organization.intake_token.
    token TEXT NOT NULL,
    -- Deterministic, UNKEYED SHA-256 digest of `token` (docs/specs/SLICE_009.md
    -- §3): the lookup key, so the B-tree probe never touches the secret
    -- token's own bytes (a real security property here, unlike intake's
    -- lookup-by-public-slug — see address.rs for the full reasoning).
    token_lookup BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Ties every capture address to a real (organization, user) membership
    -- pair, regardless of that membership's current status (deactivation
    -- gates RESOLUTION via a status='active' join at lookup time, not this
    -- FK — see domain/capture/address.rs).
    FOREIGN KEY (organization_id, user_id)
        REFERENCES organization_membership (organization_id, user_id),
    UNIQUE (token_lookup),
    UNIQUE (organization_id, user_id)
);

GRANT SELECT, INSERT ON capture_address TO crm_app;
GRANT UPDATE (token, token_lookup) ON capture_address TO crm_app;

-- --------------------------------------------------------------------
-- 2. correspondence_raw: encrypted deletable blob, NO read endpoint ever
--    (D-042.6). NOT raw_payload — structurally immune from the 007f/D-038
--    extraction sweep and workbench/D-037 lifecycle assumptions.
--    `processed` starts false (Phase A, store-raw-first); Phase B flips it
--    to true in the same transaction that inserts the fact/held row — a
--    crash between the two leaves it false, rescued by MTA redelivery
--    (the content_hmac UNIQUE finds the existing row; no admin queue
--    exists for this table, unlike raw_payload's Unresolved+Try-again).
-- --------------------------------------------------------------------

CREATE TABLE correspondence_raw (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organization (id),
    received_at TIMESTAMPTZ NOT NULL,
    nonce BYTEA NOT NULL,
    ciphertext BYTEA NOT NULL,
    content_hmac BYTEA NOT NULL,
    byte_len INT NOT NULL,
    processed BOOLEAN NOT NULL DEFAULT false,
    UNIQUE (organization_id, content_hmac)
);

GRANT SELECT, INSERT ON correspondence_raw TO crm_app;
GRANT UPDATE (processed) ON correspondence_raw TO crm_app;

-- --------------------------------------------------------------------
-- 3. correspondence_captured: envelope-style fact table (the
--    intake_token_rotated template) plus capture-specific columns.
--    PII-free per D-015 §3: person_id/agent_user_id are bare ids, no
--    subject/address/body (D-042.1/2). message_id/thread_key are
--    third-party technical identifiers, not name/contact PII (declared
--    D-015 §3 tension; O-013 decides redaction-on-erasure later).
-- --------------------------------------------------------------------

CREATE TABLE correspondence_captured (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id UUID REFERENCES app_user (id),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL)),
    on_behalf_of_user_id UUID REFERENCES app_user (id),
    origin TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,
    causation_id UUID,
    corrects_id UUID REFERENCES correspondence_captured (id),
    -- No FK (D-015 §5 pattern, matching every other fact table's
    -- person_id): a fact references an erasable row by bare id.
    person_id UUID NOT NULL,
    -- The capture token's owner — ALWAYS the attribution, regardless of
    -- which ladder step matched (docs/specs/SLICE_009.md §5). app_user
    -- rows are never deleted, so this stays FK'd (matches
    -- routing_decision.assignee_user_id's precedent).
    agent_user_id UUID NOT NULL REFERENCES app_user (id),
    direction TEXT NOT NULL CHECK (direction IN ('inbound', 'outbound')),
    -- Normalized (brackets stripped, capped, NUL-stripped) — see
    -- mime.rs/forward.rs. NULL for Message-ID-less mail (dedups only at
    -- the raw byte layer then — accepted, spec §5).
    message_id TEXT,
    thread_key TEXT,
    via TEXT NOT NULL CHECK (via IN ('cc', 'forward')),
    correspondence_raw_id UUID NOT NULL REFERENCES correspondence_raw (id),
    backdated BOOLEAN NOT NULL
);

CREATE INDEX correspondence_captured_org_person_occurred_idx
    ON correspondence_captured (organization_id, person_id, occurred_at);
-- Serves the Today client_replied arm's "latest inbound"/"latest
-- outbound per Person" lookups (domain/today/queries.rs).
CREATE INDEX correspondence_captured_org_person_direction_occurred_idx
    ON correspondence_captured (organization_id, person_id, direction, occurred_at DESC);
CREATE INDEX correspondence_captured_org_correlation_idx
    ON correspondence_captured (organization_id, correlation_id);
-- The dedup layer (spec §5): partial so Message-ID-less mail (NULL) never
-- collides with anything, by construction.
CREATE UNIQUE INDEX correspondence_captured_dedup_idx
    ON correspondence_captured (organization_id, person_id, message_id)
    WHERE message_id IS NOT NULL;

GRANT SELECT, INSERT ON correspondence_captured TO crm_app;

CREATE TRIGGER correspondence_captured_append_only
    BEFORE UPDATE OR DELETE ON correspondence_captured
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER correspondence_captured_no_truncate
    BEFORE TRUNCATE ON correspondence_captured
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

-- --------------------------------------------------------------------
-- 4. capture_message: the per-agent held queue (docs/specs/SLICE_009.md
--    §4 item 4). Mutable (status transitions), never append-only, no
--    DELETE grant — link/dismiss are the only two terminal transitions,
--    both idempotent at the domain layer.
--    counterparty_email is nullable: NULLED on every terminal transition
--    (link AND dismiss) so a no-DELETE table never retains third-party
--    PII forever (D-015 §4; reviewer finding).
-- --------------------------------------------------------------------

CREATE TABLE capture_message (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    agent_user_id UUID NOT NULL REFERENCES app_user (id),
    correspondence_raw_id UUID NOT NULL REFERENCES correspondence_raw (id),
    counterparty_email TEXT,
    -- The ladder's presumed direction at capture time (docs/specs/SLICE_009.md
    -- §5 step 4 / §8): read directly by the link endpoint rather than
    -- re-derived from current state.
    direction_hint TEXT CHECK (direction_hint IN ('inbound', 'outbound')),
    captured_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('held', 'linked', 'dismissed'))
);

CREATE INDEX capture_message_org_agent_status_idx
    ON capture_message (organization_id, agent_user_id, status, captured_at DESC);

GRANT SELECT, INSERT ON capture_message TO crm_app;
GRANT UPDATE (status, counterparty_email) ON capture_message TO crm_app;

-- --------------------------------------------------------------------
-- 5. capture_token_rotated: the 007g §4 shape, renamed. Self-service
--    rotation is always User-actor-on-self, so (unlike capture_address)
--    no extra column is needed to name which agent rotated — the
--    envelope's own actor_user_id already is that agent.
-- --------------------------------------------------------------------

CREATE TABLE capture_token_rotated (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id UUID REFERENCES app_user (id),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL)),
    on_behalf_of_user_id UUID REFERENCES app_user (id),
    origin TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,
    causation_id UUID,
    corrects_id UUID REFERENCES capture_token_rotated (id)
);
CREATE INDEX capture_token_rotated_org_occurred_idx
    ON capture_token_rotated (organization_id, occurred_at);
CREATE INDEX capture_token_rotated_org_correlation_idx
    ON capture_token_rotated (organization_id, correlation_id);
GRANT SELECT, INSERT ON capture_token_rotated TO crm_app;

CREATE TRIGGER capture_token_rotated_append_only
    BEFORE UPDATE OR DELETE ON capture_token_rotated
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER capture_token_rotated_no_truncate
    BEFORE TRUNCATE ON capture_token_rotated
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

-- --------------------------------------------------------------------
-- Backfill-mint (docs/specs/SLICE_009.md §4 item 1): every currently
-- ACTIVE member gets a capture address now. Migrations are the sanctioned
-- non-application writer (D-021), mirroring 007a's intake_slug/token
-- backfill. sha256() is a PostgreSQL-core function (14+; this project
-- runs 18) — no pgcrypto extension needed. Token collision odds at
-- 12-char/33-symbol entropy (~2^60) are astronomically below any
-- retry-worthiness threshold for a one-time backfill of a handful of
-- rows, so — unlike the Rust mint path's collision retry
-- (domain/capture/address.rs) — this is a single pass, no loop.
--
-- The mint call sits in the `minted` CTE's SELECT list, NOT inside a
-- `LATERAL` subquery: live-verified (2026-08-27, this checkout) that
-- PostgreSQL's planner may evaluate an uncorrelated `CROSS JOIN LATERAL
-- (SELECT volatile_fn())` exactly ONCE and reuse it for every outer row
-- despite `VOLATILE` — silently minting the SAME token for every member.
-- A plain per-row SELECT-list call does not have this failure mode (also
-- live-verified); the CTE materializes each row's token once so the
-- outer INSERT's `token`/`token_lookup` columns read the same already-
-- computed value instead of invoking the function a second time (which
-- would pair `token` with a DIFFERENT random value's digest).
-- --------------------------------------------------------------------

CREATE FUNCTION _mint_capture_token_backfill() RETURNS TEXT AS $$
DECLARE
    alphabet TEXT := 'abcdefghijklmnopqrstuvwxyz234567';
    result TEXT := '';
    i INT;
BEGIN
    FOR i IN 1..12 LOOP
        result := result || substr(alphabet, (floor(random() * length(alphabet)) + 1)::int, 1);
    END LOOP;
    RETURN result;
END;
$$ LANGUAGE plpgsql;

WITH minted AS (
    SELECT m.organization_id, m.user_id, _mint_capture_token_backfill() AS token
    FROM organization_membership m
    WHERE m.status = 'active'
)
INSERT INTO capture_address (organization_id, user_id, token, token_lookup)
SELECT organization_id, user_id, token, sha256(convert_to(token, 'UTF8'))
FROM minted;

DROP FUNCTION _mint_capture_token_backfill();
