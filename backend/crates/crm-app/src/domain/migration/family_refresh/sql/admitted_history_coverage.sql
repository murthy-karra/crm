SELECT s.id,s.source_account_id,s.started_at,s.completed_at,s.state,
 a.state AS owner_state,COALESCE(a.completed_at,a.updated_at) AS terminal_at,
 EXISTS(SELECT 1 FROM migration_admitted_history_result r
 JOIN migration_admitted_history_manifest m ON m.id=r.manifest_id AND m.plan_id=r.plan_id
  AND m.root_id=r.root_id AND m.organization_id=r.organization_id
 JOIN migration_people_admission_result person ON person.id=$7 AND person.organization_id=r.organization_id
 WHERE r.organization_id=a.organization_id AND r.root_id=a.id AND r.plan_id=a.confirmed_plan_id
  AND r.disposition IN ('imported','already_present') AND r.fact_id IS NOT NULL AND m.person_id=$6
  AND person.admission_id=$5 AND person.person_id=$6 AND person.source_id=$8) AS proven
FROM migration_admitted_history_root a
JOIN migration_history_capture_run s ON s.id=a.history_capture_id AND s.organization_id=a.organization_id
WHERE a.organization_id=$1 AND a.parent_import_id=$2 AND a.parent_plan_id=$3
 AND a.source_account_id=$4 AND a.admission_id=$5
 AND a.confirmed_at IS NOT NULL AND a.confirmed_plan_id IS NOT NULL
