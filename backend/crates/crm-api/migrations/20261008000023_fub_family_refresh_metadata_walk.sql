-- D-092: metadata traversal shares the immutable source index but owns its
-- checkpoint and outcomes. It may finish only after every note/task occurrence.
CREATE INDEX family_refresh_metadata_walk ON migration_family_refresh_source(bundle_id,organization_id,id) WHERE kind='person';
CREATE FUNCTION crm_family_refresh_metadata_walk_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle;
BEGIN
 IF OLD.family<>'metadata' OR (NEW.checkpoint_id IS NOT DISTINCT FROM OLD.checkpoint_id AND NEW.source_walk_complete=OLD.source_walk_complete) THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR NOT OLD.catalog_walk_complete
  OR OLD.phase NOT IN ('mappings','classify') OR OLD.source_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale metadata source walk'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND
  (NEW.checkpoint_id IS NULL OR (OLD.checkpoint_id IS NOT NULL AND NEW.checkpoint_id<=OLD.checkpoint_id)
   OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE id=NEW.checkpoint_id AND bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind='person')
   OR EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind='person' AND (OLD.checkpoint_id IS NULL OR id>OLD.checkpoint_id) AND id<NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'metadata source walk skipped an occurrence'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND NOT EXISTS(
  SELECT 1 FROM migration_family_refresh_source s JOIN migration_family_refresh_manifest m
   ON m.plan_id=OLD.id AND m.organization_id=s.organization_id AND m.kind='metadata'
  LEFT JOIN migration_family_refresh_source original ON original.id=m.source_row_id AND original.bundle_id=s.bundle_id AND original.organization_id=s.organization_id AND original.kind=s.kind
  WHERE s.id=NEW.checkpoint_id AND s.bundle_id=OLD.bundle_id AND s.organization_id=OLD.organization_id AND s.kind='person'
   AND (m.source_row_id=s.id OR (s.source_id IS NOT NULL AND (original.source_id=s.source_id OR m.source_id=s.source_id))))
  THEN RAISE EXCEPTION 'metadata source walk outcome missing'; END IF;
 IF NEW.source_walk_complete AND EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind='person' AND (NEW.checkpoint_id IS NULL OR id>NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'metadata source walk incomplete'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_metadata_walk_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_metadata_walk_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_metadata_walk_fence() FROM PUBLIC;

DO $$ DECLARE definition TEXT; fn TEXT; BEGIN
 FOREACH fn IN ARRAY ARRAY['crm_family_refresh_walk_fence()','crm_family_refresh_owned_walk_fence()'] LOOP
  SELECT pg_get_functiondef(fn::regprocedure) INTO definition;
  IF position('IF OLD.family=''activity'' THEN RETURN NEW;' IN definition)=0 THEN RAISE EXCEPTION 'unexpected historical traversal fence'; END IF;
  EXECUTE replace(definition,'IF OLD.family=''activity'' THEN RETURN NEW;','IF OLD.family IN (''activity'',''metadata'') THEN RETURN NEW;');
 END LOOP;
END $$;
CREATE FUNCTION crm_family_refresh_metadata_owned_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE candidate UUID;
BEGIN
 IF current_user<>'crm_app' OR OLD.family<>'metadata' OR ROW(NEW.owned_after,NEW.owned_walk_complete) IS NOT DISTINCT FROM ROW(OLD.owned_after,OLD.owned_walk_complete) THEN RETURN NEW; END IF;
 IF OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR OLD.phase<>'classify' OR NOT OLD.source_walk_complete OR NOT OLD.catalog_walk_complete OR OLD.owned_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_bundle b JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE b.id=OLD.bundle_id AND b.organization_id=OLD.organization_id AND b.state='preparing' AND b.confirmed_at IS NULL AND m.status='active' AND m.role='admin')
  THEN RAISE EXCEPTION 'stale Person metadata ownership walk'; END IF;
 SELECT id INTO candidate FROM migration_family_refresh_cohort WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND (OLD.owned_after IS NULL OR id>OLD.owned_after) ORDER BY id LIMIT 1;
 IF NEW.owned_after IS DISTINCT FROM OLD.owned_after AND (NEW.owned_after IS DISTINCT FROM candidate OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=OLD.id AND organization_id=OLD.organization_id AND kind='metadata' AND cohort_id=NEW.owned_after)) THEN RAISE EXCEPTION 'Person metadata walk skipped outcome'; END IF;
 IF NEW.owned_walk_complete AND EXISTS(SELECT 1 FROM migration_family_refresh_cohort WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND (NEW.owned_after IS NULL OR id>NEW.owned_after)) THEN RAISE EXCEPTION 'Person metadata ownership walk incomplete'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_metadata_owned_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_metadata_owned_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_metadata_owned_fence() FROM PUBLIC;
