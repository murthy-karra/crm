-- Slice 002: Person and contact methods — erasable CRUD set (D-015 §3/§6).

CREATE TABLE person (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    first_name TEXT,
    last_name TEXT,
    stage_id UUID NOT NULL,
    assigned_user_id UUID REFERENCES app_user (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Enables composite FKs from contact_method/inquiry/stage_changed.
    UNIQUE (id, organization_id),
    -- A stage from another Organization can never be persisted, even if an
    -- application check regresses (docs/specs/SLICE_002.md §2).
    FOREIGN KEY (stage_id, organization_id) REFERENCES stage (id, organization_id)
    -- assigned_user_id is deliberately NOT composite-FK'd to
    -- organization_membership: that would make membership deletion fail
    -- while anyone is assigned and pre-decide O-004 (docs/specs/SLICE_002.md §6).
);

CREATE INDEX person_organization_created_idx ON person (organization_id, created_at);

CREATE TABLE contact_method (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL,
    person_id UUID NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('email', 'phone')),
    value TEXT NOT NULL,
    normalized_value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (person_id, organization_id) REFERENCES person (id, organization_id) ON DELETE CASCADE,
    -- Deliberately not unique per Organization: shared household emails and
    -- phones are real (docs/specs/SLICE_002.md §2).
    UNIQUE (person_id, kind, normalized_value)
);

-- The identify (dedup) lookup.
CREATE INDEX contact_method_lookup_idx ON contact_method (organization_id, kind, normalized_value);

GRANT SELECT, INSERT, UPDATE ON person TO crm_app;
GRANT SELECT, INSERT ON contact_method TO crm_app;
