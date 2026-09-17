SELECT i.source_id,i.target_id AS person_id,
 r.id AS original_result_id,a.id AS admission_id,ar.id AS admission_result_id,
 CASE WHEN i.admission_id IS NULL THEN parent.snapshot_id ELSE a.confirmed_snapshot_id END AS creation_snapshot_id,
 CASE
 WHEN i.plan_id<>parent.confirmed_plan_id THEN 'unproven'
 WHEN i.admission_id IS NULL AND
  (r.id IS NULL OR r.plan_id<>i.plan_id OR r.manifest_id IS DISTINCT FROM i.manifest_id
   OR r.person_id IS DISTINCT FROM i.target_id OR r.disposition NOT IN ('imported','already_imported')) THEN 'unproven'
 WHEN i.admission_id IS NOT NULL AND
  (ar.id IS NULL OR ar.item_id IS DISTINCT FROM i.admission_item_id OR ar.person_id IS DISTINCT FROM i.target_id
   OR ar.source_id<>i.source_id OR ar.disposition<>'settled' OR a.id IS NULL
   OR a.parent_import_id<>parent.id OR a.parent_plan_id<>parent.confirmed_plan_id
   OR a.source_account_id<>i.source_account_id OR a.confirmed_snapshot_id IS NULL) THEN 'unproven'
 WHEN i.admission_id IS NOT NULL AND NOT
  ((a.state='completed' AND a.completed_at IS NOT NULL AND a.completed_at<=$5)
   OR (a.state='cancelled' AND a.cancelled_at IS NOT NULL AND a.cancelled_at<=$5)) THEN 'unfinished'
 WHEN person.id IS NULL THEN 'erased'
 WHEN i.admission_id IS NULL THEN 'original'
 WHEN a.mode='mapping_recovery' THEN 'recovery'
 ELSE 'admitted' END AS disposition
FROM migration_import_identity i
JOIN migration_import parent ON parent.id=i.import_id AND parent.organization_id=i.organization_id
 AND parent.state='completed'
LEFT JOIN migration_import_result r ON i.admission_id IS NULL AND r.import_id=i.import_id
 AND r.organization_id=i.organization_id AND r.source_id=i.source_id
LEFT JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=i.organization_id
LEFT JOIN migration_people_admission_result ar ON ar.id=i.admission_result_id
 AND ar.admission_id=i.admission_id AND ar.organization_id=i.organization_id
LEFT JOIN person ON person.id=i.target_id AND person.organization_id=i.organization_id
WHERE i.organization_id=$1 AND i.source_account_id=$2 AND i.family='people'
 AND i.import_id=$3 AND i.source_id>$4 AND i.created_at<=$5
ORDER BY i.source_id LIMIT $6
