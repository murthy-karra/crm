-- One atomic Person outcome per plan; catalog/diagnostic rows are separate.
CREATE UNIQUE INDEX family_refresh_person_metadata_unit ON migration_family_refresh_manifest(plan_id,organization_id,cohort_id) WHERE kind='metadata' AND cohort_id IS NOT NULL;
CREATE FUNCTION crm_family_refresh_person_metadata_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; c migration_family_refresh_cohort;
BEGIN
 IF current_user<>'crm_app' OR NEW.kind<>'metadata' OR NEW.cohort_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
 SELECT * INTO c FROM migration_family_refresh_cohort WHERE id=NEW.cohort_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF p.id IS NULL OR c.id IS NULL OR p.family<>'metadata' OR p.state<>'preparing' OR p.confirmed_at IS NOT NULL OR NOT p.catalog_walk_complete
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR NEW.person_id IS DISTINCT FROM c.person_id OR NEW.source_id IS DISTINCT FROM c.source_person_id OR NEW.mapping_id IS NOT NULL
  OR NEW.disposition NOT IN ('update','already_current','held')
  OR (NEW.disposition='held' AND (NEW.reason IS NULL OR NEW.target_id IS NOT NULL OR NEW.baseline_result_id IS NOT NULL OR NEW.expected_head_id IS NOT NULL OR NEW.expected_revision IS NOT NULL))
  OR (NEW.disposition<>'held' AND (NEW.reason IS NOT NULL OR NEW.source_row_id IS NULL OR NEW.target_id IS DISTINCT FROM c.person_id OR NEW.baseline_result_id IS NULL OR NEW.expected_revision IS NULL OR NEW.expected_revision<=0))
  THEN RAISE EXCEPTION 'invalid family Person metadata unit'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_person_metadata_fence BEFORE INSERT ON migration_family_refresh_manifest FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_person_metadata_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_person_metadata_fence() FROM PUBLIC;
-- source_id is also the typed Person natural key now; keep the activity-only
-- missing-identity fence on its original domain.
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_missing_activity_fence()'::regprocedure) INTO definition;
 IF position('IF NEW.source_id IS NULL' IN definition)=0 THEN RAISE EXCEPTION 'unexpected missing activity fence'; END IF;
 EXECUTE replace(definition,'IF NEW.source_id IS NULL','IF NEW.kind=''metadata'' OR NEW.source_id IS NULL');
END $$;
