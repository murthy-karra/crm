SELECT r.*, p.revision AS plan_revision, p.source_snapshot_id, s.capture_sequence
FROM migration_family_refresh_result r
JOIN migration_family_refresh_manifest m
  ON m.id=r.manifest_id AND m.plan_id=r.plan_id AND m.bundle_id=r.bundle_id AND m.organization_id=r.organization_id
JOIN migration_family_refresh_plan p
  ON p.id=r.plan_id AND p.bundle_id=r.bundle_id AND p.organization_id=r.organization_id
JOIN migration_family_refresh_bundle b ON b.id=r.bundle_id AND b.organization_id=r.organization_id
JOIN migration_family_refresh_cohort c ON c.id=m.cohort_id AND c.bundle_id=b.id AND c.organization_id=b.organization_id
JOIN migration_snapshot s ON s.id=p.source_snapshot_id AND s.organization_id=p.organization_id
WHERE r.id=$1 AND r.organization_id=$2
  AND b.parent_import_id=$3 AND b.parent_plan_id=$4 AND b.source_account_id=$5
  AND p.family=$6 AND m.kind=$7 AND m.source_key_hmac=$8
  AND r.person_id=$9 AND m.person_id=$9 AND c.person_id=$9
  AND r.target_id=$10 AND m.target_id=$10 AND c.source_person_id=$11
  AND c.original_result_id IS NOT DISTINCT FROM $12
  AND c.admission_result_id IS NOT DISTINCT FROM $13 AND c.creation_snapshot_id=$14
  AND r.committed_at<=$15 AND b.updated_at<=$15
  AND b.confirmed_at IS NOT NULL AND b.state IN ('completed','cancelled')
  AND p.confirmed_at IS NOT NULL AND p.state IN ('completed','cancelled')
  AND r.disposition IN ('applied','already_current') AND r.native_revision>0
  AND m.disposition IN ('insert','update','already_current')
