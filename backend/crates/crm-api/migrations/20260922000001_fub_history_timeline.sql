-- D-072: retained-only imported observations, independent from native truth.
CREATE TABLE migration_history_import_run (
 id UUID PRIMARY KEY,organization_id UUID NOT NULL REFERENCES migration_snapshot_storage(organization_id),
 plan_id UUID NOT NULL,parent_import_id UUID NOT NULL,executor_user_id UUID NOT NULL REFERENCES app_user(id),
 state TEXT NOT NULL CHECK(state IN ('preparing','ready','queued','running','paused','completed','cancelled')),
 phase TEXT NOT NULL CHECK(phase IN ('capture','classify','apply','finished')),
 revision BIGINT NOT NULL DEFAULT 1,confirmed_at TIMESTAMPTZ,completed_at TIMESTAMPTZ,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(),updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 pause_reason TEXT CHECK(pause_reason IN ('retained_integrity_failed','source_binding_changed','executor_not_authorized','release_not_ready','storage_limit','interpretation_bound_exceeded')),
 applied_position BIGINT NOT NULL DEFAULT 0,processed BIGINT NOT NULL DEFAULT 0,
 inserted BIGINT NOT NULL DEFAULT 0,already_imported BIGINT NOT NULL DEFAULT 0,application_held BIGINT NOT NULL DEFAULT 0,
 run_byte_limit BIGINT NOT NULL CHECK(run_byte_limit>0),budget_revision BIGINT NOT NULL DEFAULT 1,
 budget_policy_revision TEXT NOT NULL CHECK(octet_length(budget_policy_revision)<=80),
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 lease_token UUID,lease_expires_at TIMESTAMPTZ,admitted_at TIMESTAMPTZ,
 UNIQUE(id,organization_id),CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE UNIQUE INDEX migration_history_import_one_active ON migration_history_import_run(organization_id,parent_import_id)
 WHERE state IN ('preparing','ready','queued','running','paused');
CREATE INDEX migration_history_import_claim ON migration_history_import_run(created_at,id) WHERE state IN ('preparing','queued','running');
CREATE INDEX migration_history_import_list ON migration_history_import_run(organization_id,created_at DESC,id DESC);
CREATE INDEX migration_history_import_parent_list ON migration_history_import_run(organization_id,parent_import_id,created_at DESC,id DESC);
CREATE TABLE migration_history_import_plan (
 id UUID PRIMARY KEY,organization_id UUID NOT NULL,owner_run_id UUID NOT NULL,
 parent_import_id UUID NOT NULL,parent_plan_id UUID NOT NULL,snapshot_id UUID NOT NULL,
 parent_capture_sequence BIGINT NOT NULL,workspace_revision BIGINT NOT NULL,
 capture_id UUID NOT NULL,capture_revision BIGINT NOT NULL,capture_sequence BIGINT NOT NULL,
 source_account_id BIGINT NOT NULL,source_access_user_id BIGINT NOT NULL,
 profile_version TEXT NOT NULL CHECK(octet_length(profile_version)<=64),
 parser_version TEXT NOT NULL CHECK(octet_length(parser_version)<=64),schema_version TEXT NOT NULL CHECK(octet_length(schema_version)<=128),
 interpretation_version TEXT NOT NULL CHECK(octet_length(interpretation_version)<=80),
 reader_version TEXT NOT NULL CHECK(octet_length(reader_version)<=80),
 binding_hmac BYTEA NOT NULL CHECK(octet_length(binding_hmac)=32),
 state TEXT NOT NULL CHECK(state IN ('building','ready')),revision BIGINT NOT NULL DEFAULT 1,
 ready_at TIMESTAMPTZ,expires_at TIMESTAMPTZ,last_capture_sequence BIGINT NOT NULL DEFAULT 0,
 page_ordinal INTEGER NOT NULL DEFAULT 0 CHECK(page_ordinal BETWEEN 0 AND 99),classified_position BIGINT NOT NULL DEFAULT 0,
 occurrences BIGINT NOT NULL DEFAULT 0,eligible BIGINT NOT NULL DEFAULT 0,equal_repeats BIGINT NOT NULL DEFAULT 0,held BIGINT NOT NULL DEFAULT 0,
 added_byte_bound BIGINT NOT NULL DEFAULT 0,coverage JSONB NOT NULL CHECK(octet_length(coverage::text)<=4096),
 UNIQUE(id,organization_id),
 FOREIGN KEY(owner_run_id,organization_id) REFERENCES migration_history_import_run(id,organization_id) DEFERRABLE INITIALLY DEFERRED,
 FOREIGN KEY(parent_import_id,snapshot_id,organization_id) REFERENCES migration_import(id,snapshot_id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(capture_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id)
);
ALTER TABLE migration_history_import_run ADD FOREIGN KEY(plan_id,organization_id) REFERENCES migration_history_import_plan(id,organization_id) DEFERRABLE INITIALLY DEFERRED;
CREATE TABLE migration_history_import_anchor (
 organization_id UUID NOT NULL,parent_import_id UUID NOT NULL,plan_id UUID NOT NULL,
 interpretation_version TEXT NOT NULL,reader_version TEXT NOT NULL,confirmed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(organization_id,parent_import_id),
 FOREIGN KEY(plan_id,organization_id) REFERENCES migration_history_import_plan(id,organization_id)
);
CREATE TABLE migration_history_import_stream (
 plan_id UUID NOT NULL,organization_id UUID NOT NULL,owner_run_id UUID NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('events','calls','text_messages')),checkpoint BIGINT NOT NULL DEFAULT 0,
 reported_total TEXT NOT NULL CHECK(reported_total ~ '^(0|[1-9][0-9]{0,18})$'),finished BOOLEAN NOT NULL DEFAULT false,
 nonce BYTEA CHECK(octet_length(nonce)=24),ciphertext BYTEA CHECK(octet_length(ciphertext)<=8192),
 PRIMARY KEY(plan_id,organization_id,family),FOREIGN KEY(plan_id,organization_id) REFERENCES migration_history_import_plan(id,organization_id)
);
CREATE TABLE migration_history_import_manifest (
 id UUID PRIMARY KEY,organization_id UUID NOT NULL,plan_id UUID NOT NULL,owner_run_id UUID NOT NULL,
 position BIGINT NOT NULL,capture_id UUID NOT NULL,observation_id UUID NOT NULL,ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 99),
 family TEXT NOT NULL CHECK(family IN ('events','calls','text_messages')),representation TEXT NOT NULL CHECK(octet_length(representation)<=128),
 identity_hmac BYTEA CHECK(octet_length(identity_hmac)=32),semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 person_id UUID,source_created_at TIMESTAMPTZ,
 disposition TEXT NOT NULL CHECK(disposition IN ('pending','eligible','equal_repeat','held')),
 reason TEXT CHECK(reason IN ('invalid_identity','ambiguous_relationship','parent_excluded','no_parent_identity','parent_erased','conflicting_variants','identity_erased','identity_conflict')),
 UNIQUE(id,plan_id,organization_id),UNIQUE(plan_id,position),UNIQUE(plan_id,observation_id),
 FOREIGN KEY(plan_id,organization_id) REFERENCES migration_history_import_plan(id,organization_id)
);
CREATE INDEX migration_history_import_manifest_page ON migration_history_import_manifest(plan_id,organization_id,position);
CREATE INDEX migration_history_import_manifest_filter ON migration_history_import_manifest(plan_id,organization_id,family,disposition,position);
CREATE INDEX migration_history_import_manifest_person ON migration_history_import_manifest(organization_id,person_id,id);
CREATE INDEX migration_history_import_manifest_identity ON migration_history_import_manifest(organization_id,identity_hmac,plan_id);
CREATE TABLE migration_history_import_candidate (
 plan_id UUID NOT NULL,organization_id UUID NOT NULL,owner_run_id UUID NOT NULL,
 identity_hmac BYTEA NOT NULL CHECK(octet_length(identity_hmac)=32),semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 first_manifest_id UUID NOT NULL,person_id UUID,occurrences BIGINT NOT NULL DEFAULT 1,conflicting BOOLEAN NOT NULL DEFAULT false,
 PRIMARY KEY(plan_id,organization_id,identity_hmac),FOREIGN KEY(plan_id,organization_id) REFERENCES migration_history_import_plan(id,organization_id)
);
CREATE TABLE migration_history_import_display (
 id UUID PRIMARY KEY,organization_id UUID NOT NULL,plan_id UUID NOT NULL,owner_run_id UUID NOT NULL,
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=4112),
 FOREIGN KEY(id,plan_id,organization_id) REFERENCES migration_history_import_manifest(id,plan_id,organization_id)
);
CREATE TABLE migration_history_import_identity (
 id UUID PRIMARY KEY,organization_id UUID NOT NULL,owner_run_id UUID NOT NULL,
 identity_hmac BYTEA NOT NULL CHECK(octet_length(identity_hmac)=32),semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 person_id UUID,fact_id UUID,family TEXT NOT NULL CHECK(family IN ('events','calls','text_messages')),erased_at TIMESTAMPTZ,
 UNIQUE(organization_id,identity_hmac),UNIQUE(id,organization_id),
 FOREIGN KEY(owner_run_id,organization_id) REFERENCES migration_history_import_run(id,organization_id)
);
CREATE INDEX migration_history_import_identity_person ON migration_history_import_identity(organization_id,person_id);
CREATE TABLE migration_history_import_result (
 id UUID PRIMARY KEY,organization_id UUID NOT NULL,plan_id UUID NOT NULL,owner_run_id UUID NOT NULL,
 manifest_id UUID NOT NULL,position BIGINT NOT NULL,family TEXT NOT NULL CHECK(family IN ('events','calls','text_messages')),
 disposition TEXT NOT NULL CHECK(disposition IN ('imported','already_imported','equal_repeat','held')),
 reason TEXT CHECK(reason IN ('invalid_identity','ambiguous_relationship','parent_excluded','no_parent_identity','parent_erased','conflicting_variants','identity_erased','identity_conflict')),
 fact_id UUID,UNIQUE(owner_run_id,manifest_id),
 FOREIGN KEY(manifest_id,plan_id,organization_id) REFERENCES migration_history_import_manifest(id,plan_id,organization_id),
 FOREIGN KEY(owner_run_id,organization_id) REFERENCES migration_history_import_run(id,organization_id)
);
CREATE INDEX migration_history_import_result_page ON migration_history_import_result(owner_run_id,organization_id,position);
CREATE INDEX migration_history_import_result_filter ON migration_history_import_result(owner_run_id,organization_id,family,disposition,position);
CREATE TABLE migration_history_import_reservation (
 token UUID PRIMARY KEY,organization_id UUID NOT NULL,run_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('control','unit')),byte_count BIGINT NOT NULL CHECK(byte_count>0),expires_at TIMESTAMPTZ,
 UNIQUE(run_id,organization_id,kind),FOREIGN KEY(run_id,organization_id) REFERENCES migration_history_import_run(id,organization_id)
);
CREATE TABLE migration_history_import_receipt (
 organization_id UUID NOT NULL,request_id UUID NOT NULL,owner_run_id UUID NOT NULL,actor_user_id UUID NOT NULL,
 operation TEXT NOT NULL CHECK(operation IN ('prepare','confirm','resume','cancel','budget')),
 digest BYTEA NOT NULL CHECK(octet_length(digest)=32),nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),
 ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=4112),PRIMARY KEY(organization_id,request_id),
 FOREIGN KEY(owner_run_id,organization_id) REFERENCES migration_history_import_run(id,organization_id)
);
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['fub_event_record_imported','fub_call_record_imported','fub_text_record_imported'] LOOP
  EXECUTE format('CREATE TABLE %I (
   id UUID PRIMARY KEY,organization_id UUID NOT NULL REFERENCES organization(id),
   actor_kind TEXT NOT NULL CHECK(actor_kind IN (''user'',''system'')),actor_user_id UUID REFERENCES app_user(id),
   on_behalf_of_user_id UUID REFERENCES app_user(id),origin TEXT NOT NULL CHECK(origin=''migration''),
   occurred_at TIMESTAMPTZ NOT NULL,recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
   correlation_id UUID NOT NULL,causation_id UUID,corrects_id UUID REFERENCES %I(id),
   person_id UUID NOT NULL,plan_id UUID NOT NULL,attempt_id UUID NOT NULL,manifest_id UUID NOT NULL,identity_id UUID NOT NULL,
   source_created_at TIMESTAMPTZ,source_time_basis TEXT NOT NULL CHECK(source_time_basis IN (''fub_record_created'',''unknown'')),stable_position BIGINT NOT NULL,
   CHECK((actor_kind=''user'')=(actor_user_id IS NOT NULL)),
   CHECK((source_created_at IS NULL)=(source_time_basis=''unknown'')),
   UNIQUE(organization_id,identity_id),
   FOREIGN KEY(manifest_id,plan_id,organization_id) REFERENCES migration_history_import_manifest(id,plan_id,organization_id),
   FOREIGN KEY(identity_id,organization_id) REFERENCES migration_history_import_identity(id,organization_id),
   FOREIGN KEY(attempt_id,organization_id) REFERENCES migration_history_import_run(id,organization_id))',name,name);
  EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_mutation()',name||'_immutable',name);
  EXECUTE format('GRANT SELECT,INSERT ON %I TO crm_app',name);
 END LOOP;
END $$;
CREATE FUNCTION crm_history_import_frozen() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p UUID; o UUID;
BEGIN
 p:=(to_jsonb(OLD)->>CASE WHEN TG_TABLE_NAME='migration_history_import_plan' THEN 'id' ELSE 'plan_id' END)::uuid;
 o:=OLD.organization_id;
 IF EXISTS(SELECT 1 FROM migration_history_import_plan WHERE id=p AND organization_id=o AND state='ready') THEN
  RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_plan_immutable';
 END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['migration_history_import_plan','migration_history_import_manifest','migration_history_import_candidate','migration_history_import_stream'] LOOP
  EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_history_import_frozen()',name||'_frozen',name);
 END LOOP;
 FOREACH name IN ARRAY ARRAY['migration_history_import_anchor','migration_history_import_result','migration_history_import_receipt'] LOOP
  EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_mutation()',name||'_immutable',name);
 END LOOP;
END $$;
CREATE FUNCTION crm_history_import_identity_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (to_jsonb(NEW)-'erased_at') IS DISTINCT FROM (to_jsonb(OLD)-'erased_at') OR OLD.erased_at IS NOT NULL OR NEW.erased_at IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_identity_immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER migration_history_import_identity_immutable BEFORE UPDATE OR DELETE ON migration_history_import_identity FOR EACH ROW EXECUTE FUNCTION crm_history_import_identity_immutable();

-- Exact logical variable-byte inventory, independent of fixed tuple/index size.
CREATE FUNCTION crm_history_import_retained_size(v JSONB) RETURNS BIGINT LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE total BIGINT:=0;k TEXT;value TEXT;
BEGIN
 IF v IS NULL THEN RETURN 0;END IF;
 FOR k,value IN SELECT e.key,e.value #>> '{}' FROM jsonb_each(v) e LOOP
  IF value IS NULL THEN CONTINUE;END IF;
  IF k IN ('nonce','ciphertext','identity_hmac','semantic_hmac','digest','binding_hmac') THEN total:=total+octet_length(decode(substring(value FROM 3),'hex'));
  ELSIF k IN ('profile_version','parser_version','schema_version','interpretation_version','reader_version','budget_policy_revision','representation','reported_total','coverage') THEN total:=total+octet_length(value);
  END IF;
 END LOOP;
 RETURN total;
END $$;
CREATE FUNCTION crm_history_import_measure() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE delta BIGINT;owner_id UUID;org UUID;v JSONB;
BEGIN
 delta:=crm_history_import_retained_size(CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)-crm_history_import_retained_size(CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta=0 THEN RETURN COALESCE(NEW,OLD);END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD));org:=(v->>'organization_id')::uuid;
 IF TG_TABLE_NAME='migration_history_import_anchor' THEN
  SELECT owner_run_id INTO STRICT owner_id FROM migration_history_import_plan WHERE id=(v->>'plan_id')::uuid AND organization_id=org;
 ELSE owner_id:=(v->>CASE WHEN TG_TABLE_NAME='migration_history_import_run' THEN 'id' ELSE 'owner_run_id' END)::uuid;END IF;
 UPDATE migration_history_import_run SET retained_bytes=retained_bytes+delta WHERE id=owner_id AND organization_id=org;
 UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+delta WHERE organization_id=org;
 RETURN COALESCE(NEW,OLD);
END $$;
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['migration_history_import_run','migration_history_import_plan','migration_history_import_anchor','migration_history_import_stream','migration_history_import_manifest','migration_history_import_candidate','migration_history_import_display','migration_history_import_identity','migration_history_import_result','migration_history_import_receipt'] LOOP
  EXECUTE format('CREATE TRIGGER %I AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_history_import_measure()',name||'_measure',name);
 END LOOP;
END $$;
CREATE OR REPLACE FUNCTION crm_workspace_read(org UUID,actor UUID,operational_only BOOLEAN) RETURNS void LANGUAGE plpgsql AS $$
DECLARE mode TEXT;member_role TEXT;
BEGIN
 PERFORM crm_workspace_shared(org);
 SELECT o.workspace_mode,m.role INTO mode,member_role FROM organization o JOIN organization_membership m ON m.organization_id=o.id WHERE o.id=org AND m.user_id=actor AND m.status='active';
 IF mode IS NULL THEN RAISE EXCEPTION USING ERRCODE='P010A',MESSAGE='forbidden';END IF;
 IF mode<>'operational' AND (operational_only OR member_role<>'admin') THEN RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';END IF;
 IF EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=org) AND current_setting('crm.history_reader',true) IS DISTINCT FROM 'fub-history-timeline-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_reader_required';
 END IF;
END $$;
CREATE FUNCTION crm_history_complete_read(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM crm_workspace_shared(org);
 IF EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=org) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_review_required';END IF;
END $$;
GRANT SELECT,INSERT,UPDATE ON migration_history_import_run,migration_history_import_plan,migration_history_import_stream,migration_history_import_manifest,migration_history_import_candidate TO crm_app;
GRANT SELECT,INSERT ON migration_history_import_anchor,migration_history_import_result,migration_history_import_receipt TO crm_app;
GRANT SELECT,INSERT,UPDATE ON migration_history_import_identity TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_history_import_display,migration_history_import_reservation TO crm_app;
REVOKE ALL ON FUNCTION crm_history_complete_read(UUID),crm_history_import_retained_size(JSONB),crm_history_import_measure(),crm_history_import_frozen(),crm_history_import_identity_immutable() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_history_complete_read(UUID),crm_history_import_retained_size(JSONB) TO crm_app;

-- Native review revision/count state and bounded-reader indexes.
-- External visibility/count/erasure triggers remain import owner's responsibility.
CREATE TABLE migration_history_review_state(organization_id UUID NOT NULL REFERENCES organization(id),person_id UUID NOT NULL REFERENCES person(id) ON DELETE CASCADE,revision BIGINT NOT NULL DEFAULT 0 CHECK(revision>=0),counts JSONB NOT NULL DEFAULT '{"inquiries":0,"notes":0,"open_tasks":0,"completed_tasks":0,"person_imported":0,"inquiry_received":0,"routing_decision":0,"assignment_changed":0,"stage_changed":0,"contact_attempted":0,"call_completed":0,"correspondence":0,"fub_event_record_imported_known":0,"fub_event_record_imported_unknown":0,"fub_call_record_imported_known":0,"fub_call_record_imported_unknown":0,"fub_text_record_imported_known":0,"fub_text_record_imported_unknown":0}'::jsonb,PRIMARY KEY(organization_id,person_id));
GRANT SELECT ON migration_history_review_state TO crm_app;
INSERT INTO migration_history_review_state(organization_id,person_id) SELECT organization_id,id FROM person;
CREATE FUNCTION crm_history_review_delta(o UUID,p UUID,k TEXT,d BIGINT) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF p IS NULL THEN RETURN; END IF;
 UPDATE migration_history_review_state SET revision=revision+1,
 counts=CASE WHEN k IS NULL THEN counts ELSE jsonb_set(counts,ARRAY[k],to_jsonb((counts->>k)::bigint+d)) END
 WHERE organization_id=o AND person_id=p;
END $$;
REVOKE ALL ON FUNCTION crm_history_review_delta(UUID,UUID,TEXT,BIGINT) FROM PUBLIC;
CREATE FUNCTION crm_history_review_person() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF TG_OP='INSERT' THEN INSERT INTO migration_history_review_state(organization_id,person_id) VALUES(NEW.organization_id,NEW.id);
 ELSE PERFORM crm_history_review_delta(NEW.organization_id,NEW.id,NULL,0); END IF; RETURN NEW;
END $$;
CREATE TRIGGER history_review_person AFTER INSERT OR UPDATE ON person FOR EACH ROW EXECUTE FUNCTION crm_history_review_person();
CREATE FUNCTION crm_history_review_rows() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE r JSONB; k TEXT; sign BIGINT;
BEGIN
 FOR r,sign IN SELECT to_jsonb(OLD),-1::bigint WHERE TG_OP<>'INSERT' UNION ALL SELECT to_jsonb(NEW),1::bigint WHERE TG_OP<>'DELETE' LOOP
  k=CASE TG_TABLE_NAME WHEN 'inquiry' THEN 'inquiries' WHEN 'note' THEN CASE WHEN r->>'deleted_at' IS NULL THEN 'notes' END
    WHEN 'task' THEN CASE WHEN r->>'deleted_at' IS NULL THEN CASE WHEN r->>'completed_at' IS NULL THEN 'open_tasks' ELSE 'completed_tasks' END END
    WHEN 'correspondence_captured' THEN 'correspondence' ELSE TG_TABLE_NAME END;
  PERFORM crm_history_review_delta((r->>'organization_id')::uuid,(r->>'person_id')::uuid,k,sign);
 END LOOP; RETURN NULL;
END $$;
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON person_imported FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON inquiry_received FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON routing_decision FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON assignment_changed FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON stage_changed FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON contact_attempted FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON call_completed FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON correspondence_captured FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON inquiry FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON note FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
CREATE TRIGGER history_review_rows AFTER INSERT OR UPDATE OR DELETE ON task FOR EACH ROW EXECUTE FUNCTION crm_history_review_rows();
WITH all_rows AS (SELECT organization_id,person_id,'person_imported' AS kind FROM person_imported UNION ALL SELECT organization_id,person_id,'inquiry_received' AS kind FROM inquiry_received UNION ALL SELECT organization_id,person_id,'routing_decision' AS kind FROM routing_decision UNION ALL SELECT organization_id,person_id,'assignment_changed' AS kind FROM assignment_changed UNION ALL SELECT organization_id,person_id,'stage_changed' AS kind FROM stage_changed UNION ALL SELECT organization_id,person_id,'contact_attempted' AS kind FROM contact_attempted UNION ALL SELECT organization_id,person_id,'call_completed' AS kind FROM call_completed UNION ALL SELECT organization_id,person_id,'correspondence' AS kind FROM correspondence_captured UNION ALL SELECT organization_id,person_id,'inquiries' AS kind FROM inquiry UNION ALL SELECT organization_id,person_id,'notes' AS kind FROM note WHERE deleted_at IS NULL UNION ALL SELECT organization_id,person_id,CASE WHEN completed_at IS NULL THEN 'open_tasks' ELSE 'completed_tasks' END AS kind FROM task WHERE deleted_at IS NULL), grouped AS (SELECT organization_id,person_id,kind,count(*) AS n FROM all_rows WHERE person_id IS NOT NULL GROUP BY organization_id,person_id,kind), objects AS(SELECT organization_id,person_id,jsonb_object_agg(kind,n) AS v FROM grouped GROUP BY organization_id,person_id) UPDATE migration_history_review_state s SET counts=s.counts||o.v FROM objects o WHERE s.organization_id=o.organization_id AND s.person_id=o.person_id;
CREATE FUNCTION crm_history_review_core_rows() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF TG_OP<>'INSERT' THEN PERFORM crm_history_review_delta(OLD.organization_id,OLD.person_id,NULL,0); END IF;
 IF TG_OP<>'DELETE' THEN PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,NULL,0); END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER history_review_core_rows AFTER INSERT OR UPDATE OR DELETE ON contact_method FOR EACH ROW EXECUTE FUNCTION crm_history_review_core_rows();
CREATE TRIGGER history_review_core_rows AFTER INSERT OR UPDATE OR DELETE ON person_tag FOR EACH ROW EXECUTE FUNCTION crm_history_review_core_rows();
CREATE TRIGGER history_review_core_rows AFTER INSERT OR UPDATE OR DELETE ON person_custom_field_value FOR EACH ROW EXECUTE FUNCTION crm_history_review_core_rows();
CREATE FUNCTION crm_history_review_user_label() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$ BEGIN IF OLD.display_name IS DISTINCT FROM NEW.display_name THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,id AS person_id FROM person WHERE assigned_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM inquiry_received WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM routing_decision WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM assignment_changed WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM stage_changed WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM contact_attempted WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM call_completed WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM routing_decision WHERE assignee_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM assignment_changed WHERE from_user_id=NEW.id OR to_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM correspondence_captured WHERE on_behalf_of_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_event_record_imported WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_call_record_imported WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_text_record_imported WHERE actor_user_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $$;
CREATE TRIGGER history_review_user_label AFTER UPDATE ON app_user FOR EACH ROW EXECUTE FUNCTION crm_history_review_user_label();
CREATE FUNCTION crm_history_review_stage_label() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$ BEGIN IF OLD.name IS DISTINCT FROM NEW.name THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,id AS person_id FROM person WHERE stage_id=NEW.id UNION SELECT organization_id,person_id FROM stage_changed WHERE from_stage_id=NEW.id OR to_stage_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $$;
CREATE TRIGGER history_review_label AFTER UPDATE ON stage FOR EACH ROW EXECUTE FUNCTION crm_history_review_stage_label();
CREATE FUNCTION crm_history_review_tag_label() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$ BEGIN IF OLD.name IS DISTINCT FROM NEW.name THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,person_id FROM person_tag WHERE tag_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $$;
CREATE TRIGGER history_review_label AFTER UPDATE ON tag FOR EACH ROW EXECUTE FUNCTION crm_history_review_tag_label();
CREATE FUNCTION crm_history_review_custom_field_label() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$ BEGIN IF OLD IS DISTINCT FROM NEW THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,person_id FROM person_custom_field_value WHERE field_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $$;
CREATE TRIGGER history_review_label AFTER UPDATE ON custom_field FOR EACH ROW EXECUTE FUNCTION crm_history_review_custom_field_label();
CREATE FUNCTION crm_history_review_custom_field_option_label() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$ BEGIN IF OLD IS DISTINCT FROM NEW THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,person_id FROM person_custom_field_value WHERE option_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $$;
CREATE TRIGGER history_review_label AFTER UPDATE ON custom_field_option FOR EACH ROW EXECUTE FUNCTION crm_history_review_custom_field_option_label();
-- call existence is rendered as contact_attempted.metadata.call_id; insertion/deletion
-- or changing the joined identifier must invalidate affected Person page series.
CREATE FUNCTION crm_history_review_call_link() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE r JSONB;
BEGIN FOR r IN SELECT to_jsonb(OLD) WHERE TG_OP<>'INSERT' UNION ALL SELECT to_jsonb(NEW) WHERE TG_OP<>'DELETE' LOOP
 UPDATE migration_history_review_state s SET revision=revision+1 FROM contact_attempted c WHERE c.organization_id=(r->>'organization_id')::uuid AND c.causation_id=(r->>'id')::uuid AND s.organization_id=c.organization_id AND s.person_id=c.person_id;
 END LOOP; RETURN NULL; END $$;
CREATE TRIGGER history_review_call_link AFTER INSERT OR UPDATE OR DELETE ON call FOR EACH ROW EXECUTE FUNCTION crm_history_review_call_link();
CREATE INDEX person_imported_history_review_page ON person_imported(organization_id,person_id,occurred_at DESC,recorded_at DESC,id DESC);
CREATE INDEX inquiry_received_history_review_page ON inquiry_received(organization_id,person_id,occurred_at DESC,recorded_at DESC,id DESC);
CREATE INDEX routing_decision_history_review_page ON routing_decision(organization_id,person_id,occurred_at DESC,recorded_at DESC,id DESC);
CREATE INDEX assignment_changed_history_review_page ON assignment_changed(organization_id,person_id,occurred_at DESC,recorded_at DESC,id DESC);
CREATE INDEX stage_changed_history_review_page ON stage_changed(organization_id,person_id,occurred_at DESC,recorded_at DESC,id DESC);
CREATE INDEX contact_attempted_history_review_page ON contact_attempted(organization_id,person_id,(CASE WHEN corrects_id IS NOT NULL THEN recorded_at ELSE occurred_at END) DESC,recorded_at DESC,id DESC);
CREATE INDEX call_completed_history_review_page ON call_completed(organization_id,person_id,occurred_at DESC,recorded_at DESC,id DESC);
CREATE INDEX correspondence_captured_history_review_page ON correspondence_captured(organization_id,person_id,occurred_at DESC,recorded_at DESC,id DESC);
CREATE INDEX inquiry_history_review_page ON inquiry(organization_id,person_id,received_at DESC,id DESC);
CREATE INDEX contact_method_history_review_primary ON contact_method(organization_id,person_id,kind,import_order ASC NULLS LAST,created_at,id);
-- Import owner adds external known/unknown indexes, initial zero counts, fact insertion
-- and visibility deltas for identity erasure or display removal. Updates to visible
-- display metadata bump revision even if counts do not change. Suppressed rows never count.

CREATE FUNCTION crm_history_import_fact_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM migration_history_import_run r
  JOIN migration_history_import_anchor a ON a.organization_id=r.organization_id AND a.parent_import_id=r.parent_import_id AND a.plan_id=r.plan_id
  JOIN migration_history_import_manifest m ON m.organization_id=r.organization_id AND m.plan_id=r.plan_id AND m.id=NEW.manifest_id AND m.disposition='eligible'
  JOIN migration_history_import_identity i ON i.organization_id=m.organization_id AND i.identity_hmac=m.identity_hmac AND i.id=NEW.identity_id AND i.erased_at IS NULL AND i.fact_id=NEW.id
  JOIN migration_history_import_display d ON d.id=m.id AND d.organization_id=m.organization_id AND d.plan_id=m.plan_id
  JOIN person p ON p.id=m.person_id AND p.organization_id=m.organization_id
  JOIN organization_membership member ON member.organization_id=r.organization_id AND member.user_id=r.executor_user_id AND member.status='active' AND member.role='admin'
  JOIN organization o ON o.id=r.organization_id AND o.workspace_mode='migration_review'
  WHERE r.id=NEW.attempt_id AND r.organization_id=NEW.organization_id AND r.plan_id=NEW.plan_id AND m.person_id=NEW.person_id
   AND r.state='running' AND r.lease_expires_at>clock_timestamp() AND r.lease_token::text=current_setting('crm.history_import_token',true)
   AND r.executor_user_id=NEW.actor_user_id AND NEW.actor_kind='user' AND NEW.on_behalf_of_user_id IS NULL
   AND NEW.corrects_id IS NULL AND NEW.source_created_at IS NOT DISTINCT FROM m.source_created_at AND NEW.stable_position=m.position
   AND a.reader_version='fub-history-timeline-v1' AND a.interpretation_version='fub-history-interpretation-v1'
   AND m.family=CASE TG_TABLE_NAME WHEN 'fub_event_record_imported' THEN 'events' WHEN 'fub_call_record_imported' THEN 'calls' ELSE 'text_messages' END)
 THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_import_permit_required';END IF;
 RETURN NEW;
END $$;
CREATE FUNCTION crm_history_import_fact_count() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,TG_TABLE_NAME||CASE WHEN NEW.source_created_at IS NULL THEN '_unknown' ELSE '_known' END,1);
 RETURN NEW;
END $$;
DO $$ DECLARE name TEXT;BEGIN
 FOREACH name IN ARRAY ARRAY['fub_event_record_imported','fub_call_record_imported','fub_text_record_imported'] LOOP
  EXECUTE format('CREATE TRIGGER history_import_guard BEFORE INSERT ON %I FOR EACH ROW EXECUTE FUNCTION crm_history_import_fact_guard()',name);
  EXECUTE format('CREATE TRIGGER history_import_count AFTER INSERT ON %I FOR EACH ROW EXECUTE FUNCTION crm_history_import_fact_count()',name);
  EXECUTE format('CREATE INDEX %I ON %I(organization_id,person_id,source_created_at DESC,recorded_at DESC,id DESC) WHERE source_created_at IS NOT NULL',name||'_known',name);
  EXECUTE format('CREATE INDEX %I ON %I(organization_id,person_id,stable_position DESC,recorded_at DESC,id DESC) WHERE source_created_at IS NULL',name||'_unknown',name);
 END LOOP;
END $$;
CREATE FUNCTION crm_history_import_erased_identity() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE name TEXT;r RECORD;
BEGIN
 IF OLD.erased_at IS NULL AND NEW.erased_at IS NOT NULL THEN
  UPDATE migration_history_import_run attempt SET revision=revision+1,updated_at=clock_timestamp() WHERE attempt.organization_id=NEW.organization_id AND EXISTS(SELECT 1 FROM migration_history_import_manifest m WHERE m.organization_id=NEW.organization_id AND m.identity_hmac=NEW.identity_hmac AND m.plan_id=attempt.plan_id);
  FOREACH name IN ARRAY ARRAY['fub_event_record_imported','fub_call_record_imported','fub_text_record_imported'] LOOP
   FOR r IN EXECUTE format('SELECT f.person_id,f.source_created_at FROM %I f JOIN migration_history_import_display d ON d.id=f.manifest_id AND d.organization_id=f.organization_id WHERE f.organization_id=$1 AND f.identity_id=$2',name) USING NEW.organization_id,NEW.id LOOP
    PERFORM crm_history_review_delta(NEW.organization_id,r.person_id,name||CASE WHEN r.source_created_at IS NULL THEN '_unknown' ELSE '_known' END,-1);
   END LOOP;
  END LOOP;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER history_import_erased_identity AFTER UPDATE ON migration_history_import_identity FOR EACH ROW EXECUTE FUNCTION crm_history_import_erased_identity();
CREATE FUNCTION crm_history_import_display_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM migration_history_import_manifest m JOIN migration_history_import_identity i ON i.organization_id=m.organization_id AND i.identity_hmac=m.identity_hmac WHERE m.id=NEW.id AND m.organization_id=NEW.organization_id AND i.erased_at IS NOT NULL)
 THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_identity_erased';END IF;RETURN NEW;
END $$;
CREATE TRIGGER history_import_display_guard BEFORE INSERT ON migration_history_import_display FOR EACH ROW EXECUTE FUNCTION crm_history_import_display_guard();
CREATE FUNCTION crm_history_import_display_change() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE name TEXT;r RECORD;m RECORD;v JSONB;
BEGIN
 v:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 UPDATE migration_history_import_run SET revision=revision+1,updated_at=clock_timestamp() WHERE organization_id=(v->>'organization_id')::uuid AND plan_id=(v->>'plan_id')::uuid;
 FOREACH name IN ARRAY ARRAY['fub_event_record_imported','fub_call_record_imported','fub_text_record_imported'] LOOP
  FOR r IN EXECUTE format('SELECT f.person_id,f.source_created_at FROM %I f JOIN migration_history_import_identity i ON i.id=f.identity_id AND i.organization_id=f.organization_id AND i.erased_at IS NULL WHERE f.organization_id=$1 AND f.manifest_id=$2',name) USING (v->>'organization_id')::uuid,(v->>'id')::uuid LOOP
   PERFORM crm_history_review_delta((v->>'organization_id')::uuid,r.person_id,CASE WHEN TG_OP='DELETE' THEN name||CASE WHEN r.source_created_at IS NULL THEN '_unknown' ELSE '_known' END END,CASE WHEN TG_OP='DELETE' THEN -1 ELSE 0 END);
  END LOOP;
 END LOOP;
 IF TG_OP='DELETE' THEN
  SELECT * INTO m FROM migration_history_import_manifest WHERE id=OLD.id AND organization_id=OLD.organization_id;
  IF m.identity_hmac IS NOT NULL THEN
   INSERT INTO migration_history_import_identity(id,organization_id,owner_run_id,identity_hmac,semantic_hmac,person_id,fact_id,family,erased_at)
   VALUES(gen_random_uuid(),m.organization_id,m.owner_run_id,m.identity_hmac,m.semantic_hmac,m.person_id,NULL,m.family,clock_timestamp()) ON CONFLICT(organization_id,identity_hmac) DO NOTHING;
   UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=m.organization_id AND identity_hmac=m.identity_hmac AND erased_at IS NULL;
  END IF;
 END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
CREATE TRIGGER history_import_display_change AFTER UPDATE OR DELETE ON migration_history_import_display FOR EACH ROW EXECUTE FUNCTION crm_history_import_display_change();
CREATE FUNCTION crm_history_import_person_erasure() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=OLD.organization_id AND person_id=OLD.id AND erased_at IS NULL;
 DELETE FROM migration_history_import_display d USING migration_history_import_manifest m
 WHERE d.id=m.id AND d.organization_id=m.organization_id AND m.organization_id=OLD.organization_id
 AND (m.person_id=OLD.id OR EXISTS(SELECT 1 FROM migration_history_person_link l WHERE l.observation_id=m.observation_id AND l.organization_id=m.organization_id AND l.person_id=OLD.id));
 RETURN OLD;
END $$;
CREATE TRIGGER history_import_person_erasure BEFORE DELETE ON person FOR EACH ROW EXECUTE FUNCTION crm_history_import_person_erasure();
REVOKE ALL ON FUNCTION crm_history_import_fact_guard(),crm_history_import_fact_count(),crm_history_import_erased_identity(),crm_history_import_display_guard(),crm_history_import_display_change(),crm_history_import_person_erasure() FROM PUBLIC;
