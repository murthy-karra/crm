-- The internal page runner is not the only defense against stale worker writes.
-- Preserve migrator fixture/repair access; application writes need the payer's
-- current preparation claim, including progress-only writes.
CREATE FUNCTION crm_family_refresh_cohort_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan;
BEGIN
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  IF NEW.cohort_after IS NOT DISTINCT FROM OLD.cohort_after
   AND NEW.cohort_counts IS NOT DISTINCT FROM OLD.cohort_counts THEN RETURN NEW; END IF;
  IF current_user<>'crm_app' THEN RETURN NEW; END IF;
  p:=OLD;
  SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 ELSE
  SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
  IF NOT EXISTS(SELECT 1 FROM person WHERE id=NEW.person_id AND organization_id=NEW.organization_id)
   OR NOT EXISTS(SELECT 1 FROM migration_import_identity i WHERE i.organization_id=NEW.organization_id
    AND i.source_account_id=b.source_account_id AND i.family='people' AND i.source_id=NEW.source_person_id
    AND i.target_id=NEW.person_id AND i.import_id=b.parent_import_id AND i.created_at<=b.created_at)
   THEN RAISE EXCEPTION 'family cohort outside frozen identity boundary'; END IF;
  IF NEW.admission_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_people_admission a
   WHERE a.id=NEW.admission_id AND a.organization_id=NEW.organization_id
    AND ((a.state='completed' AND a.completed_at<=b.created_at)
      OR (a.state='cancelled' AND a.cancelled_at<=b.created_at)))
   THEN RAISE EXCEPTION 'family cohort outside frozen terminal boundary'; END IF;
  IF current_user<>'crm_app' THEN RETURN NEW; END IF;
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=b.payer_plan_id AND bundle_id=b.id AND organization_id=b.organization_id FOR SHARE;
 END IF;
 IF b.id IS NULL OR p.id IS NULL OR b.payer_plan_id IS DISTINCT FROM p.id
  OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR p.state<>'preparing' OR p.phase<>'cohort' OR p.confirmed_at IS NOT NULL
  OR p.lease_epoch<=0 OR p.lease_token IS NULL
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=b.organization_id
   AND m.user_id=b.executor_user_id AND m.status='active' AND m.role='admin')
  THEN RAISE EXCEPTION 'stale family cohort preparation claim'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_cohort_fence BEFORE INSERT ON migration_family_refresh_cohort FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_cohort_fence();
CREATE TRIGGER family_refresh_cohort_progress_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_cohort_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_cohort_fence() FROM PUBLIC;
