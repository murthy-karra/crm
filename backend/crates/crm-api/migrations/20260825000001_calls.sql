-- Slice 006 §2: the `call` aggregate (live state, D-007/D-015) and the
-- `call_completed` fact (immutable, PII-free, AGENTS §4.6). No table
-- stores a phone number: `call` references `contact_method_id` as a bare
-- UUID and the dial task reads the number at dial time.

CREATE TABLE call (
    id                 UUID PRIMARY KEY,                 -- generated in Rust; room = 'call:' || id
    organization_id    UUID NOT NULL REFERENCES organization(id),
    person_id          UUID NOT NULL,                    -- bare (SLICE_002 §2 rule)
    contact_method_id  UUID NOT NULL,                    -- bare
    caller_user_id     UUID NOT NULL REFERENCES app_user(id),
    origin             TEXT NOT NULL,                    -- 'web_session' (006); 'operator' in 006b
    correlation_id     UUID NOT NULL,
    status             TEXT NOT NULL CHECK (status IN ('placing','ringing','answered','ended','failed')),
    failure_reason     TEXT CHECK (failure_reason IN ('no_answer','busy','declined','cancelled',
                         'ring_timeout','agent_not_joined','provider_error','expired')),
    end_reason         TEXT CHECK (end_reason IN ('agent_hangup','agent_disconnected','remote_hangup','max_duration','reconciled')),
    provider           TEXT NOT NULL,                    -- 'livekit' | 'scripted'
    provider_room      TEXT NOT NULL,
    provider_call_ref  TEXT,                             -- LiveKit sip_call_id; not PII
    placed_at          TIMESTAMPTZ NOT NULL,
    dial_requested_at  TIMESTAMPTZ,                     -- set once by POST /dial; second dial → 409
    ringing_at         TIMESTAMPTZ,
    answered_at        TIMESTAMPTZ,
    ended_at           TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'failed') = (failure_reason IS NOT NULL)),
    CHECK ((status = 'ended') = (end_reason IS NOT NULL)),
    CHECK (status NOT IN ('ended','failed') OR ended_at IS NOT NULL)
);
CREATE UNIQUE INDEX call_one_active_per_caller
    ON call (organization_id, caller_user_id) WHERE status IN ('placing','ringing','answered');
CREATE INDEX call_org_person_placed_idx ON call (organization_id, person_id, placed_at DESC);
GRANT SELECT, INSERT ON call TO crm_app;
GRANT UPDATE (status, failure_reason, end_reason, provider_call_ref, dial_requested_at,
              ringing_at, answered_at, ended_at, updated_at) ON call TO crm_app;
-- The partial unique index doubles as the sweep's index over active calls.

CREATE TABLE call_completed (
    -- envelope columns, CHECKs, FKs verbatim from contact_attempted (SLICE_002 §2)
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id UUID REFERENCES app_user (id),
    on_behalf_of_user_id UUID REFERENCES app_user (id),
    origin TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,                    -- = ended_at
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,                        -- = call.correlation_id
    causation_id UUID,
    corrects_id UUID REFERENCES call_completed (id),
    call_id UUID NOT NULL,
    person_id UUID NOT NULL,
    contact_method_id UUID NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('reached','no_answer','busy','declined','cancelled',
                                             'ring_timeout','agent_not_joined','provider_error','expired')),
    answered_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ NOT NULL,
    talk_seconds INTEGER,
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);
CREATE INDEX call_completed_org_person_occurred_idx ON call_completed (organization_id, person_id, occurred_at);
CREATE INDEX call_completed_org_correlation_idx ON call_completed (organization_id, correlation_id);
GRANT SELECT, INSERT ON call_completed TO crm_app;

CREATE TRIGGER call_completed_append_only
    BEFORE UPDATE OR DELETE ON call_completed
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TRIGGER call_completed_no_truncate
    BEFORE TRUNCATE ON call_completed
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
