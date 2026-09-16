-- __CORRECTED__ is replaced only by the closed typed-history table mapping.
SELECT v.*, d.nonce, d.ciphertext, p.revision AS plan_revision,
       s.id AS source_id, s.plan_id AS source_plan, sp.revision AS source_revision,
       s.nonce AS source_nonce, s.ciphertext AS source_ciphertext
FROM __CORRECTED__ v
JOIN migration_family_refresh_history_display d
  ON d.id=v.id AND d.identity_id=v.identity_id AND d.organization_id=v.organization_id
 AND d.result_id=v.result_id AND d.plan_id=v.plan_id AND d.bundle_id=v.bundle_id
JOIN migration_family_refresh_plan p
  ON p.id=v.plan_id AND p.bundle_id=v.bundle_id AND p.organization_id=v.organization_id
 AND p.family='history' AND p.history_capture_id=v.capture_id
JOIN migration_family_refresh_bundle b ON b.id=v.bundle_id AND b.organization_id=v.organization_id
JOIN migration_family_refresh_result r
  ON r.id=v.result_id AND r.manifest_id=v.manifest_id AND r.plan_id=v.plan_id
 AND r.bundle_id=v.bundle_id AND r.organization_id=v.organization_id
 AND r.disposition='applied' AND r.person_id=v.person_id AND r.target_id=v.original_fact_id
JOIN migration_family_refresh_manifest m
  ON m.id=v.manifest_id AND m.plan_id=v.plan_id AND m.bundle_id=v.bundle_id AND m.organization_id=v.organization_id
 AND m.disposition='correction' AND m.person_id=v.person_id AND m.target_id=v.original_fact_id
JOIN migration_family_refresh_cohort c
  ON c.id=m.cohort_id AND c.bundle_id=v.bundle_id AND c.organization_id=v.organization_id AND c.person_id=v.person_id
JOIN migration_family_refresh_source s
  ON s.id=m.source_row_id AND s.bundle_id=v.bundle_id AND s.organization_id=v.organization_id
 AND s.qualified AND s.semantic_hmac=v.semantic_hmac
JOIN migration_family_refresh_plan sp
  ON sp.id=s.plan_id AND sp.bundle_id=s.bundle_id AND sp.organization_id=s.organization_id
 AND sp.family='history' AND sp.history_capture_id=v.capture_id
LEFT JOIN __CORRECTED__ previous
  ON previous.id=v.corrects_id AND previous.identity_id=v.identity_id AND previous.organization_id=v.organization_id
 AND previous.original_fact_id=v.original_fact_id AND previous.version=v.version-1
WHERE v.id=$1 AND v.organization_id=$2 AND v.identity_id=$3
  AND b.parent_import_id=$4 AND b.parent_plan_id=$5 AND b.source_account_id=$6
  AND c.original_result_id IS NOT DISTINCT FROM $7
  AND c.admission_result_id IS NOT DISTINCT FROM $8 AND c.creation_snapshot_id=$9
  AND r.committed_at<=$10 AND v.recorded_at<=$10 AND b.updated_at<=$10
  AND b.confirmed_at IS NOT NULL AND b.state IN ('completed','cancelled')
  AND p.confirmed_at IS NOT NULL AND p.state IN ('completed','cancelled')
  AND m.kind=$11 AND s.kind=$11 AND m.source_key_hmac=$12 AND s.identity_hmac=$12
  AND c.source_person_id=$13 AND s.source_person_id=$13
  AND ((v.version=2 AND v.corrects_id IS NULL AND m.expected_head_id IS NULL)
    OR (v.version>2 AND previous.id IS NOT NULL AND m.expected_head_id=previous.result_id))
