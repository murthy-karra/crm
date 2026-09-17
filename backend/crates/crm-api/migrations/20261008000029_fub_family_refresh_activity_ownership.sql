-- Refresh-created notes/tasks keep the same global source identity and tombstone.
ALTER TABLE migration_activity_identity
 ADD COLUMN refresh_bundle_id UUID,
 ADD COLUMN refresh_plan_id UUID,
 ADD COLUMN refresh_manifest_id UUID,
 ADD CONSTRAINT family_activity_owner_fk FOREIGN KEY(refresh_manifest_id,refresh_plan_id,refresh_bundle_id,organization_id)
 REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id),
 DROP CONSTRAINT migration_activity_identity_exclusive_owner;
ALTER TABLE migration_activity_identity ADD CONSTRAINT migration_activity_identity_exclusive_owner CHECK (
 (import_id IS NOT NULL AND plan_id IS NOT NULL AND manifest_id IS NOT NULL
  AND admitted_import_id IS NULL AND admitted_plan_id IS NULL AND admitted_manifest_id IS NULL
  AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL)
 OR (import_id IS NULL AND plan_id IS NULL AND manifest_id IS NULL
  AND admitted_import_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_manifest_id IS NOT NULL
  AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL)
 OR (import_id IS NULL AND plan_id IS NULL AND manifest_id IS NULL
  AND admitted_import_id IS NULL AND admitted_plan_id IS NULL AND admitted_manifest_id IS NULL
  AND refresh_bundle_id IS NOT NULL AND refresh_plan_id IS NOT NULL AND refresh_manifest_id IS NOT NULL)
);
CREATE FUNCTION crm_family_refresh_activity_owner() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; u migration_family_refresh_manifest; native_row JSONB;
BEGIN
 IF NEW.refresh_plan_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.refresh_bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.refresh_plan_id AND bundle_id=b.id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=NEW.refresh_manifest_id AND plan_id=p.id AND bundle_id=b.id AND organization_id=NEW.organization_id;
 IF b.id IS NULL OR p.id IS NULL OR u.id IS NULL OR b.confirmed_at IS NULL OR p.confirmed_at IS NULL OR p.state<>'running' OR p.cancel_requested
  OR p.family<>'activity' OR p.phase<>'apply' OR b.state NOT IN ('queued','running','paused')
  OR p.lease_token IS NULL OR p.lease_expires_at IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR u.kind<>NEW.kind OR u.disposition<>'insert' OR u.position<>p.apply_position+1 OR u.target_id IS DISTINCT FROM NEW.target_id
  OR NEW.source_account_id<>b.source_account_id
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_source s WHERE s.id=u.source_row_id AND s.bundle_id=b.id AND s.organization_id=b.organization_id AND s.kind=NEW.kind AND s.source_id=NEW.source_id AND s.qualified)
  OR NOT EXISTS(SELECT 1 FROM migration_workspace w WHERE w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id)
  OR NOT EXISTS(SELECT 1 FROM organization_membership a WHERE a.organization_id=b.organization_id AND a.user_id=b.executor_user_id AND a.role='admin' AND a.status='active')
  OR EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE manifest_id=u.id AND organization_id=u.organization_id)
  THEN RAISE EXCEPTION 'unbound refresh activity identity'; END IF;
 IF NEW.kind='note' THEN SELECT to_jsonb(n) INTO native_row FROM note n WHERE n.id=NEW.target_id AND n.organization_id=NEW.organization_id;
 ELSE SELECT to_jsonb(t) INTO native_row FROM task t WHERE t.id=NEW.target_id AND t.organization_id=NEW.organization_id; END IF;
 IF native_row IS NULL OR (native_row->>'person_id')::uuid IS DISTINCT FROM u.person_id
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_write_proof w WHERE w.manifest_id=u.id AND w.plan_id=p.id AND w.organization_id=p.organization_id AND w.table_name=NEW.kind AND w.operation='INSERT' AND w.target_id=NEW.target_id AND w.after_hash=crm_family_refresh_native_digest(native_row))
  THEN RAISE EXCEPTION 'refresh activity identity native proof missing'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_activity_owner BEFORE INSERT ON migration_activity_identity FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_activity_owner();
REVOKE ALL ON FUNCTION crm_family_refresh_activity_owner() FROM PUBLIC;
-- The existing identity inventory has one fixed owner tuple, independent of its
-- type. Keep its exact byte formula; move only the accounting destination.
DO $$ DECLARE definition TEXT; needle TEXT; BEGIN
 SELECT pg_get_functiondef('crm_activity_measure_row()'::regprocedure) INTO definition;
 needle:='IF TG_TABLE_NAME=''migration_activity_identity'' AND row_value->>''admitted_import_id'' IS NOT NULL THEN';
 IF position(needle IN definition)=0 THEN RAISE EXCEPTION 'unexpected activity identity inventory'; END IF;
 EXECUTE replace(definition,needle,'IF TG_TABLE_NAME=''migration_activity_identity'' AND row_value->>''refresh_plan_id'' IS NOT NULL THEN UPDATE migration_family_refresh_plan SET measured_bytes=measured_bytes+delta WHERE id=(row_value->>''refresh_plan_id'')::uuid AND organization_id=org; IF NOT FOUND THEN RAISE EXCEPTION ''refresh activity accounting owner missing''; END IF; RETURN COALESCE(NEW,OLD); END IF; '||needle);
END $$;

CREATE OR REPLACE FUNCTION crm_family_refresh_owned_activity(org UUID,bundle UUID)
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
  UNION ALL
  SELECT m.person_id,prior.source_person_id,prior.original_result_id,prior.admission_result_id
  FROM migration_family_refresh_bundle previous
  JOIN migration_family_refresh_plan p ON p.bundle_id=previous.id AND p.organization_id=previous.organization_id
  JOIN migration_family_refresh_manifest m ON m.plan_id=p.id AND m.bundle_id=p.bundle_id AND m.organization_id=p.organization_id
  JOIN migration_family_refresh_cohort prior ON prior.id=m.cohort_id AND prior.bundle_id=m.bundle_id AND prior.organization_id=m.organization_id
  JOIN migration_family_refresh_result r ON r.manifest_id=m.id AND r.plan_id=p.id AND r.organization_id=m.organization_id
  WHERE previous.id=i.refresh_bundle_id AND p.id=i.refresh_plan_id AND m.id=i.refresh_manifest_id
   AND previous.organization_id=i.organization_id AND previous.parent_import_id=b.parent_import_id AND previous.parent_plan_id=b.parent_plan_id AND previous.source_account_id=b.source_account_id
   AND previous.confirmed_at IS NOT NULL AND previous.state IN ('completed','cancelled') AND previous.updated_at<=b.created_at
   AND p.family='activity' AND p.confirmed_at IS NOT NULL AND p.state IN ('completed','cancelled')
   AND m.disposition='insert' AND m.kind=i.kind AND r.disposition='applied'
   AND r.target_id=i.target_id AND r.person_id=m.person_id AND r.committed_at<=b.created_at
   AND EXISTS(SELECT 1 FROM migration_family_refresh_source s WHERE s.id=m.source_row_id AND s.bundle_id=m.bundle_id AND s.organization_id=m.organization_id AND s.kind=i.kind AND s.source_id=i.source_id)
 ) owner ON true
 JOIN migration_family_refresh_cohort c ON c.bundle_id=b.id AND c.organization_id=b.organization_id
  AND c.person_id=owner.person_id AND c.source_person_id=owner.source_person_id
  AND ((owner.original_result_id IS NOT NULL AND c.original_result_id=owner.original_result_id)
    OR (owner.admission_result_id IS NOT NULL AND c.admission_result_id=owner.admission_result_id))
 WHERE b.id=bundle AND b.organization_id=org
$$;
