-- D-075: additive retained-only source comparison. No canonical CRM writer.
CREATE TABLE migration_core_change_report (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL, parent_import_id UUID NOT NULL,
 parent_plan_id UUID NOT NULL, baseline_snapshot_id UUID NOT NULL, newer_snapshot_id UUID NOT NULL,
 baseline_sequence BIGINT NOT NULL, newer_sequence BIGINT NOT NULL,
 source_account_id BIGINT NOT NULL, workspace_revision BIGINT NOT NULL,
 initiated_by_user_id UUID NOT NULL REFERENCES app_user(id),
 engine_version TEXT NOT NULL CHECK(engine_version='fub-core-change-v1'),
 tuple_hmac BYTEA NOT NULL CHECK(octet_length(tuple_hmac)=32),
 inputs_nonce BYTEA NOT NULL, inputs_ciphertext BYTEA NOT NULL,
 summary_nonce BYTEA, summary_ciphertext BYTEA,
 state TEXT NOT NULL CHECK(state IN ('queued','running','paused','completed','cancelled')),
 phase TEXT NOT NULL DEFAULT 'capture' CHECK(phase IN ('capture','compare','sealed')),
 pause_reason TEXT, capture_side SMALLINT NOT NULL DEFAULT 0 CHECK(capture_side BETWEEN 0 AND 2),
 capture_checkpoint BIGINT NOT NULL DEFAULT 0, group_checkpoint UUID,
 captures_processed BIGINT NOT NULL DEFAULT 0, observations_processed BIGINT NOT NULL DEFAULT 0,
 groups_compared BIGINT NOT NULL DEFAULT 0,
 baseline_uncertain TEXT[] NOT NULL DEFAULT '{}', newer_uncertain TEXT[] NOT NULL DEFAULT '{}',
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 lease_token UUID, lease_expires_at TIMESTAMPTZ, lease_epoch BIGINT NOT NULL DEFAULT 0, output_revision UUID,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), completed_at TIMESTAMPTZ,
 UNIQUE(id,organization_id),
 FOREIGN KEY(parent_import_id,organization_id) REFERENCES migration_import(id,organization_id),
 FOREIGN KEY(baseline_snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 FOREIGN KEY(newer_snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 CHECK(baseline_snapshot_id<>newer_snapshot_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL)),
 CHECK((output_revision IS NOT NULL)=(state='completed')),
 CHECK((phase='sealed')=(state='completed'))
);
CREATE UNIQUE INDEX migration_core_change_one_active ON migration_core_change_report(organization_id) WHERE state IN ('queued','running','paused');
CREATE UNIQUE INDEX migration_core_change_tuple ON migration_core_change_report(organization_id,parent_import_id,baseline_sequence,newer_snapshot_id,newer_sequence,engine_version) WHERE state<>'cancelled';
CREATE INDEX migration_core_change_claim ON migration_core_change_report(created_at,id) WHERE state IN ('queued','running');
CREATE INDEX migration_core_change_list ON migration_core_change_report(organization_id,parent_import_id,created_at DESC,id DESC);
CREATE TABLE migration_core_change_group (
 id UUID PRIMARY KEY, report_id UUID NOT NULL, organization_id UUID NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('people','users','stages','custom_fields','notes','tasks')),
 source_key TEXT NOT NULL CHECK(octet_length(source_key)<=180), source_id TEXT CHECK(source_id ~ '^[1-9][0-9]{0,127}$'),
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 disposition TEXT CHECK(disposition IN ('unchanged','changed','newly_observed','not_seen_again','unresolved')),
 output_nonce BYTEA, output_ciphertext BYTEA,
 UNIQUE(report_id,organization_id,family,source_key), UNIQUE(id,report_id,organization_id),
 FOREIGN KEY(report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id),
 CHECK((output_nonce IS NULL)=(output_ciphertext IS NULL)),
 CHECK((disposition IS NULL)=(output_nonce IS NULL))
);
CREATE INDEX migration_core_change_group_walk ON migration_core_change_group(report_id,organization_id,id);
CREATE INDEX migration_core_change_group_page ON migration_core_change_group(report_id,organization_id,family,disposition,id);
CREATE INDEX migration_core_change_group_disposition ON migration_core_change_group(report_id,organization_id,disposition,id);
CREATE TABLE migration_core_change_variant (
 group_id UUID NOT NULL, report_id UUID NOT NULL, organization_id UUID NOT NULL,
 side SMALLINT NOT NULL CHECK(side IN (0,1)), representation TEXT NOT NULL,
 semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32), occurrences BIGINT NOT NULL DEFAULT 1,
 PRIMARY KEY(group_id,side,representation,semantic_hmac),
 FOREIGN KEY(group_id,report_id,organization_id) REFERENCES migration_core_change_group(id,report_id,organization_id)
);
CREATE TABLE migration_core_change_note_key (
 report_id UUID NOT NULL, organization_id UUID NOT NULL, side SMALLINT NOT NULL CHECK(side IN (0,1)),
 request_hmac BYTEA NOT NULL CHECK(octet_length(request_hmac)=32), source_id TEXT NOT NULL,
 PRIMARY KEY(report_id,organization_id,side,request_hmac),
 FOREIGN KEY(report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id)
);
CREATE TABLE migration_core_change_receipt (
 organization_id UUID NOT NULL, request_id UUID NOT NULL, report_id UUID NOT NULL,
 actor_user_id UUID NOT NULL REFERENCES app_user(id), operation TEXT NOT NULL,
 digest BYTEA NOT NULL CHECK(octet_length(digest)=32), nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL,
 PRIMARY KEY(organization_id,request_id),
 FOREIGN KEY(report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id)
);
CREATE TABLE migration_core_change_reservation (
 token UUID PRIMARY KEY, report_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind SMALLINT NOT NULL CHECK(kind IN (0,1)), lease_token UUID,
 byte_count BIGINT NOT NULL CHECK(byte_count>0),
 UNIQUE(report_id,organization_id,kind),
 FOREIGN KEY(report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id)
);
-- Logical retained bytes: every persisted variable key, opaque hash and encrypted
-- payload counted once. Fixed-size UUID/numeric/timestamp/index overhead follows
-- the existing migration logical budget convention (not physical disk quotas).
CREATE FUNCTION migration_core_change_bytes(v JSONB, table_name TEXT) RETURNS BIGINT LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE names TEXT[]; name TEXT; total BIGINT:=0;
BEGIN
 CASE table_name
 WHEN 'migration_core_change_report' THEN names:=ARRAY['engine_version','state','phase','pause_reason','baseline_uncertain','newer_uncertain'];
 WHEN 'migration_core_change_group' THEN names:=ARRAY['family','source_key','source_id','disposition'];
 WHEN 'migration_core_change_note_key' THEN names:=ARRAY['source_id'];
 WHEN 'migration_core_change_variant' THEN names:=ARRAY['representation'];
 WHEN 'migration_core_change_receipt' THEN names:=ARRAY['operation'];
 END CASE;
 FOREACH name IN ARRAY names LOOP total:=total+COALESCE(octet_length(v->>name),0); END LOOP;
 names:=CASE table_name
 WHEN 'migration_core_change_report' THEN ARRAY['tuple_hmac','inputs_nonce','inputs_ciphertext','summary_nonce','summary_ciphertext']
 WHEN 'migration_core_change_group' THEN ARRAY['nonce','ciphertext','output_nonce','output_ciphertext']
 WHEN 'migration_core_change_note_key' THEN ARRAY['request_hmac']
 WHEN 'migration_core_change_variant' THEN ARRAY['semantic_hmac']
 ELSE ARRAY['digest','nonce','ciphertext'] END;
 FOREACH name IN ARRAY names LOOP total:=total+COALESCE(octet_length((v->>name)::BYTEA),0); END LOOP;
 RETURN total;
END $$;
CREATE FUNCTION migration_core_change_charge() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE delta BIGINT; owner_id UUID; org UUID; snapshot UUID;
BEGIN
 delta:=migration_core_change_bytes(to_jsonb(NEW),TG_TABLE_NAME)-CASE WHEN TG_OP='UPDATE' THEN migration_core_change_bytes(to_jsonb(OLD),TG_TABLE_NAME) ELSE 0 END;
 owner_id:=(to_jsonb(NEW)->>CASE WHEN TG_TABLE_NAME='migration_core_change_report' THEN 'id' ELSE 'report_id' END)::UUID;
 org:=NEW.organization_id;
 SELECT newer_snapshot_id INTO STRICT snapshot FROM migration_core_change_report WHERE id=owner_id AND organization_id=org;
 UPDATE migration_core_change_report SET retained_bytes=retained_bytes+delta WHERE id=owner_id AND organization_id=org;
 UPDATE migration_snapshot SET retained_bytes=retained_bytes+delta WHERE id=snapshot AND organization_id=org;
 UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+delta WHERE organization_id=org;
 RETURN NEW;
END $$;
CREATE TRIGGER migration_core_change_report_charge AFTER INSERT OR UPDATE OF engine_version,state,phase,pause_reason,baseline_uncertain,newer_uncertain,tuple_hmac,inputs_nonce,inputs_ciphertext,summary_nonce,summary_ciphertext ON migration_core_change_report FOR EACH ROW EXECUTE FUNCTION migration_core_change_charge();
CREATE TRIGGER migration_core_change_group_charge AFTER INSERT OR UPDATE ON migration_core_change_group FOR EACH ROW EXECUTE FUNCTION migration_core_change_charge();
CREATE TRIGGER migration_core_change_variant_charge AFTER INSERT ON migration_core_change_variant FOR EACH ROW EXECUTE FUNCTION migration_core_change_charge();
CREATE TRIGGER migration_core_change_note_charge AFTER INSERT ON migration_core_change_note_key FOR EACH ROW EXECUTE FUNCTION migration_core_change_charge();
CREATE TRIGGER migration_core_change_receipt_charge AFTER INSERT ON migration_core_change_receipt FOR EACH ROW EXECUTE FUNCTION migration_core_change_charge();
CREATE FUNCTION migration_core_change_immutable() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='migration_core_change_report' THEN
   IF ROW(NEW.organization_id,NEW.parent_import_id,NEW.parent_plan_id,NEW.baseline_snapshot_id,NEW.newer_snapshot_id,NEW.baseline_sequence,NEW.newer_sequence,NEW.source_account_id,NEW.workspace_revision,NEW.initiated_by_user_id,NEW.engine_version,NEW.tuple_hmac,NEW.inputs_nonce,NEW.inputs_ciphertext) IS DISTINCT FROM ROW(OLD.organization_id,OLD.parent_import_id,OLD.parent_plan_id,OLD.baseline_snapshot_id,OLD.newer_snapshot_id,OLD.baseline_sequence,OLD.newer_sequence,OLD.source_account_id,OLD.workspace_revision,OLD.initiated_by_user_id,OLD.engine_version,OLD.tuple_hmac,OLD.inputs_nonce,OLD.inputs_ciphertext) THEN RAISE EXCEPTION 'immutable report inputs' USING ERRCODE='23514'; END IF;
   IF OLD.state IN ('completed','cancelled') AND ROW(NEW.state,NEW.phase,NEW.summary_nonce,NEW.summary_ciphertext,NEW.output_revision,NEW.capture_side,NEW.capture_checkpoint,NEW.group_checkpoint,NEW.captures_processed,NEW.observations_processed,NEW.groups_compared) IS DISTINCT FROM ROW(OLD.state,OLD.phase,OLD.summary_nonce,OLD.summary_ciphertext,OLD.output_revision,OLD.capture_side,OLD.capture_checkpoint,OLD.group_checkpoint,OLD.captures_processed,OLD.observations_processed,OLD.groups_compared) THEN RAISE EXCEPTION 'terminal report' USING ERRCODE='23514'; END IF;
 ELSE
   IF EXISTS(SELECT 1 FROM migration_core_change_report WHERE id=NEW.report_id AND organization_id=NEW.organization_id AND state IN ('completed','cancelled')) THEN RAISE EXCEPTION 'sealed report rows' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER migration_core_change_report_immutable BEFORE UPDATE ON migration_core_change_report FOR EACH ROW EXECUTE FUNCTION migration_core_change_immutable();
CREATE TRIGGER migration_core_change_group_immutable BEFORE INSERT OR UPDATE ON migration_core_change_group FOR EACH ROW EXECUTE FUNCTION migration_core_change_immutable();
CREATE TRIGGER migration_core_change_variant_immutable BEFORE INSERT OR UPDATE ON migration_core_change_variant FOR EACH ROW EXECUTE FUNCTION migration_core_change_immutable();
GRANT SELECT,INSERT,UPDATE ON migration_core_change_report,migration_core_change_group,migration_core_change_variant TO crm_app;
GRANT SELECT,INSERT ON migration_core_change_note_key,migration_core_change_receipt TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_core_change_reservation TO crm_app;

GRANT UPDATE(byte_count) ON migration_core_change_reservation TO crm_app;
