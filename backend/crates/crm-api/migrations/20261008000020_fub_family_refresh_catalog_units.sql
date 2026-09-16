-- Exact typed mapping ownership for immutable catalog preparation outcomes.
ALTER TABLE migration_family_refresh_manifest ADD COLUMN mapping_id UUID,
 ADD CHECK(mapping_id IS NULL OR kind='catalog'),
 ADD FOREIGN KEY(mapping_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_mapping(id,plan_id,bundle_id,organization_id);
CREATE UNIQUE INDEX family_refresh_catalog_unit ON migration_family_refresh_manifest(plan_id,organization_id,mapping_id) WHERE mapping_id IS NOT NULL;
CREATE FUNCTION crm_family_refresh_catalog_unit_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; m migration_family_refresh_mapping; parent_unit migration_family_refresh_manifest;
BEGIN
 IF current_user<>'crm_app' OR NEW.kind<>'catalog' THEN RETURN NEW; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO m FROM migration_family_refresh_mapping WHERE id=NEW.mapping_id AND plan_id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF p.id IS NULL OR b.id IS NULL OR m.id IS NULL OR p.family<>'metadata' OR p.state<>'preparing' OR p.phase NOT IN ('mappings','classify') OR NOT p.mappings_complete
  OR p.confirmed_at IS NOT NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  OR m.kind NOT IN ('tag','field','option') OR NEW.source_row_id IS DISTINCT FROM m.source_row_id OR NEW.source_key_hmac<>m.source_key_hmac
  OR NEW.cohort_id IS NOT NULL OR NEW.person_id IS NOT NULL OR NEW.baseline_result_id IS NOT NULL OR NEW.expected_head_id IS NOT NULL OR NEW.expected_revision IS NOT NULL
  THEN RAISE EXCEPTION 'invalid family catalog unit binding'; END IF;
 IF m.kind='option' THEN
  SELECT * INTO parent_unit FROM migration_family_refresh_manifest WHERE plan_id=m.plan_id AND organization_id=m.organization_id AND mapping_id=m.parent_id AND kind='catalog';
  IF parent_unit.id IS NULL OR (NEW.disposition<>'held' AND parent_unit.disposition NOT IN ('insert','already_current')) THEN RAISE EXCEPTION 'family catalog prerequisite missing'; END IF;
 END IF;
 IF NEW.disposition='held' THEN
  IF NEW.target_id IS NOT NULL OR NEW.reason IS NULL THEN RAISE EXCEPTION 'invalid held family catalog unit'; END IF;
 ELSIF NOT m.qualified OR (m.kind='tag' AND NOT b.mapping_capture_order) OR NEW.target_id IS DISTINCT FROM m.target_id OR NEW.target_id IS NULL OR NEW.reason IS NOT NULL
  OR NOT ((NEW.disposition='insert' AND m.disposition='create_matching') OR (NEW.disposition='already_current' AND m.disposition='existing'))
  OR NOT EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=b.organization_id AND state='ready' AND engine_version='fub-admitted-metadata-v1')
  THEN RAISE EXCEPTION 'invalid eligible family catalog unit'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_catalog_unit_fence BEFORE INSERT ON migration_family_refresh_manifest FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_catalog_unit_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_catalog_unit_fence() FROM PUBLIC;
