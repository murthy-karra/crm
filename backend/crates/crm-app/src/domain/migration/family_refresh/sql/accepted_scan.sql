-- Source table/column are selected from two fixed server-owned alternatives.
-- Every confirmed plan is a scan boundary, including cancellation with no writes.
-- Return a violating boundary; never choose a source record by its timestamp.
SELECT CASE
 WHEN b.source_account_id<>$4 OR b.parent_plan_id<>$5 OR s.source_account_id<>$4
   THEN 'identity_mismatch'
 WHEN b.state NOT IN ('completed','cancelled')
   OR p.state NOT IN ('completed','cancelled','superseded')
   OR b.confirmed_at>$10 OR p.confirmed_at>$10 OR b.updated_at>$10
   THEN 'baseline_unproven'
 WHEN s.id IS NULL OR s.started_at IS NULL OR s.completed_at IS NULL
   OR s.state NOT IN ('completed','completed_with_gaps') OR s.completed_at<s.started_at
   THEN 'source_unavailable'
 ELSE 'source_not_newer' END AS reason
FROM migration_family_refresh_bundle b
JOIN migration_family_refresh_plan p ON p.bundle_id=b.id AND p.organization_id=b.organization_id
JOIN migration_family_refresh_cohort c ON c.bundle_id=b.id AND c.organization_id=b.organization_id
LEFT JOIN __SOURCE_TABLE__ s ON s.id=p.__SOURCE_COLUMN__ AND s.organization_id=p.organization_id
WHERE b.organization_id=$1 AND b.parent_import_id=$2 AND b.id<>$3
  AND b.confirmed_at IS NOT NULL AND p.confirmed_at IS NOT NULL AND p.family=$6
  AND c.person_id=$7 AND c.original_result_id IS NOT DISTINCT FROM $8
  AND c.admission_result_id IS NOT DISTINCT FROM $9 AND c.creation_snapshot_id=$13
  AND c.source_person_id=$14
  AND (b.source_account_id<>$4 OR b.parent_plan_id<>$5 OR s.source_account_id<>$4
    OR b.state NOT IN ('completed','cancelled')
    OR p.state NOT IN ('completed','cancelled','superseded')
    OR b.confirmed_at>$10 OR p.confirmed_at>$10 OR b.updated_at>$10
    OR s.id IS NULL OR s.started_at IS NULL OR s.completed_at IS NULL
    OR s.state NOT IN ('completed','completed_with_gaps') OR s.completed_at<s.started_at
    OR ((s.id=$11 OR s.completed_at>=$12) AND NOT (
      s.id=$11 AND EXISTS(SELECT 1 FROM migration_family_refresh_bundle current
        WHERE current.id=$3 AND current.organization_id=$1
          AND current.remainder_origin_bundle_id IS NOT NULL
          AND COALESCE(b.remainder_origin_bundle_id,b.id)=current.remainder_origin_bundle_id))))
LIMIT 1
