-- Slice 003 §2: the fifth typed fact table (D-022), same envelope and
-- append-only discipline as the four from Slice 002. PII-free, no free
-- text. `person_id` is a bare UUID, no FK, per the SLICE_002 §2 rule for
-- facts referencing erasable rows.

CREATE TABLE contact_attempted (
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
    corrects_id UUID REFERENCES contact_attempted (id),
    person_id UUID NOT NULL,   -- bare UUID, no FK (SLICE_002 §2 rule)
    channel TEXT NOT NULL CHECK (channel IN ('call', 'text', 'email', 'other')),
    outcome TEXT NOT NULL CHECK (outcome IN ('reached', 'no_answer', 'left_message', 'sent')),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX contact_attempted_org_person_occurred_idx
    ON contact_attempted (organization_id, person_id, occurred_at);
CREATE INDEX contact_attempted_org_correlation_idx
    ON contact_attempted (organization_id, correlation_id);

GRANT SELECT, INSERT ON contact_attempted TO crm_app;

CREATE TRIGGER contact_attempted_append_only
    BEFORE UPDATE OR DELETE ON contact_attempted
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER contact_attempted_no_truncate
    BEFORE TRUNCATE ON contact_attempted
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

-- Today (docs/specs/SLICE_003.md §3, §4): "People assigned to me". IS NULL
-- lookups (unassigned People) are served by the same btree.
CREATE INDEX person_org_assignee_idx ON person (organization_id, assigned_user_id);
