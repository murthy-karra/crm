-- D-092: activity traversal shares the immutable source index but owns its
-- checkpoint and outcomes. It may finish only after every note/task occurrence.
CREATE INDEX family_refresh_activity_walk ON migration_family_refresh_source(bundle_id,organization_id,id) WHERE kind IN ('note','task');
CREATE FUNCTION crm_family_refresh_activity_walk_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle;
BEGIN
 IF OLD.family<>'activity' OR (NEW.checkpoint_id IS NOT DISTINCT FROM OLD.checkpoint_id AND NEW.source_walk_complete=OLD.source_walk_complete) THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR NOT OLD.mappings_complete
  OR OLD.phase NOT IN ('mappings','classify') OR OLD.source_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale activity source walk'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND
  (NEW.checkpoint_id IS NULL OR (OLD.checkpoint_id IS NOT NULL AND NEW.checkpoint_id<=OLD.checkpoint_id)
   OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE id=NEW.checkpoint_id AND bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('note','task'))
   OR EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('note','task') AND (OLD.checkpoint_id IS NULL OR id>OLD.checkpoint_id) AND id<NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'activity source walk skipped an occurrence'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND NOT EXISTS(
  SELECT 1 FROM migration_family_refresh_source s JOIN migration_family_refresh_manifest m
   ON m.plan_id=OLD.id AND m.organization_id=s.organization_id AND m.kind=s.kind
  JOIN migration_family_refresh_source original ON original.id=m.source_row_id AND original.bundle_id=s.bundle_id AND original.organization_id=s.organization_id AND original.kind=s.kind
  WHERE s.id=NEW.checkpoint_id AND s.bundle_id=OLD.bundle_id AND s.organization_id=OLD.organization_id AND s.kind IN ('note','task')
   AND (m.source_row_id=s.id OR (s.source_id IS NOT NULL AND original.source_id=s.source_id)))
  THEN RAISE EXCEPTION 'activity source walk outcome missing'; END IF;
 IF NEW.source_walk_complete AND EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('note','task') AND (NEW.checkpoint_id IS NULL OR id>NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'activity source walk incomplete'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_activity_walk_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_activity_walk_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_activity_walk_fence() FROM PUBLIC;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_walk_fence()'::regprocedure) INTO definition;
 IF position('IF NEW.checkpoint_id' IN definition)=0 THEN RAISE EXCEPTION 'unexpected source walk fence'; END IF;
 EXECUTE replace(definition,'IF NEW.checkpoint_id IS NOT DISTINCT FROM OLD.checkpoint_id','IF OLD.family=''activity'' THEN RETURN NEW; END IF; IF NEW.checkpoint_id IS NOT DISTINCT FROM OLD.checkpoint_id');
END $$;
