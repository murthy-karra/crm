-- D-092: activity identities have a composite natural key. Preserve it and use
-- that bounded key for ownership traversal without rewriting the global registry.
ALTER TABLE migration_family_refresh_plan
 ADD COLUMN owned_activity_kind TEXT CHECK(owned_activity_kind IN ('note','task')),
 ADD COLUMN owned_activity_source_id TEXT CHECK(octet_length(owned_activity_source_id) BETWEEN 1 AND 128),
 ADD CONSTRAINT family_activity_owned_cursor_pair CHECK((owned_activity_kind IS NULL)=(owned_activity_source_id IS NULL));
ALTER TABLE migration_family_refresh_manifest ADD COLUMN source_id TEXT CHECK(octet_length(source_id) BETWEEN 1 AND 128);
CREATE INDEX family_refresh_manifest_source_outcome ON migration_family_refresh_manifest(plan_id,organization_id,source_row_id);
CREATE INDEX family_refresh_manifest_missing_activity ON migration_family_refresh_manifest(plan_id,organization_id,kind,source_id) WHERE source_row_id IS NULL;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('''owned_after''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family plan guard'; END IF;
 EXECUTE replace(definition,'''owned_after''','''owned_after'',''owned_activity_kind'',''owned_activity_source_id''');
 SELECT pg_get_functiondef('crm_family_refresh_retained_size(jsonb)'::regprocedure) INTO definition;
 IF position('''source_id''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family byte inventory'; END IF;
 EXECUTE replace(definition,'''source_id''','''source_id'',''owned_activity_kind'',''owned_activity_source_id''');
END $$;

CREATE FUNCTION crm_family_refresh_owned_activity(org UUID,bundle UUID)
 RETURNS TABLE(cohort_id UUID,person_id UUID,kind TEXT,source_id TEXT,target_id UUID)
 LANGUAGE sql STABLE AS $$
 SELECT c.id,c.person_id,i.kind,i.source_id,i.target_id
 FROM migration_family_refresh_bundle b
 JOIN migration_activity_identity i ON i.organization_id=b.organization_id AND i.source_account_id=b.source_account_id
 JOIN LATERAL (
  SELECT m.person_id,m.source_person_id,m.parent_result_id AS original_result_id,NULL::uuid AS admission_result_id
  FROM migration_activity_import a
  JOIN migration_activity_manifest m ON m.import_id=a.id AND m.organization_id=a.organization_id AND m.plan_id=a.confirmed_plan_id
  JOIN migration_activity_result r ON r.manifest_id=m.id AND r.plan_id=m.plan_id AND r.import_id=m.import_id AND r.organization_id=m.organization_id
  WHERE a.id=i.import_id AND a.organization_id=i.organization_id AND a.confirmed_plan_id=i.plan_id AND m.id=i.manifest_id
   AND a.parent_import_id=b.parent_import_id AND a.parent_plan_id=b.parent_plan_id AND a.source_account_id=b.source_account_id
   AND a.confirmed_at IS NOT NULL AND a.state IN ('completed','cancelled')
   AND (CASE WHEN a.state='completed' THEN a.completed_at ELSE a.updated_at END)<=b.created_at
   AND r.disposition='applied' AND r.kind=i.kind AND m.kind=i.kind AND r.source_id=i.source_id AND m.source_id=i.source_id
   AND r.target_id=i.target_id AND r.person_id=m.person_id AND r.committed_at<=b.created_at
  UNION ALL
  SELECT m.person_id,m.source_person_id,NULL::uuid,m.admission_result_id
  FROM migration_admitted_activity_import a
  JOIN migration_admitted_activity_manifest m ON m.import_id=a.id AND m.organization_id=a.organization_id AND m.plan_id=a.confirmed_plan_id
  JOIN migration_admitted_activity_result r ON r.manifest_id=m.id AND r.plan_id=m.plan_id AND r.import_id=m.import_id AND r.organization_id=m.organization_id
  WHERE a.id=i.admitted_import_id AND a.organization_id=i.organization_id AND a.confirmed_plan_id=i.admitted_plan_id AND m.id=i.admitted_manifest_id
   AND a.parent_import_id=b.parent_import_id AND a.parent_plan_id=b.parent_plan_id AND a.source_account_id=b.source_account_id
   AND a.confirmed_at IS NOT NULL AND a.state IN ('completed','cancelled')
   AND (CASE WHEN a.state='completed' THEN a.completed_at ELSE a.updated_at END)<=b.created_at
   AND r.disposition='applied' AND r.kind=i.kind AND m.kind=i.kind AND r.source_id=i.source_id AND m.source_id=i.source_id
   AND r.target_id=i.target_id AND r.person_id=m.person_id AND r.committed_at<=b.created_at
 ) owner ON true
 JOIN migration_family_refresh_cohort c ON c.bundle_id=b.id AND c.organization_id=b.organization_id
  AND c.person_id=owner.person_id AND c.source_person_id=owner.source_person_id
  AND ((owner.original_result_id IS NOT NULL AND c.original_result_id=owner.original_result_id)
    OR (owner.admission_result_id IS NOT NULL AND c.admission_result_id=owner.admission_result_id))
 WHERE b.id=bundle AND b.organization_id=org
$$;
REVOKE ALL ON FUNCTION crm_family_refresh_owned_activity(UUID,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_owned_activity(UUID,UUID) TO crm_app;
CREATE FUNCTION crm_family_refresh_next_owned_activity(org UUID,bundle UUID,after_kind TEXT,after_source TEXT)
 RETURNS TABLE(cohort_id UUID,person_id UUID,kind TEXT,source_id TEXT,target_id UUID)
 LANGUAGE sql STABLE AS $$
 SELECT * FROM crm_family_refresh_owned_activity(org,bundle) a
 WHERE after_kind IS NULL OR (a.kind,a.source_id)>(after_kind,after_source)
 ORDER BY a.kind,a.source_id LIMIT 1
$$;
REVOKE ALL ON FUNCTION crm_family_refresh_next_owned_activity(UUID,UUID,TEXT,TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_next_owned_activity(UUID,UUID,TEXT,TEXT) TO crm_app;

CREATE FUNCTION crm_family_refresh_activity_owned_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; candidate RECORD;
BEGIN
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 IF OLD.family<>'activity' THEN
  IF ROW(NEW.owned_activity_kind,NEW.owned_activity_source_id) IS DISTINCT FROM ROW(OLD.owned_activity_kind,OLD.owned_activity_source_id) THEN RAISE EXCEPTION 'activity cursor on another family'; END IF;
  RETURN NEW;
 END IF;
 IF NEW.owned_after IS DISTINCT FROM OLD.owned_after THEN RAISE EXCEPTION 'history cursor on activity family'; END IF;
 IF ROW(NEW.owned_activity_kind,NEW.owned_activity_source_id,NEW.owned_walk_complete) IS NOT DISTINCT FROM ROW(OLD.owned_activity_kind,OLD.owned_activity_source_id,OLD.owned_walk_complete) THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR OLD.phase<>'classify' OR NOT OLD.source_walk_complete OR OLD.owned_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active')
  THEN RAISE EXCEPTION 'stale activity ownership walk'; END IF;
 IF ROW(NEW.owned_activity_kind,NEW.owned_activity_source_id) IS DISTINCT FROM ROW(OLD.owned_activity_kind,OLD.owned_activity_source_id) THEN
  SELECT * INTO candidate FROM crm_family_refresh_next_owned_activity(OLD.organization_id,OLD.bundle_id,OLD.owned_activity_kind,OLD.owned_activity_source_id);
  IF candidate.kind IS NULL OR ROW(candidate.kind,candidate.source_id) IS DISTINCT FROM ROW(NEW.owned_activity_kind,NEW.owned_activity_source_id)
   THEN RAISE EXCEPTION 'activity ownership walk skipped an identity'; END IF;
  IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_source s JOIN migration_family_refresh_manifest m ON m.source_row_id=s.id AND m.bundle_id=s.bundle_id AND m.organization_id=s.organization_id
   WHERE s.bundle_id=OLD.bundle_id AND s.organization_id=OLD.organization_id AND s.kind=candidate.kind AND s.source_id=candidate.source_id AND m.plan_id=OLD.id AND m.kind=candidate.kind)
   AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest m WHERE m.plan_id=OLD.id AND m.organization_id=OLD.organization_id AND m.kind=candidate.kind
    AND m.source_row_id IS NULL AND m.source_id=candidate.source_id AND m.cohort_id=candidate.cohort_id AND m.person_id=candidate.person_id AND m.disposition='held')
   THEN RAISE EXCEPTION 'activity ownership outcome missing'; END IF;
 END IF;
 IF NEW.owned_walk_complete AND EXISTS(SELECT 1 FROM crm_family_refresh_next_owned_activity(OLD.organization_id,OLD.bundle_id,NEW.owned_activity_kind,NEW.owned_activity_source_id)) THEN RAISE EXCEPTION 'activity ownership walk incomplete'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_activity_owned_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_activity_owned_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_activity_owned_fence() FROM PUBLIC;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_owned_walk_fence()'::regprocedure) INTO definition;
 IF position('IF NEW.owned_after' IN definition)=0 THEN RAISE EXCEPTION 'unexpected owned walk fence'; END IF;
 EXECUTE replace(definition,'IF NEW.owned_after IS NOT DISTINCT FROM OLD.owned_after','IF OLD.family=''activity'' THEN RETURN NEW; END IF; IF NEW.owned_after IS NOT DISTINCT FROM OLD.owned_after');
END $$;
CREATE FUNCTION crm_family_refresh_missing_activity_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; candidate RECORD;
BEGIN
 IF NEW.source_id IS NULL OR current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 IF p.id IS NULL OR p.family<>'activity' OR p.phase<>'classify' OR p.state<>'preparing' OR p.confirmed_at IS NOT NULL
  OR NOT p.source_walk_complete OR p.owned_walk_complete OR NEW.source_row_id IS NOT NULL OR NEW.disposition<>'held'
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  THEN RAISE EXCEPTION 'invalid missing activity proposal'; END IF;
 SELECT * INTO candidate FROM crm_family_refresh_next_owned_activity(p.organization_id,p.bundle_id,p.owned_activity_kind,p.owned_activity_source_id);
 IF candidate.kind IS NULL OR ROW(NEW.kind,NEW.source_id,NEW.cohort_id,NEW.person_id) IS DISTINCT FROM ROW(candidate.kind,candidate.source_id,candidate.cohort_id,candidate.person_id)
  OR EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=p.bundle_id AND organization_id=p.organization_id AND kind=NEW.kind AND source_id=NEW.source_id)
  THEN RAISE EXCEPTION 'missing activity ownership or absence mismatch'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_missing_activity_fence BEFORE INSERT ON migration_family_refresh_manifest FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_missing_activity_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_missing_activity_fence() FROM PUBLIC;
