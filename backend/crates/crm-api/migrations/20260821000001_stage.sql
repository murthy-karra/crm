-- Slice 002: Person stages (D-019) — a per-Organization list, not a fixed
-- enum. Seeded via the stage::seed_defaults library helper (never by the
-- application; crm_app has SELECT only).

CREATE TABLE stage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    name TEXT NOT NULL,
    position SMALLINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, name),
    UNIQUE (organization_id, position),
    -- Enables composite FKs from person/stage_changed so a stage from
    -- another Organization can never be persisted, independent of any
    -- application check (docs/specs/SLICE_002.md §2).
    UNIQUE (id, organization_id)
);

GRANT SELECT ON stage TO crm_app;
