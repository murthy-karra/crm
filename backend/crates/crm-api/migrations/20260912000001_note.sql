-- Slice 015: free-text notes on a Person. Ordinary relational CRUD
-- (AGENTS.md §4.6, D-053) — erasable, cascaded with the Person, never a
-- history fact. Delete is a tombstone (body '', deleted_at,
-- deleted_by_user_id): the future import's idempotency key must survive a
-- re-run, and no body lingers in the row (O-012, O-013).

CREATE TABLE note (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    person_id UUID NOT NULL,
    -- NULL only for an imported note whose FUB author matched no member.
    author_user_id UUID REFERENCES app_user (id),
    body TEXT NOT NULL,
    -- Origin::as_str: 'web_session' | 'operator' | 'migration' | ...
    origin TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    -- Import provenance (Slice 010 notes rung); NULL for notes written here.
    source TEXT,
    source_external_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    deleted_by_user_id UUID REFERENCES app_user (id),
    CHECK (author_user_id IS NOT NULL OR origin = 'migration'),
    CHECK ((source IS NULL) = (source_external_id IS NULL)),
    CHECK (
        (deleted_at IS NULL
            AND char_length(body) BETWEEN 1 AND 10000
            AND body = btrim(body, E' \t\r\n'))
        OR (deleted_at IS NOT NULL AND body = '')
    ),
    CHECK ((deleted_at IS NULL) = (deleted_by_user_id IS NULL)),
    CHECK (updated_at >= created_at),
    -- Composite-FK anchor (stage / person / tag convention).
    UNIQUE (id, organization_id),
    -- A note for a Person of another Organization can never be persisted,
    -- even if an application check regresses. Erasure is the Person cascade.
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, author_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, deleted_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
-- The detail read: one Person's live notes in creation order.
CREATE INDEX note_org_person_created_idx
    ON note (organization_id, person_id, created_at, id);
-- Import idempotency: a re-run of the notes rung inserts zero rows, and a
-- tombstoned imported note is never resurrected.
CREATE UNIQUE INDEX note_org_source_external_idx
    ON note (organization_id, source, source_external_id)
    WHERE source_external_id IS NOT NULL;

-- No DELETE: tombstones are UPDATEs; erasure is the Person cascade, which
-- runs as the table owner and needs no grant.
GRANT SELECT, INSERT, UPDATE ON note TO crm_app;
