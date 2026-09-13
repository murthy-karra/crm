-- D-080: each executable non-null target must retain the exact qualified
-- original-plan mapping that authorized it. Existing ready plans intentionally
-- remain null and fail closed until re-previewed.
ALTER TABLE migration_admitted_people_refresh_item
  ADD COLUMN stage_mapping_id UUID,
  ADD COLUMN assignee_mapping_id UUID;

CREATE INDEX migration_admitted_people_refresh_item_mapping_binding
  ON migration_admitted_people_refresh_item(plan_id, organization_id, stage_mapping_id, assignee_mapping_id)
  WHERE settled_at IS NULL;
