-- D-092: absence is a review hold, never a native deletion. Fixed-width
-- traversal state does not add variable evidence or change byte inventories.
ALTER TABLE migration_family_refresh_plan
 ADD COLUMN owned_after UUID,
 ADD COLUMN owned_walk_complete BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX family_refresh_history_owned_walk ON migration_history_import_identity(organization_id,id);
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('''source_walk_complete''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family plan guard'; END IF;
 EXECUTE replace(definition,'''source_walk_complete''','''source_walk_complete'',''owned_after'',''owned_walk_complete''');
END $$;

-- One immutable first owner in this bundle's frozen cohort. The runtime and
-- cursor fence share this exact selection; another import/account/Organization
-- cannot add an identity to the walk. Baseline authentication remains in Rust.
CREATE FUNCTION crm_family_refresh_next_owned_history(org UUID,bundle UUID,after_id UUID)
 RETURNS TABLE(identity_id UUID,cohort_id UUID,person_id UUID,kind TEXT,identity_hmac BYTEA)
 LANGUAGE sql STABLE AS $$
 SELECT i.id,c.id,c.person_id,
  CASE i.family WHEN 'events' THEN 'event' WHEN 'calls' THEN 'call' ELSE 'text' END,i.identity_hmac
 FROM migration_family_refresh_bundle b
 JOIN migration_family_refresh_cohort c ON c.bundle_id=b.id AND c.organization_id=b.organization_id
 JOIN migration_history_import_identity i ON i.organization_id=c.organization_id AND i.person_id=c.person_id
 WHERE b.id=bundle AND b.organization_id=org AND i.fact_id IS NOT NULL
  AND (after_id IS NULL OR i.id>after_id)
  AND ((i.owner_run_id IS NOT NULL AND c.original_result_id IS NOT NULL AND EXISTS(
   SELECT 1 FROM migration_history_import_run r
   JOIN migration_history_import_plan p ON p.id=r.plan_id AND p.organization_id=r.organization_id
   WHERE r.id=i.owner_run_id AND r.organization_id=org AND r.parent_import_id=b.parent_import_id
    AND p.parent_plan_id=b.parent_plan_id AND p.source_account_id=b.source_account_id
    AND r.confirmed_at IS NOT NULL AND r.state IN ('completed','cancelled')
    AND COALESCE(r.completed_at,r.updated_at)<=b.created_at))
  OR (i.admitted_root_id IS NOT NULL AND c.admission_id IS NOT NULL AND EXISTS(
   SELECT 1 FROM migration_admitted_history_root r WHERE r.id=i.admitted_root_id AND r.organization_id=org
    AND r.parent_import_id=b.parent_import_id AND r.parent_plan_id=b.parent_plan_id
    AND r.source_account_id=b.source_account_id AND r.admission_id=c.admission_id
    AND r.confirmed_plan_id=i.admitted_plan_id AND r.state IN ('completed','cancelled')
    AND COALESCE(r.completed_at,r.updated_at)<=b.created_at)))
 ORDER BY i.id LIMIT 1
$$;
REVOKE ALL ON FUNCTION crm_family_refresh_next_owned_history(UUID,UUID,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_next_owned_history(UUID,UUID,UUID) TO crm_app;

CREATE FUNCTION crm_family_refresh_owned_walk_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; candidate RECORD;
BEGIN
 IF NEW.owned_after IS NOT DISTINCT FROM OLD.owned_after AND NEW.owned_walk_complete=OLD.owned_walk_complete THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.family<>'history' OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL
  OR OLD.phase<>'classify' OR NOT OLD.source_walk_complete OR OLD.owned_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale family owned history walk'; END IF;
 IF NEW.owned_after IS DISTINCT FROM OLD.owned_after THEN
  SELECT * INTO candidate FROM crm_family_refresh_next_owned_history(OLD.organization_id,OLD.bundle_id,OLD.owned_after);
  IF candidate.identity_id IS NULL OR candidate.identity_id IS DISTINCT FROM NEW.owned_after
   THEN RAISE EXCEPTION 'family owned history walk skipped an identity'; END IF;
  IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=OLD.id AND organization_id=OLD.organization_id AND kind=candidate.kind AND source_key_hmac=candidate.identity_hmac)
   THEN RAISE EXCEPTION 'family owned history walk outcome missing'; END IF;
 END IF;
 IF NEW.owned_walk_complete AND EXISTS(SELECT 1 FROM crm_family_refresh_next_owned_history(OLD.organization_id,OLD.bundle_id,NEW.owned_after))
  THEN RAISE EXCEPTION 'family owned history walk incomplete'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_owned_walk_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_owned_walk_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_owned_walk_fence() FROM PUBLIC;
