-- Slice 006c §5a (D-033): indexes for the "outcome needed" Today source
-- (docs/specs/SLICE_006c.md §5a). `today/queries.rs` scans a viewer's
-- terminal calls by caller and probes `contact_attempted` by the call's
-- causation id. Grants unchanged.

CREATE INDEX call_org_caller_ended_idx
    ON call (organization_id, caller_user_id, ended_at DESC)
    WHERE status IN ('ended', 'failed');

CREATE INDEX contact_attempted_org_causation_idx
    ON contact_attempted (organization_id, causation_id);
