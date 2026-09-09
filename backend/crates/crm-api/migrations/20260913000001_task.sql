-- Slice 016a: typed tasks on a Person. Ordinary relational CRUD
-- (AGENTS.md §4.6, D-053/D-054) — erasable, cascaded with the Person,
-- never a history fact except the derived `task_completed` timeline
-- projection. Delete is a tombstone (title '', deleted_at,
-- deleted_by_user_id): the future import's idempotency key must survive a
-- re-run, and no title lingers in the row (O-012, O-013). Both indexes
-- ship here (the Person detail/timeline order and the Today axis' bound
-- scan) so 016b adds none.

CREATE TABLE task (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    person_id UUID NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'follow_up'
        CHECK (kind IN ('call', 'email', 'text', 'follow_up', 'other')),
    due_at TIMESTAMPTZ,
    -- NULL only for an imported task whose FUB assignee matched no member.
    assignee_user_id UUID REFERENCES app_user (id),
    -- NULL only for an imported task whose FUB creator matched no member.
    created_by_user_id UUID REFERENCES app_user (id),
    completed_at TIMESTAMPTZ,
    completed_by_user_id UUID REFERENCES app_user (id),
    -- Origin::as_str: 'web_session' | 'operator' | 'migration' | ...
    origin TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    -- Import provenance (Slice 010 tasks rung); NULL for tasks created here.
    source TEXT,
    source_external_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    deleted_by_user_id UUID REFERENCES app_user (id),
    CHECK (assignee_user_id IS NOT NULL OR origin = 'migration'),
    CHECK (created_by_user_id IS NOT NULL OR origin = 'migration'),
    CHECK (completed_by_user_id IS NULL OR completed_at IS NOT NULL),
    CHECK (completed_at IS NULL OR completed_by_user_id IS NOT NULL
           OR origin = 'migration'),
    CHECK ((source IS NULL) = (source_external_id IS NULL)),
    CHECK (
        (deleted_at IS NULL
            AND char_length(title) BETWEEN 1 AND 500
            AND title = btrim(title, E' \t\r\n')
            AND position(E'\n' IN title) = 0)
        OR (deleted_at IS NOT NULL AND title = '')
    ),
    CHECK ((deleted_at IS NULL) = (deleted_by_user_id IS NULL)),
    CHECK (updated_at >= created_at),
    UNIQUE (id, organization_id),
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, assignee_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, created_by_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, completed_by_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, deleted_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
-- The Person detail and timeline: one Person's tasks by due time.
CREATE INDEX task_org_person_due_idx
    ON task (organization_id, person_id, due_at, id);
-- The Today axis and the panel: the viewer's open, live, dated tasks.
CREATE INDEX task_org_assignee_due_open_idx
    ON task (organization_id, assignee_user_id, due_at, id)
    WHERE completed_at IS NULL AND deleted_at IS NULL AND due_at IS NOT NULL;
-- Import idempotency and the tombstone resurrection guard.
CREATE UNIQUE INDEX task_org_source_external_idx
    ON task (organization_id, source, source_external_id)
    WHERE source_external_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE ON task TO crm_app;
