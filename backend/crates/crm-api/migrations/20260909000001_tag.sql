-- Slice 011e (e1): free-form tags on People. Ordinary relational CRUD
-- (AGENTS.md §4.6) — no tombstone, no fact table, no history.

CREATE TABLE tag (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    name TEXT NOT NULL
        CHECK (char_length(name) BETWEEN 1 AND 40 AND name = btrim(name)),
    created_by_user_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Composite-FK anchor (stage / person convention).
    UNIQUE (id, organization_id),
    FOREIGN KEY (organization_id, created_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
-- Case-insensitive uniqueness per Organization; ON CONFLICT infers it.
CREATE UNIQUE INDEX tag_org_lower_name_key ON tag (organization_id, lower(name));

CREATE TABLE person_tag (
    organization_id UUID NOT NULL,
    person_id UUID NOT NULL,
    tag_id UUID NOT NULL,
    added_by_user_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, person_id, tag_id),
    -- A row for a Person or tag of another Organization can never be
    -- persisted, even if an application check regresses.
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id, organization_id)
        REFERENCES tag (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, added_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
-- Usage counts, delete, and a tag-led probe.
CREATE INDEX person_tag_org_tag_person_idx ON person_tag (organization_id, tag_id, person_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON tag TO crm_app;
GRANT SELECT, INSERT, DELETE ON person_tag TO crm_app;
