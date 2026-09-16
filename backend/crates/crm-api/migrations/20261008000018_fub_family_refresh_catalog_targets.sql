-- Bounded probes for distinct source definitions/choices claiming one target.
CREATE INDEX family_refresh_catalog_target ON migration_family_refresh_mapping
 (plan_id,organization_id,kind,target_id) WHERE target_id IS NOT NULL;
