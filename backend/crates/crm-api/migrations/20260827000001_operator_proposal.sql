-- Slice 006b (docs/specs/SLICE_006b.md §2): the Operator's `start_call`
-- proposal — live aggregate state (D-007), not a fact. PII-free (D-029):
-- ids, timestamps, and code strings only. Lifecycle:
-- proposed → claimed → confirmed | failed; a row that leaves 'proposed'
-- is consumed forever (a crash between claim and finalize leaves
-- 'claimed', which reads as consumed).
CREATE TABLE operator_proposal (
    id                UUID PRIMARY KEY,
    organization_id   UUID NOT NULL REFERENCES organization(id),
    actor_user_id     UUID NOT NULL REFERENCES app_user(id),
    -- operator_turn.id; no FK: the ledger insert may fail after the turn
    -- (docs/specs/SLICE_005.md §9) and the proposal must still work.
    turn_id           UUID NOT NULL,
    tool              TEXT NOT NULL CHECK (tool = 'start_call'),
    person_id         UUID NOT NULL,
    contact_method_id UUID NOT NULL,
    status            TEXT NOT NULL CHECK (status IN ('proposed','claimed','confirmed','failed')),
    -- a CallError kind() string; code-only by construction (D-029)
    failure_code      TEXT,
    call_id           UUID,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- computed server-side (now() + TTL) so expiry and creation share a clock
    expires_at        TIMESTAMPTZ NOT NULL,
    confirmed_at      TIMESTAMPTZ,
    CHECK (status <> 'proposed' OR (call_id IS NULL AND failure_code IS NULL AND confirmed_at IS NULL)),
    CHECK (status <> 'confirmed' OR (call_id IS NOT NULL AND confirmed_at IS NOT NULL)),
    CHECK (status <> 'failed' OR failure_code IS NOT NULL)
);

GRANT SELECT, INSERT ON operator_proposal TO crm_app;
GRANT UPDATE (status, failure_code, call_id, confirmed_at) ON operator_proposal TO crm_app;
