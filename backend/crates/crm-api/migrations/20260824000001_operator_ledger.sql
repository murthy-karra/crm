-- Slice 005 §2: the Operator turn ledger (D-029). PII-free by
-- construction: no column holds the message, the reply, tool arguments,
-- search strings, or history; `person_ids` are ids only. `operator_turn`
-- follows the fact envelope (columns, CHECKs, FKs copied verbatim from
-- `contact_attempted`, SLICE_002 §2) so it reads like every other history
-- table; `actor_kind = 'user'`, `origin = 'operator'` in this slice.
-- Both tables carry the same append-only discipline.

CREATE TABLE operator_turn (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organization (id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id UUID REFERENCES app_user (id),
    on_behalf_of_user_id UUID REFERENCES app_user (id),
    origin TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,                -- turn started
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,                    -- = id
    causation_id UUID,
    corrects_id UUID REFERENCES operator_turn (id),
    completed_at TIMESTAMPTZ NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN (
        'completed', 'tool_budget_exhausted', 'malformed_tool_call',
        'model_timeout', 'turn_timeout', 'provider_error', 'tool_error')),
    provider TEXT NOT NULL,                          -- 'groq' | 'scripted'
    model TEXT NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    model_call_count INTEGER NOT NULL,
    tool_call_count INTEGER NOT NULL,
    context_route TEXT CHECK (context_route IN ('today', 'person', 'people', 'other')),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);

CREATE INDEX operator_turn_org_occurred_idx ON operator_turn (organization_id, occurred_at DESC);

CREATE TABLE operator_tool_call (
    turn_id UUID NOT NULL REFERENCES operator_turn (id),
    seq SMALLINT NOT NULL,
    tool_name TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('ok', 'not_found', 'invalid_arguments', 'error')),
    duration_ms INTEGER NOT NULL,
    person_ids UUID[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (turn_id, seq)
);

GRANT SELECT, INSERT ON operator_turn, operator_tool_call TO crm_app;

CREATE TRIGGER operator_turn_append_only
    BEFORE UPDATE OR DELETE ON operator_turn
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER operator_turn_no_truncate
    BEFORE TRUNCATE ON operator_turn
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER operator_tool_call_append_only
    BEFORE UPDATE OR DELETE ON operator_tool_call
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER operator_tool_call_no_truncate
    BEFORE TRUNCATE ON operator_tool_call
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
