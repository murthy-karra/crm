-- D-092: scoped keyset review of one mapping kind/disposition without walking
-- every other mapping in a large retained catalog. Unfiltered pages retain the
-- existing (plan_id,organization_id,id) index.
CREATE INDEX family_refresh_mapping_filter ON migration_family_refresh_mapping(plan_id,organization_id,kind,disposition,id);
