-- D-080 sparse item/result reads bind the confirmed plan and disposition
-- before joining bounded descriptors. These indexes keep those hot traversals
-- scoped to one admitted-refresh revision without widening legacy page scans.
CREATE INDEX migration_admitted_people_refresh_item_plan_page
  ON migration_admitted_people_refresh_item(refresh_id, organization_id, plan_id, id);

CREATE INDEX migration_admitted_people_refresh_item_plan_unsettled_disposition_page
  ON migration_admitted_people_refresh_item(refresh_id, organization_id, plan_id, disposition, id)
  WHERE settled_at IS NULL;

CREATE INDEX migration_admitted_people_refresh_result_disposition_item_page
  ON migration_admitted_people_refresh_result(refresh_id, organization_id, disposition, item_id);

-- Preparation walks the frozen terminal admission's settled cohort by source
-- key. This excludes unrelated admission results before the bounded sort.
CREATE INDEX migration_people_admission_result_settled_cohort_source_page
  ON migration_people_admission_result(admission_id, organization_id, source_id)
  WHERE disposition = 'settled';
