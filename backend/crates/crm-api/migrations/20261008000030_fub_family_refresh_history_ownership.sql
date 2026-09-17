-- D-092: one immutable global history identity with an exclusive refresh first
-- owner. Existing first facts and original/admitted owner tuples are unchanged.
ALTER TABLE migration_history_import_identity ADD COLUMN refresh_bundle_id UUID, ADD COLUMN refresh_plan_id UUID, ADD COLUMN refresh_manifest_id UUID;
ALTER TABLE migration_history_import_identity DROP CONSTRAINT migration_history_import_identity_exclusive_owner, ADD CONSTRAINT migration_history_import_identity_exclusive_owner CHECK (
 (owner_run_id IS NOT NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (owner_run_id IS NULL AND admitted_root_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_attempt_id IS NOT NULL AND admitted_manifest_id IS NOT NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (owner_run_id IS NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL AND refresh_bundle_id IS NOT NULL AND refresh_plan_id IS NOT NULL AND refresh_manifest_id IS NOT NULL));
ALTER TABLE migration_history_import_identity ADD CONSTRAINT migration_history_import_identity_refresh_owner_fk FOREIGN KEY(refresh_manifest_id,refresh_plan_id,refresh_bundle_id,organization_id) REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id);
ALTER TABLE migration_history_import_display ADD COLUMN refresh_bundle_id UUID, ADD COLUMN refresh_plan_id UUID, ADD COLUMN refresh_manifest_id UUID;
ALTER TABLE migration_history_import_display DROP CONSTRAINT history_display_exclusive_owner, ADD CONSTRAINT history_display_exclusive_owner CHECK (
 (plan_id IS NOT NULL AND owner_run_id IS NOT NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (plan_id IS NULL AND owner_run_id IS NULL AND admitted_root_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_attempt_id IS NOT NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (plan_id IS NULL AND owner_run_id IS NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND refresh_bundle_id IS NOT NULL AND refresh_plan_id IS NOT NULL AND refresh_manifest_id IS NOT NULL));
ALTER TABLE migration_history_import_display ADD CONSTRAINT migration_history_import_display_refresh_owner_fk FOREIGN KEY(refresh_manifest_id,refresh_plan_id,refresh_bundle_id,organization_id) REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id);
ALTER TABLE migration_history_import_display ADD CHECK(refresh_manifest_id IS NULL OR id=refresh_manifest_id);
ALTER TABLE fub_event_record_imported ADD COLUMN refresh_bundle_id UUID, ADD COLUMN refresh_plan_id UUID, ADD COLUMN refresh_manifest_id UUID;
ALTER TABLE fub_event_record_imported DROP CONSTRAINT fub_event_record_imported_exclusive_owner, ADD CONSTRAINT fub_event_record_imported_exclusive_owner CHECK (
 (plan_id IS NOT NULL AND attempt_id IS NOT NULL AND manifest_id IS NOT NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (plan_id IS NULL AND attempt_id IS NULL AND manifest_id IS NULL AND admitted_root_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_attempt_id IS NOT NULL AND admitted_manifest_id IS NOT NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (plan_id IS NULL AND attempt_id IS NULL AND manifest_id IS NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL AND refresh_bundle_id IS NOT NULL AND refresh_plan_id IS NOT NULL AND refresh_manifest_id IS NOT NULL));
ALTER TABLE fub_event_record_imported ADD CONSTRAINT fub_event_record_imported_refresh_owner_fk FOREIGN KEY(refresh_manifest_id,refresh_plan_id,refresh_bundle_id,organization_id) REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id);
ALTER TABLE fub_call_record_imported ADD COLUMN refresh_bundle_id UUID, ADD COLUMN refresh_plan_id UUID, ADD COLUMN refresh_manifest_id UUID;
ALTER TABLE fub_call_record_imported DROP CONSTRAINT fub_call_record_imported_exclusive_owner, ADD CONSTRAINT fub_call_record_imported_exclusive_owner CHECK (
 (plan_id IS NOT NULL AND attempt_id IS NOT NULL AND manifest_id IS NOT NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (plan_id IS NULL AND attempt_id IS NULL AND manifest_id IS NULL AND admitted_root_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_attempt_id IS NOT NULL AND admitted_manifest_id IS NOT NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (plan_id IS NULL AND attempt_id IS NULL AND manifest_id IS NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL AND refresh_bundle_id IS NOT NULL AND refresh_plan_id IS NOT NULL AND refresh_manifest_id IS NOT NULL));
ALTER TABLE fub_call_record_imported ADD CONSTRAINT fub_call_record_imported_refresh_owner_fk FOREIGN KEY(refresh_manifest_id,refresh_plan_id,refresh_bundle_id,organization_id) REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id);
ALTER TABLE fub_text_record_imported ADD COLUMN refresh_bundle_id UUID, ADD COLUMN refresh_plan_id UUID, ADD COLUMN refresh_manifest_id UUID;
ALTER TABLE fub_text_record_imported DROP CONSTRAINT fub_text_record_imported_exclusive_owner, ADD CONSTRAINT fub_text_record_imported_exclusive_owner CHECK (
 (plan_id IS NOT NULL AND attempt_id IS NOT NULL AND manifest_id IS NOT NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (plan_id IS NULL AND attempt_id IS NULL AND manifest_id IS NULL AND admitted_root_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_attempt_id IS NOT NULL AND admitted_manifest_id IS NOT NULL AND refresh_bundle_id IS NULL AND refresh_plan_id IS NULL AND refresh_manifest_id IS NULL) OR
 (plan_id IS NULL AND attempt_id IS NULL AND manifest_id IS NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL AND refresh_bundle_id IS NOT NULL AND refresh_plan_id IS NOT NULL AND refresh_manifest_id IS NOT NULL));
ALTER TABLE fub_text_record_imported ADD CONSTRAINT fub_text_record_imported_refresh_owner_fk FOREIGN KEY(refresh_manifest_id,refresh_plan_id,refresh_bundle_id,organization_id) REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id);

-- The first identity/display and typed fact all refer to the same applied result
-- in this transaction. No row can establish a second or partial owner shape.
CREATE FUNCTION crm_family_history_first_allowed(v JSONB, table_name TEXT) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle;
 u migration_family_refresh_manifest; s migration_family_refresh_source; i migration_history_import_identity; family_name TEXT;
BEGIN
 PERFORM crm_workspace_shared((v->>'organization_id')::uuid);
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=(v->>'refresh_bundle_id')::uuid AND organization_id=(v->>'organization_id')::uuid;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=(v->>'refresh_plan_id')::uuid AND bundle_id=b.id AND organization_id=b.organization_id;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=(v->>'refresh_manifest_id')::uuid AND plan_id=p.id AND bundle_id=b.id AND organization_id=b.organization_id;
 IF b.id IS NULL OR p.id IS NULL OR u.id IS NULL OR b.confirmed_at IS NULL OR p.confirmed_at IS NULL
  OR b.state NOT IN ('queued','running','paused') OR p.state<>'running' OR p.phase<>'apply' OR p.family<>'history' OR p.cancel_requested
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR u.position<>p.apply_position+1 OR u.disposition<>'insert' OR u.expected_head_id IS NOT NULL OR u.expected_revision IS NOT NULL
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=b.organization_id)
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active')
  OR NOT EXISTS(SELECT 1 FROM migration_workspace w JOIN organization o ON o.id=w.organization_id WHERE w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id AND o.workspace_mode='migration_review')
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE organization_id=b.organization_id AND bundle_id=b.id AND plan_id=p.id AND manifest_id=u.id AND disposition='applied' AND person_id=u.person_id AND target_id=u.target_id)
 THEN RETURN false; END IF;
 SELECT * INTO s FROM migration_family_refresh_source WHERE id=u.source_row_id AND bundle_id=b.id AND organization_id=b.organization_id;
 family_name:=CASE u.kind WHEN 'event' THEN 'events' WHEN 'call' THEN 'calls' WHEN 'text' THEN 'text_messages' END;
 IF s.id IS NULL OR family_name IS NULL OR NOT s.qualified OR s.kind<>u.kind OR s.identity_hmac IS DISTINCT FROM u.source_key_hmac
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_cohort c JOIN person person ON person.id=c.person_id AND person.organization_id=c.organization_id
   JOIN migration_import_identity pi ON pi.organization_id=c.organization_id AND pi.target_id=c.person_id AND pi.family='people' AND pi.source_account_id=b.source_account_id AND pi.source_id=c.source_person_id
   WHERE c.id=u.cohort_id AND c.bundle_id=b.id AND c.organization_id=b.organization_id AND c.person_id=u.person_id AND c.source_person_id=s.source_person_id
    AND pi.admission_result_id IS NOT DISTINCT FROM c.admission_result_id
    AND (c.original_result_id IS NULL OR EXISTS(SELECT 1 FROM migration_import_result r WHERE r.id=c.original_result_id AND r.organization_id=c.organization_id AND r.import_id=pi.import_id AND r.plan_id=pi.plan_id AND r.person_id=c.person_id AND r.source_id=c.source_person_id AND r.disposition='imported')))
  OR NOT EXISTS(SELECT 1 FROM migration_history_capture raw JOIN migration_history_capture_run selected ON selected.id=raw.run_id AND selected.organization_id=raw.organization_id
   WHERE raw.id=s.capture_id AND raw.organization_id=b.organization_id AND raw.sequence=s.capture_sequence AND raw.run_id=p.history_capture_id AND raw.family=family_name AND raw.classification='advancing' AND NOT raw.truncated
    AND raw.representation=s.representation AND selected.state='completed_with_gaps' AND selected.completed_at IS NOT NULL AND selected.parent_import_id=b.parent_import_id AND selected.source_account_id=b.source_account_id)
  OR EXISTS(SELECT 1 FROM migration_family_refresh_source sibling WHERE sibling.bundle_id=b.id AND sibling.organization_id=b.organization_id AND sibling.kind=u.kind AND sibling.identity_hmac=s.identity_hmac
   AND (NOT sibling.qualified OR sibling.semantic_hmac<>s.semantic_hmac OR sibling.source_person_id IS DISTINCT FROM s.source_person_id OR sibling.representation<>s.representation))
 THEN RETURN false; END IF;
 IF table_name='migration_history_import_identity' THEN
  RETURN (v->>'person_id')::uuid=u.person_id AND (v->>'fact_id')::uuid=u.target_id AND v->>'family'=family_name AND v->>'erased_at' IS NULL
   AND (v->>'identity_hmac')::bytea=s.identity_hmac AND (v->>'semantic_hmac')::bytea=s.semantic_hmac;
 END IF;
 SELECT * INTO i FROM migration_history_import_identity WHERE organization_id=b.organization_id AND refresh_bundle_id=b.id AND refresh_plan_id=p.id AND refresh_manifest_id=u.id AND identity_hmac=s.identity_hmac AND semantic_hmac=s.semantic_hmac AND person_id=u.person_id AND fact_id=u.target_id AND erased_at IS NULL;
 IF i.id IS NULL THEN RETURN false; END IF;
 IF table_name='migration_history_import_display' THEN RETURN (v->>'id')::uuid=u.id; END IF;
 RETURN table_name='fub_'||u.kind||'_record_imported' AND (v->>'identity_id')::uuid=i.id AND (v->>'id')::uuid=u.target_id AND (v->>'person_id')::uuid=u.person_id
  AND v->>'actor_kind'='user' AND (v->>'actor_user_id')::uuid=b.executor_user_id AND v->>'origin'='migration' AND v->>'on_behalf_of_user_id' IS NULL AND v->>'corrects_id' IS NULL
  AND (v->>'stable_position')::bigint=u.position
  AND EXISTS(SELECT 1 FROM migration_history_import_display d WHERE d.organization_id=b.organization_id AND d.id=u.id AND d.refresh_plan_id=p.id AND d.refresh_bundle_id=b.id);
END $$;
CREATE FUNCTION crm_family_history_first_insert() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.refresh_bundle_id IS NOT NULL AND NOT COALESCE(crm_family_history_first_allowed(to_jsonb(NEW),TG_TABLE_NAME),false) THEN RAISE EXCEPTION 'family history first owner invalid'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_history_first_insert BEFORE INSERT ON migration_history_import_identity FOR EACH ROW EXECUTE FUNCTION crm_family_history_first_insert();
CREATE TRIGGER family_history_first_insert BEFORE INSERT ON migration_history_import_display FOR EACH ROW EXECUTE FUNCTION crm_family_history_first_insert();
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_history_import_fact_guard()'::regprocedure) INTO definition;
 IF position('IF NEW.admitted_root_id IS NOT NULL' in definition)=0 THEN RAISE EXCEPTION 'history fact fence drift'; END IF;
 definition:=replace(definition,E'BEGIN\n',E'BEGIN\n IF NEW.refresh_bundle_id IS NOT NULL THEN\n  IF NOT COALESCE(crm_family_history_first_allowed(to_jsonb(NEW),TG_TABLE_NAME),false) THEN RAISE EXCEPTION ''family history first fact invalid''; END IF;\n  RETURN NEW;\n END IF;\n');
 EXECUTE definition;
END $$;
REVOKE ALL ON FUNCTION crm_family_history_first_allowed(JSONB,TEXT),crm_family_history_first_insert() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_history_first_allowed(JSONB,TEXT) TO crm_app;

-- Retain the established variable-byte formula. Refresh evidence settles with
-- its confirmed unit; deleting a settled display refunds its original payer.
DO $$ DECLARE definition TEXT; needle TEXT; replacement TEXT; BEGIN
 SELECT pg_get_functiondef('crm_history_import_measure()'::regprocedure) INTO definition;
 needle:=E' IF v->>''admitted_root_id'' IS NOT NULL THEN';
 replacement:=E' IF v->>''refresh_plan_id'' IS NOT NULL THEN\n'
  ||E'  IF delta<0 AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE id=(v->>''refresh_plan_id'')::uuid AND organization_id=org AND measured_bytes=retained_bytes) THEN RAISE EXCEPTION ''history first display unsettled''; END IF;\n'
  ||E'  UPDATE migration_family_refresh_plan SET measured_bytes=measured_bytes+delta,retained_bytes=retained_bytes+LEAST(delta,0) WHERE id=(v->>''refresh_plan_id'')::uuid AND organization_id=org;\n'
  ||E'  IF delta<0 THEN\n'
  ||E'   UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+delta WHERE organization_id=org;\n'
  ||E'   UPDATE migration_history_capture_run h SET retained_bytes=h.retained_bytes+delta FROM migration_family_refresh_plan p WHERE p.id=(v->>''refresh_plan_id'')::uuid AND p.organization_id=org AND h.id=p.history_capture_id AND h.organization_id=org;\n'
  ||E'  END IF;\n  RETURN COALESCE(NEW,OLD);\n END IF;\n' || needle;
 IF position(needle in definition)=0 THEN RAISE EXCEPTION 'history meter drift'; END IF;
 EXECUTE replace(definition,needle,replacement);
END $$;

-- The initial refresh display participates in existing current-count and
-- suppression paths; immutable source identity survives all display erasures.
DO $$ DECLARE name TEXT; definition TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['crm_history_import_erased_identity()','crm_history_import_display_change()'] LOOP
  SELECT pg_get_functiondef(name::regprocedure) INTO definition;
  IF position('COALESCE(f.manifest_id,f.admitted_manifest_id)' in definition)=0 THEN RAISE EXCEPTION 'history display projection drift'; END IF;
  definition:=replace(definition,'COALESCE(f.manifest_id,f.admitted_manifest_id)','COALESCE(f.manifest_id,f.admitted_manifest_id,f.refresh_manifest_id)');
  IF name='crm_history_import_display_change()' THEN
   definition:=replace(definition,'IF TG_OP=''DELETE'' AND OLD.admitted_root_id IS NULL THEN','IF TG_OP=''DELETE'' AND OLD.admitted_root_id IS NULL AND OLD.refresh_bundle_id IS NULL THEN');
  END IF;
  EXECUTE definition;
 END LOOP;
END $$;
CREATE FUNCTION crm_family_history_first_suppress() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF OLD.refresh_bundle_id IS NOT NULL THEN
  UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=OLD.organization_id AND refresh_bundle_id=OLD.refresh_bundle_id AND refresh_plan_id=OLD.refresh_plan_id AND refresh_manifest_id=OLD.id AND erased_at IS NULL;
 END IF;
 RETURN OLD;
END $$;
CREATE TRIGGER zz_family_history_first_suppress AFTER DELETE ON migration_history_import_display FOR EACH ROW EXECUTE FUNCTION crm_family_history_first_suppress();
CREATE FUNCTION crm_family_history_first_erasure() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF OLD.erased_at IS NULL AND NEW.erased_at IS NOT NULL AND NEW.refresh_bundle_id IS NOT NULL THEN
  DELETE FROM migration_history_import_display WHERE organization_id=NEW.organization_id AND id=NEW.refresh_manifest_id AND refresh_bundle_id=NEW.refresh_bundle_id AND refresh_plan_id=NEW.refresh_plan_id;
 END IF;
 RETURN NEW;
END $$;
-- Counts are decremented by the existing identity erasure trigger before this
-- display deletion. The display-change reader ignores already erased identities.
CREATE TRIGGER zz_family_history_first_erasure AFTER UPDATE ON migration_history_import_identity FOR EACH ROW EXECUTE FUNCTION crm_family_history_first_erasure();
REVOKE ALL ON FUNCTION crm_family_history_first_suppress(),crm_family_history_first_erasure() FROM PUBLIC;

DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_history_head_guard()'::regprocedure) INTO definition;
 IF position('COALESCE(p.capture_id,a.history_capture_id)' in definition)=0 THEN RAISE EXCEPTION 'history bootstrap drift'; END IF;
 definition:=replace(definition,'COALESCE(p.capture_id,a.history_capture_id)','COALESCE(p.capture_id,a.history_capture_id,rp.history_capture_id)');
 definition:=replace(definition,'COALESCE(f.manifest_id,f.admitted_manifest_id)','COALESCE(f.manifest_id,f.admitted_manifest_id,f.refresh_manifest_id)');
 definition:=replace(definition,'LEFT JOIN migration_admitted_history_root a ON a.id=f.admitted_root_id AND a.organization_id=f.organization_id',E'LEFT JOIN migration_admitted_history_root a ON a.id=f.admitted_root_id AND a.organization_id=f.organization_id\n   LEFT JOIN migration_family_refresh_plan rp ON rp.id=f.refresh_plan_id AND rp.bundle_id=f.refresh_bundle_id AND rp.organization_id=f.organization_id');
 definition:=replace(definition,'AND d.admitted_root_id=$5 AND d.admitted_plan_id=$6 AND d.admitted_attempt_id=$7))',E'AND d.admitted_root_id=$5 AND d.admitted_plan_id=$6 AND d.admitted_attempt_id=$7)\n     OR (f.refresh_bundle_id=$9 AND f.refresh_plan_id=$10 AND f.refresh_manifest_id=$11 AND d.refresh_bundle_id=$9 AND d.refresh_plan_id=$10 AND d.refresh_manifest_id=$11))');
 definition:=replace(definition,'i.admitted_attempt_id,i.admitted_manifest_id;','i.admitted_attempt_id,i.admitted_manifest_id,i.refresh_bundle_id,i.refresh_plan_id,i.refresh_manifest_id;');
 EXECUTE definition;
END $$;

DO $$ DECLARE definition TEXT; needle TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_next_owned_history(uuid,uuid,uuid)'::regprocedure) INTO definition;
 needle:='AND COALESCE(r.completed_at,r.updated_at)<=b.created_at)))';
 IF position(needle in definition)=0 THEN RAISE EXCEPTION 'history owned walk drift'; END IF;
 definition:=replace(definition,needle,E'AND COALESCE(r.completed_at,r.updated_at)<=b.created_at))\n'
 ||E' OR (i.refresh_bundle_id IS NOT NULL AND EXISTS(\n'
 ||E'  SELECT 1 FROM migration_family_refresh_bundle owner\n'
 ||E'  JOIN migration_family_refresh_plan p ON p.id=i.refresh_plan_id AND p.bundle_id=owner.id AND p.organization_id=org\n'
 ||E'  JOIN migration_family_refresh_manifest m ON m.id=i.refresh_manifest_id AND m.plan_id=p.id AND m.bundle_id=owner.id AND m.organization_id=org\n'
 ||E'  JOIN migration_family_refresh_cohort co ON co.id=m.cohort_id AND co.bundle_id=owner.id AND co.organization_id=org\n'
 ||E'  JOIN migration_family_refresh_result r ON r.manifest_id=m.id AND r.plan_id=p.id AND r.organization_id=org\n'
 ||E'  WHERE owner.id=i.refresh_bundle_id AND owner.organization_id=org AND owner.parent_import_id=b.parent_import_id AND owner.parent_plan_id=b.parent_plan_id AND owner.source_account_id=b.source_account_id\n'
 ||E'   AND owner.state IN (''completed'',''cancelled'') AND p.state IN (''completed'',''cancelled'') AND p.family=''history'' AND p.confirmed_at IS NOT NULL AND owner.confirmed_at IS NOT NULL\n'
 ||E'   AND owner.updated_at<=b.created_at AND r.committed_at<=b.created_at AND m.disposition=''insert'' AND r.disposition=''applied'' AND r.person_id=i.person_id AND r.target_id=i.fact_id\n'
 ||E'   AND co.person_id=c.person_id AND co.source_person_id=c.source_person_id AND co.original_result_id IS NOT DISTINCT FROM c.original_result_id AND co.admission_result_id IS NOT DISTINCT FROM c.admission_result_id AND co.creation_snapshot_id=c.creation_snapshot_id)))');
 EXECUTE definition;
END $$;
