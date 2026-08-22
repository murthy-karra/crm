-- Slice 002: encrypted deletable blob for raw lead payloads (D-015 §4).
-- `id` has no DEFAULT: it is generated in Rust because it is part of the
-- AEAD associated data (docs/specs/SLICE_002.md §2, §7).

CREATE TABLE raw_payload (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organization (id),
    source TEXT NOT NULL,
    payload_format TEXT NOT NULL,
    origin TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    nonce BYTEA NOT NULL,
    ciphertext BYTEA NOT NULL,
    content_hmac BYTEA NOT NULL,
    byte_len INT NOT NULL,
    resolution TEXT NOT NULL CHECK (resolution IN ('pending', 'resolved', 'unresolved')),
    unresolved_reason TEXT,
    resolved_at TIMESTAMPTZ,
    inquiry_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The delivery idempotency key for this slice's generic ingress
    -- (docs/specs/SLICE_002.md §3).
    UNIQUE (organization_id, source, content_hmac)
);

GRANT SELECT, INSERT ON raw_payload TO crm_app;
-- Column-level UPDATE only: nonce, ciphertext, and content_hmac stay
-- immutable to the application (docs/specs/SLICE_002.md §2).
GRANT UPDATE (resolution, unresolved_reason, resolved_at, inquiry_id) ON raw_payload TO crm_app;
