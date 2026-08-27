-- Slice 008 (docs/specs/SLICE_008.md §3; D-041): intake routing modes —
-- default_assignee | round_robin | unassigned — plus the round-robin
-- pointer table and the routing_decision.strategy CHECK widening for the
-- new 'round_robin' fact value.

-- Named now (unlike the auto-generated 007c-era CHECK names) so a future
-- 'rules' widening is a clean DROP/ADD.
ALTER TABLE organization
    ADD COLUMN intake_routing_mode TEXT NOT NULL DEFAULT 'unassigned';
ALTER TABLE organization ADD CONSTRAINT organization_intake_routing_mode_check
    CHECK (intake_routing_mode IN ('default_assignee', 'round_robin', 'unassigned'));

-- Deterministic migration mapping (D-041): an existing Organization with a
-- default assignee already configured keeps behaving exactly as before
-- under the new three-mode model; NULL-assignee orgs and brand-new orgs
-- stay 'unassigned' via the column DEFAULT above (mirrors today's initial
-- no-default state and its warning).
UPDATE organization SET intake_routing_mode = 'default_assignee'
    WHERE intake_default_assignee_user_id IS NOT NULL;

GRANT UPDATE (intake_routing_mode) ON organization TO crm_app;

-- The round-robin pointer. Deliberately NOT a column on `organization`:
-- bumping it per lead must not rewrite `organization.updated_at`, contend
-- with the admin settings PUT's row, or widen the `organization` UPDATE
-- grant beyond the one mode column above. Mutable operational state, not
-- history — no append-only triggers. Plain FK to `app_user` (007c §3
-- precedent: active membership is a domain-layer concern, re-checked at
-- read/rotation time, not a schema constraint).
CREATE TABLE intake_rotation (
    organization_id UUID PRIMARY KEY REFERENCES organization (id),
    last_assigned_user_id UUID NOT NULL REFERENCES app_user (id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
GRANT SELECT, INSERT, UPDATE ON intake_rotation TO crm_app;

-- The declared persistence-contract change (pointer amendments into
-- SLICE_002 §5 and SLICE_007c §5 ride with this spec): the fact
-- vocabulary gains 'round_robin'.
ALTER TABLE routing_decision
    DROP CONSTRAINT routing_decision_strategy_check;
ALTER TABLE routing_decision ADD CONSTRAINT routing_decision_strategy_check
    CHECK (strategy IN ('explicit', 'actor_default', 'kept_existing',
                        'organization_default', 'unassigned', 'round_robin'));
