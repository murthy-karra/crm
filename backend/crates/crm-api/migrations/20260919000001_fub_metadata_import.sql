-- D-066: retained metadata child; original People/workspace/source rows stay intact.
CREATE TABLE migration_metadata_import (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL, parent_import_id UUID NOT NULL,
 parent_plan_id UUID NOT NULL, snapshot_id UUID NOT NULL, preview_id UUID NOT NULL,
 source_account_id BIGINT NOT NULL, capture_sequence BIGINT NOT NULL, workspace_revision BIGINT NOT NULL,
 executor_user_id UUID NOT NULL REFERENCES app_user(id),
 state TEXT NOT NULL CHECK(state IN ('proposed','queued','running','paused','completed','cancelled','expired')),
 phase TEXT NOT NULL DEFAULT 'preparation' CHECK(phase IN ('preparation','catalog','people','complete')),
 pause_reason TEXT, latest_plan_id UUID, confirmed_plan_id UUID, cancel_reservation_token UUID,
 lease_token UUID, lease_expires_at TIMESTAMPTZ, checkpoint_id UUID,
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 counts JSONB NOT NULL DEFAULT '{}',
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 confirmed_at TIMESTAMPTZ, completed_at TIMESTAMPTZ,
 UNIQUE(id,organization_id), UNIQUE(id,snapshot_id,organization_id), UNIQUE(parent_import_id,organization_id),
 FOREIGN KEY(parent_import_id,snapshot_id,organization_id) REFERENCES migration_import(id,snapshot_id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(preview_id,snapshot_id,organization_id) REFERENCES migration_snapshot_preview(id,snapshot_id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE INDEX migration_metadata_import_page ON migration_metadata_import(organization_id,id);
CREATE INDEX migration_metadata_import_latest ON migration_metadata_import(organization_id,created_at DESC,id DESC);
CREATE INDEX migration_metadata_import_claim ON migration_metadata_import(created_at,id) WHERE state IN ('proposed','queued','running');
CREATE TABLE migration_metadata_plan (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 revision BIGINT NOT NULL CHECK(revision>0), parent_plan_id UUID, inherit_plan_id UUID,
 state TEXT NOT NULL CHECK(state IN ('building','ready','paused','failed','superseded')),
 phase TEXT NOT NULL DEFAULT 'copying_choices' CHECK(phase IN ('copying_choices','captures','catalog','mappings','people','ready')),
 checkpoint_capture BIGINT NOT NULL DEFAULT 0, checkpoint_element INTEGER NOT NULL DEFAULT 0 CHECK(checkpoint_element>=0), checkpoint_kind TEXT NOT NULL DEFAULT '', checkpoint_key BYTEA,
 checkpoint_id UUID, pause_reason TEXT, patch_nonce BYTEA NOT NULL, patch_ciphertext BYTEA NOT NULL,
 destination_nonce BYTEA NOT NULL, destination_ciphertext BYTEA NOT NULL,
 confirmation_digest BYTEA, expires_at TIMESTAMPTZ, counts JSONB NOT NULL DEFAULT '{}',
 max_added_byte_bound BIGINT NOT NULL DEFAULT 0 CHECK(max_added_byte_bound>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), completed_at TIMESTAMPTZ,
 UNIQUE(id,import_id,organization_id), UNIQUE(import_id,organization_id,revision),
 FOREIGN KEY(import_id,snapshot_id,organization_id) REFERENCES migration_metadata_import(id,snapshot_id,organization_id),
 FOREIGN KEY(parent_plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(inherit_plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id)
);
ALTER TABLE migration_metadata_import ADD FOREIGN KEY(latest_plan_id,id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id);
ALTER TABLE migration_metadata_import ADD FOREIGN KEY(confirmed_plan_id,id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id);
CREATE TABLE migration_metadata_choice (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option')), source_key BYTEA NOT NULL CHECK(octet_length(source_key)=32),
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, UNIQUE(plan_id,organization_id,kind,source_key),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id)
);
CREATE TABLE migration_metadata_source (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('people','custom_fields')), source_id TEXT NOT NULL CHECK(length(source_id)<=128),
 record_id UUID NOT NULL, capture_id UUID NOT NULL, capture_sequence BIGINT NOT NULL, ordinal INTEGER NOT NULL,
 representation TEXT NOT NULL, semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 qualified BOOLEAN NOT NULL, conflict BOOLEAN NOT NULL DEFAULT false, observations BIGINT NOT NULL DEFAULT 1,
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,family,source_id), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(record_id,snapshot_id,organization_id) REFERENCES migration_snapshot_record(id,snapshot_id,organization_id),
 FOREIGN KEY(capture_id,snapshot_id,organization_id) REFERENCES migration_snapshot_capture(id,snapshot_id,organization_id)
);
CREATE INDEX migration_metadata_source_page ON migration_metadata_source(plan_id,organization_id,family,capture_sequence,ordinal,id);
CREATE TABLE migration_metadata_mapping (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option')), source_key BYTEA NOT NULL CHECK(octet_length(source_key)=32),
 source_row_id UUID, parent_mapping_id UUID, source_sequence BIGINT NOT NULL, source_ordinal INTEGER NOT NULL,
 element_ordinal INTEGER NOT NULL DEFAULT 0, name_hmac BYTEA,
 qualified BOOLEAN NOT NULL, disposition TEXT NOT NULL DEFAULT 'hold' CHECK(disposition IN ('hold','create_matching','map_existing')),
 target_id UUID, target_field_id UUID, candidate_disposition TEXT, candidate_target_id UUID, candidate_field_id UUID, target_label_hmac BYTEA, dependent_count BIGINT NOT NULL DEFAULT 0,
 added_byte_bound BIGINT NOT NULL DEFAULT 0 CHECK(added_byte_bound>=0), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,kind,source_key), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(source_row_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_source(id,plan_id,import_id,organization_id),
 FOREIGN KEY(parent_mapping_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_mapping(id,plan_id,import_id,organization_id)
);
CREATE INDEX migration_metadata_mapping_unfiltered ON migration_metadata_mapping(plan_id,organization_id,id);
CREATE INDEX migration_metadata_mapping_page ON migration_metadata_mapping(plan_id,organization_id,kind,id);
CREATE INDEX migration_metadata_mapping_name ON migration_metadata_mapping(plan_id,organization_id,kind,name_hmac,id);
CREATE INDEX migration_metadata_mapping_target ON migration_metadata_mapping(plan_id,organization_id,kind,target_id);
CREATE INDEX migration_metadata_mapping_candidate ON migration_metadata_mapping(plan_id,organization_id,kind,candidate_disposition,candidate_target_id,candidate_field_id);
CREATE INDEX migration_metadata_mapping_label ON migration_metadata_mapping(plan_id,organization_id,kind,target_label_hmac,candidate_field_id);
CREATE INDEX migration_metadata_mapping_source ON migration_metadata_mapping(plan_id,organization_id,kind,source_row_id);
CREATE INDEX migration_metadata_mapping_parent ON migration_metadata_mapping(parent_mapping_id,organization_id,element_ordinal,id);
CREATE INDEX migration_metadata_mapping_execute ON migration_metadata_mapping(plan_id,organization_id,kind,source_sequence,source_ordinal,element_ordinal,id);
CREATE TABLE migration_metadata_alias (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 mapping_id UUID NOT NULL, source_row_id UUID NOT NULL, element_ordinal INTEGER NOT NULL,
 alias_key BYTEA NOT NULL CHECK(octet_length(alias_key)=32),
 UNIQUE(plan_id,organization_id,source_row_id,element_ordinal),
 FOREIGN KEY(mapping_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_mapping(id,plan_id,import_id,organization_id),
 FOREIGN KEY(source_row_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_source(id,plan_id,import_id,organization_id)
);
CREATE INDEX migration_metadata_alias_mapping ON migration_metadata_alias(mapping_id,organization_id,id);
CREATE INDEX migration_metadata_alias_exact ON migration_metadata_alias(mapping_id,organization_id,alias_key,id);
CREATE TABLE migration_metadata_manifest (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 source_id TEXT NOT NULL CHECK(length(source_id)<=128), source_row_id UUID NOT NULL, parent_result_id UUID,
 person_id UUID, disposition TEXT NOT NULL CHECK(disposition IN ('eligible','held')),
 added_byte_bound BIGINT NOT NULL CHECK(added_byte_bound>=0), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,source_id), UNIQUE(id,plan_id,import_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(source_row_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_source(id,plan_id,import_id,organization_id),
 FOREIGN KEY(parent_result_id,organization_id) REFERENCES migration_import_result(id,organization_id)
);
CREATE INDEX migration_metadata_manifest_unfiltered ON migration_metadata_manifest(plan_id,organization_id,id);
CREATE INDEX migration_metadata_manifest_page ON migration_metadata_manifest(plan_id,organization_id,disposition,id);
CREATE INDEX migration_metadata_manifest_person ON migration_metadata_manifest(organization_id,person_id,id);
CREATE TABLE migration_metadata_operation (
 id UUID PRIMARY KEY, plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL,
 manifest_id UUID NOT NULL, mapping_id UUID, kind TEXT NOT NULL CHECK(kind IN ('tag_link','value')),
 source_key BYTEA NOT NULL CHECK(octet_length(source_key)=32), target_id UUID,
 disposition TEXT NOT NULL CHECK(disposition IN ('eligible','already_present','held','not_supplied','source_null')),
 UNIQUE(manifest_id,organization_id,kind,source_key),
 FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_manifest(id,plan_id,import_id,organization_id),
 FOREIGN KEY(mapping_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_mapping(id,plan_id,import_id,organization_id)
);
CREATE INDEX migration_metadata_operation_native ON migration_metadata_operation(manifest_id,organization_id,kind,target_id) WHERE disposition='eligible';
CREATE INDEX migration_metadata_operation_mapping ON migration_metadata_operation(mapping_id,organization_id,disposition);
CREATE TABLE migration_metadata_identity (
 organization_id UUID NOT NULL REFERENCES organization(id), source_account_id BIGINT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option')), source_key BYTEA NOT NULL CHECK(octet_length(source_key)=32),
 target_id UUID NOT NULL, import_id UUID NOT NULL, plan_id UUID NOT NULL, mapping_id UUID NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY(organization_id,source_account_id,kind,source_key),
 FOREIGN KEY(mapping_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_mapping(id,plan_id,import_id,organization_id)
 -- No target FK: missing native rows retain identity tombstones.
);
CREATE TABLE migration_metadata_result (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option','people')), mapping_id UUID, manifest_id UUID,
 unit_id UUID NOT NULL, source_id TEXT, person_id UUID,
 disposition TEXT NOT NULL CHECK(disposition IN ('applied','created','already_present','held','not_supplied','source_null')),
 counts JSONB NOT NULL DEFAULT '{}', nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, committed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(import_id,organization_id,unit_id), UNIQUE(id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(mapping_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_mapping(id,plan_id,import_id,organization_id),
 FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_metadata_manifest(id,plan_id,import_id,organization_id),
 CHECK((mapping_id IS NULL)<>(manifest_id IS NULL))
);
CREATE INDEX migration_metadata_result_unfiltered ON migration_metadata_result(import_id,organization_id,id);
CREATE INDEX migration_metadata_result_page ON migration_metadata_result(import_id,organization_id,kind,disposition,id);
CREATE INDEX migration_metadata_result_person ON migration_metadata_result(organization_id,person_id,id) WHERE person_id IS NOT NULL;
CREATE TABLE migration_metadata_receipt (
 organization_id UUID NOT NULL, actor_user_id UUID NOT NULL REFERENCES app_user(id), action TEXT NOT NULL,
 request_id UUID NOT NULL, import_id UUID NOT NULL, input_digest BYTEA NOT NULL CHECK(octet_length(input_digest)=32),
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(organization_id,actor_user_id,action,request_id),
 FOREIGN KEY(import_id,organization_id) REFERENCES migration_metadata_import(id,organization_id)
);
CREATE TABLE migration_metadata_reservation (
 token UUID PRIMARY KEY, import_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 plan_id UUID NOT NULL, lease_token UUID NOT NULL, purpose TEXT NOT NULL CHECK(purpose IN ('work','cancel')),
 byte_count BIGINT NOT NULL CHECK(byte_count>0 AND byte_count<=67108864), expires_at TIMESTAMPTZ NOT NULL,
 FOREIGN KEY(import_id,snapshot_id,organization_id) REFERENCES migration_metadata_import(id,snapshot_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id)
);
CREATE INDEX migration_metadata_reservation_owner ON migration_metadata_reservation(import_id,organization_id,purpose);
CREATE TABLE migration_metadata_issue (
 plan_id UUID NOT NULL, import_id UUID NOT NULL, organization_id UUID NOT NULL, code TEXT NOT NULL,
 record_count BIGINT NOT NULL CHECK(record_count>0), PRIMARY KEY(plan_id,organization_id,code),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_metadata_plan(id,import_id,organization_id)
);
GRANT SELECT,INSERT,UPDATE ON migration_metadata_import,migration_metadata_plan,migration_metadata_source,migration_metadata_mapping,migration_metadata_issue TO crm_app;
GRANT SELECT,INSERT ON migration_metadata_choice,migration_metadata_alias,migration_metadata_manifest,migration_metadata_operation,migration_metadata_identity,migration_metadata_result,migration_metadata_receipt TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_metadata_reservation TO crm_app;
CREATE TRIGGER migration_metadata_receipt_append_only BEFORE UPDATE OR DELETE ON migration_metadata_receipt FOR EACH ROW EXECUTE FUNCTION reject_mutation();

-- A token is not a generic review-write bypass: target IDs must occur in the
-- frozen planned catalog unit or Person operation descriptors. Source text never
-- enters an exception. Application commands additionally validate exact values.
CREATE FUNCTION crm_metadata_insert_allowed(org UUID,table_name TEXT,row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE token TEXT:=current_setting('crm.metadata_token',true); unit_text TEXT:=current_setting('crm.metadata_unit',true); unit UUID; run UUID; plan UUID;
BEGIN
 IF token IS NULL OR token='' OR unit_text IS NULL OR unit_text='' THEN RETURN false; END IF;
 BEGIN unit:=unit_text::uuid; EXCEPTION WHEN invalid_text_representation THEN RETURN false; END;
 SELECT i.id,i.confirmed_plan_id INTO run,plan FROM migration_metadata_import i
 JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id
 JOIN migration_import p ON p.id=i.parent_import_id AND p.organization_id=i.organization_id AND p.confirmed_plan_id=i.parent_plan_id
 JOIN organization o ON o.id=i.organization_id
 JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id
 WHERE i.organization_id=org AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp()
 AND p.state='completed' AND o.workspace_mode='migration_review' AND o.workspace_revision=i.workspace_revision
 AND m.role='admin' AND m.status='active';
 IF run IS NULL THEN RETURN false; END IF;
 IF table_name IN ('tag','custom_field','custom_field_option') THEN
  RETURN EXISTS(SELECT 1 FROM migration_metadata_mapping a WHERE a.plan_id=plan AND a.import_id=run AND a.organization_id=org
   AND a.qualified AND a.disposition='create_matching' AND a.target_id=(row_value->>'id')::uuid
   AND ((table_name='tag' AND a.kind='tag' AND a.id=unit)
    OR (table_name='custom_field' AND a.kind='field' AND a.id=unit)
    OR (table_name='custom_field_option' AND a.kind='option' AND a.target_field_id=(row_value->>'field_id')::uuid
     AND (a.id=unit OR a.parent_mapping_id=unit))));
 END IF;
 IF table_name IN ('person_tag','person_custom_field_value') THEN
  RETURN EXISTS(SELECT 1 FROM migration_metadata_manifest a JOIN migration_metadata_operation x ON x.manifest_id=a.id AND x.organization_id=a.organization_id
   WHERE a.id=unit AND a.plan_id=plan AND a.import_id=run AND a.organization_id=org AND a.person_id=(row_value->>'person_id')::uuid
   AND x.disposition='eligible' AND ((table_name='person_tag' AND x.kind='tag_link' AND x.target_id=(row_value->>'tag_id')::uuid)
    OR(table_name='person_custom_field_value' AND x.kind='value' AND x.target_id=(row_value->>'field_id')::uuid)));
 END IF;
 RETURN false;
END $$;
REVOKE ALL ON FUNCTION crm_metadata_insert_allowed(UUID,TEXT,JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_metadata_insert_allowed(UUID,TEXT,JSONB) TO crm_app;

-- Preserve the original 010c and terminal-call cases verbatim.
CREATE OR REPLACE FUNCTION crm_workspace_mutation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
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
 IF TG_OP='INSERT' AND crm_metadata_insert_allowed(org,TG_TABLE_NAME,row_value) THEN RETURN NEW; END IF;
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
