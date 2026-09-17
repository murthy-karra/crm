-- D-092 final 25k acceptance: bound Person drilldown and sparse result filters.
CREATE INDEX family_refresh_manifest_cohort_page
 ON migration_family_refresh_manifest(plan_id,organization_id,cohort_id,position);
CREATE INDEX family_refresh_result_outcome
 ON migration_family_refresh_result(plan_id,organization_id,disposition,manifest_id);
