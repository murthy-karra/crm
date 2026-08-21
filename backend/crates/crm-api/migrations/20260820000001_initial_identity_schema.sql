-- Slice 001: identity, tenancy, and database foundation.
-- Applied by crm_migrator (schema owner). crm_app is granted DML only,
-- exactly as specified in docs/specs/SLICE_001.md §2.

CREATE TABLE organization (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE app_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Case-insensitive uniqueness on email without a citext dependency.
CREATE UNIQUE INDEX app_user_email_lower_idx ON app_user (lower(email));

-- Identity-provider-neutral: app_user carries no password material.
-- When ZITADEL arrives, an external_identity table sits beside this one;
-- app_user and everything downstream is untouched.
CREATE TABLE local_credential (
    user_id UUID PRIMARY KEY REFERENCES app_user (id),
    password_hash TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE organization_membership (
    organization_id UUID NOT NULL REFERENCES organization (id),
    user_id UUID NOT NULL REFERENCES app_user (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, user_id)
);

CREATE TABLE user_session (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash TEXT NOT NULL,
    user_id UUID NOT NULL REFERENCES app_user (id),
    active_organization_id UUID NOT NULL REFERENCES organization (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX user_session_token_hash_idx ON user_session (token_hash);

GRANT SELECT ON organization TO crm_app;
GRANT SELECT ON app_user TO crm_app;
GRANT SELECT ON local_credential TO crm_app;
GRANT SELECT ON organization_membership TO crm_app;
GRANT SELECT, INSERT, UPDATE ON user_session TO crm_app;
