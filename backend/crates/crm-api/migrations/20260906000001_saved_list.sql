-- Slice 011b: named, dynamic People criteria. These are ordinary relational
-- configuration rows, not immutable history facts: deletion keeps a minimal
-- tombstone so a delayed create retry cannot resurrect a definition.

CREATE TABLE saved_list (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    created_by_user_id UUID NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('personal', 'shared')),
    name TEXT,
    filter JSONB,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    create_request_id UUID NOT NULL,
    create_fingerprint BYTEA NOT NULL CHECK (octet_length(create_fingerprint) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    FOREIGN KEY (organization_id, created_by_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    CHECK (
        (deleted_at IS NULL
         AND name IS NOT NULL
         AND char_length(name) BETWEEN 1 AND 80
         AND filter IS NOT NULL
         AND jsonb_typeof(filter) = 'object')
        OR
        (deleted_at IS NOT NULL AND name IS NULL AND filter IS NULL)
    ),
    UNIQUE (organization_id, created_by_user_id, create_request_id)
);

-- The index read is metadata-only and actor-filtered in application SQL. The
-- partial indexes cover the two live scopes while tombstones retain retry
-- identity without consuming quota or normal-read scan space.
CREATE INDEX saved_list_live_org_scope_created_idx
    ON saved_list (organization_id, scope, created_at, id)
    WHERE deleted_at IS NULL;
CREATE INDEX saved_list_live_personal_owner_created_idx
    ON saved_list (organization_id, created_by_user_id, created_at, id)
    WHERE deleted_at IS NULL AND scope = 'personal';

-- Deliberately no DELETE/TRUNCATE privilege: application deletion clears
-- content into a tombstone via UPDATE, preserving no-resurrection retries.
GRANT SELECT, INSERT, UPDATE ON saved_list TO crm_app;
