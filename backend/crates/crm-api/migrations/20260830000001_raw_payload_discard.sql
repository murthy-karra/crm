-- Slice 007e (docs/specs/SLICE_007e.md §3): the 'discarded' resolution
-- with attribution on the row. Discard is admin-only and human-only, so
-- discarded_by_user_id is a plain user FK (no actor_kind). Discard is
-- NOT deletion: ciphertext is retained until the erasure runbook /
-- O-013 (D-015 §4/§7).

ALTER TABLE raw_payload
    ADD COLUMN discarded_by_user_id UUID NULL REFERENCES app_user (id),
    ADD COLUMN discarded_at TIMESTAMPTZ NULL;

-- The inline CHECK from 20260821000003 auto-named itself
-- raw_payload_resolution_check (verified against a live database before
-- this migration was written — spec §3). DROP+ADD is the established
-- widening pattern (20260829000001 did the same for routing_decision).
ALTER TABLE raw_payload DROP CONSTRAINT raw_payload_resolution_check;
ALTER TABLE raw_payload ADD CONSTRAINT raw_payload_resolution_check
    CHECK (resolution IN ('pending', 'resolved', 'unresolved', 'discarded'));

-- Attribution is exactly paired with the discarded state — the fact
-- tables' (actor_kind='user') = (actor_user_id IS NOT NULL) discipline.
ALTER TABLE raw_payload ADD CONSTRAINT raw_payload_discard_fields_check
    CHECK ((resolution = 'discarded') = (discarded_at IS NOT NULL)
       AND (resolution = 'discarded') = (discarded_by_user_id IS NOT NULL));

GRANT UPDATE (discarded_by_user_id, discarded_at) ON raw_payload TO crm_app;
