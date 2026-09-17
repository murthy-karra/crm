SELECT s.id,s.source_account_id,s.started_at,s.completed_at,s.state,
 a.state AS owner_state,COALESCE(a.completed_at,a.updated_at) AS terminal_at,
 EXISTS(SELECT 1 FROM __PREFIX___result r
 JOIN __PREFIX___manifest m ON m.id=r.manifest_id AND m.plan_id=r.plan_id
  AND m.import_id=r.import_id AND m.organization_id=r.organization_id
 WHERE r.organization_id=a.organization_id AND r.import_id=a.id
  AND r.plan_id=a.confirmed_plan_id AND r.disposition IN ('applied','already_present')
  AND r.person_id=$6 AND m.person_id=$6 AND m.__COHORT__=$7
  AND m.source_person_id=$8 AND r.kind=m.kind AND r.source_id=m.source_id
  AND r.target_id=m.target_id) AS proven
FROM __PREFIX___import a
JOIN migration_snapshot s ON s.id=a.snapshot_id AND s.organization_id=a.organization_id
WHERE a.organization_id=$1 AND a.parent_import_id=$2 AND a.parent_plan_id=$3
 AND a.source_account_id=$4 AND __ROOT_FILTER__
 AND a.confirmed_at IS NOT NULL AND a.confirmed_plan_id IS NOT NULL
