-- Slice 004: administration (D-021, D-026, D-027, O-007).
-- Roles/status on membership, the platform-admin allowlist, invitations,
-- and four admin fact tables sharing the same envelope and append-only
-- discipline as the Slice 002/003 fact tables. docs/specs/SLICE_004.md §2
-- (verbatim contract).

ALTER TABLE organization_membership
    ADD COLUMN role TEXT NOT NULL DEFAULT 'member'
        CHECK (role IN ('admin', 'member')),
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'inactive')),
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE organization_membership
    ALTER COLUMN role DROP DEFAULT, ALTER COLUMN status DROP DEFAULT;
-- Explicit writes only from here on; the defaults existed solely to
-- backfill pre-004 rows. dev-bootstrap promotes Alice/Bob afterwards.

ALTER TABLE organization
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active'));   -- D-027 §5; widened by O-009
CREATE UNIQUE INDEX organization_name_lower_idx ON organization (lower(name));

ALTER TABLE user_session ALTER COLUMN active_organization_id DROP NOT NULL;
-- NULL = platform-admin session with no active Organization (§7).

CREATE TABLE platform_admin (
    user_id UUID PRIMARY KEY REFERENCES app_user (id),
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    granted_via TEXT NOT NULL CHECK (granted_via IN ('cli'))
);

CREATE TABLE invitation (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    email TEXT NOT NULL,                 -- normalized: trimmed, lowercased
    role TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    token_hash TEXT NOT NULL,            -- SHA-256, base64url, of the raw token
    invited_by_user_id UUID NOT NULL REFERENCES app_user (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    accepted_user_id UUID REFERENCES app_user (id),
    revoked_at TIMESTAMPTZ,
    revoke_reason TEXT CHECK (revoke_reason IN ('revoked', 'superseded')),
    CHECK ((accepted_at IS NULL) = (accepted_user_id IS NULL)),
    CHECK ((revoked_at IS NULL) = (revoke_reason IS NULL)),
    CHECK (accepted_at IS NULL OR revoked_at IS NULL)
);
CREATE UNIQUE INDEX invitation_token_hash_idx ON invitation (token_hash);
CREATE UNIQUE INDEX invitation_open_per_org_email_idx
    ON invitation (organization_id, email)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;
CREATE INDEX invitation_org_created_idx ON invitation (organization_id, created_at);

-- --------------------------------------------------------------------
-- Four typed admin fact tables (docs/specs/SLICE_004.md §2), same envelope
-- shape and append-only discipline (reject_mutation(), created in
-- 20260821000004_inquiry_and_facts.sql) as every other fact table.
-- --------------------------------------------------------------------

CREATE TABLE organization_created (
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
    corrects_id UUID REFERENCES organization_created (id),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX organization_created_org_occurred_idx ON organization_created (organization_id, occurred_at);
CREATE INDEX organization_created_org_correlation_idx ON organization_created (organization_id, correlation_id);

GRANT SELECT, INSERT ON organization_created TO crm_app;

CREATE TRIGGER organization_created_append_only
    BEFORE UPDATE OR DELETE ON organization_created
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER organization_created_no_truncate
    BEFORE TRUNCATE ON organization_created
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TABLE invitation_issued (
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
    corrects_id UUID REFERENCES invitation_issued (id),
    invitation_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    superseded_invitation_id UUID,
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX invitation_issued_org_occurred_idx ON invitation_issued (organization_id, occurred_at);
CREATE INDEX invitation_issued_org_correlation_idx ON invitation_issued (organization_id, correlation_id);

GRANT SELECT, INSERT ON invitation_issued TO crm_app;

CREATE TRIGGER invitation_issued_append_only
    BEFORE UPDATE OR DELETE ON invitation_issued
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER invitation_issued_no_truncate
    BEFORE TRUNCATE ON invitation_issued
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TABLE invitation_resolved (
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
    corrects_id UUID REFERENCES invitation_resolved (id),
    invitation_id UUID NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('accepted', 'revoked', 'superseded')),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX invitation_resolved_org_occurred_idx ON invitation_resolved (organization_id, occurred_at);
CREATE INDEX invitation_resolved_org_correlation_idx ON invitation_resolved (organization_id, correlation_id);

GRANT SELECT, INSERT ON invitation_resolved TO crm_app;

CREATE TRIGGER invitation_resolved_append_only
    BEFORE UPDATE OR DELETE ON invitation_resolved
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER invitation_resolved_no_truncate
    BEFORE TRUNCATE ON invitation_resolved
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TABLE membership_changed (
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
    corrects_id UUID REFERENCES membership_changed (id),
    user_id UUID NOT NULL,
    from_role TEXT CHECK (from_role IN ('admin', 'member')),
    to_role TEXT NOT NULL CHECK (to_role IN ('admin', 'member')),
    from_status TEXT CHECK (from_status IN ('active', 'inactive')),
    to_status TEXT NOT NULL CHECK (to_status IN ('active', 'inactive')),
    reason TEXT NOT NULL CHECK (reason IN
        ('invitation', 'bootstrap', 'promote', 'demote', 'deactivate', 'reactivate')),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL)),
    CHECK ((from_role IS NULL) = (from_status IS NULL))
);

CREATE INDEX membership_changed_org_occurred_idx ON membership_changed (organization_id, occurred_at);
CREATE INDEX membership_changed_org_correlation_idx ON membership_changed (organization_id, correlation_id);

GRANT SELECT, INSERT ON membership_changed TO crm_app;

CREATE TRIGGER membership_changed_append_only
    BEFORE UPDATE OR DELETE ON membership_changed
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER membership_changed_no_truncate
    BEFORE TRUNCATE ON membership_changed
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

-- --------------------------------------------------------------------
-- Grants (docs/specs/SLICE_004.md §2). crm_app still has no DELETE
-- anywhere.
-- --------------------------------------------------------------------

GRANT INSERT ON organization, app_user, local_credential,
                organization_membership, invitation, stage TO crm_app;
-- Deviation from SLICE_004.md §2's literal grant list, flagged in the Lane
-- A report per AGENTS.md §11 rather than applied silently: `invitation` is
-- a brand-new table this migration creates, and the spec's grant block
-- above gives crm_app INSERT (and, below, column-UPDATE) on it but never
-- SELECT — unlike every other table in that INSERT line, which already had
-- SELECT from an earlier migration. Without SELECT, every invitation read
-- path (list, preview, revoke's FOR UPDATE lookup, accept's lookup, the
-- AlreadyMember/open-invitation checks) fails with a bare "permission
-- denied for table invitation", so the entire invitation mechanism is
-- inoperable as literally specified. Confirmed against a live crm_app
-- connection before adding this line.
GRANT SELECT ON invitation TO crm_app;
GRANT UPDATE (role, status, updated_at) ON organization_membership TO crm_app;
GRANT UPDATE (accepted_at, accepted_user_id, revoked_at, revoke_reason)
    ON invitation TO crm_app;          -- token_hash/email/expires_at immutable
GRANT UPDATE (password_hash, updated_at) ON local_credential TO crm_app; -- accept, CLI set-password
GRANT SELECT ON platform_admin TO crm_app;   -- read-only: cannot mint
GRANT SELECT, INSERT ON organization_created, invitation_issued,
                       invitation_resolved, membership_changed TO crm_app;
