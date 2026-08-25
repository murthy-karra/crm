-- Slice 007f (docs/specs/SLICE_007f.md §3): extraction state on
-- raw_payload + the PII-free intake_extraction ledger (D-029 pattern —
-- ids, numbers, and static tags only; never subject/sender/body/
-- extracted values).

ALTER TABLE raw_payload
    ADD COLUMN extraction_attempts INT NOT NULL DEFAULT 0,
    ADD COLUMN extraction_next_attempt_at TIMESTAMPTZ;
GRANT UPDATE (extraction_attempts, extraction_next_attempt_at)
    ON raw_payload TO crm_app;

-- Serves the worker's eligibility scan; extraction_attempts /
-- next_attempt_at act as filters on the (small) matching set.
CREATE INDEX raw_payload_extraction_eligible_idx
    ON raw_payload (received_at)
    WHERE resolution = 'unresolved'
      AND unresolved_reason = 'email_unrecognized_format';

CREATE TABLE intake_extraction (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organization (id),
    raw_payload_id UUID NOT NULL REFERENCES raw_payload (id),
    -- 1-based per raw_payload_id, computed under the raw_payload row
    -- lock (spec §3). INTEGER, not SMALLINT: a transport-failing row
    -- ledgered every minute would overflow SMALLINT in ~22 days.
    seq INTEGER NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN (
        'extracted', 'not_a_lead', 'low_confidence',
        'hallucinated_contact', 'schema_invalid', 'no_contact_method',
        'provider_timeout', 'provider_unavailable', 'rate_limited',
        'malformed_response', 'intake_busy', 'internal_error',
        'superseded')),
    confidence REAL CHECK (confidence >= 0 AND confidence <= 1),
    input_truncated BOOLEAN NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    duration_ms INTEGER NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,
    UNIQUE (raw_payload_id, seq)
);
GRANT SELECT, INSERT ON intake_extraction TO crm_app;

-- Append-only, exactly the operator-ledger pattern (20260824000001).
CREATE TRIGGER intake_extraction_append_only
    BEFORE UPDATE OR DELETE ON intake_extraction
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER intake_extraction_no_truncate
    BEFORE TRUNCATE ON intake_extraction
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
