-- Slice 002: Inquiry (erasable CRUD) and the four D-015 §8 immutable,
-- PII-free fact tables sharing one envelope shape.
--
-- Facts reference erasable rows (person, inquiry, raw_payload) by bare
-- UUID, no FK: D-015 §5 keeps orphaned IDs after erasure, and append-only
-- (below) rules out SET NULL mutating a history row. FK only to rows that
-- are never deleted: organization, app_user, stage (docs/specs/SLICE_002.md
-- §2).

CREATE TABLE inquiry (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    person_id UUID NOT NULL,
    raw_payload_id UUID NOT NULL,
    source TEXT NOT NULL,
    source_external_id TEXT,
    -- Truncated to 4 KiB on insert; the encrypted raw payload keeps the
    -- full text (docs/specs/SLICE_002.md §2, §14 default 11).
    message TEXT,
    received_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (person_id, organization_id) REFERENCES person (id, organization_id) ON DELETE CASCADE
);

CREATE INDEX inquiry_org_person_received_idx ON inquiry (organization_id, person_id, received_at);
CREATE INDEX inquiry_org_received_idx ON inquiry (organization_id, received_at);

GRANT SELECT, INSERT ON inquiry TO crm_app;

-- --------------------------------------------------------------------
-- Fact envelope, identical on all four tables (docs/specs/SLICE_002.md §2):
-- id, organization_id, actor_kind/actor_user_id/on_behalf_of_user_id,
-- origin, occurred_at/recorded_at, correlation_id/causation_id,
-- corrects_id (D-015 §2 fix-forward hook; nothing writes it yet).
-- --------------------------------------------------------------------

CREATE TABLE inquiry_received (
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
    corrects_id UUID REFERENCES inquiry_received (id),
    inquiry_id UUID NOT NULL,
    person_id UUID NOT NULL,
    raw_payload_id UUID NOT NULL,
    content_hmac BYTEA NOT NULL,
    source TEXT NOT NULL,
    person_created BOOLEAN NOT NULL,
    matched_by TEXT,
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX inquiry_received_org_person_occurred_idx ON inquiry_received (organization_id, person_id, occurred_at);
CREATE INDEX inquiry_received_org_correlation_idx ON inquiry_received (organization_id, correlation_id);

GRANT SELECT, INSERT ON inquiry_received TO crm_app;

CREATE TABLE routing_decision (
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
    corrects_id UUID REFERENCES routing_decision (id),
    inquiry_id UUID NOT NULL,
    person_id UUID NOT NULL,
    strategy TEXT NOT NULL CHECK (strategy IN ('explicit', 'actor_default', 'kept_existing')),
    assignee_user_id UUID REFERENCES app_user (id),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX routing_decision_org_person_occurred_idx ON routing_decision (organization_id, person_id, occurred_at);
CREATE INDEX routing_decision_org_correlation_idx ON routing_decision (organization_id, correlation_id);

GRANT SELECT, INSERT ON routing_decision TO crm_app;

CREATE TABLE assignment_changed (
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
    corrects_id UUID REFERENCES assignment_changed (id),
    person_id UUID NOT NULL,
    from_user_id UUID REFERENCES app_user (id),
    to_user_id UUID REFERENCES app_user (id),
    reason TEXT NOT NULL,
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX assignment_changed_org_person_occurred_idx ON assignment_changed (organization_id, person_id, occurred_at);
CREATE INDEX assignment_changed_org_correlation_idx ON assignment_changed (organization_id, correlation_id);

GRANT SELECT, INSERT ON assignment_changed TO crm_app;

CREATE TABLE stage_changed (
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
    corrects_id UUID REFERENCES stage_changed (id),
    person_id UUID NOT NULL,
    from_stage_id UUID,
    to_stage_id UUID NOT NULL,
    reason TEXT NOT NULL,
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL)),
    FOREIGN KEY (from_stage_id, organization_id) REFERENCES stage (id, organization_id),
    FOREIGN KEY (to_stage_id, organization_id) REFERENCES stage (id, organization_id)
);

CREATE INDEX stage_changed_org_person_occurred_idx ON stage_changed (organization_id, person_id, occurred_at);
CREATE INDEX stage_changed_org_correlation_idx ON stage_changed (organization_id, correlation_id);

GRANT SELECT, INSERT ON stage_changed TO crm_app;

-- --------------------------------------------------------------------
-- Append-only enforcement (docs/specs/SLICE_002.md §2). Two layers, both
-- required: crm_app's grants above carry no UPDATE/DELETE, and this
-- trigger blocks the migrator role, fixtures, and future scripts too. A
-- FOR EACH ROW trigger only fires against an existing row, so it does not
-- interfere with corrects_id fix-forward INSERTs (D-015 §2) — it never
-- fires on INSERT at all.
--
-- TRUNCATE is a third mutation path distinct from UPDATE/DELETE: Postgres
-- row-level triggers never fire on TRUNCATE (there are no individual rows
-- to iterate), so a BEFORE UPDATE OR DELETE FOR EACH ROW trigger alone
-- lets any role with TRUNCATE privilege (crm_migrator, as schema owner)
-- silently empty a fact table. TRUNCATE only supports statement-level
-- triggers, so each table gets a second trigger binding — same function,
-- same "always reject" body, which needs no TRUNCATE-specific logic.
-- --------------------------------------------------------------------

CREATE FUNCTION reject_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'history table % is append-only', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER inquiry_received_append_only
    BEFORE UPDATE OR DELETE ON inquiry_received
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER inquiry_received_no_truncate
    BEFORE TRUNCATE ON inquiry_received
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER routing_decision_append_only
    BEFORE UPDATE OR DELETE ON routing_decision
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER routing_decision_no_truncate
    BEFORE TRUNCATE ON routing_decision
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER assignment_changed_append_only
    BEFORE UPDATE OR DELETE ON assignment_changed
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER assignment_changed_no_truncate
    BEFORE TRUNCATE ON assignment_changed
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER stage_changed_append_only
    BEFORE UPDATE OR DELETE ON stage_changed
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER stage_changed_no_truncate
    BEFORE TRUNCATE ON stage_changed
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
