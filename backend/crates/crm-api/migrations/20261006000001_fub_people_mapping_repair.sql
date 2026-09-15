-- D-088 / 010e5. Repair approvals are new evidence; original mappings are immutable.
CREATE TABLE migration_mapping_repair_requirement (
 organization_id UUID PRIMARY KEY REFERENCES organization(id),
 capability TEXT NOT NULL CHECK(capability='fub-people-mapping-repair-v1'),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
GRANT SELECT,INSERT ON migration_mapping_repair_requirement TO crm_app;
CREATE TRIGGER migration_mapping_repair_requirement_immutable BEFORE UPDATE OR DELETE
 ON migration_mapping_repair_requirement FOR EACH ROW EXECUTE FUNCTION reject_mutation();

DO $$
DECLARE p TEXT;
BEGIN
 FOREACH p IN ARRAY ARRAY['migration_people_refresh','migration_admitted_people_refresh'] LOOP
  EXECUTE format('ALTER TABLE %I
   ADD COLUMN repair_source_refresh_id UUID,
   ADD COLUMN repair_source_plan_id UUID,
   ADD COLUMN repair_anchor_kind TEXT CHECK(repair_anchor_kind IN (''results'',''preview'')),
   ADD COLUMN repair_draft_revision BIGINT NOT NULL DEFAULT 0 CHECK(repair_draft_revision>=0),
   ADD COLUMN repair_frozen_revision BIGINT,
   ADD COLUMN repair_choices_digest BYTEA CHECK(repair_choices_digest IS NULL OR octet_length(repair_choices_digest)=32),
   ADD COLUMN repair_candidate_count BIGINT NOT NULL DEFAULT 0 CHECK(repair_candidate_count>=0),
   ADD COLUMN repair_candidates_complete BOOLEAN NOT NULL DEFAULT false,
   ADD COLUMN repair_checkpoint_id UUID,
   ADD FOREIGN KEY(repair_source_refresh_id,organization_id) REFERENCES %I(id,organization_id),
   ADD FOREIGN KEY(repair_source_plan_id,repair_source_refresh_id,organization_id) REFERENCES %I(id,refresh_id,organization_id),
   ADD CHECK((repair_source_refresh_id IS NULL AND repair_source_plan_id IS NULL AND repair_anchor_kind IS NULL)
      OR (repair_source_refresh_id IS NOT NULL AND repair_source_plan_id IS NOT NULL AND repair_anchor_kind IS NOT NULL)),
   ADD CHECK(repair_source_refresh_id IS NULL OR repair_source_refresh_id<>id)',p,p,p||'_plan');
  EXECUTE format('ALTER TABLE %I ADD COLUMN repair_choices_revision BIGINT,
   ADD COLUMN repair_choices_digest BYTEA,
   ADD COLUMN repair_candidate_count BIGINT NOT NULL DEFAULT 0,
   ADD COLUMN repair_approval_only_count BIGINT NOT NULL DEFAULT 0,
   ADD COLUMN repair_unassigned_count BIGINT NOT NULL DEFAULT 0',p||'_plan');
  EXECUTE format('ALTER TABLE %I ADD UNIQUE(id,refresh_id,item_id,organization_id)',p||'_result');
  EXECUTE format('ALTER TABLE %I ADD FOREIGN KEY(settled_result_id,refresh_id,id,organization_id) REFERENCES %I(id,refresh_id,item_id,organization_id)',p||'_item',p||'_result');
  EXECUTE format('CREATE TABLE %I (
   refresh_id UUID NOT NULL, organization_id UUID NOT NULL,
   kind TEXT NOT NULL CHECK(kind IN (''stage'',''assignee'')),
   source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32),
   id UUID NOT NULL UNIQUE, nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),
   ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65536),
   PRIMARY KEY(refresh_id,organization_id,kind,source_key_hmac),
   FOREIGN KEY(refresh_id,organization_id) REFERENCES %I(id,organization_id))',p||'_repair_key',p);
  EXECUTE format('CREATE TABLE %I (
   id UUID PRIMARY KEY, refresh_id UUID NOT NULL, organization_id UUID NOT NULL,
   revision BIGINT NOT NULL CHECK(revision>0),
   kind TEXT NOT NULL CHECK(kind IN (''stage'',''assignee'')),
   source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32),
   disposition TEXT NOT NULL CHECK(disposition IN (''existing'',''member'',''unassigned'',''unresolved'')),
   target_id UUID, nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),
   ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65536),
   approved_by_user_id UUID NOT NULL REFERENCES app_user(id),
   created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
   UNIQUE(id,refresh_id,organization_id),
   UNIQUE(refresh_id,revision,kind,source_key_hmac),
   FOREIGN KEY(refresh_id,organization_id) REFERENCES %I(id,organization_id),
   FOREIGN KEY(refresh_id,organization_id,kind,source_key_hmac) REFERENCES %I(refresh_id,organization_id,kind,source_key_hmac),
   CHECK((disposition=''existing'' AND kind=''stage'' AND target_id IS NOT NULL)
      OR (disposition=''member'' AND kind=''assignee'' AND target_id IS NOT NULL)
      OR (disposition=''unassigned'' AND kind=''assignee'' AND target_id IS NULL)
      OR (disposition=''unresolved'' AND target_id IS NULL)))',p||'_repair_choice',p,p||'_repair_key');
  EXECUTE format('ALTER TABLE %I ADD COLUMN inherited_choice_id UUID, ADD COLUMN inherited_refresh_id UUID, ADD FOREIGN KEY(inherited_choice_id,inherited_refresh_id,organization_id) REFERENCES %I(id,refresh_id,organization_id), ADD CHECK((inherited_choice_id IS NULL)=(inherited_refresh_id IS NULL))',p||'_repair_choice',p||'_repair_choice');
  EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id,kind,source_key_hmac,revision DESC)',p||'_repair_choice');
  EXECUTE format('CREATE TABLE %I (
   refresh_id UUID NOT NULL, organization_id UUID NOT NULL,
   anchor_refresh_id UUID NOT NULL, anchor_plan_id UUID NOT NULL, anchor_item_id UUID NOT NULL,
   source_id TEXT NOT NULL CHECK(source_id ~ ''^[1-9][0-9]{0,127}$''), person_id UUID NOT NULL,
   successful_result_id UUID NOT NULL,
   stage_source_hmac BYTEA CHECK(stage_source_hmac IS NULL OR octet_length(stage_source_hmac)=32),
   assignee_source_hmac BYTEA CHECK(assignee_source_hmac IS NULL OR octet_length(assignee_source_hmac)=32),
   PRIMARY KEY(refresh_id,organization_id,source_id),
   UNIQUE(refresh_id,organization_id,anchor_item_id),
   FOREIGN KEY(refresh_id,organization_id) REFERENCES %I(id,organization_id),
   FOREIGN KEY(anchor_plan_id,anchor_refresh_id,organization_id) REFERENCES %I(id,refresh_id,organization_id),
   FOREIGN KEY(anchor_item_id,anchor_refresh_id,organization_id) REFERENCES %I(id,refresh_id,organization_id),
   FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id))',
   p||'_repair_candidate',p,p||'_plan',p||'_item');
  EXECUTE format('ALTER TABLE %I ADD FOREIGN KEY(successful_result_id,organization_id) REFERENCES %I(id,organization_id)',p||'_repair_candidate',CASE p WHEN 'migration_people_refresh' THEN 'migration_import_result' ELSE 'migration_people_admission_result' END);
  EXECUTE format('CREATE INDEX ON %I(organization_id,anchor_item_id,refresh_id)',p||'_repair_candidate');
  EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id,stage_source_hmac)',p||'_repair_candidate');
  EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id,assignee_source_hmac)',p||'_repair_candidate');
  IF p='migration_admitted_people_refresh' THEN
   EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id,plan_id,id)',p||'_item');
  END IF;
  EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id,plan_id,disposition,id)',p||'_item');
  EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id,disposition,item_id)',p||'_result');
  EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id,item_id) WHERE disposition IN (''held_mapping_gap'',''held_stale'')',p||'_result');
  EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id,id)',p||'_repair_key');
  EXECUTE format('CREATE TABLE %I (
   id UUID PRIMARY KEY, organization_id UUID NOT NULL, person_id UUID NOT NULL,
   source_account_id BIGINT NOT NULL, source_id TEXT NOT NULL,
   kind TEXT NOT NULL CHECK(kind IN (''stage'',''assignee'')),
   source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32),
   refresh_id UUID NOT NULL, plan_id UUID NOT NULL, item_id UUID NOT NULL, result_id UUID NOT NULL,
   choice_id UUID NOT NULL, choice_refresh_id UUID NOT NULL, previous_binding_id UUID,
   created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
   UNIQUE(id,organization_id), UNIQUE(id,organization_id,person_id,source_account_id,kind,source_key_hmac),
   UNIQUE(result_id,kind),
   FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id),
   FOREIGN KEY(plan_id,refresh_id,organization_id) REFERENCES %I(id,refresh_id,organization_id),
   FOREIGN KEY(result_id,refresh_id,item_id,organization_id) REFERENCES %I(id,refresh_id,item_id,organization_id),
   FOREIGN KEY(choice_id,choice_refresh_id,organization_id) REFERENCES %I(id,refresh_id,organization_id),
   FOREIGN KEY(previous_binding_id,organization_id) REFERENCES %I(id,organization_id))',
   p||'_mapping_binding',p||'_plan',p||'_result',p||'_repair_choice',p||'_mapping_binding');
  EXECUTE format('CREATE TABLE %I (
   organization_id UUID NOT NULL, person_id UUID NOT NULL, source_account_id BIGINT NOT NULL,
   kind TEXT NOT NULL CHECK(kind IN (''stage'',''assignee'')), source_key_hmac BYTEA NOT NULL,
   binding_id UUID NOT NULL, version BIGINT NOT NULL CHECK(version>0),
   PRIMARY KEY(organization_id,person_id,source_account_id,kind,source_key_hmac),
   FOREIGN KEY(binding_id,organization_id,person_id,source_account_id,kind,source_key_hmac)
     REFERENCES %I(id,organization_id,person_id,source_account_id,kind,source_key_hmac))',p||'_mapping_head',p||'_mapping_binding');
  EXECUTE format('CREATE INDEX ON %I(refresh_id,organization_id)',p||'_mapping_binding');
  EXECUTE format('CREATE INDEX ON %I(binding_id,organization_id)',p||'_mapping_head');
  EXECUTE format('ALTER TABLE %I
   ADD COLUMN repair_stage_choice_id UUID, ADD COLUMN repair_stage_choice_refresh_id UUID,
   ADD COLUMN repair_assignee_choice_id UUID, ADD COLUMN repair_assignee_choice_refresh_id UUID,
   ADD COLUMN repair_stage_source_hmac BYTEA, ADD COLUMN repair_assignee_source_hmac BYTEA,
   ADD COLUMN repair_stage_head_id UUID, ADD COLUMN repair_assignee_head_id UUID,
   ADD COLUMN repair_stage_head_version BIGINT NOT NULL DEFAULT 0,
   ADD COLUMN repair_assignee_head_version BIGINT NOT NULL DEFAULT 0,
   ADD COLUMN repair_approval_only BOOLEAN NOT NULL DEFAULT false,
   ADD COLUMN mapping_evidence_nonce BYTEA,
   ADD COLUMN mapping_evidence_ciphertext BYTEA,
   ADD COLUMN native_fingerprint BYTEA,
   ADD FOREIGN KEY(repair_stage_choice_id,repair_stage_choice_refresh_id,organization_id) REFERENCES %I(id,refresh_id,organization_id),
   ADD FOREIGN KEY(repair_assignee_choice_id,repair_assignee_choice_refresh_id,organization_id) REFERENCES %I(id,refresh_id,organization_id),
   ADD FOREIGN KEY(repair_stage_head_id,organization_id) REFERENCES %I(id,organization_id),
   ADD FOREIGN KEY(repair_assignee_head_id,organization_id) REFERENCES %I(id,organization_id),
   ADD CHECK((repair_stage_choice_id IS NULL)=(repair_stage_choice_refresh_id IS NULL)),
   ADD CHECK((repair_assignee_choice_id IS NULL)=(repair_assignee_choice_refresh_id IS NULL)),
   ADD CHECK((mapping_evidence_nonce IS NULL)=(mapping_evidence_ciphertext IS NULL))',
   p||'_item',p||'_repair_choice',p||'_repair_choice',p||'_mapping_binding',p||'_mapping_binding');
  EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I',p||'_receipt',p||'_receipt_action_check');
  EXECUTE format('ALTER TABLE %I ADD CHECK(action IN (''prepare'',''repreview'',''confirm'',''retry'',''cancel'',''mapping_repair'',''mapping_choices''))',p||'_receipt');
  EXECUTE format('GRANT SELECT,INSERT ON %I,%I,%I,%I TO crm_app',p||'_repair_choice',p||'_repair_candidate',p||'_mapping_binding',p||'_repair_key');
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON %I TO crm_app',p||'_mapping_head');
  EXECUTE format('CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_mutation()',p||'_repair_choice');
  EXECUTE format('CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_mutation()',p||'_repair_candidate');
  EXECUTE format('CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_mutation()',p||'_mapping_binding');
  EXECUTE format('CREATE TRIGGER immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_mutation()',p||'_repair_key');
 END LOOP;
END $$;

CREATE FUNCTION crm_mapping_repair_identity(owner_name TEXT, root JSONB, item JSONB) RETURNS BOOLEAN
LANGUAGE plpgsql STABLE SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE valid BOOLEAN;
BEGIN
 IF owner_name='migration_people_refresh' THEN
  SELECT EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.import_id=r.import_id AND mi.manifest_id=r.manifest_id AND mi.source_id=r.source_id AND mi.target_id=r.person_id AND mi.family='people'
   WHERE r.id=(item->>'original_result_id')::uuid AND r.import_id=(root->>'parent_import_id')::uuid AND r.organization_id=(root->>'organization_id')::uuid AND r.source_id=item->>'source_id' AND r.person_id=(item->>'person_id')::uuid AND r.disposition='imported' AND mi.source_account_id=(root->>'source_account_id')::bigint) INTO valid;
 ELSIF owner_name='migration_admitted_people_refresh' THEN
  SELECT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.admission_result_id=r.id AND mi.admission_id=r.admission_id AND mi.admission_item_id=r.item_id AND mi.source_id=r.source_id AND mi.target_id=r.person_id AND mi.family='people'
   WHERE r.id=(item->>'admission_result_id')::uuid AND r.admission_id=(root->>'admission_id')::uuid AND r.organization_id=(root->>'organization_id')::uuid AND r.source_id=item->>'source_id' AND r.person_id=(item->>'person_id')::uuid AND r.disposition='settled' AND mi.source_account_id=(root->>'source_account_id')::bigint) INTO valid;
 ELSE RETURN false; END IF;
 RETURN valid AND EXISTS(SELECT 1 FROM migration_snapshot_record r JOIN migration_snapshot_capture c ON c.id=r.capture_id AND c.snapshot_id=r.snapshot_id AND c.organization_id=r.organization_id WHERE r.organization_id=(root->>'organization_id')::uuid AND r.snapshot_id=(root->>'newer_snapshot_id')::uuid AND r.source_id=item->>'source_id' AND r.family='people' AND r.capture_id=(item->>'source_capture_id')::uuid AND r.ordinal=(item->>'source_ordinal')::integer AND c.accepted AND NOT c.truncated AND c.classification='success' AND c.http_status BETWEEN 200 AND 299);
END $$;
REVOKE ALL ON FUNCTION crm_mapping_repair_identity(TEXT,JSONB,JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_mapping_repair_identity(TEXT,JSONB,JSONB) TO crm_app;

CREATE FUNCTION crm_mapping_repair_mutation_evidence(org UUID, owner_name TEXT, permit TEXT, table_name TEXT) RETURNS BOOLEAN
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE unit UUID; lease UUID; item JSONB; root JSONB; baseline JSONB; cohort TEXT;
BEGIN
 IF owner_name NOT IN ('migration_people_refresh','migration_admitted_people_refresh') THEN RETURN false; END IF;
 PERFORM crm_mapping_repair_capability(org);
 BEGIN unit:=(permit::jsonb->>'item')::uuid; lease:=(permit::jsonb->>'lease')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 EXECUTE format('SELECT to_jsonb(i),to_jsonb(r) FROM %I i JOIN %I r ON r.id=i.refresh_id AND r.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2 AND r.lease_token=$3 AND r.state=''running'' AND r.lease_expires_at>clock_timestamp() AND i.plan_id=r.confirmed_refresh_plan_id AND i.settled_at IS NULL',owner_name||'_item',owner_name) INTO item,root USING unit,org,lease;
 IF item IS NULL OR item->>'mapping_evidence_nonce' IS NULL OR NOT crm_mapping_repair_identity(owner_name,root,item) THEN RETURN false; END IF;
 cohort:=CASE owner_name WHEN 'migration_people_refresh' THEN 'parent_import_id' ELSE 'admission_id' END;
 EXECUTE format('SELECT to_jsonb(b) FROM %I b WHERE organization_id=$1 AND %I=$2 AND source_id=$3 AND person_id=$4',owner_name||'_baseline',cohort) INTO baseline USING org,(root->>cohort)::uuid,item->>'source_id',(item->>'person_id')::uuid;
 IF baseline IS NULL OR baseline->>'version' IS DISTINCT FROM item->>'baseline_version' OR baseline->>'result_id' IS DISTINCT FROM item->>'baseline_result_id' THEN RETURN false; END IF;
 IF table_name='person' AND decode(substr(item->>'native_fingerprint',3),'hex') IS DISTINCT FROM crm_mapping_repair_native_fingerprint(org,(item->>'person_id')::uuid) THEN RETURN false; END IF;
 RETURN true;
END $$;
REVOKE ALL ON FUNCTION crm_mapping_repair_mutation_evidence(UUID,TEXT,TEXT,TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_mapping_repair_mutation_evidence(UUID,TEXT,TEXT,TEXT) TO crm_app;

CREATE FUNCTION crm_mapping_repair_native_fingerprint(org UUID, target UUID) RETURNS BYTEA
LANGUAGE sql STABLE SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
 SELECT sha256(convert_to(jsonb_build_object('first_name',p.first_name,'last_name',p.last_name,
  'stage',p.stage_id,'assignee',p.assigned_user_id,'contacts',
  COALESCE((SELECT jsonb_agg(jsonb_build_array(c.id,c.kind,c.value,c.normalized_value,c.import_order) ORDER BY c.kind,c.import_order,c.id)
    FROM contact_method c WHERE c.person_id=p.id AND c.organization_id=p.organization_id),'[]'::jsonb))::text,'UTF8'))
 FROM person p WHERE p.id=target AND p.organization_id=org
$$;
REVOKE ALL ON FUNCTION crm_mapping_repair_native_fingerprint(UUID,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_mapping_repair_native_fingerprint(UUID,UUID) TO crm_app;

-- Row locks require UPDATE privileges in PostgreSQL. This narrow, fixed-query
-- helper locks a target without granting application writes to stage/app_user.
CREATE FUNCTION crm_mapping_repair_target(org UUID, kind TEXT, target UUID) RETURNS JSONB
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE label TEXT;
BEGIN
 IF kind='stage' THEN
  SELECT name INTO label FROM stage WHERE organization_id=org AND id=target FOR SHARE;
  IF FOUND THEN RETURN jsonb_build_object('id',target,'name',label); END IF;
 ELSIF kind='assignee' THEN
  SELECT u.email INTO label FROM organization_membership m JOIN app_user u ON u.id=m.user_id
   WHERE m.organization_id=org AND m.user_id=target AND m.status='active' FOR SHARE OF m,u;
  IF FOUND THEN RETURN jsonb_build_object('id',target,'email',label); END IF;
 END IF;
 RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION crm_mapping_repair_target(UUID,TEXT,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_mapping_repair_target(UUID,TEXT,UUID) TO crm_app;

-- These guards apply to both owners but never accept a request-supplied table.
CREATE FUNCTION crm_mapping_repair_owned_write() RETURNS trigger LANGUAGE plpgsql
SET search_path=pg_catalog,public,pg_temp AS $$
#variable_conflict use_variable
DECLARE p TEXT:=TG_ARGV[0]; family TEXT:=TG_ARGV[1]; n JSONB:=to_jsonb(NEW);
 root JSONB; item JSONB; plan JSONB; source JSONB; binding JSONB; head JSONB;
 valid BOOLEAN; kind TEXT; choice JSONB; column_prefix TEXT; permit JSONB; cohort TEXT;
BEGIN
 IF family='root' THEN root:=n;
 ELSE
  EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE id=$1 AND organization_id=$2',p)
   INTO root USING (n->>'refresh_id')::uuid,(n->>'organization_id')::uuid;
 END IF;
 cohort:=CASE p WHEN 'migration_people_refresh' THEN 'parent_import_id' ELSE 'admission_id' END;
 IF family='root' THEN
  IF TG_OP='UPDATE' AND (to_jsonb(OLD)-ARRAY['state','pause_reason','lease_token','lease_epoch','lease_expires_at',
   'preparation_checkpoint_key','preparation_checkpoint_id','preparation_phase','checkpoint_id','lifecycle_revision',
   'retained_bytes','reserved_bytes','settled_items','updated_at','completed_at','cancelled_at',
   'confirmed_boundary','confirmed_snapshot_id','confirmed_started_at','confirmed_completed_at','confirmed_refresh_plan_id',
   'repair_draft_revision','repair_frozen_revision','repair_choices_digest','repair_candidate_count','repair_candidates_complete','repair_checkpoint_id'])
   IS DISTINCT FROM (n-ARRAY['state','pause_reason','lease_token','lease_epoch','lease_expires_at',
   'preparation_checkpoint_key','preparation_checkpoint_id','preparation_phase','checkpoint_id','lifecycle_revision',
   'retained_bytes','reserved_bytes','settled_items','updated_at','completed_at','cancelled_at',
   'confirmed_boundary','confirmed_snapshot_id','confirmed_started_at','confirmed_completed_at','confirmed_refresh_plan_id',
   'repair_draft_revision','repair_frozen_revision','repair_choices_digest','repair_candidate_count','repair_candidates_complete','repair_checkpoint_id']) THEN
    RAISE EXCEPTION 'refresh owner is immutable';
  END IF;
  IF n->>'repair_source_refresh_id' IS NOT NULL THEN
   EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE id=$1 AND organization_id=$2',p) INTO source
    USING (n->>'repair_source_refresh_id')::uuid,(n->>'organization_id')::uuid;
   IF source IS NULL OR source->>'parent_import_id' IS DISTINCT FROM n->>'parent_import_id'
    OR source->>'parent_plan_id' IS DISTINCT FROM n->>'parent_plan_id'
    OR source->>'source_account_id' IS DISTINCT FROM n->>'source_account_id'
    OR source->>'admission_id' IS DISTINCT FROM n->>'admission_id' THEN RAISE EXCEPTION 'repair owner mismatch'; END IF;
   IF TG_OP='INSERT' AND source->>'state' NOT IN ('completed','cancelled') THEN RAISE EXCEPTION 'repair source not terminal'; END IF;
   IF n->>'confirmed_refresh_plan_id' IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_mapping_repair_requirement WHERE organization_id=(n->>'organization_id')::uuid) THEN RAISE EXCEPTION 'repair admission required'; END IF;
   IF TG_OP='UPDATE' AND OLD.confirmed_refresh_plan_id IS NOT NULL AND
     (to_jsonb(OLD)->'repair_choices_digest' IS DISTINCT FROM n->'repair_choices_digest'
      OR to_jsonb(OLD)->'repair_frozen_revision' IS DISTINCT FROM n->'repair_frozen_revision') THEN RAISE EXCEPTION 'confirmed choices are immutable'; END IF;
  END IF;
  RETURN NEW;
 END IF;
 IF family='candidate' THEN
  EXECUTE format('SELECT to_jsonb(i) FROM %I i WHERE id=$1 AND refresh_id=$2 AND plan_id=$3 AND organization_id=$4',p||'_item') INTO item
   USING NEW.anchor_item_id,NEW.anchor_refresh_id,NEW.anchor_plan_id,NEW.organization_id;
  IF root->>'state'<>'preparing' OR (root->>'repair_candidates_complete')::boolean OR item IS NULL
   OR item->>'source_id' IS DISTINCT FROM n->>'source_id' OR item->>'person_id' IS DISTINCT FROM n->>'person_id'
   OR root->>'repair_source_refresh_id' IS DISTINCT FROM n->>'anchor_refresh_id'
   OR root->>'repair_source_plan_id' IS DISTINCT FROM n->>'anchor_plan_id' THEN RAISE EXCEPTION 'invalid repair candidate'; END IF;
  EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I z WHERE z.refresh_id=$1 AND z.item_id=$2 AND z.organization_id=$3 AND z.disposition IN (''held_mapping_gap'',''held_stale''))',p||'_result') INTO valid USING NEW.anchor_refresh_id,NEW.anchor_item_id,NEW.organization_id;
  EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE id=$1 AND organization_id=$2',p) INTO source USING NEW.anchor_refresh_id,NEW.organization_id;
  IF source->>'state'='cancelled' AND source->>'repair_source_refresh_id' IS NOT NULL AND source->>'confirmed_refresh_plan_id'=root->>'repair_source_plan_id' THEN
   IF source->>'report_id' IS DISTINCT FROM root->>'report_id' OR item->>'settled_at' IS NOT NULL OR item->>'disposition' NOT IN ('eligible','already_current') THEN RAISE EXCEPTION 'remainder requires unfinished frozen candidate'; END IF;
  ELSIF item->>'disposition'<>'held_mapping_gap' AND NOT (root->>'repair_anchor_kind'='results' AND valid) THEN RAISE EXCEPTION 'candidate is not held'; END IF;
  IF n->>'successful_result_id' IS DISTINCT FROM item->>(CASE p WHEN 'migration_people_refresh' THEN 'original_result_id' ELSE 'admission_result_id' END) THEN RAISE EXCEPTION 'candidate success mismatch'; END IF;
  IF p='migration_people_refresh' THEN
   SELECT EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.manifest_id=r.manifest_id AND mi.import_id=r.import_id AND mi.source_id=r.source_id AND mi.target_id=r.person_id AND mi.family='people'
    WHERE r.import_id=(root->>'parent_import_id')::uuid AND r.organization_id=NEW.organization_id AND r.person_id=NEW.person_id AND r.source_id=NEW.source_id AND r.disposition='imported' AND mi.source_account_id=(root->>'source_account_id')::bigint) INTO valid;
  ELSE
   SELECT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.admission_result_id=r.id AND mi.target_id=r.person_id AND mi.source_id=r.source_id AND mi.family='people'
    WHERE r.admission_id=(root->>'admission_id')::uuid AND r.organization_id=NEW.organization_id AND r.person_id=NEW.person_id AND r.source_id=NEW.source_id AND r.disposition='settled' AND mi.source_account_id=(root->>'source_account_id')::bigint) INTO valid;
  END IF;
  IF NOT valid THEN RAISE EXCEPTION 'candidate requires successful Person identity'; END IF;
 ELSIF family='plan' THEN
  IF TG_OP='UPDATE' AND OLD.sealed_at IS NOT NULL AND (to_jsonb(OLD)-'state') IS DISTINCT FROM (n-'state') THEN RAISE EXCEPTION 'sealed plan is immutable'; END IF;
  IF NEW.sealed_at IS NOT NULL AND root->>'repair_source_refresh_id' IS NOT NULL AND
    (n->>'repair_choices_revision' IS DISTINCT FROM root->>'repair_frozen_revision' OR n->>'repair_choices_digest' IS DISTINCT FROM root->>'repair_choices_digest' OR n->>'repair_candidate_count' IS DISTINCT FROM root->>'repair_candidate_count') THEN RAISE EXCEPTION 'plan choices mismatch'; END IF;
 ELSIF family='choice' THEN
  IF NEW.inherited_choice_id IS NOT NULL THEN
   EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE id=$1 AND organization_id=$2',p) INTO source USING NEW.inherited_refresh_id,NEW.organization_id;
   EXECUTE format('SELECT to_jsonb(c) FROM %I c WHERE id=$1 AND refresh_id=$2 AND organization_id=$3',p||'_repair_choice') INTO choice USING NEW.inherited_choice_id,NEW.inherited_refresh_id,NEW.organization_id;
   IF root->>'state'<>'preparing' OR (root->>'repair_candidates_complete')::boolean OR NEW.revision<>1
    OR root->>'repair_source_refresh_id' IS DISTINCT FROM source->>'id' OR source->>'state'<>'cancelled'
    OR source->>'confirmed_refresh_plan_id' IS DISTINCT FROM root->>'repair_source_plan_id'
    OR source->>'report_id' IS DISTINCT FROM root->>'report_id' OR choice IS NULL
    OR (choice->>'revision')::bigint>(source->>'repair_frozen_revision')::bigint
    OR choice->>'kind' IS DISTINCT FROM n->>'kind' OR choice->>'source_key_hmac' IS DISTINCT FROM n->>'source_key_hmac'
    OR choice->>'disposition' IS DISTINCT FROM n->>'disposition' OR choice->>'target_id' IS DISTINCT FROM n->>'target_id'
    OR NEW.approved_by_user_id<>(root->>'initiated_by_user_id')::uuid THEN RAISE EXCEPTION 'invalid inherited choice'; END IF;
   RETURN NEW;
  END IF;
  IF root->>'confirmed_refresh_plan_id' IS NOT NULL OR root->>'repair_source_refresh_id' IS NULL
   OR root->>'state' NOT IN ('paused','ready') OR NEW.revision<>(root->>'repair_draft_revision')::bigint+1
   OR NEW.approved_by_user_id<>(root->>'initiated_by_user_id')::uuid THEN RAISE EXCEPTION 'choice revision or owner mismatch'; END IF;
 ELSIF family='item' THEN
  EXECUTE format('SELECT to_jsonb(v) FROM %I v WHERE id=$1 AND refresh_id=$2 AND organization_id=$3',p||'_plan') INTO plan USING NEW.plan_id,NEW.refresh_id,NEW.organization_id;
  IF TG_OP='UPDATE' AND plan->>'state'<>'building' AND
   (to_jsonb(OLD)-ARRAY['settled_result_id','settled_at']) IS DISTINCT FROM (n-ARRAY['settled_result_id','settled_at']) THEN RAISE EXCEPTION 'sealed item is immutable'; END IF;
  -- A held result must remain recordable when its old mapping was invalidated.
  IF TG_OP='UPDATE' AND plan->>'state'<>'building' THEN
   IF OLD.settled_at IS NOT NULL AND (OLD.settled_result_id IS DISTINCT FROM NEW.settled_result_id OR OLD.settled_at IS DISTINCT FROM NEW.settled_at) THEN RAISE EXCEPTION 'settlement is immutable'; END IF;
   RETURN NEW;
  END IF;
  IF NEW.mapping_evidence_nonce IS NOT NULL THEN
   IF octet_length(NEW.mapping_evidence_nonce)<>24 OR octet_length(NEW.mapping_evidence_ciphertext)>131072 OR octet_length(NEW.native_fingerprint)<>32 THEN RAISE EXCEPTION 'invalid mapping evidence'; END IF;
   FOREACH kind IN ARRAY ARRAY['stage','assignee'] LOOP
    column_prefix:='repair_'||kind;
    IF n->>(column_prefix||'_choice_id') IS NOT NULL AND n->>(kind||'_mapping_id') IS NOT NULL THEN RAISE EXCEPTION 'mixed mapping authority'; END IF;
    IF n->>(kind||'_mapping_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_import_mapping m WHERE m.id=(n->>(kind||'_mapping_id'))::uuid AND m.organization_id=NEW.organization_id AND m.plan_id=(root->>'parent_plan_id')::uuid AND m.kind=kind AND m.qualified) THEN RAISE EXCEPTION 'wrong original mapping owner'; END IF;
    IF n->>(column_prefix||'_choice_id') IS NOT NULL THEN
     EXECUTE format('SELECT to_jsonb(c) FROM %I c WHERE id=$1 AND refresh_id=$2 AND organization_id=$3',p||'_repair_choice') INTO choice USING (n->>(column_prefix||'_choice_id'))::uuid,(n->>(column_prefix||'_choice_refresh_id'))::uuid,NEW.organization_id;
     IF choice IS NULL OR choice->>'kind'<>kind OR choice->>( 'source_key_hmac') IS DISTINCT FROM n->>(column_prefix||'_source_hmac') THEN RAISE EXCEPTION 'wrong repair key'; END IF;
     IF n->>(column_prefix||'_choice_refresh_id')=root->>'id' THEN
      EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I c WHERE c.refresh_id=$1 AND c.organization_id=$2 AND c.source_id=$3 AND c.person_id=$4 AND %I=$5)',p||'_repair_candidate',CASE kind WHEN 'stage' THEN 'stage_source_hmac' ELSE 'assignee_source_hmac' END) INTO valid USING NEW.refresh_id,NEW.organization_id,NEW.source_id,NEW.person_id,decode(substr(n->>(column_prefix||'_source_hmac'),3),'hex');
      IF NOT valid THEN RAISE EXCEPTION 'choice outside frozen candidates'; END IF;
     END IF;
    END IF;
   END LOOP;
  END IF;
 ELSIF family='baseline' AND TG_OP='UPDATE' THEN
  EXECUTE format('SELECT to_jsonb(i) FROM %I i JOIN %I z ON z.item_id=i.id AND z.refresh_id=i.refresh_id AND z.organization_id=i.organization_id WHERE z.id=$1 AND z.refresh_id=$2 AND z.organization_id=$3 AND z.disposition IN (''settled'',''settled_noop'')',p||'_item',p||'_result') INTO item USING NEW.result_id,NEW.refresh_id,NEW.organization_id;
  IF item IS NULL OR NEW.version<>OLD.version+1 OR item->>'baseline_result_id' IS DISTINCT FROM to_jsonb(OLD)->>'result_id'
   OR (item->>'baseline_version')::bigint<>OLD.version OR NEW.projection_row_id<>(item->>'id')::uuid
   OR NEW.person_id<>OLD.person_id OR NEW.source_id<>OLD.source_id OR n->>cohort IS DISTINCT FROM to_jsonb(OLD)->>cohort
   OR NEW.organization_id<>OLD.organization_id THEN RAISE EXCEPTION 'baseline successor mismatch'; END IF;
 ELSIF family IN ('result','binding') THEN
  EXECUTE format('SELECT to_jsonb(i) FROM %I i WHERE id=$1 AND refresh_id=$2 AND organization_id=$3',p||'_item') INTO item USING NEW.item_id,NEW.refresh_id,NEW.organization_id;
  IF family='result' AND n->>'disposition' NOT IN ('settled','settled_noop') THEN RETURN NEW; END IF;
  -- Legacy plans remain readable; compatible workers hold them for a fresh preview.
  IF item->>'mapping_evidence_nonce' IS NULL OR NOT crm_mapping_repair_identity(p,root,item) THEN RAISE EXCEPTION 'fresh mapping preview or source identity required'; END IF;
  BEGIN permit:=current_setting('crm.mapping_repair_settlement',true)::jsonb; EXCEPTION WHEN others THEN permit:=NULL; END;
  IF item IS NULL OR root->>'state'<>'running' OR root->>'lease_token' IS DISTINCT FROM permit->>'lease'
   OR item->>'id' IS DISTINCT FROM permit->>'item' OR (root->>'lease_expires_at')::timestamptz<=clock_timestamp()
   OR item->>'settled_at' IS NOT NULL OR item->>'plan_id' IS DISTINCT FROM root->>'confirmed_refresh_plan_id'
   OR item->>'person_id' IS DISTINCT FROM n->>'person_id' OR item->>'source_id' IS DISTINCT FROM n->>'source_id'
   OR NOT EXISTS(SELECT 1 FROM organization_membership m JOIN migration_workspace w ON w.organization_id=m.organization_id WHERE m.organization_id=NEW.organization_id AND m.user_id=(root->>'initiated_by_user_id')::uuid AND m.status='active' AND m.role='admin' AND w.import_id=(root->>'parent_import_id')::uuid AND w.plan_id=(root->>'parent_plan_id')::uuid) THEN RAISE EXCEPTION 'settlement lease or identity mismatch'; END IF;
  EXECUTE format('SELECT to_jsonb(b) FROM %I b WHERE organization_id=$1 AND %I=$2 AND source_id=$3 AND person_id=$4',p||'_baseline',cohort) INTO head USING NEW.organization_id,(root->>cohort)::uuid,NEW.source_id,NEW.person_id;
  IF head IS NULL OR head->>'result_id' IS DISTINCT FROM item->>'baseline_result_id' OR head->>'version' IS DISTINCT FROM item->>'baseline_version' THEN RAISE EXCEPTION 'baseline changed'; END IF;
  EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE organization_id=$1 AND %I=$2 AND confirmed_snapshot_id IS NOT NULL AND confirmed_completed_at IS NOT NULL ORDER BY confirmed_completed_at DESC,id DESC LIMIT 1',p,cohort) INTO source USING NEW.organization_id,(root->>cohort)::uuid;
  IF source IS NOT NULL AND (source->>'confirmed_snapshot_id'<>root->>'newer_snapshot_id' OR source->>'newer_sequence'<>root->>'newer_sequence')
    AND (root->>'newer_started_at')::timestamptz<=(source->>'confirmed_completed_at')::timestamptz THEN RAISE EXCEPTION 'stale source boundary'; END IF;
  FOREACH kind IN ARRAY ARRAY['stage','assignee'] LOOP
   IF family='binding' AND kind<>n->>'kind' THEN CONTINUE; END IF;
   column_prefix:='repair_'||kind;
   IF item->>(column_prefix||'_source_hmac') IS NOT NULL THEN
    EXECUTE format('SELECT to_jsonb(h) FROM %I h WHERE organization_id=$1 AND person_id=$2 AND source_account_id=$3 AND kind=$4 AND source_key_hmac=$5',p||'_mapping_head') INTO head USING NEW.organization_id,NEW.person_id,(root->>'source_account_id')::bigint,kind,decode(substr(item->>(column_prefix||'_source_hmac'),3),'hex');
    IF head->>'binding_id' IS DISTINCT FROM item->>(column_prefix||'_head_id') OR COALESCE((head->>'version')::bigint,0)<>(item->>(column_prefix||'_head_version'))::bigint THEN RAISE EXCEPTION 'mapping head changed'; END IF;
    IF item->>(column_prefix||'_choice_refresh_id')=root->>'id' THEN
     EXECUTE format('SELECT to_jsonb(c) FROM %I c WHERE refresh_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4 AND revision<=$5 ORDER BY revision DESC LIMIT 1',p||'_repair_choice') INTO choice USING NEW.refresh_id,NEW.organization_id,kind,decode(substr(item->>(column_prefix||'_source_hmac'),3),'hex'),(root->>'repair_frozen_revision')::bigint;
     IF choice->>'id' IS DISTINCT FROM item->>(column_prefix||'_choice_id') THEN RAISE EXCEPTION 'choice not in confirmed revision'; END IF;
    END IF;
   END IF;
  END LOOP;
  IF family='result' THEN
   IF NEW.disposition='settled_noop' AND (item->>'disposition'<>'already_current' OR decode(substr(item->>'native_fingerprint',3),'hex') IS DISTINCT FROM crm_mapping_repair_native_fingerprint(NEW.organization_id,NEW.person_id)) THEN RAISE EXCEPTION 'stale no-op'; END IF;
  ELSE
   EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I z WHERE z.id=$1 AND z.refresh_id=$2 AND z.item_id=$3 AND z.organization_id=$4 AND z.disposition IN (''settled'',''settled_noop''))',p||'_result') INTO valid USING NEW.result_id,NEW.refresh_id,NEW.item_id,NEW.organization_id;
   IF NOT valid OR NEW.choice_refresh_id<>NEW.refresh_id OR NEW.plan_id<>(item->>'plan_id')::uuid OR NEW.source_account_id<>(root->>'source_account_id')::bigint THEN RAISE EXCEPTION 'binding requires owned success'; END IF;
   column_prefix:='repair_'||NEW.kind;
   IF n->>'choice_id' IS DISTINCT FROM item->>(column_prefix||'_choice_id') OR n->>'source_key_hmac' IS DISTINCT FROM item->>(column_prefix||'_source_hmac') OR n->>'previous_binding_id' IS DISTINCT FROM item->>(column_prefix||'_head_id') THEN RAISE EXCEPTION 'binding differs from preview'; END IF;
  END IF;
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION crm_mapping_repair_head_write() RETURNS trigger LANGUAGE plpgsql
SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE b JSONB; p TEXT:=TG_ARGV[0];
BEGIN
 EXECUTE format('SELECT to_jsonb(b) FROM %I b WHERE id=$1 AND organization_id=$2',p||'_mapping_binding') INTO b USING NEW.binding_id,NEW.organization_id;
 IF b IS NULL OR (TG_OP='INSERT' AND (NEW.version<>1 OR b->>'previous_binding_id' IS NOT NULL))
 OR (TG_OP='UPDATE' AND ((to_jsonb(OLD)-ARRAY['binding_id','version']) IS DISTINCT FROM (to_jsonb(NEW)-ARRAY['binding_id','version']) OR NEW.version<>OLD.version+1 OR (b->>'previous_binding_id')::uuid IS DISTINCT FROM OLD.binding_id)) THEN RAISE EXCEPTION 'mapping head changed'; END IF;
 RETURN NEW;
END $$;
DO $$ DECLARE p TEXT; f TEXT; tab TEXT; BEGIN
 FOREACH p IN ARRAY ARRAY['migration_people_refresh','migration_admitted_people_refresh'] LOOP
  FOREACH f IN ARRAY ARRAY['root','candidate','choice','plan','item','result','binding','baseline'] LOOP
   tab:=CASE f WHEN 'root' THEN p WHEN 'candidate' THEN p||'_repair_candidate' WHEN 'choice' THEN p||'_repair_choice' WHEN 'binding' THEN p||'_mapping_binding' ELSE p||'_'||f END;
   EXECUTE format('CREATE TRIGGER mapping_repair_owned BEFORE INSERT OR UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION crm_mapping_repair_owned_write(%L,%L)',tab,p,f);
  END LOOP;
  EXECUTE format('CREATE TRIGGER mapping_repair_head BEFORE INSERT OR UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION crm_mapping_repair_head_write(%L)',p||'_mapping_head',p);
 END LOOP;
END $$;

-- Original refresh had boundary uniqueness; admitted refresh deliberately
-- dropped it in 20260930000002 for exact-boundary remainders. Do not restore it.
DROP INDEX migration_people_refresh_boundary;
CREATE UNIQUE INDEX migration_people_refresh_boundary
 ON migration_people_refresh(organization_id,parent_import_id,newer_snapshot_id)
 WHERE state<>'cancelled' AND repair_source_refresh_id IS NULL;

-- Original items need exact original mapping and baseline-head evidence too.
ALTER TABLE migration_people_refresh_item
 ADD COLUMN stage_mapping_id UUID REFERENCES migration_import_mapping(id),
 ADD COLUMN assignee_mapping_id UUID REFERENCES migration_import_mapping(id),
 ADD COLUMN baseline_version BIGINT NOT NULL DEFAULT 0;
ALTER TABLE migration_people_refresh_baseline ADD COLUMN version BIGINT NOT NULL DEFAULT 1 CHECK(version>0);

CREATE FUNCTION crm_mapping_repair_capability(org UUID) RETURNS void
 LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
 IF current_user='crm_app' AND EXISTS(SELECT 1 FROM migration_mapping_repair_requirement WHERE organization_id=org)
    AND current_setting('crm.mapping_repair_reader',true) IS DISTINCT FROM 'fub-people-mapping-repair-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010R',MESSAGE='mapping_repair_capability_required';
 END IF;
END $$;
REVOKE ALL ON FUNCTION crm_mapping_repair_capability(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_mapping_repair_capability(UUID) TO crm_app;

CREATE OR REPLACE FUNCTION crm_workspace_shared(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF NOT pg_try_advisory_xact_lock_shared(hashtextextended('crm-workspace-v1:'||org::text,0)) THEN RAISE EXCEPTION USING ERRCODE='55P03',MESSAGE='workspace_busy'; END IF;
 IF current_user='crm_app' THEN
  IF EXISTS(SELECT 1 FROM migration_admitted_activity_import WHERE organization_id=org AND confirmed_plan_id IS NOT NULL) AND current_setting('crm.admitted_activity_reader',true) IS DISTINCT FROM 'fub-admitted-activity-v1' THEN RAISE EXCEPTION USING ERRCODE='P010F',MESSAGE='admitted_activity_handover_required'; END IF;
  IF EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=org) AND current_setting('crm.history_reader',true) IS DISTINCT FROM 'fub-history-timeline-v1' THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_reader_required'; END IF;
  IF EXISTS(SELECT 1 FROM migration_admitted_history_root WHERE organization_id=org AND confirmed_plan_id IS NOT NULL) AND current_setting('crm.admitted_history_reader',true) IS DISTINCT FROM 'fub-admitted-history-v1' THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_reader_required'; END IF;
  PERFORM crm_mapping_repair_capability(org);
 END IF;
END $$;

-- A legacy worker must not mutate a root, settle a no-op, or move a baseline
-- merely because its unit never calls a business-table permit.
CREATE FUNCTION crm_mapping_repair_write_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v JSONB;
BEGIN
 v:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 PERFORM crm_mapping_repair_capability((v->>'organization_id')::uuid);
 IF current_user='crm_app' AND v->>'repair_source_refresh_id' IS NOT NULL
    AND current_setting('crm.mapping_repair_reader',true) IS DISTINCT FROM 'fub-people-mapping-repair-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010R',MESSAGE='mapping_repair_capability_required';
 END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
DO $$ DECLARE p TEXT; suffix TEXT; BEGIN
 FOREACH p IN ARRAY ARRAY['migration_people_refresh','migration_admitted_people_refresh'] LOOP
  FOREACH suffix IN ARRAY ARRAY['','_plan','_item','_result','_baseline','_repair_key','_repair_choice','_repair_candidate','_mapping_binding','_mapping_head'] LOOP
   EXECUTE format('CREATE TRIGGER mapping_repair_fence BEFORE INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_mapping_repair_write_fence()',p||suffix);
  END LOOP;
 END LOOP;
END $$;

-- Preserve the existing business-column/identity restrictions and add evidence fencing.
CREATE OR REPLACE FUNCTION crm_people_refresh_mutation_allowed(org UUID, permit TEXT, table_name TEXT, operation TEXT, row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; person UUID;
BEGIN
 IF NOT crm_mapping_repair_mutation_evidence(org,'migration_people_refresh',permit,table_name) THEN RETURN false; END IF;
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.person_id INTO person FROM migration_people_refresh_item i JOIN migration_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id JOIN migration_workspace w ON w.organization_id=r.organization_id AND w.import_id=r.parent_import_id AND w.plan_id=r.parent_plan_id JOIN organization_membership m ON m.organization_id=r.organization_id AND m.user_id=r.initiated_by_user_id WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND r.state='running' AND r.lease_token=lease AND r.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF person IS NULL THEN RETURN false; END IF;
 IF table_name='person' THEN RETURN operation='UPDATE' AND (row_value->>'id')::uuid=person; END IF;
 IF table_name='contact_method' THEN RETURN (row_value->>'person_id')::uuid=person AND EXISTS(SELECT 1 FROM migration_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.contact_id=(row_value->>'id')::uuid AND ((operation='INSERT' AND c.side='proposed') OR (operation='DELETE' AND c.side IN ('baseline','current')) OR (operation='UPDATE' AND c.side IN ('baseline','current','proposed')))); END IF;
 IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=person AND row_value->>'origin'='migration' AND row_value->>'reason'='migration_refresh' AND (row_value->>'on_behalf_of_user_id')::uuid=(SELECT initiated_by_user_id FROM migration_people_refresh_item i JOIN migration_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id WHERE i.id=unit AND i.organization_id=org); END IF;
 RETURN false;
END $$;

-- Preserve the existing business-column/identity restrictions and add evidence fencing.
CREATE OR REPLACE FUNCTION crm_admitted_people_refresh_mutation_allowed(org UUID, permit TEXT, table_name TEXT, operation TEXT, old_row JSONB, new_row JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; target UUID; actor UUID;
BEGIN
 IF NOT crm_mapping_repair_mutation_evidence(org,'migration_admitted_people_refresh',permit,table_name) THEN RETURN false; END IF;
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.person_id,r.initiated_by_user_id INTO target,actor FROM migration_admitted_people_refresh_item i JOIN migration_admitted_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id AND r.confirmed_refresh_plan_id=i.plan_id AND i.source_account_id=r.source_account_id JOIN migration_people_admission_result ar ON ar.id=i.admission_result_id AND ar.admission_id=r.admission_id AND ar.organization_id=i.organization_id AND ar.person_id=i.person_id AND ar.disposition='settled' JOIN migration_import_identity mi ON mi.organization_id=i.organization_id AND mi.source_account_id=i.source_account_id AND mi.family='people' AND mi.source_id=i.source_id AND mi.target_id=i.person_id AND mi.admission_id=r.admission_id AND mi.admission_item_id=i.admission_item_id AND mi.admission_result_id=ar.id JOIN migration_admitted_people_refresh_baseline b ON b.organization_id=i.organization_id AND b.admission_id=r.admission_id AND b.source_id=i.source_id AND b.person_id=i.person_id AND b.admission_result_id=ar.id AND b.version=i.baseline_version AND b.result_id IS NOT DISTINCT FROM i.baseline_result_id JOIN migration_workspace w ON w.organization_id=r.organization_id AND w.import_id=r.parent_import_id AND w.plan_id=r.parent_plan_id JOIN organization o ON o.id=r.organization_id AND o.workspace_revision=r.workspace_revision JOIN organization_membership m ON m.organization_id=r.organization_id AND m.user_id=r.initiated_by_user_id WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND r.state='running' AND r.lease_token=lease AND r.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF target IS NULL THEN RETURN false; END IF;
 IF table_name='person' THEN RETURN operation='UPDATE' AND (old_row->>'id')::uuid=target AND (new_row->>'id')::uuid=target AND (old_row->>'organization_id')::uuid=org AND (new_row->>'organization_id')::uuid=org AND (old_row - ARRAY['first_name','last_name','stage_id','assigned_user_id','updated_at','stage_revision','mobile_revision','details_revision'])=(new_row - ARRAY['first_name','last_name','stage_id','assigned_user_id','updated_at','stage_revision','mobile_revision','details_revision']); END IF;
 IF table_name='contact_method' THEN RETURN CASE operation WHEN 'INSERT' THEN (new_row->>'person_id')::uuid=target AND (new_row->>'organization_id')::uuid=org AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side='proposed' AND c.contact_id=(new_row->>'id')::uuid AND c.kind=new_row->>'kind') WHEN 'UPDATE' THEN (old_row->>'id')=(new_row->>'id') AND (old_row->>'person_id')::uuid=target AND (new_row->>'person_id')::uuid=target AND (old_row->>'organization_id')=(new_row->>'organization_id') AND (old_row->>'kind')=(new_row->>'kind') AND (old_row - ARRAY['value','normalized_value','import_order'])=(new_row - ARRAY['value','normalized_value','import_order']) AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side IN ('baseline','current') AND c.contact_id=(old_row->>'id')::uuid AND c.kind=old_row->>'kind') WHEN 'DELETE' THEN (old_row->>'person_id')::uuid=target AND (old_row->>'organization_id')::uuid=org AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side IN ('baseline','current') AND c.contact_id=(old_row->>'id')::uuid AND c.kind=old_row->>'kind') ELSE false END; END IF;
 IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (new_row->>'person_id')::uuid=target AND new_row->>'actor_kind'='system' AND new_row->>'origin'='migration' AND new_row->>'reason'='migration_refresh' AND (new_row->>'on_behalf_of_user_id')::uuid=actor; END IF;
 RETURN false;
END $$;
