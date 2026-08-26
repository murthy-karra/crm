-- Slice 007g (docs/specs/SLICE_007g.md §4): token rotation. The 007a
-- migration deliberately withheld this grant ("rotation is a later
-- rung") — this is that rung.

GRANT UPDATE (intake_token) ON organization TO crm_app;

-- The rotation audit fact: the standard envelope, NEVER a token value
-- (old or new — there is no column for one).
CREATE TABLE intake_token_rotated (
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
    corrects_id UUID REFERENCES intake_token_rotated (id)
);
CREATE INDEX intake_token_rotated_org_occurred_idx
    ON intake_token_rotated (organization_id, occurred_at);
CREATE INDEX intake_token_rotated_org_correlation_idx
    ON intake_token_rotated (organization_id, correlation_id);
GRANT SELECT, INSERT ON intake_token_rotated TO crm_app;

CREATE TRIGGER intake_token_rotated_append_only
    BEFORE UPDATE OR DELETE ON intake_token_rotated
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER intake_token_rotated_no_truncate
    BEFORE TRUNCATE ON intake_token_rotated
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
