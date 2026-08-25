-- Slice 007c (docs/specs/SLICE_007c.md §3): the Organization's default
-- assignee for unattended (system-actor) intake, and the widened
-- routing_decision.strategy CHECK for the two new strategies it can
-- produce. No backfill: NULL is the correct initial state for every
-- existing Organization.
ALTER TABLE organization
    ADD COLUMN intake_default_assignee_user_id UUID NULL
        REFERENCES app_user (id);
GRANT UPDATE (intake_default_assignee_user_id, updated_at)
    ON organization TO crm_app;

ALTER TABLE routing_decision
    DROP CONSTRAINT routing_decision_strategy_check;
ALTER TABLE routing_decision ADD CONSTRAINT routing_decision_strategy_check
    CHECK (strategy IN ('explicit', 'actor_default', 'kept_existing',
                        'organization_default', 'unassigned'));
