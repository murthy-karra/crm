-- D-074: a successful reconciliation is terminal cache metadata, not an
-- operation receipt. Retain the marker only until bounded cache reclamation.
-- Existing complete=true rows are not backfilled: complete describes selection
-- coverage and cannot establish that a seal ever succeeded.
ALTER TABLE mobile_reconciliation ADD COLUMN sealed_at TIMESTAMPTZ;
ALTER TABLE mobile_reconciliation ADD CONSTRAINT mobile_sealed_generation_complete
    CHECK (sealed_at IS NULL OR complete);
