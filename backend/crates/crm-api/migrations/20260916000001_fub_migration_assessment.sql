-- Slice 010a: bounded, read-only FUB connection assessment.  This is a
-- dedicated encrypted capture store; migration probe data must never enter
-- raw_payload, whose intake worker is permitted to inspect its rows.

CREATE TABLE migration_connection (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organization(id),
    source_account_id BIGINT NOT NULL CHECK (source_account_id > 0),
    identity_revision INTEGER NOT NULL DEFAULT 1,
    identity_nonce BYTEA NOT NULL,
    identity_ciphertext BYTEA NOT NULL,
    credential_nonce BYTEA,
    credential_ciphertext BYTEA,
    status TEXT NOT NULL CHECK (status IN ('connected', 'disconnected')),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    initiated_by_user_id UUID NOT NULL REFERENCES app_user(id),
    disconnected_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id),
    UNIQUE (id, organization_id),
    CHECK ((status = 'connected') = (credential_nonce IS NOT NULL AND credential_ciphertext IS NOT NULL)),
    CHECK ((status = 'disconnected') = (credential_nonce IS NULL AND credential_ciphertext IS NULL))
);

CREATE TABLE migration_assessment (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL,
    connection_id UUID NOT NULL,
    connection_revision INTEGER NOT NULL,
    profile_version TEXT NOT NULL,
    source_account_id BIGINT NOT NULL,
    identity_nonce BYTEA NOT NULL,
    identity_ciphertext BYTEA NOT NULL,
    initiated_by_user_id UUID NOT NULL REFERENCES app_user(id),
    state TEXT NOT NULL CHECK (state IN ('queued','running','waiting_retry','paused','completed','cancelled')),
    pause_reason TEXT,
    next_attempt_at TIMESTAMPTZ,
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, organization_id),
    FOREIGN KEY (connection_id, organization_id) REFERENCES migration_connection(id, organization_id),
    CHECK ((state = 'paused') = (pause_reason IS NOT NULL)),
    CHECK ((lease_token IS NULL) = (lease_expires_at IS NULL))
);
CREATE UNIQUE INDEX migration_assessment_one_active_per_org
    ON migration_assessment(organization_id)
    WHERE state IN ('queued','running','waiting_retry');
CREATE INDEX migration_assessment_due_claim_idx
    ON migration_assessment(next_attempt_at, created_at)
    WHERE state IN ('queued','waiting_retry','running');
CREATE INDEX migration_assessment_org_latest_idx
    ON migration_assessment(organization_id, created_at DESC, id DESC);
CREATE INDEX migration_assessment_org_report_idx ON migration_assessment(organization_id, created_at DESC, id DESC) WHERE state='completed';
CREATE INDEX migration_assessment_org_active_idx ON migration_assessment(organization_id, created_at DESC, id DESC) WHERE state IN ('queued','running','waiting_retry','paused');

CREATE TABLE migration_assessment_evidence (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL,
    assessment_id UUID NOT NULL,
    check_key TEXT NOT NULL,
    http_status INTEGER NOT NULL CHECK (http_status BETWEEN 100 AND 599),
    byte_len INTEGER NOT NULL CHECK (byte_len >= 0),
    nonce BYTEA NOT NULL,
    ciphertext BYTEA NOT NULL,
    content_hmac BYTEA NOT NULL CHECK (octet_length(content_hmac) = 32),
    source_version TEXT,
    classification TEXT NOT NULL,
    truncated BOOLEAN NOT NULL DEFAULT false,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, organization_id),
    UNIQUE (id, organization_id, assessment_id, check_key),
    FOREIGN KEY (assessment_id, organization_id) REFERENCES migration_assessment(id, organization_id)
);

CREATE TABLE migration_assessment_check (
    assessment_id UUID NOT NULL,
    organization_id UUID NOT NULL,
    check_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending','running','waiting_retry','paused','completed','cancelled')),
    reported_total NUMERIC(30, 0),
    retrieved_count INTEGER NOT NULL DEFAULT 0 CHECK (retrieved_count >= 0),
    coverage TEXT NOT NULL CHECK (coverage IN ('partial','unavailable','not_checked','complete_for_query')),
    error_code TEXT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    cycle_attempts INTEGER NOT NULL DEFAULT 0 CHECK (cycle_attempts >= 0),
    next_attempt_at TIMESTAMPTZ,
    evidence_id UUID,
    observed_at TIMESTAMPTZ,
    PRIMARY KEY (assessment_id, check_key),
    FOREIGN KEY (assessment_id, organization_id) REFERENCES migration_assessment(id, organization_id),
    FOREIGN KEY (evidence_id, organization_id, assessment_id, check_key) REFERENCES migration_assessment_evidence(id, organization_id, assessment_id, check_key),
    CHECK (reported_total IS NULL OR reported_total >= 0)
);

CREATE TABLE migration_request_receipt (
    organization_id UUID NOT NULL REFERENCES organization(id),
    operation TEXT NOT NULL,
    request_id UUID NOT NULL,
    digest BYTEA NOT NULL CHECK (octet_length(digest) = 32),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, operation, request_id)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON migration_connection TO crm_app;
GRANT SELECT, INSERT, UPDATE ON migration_assessment TO crm_app;
GRANT SELECT, INSERT ON migration_assessment_evidence TO crm_app;
GRANT SELECT, INSERT, UPDATE ON migration_assessment_check TO crm_app;
GRANT SELECT, INSERT ON migration_request_receipt TO crm_app;
