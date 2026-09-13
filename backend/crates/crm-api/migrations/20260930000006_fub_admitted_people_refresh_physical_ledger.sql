-- D-080 accounts every refresh-owned physical variable field. Reconcile every
-- existing local run from its complete physical footprint, rather than assuming
-- which earlier feature revisions already charged provenance or source fields.
CREATE TEMPORARY TABLE fub_admitted_people_refresh_ledger_backfill ON COMMIT DROP AS
SELECT
  r.id AS refresh_id,
  r.organization_id,
  r.newer_snapshot_id,
  (
    COALESCE((SELECT sum(octet_length(p.inputs_nonce) + octet_length(p.inputs_ciphertext)
                       + COALESCE(octet_length(p.digest), 0))
              FROM migration_admitted_people_refresh_plan p
              WHERE p.refresh_id = r.id AND p.organization_id = r.organization_id), 0)
    + COALESCE((
      SELECT sum(octet_length(i.source_key) + COALESCE(octet_length(i.source_id), 0)
               + octet_length(i.proposed_nonce) + octet_length(i.proposed_ciphertext)
               + octet_length(i.baseline_nonce) + octet_length(i.baseline_ciphertext)
               + octet_length(i.current_nonce) + octet_length(i.current_ciphertext)
               + octet_length(i.instructions_nonce) + octet_length(i.instructions_ciphertext))
      FROM migration_admitted_people_refresh_item i
      WHERE i.refresh_id = r.id AND i.organization_id = r.organization_id
    ), 0)
    + COALESCE((
      SELECT sum(octet_length(c.value_nonce) + octet_length(c.value_ciphertext))
      FROM migration_admitted_people_refresh_contact c
      WHERE c.refresh_id = r.id AND c.organization_id = r.organization_id
    ), 0)
    + COALESCE((
      SELECT sum(octet_length(x.source_id) + octet_length(x.before_nonce)
               + octet_length(x.before_ciphertext) + octet_length(x.after_nonce)
               + octet_length(x.after_ciphertext))
      FROM migration_admitted_people_refresh_result x
      WHERE x.refresh_id = r.id AND x.organization_id = r.organization_id
    ), 0)
    + COALESCE((
      SELECT sum(octet_length(p.source_id) + octet_length(p.projection_nonce)
               + octet_length(p.projection_ciphertext))
      FROM migration_admitted_people_refresh_baseline p
      WHERE p.refresh_id = r.id AND p.organization_id = r.organization_id
    ), 0)
    + COALESCE((
      SELECT sum(octet_length(p.before_nonce) + octet_length(p.before_ciphertext)
               + octet_length(p.after_nonce) + octet_length(p.after_ciphertext))
      FROM person_admitted_refresh_provenance p
      WHERE p.refresh_id = r.id AND p.organization_id = r.organization_id
    ), 0)
    + COALESCE((
      SELECT sum(octet_length(x.nonce) + octet_length(x.ciphertext))
      FROM migration_admitted_people_refresh_receipt x
      WHERE x.refresh_id = r.id AND x.organization_id = r.organization_id
    ), 0)
    + octet_length(r.preparation_checkpoint_key)
  )::bigint AS physical_bytes,
  r.retained_bytes AS recorded_bytes
FROM migration_admitted_people_refresh r;

DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM fub_admitted_people_refresh_ledger_backfill
    WHERE physical_bytes < recorded_bytes
  ) THEN
    RAISE EXCEPTION 'admitted refresh ledger exceeds its physical footprint';
  END IF;
END $$;

UPDATE migration_admitted_people_refresh r
SET retained_bytes = b.physical_bytes
FROM fub_admitted_people_refresh_ledger_backfill b
WHERE r.id = b.refresh_id AND r.organization_id = b.organization_id
  AND b.physical_bytes > b.recorded_bytes;

UPDATE migration_snapshot s
SET retained_bytes = s.retained_bytes + b.bytes
FROM (
  SELECT organization_id, newer_snapshot_id,
         sum(physical_bytes - recorded_bytes)::bigint AS bytes
  FROM fub_admitted_people_refresh_ledger_backfill
  WHERE physical_bytes > recorded_bytes
  GROUP BY organization_id, newer_snapshot_id
) b
WHERE s.id = b.newer_snapshot_id AND s.organization_id = b.organization_id;

UPDATE migration_snapshot_storage s
SET retained_bytes = s.retained_bytes + b.bytes
FROM (
  SELECT organization_id, sum(physical_bytes - recorded_bytes)::bigint AS bytes
  FROM fub_admitted_people_refresh_ledger_backfill
  WHERE physical_bytes > recorded_bytes
  GROUP BY organization_id
) b
WHERE s.organization_id = b.organization_id;
