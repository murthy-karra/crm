-- D-090: positive mapping-hold recovery is an explicit admission mode.
-- Existing original import and ordinary admission evidence is never rewritten.
ALTER TABLE migration_people_admission
 ADD COLUMN mode TEXT NOT NULL DEFAULT 'ordinary',
 ADD COLUMN recovery_original_plan_id UUID,
 ADD COLUMN recovery_admission_id UUID,
 ADD COLUMN recovery_admission_plan_id UUID,
 ADD COLUMN recovery_remainder BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN recovery_checkpoint_id UUID,
 ADD COLUMN recovery_catalog_complete BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN recovery_candidates_complete BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN recovery_candidate_count BIGINT NOT NULL DEFAULT 0 CHECK(recovery_candidate_count>=0),
 ADD COLUMN recovery_draft_revision BIGINT NOT NULL DEFAULT 0 CHECK(recovery_draft_revision>=0),
 ADD COLUMN recovery_frozen_revision BIGINT,
 ADD COLUMN recovery_draft_digest BYTEA CHECK(recovery_draft_digest IS NULL OR octet_length(recovery_draft_digest)=32),
 ADD COLUMN recovery_choices_digest BYTEA CHECK(recovery_choices_digest IS NULL OR octet_length(recovery_choices_digest)=32),
 DROP CONSTRAINT migration_people_admission_engine_version_check,
 ADD CONSTRAINT people_recovery_mode CHECK(
  (mode='ordinary' AND engine_version='fub-people-admission-v1'
   AND recovery_original_plan_id IS NULL AND recovery_admission_id IS NULL
   AND recovery_admission_plan_id IS NULL AND NOT recovery_remainder)
  OR (mode='mapping_recovery' AND engine_version='fub-people-recovery-v1'
   AND ((recovery_original_plan_id IS NOT NULL AND recovery_original_plan_id=parent_plan_id AND recovery_admission_id IS NULL
         AND recovery_admission_plan_id IS NULL AND NOT recovery_remainder)
     OR (recovery_original_plan_id IS NULL AND recovery_admission_id IS NOT NULL
         AND recovery_admission_plan_id IS NOT NULL)))),
 ADD FOREIGN KEY(recovery_admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 ADD FOREIGN KEY(recovery_admission_plan_id,recovery_admission_id,organization_id)
  REFERENCES migration_people_admission_plan(id,admission_id,organization_id);

CREATE TABLE migration_people_recovery_requirement (
 organization_id UUID PRIMARY KEY REFERENCES organization(id),
 capability TEXT NOT NULL CHECK(capability='fub-people-recovery-v1'),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
GRANT SELECT,INSERT ON migration_people_recovery_requirement TO crm_app;
CREATE TRIGGER people_recovery_requirement_immutable BEFORE UPDATE OR DELETE
 ON migration_people_recovery_requirement FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TABLE migration_people_recovery_candidate (
 id UUID NOT NULL UNIQUE,
 admission_id UUID NOT NULL, organization_id UUID NOT NULL,
 source_id TEXT NOT NULL CHECK(source_id ~ '^[1-9][0-9]{0,127}$'),
 parent_import_id UUID NOT NULL, parent_plan_id UUID NOT NULL,
 original_manifest_id UUID,
 anchor_admission_id UUID, anchor_plan_id UUID, anchor_item_id UUID,
 stage_source_hmac BYTEA CHECK(stage_source_hmac IS NULL OR octet_length(stage_source_hmac)=32),
 assignee_source_hmac BYTEA CHECK(assignee_source_hmac IS NULL OR octet_length(assignee_source_hmac)=32),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),
 ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65552),
 PRIMARY KEY(admission_id,organization_id,source_id),
 CHECK((original_manifest_id IS NOT NULL AND anchor_admission_id IS NULL AND anchor_plan_id IS NULL AND anchor_item_id IS NULL)
    OR (original_manifest_id IS NULL AND anchor_admission_id IS NOT NULL AND anchor_plan_id IS NOT NULL AND anchor_item_id IS NOT NULL)),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(original_manifest_id,parent_plan_id,parent_import_id,organization_id)
  REFERENCES migration_import_manifest(id,plan_id,import_id,organization_id),
 FOREIGN KEY(anchor_item_id,anchor_admission_id,organization_id)
  REFERENCES migration_people_admission_item(id,admission_id,organization_id),
 FOREIGN KEY(anchor_plan_id,anchor_admission_id,organization_id)
  REFERENCES migration_people_admission_plan(id,admission_id,organization_id)
);
CREATE INDEX people_recovery_candidate_stage ON migration_people_recovery_candidate(admission_id,organization_id,stage_source_hmac);
CREATE INDEX people_recovery_candidate_assignee ON migration_people_recovery_candidate(admission_id,organization_id,assignee_source_hmac);

CREATE TABLE migration_people_recovery_key (
 id UUID PRIMARY KEY, admission_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('stage','assignee')),
 source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),
 ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65552),
 UNIQUE(id,admission_id,organization_id),
 UNIQUE(admission_id,organization_id,kind,source_key_hmac),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id)
);
CREATE INDEX people_recovery_key_page ON migration_people_recovery_key(admission_id,organization_id,id);
CREATE TABLE migration_people_recovery_choice (
 id UUID PRIMARY KEY, admission_id UUID NOT NULL, organization_id UUID NOT NULL,
 key_id UUID NOT NULL, revision BIGINT NOT NULL CHECK(revision>0),
 kind TEXT NOT NULL CHECK(kind IN ('stage','assignee')),
 source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32),
 disposition TEXT NOT NULL CHECK(disposition IN ('existing','member','unassigned','hold')),
 target_id UUID,
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),
 ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65552),
 predecessor_choice_id UUID,
 CHECK((kind='stage' AND disposition='existing' AND target_id IS NOT NULL)
    OR (kind='assignee' AND disposition='member' AND target_id IS NOT NULL)
    OR (kind='assignee' AND disposition='unassigned' AND target_id IS NULL)
    OR (disposition='hold' AND target_id IS NULL)),
 UNIQUE(id,admission_id,organization_id),
 UNIQUE(admission_id,organization_id,kind,source_key_hmac,revision),
 FOREIGN KEY(key_id,admission_id,organization_id) REFERENCES migration_people_recovery_key(id,admission_id,organization_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(predecessor_choice_id) REFERENCES migration_people_recovery_choice(id)
);
CREATE INDEX people_recovery_choice_lookup ON migration_people_recovery_choice(admission_id,organization_id,kind,source_key_hmac,revision DESC);
ALTER TABLE migration_people_admission_item
 ADD COLUMN recovery_stage_choice_id UUID,
 ADD COLUMN recovery_assignee_choice_id UUID,
 ADD CONSTRAINT people_recovery_stage_owner CHECK(stage_mapping_id IS NULL OR recovery_stage_choice_id IS NULL),
 ADD CONSTRAINT people_recovery_assignee_owner CHECK(assignee_mapping_id IS NULL OR recovery_assignee_choice_id IS NULL),
 ADD FOREIGN KEY(recovery_stage_choice_id,admission_id,organization_id) REFERENCES migration_people_recovery_choice(id,admission_id,organization_id),
 ADD FOREIGN KEY(recovery_assignee_choice_id,admission_id,organization_id) REFERENCES migration_people_recovery_choice(id,admission_id,organization_id);
ALTER TABLE migration_people_admission_plan
 ADD COLUMN recovery_candidate_count BIGINT NOT NULL DEFAULT 0,
 ADD COLUMN recovery_unassigned_count BIGINT NOT NULL DEFAULT 0;

CREATE FUNCTION crm_people_recovery_capability(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM migration_people_recovery_requirement WHERE organization_id=org)
    AND current_setting('crm.people_recovery_reader',true) IS DISTINCT FROM 'fub-people-recovery-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010R',MESSAGE='people_recovery_capability_required';
 END IF;
END $$;
REVOKE ALL ON FUNCTION crm_people_recovery_capability(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_people_recovery_capability(UUID) TO crm_app;

-- Every already-running path which takes the common workspace lock is fenced.
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_workspace_shared(uuid)'::regprocedure) INTO definition;
 IF position('PERFORM crm_mapping_repair_capability(org);' IN definition)=0 THEN
  RAISE EXCEPTION 'unexpected shared workspace definition';
 END IF;
 EXECUTE replace(definition,'PERFORM crm_mapping_repair_capability(org);',
  'PERFORM crm_mapping_repair_capability(org); PERFORM crm_people_recovery_capability(org);');
END $$;

CREATE FUNCTION crm_people_recovery_owned_write() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE a migration_people_admission; k migration_people_recovery_key; c migration_people_recovery_choice;
BEGIN
 SELECT * INTO a FROM migration_people_admission WHERE id=NEW.admission_id AND organization_id=NEW.organization_id FOR SHARE;
 PERFORM crm_people_recovery_capability(NEW.organization_id);
 IF a.id IS NULL OR a.mode<>'mapping_recovery' OR a.confirmed_admission_plan_id IS NOT NULL THEN
  RAISE EXCEPTION 'recovery owner is not editable' USING ERRCODE='P0001';
 END IF;
 IF TG_TABLE_NAME='migration_people_recovery_candidate' THEN
  IF a.state<>'preparing' OR a.recovery_candidates_complete
     OR NEW.parent_import_id<>a.parent_import_id OR NEW.parent_plan_id<>a.parent_plan_id THEN
   RAISE EXCEPTION 'recovery candidate owner mismatch' USING ERRCODE='P0001';
  END IF;
  IF NEW.original_manifest_id IS NOT NULL THEN
   IF a.recovery_original_plan_id IS NULL OR NOT EXISTS(
    SELECT 1 FROM migration_import_manifest m WHERE m.id=NEW.original_manifest_id
     AND m.organization_id=a.organization_id AND m.plan_id=a.recovery_original_plan_id
     AND m.import_id=a.parent_import_id AND m.source_id=NEW.source_id
     AND m.disposition='held' AND EXISTS(SELECT 1 FROM migration_import_mapping z
      WHERE z.id IN(m.stage_mapping_id,m.assignee_mapping_id) AND z.organization_id=m.organization_id
       AND z.plan_id=m.plan_id AND z.disposition='hold')) THEN
    RAISE EXCEPTION 'unqualified original recovery anchor' USING ERRCODE='P0001';
   END IF;
  ELSE
   IF NEW.anchor_admission_id IS DISTINCT FROM a.recovery_admission_id
     OR NEW.anchor_plan_id IS DISTINCT FROM a.recovery_admission_plan_id
     OR NOT EXISTS(SELECT 1 FROM migration_people_admission_item i
      JOIN migration_people_admission prior ON prior.id=i.admission_id AND prior.organization_id=i.organization_id
      WHERE i.id=NEW.anchor_item_id AND i.admission_id=a.recovery_admission_id
       AND i.plan_id=a.recovery_admission_plan_id AND i.organization_id=a.organization_id
       AND prior.parent_import_id=a.parent_import_id AND prior.source_account_id=a.source_account_id
       AND i.source_id=NEW.source_id
       AND ((NOT a.recovery_remainder AND i.disposition='held_mapping_gap') OR
        (a.recovery_remainder AND prior.mode='mapping_recovery' AND prior.state='cancelled'
         AND prior.confirmed_admission_plan_id=i.plan_id AND prior.report_id=a.report_id
         AND i.disposition='eligible' AND i.settled_at IS NULL))) THEN
    RAISE EXCEPTION 'unqualified admission recovery anchor' USING ERRCODE='P0001';
   END IF;
  END IF;
 ELSIF TG_TABLE_NAME='migration_people_recovery_key' THEN
  IF a.state<>'preparing' OR a.recovery_candidates_complete THEN
   RAISE EXCEPTION 'recovery key catalog sealed' USING ERRCODE='P0001';
  END IF;
 ELSE
  SELECT * INTO k FROM migration_people_recovery_key WHERE id=NEW.key_id AND admission_id=a.id AND organization_id=a.organization_id;
  IF k.id IS NULL OR k.kind<>NEW.kind OR k.source_key_hmac<>NEW.source_key_hmac
     OR a.state NOT IN ('preparing','paused','ready') OR NEW.revision<>a.recovery_draft_revision+1 THEN
   RAISE EXCEPTION 'recovery choice revision mismatch' USING ERRCODE='P0001';
  END IF;
  IF NEW.predecessor_choice_id IS NOT NULL THEN
   SELECT * INTO c FROM migration_people_recovery_choice WHERE id=NEW.predecessor_choice_id;
   IF c.id IS NULL OR NOT a.recovery_remainder OR c.admission_id<>a.recovery_admission_id OR c.organization_id<>a.organization_id
      OR c.kind<>NEW.kind OR c.source_key_hmac<>NEW.source_key_hmac
      OR c.disposition<>NEW.disposition OR c.target_id IS DISTINCT FROM NEW.target_id THEN
    RAISE EXCEPTION 'recovery predecessor mismatch' USING ERRCODE='P0001';
   END IF;
  END IF;
 END IF;
 RETURN NEW;
END $$;

DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['migration_people_recovery_candidate','migration_people_recovery_key','migration_people_recovery_choice'] LOOP
  EXECUTE format('GRANT SELECT,INSERT ON %I TO crm_app',name);
  EXECUTE format('CREATE TRIGGER people_recovery_owned BEFORE INSERT ON %I FOR EACH ROW EXECUTE FUNCTION crm_people_recovery_owned_write()',name);
  EXECUTE format('CREATE TRIGGER people_recovery_immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_mutation()',name);
 END LOOP;
END $$;

CREATE FUNCTION crm_people_recovery_write_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v JSONB; a migration_people_admission;
BEGIN
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD));
 PERFORM crm_people_recovery_capability((v->>'organization_id')::uuid);
 IF TG_TABLE_NAME='migration_people_admission' THEN
  IF v->>'mode'='mapping_recovery' AND current_setting('crm.people_recovery_reader',true) IS DISTINCT FROM 'fub-people-recovery-v1' THEN
   RAISE EXCEPTION USING ERRCODE='P010R',MESSAGE='people_recovery_capability_required';
  END IF;
  IF TG_OP='UPDATE' AND OLD.mode IS DISTINCT FROM NEW.mode THEN
   RAISE EXCEPTION 'admission mode immutable' USING ERRCODE='P0001';
  END IF;
  IF TG_OP='UPDATE' AND OLD.mode='mapping_recovery' THEN
   IF (to_jsonb(OLD)-ARRAY['state','pause_reason','lifecycle_revision','lease_token','lease_epoch','lease_expires_at',
      'preparation_phase','preparation_checkpoint_key','recovery_checkpoint_id','recovery_candidates_complete',
      'recovery_catalog_complete','recovery_candidate_count','recovery_draft_revision','recovery_draft_digest','recovery_frozen_revision','recovery_choices_digest',
      'confirmed_admission_plan_id','confirmed_boundary','confirmed_snapshot_id','confirmed_completed_at',
      'retained_bytes','reserved_bytes','settled_items','updated_at','completed_at','cancelled_at'])
    IS DISTINCT FROM (to_jsonb(NEW)-ARRAY['state','pause_reason','lifecycle_revision','lease_token','lease_epoch','lease_expires_at',
      'preparation_phase','preparation_checkpoint_key','recovery_checkpoint_id','recovery_candidates_complete',
      'recovery_catalog_complete','recovery_candidate_count','recovery_draft_revision','recovery_draft_digest','recovery_frozen_revision','recovery_choices_digest',
      'confirmed_admission_plan_id','confirmed_boundary','confirmed_snapshot_id','confirmed_completed_at',
      'retained_bytes','reserved_bytes','settled_items','updated_at','completed_at','cancelled_at']) THEN
    RAISE EXCEPTION 'recovery owner immutable' USING ERRCODE='P0001';
   END IF;
   IF OLD.confirmed_admission_plan_id IS NOT NULL AND
      (OLD.recovery_frozen_revision IS DISTINCT FROM NEW.recovery_frozen_revision
       OR OLD.recovery_choices_digest IS DISTINCT FROM NEW.recovery_choices_digest
       OR OLD.confirmed_admission_plan_id IS DISTINCT FROM NEW.confirmed_admission_plan_id) THEN
    RAISE EXCEPTION 'confirmed recovery is immutable' USING ERRCODE='P0001';
   END IF;
  END IF;
 END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['migration_people_admission','migration_people_admission_plan','migration_people_admission_item',
  'migration_people_admission_result','migration_people_admission_contact','migration_people_admission_receipt'] LOOP
  EXECUTE format('CREATE TRIGGER people_recovery_write_fence BEFORE INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_people_recovery_write_fence()',name);
 END LOOP;
END $$;

-- The original held-descriptor lookup is selective even on a 25k book.
CREATE INDEX people_recovery_original_hold ON migration_import_manifest(import_id,organization_id,plan_id,id) WHERE disposition='held';
CREATE INDEX people_recovery_admission_hold ON migration_people_admission_item(admission_id,organization_id,plan_id,id) WHERE disposition='held_mapping_gap';
CREATE INDEX people_recovery_original_success ON migration_import_result(organization_id,source_id,import_id) WHERE disposition IN ('imported','already_imported');
CREATE INDEX people_recovery_admission_success ON migration_people_admission_result(organization_id,source_id,admission_id) WHERE disposition='settled';

ALTER TABLE migration_people_admission_receipt DROP CONSTRAINT migration_people_admission_receipt_action_check,
 ADD CHECK(action IN ('prepare','repreview','confirm','retry','cancel','prepare_recovery','recovery_mapping','seal_recovery'));

-- Never infer permission to recreate a source from a missing identity row.
CREATE FUNCTION crm_people_recovery_previously_materialized(org UUID, account BIGINT, source TEXT)
RETURNS BOOLEAN LANGUAGE sql STABLE SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
 SELECT EXISTS(SELECT 1 FROM migration_import_identity WHERE organization_id=org
   AND source_account_id=account AND family='people' AND source_id=source)
 OR EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import a
   ON a.id=r.import_id AND a.organization_id=r.organization_id
   WHERE r.organization_id=org AND a.source_account_id=account AND r.source_id=source
    AND r.disposition IN ('imported','already_imported'))
 OR EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_people_admission a
   ON a.id=r.admission_id AND a.organization_id=r.organization_id
   WHERE r.organization_id=org AND a.source_account_id=account AND r.source_id=source AND r.disposition='settled')
$$;
REVOKE ALL ON FUNCTION crm_people_recovery_previously_materialized(UUID,BIGINT,TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_people_recovery_previously_materialized(UUID,BIGINT,TEXT) TO crm_app;

CREATE TABLE person_recovered (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id),
 actor_kind TEXT NOT NULL CHECK(actor_kind='system'), actor_user_id UUID CHECK(actor_user_id IS NULL),
 on_behalf_of_user_id UUID NOT NULL REFERENCES app_user(id), origin TEXT NOT NULL CHECK(origin='migration'),
 occurred_at TIMESTAMPTZ NOT NULL, recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 correlation_id UUID NOT NULL, person_id UUID NOT NULL, admission_id UUID NOT NULL,
 plan_id UUID NOT NULL, item_id UUID NOT NULL, result_id UUID NOT NULL, candidate_id UUID NOT NULL,
 UNIQUE(organization_id,admission_id,person_id),
 FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id),
 FOREIGN KEY(plan_id,admission_id,organization_id) REFERENCES migration_people_admission_plan(id,admission_id,organization_id),
 FOREIGN KEY(item_id,admission_id,organization_id) REFERENCES migration_people_admission_item(id,admission_id,organization_id),
 FOREIGN KEY(result_id,admission_id,item_id,organization_id,person_id) REFERENCES migration_people_admission_result(id,admission_id,item_id,organization_id,person_id),
 FOREIGN KEY(candidate_id) REFERENCES migration_people_recovery_candidate(id)
);
CREATE INDEX person_recovered_history ON person_recovered(organization_id,person_id,occurred_at,id);
GRANT SELECT,INSERT ON person_recovered TO crm_app;
CREATE TRIGGER person_recovered_append_only BEFORE UPDATE OR DELETE ON person_recovered FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER person_recovered_no_truncate BEFORE TRUNCATE ON person_recovered FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER workspace_mutation_guard BEFORE INSERT OR UPDATE OR DELETE ON person_recovered FOR EACH ROW EXECUTE FUNCTION crm_workspace_mutation_guard();
CREATE FUNCTION crm_person_recovered_prepare_history() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 UPDATE migration_history_review_state SET counts=counts||jsonb_build_object('person_recovered',0)
 WHERE organization_id=NEW.organization_id AND person_id=NEW.person_id AND NOT counts ? 'person_recovered';
 RETURN NEW;
END $$;
CREATE TRIGGER person_recovered_prepare_history BEFORE INSERT ON person_recovered FOR EACH ROW EXECUTE FUNCTION crm_person_recovered_prepare_history();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON person_recovered FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();

CREATE FUNCTION crm_people_recovery_item_proof(org UUID, unit UUID) RETURNS BOOLEAN
LANGUAGE sql STABLE SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
 SELECT EXISTS(SELECT 1 FROM migration_people_admission_item i
 JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=i.organization_id
 JOIN migration_people_recovery_candidate c ON c.admission_id=a.id AND c.organization_id=a.organization_id AND c.source_id=i.source_id
 JOIN migration_people_recovery_choice stage_choice ON stage_choice.id=i.recovery_stage_choice_id
  AND stage_choice.admission_id=a.id AND stage_choice.organization_id=a.organization_id
 WHERE i.id=unit AND i.organization_id=org AND a.mode='mapping_recovery' AND a.engine_version='fub-people-recovery-v1'
  AND a.recovery_candidates_complete AND a.recovery_frozen_revision IS NOT NULL AND a.recovery_choices_digest IS NOT NULL
  AND i.stage_mapping_id IS NULL AND i.assignee_mapping_id IS NULL
  AND stage_choice.kind='stage' AND stage_choice.source_key_hmac=c.stage_source_hmac
  AND stage_choice.disposition='existing' AND stage_choice.revision<=a.recovery_frozen_revision
  AND NOT EXISTS(SELECT 1 FROM migration_people_recovery_choice newer WHERE newer.admission_id=a.id AND newer.organization_id=org
   AND newer.kind='stage' AND newer.source_key_hmac=c.stage_source_hmac AND newer.revision>stage_choice.revision AND newer.revision<=a.recovery_frozen_revision)
  AND EXISTS(SELECT 1 FROM stage t WHERE t.id=stage_choice.target_id AND t.organization_id=org)
  AND ((c.assignee_source_hmac IS NULL AND i.recovery_assignee_choice_id IS NULL)
   OR EXISTS(SELECT 1 FROM migration_people_recovery_choice assignment_choice WHERE assignment_choice.id=i.recovery_assignee_choice_id
    AND assignment_choice.admission_id=a.id AND assignment_choice.organization_id=org AND assignment_choice.kind='assignee'
    AND assignment_choice.source_key_hmac=c.assignee_source_hmac AND assignment_choice.revision<=a.recovery_frozen_revision
    AND NOT EXISTS(SELECT 1 FROM migration_people_recovery_choice newer WHERE newer.admission_id=a.id AND newer.organization_id=org
     AND newer.kind='assignee' AND newer.source_key_hmac=c.assignee_source_hmac AND newer.revision>assignment_choice.revision AND newer.revision<=a.recovery_frozen_revision)
    AND ((assignment_choice.disposition='unassigned' AND assignment_choice.target_id IS NULL)
     OR (assignment_choice.disposition='member' AND EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=org AND m.user_id=assignment_choice.target_id AND m.status='active')))))
 )
$$;
REVOKE ALL ON FUNCTION crm_people_recovery_item_proof(UUID,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_people_recovery_item_proof(UUID,UUID) TO crm_app;

CREATE OR REPLACE FUNCTION crm_people_admission_mutation_allowed(org UUID, permit TEXT, table_name TEXT, operation TEXT, row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; target UUID; root migration_people_admission; item migration_people_admission_item;
 expected_stage UUID; expected_assignee UUID;
BEGIN
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.* INTO item FROM migration_people_admission_item i JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=i.organization_id JOIN migration_workspace w ON w.organization_id=a.organization_id AND w.import_id=a.parent_import_id AND w.plan_id=a.parent_plan_id JOIN organization_membership m ON m.organization_id=a.organization_id AND m.user_id=a.initiated_by_user_id WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND a.state='running' AND a.confirmed_admission_plan_id=i.plan_id AND EXISTS(SELECT 1 FROM organization o WHERE o.id=a.organization_id AND o.workspace_revision=a.workspace_revision) AND a.lease_token=lease AND a.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF item.id IS NULL THEN RETURN false; END IF;
 target:=item.prospective_person_id;
 SELECT * INTO root FROM migration_people_admission WHERE id=item.admission_id AND organization_id=org;
 PERFORM crm_people_recovery_capability(org);
 IF root.mode='mapping_recovery' THEN
  IF current_setting('crm.people_recovery_reader',true) IS DISTINCT FROM 'fub-people-recovery-v1'
    OR NOT crm_people_recovery_item_proof(org,unit) THEN RETURN false; END IF;
  SELECT target_id INTO expected_stage FROM migration_people_recovery_choice WHERE id=item.recovery_stage_choice_id AND admission_id=root.id AND organization_id=org;
  SELECT target_id INTO expected_assignee FROM migration_people_recovery_choice WHERE id=item.recovery_assignee_choice_id AND admission_id=root.id AND organization_id=org;
  IF table_name='person' THEN
   RETURN operation='INSERT' AND (row_value->>'id')::uuid=target AND (row_value->>'stage_id')::uuid=expected_stage
    AND (row_value->>'assigned_user_id')::uuid IS NOT DISTINCT FROM expected_assignee
    AND NOT crm_people_recovery_previously_materialized(org,root.source_account_id,item.source_id);
  END IF;
  IF table_name IN ('assignment_changed','stage_changed') THEN
   RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=target AND row_value->>'origin'='migration'
    AND row_value->>'reason'='migration_recovery';
  END IF;
  IF table_name='person_recovered' THEN
   RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=target AND (row_value->>'item_id')::uuid=unit
    AND (row_value->>'admission_id')::uuid=root.id AND (row_value->>'plan_id')::uuid=item.plan_id
    AND EXISTS(SELECT 1 FROM migration_people_recovery_candidate c WHERE c.id=(row_value->>'candidate_id')::uuid
     AND c.admission_id=root.id AND c.organization_id=org AND c.source_id=item.source_id);
  END IF;
  IF table_name='person_admitted' THEN RETURN false; END IF;
 ELSE
  IF table_name='person' THEN RETURN operation='INSERT' AND (row_value->>'id')::uuid=target; END IF;
  IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=target AND row_value->>'origin'='migration' AND row_value->>'reason'='migration_admission'; END IF;
  IF table_name='person_admitted' THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=target AND (row_value->>'item_id')::uuid=unit; END IF;
 END IF;
 IF table_name='contact_method' THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=target AND EXISTS(SELECT 1 FROM migration_people_admission_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.id=(row_value->>'id')::uuid AND c.kind=row_value->>'kind'); END IF;
 RETURN false;
END $$;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_workspace_mutation_guard()'::regprocedure) INTO definition;
 IF position('''person_admitted''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected workspace guard'; END IF;
 EXECUTE replace(definition,'''person_admitted''','''person_admitted'',''person_recovered''');
END $$;

CREATE FUNCTION crm_people_recovery_item_owned() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE a migration_people_admission;
BEGIN
 SELECT * INTO a FROM migration_people_admission WHERE id=NEW.admission_id AND organization_id=NEW.organization_id;
 IF a.mode='ordinary' THEN
  IF NEW.recovery_stage_choice_id IS NOT NULL OR NEW.recovery_assignee_choice_id IS NOT NULL THEN
   RAISE EXCEPTION 'ordinary item has recovery approval' USING ERRCODE='P0001';
  END IF;
 ELSE
  IF NEW.stage_mapping_id IS NOT NULL OR NEW.assignee_mapping_id IS NOT NULL
    OR NOT EXISTS(SELECT 1 FROM migration_people_recovery_candidate c WHERE c.admission_id=a.id AND c.organization_id=a.organization_id AND c.source_id=NEW.source_id)
    OR (NEW.disposition='eligible' AND NOT crm_people_recovery_item_proof(NEW.organization_id,NEW.id)) THEN
   RAISE EXCEPTION 'recovery item approval mismatch' USING ERRCODE='P0001';
  END IF;
 END IF;
 RETURN NEW;
END $$;
-- AFTER INSERT lets the proof query see the just-created immutable descriptor.
CREATE TRIGGER people_recovery_item_owned AFTER INSERT ON migration_people_admission_item FOR EACH ROW EXECUTE FUNCTION crm_people_recovery_item_owned();

CREATE FUNCTION crm_people_recovery_plan_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.state<>'building' AND (OLD.recovery_candidate_count<>NEW.recovery_candidate_count OR OLD.recovery_unassigned_count<>NEW.recovery_unassigned_count) THEN
  RAISE EXCEPTION 'recovery preview counts immutable' USING ERRCODE='P0001';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER people_recovery_plan_immutable BEFORE UPDATE ON migration_people_admission_plan FOR EACH ROW EXECUTE FUNCTION crm_people_recovery_plan_immutable();

ALTER TABLE migration_admitted_people_refresh_item
 ADD COLUMN recovery_stage_choice_id UUID,
 ADD COLUMN recovery_assignee_choice_id UUID,
 ADD FOREIGN KEY(recovery_stage_choice_id,admission_id,organization_id) REFERENCES migration_people_recovery_choice(id,admission_id,organization_id),
 ADD FOREIGN KEY(recovery_assignee_choice_id,admission_id,organization_id) REFERENCES migration_people_recovery_choice(id,admission_id,organization_id),
 ADD CHECK(recovery_stage_choice_id IS NULL OR (stage_mapping_id IS NULL AND repair_stage_choice_id IS NULL)),
 ADD CHECK(recovery_assignee_choice_id IS NULL OR (assignee_mapping_id IS NULL AND repair_assignee_choice_id IS NULL));
CREATE FUNCTION crm_people_recovery_refresh_approval() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE approval_kind TEXT; v JSONB:=to_jsonb(NEW); chosen UUID;
BEGIN
 PERFORM crm_people_recovery_capability(NEW.organization_id);
 FOREACH approval_kind IN ARRAY ARRAY['stage','assignee'] LOOP
  chosen:=(v->>('recovery_'||approval_kind||'_choice_id'))::uuid;
  IF chosen IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_people_admission_item i
   JOIN migration_people_admission_result z ON z.item_id=i.id AND z.admission_id=i.admission_id AND z.organization_id=i.organization_id
   JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=i.organization_id
   JOIN migration_people_recovery_choice c ON c.id=chosen AND c.admission_id=a.id AND c.organization_id=a.organization_id
   JOIN migration_import_identity mi ON mi.admission_result_id=z.id AND mi.admission_id=a.id AND mi.admission_item_id=i.id
    AND mi.organization_id=a.organization_id AND mi.family='people' AND mi.source_id=z.source_id AND mi.target_id=z.person_id
   WHERE i.id=NEW.admission_item_id AND i.admission_id=NEW.admission_id AND i.organization_id=NEW.organization_id
    AND z.id=NEW.admission_result_id AND z.person_id=NEW.person_id AND z.source_id=NEW.source_id AND z.disposition='settled'
    AND a.mode='mapping_recovery' AND a.confirmed_admission_plan_id=i.plan_id
    AND c.kind=approval_kind AND c.revision<=a.recovery_frozen_revision
    AND chosen=CASE WHEN approval_kind='stage' THEN i.recovery_stage_choice_id ELSE i.recovery_assignee_choice_id END
    AND (NEW.mapping_evidence_nonce IS NULL OR c.source_key_hmac=CASE WHEN approval_kind='stage' THEN NEW.repair_stage_source_hmac ELSE NEW.repair_assignee_source_hmac END)) THEN
   RAISE EXCEPTION 'recovery initial approval lacks successful owner' USING ERRCODE='P0001';
  END IF;
 END LOOP;
 RETURN NEW;
END $$;
CREATE TRIGGER people_recovery_refresh_approval BEFORE INSERT OR UPDATE ON migration_admitted_people_refresh_item FOR EACH ROW EXECUTE FUNCTION crm_people_recovery_refresh_approval();

-- Once-per-root retained stage catalog, allowing exact label lookups without a
-- full catalog scan for every candidate. Unsupported/duplicate entries stay held.
CREATE TABLE migration_people_recovery_catalog (
 admission_id UUID NOT NULL, organization_id UUID NOT NULL, source_id TEXT NOT NULL,
 source_key_hmac BYTEA CHECK(source_key_hmac IS NULL OR octet_length(source_key_hmac)=32),
 semantic_hmac BYTEA CHECK(semantic_hmac IS NULL OR octet_length(semantic_hmac)=32),
 qualified BOOLEAN NOT NULL,
 PRIMARY KEY(admission_id,organization_id,source_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id)
);
CREATE INDEX people_recovery_catalog_key ON migration_people_recovery_catalog(admission_id,organization_id,source_key_hmac);
GRANT SELECT,INSERT ON migration_people_recovery_catalog TO crm_app;
CREATE TRIGGER people_recovery_catalog_immutable BEFORE UPDATE OR DELETE ON migration_people_recovery_catalog FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE FUNCTION crm_people_recovery_catalog_owned() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM migration_people_admission a WHERE a.id=NEW.admission_id AND a.organization_id=NEW.organization_id
  AND a.mode='mapping_recovery' AND a.state='preparing' AND NOT a.recovery_catalog_complete AND NOT a.recovery_candidates_complete) THEN
  RAISE EXCEPTION 'recovery source catalog sealed' USING ERRCODE='P0001';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER people_recovery_catalog_owned BEFORE INSERT ON migration_people_recovery_catalog FOR EACH ROW EXECUTE FUNCTION crm_people_recovery_catalog_owned();

CREATE FUNCTION crm_people_recovery_result_owned() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE a migration_people_admission; i migration_people_admission_item; permit JSONB;
BEGIN
 SELECT * INTO a FROM migration_people_admission WHERE id=NEW.admission_id AND organization_id=NEW.organization_id;
 IF a.mode<>'mapping_recovery' THEN RETURN NEW; END IF;
 SELECT * INTO i FROM migration_people_admission_item WHERE id=NEW.item_id AND admission_id=a.id AND organization_id=a.organization_id;
 BEGIN permit:=current_setting('crm.people_admission_permit',true)::jsonb; EXCEPTION WHEN others THEN permit:=NULL; END;
 IF i.id IS NULL OR permit IS NULL OR a.state<>'running' OR a.lease_token IS DISTINCT FROM (permit->>'lease')::uuid
  OR a.lease_expires_at<=clock_timestamp() OR i.id IS DISTINCT FROM (permit->>'item')::uuid OR i.settled_at IS NOT NULL
  OR i.plan_id IS DISTINCT FROM a.confirmed_admission_plan_id OR i.source_id IS DISTINCT FROM NEW.source_id
  OR NEW.actor_user_id IS DISTINCT FROM a.initiated_by_user_id OR i.disposition<>'eligible'
  OR NOT EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=a.organization_id AND m.user_id=a.initiated_by_user_id AND m.role='admin' AND m.status='active') THEN
  RAISE EXCEPTION 'recovery result lease or owner mismatch' USING ERRCODE='P0001';
 END IF;
 IF NEW.disposition='settled' AND (NEW.person_id IS DISTINCT FROM i.prospective_person_id
   OR NOT crm_people_recovery_item_proof(a.organization_id,i.id)
   OR crm_people_recovery_previously_materialized(a.organization_id,a.source_account_id,i.source_id)) THEN
  RAISE EXCEPTION 'recovery result does not prove a new Person' USING ERRCODE='P0001';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER people_recovery_result_owned BEFORE INSERT ON migration_people_admission_result FOR EACH ROW EXECUTE FUNCTION crm_people_recovery_result_owned();

-- Bounded cohort handoff reads; other list indexes start with Organization only.
CREATE INDEX recovery_metadata_coverage ON migration_admitted_metadata_import(organization_id,admission_id,created_at DESC,id DESC);
CREATE INDEX recovery_activity_coverage ON migration_admitted_activity_import(organization_id,admission_id,created_at DESC,id DESC);
CREATE INDEX recovery_history_coverage ON migration_admitted_history_root(organization_id,admission_id,created_at DESC,id DESC);
CREATE INDEX recovery_choice_key_version ON migration_people_recovery_choice(admission_id,organization_id,key_id,revision DESC);
