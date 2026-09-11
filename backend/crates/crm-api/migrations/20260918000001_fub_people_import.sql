-- D-065: additive People import and durable workspace review boundary.
ALTER TABLE organization
 ADD COLUMN workspace_mode TEXT NOT NULL DEFAULT 'operational'
 CHECK(workspace_mode IN ('operational','migration_review')),
 ADD COLUMN workspace_revision BIGINT NOT NULL DEFAULT 1 CHECK(workspace_revision>0);
ALTER TABLE contact_method ADD COLUMN import_order INTEGER CHECK(import_order>=0);
CREATE UNIQUE INDEX contact_method_import_order ON contact_method(person_id,kind,import_order) WHERE import_order IS NOT NULL;
GRANT INSERT ON stage TO crm_app;
GRANT INSERT(import_order) ON contact_method TO crm_app;

CREATE TABLE workspace_operation_admission (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id),
 actor_user_id UUID NOT NULL REFERENCES app_user(id),
 kind TEXT NOT NULL CHECK(kind='operator'), created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 deadline TIMESTAMPTZ NOT NULL, CHECK(deadline>created_at)
);
CREATE INDEX workspace_admission_active ON workspace_operation_admission(organization_id,deadline);
GRANT SELECT,INSERT,DELETE ON workspace_operation_admission TO crm_app;

CREATE TABLE migration_import (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL, snapshot_id UUID NOT NULL,
 preview_id UUID NOT NULL, source_account_id BIGINT NOT NULL, capture_sequence BIGINT NOT NULL,
 executor_user_id UUID NOT NULL REFERENCES app_user(id),
 state TEXT NOT NULL CHECK(state IN ('proposed','queued','running','paused','completed','cancelled','expired')),
 phase TEXT NOT NULL DEFAULT 'preparation' CHECK(phase IN ('preparation','stages','people','complete')),
 pause_reason TEXT, latest_plan_id UUID, confirmed_plan_id UUID, cancel_reservation_token UUID,
 lease_token UUID, lease_expires_at TIMESTAMPTZ,
 checkpoint_source_id TEXT NOT NULL DEFAULT '', checkpoint_stage_id TEXT NOT NULL DEFAULT '',
 checkpoint_stage_sequence BIGINT NOT NULL DEFAULT 0, checkpoint_stage_ordinal INTEGER NOT NULL DEFAULT -1,
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0), reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 imported_people BIGINT NOT NULL DEFAULT 0, imported_contacts BIGINT NOT NULL DEFAULT 0, settled_people BIGINT NOT NULL DEFAULT 0,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 confirmed_at TIMESTAMPTZ, completed_at TIMESTAMPTZ,
 UNIQUE(id,organization_id), UNIQUE(id,snapshot_id,organization_id),
 FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 FOREIGN KEY(preview_id,snapshot_id,organization_id) REFERENCES migration_snapshot_preview(id,snapshot_id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE INDEX migration_import_latest ON migration_import(organization_id,created_at DESC,id DESC);
CREATE INDEX migration_import_claim ON migration_import(created_at,id) WHERE state IN ('proposed','queued','running');
CREATE TABLE migration_import_plan (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 revision BIGINT NOT NULL CHECK(revision>0), parent_plan_id UUID,
 state TEXT NOT NULL CHECK(state IN ('building','ready','paused','failed','superseded')),
 phase TEXT NOT NULL DEFAULT 'copying_choices' CHECK(phase IN ('copying_choices','captures','mappings','people','ready')),
 checkpoint_capture BIGINT NOT NULL DEFAULT 0, checkpoint_family TEXT NOT NULL DEFAULT '',
 checkpoint_source_id TEXT NOT NULL DEFAULT '', pause_reason TEXT,
 patch_nonce BYTEA NOT NULL, patch_ciphertext BYTEA NOT NULL,
 confirmation_digest BYTEA, expires_at TIMESTAMPTZ,
 source_people BIGINT NOT NULL DEFAULT 0, eligible_people BIGINT NOT NULL DEFAULT 0,
 held_people BIGINT NOT NULL DEFAULT 0, invalid_ids BIGINT NOT NULL DEFAULT 0,
 contacts BIGINT NOT NULL DEFAULT 0, overlap_people BIGINT NOT NULL DEFAULT 0,
 stages_to_create BIGINT NOT NULL DEFAULT 0, assigned_people BIGINT NOT NULL DEFAULT 0,
 unassigned_people BIGINT NOT NULL DEFAULT 0, max_added_byte_bound BIGINT NOT NULL DEFAULT 0,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), completed_at TIMESTAMPTZ,
 UNIQUE(id,import_id,organization_id), UNIQUE(import_id,organization_id,revision),
 FOREIGN KEY(import_id,snapshot_id,organization_id) REFERENCES migration_import(id,snapshot_id,organization_id),
 FOREIGN KEY(parent_plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id)
);
ALTER TABLE migration_import ADD FOREIGN KEY(latest_plan_id,id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id);
ALTER TABLE migration_import ADD FOREIGN KEY(confirmed_plan_id,id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id);
CREATE TABLE migration_workspace (
 organization_id UUID PRIMARY KEY REFERENCES organization(id), import_id UUID NOT NULL,
 plan_id UUID NOT NULL, entered_by_user_id UUID NOT NULL REFERENCES app_user(id),
 entered_at TIMESTAMPTZ NOT NULL DEFAULT now(), gate_version TEXT NOT NULL CHECK(gate_version='crm-workspace-v1'),
 FOREIGN KEY(import_id,organization_id) REFERENCES migration_import(id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id)
);
CREATE TABLE migration_import_choice (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('stage','assignee')), source_key TEXT NOT NULL,
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,kind,source_key),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id)
);
CREATE TABLE migration_import_source (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('people','stages','users')), source_id TEXT NOT NULL,
 record_id UUID NOT NULL, capture_id UUID NOT NULL, ordinal INTEGER NOT NULL,
 representation TEXT NOT NULL, semantic_hmac BYTEA NOT NULL,
 qualified BOOLEAN NOT NULL, conflict BOOLEAN NOT NULL DEFAULT false,
 observations BIGINT NOT NULL DEFAULT 1, nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,family,source_id), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(record_id,snapshot_id,organization_id) REFERENCES migration_snapshot_record(id,snapshot_id,organization_id),
 FOREIGN KEY(capture_id,snapshot_id,organization_id) REFERENCES migration_snapshot_capture(id,snapshot_id,organization_id)
);
CREATE TABLE migration_import_mapping (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('stage','assignee')), source_key TEXT NOT NULL,
 qualified BOOLEAN NOT NULL, disposition TEXT NOT NULL CHECK(disposition IN ('existing','create','member','unassigned','hold')),
 target_id UUID, nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 source_sequence BIGINT NOT NULL DEFAULT 0, source_ordinal INTEGER NOT NULL DEFAULT 0, label_hmac BYTEA,
 dependent_count BIGINT NOT NULL DEFAULT 0, added_byte_bound BIGINT NOT NULL DEFAULT 0 CHECK(added_byte_bound>=0),
 UNIQUE(plan_id,organization_id,kind,source_key), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id)
);
CREATE TABLE migration_import_manifest (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 source_id TEXT NOT NULL, source_row_id UUID NOT NULL,
 disposition TEXT NOT NULL CHECK(disposition IN ('eligible','held')),
 stage_mapping_id UUID, assignee_mapping_id UUID,
 contact_count BIGINT NOT NULL CHECK(contact_count>=0), overlap_count BIGINT NOT NULL DEFAULT 0,
 added_byte_bound BIGINT NOT NULL CHECK(added_byte_bound>=0), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,source_id), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(source_row_id,plan_id,import_id,organization_id) REFERENCES migration_import_source(id,plan_id,import_id,organization_id),
 FOREIGN KEY(stage_mapping_id,plan_id,import_id,organization_id) REFERENCES migration_import_mapping(id,plan_id,import_id,organization_id),
 FOREIGN KEY(assignee_mapping_id,plan_id,import_id,organization_id) REFERENCES migration_import_mapping(id,plan_id,import_id,organization_id)
);
CREATE INDEX migration_import_manifest_page ON migration_import_manifest(plan_id,organization_id,disposition,source_id);
CREATE INDEX migration_import_mapping_execution ON migration_import_mapping(plan_id,organization_id,kind,disposition,source_sequence,source_ordinal,source_key);
CREATE TABLE migration_import_identity (
 organization_id UUID NOT NULL REFERENCES organization(id), source_account_id BIGINT NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('people','stages')), source_id TEXT NOT NULL,
 target_id UUID NOT NULL, import_id UUID NOT NULL, plan_id UUID NOT NULL,
 mapping_id UUID, manifest_id UUID, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(organization_id,source_account_id,family,source_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(mapping_id,plan_id,import_id,organization_id) REFERENCES migration_import_mapping(id,plan_id,import_id,organization_id),
 FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_import_manifest(id,plan_id,import_id,organization_id)
 -- Deliberately no target FK/cascade: erasure cannot remove the identity tombstone.
);
CREATE INDEX migration_import_identity_target ON migration_import_identity(organization_id,family,target_id);
CREATE TABLE migration_import_result (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 manifest_id UUID NOT NULL, source_id TEXT NOT NULL, person_id UUID,
 disposition TEXT NOT NULL CHECK(disposition IN ('imported','already_imported','held')),
 contact_count BIGINT NOT NULL, committed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 provenance_nonce BYTEA NOT NULL, provenance_ciphertext BYTEA NOT NULL,
 UNIQUE(import_id,organization_id,source_id), UNIQUE(id,organization_id),
 FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_import_manifest(id,plan_id,import_id,organization_id)
);
CREATE INDEX migration_import_result_person ON migration_import_result(organization_id,person_id) WHERE person_id IS NOT NULL;
CREATE INDEX migration_import_result_page ON migration_import_result(import_id,organization_id,disposition,source_id);
CREATE INDEX migration_import_result_manifest ON migration_import_result(manifest_id,organization_id);
CREATE TABLE migration_import_contact (
 result_id UUID NOT NULL, organization_id UUID NOT NULL, contact_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('email','phone')), import_order INTEGER NOT NULL CHECK(import_order>=0),
 PRIMARY KEY(result_id,kind,import_order), FOREIGN KEY(result_id,organization_id) REFERENCES migration_import_result(id,organization_id)
);
CREATE TABLE migration_import_receipt (
 organization_id UUID NOT NULL REFERENCES organization(id), actor_user_id UUID NOT NULL REFERENCES app_user(id),
 action TEXT NOT NULL, request_id UUID NOT NULL, input_digest BYTEA NOT NULL,
 import_id UUID NOT NULL, nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(organization_id,actor_user_id,action,request_id),
 FOREIGN KEY(import_id,organization_id) REFERENCES migration_import(id,organization_id)
);
CREATE TABLE migration_import_reservation (
 token UUID PRIMARY KEY, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 plan_id UUID NOT NULL, lease_token UUID NOT NULL, purpose TEXT NOT NULL DEFAULT 'work' CHECK(purpose IN ('work','cancel')), byte_count BIGINT NOT NULL CHECK(byte_count>0 AND byte_count<=67108864),
 expires_at TIMESTAMPTZ NOT NULL,
 FOREIGN KEY(import_id,snapshot_id,organization_id) REFERENCES migration_import(id,snapshot_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id)
);
CREATE TABLE person_imported (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id),
 actor_kind TEXT NOT NULL CHECK(actor_kind='system'), actor_user_id UUID CHECK(actor_user_id IS NULL),
 on_behalf_of_user_id UUID NOT NULL REFERENCES app_user(id), origin TEXT NOT NULL CHECK(origin='migration'),
 occurred_at TIMESTAMPTZ NOT NULL, recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 correlation_id UUID NOT NULL, causation_id UUID, corrects_id UUID,
 person_id UUID NOT NULL, import_id UUID NOT NULL, plan_id UUID NOT NULL,
 source_record_id UUID NOT NULL, capture_id UUID NOT NULL,
 UNIQUE(organization_id,import_id,person_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id)
);
CREATE INDEX person_imported_history ON person_imported(organization_id,person_id,occurred_at,id);
GRANT SELECT,INSERT,UPDATE ON migration_import,migration_import_plan,migration_import_source,migration_import_mapping TO crm_app;
GRANT SELECT,INSERT ON migration_workspace,migration_import_choice,migration_import_manifest,migration_import_identity,migration_import_result,migration_import_contact,migration_import_receipt,person_imported TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_import_reservation TO crm_app;
GRANT UPDATE(purpose,expires_at) ON migration_import_reservation TO crm_app;
CREATE TRIGGER person_imported_append_only BEFORE UPDATE OR DELETE ON person_imported FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER person_imported_no_truncate BEFORE TRUNCATE ON person_imported FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER migration_import_receipt_append_only BEFORE UPDATE OR DELETE ON migration_import_receipt FOR EACH ROW EXECUTE FUNCTION reject_mutation();

-- Closed, non-PII error messages. Transaction locks are acquired before business
-- locks in application commands; these functions also protect direct app SQL.
CREATE FUNCTION crm_workspace_shared(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF NOT pg_try_advisory_xact_lock_shared(hashtextextended('crm-workspace-v1:'||org::text,0)) THEN
  RAISE EXCEPTION USING ERRCODE='55P03',MESSAGE='workspace_busy';
 END IF;
END $$;
CREATE FUNCTION crm_workspace_operational(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM crm_workspace_shared(org);
 IF NOT EXISTS(SELECT 1 FROM organization WHERE id=org AND workspace_mode='operational') THEN
  RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
 END IF;
END $$;
CREATE FUNCTION crm_workspace_read(org UUID, actor UUID, operational_only BOOLEAN) RETURNS void LANGUAGE plpgsql AS $$
DECLARE mode TEXT; member_role TEXT;
BEGIN
 PERFORM crm_workspace_shared(org);
 SELECT o.workspace_mode,m.role INTO mode,member_role FROM organization o
 JOIN organization_membership m ON m.organization_id=o.id
 WHERE o.id=org AND m.user_id=actor AND m.status='active';
 IF mode IS NULL THEN RAISE EXCEPTION USING ERRCODE='P010A',MESSAGE='forbidden'; END IF;
 IF mode<>'operational' AND (operational_only OR member_role<>'admin') THEN
  RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
 END IF;
END $$;
CREATE FUNCTION crm_workspace_mutation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE org UUID; row_value JSONB; token TEXT; terminal UUID; person UUID;
BEGIN
 IF current_user<>'crm_app' THEN RETURN COALESCE(NEW,OLD); END IF;
 row_value:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 org:=(row_value->>'organization_id')::uuid;
 IF TG_TABLE_NAME='operator_task_proposal' THEN
  SELECT organization_id INTO org FROM operator_proposal WHERE id=(row_value->>'proposal_id')::uuid;
 END IF;
 IF org IS NULL THEN RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_scope_required'; END IF;
 PERFORM crm_workspace_shared(org);
 IF EXISTS(SELECT 1 FROM organization WHERE id=org AND workspace_mode='operational') THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.import_token',true);
 IF token IS NOT NULL AND TG_TABLE_NAME IN ('person','contact_method','stage','assignment_changed','stage_changed','person_imported')
 AND EXISTS(SELECT 1 FROM migration_workspace w JOIN migration_import i ON i.id=w.import_id AND i.organization_id=w.organization_id
  JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id
  WHERE w.organization_id=org AND w.plan_id=i.confirmed_plan_id AND i.state='running'
   AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active')
 THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.terminal_call',true);
 IF token IS NOT NULL AND token<>'' AND TG_TABLE_NAME IN ('call','call_completed','contact_attempted','person') THEN
  terminal:=token::uuid;
  SELECT person_id INTO person FROM call WHERE id=terminal AND organization_id=org;
  IF person IS NOT NULL AND (
   (TG_TABLE_NAME='call' AND (row_value->>'id')::uuid=terminal AND row_value->>'status' IN ('ended','failed','cancelled')) OR
   (TG_TABLE_NAME IN ('call_completed','contact_attempted') AND (row_value->>'person_id')::uuid=person) OR
   (TG_TABLE_NAME='person' AND TG_OP='UPDATE' AND (row_value->>'id')::uuid=person
    AND (to_jsonb(OLD)-'updated_at'-'last_contact_at')=(to_jsonb(NEW)-'updated_at'-'last_contact_at')))
  THEN RETURN COALESCE(NEW,OLD); END IF;
 END IF;
 RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['person','contact_method','inquiry','inquiry_received','routing_decision','assignment_changed','stage_changed','contact_attempted','person_tag','person_custom_field_value','note','task','call','call_completed','raw_payload','intake_extraction','correspondence_raw','correspondence_captured','capture_message','operator_proposal','operator_task_proposal','tag','custom_field','custom_field_option','saved_list','today_work_source','today_system_feed','today_feed_changed','intake_rotation','intake_token_rotated','capture_token_rotated','person_imported','stage'] LOOP
  EXECUTE format('CREATE TRIGGER workspace_write_guard BEFORE INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_workspace_mutation_guard()',t);
 END LOOP;
END $$;
REVOKE ALL ON FUNCTION crm_workspace_shared(UUID),crm_workspace_operational(UUID),crm_workspace_read(UUID,UUID,BOOLEAN),crm_workspace_mutation_guard() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_workspace_shared(UUID),crm_workspace_operational(UUID),crm_workspace_read(UUID,UUID,BOOLEAN),crm_workspace_mutation_guard() TO crm_app;

CREATE INDEX migration_import_stage_label ON migration_import_mapping(plan_id,organization_id,label_hmac,source_key) WHERE kind='stage';
CREATE TABLE migration_import_issue (
 plan_id UUID NOT NULL, organization_id UUID NOT NULL, code TEXT NOT NULL,record_count BIGINT NOT NULL CHECK(record_count>0),
 PRIMARY KEY(plan_id,organization_id,code)
);
GRANT SELECT,INSERT,UPDATE ON migration_import_issue TO crm_app;
GRANT UPDATE(workspace_mode,workspace_revision) ON organization TO crm_app;
CREATE FUNCTION crm_workspace_organization_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 PERFORM crm_workspace_shared(NEW.id);
 IF OLD.workspace_mode<>NEW.workspace_mode OR OLD.workspace_revision<>NEW.workspace_revision THEN
  IF OLD.workspace_mode<>'operational' OR NEW.workspace_mode<>'migration_review' OR NEW.workspace_revision<>OLD.workspace_revision+1
   OR NOT EXISTS(SELECT 1 FROM migration_workspace WHERE organization_id=NEW.id)
   OR NOT pg_try_advisory_xact_lock(hashtextextended('crm-workspace-v1:'||NEW.id::text,0)) THEN
   RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_transition_forbidden';
  END IF;
 END IF;
 IF OLD.workspace_mode='migration_review' AND (to_jsonb(NEW)-ARRAY['workspace_mode','workspace_revision','updated_at','status']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['workspace_mode','workspace_revision','updated_at','status']) THEN
  RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER workspace_organization_guard BEFORE UPDATE ON organization FOR EACH ROW EXECUTE FUNCTION crm_workspace_organization_guard();

CREATE INDEX migration_import_manifest_stage ON migration_import_manifest(stage_mapping_id,organization_id) WHERE disposition='eligible';
