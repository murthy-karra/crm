-- D-068: retained activity child. No source/parent/sibling rows are rewritten.
CREATE TABLE migration_activity_import (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL, parent_import_id UUID NOT NULL,
 parent_plan_id UUID NOT NULL, snapshot_id UUID NOT NULL, preview_id UUID NOT NULL,
 source_account_id BIGINT NOT NULL, capture_sequence BIGINT NOT NULL, workspace_revision BIGINT NOT NULL,
 executor_user_id UUID NOT NULL REFERENCES app_user(id),
 state TEXT NOT NULL CHECK(state IN ('preparing','ready','queued','running','paused','completed','cancelled')),
 phase TEXT NOT NULL DEFAULT 'preparation' CHECK(phase IN ('preparation','records','complete')),
 pause_reason TEXT, latest_plan_id UUID, confirmed_plan_id UUID, cancel_reservation_token UUID,
 lease_token UUID, lease_expires_at TIMESTAMPTZ, checkpoint_id UUID,
 revision BIGINT NOT NULL DEFAULT 1 CHECK(revision>0), activity_revision BIGINT NOT NULL DEFAULT 0 CHECK(activity_revision>=0),
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0), reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 native_bytes BIGINT NOT NULL DEFAULT 0 CHECK(native_bytes>=0), counts JSONB NOT NULL DEFAULT '{}',
 measured_bytes BIGINT NOT NULL DEFAULT 0 CHECK(measured_bytes>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), confirmed_at TIMESTAMPTZ, completed_at TIMESTAMPTZ,
 UNIQUE(id,organization_id), UNIQUE(id,snapshot_id,organization_id), UNIQUE(parent_import_id,organization_id),
 FOREIGN KEY(parent_import_id,snapshot_id,organization_id) REFERENCES migration_import(id,snapshot_id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(preview_id,snapshot_id,organization_id) REFERENCES migration_snapshot_preview(id,snapshot_id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE INDEX migration_activity_import_page ON migration_activity_import(organization_id,id);
CREATE INDEX migration_activity_import_claim ON migration_activity_import(created_at,id) WHERE state IN ('preparing','queued','running');
CREATE INDEX migration_activity_import_boundary ON migration_activity_import(organization_id) WHERE confirmed_plan_id IS NOT NULL;
CREATE TABLE migration_activity_plan (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 revision BIGINT NOT NULL CHECK(revision>0), parent_plan_id UUID, inherit_plan_id UUID,
 state TEXT NOT NULL CHECK(state IN ('building','ready','paused','superseded')),
 phase TEXT NOT NULL DEFAULT 'copying_choices' CHECK(phase IN ('copying_choices','captures','manifests','ready')),
 checkpoint_capture BIGINT NOT NULL DEFAULT 0, checkpoint_id UUID,
 pause_reason TEXT, patch_nonce BYTEA NOT NULL, patch_ciphertext BYTEA NOT NULL,
 destination_nonce BYTEA NOT NULL, destination_ciphertext BYTEA NOT NULL,
 confirmation_digest BYTEA, expires_at TIMESTAMPTZ, counts JSONB NOT NULL DEFAULT '{}',
 max_added_byte_bound BIGINT NOT NULL DEFAULT 0 CHECK(max_added_byte_bound>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), completed_at TIMESTAMPTZ,
 UNIQUE(id,import_id,organization_id), UNIQUE(import_id,organization_id,revision),
 FOREIGN KEY(import_id,snapshot_id,organization_id) REFERENCES migration_activity_import(id,snapshot_id,organization_id),
 FOREIGN KEY(parent_plan_id,import_id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id),
 FOREIGN KEY(inherit_plan_id,import_id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id)
);
ALTER TABLE migration_activity_import ADD FOREIGN KEY(latest_plan_id,id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id);
ALTER TABLE migration_activity_import ADD FOREIGN KEY(confirmed_plan_id,id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id);
CREATE TABLE migration_activity_choice (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('note_author','task_creator','task_assignee','task_kind')),
 source_key BYTEA NOT NULL CHECK(octet_length(source_key)=32), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,kind,source_key),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id)
);
CREATE TABLE migration_activity_source (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('notes','tasks','users')), source_id TEXT CHECK(length(source_id)<=128),
 record_id UUID, capture_id UUID NOT NULL, capture_sequence BIGINT NOT NULL, ordinal INTEGER NOT NULL,
 stream TEXT NOT NULL, representation TEXT NOT NULL, semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 negative BOOLEAN NOT NULL DEFAULT false, source_only_counts JSONB NOT NULL DEFAULT '{}' CHECK(jsonb_typeof(source_only_counts)='object'), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,capture_id,ordinal), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id),
 FOREIGN KEY(record_id,snapshot_id,organization_id) REFERENCES migration_snapshot_record(id,snapshot_id,organization_id),
 FOREIGN KEY(capture_id,snapshot_id,organization_id) REFERENCES migration_snapshot_capture(id,snapshot_id,organization_id)
);
CREATE INDEX migration_activity_source_page ON migration_activity_source(plan_id,organization_id,id);
CREATE INDEX migration_activity_source_identity ON migration_activity_source(plan_id,organization_id,family,source_id,representation,id);
CREATE INDEX migration_activity_source_negative ON migration_activity_source(plan_id,organization_id,semantic_hmac) WHERE negative;
CREATE TABLE migration_activity_mapping (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('note_author','task_creator','task_assignee','task_kind')),
 source_key BYTEA NOT NULL CHECK(octet_length(source_key)=32), dependent_count BIGINT NOT NULL DEFAULT 0,
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,kind,source_key), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id)
);
CREATE INDEX migration_activity_mapping_page ON migration_activity_mapping(plan_id,organization_id,id);
CREATE TABLE migration_activity_manifest (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('note','task')), source_id TEXT CHECK(length(source_id)<=128), source_row_id UUID NOT NULL,
 source_person_id TEXT CHECK(length(source_person_id)<=128), parent_result_id UUID, person_id UUID, target_id UUID,
 native_source_key TEXT CHECK(octet_length(native_source_key)<=160),
 author_user_id UUID, creator_user_id UUID, assignee_user_id UUID, completed_by_user_id UUID CHECK(completed_by_user_id IS NULL),
 disposition TEXT NOT NULL CHECK(disposition IN ('eligible','already_present','held')),
 added_byte_bound BIGINT NOT NULL CHECK(added_byte_bound>=0 AND added_byte_bound<=67108864), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,source_row_id), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id),
 FOREIGN KEY(source_row_id,plan_id,import_id,organization_id) REFERENCES migration_activity_source(id,plan_id,import_id,organization_id),
 FOREIGN KEY(parent_result_id,organization_id) REFERENCES migration_import_result(id,organization_id),
 FOREIGN KEY(organization_id,author_user_id) REFERENCES organization_membership(organization_id,user_id),
 FOREIGN KEY(organization_id,creator_user_id) REFERENCES organization_membership(organization_id,user_id),
 FOREIGN KEY(organization_id,assignee_user_id) REFERENCES organization_membership(organization_id,user_id)
);
CREATE UNIQUE INDEX migration_activity_manifest_identity ON migration_activity_manifest(plan_id,organization_id,kind,source_id) WHERE source_id IS NOT NULL;
CREATE INDEX migration_activity_manifest_page ON migration_activity_manifest(plan_id,organization_id,id);
CREATE INDEX migration_activity_manifest_filter ON migration_activity_manifest(plan_id,organization_id,kind,disposition,id);
CREATE INDEX migration_activity_manifest_person ON migration_activity_manifest(organization_id,person_id,id);
CREATE TABLE migration_activity_manifest_issue (
 manifest_id UUID NOT NULL, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL, code TEXT NOT NULL,
 PRIMARY KEY(manifest_id,organization_id,code),
 FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_activity_manifest(id,plan_id,import_id,organization_id)
);
CREATE INDEX migration_activity_manifest_issue_page ON migration_activity_manifest_issue(plan_id,organization_id,code,manifest_id);
CREATE TABLE migration_activity_identity (
 organization_id UUID NOT NULL REFERENCES organization(id), source_account_id BIGINT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('note','task')), source_id TEXT NOT NULL CHECK(length(source_id)<=128), target_id UUID NOT NULL,
 import_id UUID NOT NULL, plan_id UUID NOT NULL, manifest_id UUID NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(organization_id,source_account_id,kind,source_id),
 FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_activity_manifest(id,plan_id,import_id,organization_id)
 -- No target FK: deletion retains the identity tombstone.
);
CREATE INDEX migration_activity_identity_target ON migration_activity_identity(organization_id,kind,target_id);
CREATE TABLE migration_activity_result (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 manifest_id UUID NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('note','task')), source_id TEXT, person_id UUID, target_id UUID,
 disposition TEXT NOT NULL CHECK(disposition IN ('applied','already_present','held')),
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, actual_bytes BIGINT NOT NULL CHECK(actual_bytes>=0), native_bytes BIGINT NOT NULL DEFAULT 0 CHECK(native_bytes>=0), committed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(import_id,organization_id,manifest_id), UNIQUE(id,organization_id), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_activity_manifest(id,plan_id,import_id,organization_id)
);
CREATE INDEX migration_activity_result_page ON migration_activity_result(plan_id,organization_id,id);
CREATE INDEX migration_activity_result_filter ON migration_activity_result(plan_id,organization_id,kind,disposition,id);
CREATE INDEX migration_activity_result_person ON migration_activity_result(organization_id,person_id,kind,target_id,id);
-- Immutable execution issues are distinct from the frozen plan's issues.
CREATE TABLE migration_activity_result_issue (
 result_id UUID NOT NULL, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL, code TEXT NOT NULL,
 PRIMARY KEY(result_id,organization_id,code),
 FOREIGN KEY(result_id,plan_id,import_id,organization_id) REFERENCES migration_activity_result(id,plan_id,import_id,organization_id)
);
CREATE INDEX migration_activity_result_issue_page ON migration_activity_result_issue(plan_id,organization_id,code,result_id);
CREATE TABLE migration_activity_receipt (
 organization_id UUID NOT NULL, actor_user_id UUID NOT NULL REFERENCES app_user(id), action TEXT NOT NULL,
 request_id UUID NOT NULL, import_id UUID NOT NULL, input_digest BYTEA NOT NULL CHECK(octet_length(input_digest)=32),
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(organization_id,actor_user_id,action,request_id),
 FOREIGN KEY(import_id,organization_id) REFERENCES migration_activity_import(id,organization_id)
);
CREATE TABLE migration_activity_reservation (
 token UUID PRIMARY KEY, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 plan_id UUID NOT NULL, lease_token UUID NOT NULL, purpose TEXT NOT NULL CHECK(purpose IN ('work','cancel')),
 byte_count BIGINT NOT NULL CHECK(byte_count>0 AND byte_count<=67108864), expires_at TIMESTAMPTZ NOT NULL,
 FOREIGN KEY(import_id,snapshot_id,organization_id) REFERENCES migration_activity_import(id,snapshot_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id)
);
CREATE INDEX migration_activity_reservation_owner ON migration_activity_reservation(import_id,organization_id,purpose);
CREATE TABLE migration_activity_issue (
 plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL, code TEXT NOT NULL,
 record_count BIGINT NOT NULL CHECK(record_count>0), PRIMARY KEY(plan_id,organization_id,code),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_activity_plan(id,import_id,organization_id)
);
GRANT SELECT,INSERT,UPDATE ON migration_activity_import,migration_activity_plan,migration_activity_mapping,migration_activity_issue TO crm_app;
GRANT SELECT,INSERT ON migration_activity_choice,migration_activity_source,migration_activity_manifest,migration_activity_manifest_issue,migration_activity_identity,migration_activity_result,migration_activity_result_issue,migration_activity_receipt TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_activity_reservation TO crm_app;
GRANT UPDATE(byte_count) ON migration_activity_reservation TO crm_app;
CREATE TRIGGER migration_activity_receipt_append_only BEFORE UPDATE OR DELETE ON migration_activity_receipt FOR EACH ROW EXECUTE FUNCTION reject_mutation();

-- Exact logical inventory: 256 bytes per durable row for fixed identifiers and
-- control overhead, plus every variable-length text/JSON/bytea column. Native
-- tables are deliberately absent. Reservation capacity already covers its own
-- transient descriptor; the descriptor is deleted when that capacity settles.
CREATE FUNCTION crm_activity_retained_size(table_name TEXT,v JSONB) RETURNS BIGINT LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE total BIGINT:=256; k TEXT; value TEXT;
BEGIN
 IF v IS NULL THEN RETURN 0; END IF;
 FOR k,value IN SELECT e.key,e.value #>> '{}' FROM jsonb_each(v) e LOOP
  IF value IS NULL THEN CONTINUE; END IF;
  IF k IN ('nonce','ciphertext','patch_nonce','patch_ciphertext','destination_nonce','destination_ciphertext','confirmation_digest','semantic_hmac','source_key','input_digest') THEN total:=total+octet_length(decode(substring(value FROM 3),'hex'));
  ELSIF k IN ('state','phase','pause_reason','family','source_id','source_person_id','native_source_key','stream','representation','kind','disposition','action','code','counts','source_only_counts') THEN total:=total+octet_length(value);
  END IF;
 END LOOP;
 RETURN total;
END $$;
CREATE FUNCTION crm_activity_measure_row() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE delta BIGINT; child UUID; org UUID;
BEGIN
 delta:=crm_activity_retained_size(TG_TABLE_NAME,CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)-crm_activity_retained_size(TG_TABLE_NAME,CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta<>0 THEN
  child:=(COALESCE(to_jsonb(NEW),to_jsonb(OLD))->>CASE WHEN TG_TABLE_NAME='migration_activity_import' THEN 'id' ELSE 'import_id' END)::uuid;
  org:=(COALESCE(to_jsonb(NEW),to_jsonb(OLD))->>'organization_id')::uuid;
  UPDATE migration_activity_import SET measured_bytes=measured_bytes+delta WHERE id=child AND organization_id=org;
 END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['migration_activity_import','migration_activity_plan','migration_activity_choice','migration_activity_source','migration_activity_mapping','migration_activity_manifest','migration_activity_manifest_issue','migration_activity_identity','migration_activity_result','migration_activity_result_issue','migration_activity_receipt','migration_activity_issue'] LOOP
  EXECUTE format('CREATE TRIGGER %I AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_activity_measure_row()',name||'_measure',name);
 END LOOP;
END $$;
REVOKE ALL ON FUNCTION crm_activity_retained_size(TEXT,JSONB),crm_activity_measure_row() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_activity_retained_size(TEXT,JSONB),crm_activity_measure_row() TO crm_app;

-- Complete readers invoke this under the same shared guard as their fetch.
CREATE FUNCTION crm_activity_complete_read(org UUID) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
 PERFORM crm_workspace_shared(org);
 IF EXISTS(SELECT 1 FROM organization o JOIN migration_activity_import i ON i.organization_id=o.id WHERE o.id=org AND o.workspace_mode='migration_review' AND i.confirmed_plan_id IS NOT NULL)
 THEN RAISE EXCEPTION USING ERRCODE='P010F',MESSAGE='activity_review_required'; END IF;
END $$;
REVOKE ALL ON FUNCTION crm_activity_complete_read(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_activity_complete_read(UUID) TO crm_app;

CREATE FUNCTION crm_activity_insert_allowed(org UUID,table_name TEXT,row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE token TEXT:=current_setting('crm.activity_token',true); unit_text TEXT:=current_setting('crm.activity_unit',true); unit UUID;
BEGIN
 IF table_name NOT IN ('note','task') OR token IS NULL OR token='' OR unit_text IS NULL OR unit_text='' THEN RETURN false; END IF;
 BEGIN unit:=unit_text::uuid; EXCEPTION WHEN invalid_text_representation THEN RETURN false; END;
 RETURN EXISTS(SELECT 1 FROM migration_activity_manifest a
 JOIN migration_activity_import i ON i.id=a.import_id AND i.organization_id=a.organization_id AND i.confirmed_plan_id=a.plan_id
 JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id
 JOIN migration_import p ON p.id=i.parent_import_id AND p.organization_id=i.organization_id AND p.confirmed_plan_id=i.parent_plan_id
 JOIN organization o ON o.id=i.organization_id
 JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id
 JOIN migration_import_result r ON r.id=a.parent_result_id AND r.organization_id=a.organization_id AND r.import_id=i.parent_import_id AND r.plan_id=i.parent_plan_id AND r.person_id=a.person_id AND r.source_id=a.source_person_id
 JOIN migration_import_identity d ON d.organization_id=i.organization_id AND d.source_account_id=i.source_account_id AND d.family='people' AND d.source_id=a.source_person_id AND d.target_id=a.person_id AND d.import_id=i.parent_import_id AND d.plan_id=i.parent_plan_id
 JOIN person n ON n.id=a.person_id AND n.organization_id=a.organization_id
 WHERE a.id=unit AND a.organization_id=org AND a.disposition='eligible' AND a.kind=table_name
 AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp()
 AND p.state='completed' AND o.workspace_mode='migration_review' AND o.workspace_revision=i.workspace_revision
 AND m.role='admin' AND m.status='active' AND r.disposition IN ('imported','already_imported')
 AND a.target_id=(row_value->>'id')::uuid AND a.person_id=(row_value->>'person_id')::uuid
 AND row_value->>'origin'='migration' AND row_value->>'source'='fub' AND a.native_source_key=row_value->>'source_external_id'
 AND row_value->>'deleted_at' IS NULL AND row_value->>'deleted_by_user_id' IS NULL
 AND ((table_name='note' AND a.author_user_id IS NOT DISTINCT FROM (row_value->>'author_user_id')::uuid)
 OR(table_name='task' AND a.creator_user_id IS NOT DISTINCT FROM (row_value->>'created_by_user_id')::uuid
 AND a.assignee_user_id IS NOT DISTINCT FROM (row_value->>'assignee_user_id')::uuid AND row_value->>'completed_by_user_id' IS NULL
 AND (a.assignee_user_id IS NULL OR EXISTS(SELECT 1 FROM organization_membership am WHERE am.organization_id=org AND am.user_id=a.assignee_user_id AND am.status='active')))));
END $$;
REVOKE ALL ON FUNCTION crm_activity_insert_allowed(UUID,TEXT,JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_activity_insert_allowed(UUID,TEXT,JSONB) TO crm_app;

-- Preserve every 010c/010f1/terminal-case; add only a closed INSERT branch.
CREATE OR REPLACE FUNCTION crm_workspace_mutation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE org UUID; row_value JSONB; token TEXT; terminal UUID; person UUID;
BEGIN
 IF current_user<>'crm_app' THEN RETURN COALESCE(NEW,OLD); END IF;
 row_value:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 org:=(row_value->>'organization_id')::uuid;
 IF TG_TABLE_NAME='operator_task_proposal' THEN SELECT organization_id INTO org FROM operator_proposal WHERE id=(row_value->>'proposal_id')::uuid; END IF;
 IF org IS NULL THEN RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_scope_required'; END IF;
 PERFORM crm_workspace_shared(org);
 IF EXISTS(SELECT 1 FROM organization WHERE id=org AND workspace_mode='operational') THEN RETURN COALESCE(NEW,OLD); END IF;
 IF TG_OP='INSERT' AND (crm_metadata_insert_allowed(org,TG_TABLE_NAME,row_value) OR crm_activity_insert_allowed(org,TG_TABLE_NAME,row_value)) THEN RETURN NEW; END IF;
 token:=current_setting('crm.import_token',true);
 IF token IS NOT NULL AND TG_TABLE_NAME IN ('person','contact_method','stage','assignment_changed','stage_changed','person_imported')
 AND EXISTS(SELECT 1 FROM migration_workspace w JOIN migration_import i ON i.id=w.import_id AND i.organization_id=w.organization_id
 JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id
 WHERE w.organization_id=org AND w.plan_id=i.confirmed_plan_id AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active') THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.terminal_call',true);
 IF token IS NOT NULL AND token<>'' AND TG_TABLE_NAME IN ('call','call_completed','contact_attempted','person') THEN
 terminal:=token::uuid; SELECT person_id INTO person FROM call WHERE id=terminal AND organization_id=org;
 IF person IS NOT NULL AND ((TG_TABLE_NAME='call' AND (row_value->>'id')::uuid=terminal AND row_value->>'status' IN ('ended','failed','cancelled')) OR (TG_TABLE_NAME IN ('call_completed','contact_attempted') AND (row_value->>'person_id')::uuid=person) OR (TG_TABLE_NAME='person' AND TG_OP='UPDATE' AND (row_value->>'id')::uuid=person AND (to_jsonb(OLD)-'updated_at'-'last_contact_at')=(to_jsonb(NEW)-'updated_at'-'last_contact_at'))) THEN RETURN COALESCE(NEW,OLD); END IF;
 END IF;
 RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
END $$;

CREATE INDEX task_org_person_completed_review ON task(organization_id,person_id,completed_at,id) WHERE completed_at IS NOT NULL AND deleted_at IS NULL;
