-- D-082: first metadata coverage for a terminal admitted-People cohort.
-- Original metadata identities remain immutable evidence; this migration adds a
-- separate child and the claim registry used by both catalog writers.
CREATE TABLE migration_metadata_catalog_readiness (
 organization_id UUID PRIMARY KEY REFERENCES organization(id),
 state TEXT NOT NULL CHECK(state IN ('inactive','ready')) DEFAULT 'inactive',
 activated_at TIMESTAMPTZ, activated_by_user_id UUID REFERENCES app_user(id),
 engine_version TEXT, CHECK((state='ready')=(activated_at IS NOT NULL AND engine_version='fub-admitted-metadata-v1'))
);
CREATE TABLE migration_metadata_catalog_claim (
 organization_id UUID NOT NULL REFERENCES organization(id), source_account_id BIGINT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option')), source_key BYTEA NOT NULL CHECK(octet_length(source_key)=32),
 target_id UUID NOT NULL, original_import_id UUID, original_plan_id UUID, original_mapping_id UUID,
 admitted_import_id UUID, admitted_plan_id UUID, admitted_mapping_id UUID,
 evidence_nonce BYTEA NOT NULL, evidence_ciphertext BYTEA NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(organization_id,source_account_id,kind,source_key),
 CHECK(((original_import_id IS NOT NULL)::integer + (admitted_import_id IS NOT NULL)::integer)=1)
);
CREATE TABLE migration_admitted_metadata_import (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id),
 parent_import_id UUID NOT NULL, parent_plan_id UUID NOT NULL, admission_id UUID NOT NULL,
 predecessor_import_id UUID, successor_import_id UUID, source_report_id UUID NOT NULL, snapshot_id UUID NOT NULL,
 source_account_id BIGINT NOT NULL, capture_sequence BIGINT NOT NULL, workspace_revision BIGINT NOT NULL,
 executor_user_id UUID NOT NULL REFERENCES app_user(id), engine_version TEXT NOT NULL CHECK(engine_version='fub-admitted-metadata-v1'),
 state TEXT NOT NULL CHECK(state IN ('proposed','queued','running','paused','completed','cancelled','expired')),
 phase TEXT NOT NULL CHECK(phase IN ('preparation','catalog','people','complete')) DEFAULT 'preparation',
 latest_plan_id UUID, confirmed_plan_id UUID, lease_token UUID, lease_expires_at TIMESTAMPTZ,
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0), reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,organization_id), UNIQUE(organization_id,admission_id), UNIQUE(predecessor_import_id), UNIQUE(successor_import_id),
 FOREIGN KEY(parent_import_id,organization_id) REFERENCES migration_import(id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(predecessor_import_id,organization_id) REFERENCES migration_admitted_metadata_import(id,organization_id),
 FOREIGN KEY(source_report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id),
 FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE INDEX migration_admitted_metadata_import_page ON migration_admitted_metadata_import(organization_id,created_at DESC,id DESC);
CREATE INDEX migration_admitted_metadata_import_claim ON migration_admitted_metadata_import(organization_id,id) WHERE state IN ('queued','running');
CREATE TABLE migration_admitted_metadata_plan (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, organization_id UUID NOT NULL, revision BIGINT NOT NULL CHECK(revision>0),
 state TEXT NOT NULL CHECK(state IN ('building','ready','paused','failed','superseded')), phase TEXT NOT NULL DEFAULT 'preparation',
 digest BYTEA, expires_at TIMESTAMPTZ, inputs_nonce BYTEA NOT NULL, inputs_ciphertext BYTEA NOT NULL,
 counts JSONB NOT NULL DEFAULT '{}'::jsonb, max_added_byte_bound BIGINT NOT NULL DEFAULT 0 CHECK(max_added_byte_bound BETWEEN 0 AND 67108864),
 UNIQUE(import_id,organization_id,revision), UNIQUE(id,import_id,organization_id),
 FOREIGN KEY(import_id,organization_id) REFERENCES migration_admitted_metadata_import(id,organization_id)
);
ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT migration_admitted_metadata_import_latest_plan_fk FOREIGN KEY(latest_plan_id,id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id);
ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT migration_admitted_metadata_import_confirmed_plan_fk FOREIGN KEY(confirmed_plan_id,id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id);
CREATE TABLE migration_admitted_metadata_manifest (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 admission_result_id UUID NOT NULL, person_id UUID NOT NULL, source_person_id TEXT NOT NULL,
 disposition TEXT NOT NULL CHECK(disposition IN ('eligible','settled','held','cancelled')),
 baseline_nonce BYTEA NOT NULL, baseline_ciphertext BYTEA NOT NULL, item_byte_bound BIGINT NOT NULL CHECK(item_byte_bound BETWEEN 1 AND 67108864),
 settled_at TIMESTAMPTZ, UNIQUE(plan_id,organization_id,source_person_id),
 FOREIGN KEY(import_id,organization_id) REFERENCES migration_admitted_metadata_import(id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id)
);
CREATE TABLE migration_admitted_metadata_mapping (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option')), source_key BYTEA NOT NULL CHECK(octet_length(source_key)=32),
 target_id UUID, disposition TEXT NOT NULL CHECK(disposition IN ('eligible','create_matching','map_existing','held')),
 UNIQUE(plan_id,organization_id,kind,source_key), UNIQUE(id,plan_id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id)
);
CREATE TABLE migration_admitted_metadata_source (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('people','custom_fields')), source_id TEXT NOT NULL,
 semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 UNIQUE(plan_id,organization_id,family,source_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id)
);
CREATE TABLE migration_admitted_metadata_operation (
 id UUID PRIMARY KEY, manifest_id UUID NOT NULL, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag_link','value')), target_id UUID NOT NULL, disposition TEXT NOT NULL CHECK(disposition IN ('eligible','held','not_supplied','source_null')),
 UNIQUE(manifest_id,organization_id,kind,target_id),
 FOREIGN KEY(manifest_id) REFERENCES migration_admitted_metadata_manifest(id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id)
);
CREATE TABLE migration_admitted_metadata_result (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, manifest_id UUID, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option','people')), disposition TEXT NOT NULL CHECK(disposition IN ('applied','created','already_present','held','not_supplied','source_null')),
 person_id UUID, nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(import_id,organization_id,kind,manifest_id),
 FOREIGN KEY(import_id,organization_id) REFERENCES migration_admitted_metadata_import(id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(manifest_id) REFERENCES migration_admitted_metadata_manifest(id),
 FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id)
);
CREATE TABLE migration_admitted_metadata_receipt (
 organization_id UUID NOT NULL, actor_user_id UUID NOT NULL REFERENCES app_user(id), action TEXT NOT NULL,
 request_id UUID NOT NULL, import_id UUID NOT NULL, digest BYTEA NOT NULL CHECK(octet_length(digest)=32), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY(organization_id,actor_user_id,action,request_id),
 FOREIGN KEY(import_id,organization_id) REFERENCES migration_admitted_metadata_import(id,organization_id)
);
CREATE TABLE migration_admitted_metadata_reservation (
 token UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 lease_token UUID, purpose TEXT NOT NULL CHECK(purpose IN ('prepare','work','cancel')), byte_count BIGINT NOT NULL CHECK(byte_count BETWEEN 1 AND 67108864),
 UNIQUE(import_id,organization_id,purpose), FOREIGN KEY(import_id,organization_id) REFERENCES migration_admitted_metadata_import(id,organization_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id), FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id)
);
CREATE TABLE migration_admitted_metadata_issue (
 import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL, code TEXT NOT NULL, count BIGINT NOT NULL CHECK(count>0),
 PRIMARY KEY(plan_id,organization_id,code), FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id)
);
GRANT SELECT,INSERT,UPDATE ON migration_metadata_catalog_readiness,migration_metadata_catalog_claim,migration_admitted_metadata_import,migration_admitted_metadata_plan,migration_admitted_metadata_manifest TO crm_app;
GRANT SELECT,INSERT ON migration_admitted_metadata_mapping,migration_admitted_metadata_source,migration_admitted_metadata_operation,migration_admitted_metadata_result,migration_admitted_metadata_receipt,migration_admitted_metadata_issue TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_admitted_metadata_reservation TO crm_app;

-- This replacement is the old-binary fence.  Existing metadata workers acquire
-- `crm.metadata_token`, but cannot manufacture the v1 claim proof introduced by
-- the handover.  Once readiness is durable they therefore fail at the native
-- write boundary, including a lease acquired before readiness.  A compatible
-- writer supplies a JSON proof in `crm.metadata_claim_v1`; its exact claim is
-- rechecked by the typed writer before the catalog/result transaction commits.
CREATE OR REPLACE FUNCTION crm_metadata_insert_allowed(org UUID,table_name TEXT,row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE token TEXT:=current_setting('crm.metadata_token',true); unit_text TEXT:=current_setting('crm.metadata_unit',true); unit UUID; run UUID; plan UUID; proof TEXT:=current_setting('crm.metadata_claim_v1',true);
BEGIN
 IF token IS NULL OR token='' OR unit_text IS NULL OR unit_text='' THEN RETURN false; END IF;
 IF EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=org AND state='ready')
    AND (proof IS NULL OR proof='' OR jsonb_typeof(proof::jsonb)<>'object') THEN RETURN false; END IF;
 BEGIN unit:=unit_text::uuid; EXCEPTION WHEN invalid_text_representation THEN RETURN false; END;
 SELECT i.id,i.confirmed_plan_id INTO run,plan FROM migration_metadata_import i
 JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id
 JOIN migration_import p ON p.id=i.parent_import_id AND p.organization_id=i.organization_id AND p.confirmed_plan_id=i.parent_plan_id
 JOIN organization o ON o.id=i.organization_id JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id
 WHERE i.organization_id=org AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp() AND p.state='completed' AND o.workspace_mode='migration_review' AND o.workspace_revision=i.workspace_revision AND m.role='admin' AND m.status='active';
 IF run IS NULL THEN RETURN false; END IF;
 IF EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=org AND state='ready')
    AND NOT (proof::jsonb @> jsonb_build_object('organization_id',org::text,'root_id',run::text,'plan_id',plan::text,'lease',token,'unit_id',unit::text,'target_id',CASE WHEN table_name IN ('tag','custom_field') THEN row_value->>'id' WHEN table_name='custom_field_option' THEN row_value->>'id' WHEN table_name='person_tag' THEN row_value->>'tag_id' WHEN table_name='person_custom_field_value' THEN row_value->>'field_id' ELSE row_value->>'target_id' END)) THEN RETURN false; END IF;
 IF table_name IN ('tag','custom_field','custom_field_option') THEN RETURN EXISTS(SELECT 1 FROM migration_metadata_mapping a WHERE a.plan_id=plan AND a.import_id=run AND a.organization_id=org AND a.qualified AND a.disposition='create_matching' AND a.target_id=(row_value->>'id')::uuid AND ((table_name='tag' AND a.kind='tag' AND a.id=unit) OR (table_name='custom_field' AND a.kind='field' AND a.id=unit) OR (table_name='custom_field_option' AND a.kind='option' AND a.target_field_id=(row_value->>'field_id')::uuid AND (a.id=unit OR a.parent_mapping_id=unit)))); END IF;
 IF table_name IN ('person_tag','person_custom_field_value') THEN RETURN EXISTS(SELECT 1 FROM migration_metadata_manifest a JOIN migration_metadata_operation x ON x.manifest_id=a.id AND x.organization_id=a.organization_id WHERE a.id=unit AND a.plan_id=plan AND a.import_id=run AND a.organization_id=org AND a.person_id=(row_value->>'person_id')::uuid AND x.disposition='eligible' AND ((table_name='person_tag' AND x.kind='tag_link' AND x.target_id=(row_value->>'tag_id')::uuid) OR(table_name='person_custom_field_value' AND x.kind='value' AND x.target_id=(row_value->>'field_id')::uuid))); END IF;
 IF table_name='migration_metadata_identity' THEN RETURN EXISTS(SELECT 1 FROM migration_metadata_mapping a WHERE a.id=unit AND a.plan_id=plan AND a.import_id=run AND a.organization_id=org AND a.target_id=(row_value->>'target_id')::uuid AND a.kind=row_value->>'kind' AND a.source_key=decode(replace(row_value->>'source_key','\\x',''),'hex')); END IF;
 RETURN false;
END $$;

CREATE FUNCTION crm_metadata_identity_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_user='crm_app' AND NOT crm_metadata_insert_allowed(NEW.organization_id,'migration_metadata_identity',to_jsonb(NEW)) THEN RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='metadata_identity_permit_required'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER migration_metadata_identity_claim_guard BEFORE INSERT ON migration_metadata_identity FOR EACH ROW EXECUTE FUNCTION crm_metadata_identity_guard();

-- Admitted workers use an independent permit.  The proof binds the exact
-- Organization/account/root/plan/lease/unit/native target/claim before a
-- workspace trigger accepts an INSERT; a valid JSON blob alone is never enough.
CREATE FUNCTION crm_admitted_metadata_insert_allowed(org UUID, permit TEXT, row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; root UUID; plan UUID; account BIGINT; target UUID; claim_kind TEXT; claim_key TEXT;
BEGIN
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'unit_id')::uuid; root:=(permit::jsonb->>'root_id')::uuid; plan:=(permit::jsonb->>'plan_id')::uuid; target:=(permit::jsonb->>'target_id')::uuid; claim_kind:=permit::jsonb->>'claim_kind'; claim_key:=permit::jsonb->>'claim_key'; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.source_account_id INTO account FROM migration_admitted_metadata_import i JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id JOIN organization o ON o.id=i.organization_id AND o.workspace_revision=i.workspace_revision JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id WHERE i.id=root AND i.organization_id=org AND i.confirmed_plan_id=plan AND i.state='running' AND i.lease_token=lease AND i.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF account IS NULL OR claim_key IS NULL OR claim_kind IS NULL THEN RETURN false; END IF;
 IF EXISTS(SELECT 1 FROM migration_admitted_metadata_mapping x WHERE x.id=unit AND x.import_id=root AND x.plan_id=plan AND x.organization_id=org AND x.kind=claim_kind AND x.target_id=target AND encode(x.source_key,'hex')=claim_key AND x.disposition='create_matching') THEN RETURN (row_value->>'id')::uuid=target; END IF;
 RETURN EXISTS(SELECT 1 FROM migration_admitted_metadata_manifest m JOIN migration_admitted_metadata_operation x ON x.manifest_id=m.id AND x.plan_id=plan AND x.organization_id=org WHERE m.id=unit AND m.import_id=root AND m.plan_id=plan AND m.organization_id=org AND m.disposition='eligible' AND x.target_id=target AND x.disposition='eligible' AND (row_value->>'person_id')::uuid=m.person_id AND ((x.kind='tag_link' AND (row_value->>'tag_id')::uuid=target) OR (x.kind='value' AND (row_value->>'field_id')::uuid=target)));
END $$;
REVOKE ALL ON FUNCTION crm_admitted_metadata_insert_allowed(UUID,TEXT,JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_admitted_metadata_insert_allowed(UUID,TEXT,JSONB) TO crm_app;

-- Preserve every existing review-workspace exception and add only the private
-- admitted-metadata INSERT permit.  The Mobile005 details_revision allowance
-- remains in crm_admitted_people_refresh_mutation_allowed (20261001); this
-- guard deliberately does not replace that predicate.
CREATE OR REPLACE FUNCTION crm_workspace_mutation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE org UUID; row_value JSONB; token TEXT; terminal UUID; person UUID;
BEGIN
 IF current_user<>'crm_app' THEN RETURN COALESCE(NEW,OLD); END IF;
 row_value:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END; org:=(row_value->>'organization_id')::uuid;
 IF TG_TABLE_NAME='operator_task_proposal' THEN SELECT organization_id INTO org FROM operator_proposal WHERE id=(row_value->>'proposal_id')::uuid; END IF;
 IF org IS NULL THEN RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_scope_required'; END IF;
 PERFORM crm_workspace_shared(org); IF EXISTS(SELECT 1 FROM organization WHERE id=org AND workspace_mode='operational') THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.admitted_metadata_permit',true);
 IF TG_OP='INSERT' AND crm_admitted_metadata_insert_allowed(org,token,row_value) THEN RETURN NEW; END IF;
 IF TG_OP='INSERT' AND (crm_metadata_insert_allowed(org,TG_TABLE_NAME,row_value) OR crm_activity_insert_allowed(org,TG_TABLE_NAME,row_value)) THEN RETURN NEW; END IF;
 token:=current_setting('crm.import_token',true);
 IF token IS NOT NULL AND TG_TABLE_NAME IN ('person','contact_method','stage','assignment_changed','stage_changed','person_imported') AND (TG_TABLE_NAME<>'contact_method' OR TG_OP='INSERT') AND EXISTS(SELECT 1 FROM migration_workspace w JOIN migration_import i ON i.id=w.import_id AND i.organization_id=w.organization_id JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id WHERE w.organization_id=org AND w.plan_id=i.confirmed_plan_id AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active') THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.people_refresh_permit',true); IF TG_TABLE_NAME IN ('person','contact_method','assignment_changed','stage_changed') AND crm_people_refresh_mutation_allowed(org,token,TG_TABLE_NAME,TG_OP,row_value) THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.people_admission_permit',true); IF TG_TABLE_NAME IN ('person','contact_method','assignment_changed','stage_changed','person_admitted') AND crm_people_admission_mutation_allowed(org,token,TG_TABLE_NAME,TG_OP,row_value) THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.admitted_people_refresh_permit',true); IF TG_TABLE_NAME IN ('person','contact_method','assignment_changed','stage_changed') AND crm_admitted_people_refresh_mutation_allowed(org,token,TG_TABLE_NAME,TG_OP,CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END,CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END) THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.terminal_call',true); IF token IS NOT NULL AND token<>'' AND TG_TABLE_NAME IN ('call','call_completed','contact_attempted','person') THEN terminal:=token::uuid; SELECT person_id INTO person FROM call WHERE id=terminal AND organization_id=org; IF person IS NOT NULL AND ((TG_TABLE_NAME='call' AND (row_value->>'id')::uuid=terminal AND row_value->>'status' IN ('ended','failed','cancelled')) OR (TG_TABLE_NAME IN ('call_completed','contact_attempted') AND (row_value->>'person_id')::uuid=person) OR (TG_TABLE_NAME='person' AND TG_OP='UPDATE' AND (to_jsonb(OLD)-'updated_at'-'last_contact_at')=(to_jsonb(NEW)-'updated_at'-'last_contact_at'))) THEN RETURN COALESCE(NEW,OLD); END IF; END IF;
 RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
END $$;
