-- D-086 / Slice 010d3: admitted-People retained history metadata.
-- This is additive. Existing 010d2 rows retain their original owner tuple.
CREATE TABLE migration_admitted_history_root (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id),
 parent_import_id UUID NOT NULL, parent_plan_id UUID NOT NULL,
 admission_id UUID NOT NULL, admission_plan_id UUID NOT NULL,
 history_capture_id UUID NOT NULL, capture_revision BIGINT NOT NULL,
 source_account_id BIGINT NOT NULL, source_access_user_id BIGINT NOT NULL,
 workspace_revision BIGINT NOT NULL, executor_user_id UUID NOT NULL REFERENCES app_user(id),
 state TEXT NOT NULL CHECK (state IN ('preparing','ready','queued','running','paused','completed','cancelled')),
 phase TEXT NOT NULL DEFAULT 'preparing' CHECK (phase IN ('preparing','classifying','applying','complete')),
 latest_plan_id UUID, confirmed_plan_id UUID, current_attempt_id UUID,
 lease_token UUID, lease_expires_at TIMESTAMPTZ, pause_reason TEXT, admitted_at TIMESTAMPTZ,
 revision BIGINT NOT NULL DEFAULT 1 CHECK (revision>0), budget_revision BIGINT NOT NULL DEFAULT 1 CHECK (budget_revision>0), run_byte_limit BIGINT NOT NULL DEFAULT 2147483648 CHECK (run_byte_limit>0), retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK (retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK (reserved_bytes>=0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 confirmed_at TIMESTAMPTZ, completed_at TIMESTAMPTZ,
 UNIQUE(id,organization_id), UNIQUE(id,history_capture_id,organization_id),
 FOREIGN KEY(parent_import_id,organization_id) REFERENCES migration_import(id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(admission_id,organization_id) REFERENCES migration_people_admission(id,organization_id),
 FOREIGN KEY(admission_plan_id,admission_id,organization_id) REFERENCES migration_people_admission_plan(id,admission_id,organization_id),
 FOREIGN KEY(history_capture_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE INDEX migration_admitted_history_root_page ON migration_admitted_history_root(organization_id,created_at DESC,id DESC);
CREATE UNIQUE INDEX migration_admitted_history_one_active ON migration_admitted_history_root(organization_id,admission_id) WHERE state NOT IN ('cancelled','completed');

CREATE TABLE migration_admitted_history_plan (
 id UUID PRIMARY KEY, root_id UUID NOT NULL, organization_id UUID NOT NULL, revision BIGINT NOT NULL CHECK(revision>0),
 state TEXT NOT NULL CHECK(state IN ('building','ready','expired','superseded')),
 binding_hmac BYTEA NOT NULL CHECK(octet_length(binding_hmac)=32), source_binding JSONB NOT NULL, coverage JSONB NOT NULL DEFAULT '{}', counts JSONB NOT NULL DEFAULT '{}',
 last_capture_sequence BIGINT NOT NULL DEFAULT 0, page_ordinal INTEGER NOT NULL DEFAULT 0 CHECK(page_ordinal BETWEEN 0 AND 100),
 stream_progress JSONB NOT NULL DEFAULT '{}', classified_position BIGINT NOT NULL DEFAULT 0,
 occurrences BIGINT NOT NULL DEFAULT 0, eligible BIGINT NOT NULL DEFAULT 0, equal_repeats BIGINT NOT NULL DEFAULT 0,
 held BIGINT NOT NULL DEFAULT 0, excluded BIGINT NOT NULL DEFAULT 0, unknown_dates BIGINT NOT NULL DEFAULT 0,
 expires_at TIMESTAMPTZ, created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), sealed_at TIMESTAMPTZ,
 UNIQUE(id,root_id,organization_id), UNIQUE(root_id,organization_id,revision),
 FOREIGN KEY(root_id,organization_id) REFERENCES migration_admitted_history_root(id,organization_id)
);
ALTER TABLE migration_admitted_history_root ADD FOREIGN KEY(latest_plan_id, id, organization_id) REFERENCES migration_admitted_history_plan(id,root_id,organization_id);
ALTER TABLE migration_admitted_history_root ADD FOREIGN KEY(confirmed_plan_id, id, organization_id) REFERENCES migration_admitted_history_plan(id,root_id,organization_id);

CREATE TABLE migration_admitted_history_attempt (
 id UUID PRIMARY KEY, root_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('queued','running','paused','completed','cancelled')),
 executor_user_id UUID NOT NULL REFERENCES app_user(id), lease_token UUID, lease_expires_at TIMESTAMPTZ,
 applied_position BIGINT NOT NULL DEFAULT 0, revision BIGINT NOT NULL DEFAULT 1 CHECK(revision>0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), completed_at TIMESTAMPTZ,
 UNIQUE(id,root_id,organization_id), UNIQUE(id,plan_id,root_id,organization_id),
 FOREIGN KEY(root_id,organization_id) REFERENCES migration_admitted_history_root(id,organization_id),
 FOREIGN KEY(plan_id,root_id,organization_id) REFERENCES migration_admitted_history_plan(id,root_id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
ALTER TABLE migration_admitted_history_root ADD FOREIGN KEY(current_attempt_id,id,organization_id) REFERENCES migration_admitted_history_attempt(id,root_id,organization_id);

CREATE TABLE migration_admitted_history_manifest (
 id UUID PRIMARY KEY, root_id UUID NOT NULL, plan_id UUID NOT NULL, attempt_id UUID, organization_id UUID NOT NULL,
 position BIGINT NOT NULL, capture_id UUID NOT NULL, observation_id UUID NOT NULL, ordinal INTEGER NOT NULL,
 family TEXT NOT NULL CHECK(family IN ('events','calls','text_messages')), representation TEXT NOT NULL,
 identity_hmac BYTEA CHECK(octet_length(identity_hmac)=32), semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 person_id UUID, source_created_at TIMESTAMPTZ,
 disposition TEXT NOT NULL CHECK(disposition IN ('eligible','equal_repeat','excluded','held','pending')),
 reason TEXT CHECK(reason IN ('invalid_identity','ambiguous_relationship','conflicting_variants','out_of_cohort','target_missing','target_erased','target_identity_mismatch','identity_erased','identity_conflict','fact_or_display_missing','capture_integrity','source_binding_changed')),
 nonce BYTEA CHECK(octet_length(nonce)=24), ciphertext BYTEA CHECK(octet_length(ciphertext)<=4112),
 UNIQUE(plan_id,organization_id,position), UNIQUE(id,plan_id,root_id,organization_id),
 FOREIGN KEY(root_id,organization_id) REFERENCES migration_admitted_history_root(id,organization_id),
 FOREIGN KEY(plan_id,root_id,organization_id) REFERENCES migration_admitted_history_plan(id,root_id,organization_id),
 FOREIGN KEY(attempt_id,root_id,organization_id) REFERENCES migration_admitted_history_attempt(id,root_id,organization_id)
);
ALTER TABLE migration_history_observation ADD UNIQUE(id,organization_id);
ALTER TABLE migration_admitted_history_manifest ADD FOREIGN KEY(observation_id,organization_id) REFERENCES migration_history_observation(id,organization_id);
CREATE INDEX migration_admitted_history_manifest_page ON migration_admitted_history_manifest(plan_id,organization_id,position);
CREATE INDEX migration_admitted_history_manifest_filter ON migration_admitted_history_manifest(plan_id,organization_id,family,disposition,position);

CREATE TABLE migration_admitted_history_candidate (
 root_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL, identity_hmac BYTEA NOT NULL CHECK(octet_length(identity_hmac)=32),
 semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32), first_manifest_id UUID NOT NULL, person_id UUID, occurrences BIGINT NOT NULL DEFAULT 1, conflicting BOOLEAN NOT NULL DEFAULT false,
 PRIMARY KEY(plan_id,organization_id,identity_hmac), FOREIGN KEY(first_manifest_id,plan_id,root_id,organization_id) REFERENCES migration_admitted_history_manifest(id,plan_id,root_id,organization_id)
);

CREATE TABLE migration_admitted_history_result (
 id UUID PRIMARY KEY, root_id UUID NOT NULL, plan_id UUID NOT NULL, attempt_id UUID NOT NULL, manifest_id UUID NOT NULL, organization_id UUID NOT NULL,
 position BIGINT NOT NULL, family TEXT NOT NULL, disposition TEXT NOT NULL CHECK(disposition IN ('imported','already_present','equal_repeat','excluded','held')),
 reason TEXT, fact_id UUID, committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(attempt_id,organization_id,manifest_id), FOREIGN KEY(manifest_id,plan_id,root_id,organization_id) REFERENCES migration_admitted_history_manifest(id,plan_id,root_id,organization_id),
 FOREIGN KEY(attempt_id,root_id,organization_id) REFERENCES migration_admitted_history_attempt(id,root_id,organization_id)
);
CREATE INDEX migration_admitted_history_result_page ON migration_admitted_history_result(attempt_id,organization_id,position);

CREATE TABLE migration_admitted_history_remainder (
 id UUID PRIMARY KEY, root_id UUID NOT NULL, predecessor_attempt_id UUID NOT NULL, attempt_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 never_settled_count BIGINT NOT NULL CHECK(never_settled_count>=0), created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(predecessor_attempt_id,organization_id), FOREIGN KEY(root_id,organization_id) REFERENCES migration_admitted_history_root(id,organization_id),
 FOREIGN KEY(predecessor_attempt_id,root_id,organization_id) REFERENCES migration_admitted_history_attempt(id,root_id,organization_id),
 FOREIGN KEY(attempt_id,root_id,organization_id) REFERENCES migration_admitted_history_attempt(id,root_id,organization_id),
 FOREIGN KEY(plan_id,root_id,organization_id) REFERENCES migration_admitted_history_plan(id,root_id,organization_id)
);
CREATE TABLE migration_admitted_history_receipt (
 organization_id UUID NOT NULL, actor_user_id UUID NOT NULL REFERENCES app_user(id), action TEXT NOT NULL, request_id UUID NOT NULL,
 root_id UUID NOT NULL, input_digest BYTEA NOT NULL CHECK(octet_length(input_digest)=32), nonce BYTEA, ciphertext BYTEA, created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(organization_id,actor_user_id,action,request_id), FOREIGN KEY(root_id,organization_id) REFERENCES migration_admitted_history_root(id,organization_id)
);
CREATE TABLE migration_admitted_history_reservation (
 token UUID PRIMARY KEY, root_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL, purpose TEXT NOT NULL CHECK(purpose IN ('work','cancel')),
 byte_count BIGINT NOT NULL CHECK(byte_count>0), expires_at TIMESTAMPTZ NOT NULL,
 FOREIGN KEY(root_id,organization_id) REFERENCES migration_admitted_history_root(id,organization_id), FOREIGN KEY(plan_id,root_id,organization_id) REFERENCES migration_admitted_history_plan(id,root_id,organization_id)
);
CREATE TABLE migration_admitted_history_issue (
 plan_id UUID NOT NULL, root_id UUID NOT NULL, organization_id UUID NOT NULL, code TEXT NOT NULL, record_count BIGINT NOT NULL CHECK(record_count>0),
 PRIMARY KEY(plan_id,organization_id,code), FOREIGN KEY(plan_id,root_id,organization_id) REFERENCES migration_admitted_history_plan(id,root_id,organization_id)
);

-- Extend the one global identity registry and typed facts with an exclusive admitted owner.
ALTER TABLE migration_history_import_identity ALTER COLUMN owner_run_id DROP NOT NULL;
ALTER TABLE migration_history_import_identity ADD COLUMN IF NOT EXISTS admitted_root_id UUID, ADD COLUMN IF NOT EXISTS admitted_plan_id UUID, ADD COLUMN IF NOT EXISTS admitted_attempt_id UUID, ADD COLUMN IF NOT EXISTS admitted_manifest_id UUID;
ALTER TABLE migration_history_import_identity ADD CONSTRAINT migration_history_import_identity_admitted_owner_fk FOREIGN KEY(admitted_manifest_id,admitted_plan_id,admitted_root_id,organization_id) REFERENCES migration_admitted_history_manifest(id,plan_id,root_id,organization_id);
ALTER TABLE migration_history_import_identity ADD CONSTRAINT migration_history_import_identity_exclusive_owner CHECK ((owner_run_id IS NOT NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL) OR (owner_run_id IS NULL AND admitted_root_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_attempt_id IS NOT NULL AND admitted_manifest_id IS NOT NULL));
DO $$ DECLARE n TEXT; BEGIN
 FOREACH n IN ARRAY ARRAY['fub_event_record_imported','fub_call_record_imported','fub_text_record_imported'] LOOP
  EXECUTE format('ALTER TABLE %I ALTER COLUMN plan_id DROP NOT NULL, ALTER COLUMN attempt_id DROP NOT NULL, ALTER COLUMN manifest_id DROP NOT NULL',n);
  EXECUTE format('ALTER TABLE %I ADD COLUMN IF NOT EXISTS admitted_root_id UUID, ADD COLUMN IF NOT EXISTS admitted_plan_id UUID, ADD COLUMN IF NOT EXISTS admitted_attempt_id UUID, ADD COLUMN IF NOT EXISTS admitted_manifest_id UUID',n);
  EXECUTE format('ALTER TABLE %I ADD CONSTRAINT %I FOREIGN KEY(admitted_manifest_id,admitted_plan_id,admitted_root_id,organization_id) REFERENCES migration_admitted_history_manifest(id,plan_id,root_id,organization_id)',n,n||'_admitted_owner_fk');
  EXECUTE format('ALTER TABLE %I ADD CONSTRAINT %I CHECK ((plan_id IS NOT NULL AND attempt_id IS NOT NULL AND manifest_id IS NOT NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL AND admitted_manifest_id IS NULL) OR (plan_id IS NULL AND attempt_id IS NULL AND manifest_id IS NULL AND admitted_root_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_attempt_id IS NOT NULL AND admitted_manifest_id IS NOT NULL))',n,n||'_exclusive_owner');
 END LOOP;
END $$;
ALTER TABLE migration_history_import_display ADD COLUMN IF NOT EXISTS admitted_root_id UUID, ADD COLUMN IF NOT EXISTS admitted_plan_id UUID, ADD COLUMN IF NOT EXISTS admitted_attempt_id UUID;
GRANT SELECT,INSERT,UPDATE ON migration_admitted_history_root,migration_admitted_history_plan,migration_admitted_history_attempt,migration_admitted_history_manifest,migration_admitted_history_candidate,migration_admitted_history_result,migration_admitted_history_remainder,migration_admitted_history_issue TO crm_app;
GRANT SELECT,INSERT ON migration_admitted_history_receipt TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_admitted_history_reservation TO crm_app;

ALTER TABLE migration_admitted_history_root ADD COLUMN result_counts JSONB NOT NULL DEFAULT '{}';
ALTER TABLE migration_admitted_history_reservation ADD UNIQUE(root_id,organization_id,purpose);
ALTER TABLE migration_admitted_history_result ADD UNIQUE(root_id,organization_id,manifest_id);
ALTER TABLE migration_admitted_history_result ADD FOREIGN KEY(attempt_id,plan_id,root_id,organization_id) REFERENCES migration_admitted_history_attempt(id,plan_id,root_id,organization_id);
ALTER TABLE migration_history_import_identity ADD FOREIGN KEY(admitted_attempt_id,admitted_plan_id,admitted_root_id,organization_id) REFERENCES migration_admitted_history_attempt(id,plan_id,root_id,organization_id);
ALTER TABLE migration_history_import_display ALTER COLUMN plan_id DROP NOT NULL, ALTER COLUMN owner_run_id DROP NOT NULL;
ALTER TABLE migration_history_import_display ADD CONSTRAINT history_display_exclusive_owner CHECK(
 (plan_id IS NOT NULL AND owner_run_id IS NOT NULL AND admitted_root_id IS NULL AND admitted_plan_id IS NULL AND admitted_attempt_id IS NULL) OR
 (plan_id IS NULL AND owner_run_id IS NULL AND admitted_root_id IS NOT NULL AND admitted_plan_id IS NOT NULL AND admitted_attempt_id IS NOT NULL));
ALTER TABLE migration_history_import_display ADD FOREIGN KEY(id,admitted_plan_id,admitted_root_id,organization_id) REFERENCES migration_admitted_history_manifest(id,plan_id,root_id,organization_id);
ALTER TABLE migration_history_import_display ADD FOREIGN KEY(admitted_attempt_id,admitted_plan_id,admitted_root_id,organization_id) REFERENCES migration_admitted_history_attempt(id,plan_id,root_id,organization_id);
DO $$ DECLARE n TEXT; BEGIN
 FOREACH n IN ARRAY ARRAY['fub_event_record_imported','fub_call_record_imported','fub_text_record_imported'] LOOP
  EXECUTE format('ALTER TABLE %I ADD FOREIGN KEY(admitted_attempt_id,admitted_plan_id,admitted_root_id,organization_id) REFERENCES migration_admitted_history_attempt(id,plan_id,root_id,organization_id)',n);
 END LOOP;
END $$;

-- Both durable predicates survive cancellation, completion and erasure.
CREATE OR REPLACE FUNCTION crm_workspace_shared(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF NOT pg_try_advisory_xact_lock_shared(hashtextextended('crm-workspace-v1:'||org::text,0)) THEN RAISE EXCEPTION USING ERRCODE='55P03',MESSAGE='workspace_busy'; END IF;
 IF current_user='crm_app' THEN
  IF EXISTS(SELECT 1 FROM migration_admitted_activity_import WHERE organization_id=org AND confirmed_plan_id IS NOT NULL) AND current_setting('crm.admitted_activity_reader',true) IS DISTINCT FROM 'fub-admitted-activity-v1' THEN RAISE EXCEPTION USING ERRCODE='P010F',MESSAGE='admitted_activity_handover_required'; END IF;
  IF EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=org) AND current_setting('crm.history_reader',true) IS DISTINCT FROM 'fub-history-timeline-v1' THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_reader_required'; END IF;
  IF EXISTS(SELECT 1 FROM migration_admitted_history_root WHERE organization_id=org AND confirmed_plan_id IS NOT NULL) AND current_setting('crm.admitted_history_reader',true) IS DISTINCT FROM 'fub-admitted-history-v1' THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_reader_required'; END IF;
 END IF;
END $$;
CREATE OR REPLACE FUNCTION crm_history_complete_read(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM crm_workspace_shared(org);
 IF EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=org) OR EXISTS(SELECT 1 FROM migration_admitted_history_root WHERE organization_id=org AND confirmed_plan_id IS NOT NULL) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_review_required'; END IF;
END $$;
CREATE FUNCTION crm_admitted_history_write_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE org UUID;
BEGIN
 org:=(COALESCE(to_jsonb(NEW),to_jsonb(OLD))->>'organization_id')::uuid;
 PERFORM crm_workspace_shared(org);
 RETURN COALESCE(NEW,OLD);
END $$;
DO $$ DECLARE n TEXT; BEGIN
 FOREACH n IN ARRAY ARRAY['migration_history_import_run','migration_history_import_plan','migration_history_import_manifest','migration_history_import_candidate','migration_history_import_stream','migration_history_import_identity','migration_history_import_display','migration_history_import_result','migration_history_import_receipt','migration_history_import_reservation','fub_event_record_imported','fub_call_record_imported','fub_text_record_imported','migration_admitted_history_root','migration_admitted_history_plan','migration_admitted_history_attempt','migration_admitted_history_manifest','migration_admitted_history_candidate','migration_admitted_history_result','migration_admitted_history_remainder','migration_admitted_history_receipt','migration_admitted_history_reservation','migration_admitted_history_issue'] LOOP
  EXECUTE format('CREATE TRIGGER admitted_history_write_fence BEFORE INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_write_fence()',n);
 END LOOP;
END $$;

CREATE FUNCTION crm_admitted_history_retained_size(v JSONB) RETURNS BIGINT LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE total BIGINT:=0;k TEXT;value TEXT;
BEGIN
 IF v IS NULL THEN RETURN 0; END IF;
 FOR k,value IN SELECT e.key,e.value #>> '{}' FROM jsonb_each(v) e LOOP
  IF value IS NULL THEN CONTINUE; END IF;
  IF k IN ('nonce','ciphertext','identity_hmac','semantic_hmac','input_digest','binding_hmac') THEN total:=total+octet_length(decode(substring(value FROM 3),'hex'));
  ELSIF k IN ('source_binding','coverage','counts','result_counts','stream_progress','representation','pause_reason','code') THEN total:=total+octet_length(value); END IF;
 END LOOP; RETURN total;
END $$;
CREATE FUNCTION crm_admitted_history_measure() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE delta BIGINT;v JSONB;root UUID;org UUID;
BEGIN
 delta:=crm_admitted_history_retained_size(CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)-crm_admitted_history_retained_size(CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta=0 THEN RETURN COALESCE(NEW,OLD); END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD));org:=(v->>'organization_id')::uuid;
 root:=(v->>CASE WHEN TG_TABLE_NAME='migration_admitted_history_root' THEN 'id' ELSE 'root_id' END)::uuid;
 UPDATE migration_admitted_history_root SET retained_bytes=retained_bytes+delta WHERE id=root AND organization_id=org;
 UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+delta WHERE organization_id=org;
 RETURN COALESCE(NEW,OLD);
END $$;
DO $$ DECLARE n TEXT; BEGIN
 FOREACH n IN ARRAY ARRAY['migration_admitted_history_root','migration_admitted_history_plan','migration_admitted_history_manifest','migration_admitted_history_candidate','migration_admitted_history_result','migration_admitted_history_remainder','migration_admitted_history_receipt','migration_admitted_history_issue'] LOOP
  EXECUTE format('CREATE TRIGGER admitted_history_measure AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_measure()',n);
 END LOOP;
END $$;
-- Existing measured rows dispatch charges to exactly one owner.
CREATE OR REPLACE FUNCTION crm_history_import_measure() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE delta BIGINT;owner_id UUID;org UUID;v JSONB;
BEGIN
 delta:=crm_history_import_retained_size(CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)-crm_history_import_retained_size(CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta=0 THEN RETURN COALESCE(NEW,OLD);END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD));org:=(v->>'organization_id')::uuid;
 IF v->>'admitted_root_id' IS NOT NULL THEN
  UPDATE migration_admitted_history_root SET retained_bytes=retained_bytes+delta WHERE id=(v->>'admitted_root_id')::uuid AND organization_id=org;
 ELSE
  IF TG_TABLE_NAME='migration_history_import_anchor' THEN SELECT owner_run_id INTO STRICT owner_id FROM migration_history_import_plan WHERE id=(v->>'plan_id')::uuid AND organization_id=org;
  ELSE owner_id:=(v->>CASE WHEN TG_TABLE_NAME='migration_history_import_run' THEN 'id' ELSE 'owner_run_id' END)::uuid;END IF;
  UPDATE migration_history_import_run SET retained_bytes=retained_bytes+delta WHERE id=owner_id AND organization_id=org;
 END IF;
 UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+delta WHERE organization_id=org;
 RETURN COALESCE(NEW,OLD);
END $$;

CREATE FUNCTION crm_admitted_history_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v JSONB;p UUID;org UUID;
BEGIN
 IF TG_TABLE_NAME IN ('migration_admitted_history_receipt','migration_admitted_history_remainder','migration_admitted_history_result') THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_immutable'; END IF;
 IF TG_OP='DELETE' THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_immutable'; END IF;
 IF TG_TABLE_NAME='migration_admitted_history_root' THEN
  IF (to_jsonb(OLD)-ARRAY['state','phase','latest_plan_id','confirmed_plan_id','current_attempt_id','lease_token','lease_expires_at','pause_reason','revision','retained_bytes','reserved_bytes','updated_at','confirmed_at','completed_at','executor_user_id','budget_revision','run_byte_limit','admitted_at','result_counts']) IS DISTINCT FROM (to_jsonb(NEW)-ARRAY['state','phase','latest_plan_id','confirmed_plan_id','current_attempt_id','lease_token','lease_expires_at','pause_reason','revision','retained_bytes','reserved_bytes','updated_at','confirmed_at','completed_at','executor_user_id','budget_revision','run_byte_limit','admitted_at','result_counts']) OR (OLD.confirmed_plan_id IS NOT NULL AND NEW.confirmed_plan_id IS DISTINCT FROM OLD.confirmed_plan_id) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_binding_immutable';END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_history_attempt' THEN
  IF (to_jsonb(OLD)-ARRAY['state','executor_user_id','lease_token','lease_expires_at','applied_position','revision','completed_at']) IS DISTINCT FROM (to_jsonb(NEW)-ARRAY['state','executor_user_id','lease_token','lease_expires_at','applied_position','revision','completed_at']) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_attempt_immutable';END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_history_plan' THEN
  IF OLD.source_binding IS DISTINCT FROM NEW.source_binding OR OLD.binding_hmac IS DISTINCT FROM NEW.binding_hmac OR (OLD.state<>'building' AND to_jsonb(NEW) IS DISTINCT FROM to_jsonb(OLD)) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_plan_immutable';END IF;
 ELSE
  v:=to_jsonb(OLD);p:=(v->>'plan_id')::uuid;org:=(v->>'organization_id')::uuid;
  IF EXISTS(SELECT 1 FROM migration_admitted_history_plan WHERE id=p AND organization_id=org AND state<>'building') THEN
   IF current_user<>'crm_app' AND TG_TABLE_NAME='migration_admitted_history_manifest' AND (to_jsonb(NEW)-ARRAY['nonce','ciphertext'])=(to_jsonb(OLD)-ARRAY['nonce','ciphertext']) AND to_jsonb(NEW)->>'nonce' IS NULL AND to_jsonb(NEW)->>'ciphertext' IS NULL THEN RETURN NEW;END IF;
   RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_plan_immutable';
  END IF;
 END IF;RETURN NEW;
END $$;
DO $$ DECLARE n TEXT;BEGIN
 FOREACH n IN ARRAY ARRAY['migration_admitted_history_root','migration_admitted_history_plan','migration_admitted_history_attempt','migration_admitted_history_manifest','migration_admitted_history_candidate','migration_admitted_history_issue','migration_admitted_history_result','migration_admitted_history_remainder','migration_admitted_history_receipt'] LOOP
  EXECUTE format('CREATE TRIGGER admitted_history_immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_immutable()',n);
 END LOOP;
END $$;

CREATE FUNCTION crm_admitted_history_fact_allowed(v JSONB,table_name TEXT) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE permit JSONB;
BEGIN
 PERFORM crm_workspace_shared((v->>'organization_id')::uuid);
 BEGIN permit:=current_setting('crm.admitted_history_import_permit',true)::jsonb; EXCEPTION WHEN OTHERS THEN RETURN false; END;
 RETURN EXISTS(SELECT 1 FROM migration_admitted_history_root r
  JOIN migration_admitted_history_plan p ON p.id=r.confirmed_plan_id AND p.root_id=r.id AND p.organization_id=r.organization_id AND p.state='ready'
  JOIN migration_admitted_history_attempt a ON a.id=r.current_attempt_id AND a.root_id=r.id AND a.plan_id=p.id AND a.organization_id=r.organization_id AND a.state='running'
  JOIN migration_admitted_history_manifest m ON m.id=(v->>'admitted_manifest_id')::uuid AND m.root_id=r.id AND m.plan_id=p.id AND m.organization_id=r.organization_id AND m.disposition='eligible'
  JOIN migration_history_import_identity i ON i.id=(v->>'identity_id')::uuid AND i.organization_id=r.organization_id AND i.identity_hmac=m.identity_hmac AND i.semantic_hmac=m.semantic_hmac AND i.person_id=m.person_id AND i.erased_at IS NULL AND i.fact_id=(v->>'id')::uuid
  JOIN migration_history_import_display d ON d.id=m.id AND d.organization_id=r.organization_id AND d.admitted_root_id=r.id AND d.admitted_plan_id=p.id AND d.admitted_attempt_id=a.id
  JOIN person person ON person.id=m.person_id AND person.organization_id=r.organization_id
  JOIN organization o ON o.id=r.organization_id AND o.workspace_mode='migration_review' AND o.workspace_revision=r.workspace_revision
  JOIN organization_membership member ON member.organization_id=r.organization_id AND member.user_id=r.executor_user_id AND member.status='active' AND member.role='admin'
  WHERE r.id=(v->>'admitted_root_id')::uuid AND r.organization_id=(v->>'organization_id')::uuid AND p.id=(v->>'admitted_plan_id')::uuid AND a.id=(v->>'admitted_attempt_id')::uuid
   AND r.state='running' AND r.lease_token::text=permit->>'token' AND r.lease_expires_at>clock_timestamp() AND a.lease_token=r.lease_token AND a.lease_expires_at>clock_timestamp()
   AND permit->>'root'=r.id::text AND permit->>'plan'=p.id::text AND permit->>'attempt'=a.id::text AND permit->>'manifest'=m.id::text
   AND a.executor_user_id=r.executor_user_id AND v->>'actor_user_id'=r.executor_user_id::text AND v->>'actor_kind'='user' AND v->>'origin'='migration' AND v->>'on_behalf_of_user_id' IS NULL AND v->>'corrects_id' IS NULL
   AND (v->>'person_id')::uuid=m.person_id AND (v->>'source_created_at')::timestamptz IS NOT DISTINCT FROM m.source_created_at AND (v->>'stable_position')::bigint=m.position
   AND i.admitted_root_id=r.id AND i.admitted_plan_id=p.id AND i.admitted_attempt_id=a.id AND i.admitted_manifest_id=m.id
   AND m.family=CASE table_name WHEN 'fub_event_record_imported' THEN 'events' WHEN 'fub_call_record_imported' THEN 'calls' WHEN 'fub_text_record_imported' THEN 'text_messages' END
   AND EXISTS(SELECT 1 FROM migration_people_admission_result ar JOIN migration_people_admission_item ai ON ai.id=ar.item_id AND ai.admission_id=ar.admission_id AND ai.organization_id=ar.organization_id AND ai.settled_result_id=ar.id JOIN migration_import_identity pi ON pi.organization_id=ar.organization_id AND pi.family='people' AND pi.source_account_id=r.source_account_id AND pi.source_id=ar.source_id AND pi.target_id=ar.person_id AND pi.admission_result_id=ar.id WHERE ar.admission_id=r.admission_id AND ai.plan_id=r.admission_plan_id AND ar.organization_id=r.organization_id AND ar.person_id=m.person_id AND ar.disposition='settled'));
END $$;
-- Keep the complete original permit checks, introducing an exclusive admitted branch.
DO $$ DECLARE def TEXT; BEGIN
 SELECT pg_get_functiondef('crm_history_import_fact_guard()'::regprocedure) INTO def;
 def:=replace(def,E'BEGIN\n',E'BEGIN\n IF NEW.admitted_root_id IS NOT NULL THEN\n  IF NOT COALESCE(crm_admitted_history_fact_allowed(to_jsonb(NEW),TG_TABLE_NAME),false) THEN RAISE EXCEPTION USING ERRCODE=''P010H'',MESSAGE=''admitted_history_permit_required'';END IF;\n  RETURN NEW;\n END IF;\n');
 EXECUTE def;
END $$;
REVOKE ALL ON FUNCTION crm_admitted_history_write_fence(),crm_admitted_history_retained_size(JSONB),crm_admitted_history_measure(),crm_admitted_history_immutable(),crm_admitted_history_fact_allowed(JSONB,TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_admitted_history_retained_size(JSONB),crm_admitted_history_fact_allowed(JSONB,TEXT) TO crm_app;

CREATE FUNCTION crm_admitted_history_owner_insert() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE m migration_admitted_history_manifest%ROWTYPE;r migration_admitted_history_root%ROWTYPE;v JSONB;permit JSONB;
BEGIN
 v:=to_jsonb(NEW);IF v->>'admitted_root_id' IS NULL THEN RETURN NEW;END IF;
 SELECT * INTO r FROM migration_admitted_history_root WHERE id=(v->>'admitted_root_id')::uuid AND organization_id=NEW.organization_id;
 SELECT * INTO m FROM migration_admitted_history_manifest WHERE id=COALESCE((v->>'admitted_manifest_id')::uuid,NEW.id) AND plan_id=(v->>'admitted_plan_id')::uuid AND root_id=r.id AND organization_id=NEW.organization_id;
 IF r.id IS NULL OR m.id IS NULL OR r.state<>'running' OR r.phase<>'applying' OR r.confirmed_plan_id<>m.plan_id OR r.current_attempt_id<>(v->>'admitted_attempt_id')::uuid OR r.lease_expires_at<=clock_timestamp() OR m.disposition<>'eligible' THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_owner_invalid'; END IF;
 BEGIN permit:=current_setting('crm.admitted_history_import_permit',true)::jsonb; EXCEPTION WHEN OTHERS THEN permit:=NULL; END;
 IF permit IS NULL OR permit->>'root' IS DISTINCT FROM r.id::text OR permit->>'plan' IS DISTINCT FROM m.plan_id::text OR permit->>'attempt' IS DISTINCT FROM r.current_attempt_id::text OR permit->>'manifest' IS DISTINCT FROM m.id::text OR permit->>'token' IS DISTINCT FROM r.lease_token::text
 OR NOT EXISTS(SELECT 1 FROM migration_admitted_history_attempt a JOIN organization o ON o.id=a.organization_id JOIN organization_membership member ON member.organization_id=a.organization_id AND member.user_id=a.executor_user_id WHERE a.id=r.current_attempt_id AND a.root_id=r.id AND a.plan_id=m.plan_id AND a.organization_id=r.organization_id AND a.executor_user_id=r.executor_user_id AND a.state='running' AND a.lease_token=r.lease_token AND a.lease_expires_at>clock_timestamp() AND o.workspace_mode='migration_review' AND o.workspace_revision=r.workspace_revision AND member.status='active' AND member.role='admin')
 OR NOT EXISTS(SELECT 1 FROM migration_admitted_history_reservation WHERE root_id=r.id AND organization_id=r.organization_id AND purpose='work')
 THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_permit_required';END IF;
 IF TG_TABLE_NAME='migration_history_import_identity' THEN
  IF NEW.identity_hmac IS DISTINCT FROM m.identity_hmac OR NEW.semantic_hmac IS DISTINCT FROM m.semantic_hmac OR NEW.person_id IS DISTINCT FROM m.person_id OR NEW.family<>m.family THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_owner_invalid';END IF;
 ELSE
  IF EXISTS(SELECT 1 FROM migration_history_import_identity WHERE organization_id=NEW.organization_id AND identity_hmac=m.identity_hmac AND erased_at IS NOT NULL) OR m.nonce IS NULL THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_identity_erased'; END IF;
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER admitted_history_owner_insert BEFORE INSERT ON migration_history_import_identity FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_owner_insert();
CREATE TRIGGER admitted_history_owner_insert BEFORE INSERT ON migration_history_import_display FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_owner_insert();

-- Both visible owner branches contribute to the same revision/count state.
DO $$ DECLARE n TEXT;def TEXT;BEGIN
 FOREACH n IN ARRAY ARRAY['crm_history_import_erased_identity()','crm_history_import_display_change()'] LOOP
  SELECT pg_get_functiondef(n::regprocedure) INTO def;
  def:=replace(def,'d.id=f.manifest_id','d.id=COALESCE(f.manifest_id,f.admitted_manifest_id)');
  def:=replace(def,'f.manifest_id=$2','COALESCE(f.manifest_id,f.admitted_manifest_id)=$2');
  IF n='crm_history_import_display_change()' THEN
   def:=replace(def,'IF TG_OP=''DELETE'' THEN', 'IF TG_OP=''DELETE'' AND OLD.admitted_root_id IS NULL THEN');
  END IF;
  EXECUTE def;
 END LOOP;
END $$;
CREATE FUNCTION crm_admitted_history_suppress() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE h BYTEA;
BEGIN
 IF OLD.admitted_root_id IS NULL THEN RETURN OLD; END IF;
 SELECT identity_hmac INTO h FROM migration_admitted_history_manifest WHERE id=OLD.id AND root_id=OLD.admitted_root_id AND organization_id=OLD.organization_id;
 UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=OLD.organization_id AND identity_hmac=h AND erased_at IS NULL;
 UPDATE migration_admitted_history_manifest SET nonce=NULL,ciphertext=NULL WHERE organization_id=OLD.organization_id AND identity_hmac=h AND nonce IS NOT NULL;
 UPDATE migration_admitted_history_root r SET revision=revision+1 WHERE organization_id=OLD.organization_id AND EXISTS(SELECT 1 FROM migration_admitted_history_manifest m WHERE m.root_id=r.id AND m.organization_id=r.organization_id AND m.identity_hmac=h);
 RETURN OLD;
END $$;
CREATE TRIGGER zz_admitted_history_suppress AFTER DELETE ON migration_history_import_display FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_suppress();
CREATE FUNCTION crm_admitted_history_person_erasure() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=OLD.organization_id AND person_id=OLD.id AND erased_at IS NULL;
 DELETE FROM migration_history_import_display d USING migration_admitted_history_manifest m WHERE d.id=m.id AND d.admitted_root_id=m.root_id AND d.organization_id=m.organization_id AND m.organization_id=OLD.organization_id AND m.person_id=OLD.id;
 UPDATE migration_admitted_history_manifest SET nonce=NULL,ciphertext=NULL WHERE organization_id=OLD.organization_id AND person_id=OLD.id AND nonce IS NOT NULL;
 RETURN OLD;
END $$;
CREATE TRIGGER admitted_history_person_erasure BEFORE DELETE ON person FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_person_erasure();
REVOKE ALL ON FUNCTION crm_admitted_history_owner_insert(),crm_admitted_history_suppress(),crm_admitted_history_person_erasure() FROM PUBLIC;

-- Evidence may be extended only by the leased preparation that owns this plan.
CREATE FUNCTION crm_admitted_history_evidence_insert() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v JSONB;r migration_admitted_history_root%ROWTYPE;
BEGIN
 v:=to_jsonb(NEW);
 SELECT r0.* INTO r FROM migration_admitted_history_root r0 JOIN migration_admitted_history_plan p ON p.id=(v->>'plan_id')::uuid AND p.root_id=r0.id AND p.organization_id=r0.organization_id WHERE r0.id=(v->>'root_id')::uuid AND r0.organization_id=NEW.organization_id AND p.state='building';
 IF r.id IS NULL OR r.confirmed_plan_id IS NOT NULL OR r.state<>'running' OR r.phase NOT IN ('preparing','classifying') OR r.lease_expires_at<=clock_timestamp() THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_evidence_frozen';END IF;
 IF TG_TABLE_NAME='migration_admitted_history_manifest' AND NOT EXISTS(SELECT 1 FROM migration_history_observation o JOIN migration_history_capture c ON c.id=o.capture_id AND c.organization_id=o.organization_id AND c.run_id=o.run_id WHERE o.id=(v->>'observation_id')::uuid AND o.organization_id=NEW.organization_id AND o.run_id=r.history_capture_id AND o.capture_id=(v->>'capture_id')::uuid AND o.ordinal=(v->>'ordinal')::integer AND o.family=v->>'family' AND c.classification='advancing') THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_evidence_invalid';END IF;
 RETURN NEW;
END $$;
DO $$ DECLARE n TEXT;BEGIN
 FOREACH n IN ARRAY ARRAY['migration_admitted_history_manifest','migration_admitted_history_candidate','migration_admitted_history_issue'] LOOP
  EXECUTE format('CREATE TRIGGER admitted_history_evidence_insert BEFORE INSERT ON %I FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_evidence_insert()',n);
 END LOOP;
END $$;
REVOKE ALL ON FUNCTION crm_admitted_history_evidence_insert() FROM PUBLIC;

CREATE FUNCTION crm_admitted_history_identity_erasure() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF OLD.erased_at IS NULL AND NEW.erased_at IS NOT NULL THEN
  UPDATE migration_admitted_history_manifest SET nonce=NULL,ciphertext=NULL WHERE organization_id=NEW.organization_id AND identity_hmac=NEW.identity_hmac AND nonce IS NOT NULL;
  UPDATE migration_admitted_history_root r SET revision=revision+1 WHERE r.organization_id=NEW.organization_id AND EXISTS(SELECT 1 FROM migration_admitted_history_manifest m WHERE m.root_id=r.id AND m.organization_id=r.organization_id AND m.identity_hmac=NEW.identity_hmac);
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER admitted_history_identity_erasure AFTER UPDATE ON migration_history_import_identity FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_identity_erasure();
CREATE FUNCTION crm_history_display_owner_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(OLD)-ARRAY['nonce','ciphertext']) IS DISTINCT FROM (to_jsonb(NEW)-ARRAY['nonce','ciphertext']) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_display_owner_immutable';END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER history_display_owner_immutable BEFORE UPDATE ON migration_history_import_display FOR EACH ROW EXECUTE FUNCTION crm_history_display_owner_immutable();
REVOKE ALL ON FUNCTION crm_admitted_history_identity_erasure(),crm_history_display_owner_immutable() FROM PUBLIC;

CREATE INDEX migration_admitted_history_identity_group ON migration_admitted_history_manifest(plan_id,organization_id,identity_hmac,position);
CREATE INDEX migration_admitted_history_results_plan_page ON migration_admitted_history_result(root_id,plan_id,organization_id,position);
CREATE INDEX migration_admitted_history_results_filter ON migration_admitted_history_result(root_id,plan_id,organization_id,family,disposition,position);
CREATE INDEX migration_admitted_history_confirmed_workspace ON migration_admitted_history_root(organization_id) WHERE confirmed_plan_id IS NOT NULL;
CREATE INDEX migration_admitted_history_claim ON migration_admitted_history_root(created_at,id) WHERE state IN ('preparing','queued','running');

GRANT UPDATE(byte_count) ON migration_admitted_history_reservation TO crm_app;

-- Results can settle only the current leased manifest; a reader stamp is not
-- a mutation permit. Immutable results cannot be used to skip future work.
CREATE FUNCTION crm_admitted_history_result_insert() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE permit JSONB; r migration_admitted_history_root%ROWTYPE; m migration_admitted_history_manifest%ROWTYPE; table_name TEXT; exists_fact BOOLEAN;
BEGIN
 BEGIN permit:=current_setting('crm.admitted_history_import_permit',true)::jsonb; EXCEPTION WHEN OTHERS THEN permit:=NULL;END;
 SELECT * INTO r FROM migration_admitted_history_root WHERE id=NEW.root_id AND organization_id=NEW.organization_id;
 SELECT * INTO m FROM migration_admitted_history_manifest WHERE id=NEW.manifest_id AND plan_id=NEW.plan_id AND root_id=NEW.root_id AND organization_id=NEW.organization_id;
 IF r.id IS NULL OR m.id IS NULL OR permit IS NULL OR r.state<>'running' OR r.phase<>'applying' OR r.confirmed_plan_id<>NEW.plan_id OR r.current_attempt_id<>NEW.attempt_id
 OR r.lease_expires_at<=clock_timestamp() OR permit->>'root' IS DISTINCT FROM r.id::text OR permit->>'plan' IS DISTINCT FROM NEW.plan_id::text OR permit->>'attempt' IS DISTINCT FROM NEW.attempt_id::text OR permit->>'manifest' IS DISTINCT FROM m.id::text OR permit->>'token' IS DISTINCT FROM r.lease_token::text
 OR NEW.position<>m.position OR NEW.family<>m.family
 OR NOT EXISTS(SELECT 1 FROM migration_admitted_history_attempt a JOIN organization o ON o.id=a.organization_id JOIN organization_membership member ON member.organization_id=a.organization_id AND member.user_id=a.executor_user_id WHERE a.id=NEW.attempt_id AND a.root_id=r.id AND a.plan_id=NEW.plan_id AND a.organization_id=r.organization_id AND a.executor_user_id=r.executor_user_id AND a.state='running' AND a.lease_token=r.lease_token AND a.lease_expires_at>clock_timestamp() AND o.workspace_mode='migration_review' AND o.workspace_revision=r.workspace_revision AND member.status='active' AND member.role='admin')
 OR NOT EXISTS(SELECT 1 FROM migration_admitted_history_reservation WHERE root_id=r.id AND organization_id=r.organization_id AND purpose='work')
 THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_result_permit_required';END IF;
 IF NEW.disposition IN ('imported','already_present') THEN
  IF m.disposition<>'eligible' OR NEW.fact_id IS NULL OR NEW.reason IS NOT NULL THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_result_invalid';END IF;
  table_name:=CASE m.family WHEN 'events' THEN 'fub_event_record_imported' WHEN 'calls' THEN 'fub_call_record_imported' WHEN 'text_messages' THEN 'fub_text_record_imported' END;
  EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I f JOIN migration_history_import_identity i ON i.id=f.identity_id AND i.organization_id=f.organization_id JOIN migration_history_import_display d ON d.id=COALESCE(f.manifest_id,f.admitted_manifest_id) AND d.organization_id=f.organization_id WHERE f.id=$1 AND f.organization_id=$2 AND f.person_id=$3 AND i.fact_id=f.id AND i.identity_hmac=$4 AND i.semantic_hmac=$5 AND i.erased_at IS NULL)',table_name) INTO exists_fact USING NEW.fact_id,NEW.organization_id,m.person_id,m.identity_hmac,m.semantic_hmac;
  IF NOT exists_fact THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_result_fact_missing';END IF;
 ELSIF NEW.fact_id IS NOT NULL OR (NEW.disposition='equal_repeat' AND m.disposition<>'equal_repeat') OR (NEW.disposition='excluded' AND m.disposition<>'excluded') OR (NEW.disposition='held' AND NEW.reason IS NULL) THEN
  RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_result_invalid';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER admitted_history_result_insert BEFORE INSERT ON migration_admitted_history_result FOR EACH ROW EXECUTE FUNCTION crm_admitted_history_result_insert();
REVOKE ALL ON FUNCTION crm_admitted_history_result_insert() FROM PUBLIC;
ALTER TABLE migration_admitted_history_result ADD CHECK(reason IS NULL OR reason IN ('invalid_identity','ambiguous_relationship','conflicting_variants','out_of_cohort','target_missing','target_erased','target_identity_mismatch','identity_erased','identity_conflict','fact_or_display_missing','capture_integrity','source_binding_changed'));
