-- D-078: retained-evidence admission of core-only People first observed after
-- the sealed original 010c boundary. This is intentionally separate from
-- original import and 010e2 refresh state.
CREATE TABLE migration_people_admission (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id),
 parent_import_id UUID NOT NULL, parent_plan_id UUID NOT NULL, report_id UUID NOT NULL,
 source_account_id BIGINT NOT NULL, original_snapshot_id UUID NOT NULL,
 original_sequence BIGINT NOT NULL, newer_snapshot_id UUID NOT NULL, newer_sequence BIGINT NOT NULL,
 newer_started_at TIMESTAMPTZ NOT NULL, newer_completed_at TIMESTAMPTZ NOT NULL,
 workspace_revision BIGINT NOT NULL, initiated_by_user_id UUID NOT NULL REFERENCES app_user(id),
 engine_version TEXT NOT NULL CHECK(engine_version='fub-people-admission-v1'),
 state TEXT NOT NULL CHECK(state IN ('preparing','ready','queued','running','paused','completed','cancelled')),
 lifecycle_revision BIGINT NOT NULL DEFAULT 1 CHECK(lifecycle_revision>0),
 preparation_checkpoint_key TEXT NOT NULL DEFAULT '', preparation_phase TEXT NOT NULL DEFAULT 'original_people',
 confirmed_boundary BIGINT, confirmed_snapshot_id UUID, confirmed_completed_at TIMESTAMPTZ,
 confirmed_admission_plan_id UUID, lease_token UUID, lease_epoch BIGINT NOT NULL DEFAULT 0,
 lease_expires_at TIMESTAMPTZ, pause_reason TEXT, retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0), settled_items BIGINT NOT NULL DEFAULT 0 CHECK(settled_items>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 completed_at TIMESTAMPTZ, cancelled_at TIMESTAMPTZ,
 UNIQUE(id,organization_id), UNIQUE(id,organization_id,parent_import_id,parent_plan_id),
 FOREIGN KEY(parent_import_id,organization_id) REFERENCES migration_import(id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id),
 FOREIGN KEY(original_snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 FOREIGN KEY(newer_snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL)),
 CHECK(preparation_phase IN ('original_people','newer_people','newer_users','newer_stages','groups','seal'))
);
CREATE UNIQUE INDEX migration_people_admission_one_active ON migration_people_admission(organization_id,parent_import_id) WHERE state IN ('preparing','ready','queued','running','paused');
CREATE INDEX migration_people_admission_claim ON migration_people_admission(created_at,id) WHERE state IN ('queued','running');
CREATE INDEX migration_people_admission_list ON migration_people_admission(organization_id,parent_import_id,created_at DESC,id DESC);
CREATE INDEX migration_people_admission_confirmed_boundary ON migration_people_admission(organization_id,parent_import_id,confirmed_completed_at DESC,created_at DESC,id DESC) WHERE confirmed_admission_plan_id IS NOT NULL;

CREATE TABLE migration_people_admission_plan (
 id UUID PRIMARY KEY, admission_id UUID NOT NULL, organization_id UUID NOT NULL,
 revision BIGINT NOT NULL CHECK(revision>0), state TEXT NOT NULL CHECK(state IN ('building','ready','superseded','expired')),
 inputs_nonce BYTEA NOT NULL, inputs_ciphertext BYTEA NOT NULL, digest BYTEA,
 total_count BIGINT NOT NULL DEFAULT 0, eligible_count BIGINT NOT NULL DEFAULT 0,
 already_imported_count BIGINT NOT NULL DEFAULT 0, already_admitted_count BIGINT NOT NULL DEFAULT 0,
 excluded_original_count BIGINT NOT NULL DEFAULT 0, held_count BIGINT NOT NULL DEFAULT 0,
 intended_contact_count BIGINT NOT NULL DEFAULT 0, prepared_bytes BIGINT NOT NULL DEFAULT 0 CHECK(prepared_bytes>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), sealed_at TIMESTAMPTZ, expires_at TIMESTAMPTZ,
 UNIQUE(admission_id,organization_id,revision), UNIQUE(id,admission_id,organization_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 CHECK((state IN ('ready','superseded','expired'))=(sealed_at IS NOT NULL AND expires_at IS NOT NULL AND digest IS NOT NULL))
);
CREATE TABLE migration_people_admission_item (
 id UUID PRIMARY KEY, admission_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 source_key TEXT NOT NULL CHECK(octet_length(source_key)<=180), source_id TEXT CHECK(source_id ~ '^[1-9][0-9]{0,127}$'),
 prospective_person_id UUID NOT NULL, disposition TEXT NOT NULL CHECK(disposition IN ('eligible','already_imported','already_admitted','excluded_original','held_baseline_gap','held_evidence_gap','held_mapping_gap','held_identity','held_target','settled','cancelled')),
 source_capture_id UUID, source_ordinal INTEGER, stage_mapping_id UUID, assignee_mapping_id UUID,
 projection_nonce BYTEA NOT NULL, projection_ciphertext BYTEA NOT NULL,
 provenance_nonce BYTEA NOT NULL, provenance_ciphertext BYTEA NOT NULL,
 item_byte_bound BIGINT NOT NULL CHECK(item_byte_bound>0 AND item_byte_bound<=67108864),
 settled_result_id UUID, settled_at TIMESTAMPTZ,
 UNIQUE(plan_id,organization_id,source_key), UNIQUE(id,admission_id,organization_id), UNIQUE(id,admission_id,organization_id,prospective_person_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(plan_id,admission_id,organization_id) REFERENCES migration_people_admission_plan(id,admission_id,organization_id),
 CHECK((settled_result_id IS NULL)=(settled_at IS NULL))
);
CREATE INDEX migration_people_admission_item_page ON migration_people_admission_item(admission_id,organization_id,disposition,id);
CREATE INDEX migration_people_admission_item_claim ON migration_people_admission_item(admission_id,organization_id,id) WHERE settled_at IS NULL;
-- Sparse eligible claims must not walk the held/already-seen 25k item set.
CREATE INDEX migration_people_admission_item_eligible_claim ON migration_people_admission_item(admission_id,organization_id,plan_id,id) WHERE disposition='eligible' AND settled_at IS NULL;
-- Preparation advances an exact source-key cursor one bounded descriptor at a
-- time, independent of the total report size.
CREATE INDEX migration_core_change_group_admission_people_keyset ON migration_core_change_group(report_id,organization_id,source_key) WHERE family='people';
CREATE TABLE migration_people_admission_contact (
 id UUID PRIMARY KEY, item_id UUID NOT NULL, admission_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('email','phone')), import_order INTEGER NOT NULL CHECK(import_order>=0),
 value_nonce BYTEA NOT NULL, value_ciphertext BYTEA NOT NULL,
 primary_contact BOOLEAN NOT NULL DEFAULT false,
 UNIQUE(item_id,kind,import_order),
 FOREIGN KEY(item_id,admission_id,organization_id) REFERENCES migration_people_admission_item(id,admission_id,organization_id)
);
CREATE INDEX migration_people_admission_contact_page ON migration_people_admission_contact(item_id,id);

CREATE FUNCTION crm_people_admission_plan_immutable() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state IS DISTINCT FROM NEW.state
     AND NOT ((OLD.state='building' AND NEW.state='ready')
              OR (OLD.state='ready' AND NEW.state IN ('superseded','expired'))) THEN
    RAISE EXCEPTION 'invalid admission plan state transition' USING ERRCODE='P0001';
  END IF;
  IF OLD.inputs_nonce IS DISTINCT FROM NEW.inputs_nonce
     OR OLD.inputs_ciphertext IS DISTINCT FROM NEW.inputs_ciphertext
     OR (OLD.state <> 'building' AND (OLD.digest IS DISTINCT FROM NEW.digest
         OR OLD.total_count IS DISTINCT FROM NEW.total_count OR OLD.eligible_count IS DISTINCT FROM NEW.eligible_count
         OR OLD.already_imported_count IS DISTINCT FROM NEW.already_imported_count OR OLD.already_admitted_count IS DISTINCT FROM NEW.already_admitted_count
         OR OLD.excluded_original_count IS DISTINCT FROM NEW.excluded_original_count OR OLD.held_count IS DISTINCT FROM NEW.held_count
         OR OLD.intended_contact_count IS DISTINCT FROM NEW.intended_contact_count OR OLD.prepared_bytes IS DISTINCT FROM NEW.prepared_bytes)) THEN
    RAISE EXCEPTION 'admission plan immutable' USING ERRCODE='P0001';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER migration_people_admission_plan_immutable BEFORE UPDATE ON migration_people_admission_plan FOR EACH ROW EXECUTE FUNCTION crm_people_admission_plan_immutable();

CREATE FUNCTION crm_people_admission_item_immutable() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF (to_jsonb(NEW) - ARRAY['disposition','settled_result_id','settled_at'])
       IS DISTINCT FROM (to_jsonb(OLD) - ARRAY['disposition','settled_result_id','settled_at']) THEN
    RAISE EXCEPTION 'admission item immutable' USING ERRCODE='P0001';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER migration_people_admission_item_immutable BEFORE UPDATE ON migration_people_admission_item FOR EACH ROW EXECUTE FUNCTION crm_people_admission_item_immutable();
CREATE FUNCTION crm_people_admission_item_building() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM migration_people_admission_plan p WHERE p.id=NEW.plan_id AND p.admission_id=NEW.admission_id AND p.organization_id=NEW.organization_id AND p.state='building') THEN
    RAISE EXCEPTION 'admission item plan is sealed' USING ERRCODE='P0001';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER migration_people_admission_item_building BEFORE INSERT ON migration_people_admission_item FOR EACH ROW EXECUTE FUNCTION crm_people_admission_item_building();
CREATE FUNCTION crm_people_admission_contact_building() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM migration_people_admission_item i JOIN migration_people_admission_plan p ON p.id=i.plan_id AND p.admission_id=i.admission_id AND p.organization_id=i.organization_id WHERE i.id=NEW.item_id AND i.admission_id=NEW.admission_id AND i.organization_id=NEW.organization_id AND p.state='building') THEN
    RAISE EXCEPTION 'admission contact plan is sealed' USING ERRCODE='P0001';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER migration_people_admission_contact_building BEFORE INSERT ON migration_people_admission_contact FOR EACH ROW EXECUTE FUNCTION crm_people_admission_contact_building();
CREATE TABLE migration_people_admission_result (
 id UUID PRIMARY KEY, admission_id UUID NOT NULL, item_id UUID NOT NULL, organization_id UUID NOT NULL,
 person_id UUID, source_id TEXT NOT NULL, disposition TEXT NOT NULL CHECK(disposition IN ('settled','held_baseline_gap','held_evidence_gap','held_mapping_gap','held_identity','held_target','cancelled')),
 actor_user_id UUID NOT NULL REFERENCES app_user(id), committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(admission_id,item_id), UNIQUE(id,organization_id), UNIQUE(id,admission_id,item_id,organization_id,person_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(item_id,admission_id,organization_id) REFERENCES migration_people_admission_item(id,admission_id,organization_id)
);
CREATE INDEX migration_people_admission_result_walk ON migration_people_admission_result(admission_id,organization_id,committed_at,id);
CREATE TABLE migration_people_admission_receipt (
 organization_id UUID NOT NULL, actor_user_id UUID NOT NULL REFERENCES app_user(id),
 action TEXT NOT NULL CHECK(action IN ('prepare','repreview','confirm','retry','cancel')), request_id UUID NOT NULL,
 admission_id UUID NOT NULL, digest BYTEA NOT NULL CHECK(octet_length(digest)=32), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(organization_id,actor_user_id,action,request_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id)
);
CREATE TABLE migration_people_admission_reservation (
 token UUID PRIMARY KEY, admission_id UUID NOT NULL, organization_id UUID NOT NULL,
 purpose TEXT NOT NULL CHECK(purpose IN ('work','cancel','prepare')), lease_token UUID,
 byte_count BIGINT NOT NULL CHECK(byte_count>0 AND byte_count<=67108864),
 UNIQUE(admission_id,organization_id,purpose),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id)
);

CREATE TABLE person_admission_provenance (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id), person_id UUID NOT NULL,
 admission_id UUID NOT NULL, item_id UUID NOT NULL, result_id UUID NOT NULL,
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(organization_id,person_id), UNIQUE(id,organization_id),
 FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(item_id,admission_id,organization_id) REFERENCES migration_people_admission_item(id,admission_id,organization_id),
 FOREIGN KEY(result_id,admission_id,item_id,organization_id,person_id) REFERENCES migration_people_admission_result(id,admission_id,item_id,organization_id,person_id)
);
CREATE TABLE person_admitted (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id), actor_kind TEXT NOT NULL CHECK(actor_kind='system'),
 actor_user_id UUID CHECK(actor_user_id IS NULL), on_behalf_of_user_id UUID NOT NULL REFERENCES app_user(id), origin TEXT NOT NULL CHECK(origin='migration'),
 occurred_at TIMESTAMPTZ NOT NULL, recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), correlation_id UUID NOT NULL,
 person_id UUID NOT NULL, admission_id UUID NOT NULL, plan_id UUID NOT NULL, item_id UUID NOT NULL, result_id UUID NOT NULL,
 UNIQUE(organization_id,admission_id,person_id),
 FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(plan_id,admission_id,organization_id) REFERENCES migration_people_admission_plan(id,admission_id,organization_id),
 FOREIGN KEY(item_id,admission_id,organization_id) REFERENCES migration_people_admission_item(id,admission_id,organization_id),
 FOREIGN KEY(result_id,admission_id,item_id,organization_id,person_id) REFERENCES migration_people_admission_result(id,admission_id,item_id,organization_id,person_id)
);
CREATE INDEX person_admitted_history ON person_admitted(organization_id,person_id,occurred_at,id);
CREATE TRIGGER person_admitted_append_only BEFORE UPDATE OR DELETE ON person_admitted FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER person_admitted_no_truncate BEFORE TRUNCATE ON person_admitted FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

ALTER TABLE migration_import_identity ADD COLUMN admission_id UUID, ADD COLUMN admission_item_id UUID, ADD COLUMN admission_result_id UUID;
ALTER TABLE migration_import_identity ADD CONSTRAINT migration_import_identity_admission_fk FOREIGN KEY(admission_id,organization_id,import_id,plan_id) REFERENCES migration_people_admission(id,organization_id,parent_import_id,parent_plan_id);
ALTER TABLE migration_import_identity ADD CONSTRAINT migration_import_identity_admission_item_fk FOREIGN KEY(admission_item_id,admission_id,organization_id,target_id) REFERENCES migration_people_admission_item(id,admission_id,organization_id,prospective_person_id);
ALTER TABLE migration_import_identity ADD CONSTRAINT migration_import_identity_admission_result_fk FOREIGN KEY(admission_result_id,admission_id,admission_item_id,organization_id,target_id) REFERENCES migration_people_admission_result(id,admission_id,item_id,organization_id,person_id);
ALTER TABLE migration_import_identity ADD CONSTRAINT migration_import_identity_origin CHECK(
 (admission_id IS NULL AND admission_item_id IS NULL AND admission_result_id IS NULL) OR
 (family='people' AND manifest_id IS NULL AND mapping_id IS NULL AND admission_id IS NOT NULL AND admission_item_id IS NOT NULL AND admission_result_id IS NOT NULL)
);

GRANT SELECT,INSERT,UPDATE ON migration_people_admission,migration_people_admission_plan,migration_people_admission_item TO crm_app;
GRANT SELECT,INSERT ON migration_people_admission_contact,migration_people_admission_result,migration_people_admission_receipt,person_admission_provenance,person_admitted TO crm_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON migration_people_admission_reservation TO crm_app;

-- crm_app must lock the immutable mapped target through this narrow definer
-- helper; granting UPDATE on stage would permit unrelated legacy writes.
CREATE FUNCTION crm_people_admission_lock_stage(org UUID, target UUID) RETURNS BOOLEAN
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
  PERFORM 1 FROM stage WHERE id=target AND organization_id=org FOR SHARE;
  RETURN FOUND;
END $$;
REVOKE ALL ON FUNCTION crm_people_admission_lock_stage(UUID,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_people_admission_lock_stage(UUID,UUID) TO crm_app;

CREATE FUNCTION crm_people_admission_mutation_allowed(org UUID, permit TEXT, table_name TEXT, operation TEXT, row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; target UUID;
BEGIN
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.prospective_person_id INTO target FROM migration_people_admission_item i JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=i.organization_id JOIN migration_workspace w ON w.organization_id=a.organization_id AND w.import_id=a.parent_import_id AND w.plan_id=a.parent_plan_id JOIN organization_membership m ON m.organization_id=a.organization_id AND m.user_id=a.initiated_by_user_id WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND a.state='running' AND a.confirmed_admission_plan_id=i.plan_id AND EXISTS(SELECT 1 FROM organization o WHERE o.id=a.organization_id AND o.workspace_revision=a.workspace_revision) AND a.lease_token=lease AND a.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF target IS NULL THEN RETURN false; END IF;
 IF table_name='person' THEN RETURN operation='INSERT' AND (row_value->>'id')::uuid=target; END IF;
 IF table_name='contact_method' THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=target AND EXISTS(SELECT 1 FROM migration_people_admission_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.id=(row_value->>'id')::uuid AND c.kind=row_value->>'kind'); END IF;
 IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=target AND row_value->>'origin'='migration' AND row_value->>'reason'='migration_admission'; END IF;
 IF table_name='person_admitted' THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=target AND (row_value->>'item_id')::uuid=unit; END IF;
 RETURN false;
END $$;

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
 IF token IS NOT NULL AND TG_TABLE_NAME IN ('person','contact_method','stage','assignment_changed','stage_changed','person_imported') AND (TG_TABLE_NAME<>'contact_method' OR TG_OP='INSERT') AND EXISTS(SELECT 1 FROM migration_workspace w JOIN migration_import i ON i.id=w.import_id AND i.organization_id=w.organization_id JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id WHERE w.organization_id=org AND w.plan_id=i.confirmed_plan_id AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active') THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.people_refresh_permit',true);
 IF TG_TABLE_NAME IN ('person','contact_method','assignment_changed','stage_changed') AND crm_people_refresh_mutation_allowed(org,token,TG_TABLE_NAME,TG_OP,row_value) THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.people_admission_permit',true);
 IF TG_TABLE_NAME IN ('person','contact_method','assignment_changed','stage_changed','person_admitted') AND crm_people_admission_mutation_allowed(org,token,TG_TABLE_NAME,TG_OP,row_value) THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.terminal_call',true);
 IF token IS NOT NULL AND token<>'' AND TG_TABLE_NAME IN ('call','call_completed','contact_attempted','person') THEN terminal:=token::uuid; SELECT person_id INTO person FROM call WHERE id=terminal AND organization_id=org; IF person IS NOT NULL AND ((TG_TABLE_NAME='call' AND (row_value->>'id')::uuid=terminal AND row_value->>'status' IN ('ended','failed','cancelled')) OR (TG_TABLE_NAME IN ('call_completed','contact_attempted') AND (row_value->>'person_id')::uuid=person) OR (TG_TABLE_NAME='person' AND TG_OP='UPDATE' AND (row_value->>'id')::uuid=person AND (to_jsonb(OLD)-'updated_at'-'last_contact_at')=(to_jsonb(NEW)-'updated_at'-'last_contact_at'))) THEN RETURN COALESCE(NEW,OLD); END IF; END IF;
 RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
END $$;
REVOKE ALL ON FUNCTION crm_people_admission_mutation_allowed(UUID,TEXT,TEXT,TEXT,JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_people_admission_mutation_allowed(UUID,TEXT,TEXT,TEXT,JSONB) TO crm_app;

-- New admitted People alone need the appended history kind. Legacy review-state
-- rows are never backfilled; the history reader treats its absent key as zero.
CREATE FUNCTION crm_person_admitted_prepare_history() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 UPDATE migration_history_review_state SET counts=counts||jsonb_build_object('person_admitted',0)
 WHERE organization_id=NEW.organization_id AND person_id=NEW.person_id AND NOT counts ? 'person_admitted';
 RETURN NEW;
END $$;
CREATE TRIGGER person_admitted_prepare_history BEFORE INSERT ON person_admitted FOR EACH ROW EXECUTE FUNCTION crm_person_admitted_prepare_history();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON person_admitted FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
-- This fact table was added after the original guarded-table inventory.
CREATE TRIGGER workspace_write_guard BEFORE INSERT OR UPDATE OR DELETE ON person_admitted FOR EACH ROW EXECUTE FUNCTION crm_workspace_mutation_guard();
