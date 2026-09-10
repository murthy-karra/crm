-- Slice 018 (docs/specs/SLICE_018.md §4): widens operator_proposal to
-- admit the create_task tool alongside start_call, and adds the sidecar
-- operator_task_proposal holding the one model-authored title outside the
-- ledger's content-free rows (D-029; SLICE_006b §2 PII-free precedent).
--
-- Constraint names verified on a fresh migrate (Postgres 18, this lane's
-- scratch database) before being dropped and re-created here:
-- operator_proposal_tool_check (tool), operator_proposal_check
-- (proposed), operator_proposal_check1 (confirmed), operator_proposal_
-- check2 (failed, unchanged), operator_proposal_status_check (unchanged).

ALTER TABLE operator_proposal DROP CONSTRAINT operator_proposal_tool_check;
ALTER TABLE operator_proposal DROP CONSTRAINT operator_proposal_check;
ALTER TABLE operator_proposal DROP CONSTRAINT operator_proposal_check1;

ALTER TABLE operator_proposal ALTER COLUMN contact_method_id DROP NOT NULL;
ALTER TABLE operator_proposal ADD COLUMN task_id UUID;

ALTER TABLE operator_proposal
    ADD CONSTRAINT operator_proposal_tool_check
        CHECK (tool IN ('start_call', 'create_task'));

-- contact_method_id is required for, and only for, a start_call proposal.
ALTER TABLE operator_proposal
    ADD CONSTRAINT operator_proposal_contact_method_id_check
        CHECK ((tool = 'start_call') = (contact_method_id IS NOT NULL));

ALTER TABLE operator_proposal
    ADD CONSTRAINT operator_proposal_check
        CHECK (status <> 'proposed'
               OR (call_id IS NULL AND task_id IS NULL
                   AND failure_code IS NULL AND confirmed_at IS NULL));

ALTER TABLE operator_proposal
    ADD CONSTRAINT operator_proposal_check1
        CHECK (status <> 'confirmed'
               OR (confirmed_at IS NOT NULL
                   AND ((tool = 'start_call' AND call_id IS NOT NULL)
                        OR (tool = 'create_task' AND task_id IS NOT NULL))));

-- Two hygiene CHECKs making the per-tool arms exclusive.
ALTER TABLE operator_proposal
    ADD CONSTRAINT operator_proposal_task_id_tool_check
        CHECK (tool = 'create_task' OR task_id IS NULL);
ALTER TABLE operator_proposal
    ADD CONSTRAINT operator_proposal_call_id_tool_check
        CHECK (tool = 'start_call' OR call_id IS NULL);

-- Additive to the existing column grant (migration 20260827000001).
GRANT UPDATE (task_id) ON operator_proposal TO crm_app;

-- The sidecar: PII-free by construction everywhere except `title`, which
-- carries the model-authored task title (SLICE_006b §2's sidecar
-- precedent). `title`/`kind` CHECKs are the `task` table's verbatim minus
-- the tombstone arm (migration 20260913000001).
CREATE TABLE operator_task_proposal (
    proposal_id      UUID PRIMARY KEY REFERENCES operator_proposal(id) ON DELETE CASCADE,
    organization_id  UUID NOT NULL,
    person_id        UUID NOT NULL,
    title            TEXT NOT NULL
        CHECK (char_length(title) BETWEEN 1 AND 500
               AND title = btrim(title, E' \t\r\n')
               AND position(E'\n' IN title) = 0),
    kind             TEXT NOT NULL
        CHECK (kind IN ('call', 'email', 'text', 'follow_up', 'other')),
    due_at           TIMESTAMPTZ,
    assignee_user_id UUID NOT NULL,
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, assignee_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);

-- No UPDATE, no DELETE for crm_app: retained until the Person is erased
-- (the cascade above), the same class as inquiry.message and the task row
-- itself (docs/specs/SLICE_018.md §4's retention safe default).
GRANT SELECT, INSERT ON operator_task_proposal TO crm_app;
