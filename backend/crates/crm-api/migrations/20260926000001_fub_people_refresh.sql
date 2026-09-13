-- D-076: additive, retained-evidence-only refresh of existing imported People.
-- No 010c immutable plan/result/identity row is updated by this migration.
CREATE TABLE migration_people_refresh (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id),
 parent_import_id UUID NOT NULL, parent_plan_id UUID NOT NULL, report_id UUID NOT NULL,
 source_account_id BIGINT NOT NULL, newer_snapshot_id UUID NOT NULL,
 newer_sequence BIGINT NOT NULL, newer_started_at TIMESTAMPTZ NOT NULL, newer_completed_at TIMESTAMPTZ NOT NULL, workspace_revision BIGINT NOT NULL,
 initiated_by_user_id UUID NOT NULL REFERENCES app_user(id),
 preparation_checkpoint_key TEXT NOT NULL DEFAULT '', preparation_checkpoint_id UUID,
 preparation_phase TEXT NOT NULL DEFAULT 'imported' CHECK(preparation_phase IN ('imported','groups')),
 engine_version TEXT NOT NULL CHECK(engine_version='fub-people-refresh-v1'),
 state TEXT NOT NULL CHECK(state IN ('preparing','ready','queued','running','paused','completed','cancelled')),
 lifecycle_revision BIGINT NOT NULL DEFAULT 1 CHECK(lifecycle_revision>0),
 confirmed_boundary BIGINT, confirmed_snapshot_id UUID, confirmed_started_at TIMESTAMPTZ, confirmed_completed_at TIMESTAMPTZ, confirmed_refresh_plan_id UUID, lease_token UUID, lease_epoch BIGINT NOT NULL DEFAULT 0,
 lease_expires_at TIMESTAMPTZ, pause_reason TEXT, checkpoint_id UUID,
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 settled_items BIGINT NOT NULL DEFAULT 0 CHECK(settled_items>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 completed_at TIMESTAMPTZ, cancelled_at TIMESTAMPTZ,
 UNIQUE(id,organization_id),
 FOREIGN KEY(parent_import_id,organization_id) REFERENCES migration_import(id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id),
 FOREIGN KEY(newer_snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE UNIQUE INDEX migration_people_refresh_one_active ON migration_people_refresh(organization_id,parent_import_id) WHERE state IN ('preparing','ready','queued','running','paused');
CREATE UNIQUE INDEX migration_people_refresh_boundary ON migration_people_refresh(organization_id,parent_import_id,newer_snapshot_id) WHERE state<>'cancelled';
CREATE INDEX migration_people_refresh_claim ON migration_people_refresh(created_at,id) WHERE state IN ('queued','running');
CREATE INDEX migration_people_refresh_list ON migration_people_refresh(organization_id,parent_import_id,created_at DESC,id DESC);

CREATE TABLE migration_people_refresh_plan (
 id UUID PRIMARY KEY, refresh_id UUID NOT NULL, organization_id UUID NOT NULL,
 revision BIGINT NOT NULL CHECK(revision>0), state TEXT NOT NULL CHECK(state IN ('building','ready','superseded','expired')),
 digest BYTEA, inputs_nonce BYTEA NOT NULL, inputs_ciphertext BYTEA NOT NULL,
 eligible_count BIGINT NOT NULL DEFAULT 0, already_current_count BIGINT NOT NULL DEFAULT 0,
 held_count BIGINT NOT NULL DEFAULT 0, excluded_count BIGINT NOT NULL DEFAULT 0,
 name_clear_count BIGINT NOT NULL DEFAULT 0, assignment_clear_count BIGINT NOT NULL DEFAULT 0, contact_removal_count BIGINT NOT NULL DEFAULT 0, no_instruction_count BIGINT NOT NULL DEFAULT 0, prepared_bytes BIGINT NOT NULL DEFAULT 0 CHECK(prepared_bytes>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), sealed_at TIMESTAMPTZ, expires_at TIMESTAMPTZ,
 UNIQUE(refresh_id,organization_id,revision), UNIQUE(id,refresh_id,organization_id),
 FOREIGN KEY(refresh_id,organization_id) REFERENCES migration_people_refresh(id,organization_id),
 CHECK((state IN ('ready','superseded','expired'))=(sealed_at IS NOT NULL AND expires_at IS NOT NULL AND digest IS NOT NULL))
);
CREATE TABLE migration_people_refresh_item (
 id UUID PRIMARY KEY, refresh_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 source_key TEXT NOT NULL CHECK(octet_length(source_key)<=180), source_id TEXT CHECK(source_id ~ '^[1-9][0-9]{0,127}$'), person_id UUID,
 original_result_id UUID, baseline_result_id UUID, disposition TEXT NOT NULL CHECK(disposition IN ('eligible','already_current','held_local_change','held_evidence_gap','held_mapping_gap','held_target_missing','held_original_hold','excluded_source_only','not_seen_again','settled','settled_noop','held_stale','cancelled')),
 proposed_nonce BYTEA NOT NULL, proposed_ciphertext BYTEA NOT NULL,
 baseline_nonce BYTEA NOT NULL, baseline_ciphertext BYTEA NOT NULL,
 current_nonce BYTEA NOT NULL, current_ciphertext BYTEA NOT NULL,
 instructions_nonce BYTEA NOT NULL, instructions_ciphertext BYTEA NOT NULL,
 name_clear_count BIGINT NOT NULL DEFAULT 0 CHECK(name_clear_count>=0), assignment_clear_count BIGINT NOT NULL DEFAULT 0 CHECK(assignment_clear_count>=0), contact_removal_count BIGINT NOT NULL DEFAULT 0 CHECK(contact_removal_count>=0),
 source_capture_id UUID, source_ordinal INTEGER, item_byte_bound BIGINT NOT NULL CHECK(item_byte_bound>0 AND item_byte_bound<=67108864),
 settled_result_id UUID, settled_at TIMESTAMPTZ,
 UNIQUE(plan_id,organization_id,source_key), UNIQUE(id,refresh_id,organization_id),
 FOREIGN KEY(refresh_id,organization_id) REFERENCES migration_people_refresh(id,organization_id),
 FOREIGN KEY(plan_id,refresh_id,organization_id) REFERENCES migration_people_refresh_plan(id,refresh_id,organization_id),
 FOREIGN KEY(original_result_id,organization_id) REFERENCES migration_import_result(id,organization_id),
 CHECK((settled_result_id IS NULL)=(settled_at IS NULL))
);
CREATE INDEX migration_people_refresh_item_page ON migration_people_refresh_item(refresh_id,organization_id,disposition,id);
CREATE INDEX migration_people_refresh_item_claim ON migration_people_refresh_item(refresh_id,organization_id,id) WHERE settled_at IS NULL;
CREATE INDEX migration_people_refresh_item_plan_walk ON migration_people_refresh_item(refresh_id,organization_id,plan_id,id);
CREATE TABLE migration_people_refresh_contact (
 id UUID PRIMARY KEY, item_id UUID NOT NULL, refresh_id UUID NOT NULL, organization_id UUID NOT NULL,
 side TEXT NOT NULL CHECK(side IN ('baseline','current','proposed')), contact_id UUID,
 kind TEXT NOT NULL CHECK(kind IN ('email','phone')), import_order INTEGER NOT NULL CHECK(import_order>=0),
 value_nonce BYTEA NOT NULL, value_ciphertext BYTEA NOT NULL,
 UNIQUE(item_id,side,kind,import_order),
 FOREIGN KEY(item_id,refresh_id,organization_id) REFERENCES migration_people_refresh_item(id,refresh_id,organization_id)
);
CREATE INDEX migration_people_refresh_contact_walk ON migration_people_refresh_contact(item_id,id);
CREATE INDEX migration_people_refresh_contact_side ON migration_people_refresh_contact(item_id,side,contact_id);
CREATE INDEX migration_people_refresh_contact_page ON migration_people_refresh_contact(item_id,side,kind,import_order,id);
CREATE TABLE migration_people_refresh_baseline (
 organization_id UUID NOT NULL, parent_import_id UUID NOT NULL, source_id TEXT NOT NULL,
 person_id UUID NOT NULL, refresh_id UUID NOT NULL, result_id UUID, original_result_id UUID NOT NULL,
 projection_row_id UUID NOT NULL, projection_nonce BYTEA NOT NULL, projection_ciphertext BYTEA NOT NULL,
 updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(organization_id,parent_import_id,source_id),
 FOREIGN KEY(parent_import_id,organization_id) REFERENCES migration_import(id,organization_id),
 FOREIGN KEY(refresh_id,organization_id) REFERENCES migration_people_refresh(id,organization_id),
 FOREIGN KEY(original_result_id,organization_id) REFERENCES migration_import_result(id,organization_id)
);
CREATE UNIQUE INDEX migration_people_refresh_baseline_person ON migration_people_refresh_baseline(organization_id,parent_import_id,person_id);
CREATE TABLE migration_people_refresh_result (
 id UUID PRIMARY KEY, refresh_id UUID NOT NULL, item_id UUID NOT NULL, organization_id UUID NOT NULL,
 person_id UUID, source_id TEXT NOT NULL, disposition TEXT NOT NULL CHECK(disposition IN ('settled','settled_noop','held_stale','held_local_change','held_evidence_gap','held_mapping_gap','held_target_missing','held_original_hold','excluded_source_only','not_seen_again','cancelled')),
 before_nonce BYTEA NOT NULL, before_ciphertext BYTEA NOT NULL, after_nonce BYTEA NOT NULL, after_ciphertext BYTEA NOT NULL,
 actor_user_id UUID NOT NULL REFERENCES app_user(id), committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(refresh_id,item_id), UNIQUE(id,organization_id),
 FOREIGN KEY(refresh_id,organization_id) REFERENCES migration_people_refresh(id,organization_id),
 FOREIGN KEY(item_id,refresh_id,organization_id) REFERENCES migration_people_refresh_item(id,refresh_id,organization_id)
);
CREATE TABLE migration_people_refresh_receipt (
 organization_id UUID NOT NULL, actor_user_id UUID NOT NULL REFERENCES app_user(id), action TEXT NOT NULL CHECK(action IN ('prepare','repreview','confirm','retry','cancel')),
 request_id UUID NOT NULL, refresh_id UUID NOT NULL, digest BYTEA NOT NULL CHECK(octet_length(digest)=32), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(organization_id,actor_user_id,action,request_id), FOREIGN KEY(refresh_id,organization_id) REFERENCES migration_people_refresh(id,organization_id)
);
CREATE INDEX migration_people_refresh_result_walk ON migration_people_refresh_result(refresh_id,organization_id,committed_at,id);
CREATE TABLE migration_people_refresh_reservation (
 token UUID PRIMARY KEY, refresh_id UUID NOT NULL, organization_id UUID NOT NULL, purpose TEXT NOT NULL CHECK(purpose IN ('work','cancel','prepare')),
 lease_token UUID, byte_count BIGINT NOT NULL CHECK(byte_count>0 AND byte_count<=67108864),
 UNIQUE(refresh_id,organization_id,purpose), FOREIGN KEY(refresh_id,organization_id) REFERENCES migration_people_refresh(id,organization_id)
);
GRANT SELECT,INSERT,UPDATE ON migration_people_refresh,migration_people_refresh_plan,migration_people_refresh_item,migration_people_refresh_baseline TO crm_app;
GRANT SELECT,INSERT ON migration_people_refresh_contact,migration_people_refresh_result,migration_people_refresh_receipt TO crm_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON migration_people_refresh_reservation TO crm_app;
-- The workspace trigger remains the authority boundary; these DML privileges
-- permit only the item-scoped private refresh transaction in review mode.
GRANT UPDATE,DELETE ON contact_method TO crm_app;

-- The private permit is intentionally narrower than the 010c insert token. Its
-- identity is the live refresh lease plus immutable parent workspace binding.
CREATE FUNCTION crm_people_refresh_mutation_allowed(org UUID, permit TEXT, table_name TEXT, operation TEXT, row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; person UUID;
BEGIN
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.person_id INTO person FROM migration_people_refresh_item i JOIN migration_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id JOIN migration_workspace w ON w.organization_id=r.organization_id AND w.import_id=r.parent_import_id AND w.plan_id=r.parent_plan_id JOIN organization_membership m ON m.organization_id=r.organization_id AND m.user_id=r.initiated_by_user_id WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND r.state='running' AND r.lease_token=lease AND r.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF person IS NULL THEN RETURN false; END IF;
 IF table_name='person' THEN RETURN operation='UPDATE' AND (row_value->>'id')::uuid=person; END IF;
 IF table_name='contact_method' THEN RETURN (row_value->>'person_id')::uuid=person AND EXISTS(SELECT 1 FROM migration_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.contact_id=(row_value->>'id')::uuid AND ((operation='INSERT' AND c.side='proposed') OR (operation='DELETE' AND c.side IN ('baseline','current')) OR (operation='UPDATE' AND c.side IN ('baseline','current','proposed')))); END IF;
 IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=person AND row_value->>'origin'='migration' AND row_value->>'reason'='migration_refresh' AND (row_value->>'on_behalf_of_user_id')::uuid=(SELECT initiated_by_user_id FROM migration_people_refresh_item i JOIN migration_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id WHERE i.id=unit AND i.organization_id=org); END IF;
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
 token:=current_setting('crm.terminal_call',true);
 IF token IS NOT NULL AND token<>'' AND TG_TABLE_NAME IN ('call','call_completed','contact_attempted','person') THEN
  terminal:=token::uuid; SELECT person_id INTO person FROM call WHERE id=terminal AND organization_id=org;
  IF person IS NOT NULL AND ((TG_TABLE_NAME='call' AND (row_value->>'id')::uuid=terminal AND row_value->>'status' IN ('ended','failed','cancelled')) OR (TG_TABLE_NAME IN ('call_completed','contact_attempted') AND (row_value->>'person_id')::uuid=person) OR (TG_TABLE_NAME='person' AND TG_OP='UPDATE' AND (row_value->>'id')::uuid=person AND (to_jsonb(OLD)-'updated_at'-'last_contact_at')=(to_jsonb(NEW)-'updated_at'-'last_contact_at'))) THEN RETURN COALESCE(NEW,OLD); END IF;
 END IF;
 RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
END $$;
REVOKE ALL ON FUNCTION crm_people_refresh_mutation_allowed(UUID,TEXT,TEXT,TEXT,JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_people_refresh_mutation_allowed(UUID,TEXT,TEXT,TEXT,JSONB) TO crm_app;
