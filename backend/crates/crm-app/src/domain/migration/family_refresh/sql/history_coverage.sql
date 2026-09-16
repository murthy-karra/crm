SELECT s.id,s.source_account_id,s.started_at,s.completed_at,s.state,
 a.state AS owner_state,COALESCE(a.completed_at,a.updated_at) AS terminal_at,
 EXISTS(SELECT 1 FROM migration_history_import_result r
 JOIN migration_history_import_manifest m ON m.id=r.manifest_id AND m.plan_id=r.plan_id AND m.organization_id=r.organization_id
 JOIN migration_import_result person ON person.id=$7 AND person.organization_id=r.organization_id
 WHERE r.organization_id=a.organization_id AND r.owner_run_id=a.id AND r.plan_id=p.id
  AND r.disposition IN ('imported','already_imported') AND r.fact_id IS NOT NULL AND m.person_id=$6
  AND person.import_id=$2 AND person.person_id=$6 AND person.source_id=$8) AS proven
FROM migration_history_import_run a
JOIN migration_history_import_plan p ON p.id=a.plan_id AND p.organization_id=a.organization_id
JOIN migration_history_capture_run s ON s.id=p.capture_id AND s.organization_id=p.organization_id
WHERE a.organization_id=$1 AND a.parent_import_id=$2 AND p.parent_plan_id=$3
 AND p.source_account_id=$4 AND $5::uuid IS NULL
 AND a.confirmed_at IS NOT NULL
