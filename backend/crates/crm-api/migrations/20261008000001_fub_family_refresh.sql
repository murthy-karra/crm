-- D-092: retained-source family refresh. Existing first-import evidence stays
-- immutable. All new source-bearing payloads are encrypted, bounded and charged.
CREATE TABLE migration_family_refresh_bundle (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES migration_snapshot_storage(organization_id),
 parent_import_id UUID NOT NULL, parent_plan_id UUID NOT NULL, source_account_id BIGINT NOT NULL,
 executor_user_id UUID NOT NULL, engine_version TEXT NOT NULL CHECK(engine_version='fub-family-refresh-v1'),
 core_report_id UUID, core_snapshot_id UUID, history_capture_id UUID,
 state TEXT NOT NULL CHECK(state IN ('preparing','ready','queued','running','paused','completed','cancelled')),
 revision BIGINT NOT NULL DEFAULT 1 CHECK(revision>0), confirmed_at TIMESTAMPTZ,
 source_nonce BYTEA NOT NULL CHECK(octet_length(source_nonce)=24), source_ciphertext BYTEA NOT NULL CHECK(octet_length(source_ciphertext)<=65552),
 digest BYTEA CHECK(digest IS NULL OR octet_length(digest)=32), predecessor_id UUID,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,organization_id),
 FOREIGN KEY(parent_import_id,organization_id) REFERENCES migration_import(id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(core_report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id),
 FOREIGN KEY(core_snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 FOREIGN KEY(history_capture_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id),
 FOREIGN KEY(organization_id,executor_user_id) REFERENCES organization_membership(organization_id,user_id),
 FOREIGN KEY(predecessor_id,organization_id) REFERENCES migration_family_refresh_bundle(id,organization_id),
 CHECK(core_snapshot_id IS NOT NULL OR history_capture_id IS NOT NULL),
 CHECK((core_report_id IS NULL)=(core_snapshot_id IS NULL))
);
CREATE UNIQUE INDEX family_refresh_active_parent ON migration_family_refresh_bundle(organization_id,parent_import_id)
 WHERE state IN ('preparing','ready','queued','running','paused');
CREATE UNIQUE INDEX family_refresh_successor ON migration_family_refresh_bundle(organization_id,predecessor_id) WHERE predecessor_id IS NOT NULL;
CREATE INDEX family_refresh_bundle_page ON migration_family_refresh_bundle(organization_id,parent_import_id,created_at DESC,id DESC);

CREATE TABLE migration_family_refresh_plan (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, organization_id UUID NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('metadata','activity','history')),
 revision BIGINT NOT NULL CHECK(revision>0),
 state TEXT NOT NULL CHECK(state IN ('preparing','ready','queued','running','paused','completed','cancelled','superseded')),
 phase TEXT NOT NULL CHECK(phase IN ('cohort','capture','mappings','classify','apply','finished')),
 source_snapshot_id UUID, history_capture_id UUID,
 predecessor_plan_id UUID,
 lease_token UUID, lease_epoch BIGINT NOT NULL DEFAULT 0 CHECK(lease_epoch>=0), lease_expires_at TIMESTAMPTZ,
 confirmed_at TIMESTAMPTZ, expires_at TIMESTAMPTZ, pause_reason TEXT CHECK(octet_length(pause_reason)<=96),
 checkpoint BIGINT NOT NULL DEFAULT 0 CHECK(checkpoint>=0), checkpoint_id UUID,
 position BIGINT NOT NULL DEFAULT 0 CHECK(position>=0), apply_position BIGINT NOT NULL DEFAULT 0 CHECK(apply_position>=0),
 digest BYTEA CHECK(digest IS NULL OR octet_length(digest)=32),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24), ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65552),
 counts JSONB NOT NULL DEFAULT '{}', results JSONB NOT NULL DEFAULT '{}',
 original_run_byte_limit BIGINT NOT NULL DEFAULT 2147483648 CHECK(original_run_byte_limit>0),
 run_byte_limit BIGINT NOT NULL DEFAULT 2147483648 CHECK(run_byte_limit>=original_run_byte_limit),
 measured_bytes BIGINT NOT NULL DEFAULT 0 CHECK(measured_bytes>=0),
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,bundle_id,organization_id), UNIQUE(id,organization_id), UNIQUE(bundle_id,organization_id,family,revision),
 FOREIGN KEY(bundle_id,organization_id) REFERENCES migration_family_refresh_bundle(id,organization_id),
 FOREIGN KEY(source_snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 FOREIGN KEY(history_capture_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id),
 FOREIGN KEY(predecessor_plan_id,organization_id) REFERENCES migration_family_refresh_plan(id,organization_id),
 CHECK((family='history' AND history_capture_id IS NOT NULL AND source_snapshot_id IS NULL)
    OR (family<>'history' AND source_snapshot_id IS NOT NULL AND history_capture_id IS NULL)),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE UNIQUE INDEX family_refresh_current_family ON migration_family_refresh_plan(bundle_id,organization_id,family) WHERE state<>'superseded';
CREATE INDEX family_refresh_work ON migration_family_refresh_plan(state,lease_expires_at,id) WHERE state IN ('preparing','queued','running');

CREATE TABLE migration_family_refresh_cohort (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, organization_id UUID NOT NULL,
 source_person_id TEXT NOT NULL CHECK(source_person_id ~ '^[1-9][0-9]{0,127}$'), person_id UUID NOT NULL,
 original_result_id UUID, admission_id UUID, admission_result_id UUID,
 creation_snapshot_id UUID NOT NULL,
 UNIQUE(id,bundle_id,organization_id), UNIQUE(bundle_id,organization_id,source_person_id), UNIQUE(bundle_id,organization_id,person_id),
 FOREIGN KEY(bundle_id,organization_id) REFERENCES migration_family_refresh_bundle(id,organization_id),
 FOREIGN KEY(original_result_id,organization_id) REFERENCES migration_import_result(id,organization_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(admission_result_id,organization_id) REFERENCES migration_people_admission_result(id,organization_id),
 FOREIGN KEY(creation_snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 CHECK((original_result_id IS NOT NULL AND admission_id IS NULL AND admission_result_id IS NULL)
    OR (original_result_id IS NULL AND admission_id IS NOT NULL AND admission_result_id IS NOT NULL))
);
CREATE INDEX family_refresh_cohort_page ON migration_family_refresh_cohort(bundle_id,organization_id,id);

CREATE TABLE migration_family_refresh_source (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 capture_id UUID NOT NULL, capture_sequence BIGINT NOT NULL, ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 representation TEXT NOT NULL CHECK(octet_length(representation)<=128),
 kind TEXT NOT NULL CHECK(kind IN ('person','field','user','note','task','event','call','text')),
 source_id TEXT CHECK(octet_length(source_id)<=128), source_person_id TEXT CHECK(octet_length(source_person_id)<=128),
 identity_hmac BYTEA CHECK(identity_hmac IS NULL OR octet_length(identity_hmac)=32),
 semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 qualified BOOLEAN NOT NULL, reason TEXT CHECK(octet_length(reason)<=96),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24), ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=4194320),
 UNIQUE(id,plan_id,bundle_id,organization_id), UNIQUE(plan_id,organization_id,capture_id,ordinal,kind),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id)
);
CREATE INDEX family_refresh_source_identity ON migration_family_refresh_source(plan_id,organization_id,kind,identity_hmac,id);
CREATE INDEX family_refresh_source_person ON migration_family_refresh_source(plan_id,organization_id,kind,source_person_id,id);
CREATE INDEX family_refresh_source_page ON migration_family_refresh_source(plan_id,organization_id,id);

CREATE TABLE migration_family_refresh_mapping (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option','note_author','task_creator','task_assignee','task_kind','timezone')),
 source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32), parent_id UUID,
 disposition TEXT NOT NULL CHECK(disposition IN ('hold','existing','create_matching','unassigned','kind','timezone')),
 target_id UUID, nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24), ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65552),
 UNIQUE(id,plan_id,bundle_id,organization_id), UNIQUE(plan_id,organization_id,kind,source_key_hmac),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id),
 FOREIGN KEY(parent_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_mapping(id,plan_id,bundle_id,organization_id)
);
CREATE INDEX family_refresh_mapping_page ON migration_family_refresh_mapping(plan_id,organization_id,id);

CREATE TABLE migration_family_refresh_manifest (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 cohort_id UUID, source_row_id UUID, position BIGINT NOT NULL CHECK(position>0),
 kind TEXT NOT NULL CHECK(kind IN ('catalog','metadata','note','task','event','call','text')),
 source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32),
 person_id UUID, target_id UUID, baseline_result_id UUID, expected_head_id UUID,
 expected_revision BIGINT CHECK(expected_revision>=0),
 disposition TEXT NOT NULL CHECK(disposition IN ('insert','update','already_current','correction','held','excluded')),
 reason TEXT CHECK(octet_length(reason)<=96), counts JSONB NOT NULL,
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24), ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=67108880),
 added_byte_bound BIGINT NOT NULL CHECK(added_byte_bound>=0 AND added_byte_bound<=67108864),
 UNIQUE(id,plan_id,bundle_id,organization_id), UNIQUE(id,organization_id), UNIQUE(plan_id,organization_id,position),
 UNIQUE(plan_id,organization_id,kind,source_key_hmac),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id),
 FOREIGN KEY(cohort_id,bundle_id,organization_id) REFERENCES migration_family_refresh_cohort(id,bundle_id,organization_id),
 FOREIGN KEY(source_row_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_source(id,plan_id,bundle_id,organization_id)
);
CREATE INDEX family_refresh_manifest_page ON migration_family_refresh_manifest(plan_id,organization_id,position);
CREATE INDEX family_refresh_manifest_filter ON migration_family_refresh_manifest(plan_id,organization_id,kind,disposition,position);

CREATE TABLE migration_family_refresh_result (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL, manifest_id UUID NOT NULL,
 disposition TEXT NOT NULL CHECK(disposition IN ('applied','already_current','held','excluded')),
 person_id UUID, target_id UUID, native_revision BIGINT, reason TEXT CHECK(octet_length(reason)<=96),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24), ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=67108880),
 native_bytes BIGINT NOT NULL DEFAULT 0 CHECK(native_bytes>=0),
 committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,organization_id), UNIQUE(id,plan_id,bundle_id,organization_id), UNIQUE(manifest_id,organization_id),
 FOREIGN KEY(manifest_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id)
);
CREATE INDEX family_refresh_result_page ON migration_family_refresh_result(plan_id,organization_id,id);
CREATE TABLE migration_family_refresh_head (
 organization_id UUID NOT NULL, source_account_id BIGINT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('metadata','note','task','event','call','text')),
 source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32),
 person_id UUID NOT NULL, target_id UUID NOT NULL, result_id UUID NOT NULL,
 version BIGINT NOT NULL CHECK(version>0),
 PRIMARY KEY(organization_id,source_account_id,kind,source_key_hmac),
 FOREIGN KEY(result_id,organization_id) REFERENCES migration_family_refresh_result(id,organization_id)
 -- No target/Person cascade: a removed target never becomes a new identity.
);
CREATE TABLE migration_family_refresh_receipt (
 organization_id UUID NOT NULL, actor_user_id UUID NOT NULL, action TEXT NOT NULL CHECK(octet_length(action)<=32),
 request_id UUID NOT NULL, bundle_id UUID NOT NULL, plan_id UUID NOT NULL,
 input_digest BYTEA NOT NULL CHECK(octet_length(input_digest)=32),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24), ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65552),
 PRIMARY KEY(organization_id,actor_user_id,action,request_id),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id)
);
CREATE TABLE migration_family_refresh_reservation (
 token UUID PRIMARY KEY, organization_id UUID NOT NULL, bundle_id UUID NOT NULL, plan_id UUID NOT NULL,
 lease_epoch BIGINT NOT NULL, purpose TEXT NOT NULL CHECK(purpose IN ('control','unit')),
 byte_count BIGINT NOT NULL CHECK(byte_count>=0),
 UNIQUE(plan_id,organization_id,purpose),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id)
);
CREATE TABLE migration_family_refresh_requirement (
 organization_id UUID PRIMARY KEY REFERENCES organization(id),
 capability TEXT NOT NULL CHECK(capability='fub-family-refresh-v1'),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

DO $$ DECLARE n TEXT; BEGIN
 FOREACH n IN ARRAY ARRAY['cohort','source','mapping','manifest','result','receipt','requirement'] LOOP
  EXECUTE format('GRANT SELECT,INSERT ON migration_family_refresh_%I TO crm_app',n);
  EXECUTE format('CREATE TRIGGER family_refresh_immutable BEFORE UPDATE OR DELETE ON migration_family_refresh_%I FOR EACH ROW EXECUTE FUNCTION reject_mutation()',n);
  EXECUTE format('CREATE TRIGGER family_refresh_no_truncate BEFORE TRUNCATE ON migration_family_refresh_%I FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation()',n);
 END LOOP;
END $$;
GRANT SELECT,INSERT,UPDATE ON migration_family_refresh_bundle,migration_family_refresh_plan,migration_family_refresh_head TO crm_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON migration_family_refresh_reservation TO crm_app;

CREATE FUNCTION crm_family_refresh_capability(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=org)
  AND current_setting('crm.family_refresh_reader',true) IS DISTINCT FROM 'fub-family-refresh-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010G',MESSAGE='family_refresh_capability_required';
 END IF;
END $$;
REVOKE ALL ON FUNCTION crm_family_refresh_capability(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_capability(UUID) TO crm_app;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_workspace_shared(uuid)'::regprocedure) INTO definition;
 IF position('PERFORM crm_people_recovery_capability(org);' IN definition)=0 THEN RAISE EXCEPTION 'unexpected workspace definition'; END IF;
 EXECUTE replace(definition,'PERFORM crm_people_recovery_capability(org);','PERFORM crm_people_recovery_capability(org); PERFORM crm_family_refresh_capability(org);');
END $$;

-- Immutable owner keys and lifecycle fences apply even to already-selected work.
CREATE FUNCTION crm_family_refresh_plan_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle;
BEGIN
 PERFORM crm_family_refresh_capability(NEW.organization_id);
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 IF b.id IS NULL OR NEW.source_snapshot_id IS DISTINCT FROM (CASE WHEN NEW.family='history' THEN NULL ELSE b.core_snapshot_id END)
  OR NEW.history_capture_id IS DISTINCT FROM (CASE WHEN NEW.family='history' THEN b.history_capture_id ELSE NULL END) THEN RAISE EXCEPTION 'family source binding mismatch'; END IF;
 IF TG_OP='INSERT' THEN
  IF NEW.state<>'preparing' OR NEW.confirmed_at IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_epoch<>0 OR b.confirmed_at IS NOT NULL THEN RAISE EXCEPTION 'invalid initial family plan'; END IF;
 ELSE
  IF (to_jsonb(NEW)-ARRAY['state','phase','lease_token','lease_epoch','lease_expires_at','confirmed_at','expires_at','pause_reason','checkpoint','checkpoint_id','position','apply_position','digest','counts','results','measured_bytes','retained_bytes','reserved_bytes','run_byte_limit'])
   IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['state','phase','lease_token','lease_epoch','lease_expires_at','confirmed_at','expires_at','pause_reason','checkpoint','checkpoint_id','position','apply_position','digest','counts','results','measured_bytes','retained_bytes','reserved_bytes','run_byte_limit']) THEN RAISE EXCEPTION 'family plan ownership immutable'; END IF;
  IF OLD.confirmed_at IS NOT NULL AND (NEW.digest IS DISTINCT FROM OLD.digest OR NEW.counts IS DISTINCT FROM OLD.counts OR NEW.confirmed_at IS DISTINCT FROM OLD.confirmed_at) THEN RAISE EXCEPTION 'confirmed family plan immutable'; END IF;
  IF OLD.state IN ('cancelled','completed','superseded') AND to_jsonb(NEW)-ARRAY['measured_bytes','retained_bytes','reserved_bytes'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['measured_bytes','retained_bytes','reserved_bytes'] THEN RAISE EXCEPTION 'terminal family plan'; END IF;
  IF NEW.apply_position<OLD.apply_position OR NEW.position<OLD.position OR NEW.run_byte_limit<OLD.run_byte_limit THEN RAISE EXCEPTION 'family progress regression'; END IF;
  IF NEW.lease_token IS DISTINCT FROM OLD.lease_token AND NEW.lease_token IS NOT NULL THEN
   IF NEW.lease_epoch<>OLD.lease_epoch+1 OR (OLD.lease_token IS NOT NULL AND OLD.lease_expires_at>clock_timestamp()) OR NEW.state NOT IN ('preparing','running') THEN RAISE EXCEPTION 'invalid family lease claim'; END IF;
  ELSIF NEW.lease_epoch<>OLD.lease_epoch THEN RAISE EXCEPTION 'invalid family lease epoch'; END IF;
  IF NEW.lease_token IS NOT NULL AND NEW.lease_token IS NOT DISTINCT FROM OLD.lease_token AND NEW.lease_expires_at IS DISTINCT FROM OLD.lease_expires_at
   AND (OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()) THEN RAISE EXCEPTION 'stale family lease renewal'; END IF;
 END IF;
 IF NEW.lease_token IS NOT NULL AND (NEW.lease_expires_at<=clock_timestamp() OR NEW.lease_expires_at>clock_timestamp()+interval '60 seconds') THEN RAISE EXCEPTION 'invalid family lease duration'; END IF;
 IF NEW.confirmed_at IS NOT NULL AND (NEW.digest IS NULL OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=NEW.organization_id)) THEN RAISE EXCEPTION 'family confirmation capability missing'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_plan_guard BEFORE INSERT OR UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_plan_guard();

CREATE TABLE migration_family_refresh_write_proof (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL, manifest_id UUID NOT NULL,
 table_name TEXT NOT NULL CHECK(table_name IN ('tag','custom_field','custom_field_option','person_tag','person_custom_field_value','note','task')),
 operation TEXT NOT NULL CHECK(operation IN ('INSERT','UPDATE','DELETE')),
 target_id UUID NOT NULL, expected_revision BIGINT CHECK(expected_revision>0), before_hash BYTEA CHECK(before_hash IS NULL OR octet_length(before_hash)=32),
 after_hash BYTEA CHECK(after_hash IS NULL OR octet_length(after_hash)=32),
 CHECK((operation='INSERT' AND before_hash IS NULL AND after_hash IS NOT NULL)
    OR (operation='UPDATE' AND before_hash IS NOT NULL AND after_hash IS NOT NULL)
    OR (operation='DELETE' AND before_hash IS NOT NULL AND after_hash IS NULL)),
 CHECK((table_name IN ('person_tag','person_custom_field_value') OR operation='UPDATE')=(expected_revision IS NOT NULL)),
 CHECK(operation='INSERT' OR table_name IN ('person_tag','person_custom_field_value','note','task')),
 CHECK(operation<>'DELETE' OR table_name IN ('person_tag','person_custom_field_value')),
 UNIQUE(id,manifest_id,organization_id), UNIQUE(manifest_id,organization_id,table_name,operation,target_id),
 FOREIGN KEY(manifest_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id)
);
GRANT SELECT,INSERT ON migration_family_refresh_write_proof TO crm_app;
CREATE TRIGGER family_refresh_proof_immutable BEFORE UPDATE OR DELETE ON migration_family_refresh_write_proof FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER family_refresh_proof_no_truncate BEFORE TRUNCATE ON migration_family_refresh_write_proof FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE FUNCTION crm_family_refresh_owned_insert() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; v JSONB;
BEGIN
 v:=to_jsonb(NEW); PERFORM crm_family_refresh_capability(NEW.organization_id);
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 IF b.id IS NULL THEN RAISE EXCEPTION 'family refresh owner missing'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_cohort' THEN
  IF b.confirmed_at IS NOT NULL OR b.state<>'preparing' THEN RAISE EXCEPTION 'family cohort sealed'; END IF;
  IF NEW.original_result_id IS NOT NULL THEN
   IF NOT EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import_identity i ON i.organization_id=r.organization_id AND i.target_id=r.person_id AND i.source_id=r.source_id AND i.family='people'
    WHERE r.id=NEW.original_result_id AND r.organization_id=b.organization_id AND r.import_id=b.parent_import_id AND r.person_id=NEW.person_id AND r.source_id=NEW.source_person_id AND r.disposition IN ('imported','already_imported') AND i.source_account_id=b.source_account_id AND i.import_id=r.import_id AND i.plan_id=r.plan_id AND i.manifest_id=r.manifest_id
    AND EXISTS(SELECT 1 FROM migration_import parent WHERE parent.id=r.import_id AND parent.organization_id=r.organization_id AND parent.state='completed' AND parent.confirmed_plan_id=r.plan_id AND parent.snapshot_id=NEW.creation_snapshot_id)) THEN RAISE EXCEPTION 'unproven original cohort'; END IF;
  ELSE
   IF NOT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_people_admission a ON a.id=r.admission_id AND a.organization_id=r.organization_id
    JOIN migration_import_identity i ON i.organization_id=r.organization_id AND i.admission_result_id=r.id AND i.admission_item_id=r.item_id AND i.admission_id=a.id AND i.target_id=r.person_id
    WHERE r.id=NEW.admission_result_id AND r.admission_id=NEW.admission_id AND r.organization_id=b.organization_id AND r.person_id=NEW.person_id AND r.source_id=NEW.source_person_id AND r.disposition='settled' AND a.state IN ('completed','cancelled') AND a.parent_import_id=b.parent_import_id AND a.source_account_id=b.source_account_id AND i.source_account_id=b.source_account_id AND i.source_id=r.source_id AND i.family='people' AND NEW.creation_snapshot_id=a.confirmed_snapshot_id) THEN RAISE EXCEPTION 'unproven admitted cohort'; END IF;
  END IF;
 ELSE
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=(v->>'plan_id')::uuid AND bundle_id=b.id AND organization_id=b.organization_id FOR SHARE;
  IF p.id IS NULL THEN RAISE EXCEPTION 'family plan missing'; END IF;
  IF TG_TABLE_NAME='migration_family_refresh_result' THEN
   IF p.state<>'running' OR p.confirmed_at IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
    OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN
    RAISE EXCEPTION 'family result lease invalid';
   END IF;
  ELSIF p.state<>'preparing' OR p.confirmed_at IS NOT NULL THEN RAISE EXCEPTION 'family preparation sealed'; END IF;
 END IF;
 RETURN NEW;
END $$;
DO $$ DECLARE n TEXT; BEGIN
 FOREACH n IN ARRAY ARRAY['cohort','source','mapping','manifest','write_proof','result'] LOOP
  EXECUTE format('CREATE TRIGGER family_refresh_owned_insert BEFORE INSERT ON migration_family_refresh_%I FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_owned_insert()',n);
 END LOOP;
END $$;

CREATE FUNCTION crm_family_refresh_native_digest(v JSONB) RETURNS BYTEA LANGUAGE sql IMMUTABLE AS $$
 SELECT CASE WHEN v IS NULL THEN NULL ELSE sha256(convert_to((v-ARRAY['revision','mobile_revision','metadata_revision'])::text,'UTF8')) END
$$;
CREATE FUNCTION crm_family_refresh_mutation_allowed(org UUID,table_name TEXT,operation TEXT,old_row JSONB,new_row JSONB) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE source_record migration_family_refresh_source; native_metadata_revision BIGINT; proof migration_family_refresh_write_proof; unit migration_family_refresh_manifest; p migration_family_refresh_plan; b migration_family_refresh_bundle; v JSONB; target UUID; columns TEXT[];
BEGIN
 IF NULLIF(current_setting('crm.family_refresh_proof',true),'') IS NULL THEN RETURN false; END IF;
 SELECT * INTO proof FROM migration_family_refresh_write_proof WHERE id::text=current_setting('crm.family_refresh_proof',true) AND organization_id=org;
 IF proof.id IS NULL OR proof.table_name<>table_name OR proof.operation<>operation THEN RETURN false; END IF;
 SELECT * INTO unit FROM migration_family_refresh_manifest WHERE id=proof.manifest_id AND plan_id=proof.plan_id AND bundle_id=proof.bundle_id AND organization_id=org;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=unit.bundle_id AND organization_id=org FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=unit.plan_id AND bundle_id=unit.bundle_id AND organization_id=org FOR SHARE;
 IF p.id IS NULL OR b.id IS NULL OR p.state<>'running' OR p.confirmed_at IS NULL OR b.confirmed_at IS NULL OR b.state NOT IN ('queued','running','paused')
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE manifest_id=unit.id AND organization_id=org)
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=org AND user_id=b.executor_user_id AND role='admin' AND status='active')
  OR NOT EXISTS(SELECT 1 FROM migration_workspace WHERE organization_id=org AND import_id=b.parent_import_id AND plan_id=b.parent_plan_id) THEN RETURN false; END IF;
 IF unit.disposition NOT IN ('insert','update') THEN RETURN false; END IF;
 IF unit.kind<>'catalog' THEN
  IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_cohort c JOIN migration_import_identity i
   ON i.organization_id=c.organization_id AND i.source_id=c.source_person_id AND i.target_id=c.person_id AND i.family='people'
   WHERE c.id=unit.cohort_id AND c.bundle_id=b.id AND c.organization_id=org AND c.person_id=unit.person_id AND i.source_account_id=b.source_account_id) THEN RETURN false; END IF;
  SELECT metadata_revision INTO native_metadata_revision FROM person WHERE id=unit.person_id AND organization_id=org FOR UPDATE;
  IF native_metadata_revision IS NULL THEN RETURN false; END IF;
  IF (SELECT h.result_id FROM migration_family_refresh_head h WHERE h.organization_id=org AND h.source_account_id=b.source_account_id AND h.kind=unit.kind AND h.source_key_hmac=unit.source_key_hmac)
   IS DISTINCT FROM unit.expected_head_id THEN RETURN false; END IF;
 END IF;
 IF table_name IN ('person_tag','person_custom_field_value') AND native_metadata_revision IS DISTINCT FROM proof.expected_revision THEN RETURN false; END IF;
 IF crm_family_refresh_native_digest(old_row) IS DISTINCT FROM proof.before_hash OR crm_family_refresh_native_digest(new_row) IS DISTINCT FROM proof.after_hash THEN RETURN false; END IF;
 v:=COALESCE(new_row,old_row);
 IF (v->>'organization_id')::uuid IS DISTINCT FROM org THEN RETURN false; END IF;
 target:=CASE table_name WHEN 'person_tag' THEN (v->>'tag_id')::uuid WHEN 'person_custom_field_value' THEN (v->>'field_id')::uuid ELSE (v->>'id')::uuid END;
 IF target IS DISTINCT FROM proof.target_id THEN RETURN false; END IF;
 IF table_name IN ('note','task','person_tag','person_custom_field_value') AND (v->>'person_id')::uuid IS DISTINCT FROM unit.person_id THEN RETURN false; END IF;
 IF table_name IN ('note','task') THEN
  IF unit.kind<>table_name OR v->>'origin' IS DISTINCT FROM 'migration' OR v->>'deleted_at' IS NOT NULL OR target IS DISTINCT FROM unit.target_id THEN RETURN false; END IF;
  SELECT * INTO source_record FROM migration_family_refresh_source WHERE id=unit.source_row_id AND plan_id=unit.plan_id AND bundle_id=unit.bundle_id AND organization_id=org;
  IF source_record.id IS NULL OR NOT source_record.qualified OR source_record.source_id IS NULL OR source_record.kind<>table_name
   OR v->>'source' IS DISTINCT FROM 'fub' OR v->>'source_external_id' IS DISTINCT FROM ('v1:'||b.source_account_id::text||':'||source_record.source_id) THEN RETURN false; END IF;
  IF EXISTS(SELECT 1 FROM migration_family_refresh_source sibling WHERE sibling.plan_id=unit.plan_id AND sibling.organization_id=org AND sibling.kind=unit.kind AND sibling.identity_hmac=source_record.identity_hmac AND (NOT sibling.qualified OR sibling.semantic_hmac<>source_record.semantic_hmac OR sibling.source_person_id IS DISTINCT FROM source_record.source_person_id)) THEN RETURN false; END IF;
  IF operation='UPDATE' AND NOT EXISTS(SELECT 1 FROM migration_activity_identity i WHERE i.organization_id=org AND i.source_account_id=b.source_account_id AND i.kind=unit.kind AND i.source_id=source_record.source_id AND i.target_id=target) THEN RETURN false; END IF;
  IF operation='INSERT' AND EXISTS(SELECT 1 FROM migration_activity_identity i WHERE i.organization_id=org AND i.source_account_id=b.source_account_id AND i.kind=unit.kind AND i.source_id=source_record.source_id) THEN RETURN false; END IF;
 END IF;
 IF table_name IN ('person_tag','person_custom_field_value') AND unit.kind<>'metadata' THEN RETURN false; END IF;
 IF table_name IN ('tag','custom_field','custom_field_option') AND (unit.kind<>'catalog' OR operation<>'INSERT') THEN RETURN false; END IF;
 IF operation='UPDATE' THEN
  columns:=CASE table_name WHEN 'note' THEN ARRAY['body','author_user_id','updated_at','correlation_id','revision']
   WHEN 'task' THEN ARRAY['title','kind','due_at','assignee_user_id','created_by_user_id','completed_at','completed_by_user_id','updated_at','correlation_id','revision']
   WHEN 'person_custom_field_value' THEN ARRAY['text_value','number_value','date_value','option_id','updated_by_user_id','updated_at','origin','correlation_id'] ELSE ARRAY[]::text[] END;
  IF old_row-columns IS DISTINCT FROM new_row-columns THEN RETURN false; END IF;
  IF table_name IN ('note','task') AND ((old_row->>'revision')::bigint IS DISTINCT FROM unit.expected_revision OR proof.expected_revision IS DISTINCT FROM unit.expected_revision) THEN RETURN false; END IF;
 END IF;
 IF table_name='task' AND new_row->>'completed_by_user_id' IS NOT NULL THEN RETURN false; END IF;
 RETURN true;
END $$;
REVOKE ALL ON FUNCTION crm_family_refresh_native_digest(JSONB),crm_family_refresh_mutation_allowed(UUID,TEXT,TEXT,JSONB,JSONB),crm_family_refresh_owned_insert(),crm_family_refresh_plan_guard() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_native_digest(JSONB),crm_family_refresh_mutation_allowed(UUID,TEXT,TEXT,JSONB,JSONB) TO crm_app;
DO $$ DECLARE definition TEXT; needle TEXT:='token:=current_setting(''crm.admitted_metadata_permit'',true);'; BEGIN
 SELECT pg_get_functiondef('crm_workspace_mutation_guard()'::regprocedure) INTO definition;
 IF position(needle IN definition)=0 THEN RAISE EXCEPTION 'unexpected workspace mutation guard'; END IF;
 EXECUTE replace(definition,needle,'IF crm_family_refresh_mutation_allowed(org,TG_TABLE_NAME,TG_OP,CASE WHEN TG_OP=''INSERT'' THEN NULL ELSE to_jsonb(OLD) END,CASE WHEN TG_OP=''DELETE'' THEN NULL ELSE to_jsonb(NEW) END) THEN RETURN COALESCE(NEW,OLD); END IF; '||needle);
END $$;

CREATE FUNCTION crm_family_refresh_bundle_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM crm_family_refresh_capability(NEW.organization_id);
 IF TG_OP='INSERT' THEN
  IF NEW.state<>'preparing' OR NEW.confirmed_at IS NOT NULL OR NEW.revision<>1 THEN RAISE EXCEPTION 'invalid initial family bundle'; END IF;
 ELSE
  IF to_jsonb(NEW)-ARRAY['state','revision','confirmed_at','digest','updated_at'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['state','revision','confirmed_at','digest','updated_at'] THEN RAISE EXCEPTION 'family bundle ownership immutable'; END IF;
  IF OLD.state IN ('completed','cancelled') AND NEW.state<>OLD.state THEN RAISE EXCEPTION 'terminal family bundle'; END IF;
  IF NEW.revision<OLD.revision OR NEW.revision>OLD.revision+1 THEN RAISE EXCEPTION 'invalid family bundle revision'; END IF;
  IF OLD.confirmed_at IS NOT NULL AND (NEW.confirmed_at IS DISTINCT FROM OLD.confirmed_at OR NEW.digest IS DISTINCT FROM OLD.digest) THEN RAISE EXCEPTION 'confirmed family bundle immutable'; END IF;
 END IF;
 IF NEW.confirmed_at IS NOT NULL AND (NEW.digest IS NULL OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=NEW.organization_id)) THEN RAISE EXCEPTION 'family confirmation capability missing'; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_workspace w JOIN migration_import i ON i.id=w.import_id AND i.organization_id=w.organization_id
  WHERE w.organization_id=NEW.organization_id AND w.import_id=NEW.parent_import_id AND w.plan_id=NEW.parent_plan_id AND i.state='completed' AND i.source_account_id=NEW.source_account_id AND i.confirmed_plan_id=w.plan_id) THEN RAISE EXCEPTION 'family parent workspace unavailable'; END IF;
 IF NEW.core_report_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_core_change_report r JOIN migration_snapshot s ON s.id=r.newer_snapshot_id AND s.organization_id=r.organization_id
  WHERE r.id=NEW.core_report_id AND r.organization_id=NEW.organization_id AND r.parent_import_id=NEW.parent_import_id AND r.parent_plan_id=NEW.parent_plan_id AND r.source_account_id=NEW.source_account_id AND r.newer_snapshot_id=NEW.core_snapshot_id AND r.state='completed' AND s.state IN ('completed','completed_with_gaps')) THEN RAISE EXCEPTION 'family core source unavailable'; END IF;
 IF NEW.history_capture_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_history_capture_run h WHERE h.id=NEW.history_capture_id AND h.organization_id=NEW.organization_id AND h.parent_import_id=NEW.parent_import_id AND h.parent_plan_id=NEW.parent_plan_id AND h.source_account_id=NEW.source_account_id AND h.state='completed_with_gaps' AND h.completed_at IS NOT NULL AND h.started_at IS NOT NULL
  AND (NEW.core_snapshot_id IS NULL OR h.started_at>(SELECT completed_at FROM migration_snapshot WHERE id=NEW.core_snapshot_id AND organization_id=NEW.organization_id))) THEN RAISE EXCEPTION 'family history source unavailable'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_bundle_guard BEFORE INSERT OR UPDATE ON migration_family_refresh_bundle FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_bundle_guard();

CREATE FUNCTION crm_family_refresh_head_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r migration_family_refresh_result; u migration_family_refresh_manifest; b migration_family_refresh_bundle; p migration_family_refresh_plan;
BEGIN
 PERFORM crm_family_refresh_capability(NEW.organization_id);
 SELECT * INTO r FROM migration_family_refresh_result WHERE id=NEW.result_id AND organization_id=NEW.organization_id;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=r.bundle_id AND organization_id=r.organization_id FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=r.plan_id AND bundle_id=r.bundle_id AND organization_id=r.organization_id FOR SHARE;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=r.manifest_id AND plan_id=r.plan_id AND bundle_id=r.bundle_id AND organization_id=r.organization_id;
 IF r.id IS NULL OR r.disposition NOT IN ('applied','already_current') OR p.state<>'running' OR p.confirmed_at IS NULL
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR u.kind IS DISTINCT FROM NEW.kind OR u.source_key_hmac IS DISTINCT FROM NEW.source_key_hmac OR b.source_account_id IS DISTINCT FROM NEW.source_account_id
  OR r.person_id IS DISTINCT FROM NEW.person_id OR r.target_id IS DISTINCT FROM NEW.target_id OR u.person_id IS DISTINCT FROM NEW.person_id
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=NEW.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN RAISE EXCEPTION 'unproven family head'; END IF;
 IF TG_OP='INSERT' THEN
  IF NEW.version<>1 OR u.expected_head_id IS NOT NULL THEN RAISE EXCEPTION 'stale initial family head'; END IF;
 ELSE
  IF to_jsonb(NEW)-ARRAY['result_id','version'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['result_id','version'] OR NEW.version<>OLD.version+1 OR u.expected_head_id IS DISTINCT FROM OLD.result_id THEN RAISE EXCEPTION 'stale family head'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_head_guard BEFORE INSERT OR UPDATE ON migration_family_refresh_head FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_head_guard();
REVOKE ALL ON FUNCTION crm_family_refresh_bundle_guard(),crm_family_refresh_head_guard() FROM PUBLIC;

-- Match the existing family inventory's fixed row allowance while measuring
-- every variable-width value exactly. Native content is tracked separately.
CREATE FUNCTION crm_family_refresh_retained_size(v JSONB) RETURNS BIGINT LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE total BIGINT:=256; k TEXT; value TEXT;
BEGIN
 IF v IS NULL THEN RETURN 0; END IF;
 FOR k,value IN SELECT e.key,e.value #>> '{}' FROM jsonb_each(v) e LOOP
  IF value IS NULL THEN CONTINUE; END IF;
  IF k IN ('source_nonce','source_ciphertext','digest','nonce','ciphertext','identity_hmac','semantic_hmac','source_key_hmac','input_digest','before_hash','after_hash') THEN
   total:=total+octet_length(decode(substring(value FROM 3),'hex'));
  ELSIF k IN ('source_person_id','source_id','engine_version','state','family','phase','pause_reason','representation','kind','reason','disposition','action','purpose','capability','table_name','operation','counts','results') THEN
   total:=total+octet_length(value);
  END IF;
 END LOOP;
 RETURN total;
END $$;
REVOKE ALL ON FUNCTION crm_family_refresh_retained_size(JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_retained_size(JSONB) TO crm_app;
