-- READ-ONLY AUDIT EXPORT, NOT A MIGRATION.
-- pg_get_functiondef from shared development at source 82b185d, 2026-09-17.
-- Do not apply this file to a database.

-- crm_activity_complete_read
CREATE OR REPLACE FUNCTION public.crm_activity_complete_read(org uuid)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
 PERFORM crm_workspace_shared(org);
 IF EXISTS(SELECT 1 FROM organization o WHERE o.id=org AND o.workspace_mode='migration_review' AND (EXISTS(SELECT 1 FROM migration_activity_import i WHERE i.organization_id=o.id AND i.confirmed_plan_id IS NOT NULL) OR EXISTS(SELECT 1 FROM migration_admitted_activity_import i WHERE i.organization_id=o.id AND i.confirmed_plan_id IS NOT NULL)))
 THEN RAISE EXCEPTION USING ERRCODE='P010F',MESSAGE='activity_review_required'; END IF;
END $function$


-- crm_activity_insert_allowed
CREATE OR REPLACE FUNCTION public.crm_activity_insert_allowed(org uuid, table_name text, row_value jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_activity_measure_row
CREATE OR REPLACE FUNCTION public.crm_activity_measure_row()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE delta BIGINT; child UUID; org UUID; row_value JSONB;
BEGIN
 row_value:=COALESCE(to_jsonb(NEW),to_jsonb(OLD));
 delta:=crm_activity_retained_size(TG_TABLE_NAME,CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)-crm_activity_retained_size(TG_TABLE_NAME,CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta=0 THEN RETURN COALESCE(NEW,OLD); END IF;
 org:=(row_value->>'organization_id')::uuid;
 IF TG_TABLE_NAME='migration_activity_identity' AND row_value->>'refresh_plan_id' IS NOT NULL THEN UPDATE migration_family_refresh_plan SET measured_bytes=measured_bytes+delta WHERE id=(row_value->>'refresh_plan_id')::uuid AND organization_id=org; IF NOT FOUND THEN RAISE EXCEPTION 'refresh activity accounting owner missing'; END IF; RETURN COALESCE(NEW,OLD); END IF; IF TG_TABLE_NAME='migration_activity_identity' AND row_value->>'admitted_import_id' IS NOT NULL THEN
   child:=(row_value->>'admitted_import_id')::uuid;
   UPDATE migration_admitted_activity_import SET measured_bytes=measured_bytes+delta WHERE id=child AND organization_id=org;
 ELSE
   child:=(row_value->>CASE WHEN TG_TABLE_NAME='migration_activity_import' THEN 'id' ELSE 'import_id' END)::uuid;
   UPDATE migration_activity_import SET measured_bytes=measured_bytes+delta WHERE id=child AND organization_id=org;
 END IF;
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_activity_retained_size
CREATE OR REPLACE FUNCTION public.crm_activity_retained_size(table_name text, v jsonb)
 RETURNS bigint
 LANGUAGE plpgsql
 IMMUTABLE
AS $function$
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
END $function$


-- crm_admitted_activity_insert_allowed
CREATE OR REPLACE FUNCTION public.crm_admitted_activity_insert_allowed(org uuid, table_name text, row_value jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
AS $function$
DECLARE token TEXT:=current_setting('crm.admitted_activity_token',true); unit_text TEXT:=current_setting('crm.admitted_activity_unit',true); unit UUID;
BEGIN
 IF table_name NOT IN ('note','task') OR token IS NULL OR token='' OR unit_text IS NULL OR unit_text='' THEN RETURN false; END IF;
 BEGIN unit:=unit_text::uuid; EXCEPTION WHEN invalid_text_representation THEN RETURN false; END;
 RETURN EXISTS(SELECT 1 FROM migration_admitted_activity_manifest a
 JOIN migration_admitted_activity_import i ON i.id=a.import_id AND i.organization_id=a.organization_id AND i.confirmed_plan_id=a.plan_id
 JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id
 JOIN migration_import p ON p.id=i.parent_import_id AND p.organization_id=i.organization_id AND p.confirmed_plan_id=i.parent_plan_id
 JOIN organization o ON o.id=i.organization_id
 JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id
 JOIN migration_people_admission ad ON ad.id=i.admission_id AND ad.organization_id=i.organization_id AND ad.confirmed_admission_plan_id=i.admission_plan_id
 JOIN migration_people_admission_result r ON r.id=a.admission_result_id AND r.organization_id=a.organization_id AND r.admission_id=i.admission_id AND r.person_id=a.person_id AND r.source_id=a.source_person_id
 JOIN migration_import_identity d ON d.organization_id=i.organization_id AND d.source_account_id=i.source_account_id AND d.family='people' AND d.source_id=a.source_person_id AND d.target_id=a.person_id AND d.admission_id=i.admission_id AND d.admission_result_id=r.id
 JOIN person n ON n.id=a.person_id AND n.organization_id=a.organization_id
 WHERE a.id=unit AND a.organization_id=org AND a.disposition='eligible' AND a.kind=table_name
 AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp()
 AND p.state='completed' AND ad.state IN ('completed','cancelled') AND o.workspace_mode='migration_review' AND o.workspace_revision=i.workspace_revision
 AND m.role='admin' AND m.status='active' AND r.disposition='settled'
 AND a.target_id=(row_value->>'id')::uuid AND a.person_id=(row_value->>'person_id')::uuid
 AND row_value->>'origin'='migration' AND row_value->>'source'='fub' AND a.native_source_key=row_value->>'source_external_id'
 AND row_value->>'deleted_at' IS NULL AND row_value->>'deleted_by_user_id' IS NULL
 AND ((table_name='note' AND a.author_user_id IS NOT DISTINCT FROM (row_value->>'author_user_id')::uuid)
 OR(table_name='task' AND a.creator_user_id IS NOT DISTINCT FROM (row_value->>'created_by_user_id')::uuid
 AND a.assignee_user_id IS NOT DISTINCT FROM (row_value->>'assignee_user_id')::uuid AND row_value->>'completed_by_user_id' IS NULL
 AND (a.assignee_user_id IS NULL OR EXISTS(SELECT 1 FROM organization_membership am WHERE am.organization_id=org AND am.user_id=a.assignee_user_id AND am.status='active')))));
END $function$


-- crm_admitted_activity_measure_row
CREATE OR REPLACE FUNCTION public.crm_admitted_activity_measure_row()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE delta BIGINT; child UUID; org UUID;
BEGIN
 delta:=crm_admitted_activity_retained_size(TG_TABLE_NAME,CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)-crm_admitted_activity_retained_size(TG_TABLE_NAME,CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta<>0 THEN
  child:=(COALESCE(to_jsonb(NEW),to_jsonb(OLD))->>CASE WHEN TG_TABLE_NAME='migration_admitted_activity_import' THEN 'id' ELSE 'import_id' END)::uuid;
  org:=(COALESCE(to_jsonb(NEW),to_jsonb(OLD))->>'organization_id')::uuid;
  UPDATE migration_admitted_activity_import SET measured_bytes=measured_bytes+delta WHERE id=child AND organization_id=org;
 END IF;
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_admitted_activity_retained_size
CREATE OR REPLACE FUNCTION public.crm_admitted_activity_retained_size(table_name text, v jsonb)
 RETURNS bigint
 LANGUAGE plpgsql
 IMMUTABLE
AS $function$
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
END $function$


-- crm_admitted_history_evidence_insert
CREATE OR REPLACE FUNCTION public.crm_admitted_history_evidence_insert()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE v JSONB;r migration_admitted_history_root%ROWTYPE;
BEGIN
 v:=to_jsonb(NEW);
 SELECT r0.* INTO r FROM migration_admitted_history_root r0 JOIN migration_admitted_history_plan p ON p.id=(v->>'plan_id')::uuid AND p.root_id=r0.id AND p.organization_id=r0.organization_id WHERE r0.id=(v->>'root_id')::uuid AND r0.organization_id=NEW.organization_id AND p.state='building';
 IF r.id IS NULL OR r.confirmed_plan_id IS NOT NULL OR r.state<>'running' OR r.phase NOT IN ('preparing','classifying') OR r.lease_expires_at<=clock_timestamp() THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_evidence_frozen';END IF;
 IF TG_TABLE_NAME='migration_admitted_history_manifest' AND NOT EXISTS(SELECT 1 FROM migration_history_observation o JOIN migration_history_capture c ON c.id=o.capture_id AND c.organization_id=o.organization_id AND c.run_id=o.run_id WHERE o.id=(v->>'observation_id')::uuid AND o.organization_id=NEW.organization_id AND o.run_id=r.history_capture_id AND o.capture_id=(v->>'capture_id')::uuid AND o.ordinal=(v->>'ordinal')::integer AND o.family=v->>'family' AND c.classification='advancing') THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_evidence_invalid';END IF;
 RETURN NEW;
END $function$


-- crm_admitted_history_fact_allowed
CREATE OR REPLACE FUNCTION public.crm_admitted_history_fact_allowed(v jsonb, table_name text)
 RETURNS boolean
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_admitted_history_identity_erasure
CREATE OR REPLACE FUNCTION public.crm_admitted_history_identity_erasure()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 IF OLD.erased_at IS NULL AND NEW.erased_at IS NOT NULL THEN
  UPDATE migration_admitted_history_manifest SET nonce=NULL,ciphertext=NULL WHERE organization_id=NEW.organization_id AND identity_hmac=NEW.identity_hmac AND nonce IS NOT NULL;
  UPDATE migration_admitted_history_root r SET revision=revision+1 WHERE r.organization_id=NEW.organization_id AND EXISTS(SELECT 1 FROM migration_admitted_history_manifest m WHERE m.root_id=r.id AND m.organization_id=r.organization_id AND m.identity_hmac=NEW.identity_hmac);
 END IF;RETURN NEW;
END $function$


-- crm_admitted_history_immutable
CREATE OR REPLACE FUNCTION public.crm_admitted_history_immutable()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_admitted_history_measure
CREATE OR REPLACE FUNCTION public.crm_admitted_history_measure()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE delta BIGINT;v JSONB;root UUID;org UUID;
BEGIN
 delta:=crm_admitted_history_retained_size(CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)-crm_admitted_history_retained_size(CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta=0 THEN RETURN COALESCE(NEW,OLD); END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD));org:=(v->>'organization_id')::uuid;
 root:=(v->>CASE WHEN TG_TABLE_NAME='migration_admitted_history_root' THEN 'id' ELSE 'root_id' END)::uuid;
 UPDATE migration_admitted_history_root SET retained_bytes=retained_bytes+delta WHERE id=root AND organization_id=org;
 UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+delta WHERE organization_id=org;
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_admitted_history_owner_insert
CREATE OR REPLACE FUNCTION public.crm_admitted_history_owner_insert()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_admitted_history_person_erasure
CREATE OR REPLACE FUNCTION public.crm_admitted_history_person_erasure()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=OLD.organization_id AND person_id=OLD.id AND erased_at IS NULL;
 DELETE FROM migration_history_import_display d USING migration_admitted_history_manifest m WHERE d.id=m.id AND d.admitted_root_id=m.root_id AND d.organization_id=m.organization_id AND m.organization_id=OLD.organization_id AND m.person_id=OLD.id;
 UPDATE migration_admitted_history_manifest SET nonce=NULL,ciphertext=NULL WHERE organization_id=OLD.organization_id AND person_id=OLD.id AND nonce IS NOT NULL;
 RETURN OLD;
END $function$


-- crm_admitted_history_result_insert
CREATE OR REPLACE FUNCTION public.crm_admitted_history_result_insert()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_admitted_history_retained_size
CREATE OR REPLACE FUNCTION public.crm_admitted_history_retained_size(v jsonb)
 RETURNS bigint
 LANGUAGE plpgsql
 IMMUTABLE
AS $function$
DECLARE total BIGINT:=0;k TEXT;value TEXT;
BEGIN
 IF v IS NULL THEN RETURN 0; END IF;
 FOR k,value IN SELECT e.key,e.value #>> '{}' FROM jsonb_each(v) e LOOP
  IF value IS NULL THEN CONTINUE; END IF;
  IF k IN ('nonce','ciphertext','identity_hmac','semantic_hmac','input_digest','binding_hmac') THEN total:=total+octet_length(decode(substring(value FROM 3),'hex'));
  ELSIF k IN ('source_binding','coverage','counts','result_counts','stream_progress','representation','pause_reason','code') THEN total:=total+octet_length(value); END IF;
 END LOOP; RETURN total;
END $function$


-- crm_admitted_history_suppress
CREATE OR REPLACE FUNCTION public.crm_admitted_history_suppress()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
DECLARE h BYTEA;
BEGIN
 IF OLD.admitted_root_id IS NULL THEN RETURN OLD; END IF;
 SELECT identity_hmac INTO h FROM migration_admitted_history_manifest WHERE id=OLD.id AND root_id=OLD.admitted_root_id AND organization_id=OLD.organization_id;
 UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=OLD.organization_id AND identity_hmac=h AND erased_at IS NULL;
 UPDATE migration_admitted_history_manifest SET nonce=NULL,ciphertext=NULL WHERE organization_id=OLD.organization_id AND identity_hmac=h AND nonce IS NOT NULL;
 UPDATE migration_admitted_history_root r SET revision=revision+1 WHERE organization_id=OLD.organization_id AND EXISTS(SELECT 1 FROM migration_admitted_history_manifest m WHERE m.root_id=r.id AND m.organization_id=r.organization_id AND m.identity_hmac=h);
 RETURN OLD;
END $function$


-- crm_admitted_history_write_fence
CREATE OR REPLACE FUNCTION public.crm_admitted_history_write_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE org UUID;
BEGIN
 org:=(COALESCE(to_jsonb(NEW),to_jsonb(OLD))->>'organization_id')::uuid;
 PERFORM crm_workspace_shared(org);
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_admitted_metadata_insert_allowed
CREATE OR REPLACE FUNCTION public.crm_admitted_metadata_insert_allowed(org uuid, permit text, row_value jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE lease UUID; unit UUID; root UUID; plan UUID; account BIGINT; target UUID; claim_kind TEXT; claim_key TEXT; native_table TEXT;
BEGIN
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'unit_id')::uuid; root:=(permit::jsonb->>'root_id')::uuid; plan:=(permit::jsonb->>'plan_id')::uuid; target:=(permit::jsonb->>'target_id')::uuid; claim_kind:=permit::jsonb->>'claim_kind'; claim_key:=permit::jsonb->>'claim_key'; native_table:=permit::jsonb->>'native_table'; EXCEPTION WHEN others THEN RETURN false; END;
 IF native_table NOT IN ('tag','custom_field','custom_field_option','person_tag','person_custom_field_value') THEN RETURN false; END IF;
 SELECT i.source_account_id INTO account FROM migration_admitted_metadata_import i
 JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id
 JOIN migration_import p ON p.id=i.parent_import_id AND p.organization_id=org AND p.confirmed_plan_id=i.parent_plan_id AND p.state='completed'
 JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=org AND a.parent_import_id=p.id AND a.parent_plan_id=p.confirmed_plan_id AND a.state IN ('completed','cancelled')
 JOIN organization o ON o.id=org AND o.workspace_revision=i.workspace_revision AND o.workspace_mode='migration_review'
 JOIN organization_membership m ON m.organization_id=org AND m.user_id=i.executor_user_id AND m.role='admin' AND m.status='active'
 JOIN migration_metadata_catalog_readiness r ON r.organization_id=org AND r.state='ready'
 WHERE i.id=root AND i.organization_id=org AND i.confirmed_plan_id=plan AND i.state='running' AND i.lease_token=lease AND i.lease_expires_at>clock_timestamp() AND i.engine_version='fub-admitted-metadata-v1';
 IF account IS NULL OR claim_key IS NULL OR claim_kind IS NULL THEN RETURN false; END IF;
 IF native_table IN ('tag','custom_field','custom_field_option') THEN
 RETURN EXISTS(SELECT 1 FROM migration_admitted_metadata_mapping x JOIN migration_metadata_catalog_claim c ON c.organization_id=org AND c.source_account_id=account AND c.kind=x.kind AND c.source_key=x.source_key AND c.target_id=x.target_id WHERE x.id=unit AND x.import_id=root AND x.plan_id=plan AND x.organization_id=org AND x.kind=claim_kind AND x.target_id=target AND encode(x.source_key,'hex')=claim_key AND x.disposition='create_matching' AND (row_value->>'id')::uuid=target AND ((native_table='tag' AND x.kind='tag') OR(native_table='custom_field' AND x.kind='field') OR(native_table='custom_field_option' AND x.kind='option' AND x.target_field_id=(row_value->>'field_id')::uuid)));
 END IF;
 RETURN EXISTS(SELECT 1 FROM migration_admitted_metadata_manifest m
 JOIN migration_admitted_metadata_operation x ON x.manifest_id=m.id AND x.import_id=root AND x.plan_id=plan AND x.organization_id=org
 JOIN migration_admitted_metadata_mapping a ON a.id=x.mapping_id AND a.plan_id=plan AND a.organization_id=org AND a.kind=claim_kind AND encode(a.source_key,'hex')=claim_key
 JOIN migration_metadata_catalog_claim c ON c.organization_id=org AND c.source_account_id=account AND c.kind=a.kind AND c.source_key=a.source_key AND c.target_id=x.target_id
 JOIN migration_people_admission_result ar ON ar.id=m.admission_result_id AND ar.organization_id=org AND ar.person_id=m.person_id AND ar.disposition='settled'
 JOIN migration_import_identity i ON i.organization_id=org AND i.source_account_id=account AND i.family='people' AND i.source_id=m.source_person_id AND i.target_id=m.person_id AND i.admission_result_id=ar.id AND i.admission_id=ar.admission_id AND i.admission_item_id=ar.item_id
 WHERE m.id=unit AND m.import_id=root AND m.plan_id=plan AND m.organization_id=org AND m.disposition='eligible' AND x.target_id=target AND x.disposition='eligible' AND (row_value->>'person_id')::uuid=m.person_id AND ((native_table='person_tag' AND x.kind='tag_link' AND a.kind='tag' AND (row_value->>'tag_id')::uuid=target) OR(native_table='person_custom_field_value' AND x.kind='value' AND a.kind='field' AND (row_value->>'field_id')::uuid=target)));
END $function$


-- crm_admitted_metadata_mapping_counts
CREATE OR REPLACE FUNCTION public.crm_admitted_metadata_mapping_counts()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF EXISTS(SELECT 1 FROM migration_admitted_metadata_plan WHERE id=NEW.plan_id AND organization_id=NEW.organization_id AND state='building') THEN
  IF TG_TABLE_NAME='migration_admitted_metadata_alias' THEN
   UPDATE migration_admitted_metadata_mapping SET alias_count=alias_count+1 WHERE id=NEW.mapping_id AND plan_id=NEW.plan_id AND organization_id=NEW.organization_id;
  ELSE
   UPDATE migration_admitted_metadata_mapping SET dependent_count=dependent_count+1 WHERE id=NEW.mapping_id AND plan_id=NEW.plan_id AND organization_id=NEW.organization_id;
  END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_admitted_metadata_owner_keys
CREATE OR REPLACE FUNCTION public.crm_admitted_metadata_owner_keys()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p RECORD; r RECORD; owner UUID; base UUID; source UUID; inherited UUID;
BEGIN
 IF TG_TABLE_NAME='migration_admitted_metadata_plan' THEN
  SELECT * INTO r FROM migration_admitted_metadata_import WHERE id=NEW.import_id AND organization_id=NEW.organization_id;
  NEW.admission_id:=COALESCE(NEW.admission_id,r.admission_id);
  NEW.parent_import_id:=COALESCE(NEW.parent_import_id,r.parent_import_id);
  NEW.parent_plan_id:=COALESCE(NEW.parent_plan_id,r.parent_plan_id);
  NEW.source_account_id:=COALESCE(NEW.source_account_id,r.source_account_id);
  IF NEW.remainder_plan_id IS NOT NULL OR NEW.evidence_plan_id IS NOT NULL THEN
   IF NEW.remainder_plan_id IS NULL OR NEW.evidence_plan_id IS NULL OR r.predecessor_import_id IS NULL THEN
    RAISE EXCEPTION 'admitted_metadata_plan_lineage' USING ERRCODE='23503';
   END IF;
   -- Both references must be confirmed plans of this actual predecessor chain.
   -- A cancelled partial copy may deliberately refer to an earlier ancestor.
   IF EXISTS(SELECT 1 FROM unnest(ARRAY[NEW.remainder_plan_id,NEW.evidence_plan_id]) wanted
     WHERE NOT EXISTS(WITH RECURSIVE ancestors AS (
       SELECT id,predecessor_import_id,confirmed_plan_id FROM migration_admitted_metadata_import WHERE id=r.predecessor_import_id AND organization_id=NEW.organization_id
       UNION SELECT i.id,i.predecessor_import_id,i.confirmed_plan_id FROM migration_admitted_metadata_import i JOIN ancestors a ON i.id=a.predecessor_import_id WHERE i.organization_id=NEW.organization_id
     ) SELECT 1 FROM ancestors WHERE confirmed_plan_id=wanted)) THEN
    RAISE EXCEPTION 'admitted_metadata_plan_lineage' USING ERRCODE='23503';
   END IF;
  END IF;
 ELSIF TG_TABLE_NAME IN ('migration_admitted_metadata_source','migration_admitted_metadata_observation') THEN
  SELECT snapshot_id INTO owner FROM migration_admitted_metadata_plan WHERE id=NEW.plan_id AND import_id=NEW.import_id AND organization_id=NEW.organization_id;
  NEW.snapshot_id:=COALESCE(NEW.snapshot_id,owner);
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_manifest' THEN
  SELECT admission_id,remainder_plan_id INTO p FROM migration_admitted_metadata_plan WHERE id=NEW.plan_id AND import_id=NEW.import_id AND organization_id=NEW.organization_id;
  NEW.admission_id:=COALESCE(NEW.admission_id,p.admission_id);
  SELECT item_id INTO owner FROM migration_people_admission_result WHERE id=NEW.admission_result_id AND admission_id=NEW.admission_id AND organization_id=NEW.organization_id;
  NEW.admission_item_id:=COALESCE(NEW.admission_item_id,owner);
  IF NEW.predecessor_manifest_id IS NOT NULL THEN NEW.predecessor_plan_id:=COALESCE(NEW.predecessor_plan_id,p.remainder_plan_id); END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_mapping' THEN
  SELECT remainder_plan_id,evidence_plan_id INTO p FROM migration_admitted_metadata_plan WHERE id=NEW.plan_id AND import_id=NEW.import_id AND organization_id=NEW.organization_id;
  IF NEW.predecessor_mapping_id IS NOT NULL THEN NEW.predecessor_plan_id:=COALESCE(NEW.predecessor_plan_id,p.remainder_plan_id); END IF;
  IF NEW.source_mapping_id IS NOT NULL THEN NEW.evidence_plan_id:=COALESCE(NEW.evidence_plan_id,p.evidence_plan_id); END IF;
  IF NEW.dependency_result_id IS NOT NULL THEN
   SELECT plan_id,import_id INTO r FROM migration_admitted_metadata_result WHERE id=NEW.dependency_result_id AND organization_id=NEW.organization_id;
   NEW.dependency_plan_id:=COALESCE(NEW.dependency_plan_id,r.plan_id);
   NEW.dependency_import_id:=COALESCE(NEW.dependency_import_id,r.import_id);
   SELECT COALESCE(m.dependency_result_id,x.id) INTO inherited FROM migration_admitted_metadata_mapping m
    LEFT JOIN migration_admitted_metadata_result x ON x.unit_id=m.id AND x.plan_id=m.plan_id AND x.import_id=m.import_id AND x.organization_id=m.organization_id
    WHERE m.id=NEW.predecessor_mapping_id AND m.plan_id=p.remainder_plan_id AND m.organization_id=NEW.organization_id;
   IF inherited IS DISTINCT FROM NEW.dependency_result_id THEN RAISE EXCEPTION 'admitted_metadata_dependency_owner' USING ERRCODE='23503'; END IF;
  END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_receipt' THEN
  SELECT id INTO owner FROM migration_admitted_metadata_plan WHERE import_id=NEW.import_id AND organization_id=NEW.organization_id AND snapshot_id=NEW.snapshot_id ORDER BY revision DESC LIMIT 1;
  NEW.plan_id:=COALESCE(NEW.plan_id,owner);
 END IF;
 RETURN NEW;
END $function$


-- crm_admitted_metadata_preparation_bytes
CREATE OR REPLACE FUNCTION public.crm_admitted_metadata_preparation_bytes()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE added BIGINT; prior BIGINT:=0;
BEGIN
 IF TG_TABLE_NAME='migration_admitted_metadata_source' THEN
  added:=octet_length(NEW.source_id)+octet_length(NEW.semantic_hmac)+octet_length(NEW.nonce)+octet_length(NEW.ciphertext);
  IF TG_OP='UPDATE' THEN prior:=octet_length(OLD.source_id)+octet_length(OLD.semantic_hmac)+octet_length(OLD.nonce)+octet_length(OLD.ciphertext); END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_mapping' THEN
  added:=octet_length(NEW.source_key)+octet_length(NEW.source_id)+octet_length(NEW.nonce)+octet_length(NEW.ciphertext)+COALESCE(octet_length(NEW.field_name_key),0);
  IF TG_OP='UPDATE' THEN prior:=octet_length(OLD.source_key)+octet_length(OLD.source_id)+octet_length(OLD.nonce)+octet_length(OLD.ciphertext)+COALESCE(octet_length(OLD.field_name_key),0); END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_manifest' THEN
  added:=octet_length(NEW.source_person_id)+octet_length(NEW.baseline_nonce)+octet_length(NEW.baseline_ciphertext);
  IF TG_OP='UPDATE' THEN prior:=octet_length(OLD.source_person_id)+octet_length(OLD.baseline_nonce)+octet_length(OLD.baseline_ciphertext); END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_operation' THEN
  added:=octet_length(NEW.source_key)+octet_length(NEW.nonce)+octet_length(NEW.ciphertext);
  IF TG_OP='UPDATE' THEN prior:=octet_length(OLD.source_key)+octet_length(OLD.nonce)+octet_length(OLD.ciphertext); END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_observation' THEN added:=octet_length(NEW.semantic_hmac);
 ELSE RAISE EXCEPTION 'unexpected_preparation_byte_table'; END IF;
 UPDATE migration_admitted_metadata_plan SET preparation_bytes=preparation_bytes+added-prior
 WHERE id=NEW.plan_id AND organization_id=NEW.organization_id AND state='building';
 RETURN NEW;
END $function$


-- crm_admitted_people_refresh_mutation_allowed
CREATE OR REPLACE FUNCTION public.crm_admitted_people_refresh_mutation_allowed(org uuid, permit text, table_name text, operation text, old_row jsonb, new_row jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE lease UUID; unit UUID; target UUID; actor UUID;
BEGIN
 IF NOT crm_mapping_repair_mutation_evidence(org,'migration_admitted_people_refresh',permit,table_name) THEN RETURN false; END IF;
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.person_id,r.initiated_by_user_id INTO target,actor FROM migration_admitted_people_refresh_item i JOIN migration_admitted_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id AND r.confirmed_refresh_plan_id=i.plan_id AND i.source_account_id=r.source_account_id JOIN migration_people_admission_result ar ON ar.id=i.admission_result_id AND ar.admission_id=r.admission_id AND ar.organization_id=i.organization_id AND ar.person_id=i.person_id AND ar.disposition='settled' JOIN migration_import_identity mi ON mi.organization_id=i.organization_id AND mi.source_account_id=i.source_account_id AND mi.family='people' AND mi.source_id=i.source_id AND mi.target_id=i.person_id AND mi.admission_id=r.admission_id AND mi.admission_item_id=i.admission_item_id AND mi.admission_result_id=ar.id JOIN migration_admitted_people_refresh_baseline b ON b.organization_id=i.organization_id AND b.admission_id=r.admission_id AND b.source_id=i.source_id AND b.person_id=i.person_id AND b.admission_result_id=ar.id AND b.version=i.baseline_version AND b.result_id IS NOT DISTINCT FROM i.baseline_result_id JOIN migration_workspace w ON w.organization_id=r.organization_id AND w.import_id=r.parent_import_id AND w.plan_id=r.parent_plan_id JOIN organization o ON o.id=r.organization_id AND o.workspace_revision=r.workspace_revision JOIN organization_membership m ON m.organization_id=r.organization_id AND m.user_id=r.initiated_by_user_id WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND r.state='running' AND r.lease_token=lease AND r.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF target IS NULL THEN RETURN false; END IF;
 IF table_name='person' THEN RETURN operation='UPDATE' AND (old_row->>'id')::uuid=target AND (new_row->>'id')::uuid=target AND (old_row->>'organization_id')::uuid=org AND (new_row->>'organization_id')::uuid=org AND (old_row - ARRAY['first_name','last_name','stage_id','assigned_user_id','updated_at','stage_revision','mobile_revision','details_revision'])=(new_row - ARRAY['first_name','last_name','stage_id','assigned_user_id','updated_at','stage_revision','mobile_revision','details_revision']); END IF;
 IF table_name='contact_method' THEN RETURN CASE operation WHEN 'INSERT' THEN (new_row->>'person_id')::uuid=target AND (new_row->>'organization_id')::uuid=org AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side='proposed' AND c.contact_id=(new_row->>'id')::uuid AND c.kind=new_row->>'kind') WHEN 'UPDATE' THEN (old_row->>'id')=(new_row->>'id') AND (old_row->>'person_id')::uuid=target AND (new_row->>'person_id')::uuid=target AND (old_row->>'organization_id')=(new_row->>'organization_id') AND (old_row->>'kind')=(new_row->>'kind') AND (old_row - ARRAY['value','normalized_value','import_order'])=(new_row - ARRAY['value','normalized_value','import_order']) AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side IN ('baseline','current') AND c.contact_id=(old_row->>'id')::uuid AND c.kind=old_row->>'kind') WHEN 'DELETE' THEN (old_row->>'person_id')::uuid=target AND (old_row->>'organization_id')::uuid=org AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side IN ('baseline','current') AND c.contact_id=(old_row->>'id')::uuid AND c.kind=old_row->>'kind') ELSE false END; END IF;
 IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (new_row->>'person_id')::uuid=target AND new_row->>'actor_kind'='system' AND new_row->>'origin'='migration' AND new_row->>'reason'='migration_refresh' AND (new_row->>'on_behalf_of_user_id')::uuid=actor; END IF;
 RETURN false;
END $function$


-- crm_family_history_display_guard
CREATE OR REPLACE FUNCTION public.crm_family_history_display_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF TG_OP='INSERT' THEN
  IF NEW.nonce IS NULL OR NOT EXISTS(SELECT 1 FROM migration_history_import_identity WHERE id=NEW.identity_id AND organization_id=NEW.organization_id AND erased_at IS NULL AND fact_id IS NOT NULL) THEN RAISE EXCEPTION 'history display identity unavailable'; END IF;
 ELSE
  IF to_jsonb(NEW)-ARRAY['nonce','ciphertext'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['nonce','ciphertext'] OR OLD.nonce IS NULL OR NEW.nonce IS NOT NULL
   OR NOT EXISTS(SELECT 1 FROM migration_history_import_identity WHERE id=NEW.identity_id AND organization_id=NEW.organization_id AND erased_at IS NOT NULL) THEN RAISE EXCEPTION 'history display only erasable'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_history_erasure
CREATE OR REPLACE FUNCTION public.crm_family_history_erasure()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
DECLARE r RECORD;
BEGIN
 IF OLD.erased_at IS NOT NULL OR NEW.erased_at IS NULL THEN RETURN NEW; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_history_display WHERE organization_id=NEW.organization_id AND identity_id=NEW.id AND nonce IS NOT NULL) THEN RETURN NEW; END IF;
 -- All retained displays were settled before this administrative erasure.
 -- Negative deltas refund their original plan and Org exactly once, including
 -- completed plans. The immutable fixed-width ownership survives suppression.
 PERFORM organization_id FROM migration_snapshot_storage WHERE organization_id=NEW.organization_id FOR UPDATE;
 FOR r IN SELECT d.plan_id,sum(octet_length(d.nonce)+octet_length(d.ciphertext)) AS bytes
  FROM migration_family_refresh_history_display d WHERE d.organization_id=NEW.organization_id AND d.identity_id=NEW.id AND d.nonce IS NOT NULL GROUP BY d.plan_id ORDER BY d.plan_id LOOP
  PERFORM id FROM migration_family_refresh_plan WHERE id=r.plan_id AND organization_id=NEW.organization_id AND measured_bytes=retained_bytes FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'history erasure evidence unsettled'; END IF;
  UPDATE migration_family_refresh_history_display SET nonce=NULL,ciphertext=NULL WHERE organization_id=NEW.organization_id AND identity_id=NEW.id AND plan_id=r.plan_id AND nonce IS NOT NULL;
  UPDATE migration_family_refresh_plan SET retained_bytes=retained_bytes-r.bytes WHERE id=r.plan_id AND organization_id=NEW.organization_id;
  UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes-r.bytes WHERE organization_id=NEW.organization_id;
  UPDATE migration_history_capture_run h SET retained_bytes=h.retained_bytes-r.bytes
   FROM migration_family_refresh_plan p WHERE p.id=r.plan_id AND p.organization_id=NEW.organization_id
    AND h.id=p.history_capture_id AND h.organization_id=p.organization_id;
 END LOOP;
 RETURN NEW;
END $function$


-- crm_family_history_first_allowed
CREATE OR REPLACE FUNCTION public.crm_family_history_first_allowed(v jsonb, table_name text)
 RETURNS boolean
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle;
 u migration_family_refresh_manifest; s migration_family_refresh_source; i migration_history_import_identity; family_name TEXT;
BEGIN
 PERFORM crm_workspace_shared((v->>'organization_id')::uuid);
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=(v->>'refresh_bundle_id')::uuid AND organization_id=(v->>'organization_id')::uuid;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=(v->>'refresh_plan_id')::uuid AND bundle_id=b.id AND organization_id=b.organization_id;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=(v->>'refresh_manifest_id')::uuid AND plan_id=p.id AND bundle_id=b.id AND organization_id=b.organization_id;
 IF b.id IS NULL OR p.id IS NULL OR u.id IS NULL OR b.confirmed_at IS NULL OR p.confirmed_at IS NULL
  OR b.state NOT IN ('queued','running','paused') OR p.state<>'running' OR p.phase<>'apply' OR p.family<>'history' OR p.cancel_requested
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR u.position<>p.apply_position+1 OR u.disposition<>'insert' OR u.expected_head_id IS NOT NULL OR u.expected_revision IS NOT NULL
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=b.organization_id)
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active')
  OR NOT EXISTS(SELECT 1 FROM migration_workspace w JOIN organization o ON o.id=w.organization_id WHERE w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id AND o.workspace_mode='migration_review')
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE organization_id=b.organization_id AND bundle_id=b.id AND plan_id=p.id AND manifest_id=u.id AND disposition='applied' AND person_id=u.person_id AND target_id=u.target_id)
 THEN RETURN false; END IF;
 SELECT * INTO s FROM migration_family_refresh_source WHERE id=u.source_row_id AND bundle_id=b.id AND organization_id=b.organization_id;
 family_name:=CASE u.kind WHEN 'event' THEN 'events' WHEN 'call' THEN 'calls' WHEN 'text' THEN 'text_messages' END;
 IF s.id IS NULL OR family_name IS NULL OR NOT s.qualified OR s.kind<>u.kind OR s.identity_hmac IS DISTINCT FROM u.source_key_hmac
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_cohort c JOIN person person ON person.id=c.person_id AND person.organization_id=c.organization_id
   JOIN migration_import_identity pi ON pi.organization_id=c.organization_id AND pi.target_id=c.person_id AND pi.family='people' AND pi.source_account_id=b.source_account_id AND pi.source_id=c.source_person_id
   WHERE c.id=u.cohort_id AND c.bundle_id=b.id AND c.organization_id=b.organization_id AND c.person_id=u.person_id AND c.source_person_id=s.source_person_id
    AND pi.admission_result_id IS NOT DISTINCT FROM c.admission_result_id
    AND (c.original_result_id IS NULL OR EXISTS(SELECT 1 FROM migration_import_result r WHERE r.id=c.original_result_id AND r.organization_id=c.organization_id AND r.import_id=pi.import_id AND r.plan_id=pi.plan_id AND r.person_id=c.person_id AND r.source_id=c.source_person_id AND r.disposition='imported')))
  OR NOT EXISTS(SELECT 1 FROM migration_history_capture raw JOIN migration_history_capture_run selected ON selected.id=raw.run_id AND selected.organization_id=raw.organization_id
   WHERE raw.id=s.capture_id AND raw.organization_id=b.organization_id AND raw.sequence=s.capture_sequence AND raw.run_id=p.history_capture_id AND raw.family=family_name AND raw.classification='advancing' AND NOT raw.truncated
    AND raw.representation=s.representation AND selected.state='completed_with_gaps' AND selected.completed_at IS NOT NULL AND selected.parent_import_id=b.parent_import_id AND selected.source_account_id=b.source_account_id)
  OR EXISTS(SELECT 1 FROM migration_family_refresh_source sibling WHERE sibling.bundle_id=b.id AND sibling.organization_id=b.organization_id AND sibling.kind=u.kind AND sibling.identity_hmac=s.identity_hmac
   AND (NOT sibling.qualified OR sibling.semantic_hmac<>s.semantic_hmac OR sibling.source_person_id IS DISTINCT FROM s.source_person_id OR sibling.representation<>s.representation))
 THEN RETURN false; END IF;
 IF table_name='migration_history_import_identity' THEN
  RETURN (v->>'person_id')::uuid=u.person_id AND (v->>'fact_id')::uuid=u.target_id AND v->>'family'=family_name AND v->>'erased_at' IS NULL
   AND (v->>'identity_hmac')::bytea=s.identity_hmac AND (v->>'semantic_hmac')::bytea=s.semantic_hmac;
 END IF;
 SELECT * INTO i FROM migration_history_import_identity WHERE organization_id=b.organization_id AND refresh_bundle_id=b.id AND refresh_plan_id=p.id AND refresh_manifest_id=u.id AND identity_hmac=s.identity_hmac AND semantic_hmac=s.semantic_hmac AND person_id=u.person_id AND fact_id=u.target_id AND erased_at IS NULL;
 IF i.id IS NULL THEN RETURN false; END IF;
 IF table_name='migration_history_import_display' THEN RETURN (v->>'id')::uuid=u.id; END IF;
 RETURN table_name='fub_'||u.kind||'_record_imported' AND (v->>'identity_id')::uuid=i.id AND (v->>'id')::uuid=u.target_id AND (v->>'person_id')::uuid=u.person_id
  AND v->>'actor_kind'='user' AND (v->>'actor_user_id')::uuid=b.executor_user_id AND v->>'origin'='migration' AND v->>'on_behalf_of_user_id' IS NULL AND v->>'corrects_id' IS NULL
  AND (v->>'stable_position')::bigint=u.position
  AND EXISTS(SELECT 1 FROM migration_history_import_display d WHERE d.organization_id=b.organization_id AND d.id=u.id AND d.refresh_plan_id=p.id AND d.refresh_bundle_id=b.id);
END $function$


-- crm_family_history_first_erasure
CREATE OR REPLACE FUNCTION public.crm_family_history_first_erasure()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 IF OLD.erased_at IS NULL AND NEW.erased_at IS NOT NULL AND NEW.refresh_bundle_id IS NOT NULL THEN
  DELETE FROM migration_history_import_display WHERE organization_id=NEW.organization_id AND id=NEW.refresh_manifest_id AND refresh_bundle_id=NEW.refresh_bundle_id AND refresh_plan_id=NEW.refresh_plan_id;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_history_first_insert
CREATE OR REPLACE FUNCTION public.crm_family_history_first_insert()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF NEW.refresh_bundle_id IS NOT NULL AND NOT COALESCE(crm_family_history_first_allowed(to_jsonb(NEW),TG_TABLE_NAME),false) THEN RAISE EXCEPTION 'family history first owner invalid'; END IF;
 RETURN NEW;
END $function$


-- crm_family_history_first_suppress
CREATE OR REPLACE FUNCTION public.crm_family_history_first_suppress()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 IF OLD.refresh_bundle_id IS NOT NULL THEN
  UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=OLD.organization_id AND refresh_bundle_id=OLD.refresh_bundle_id AND refresh_plan_id=OLD.refresh_plan_id AND refresh_manifest_id=OLD.id AND erased_at IS NULL;
 END IF;
 RETURN OLD;
END $function$


-- crm_family_history_head_guard
CREATE OR REPLACE FUNCTION public.crm_family_history_head_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE i migration_history_import_identity; f RECORD; p migration_family_refresh_plan; table_name TEXT; v RECORD;
BEGIN
 SELECT * INTO i FROM migration_history_import_identity WHERE id=NEW.identity_id AND organization_id=NEW.organization_id FOR UPDATE;
 IF i.id IS NULL OR i.erased_at IS NOT NULL OR i.person_id IS DISTINCT FROM NEW.person_id OR i.fact_id IS DISTINCT FROM NEW.original_fact_id OR i.family<>NEW.family
  OR NOT EXISTS(SELECT 1 FROM person WHERE id=NEW.person_id AND organization_id=NEW.organization_id) THEN RAISE EXCEPTION 'history head identity unavailable'; END IF;
 table_name:=CASE i.family WHEN 'events' THEN 'fub_event_record' WHEN 'calls' THEN 'fub_call_record' WHEN 'text_messages' THEN 'fub_text_record' END;
 IF TG_OP='INSERT' THEN
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
  IF p.id IS NULL OR p.family<>'history' OR NEW.version<>1 OR NEW.semantic_hmac<>i.semantic_hmac THEN RAISE EXCEPTION 'invalid history bootstrap'; END IF;
  EXECUTE format('SELECT f.person_id,f.source_created_at,COALESCE(p.capture_id,a.history_capture_id,rp.history_capture_id) AS capture_id FROM %I f
   JOIN migration_history_import_display d ON d.id=COALESCE(f.manifest_id,f.admitted_manifest_id,f.refresh_manifest_id) AND d.organization_id=f.organization_id
   LEFT JOIN migration_history_import_plan p ON p.id=f.plan_id AND p.organization_id=f.organization_id
   LEFT JOIN migration_admitted_history_root a ON a.id=f.admitted_root_id AND a.organization_id=f.organization_id
   LEFT JOIN migration_family_refresh_plan rp ON rp.id=f.refresh_plan_id AND rp.bundle_id=f.refresh_bundle_id AND rp.organization_id=f.organization_id
   WHERE f.id=$1 AND f.identity_id=$2 AND f.organization_id=$3 AND
    ((f.attempt_id=$4 AND f.admitted_root_id IS NULL AND d.owner_run_id=$4 AND d.plan_id=f.plan_id)
     OR (f.attempt_id IS NULL AND f.admitted_root_id=$5 AND f.admitted_plan_id=$6 AND f.admitted_attempt_id=$7 AND f.admitted_manifest_id=$8
      AND d.admitted_root_id=$5 AND d.admitted_plan_id=$6 AND d.admitted_attempt_id=$7)
     OR (f.refresh_bundle_id=$9 AND f.refresh_plan_id=$10 AND f.refresh_manifest_id=$11 AND d.refresh_bundle_id=$9 AND d.refresh_plan_id=$10 AND d.refresh_manifest_id=$11))',table_name||'_imported')
   INTO f USING i.fact_id,i.id,i.organization_id,i.owner_run_id,i.admitted_root_id,i.admitted_plan_id,i.admitted_attempt_id,i.admitted_manifest_id,i.refresh_bundle_id,i.refresh_plan_id,i.refresh_manifest_id;
  IF f.capture_id IS NULL OR f.capture_id IS DISTINCT FROM NEW.capture_id OR f.person_id IS DISTINCT FROM NEW.person_id OR f.source_created_at IS DISTINCT FROM NEW.source_created_at THEN RAISE EXCEPTION 'unproven history bootstrap'; END IF;
  IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_bundle b JOIN migration_history_capture_run c ON c.parent_import_id=b.parent_import_id AND c.source_account_id=b.source_account_id AND c.organization_id=b.organization_id WHERE b.id=NEW.bundle_id AND b.organization_id=NEW.organization_id AND c.id=NEW.capture_id) THEN RAISE EXCEPTION 'foreign history bootstrap'; END IF;
 ELSE
  IF to_jsonb(NEW)-ARRAY['version','version_id','result_id','capture_id','semantic_hmac','source_created_at'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['version','version_id','result_id','capture_id','semantic_hmac','source_created_at']
   OR NEW.version<>OLD.version+1 THEN RAISE EXCEPTION 'history head ownership immutable'; END IF;
  EXECUTE format('SELECT * FROM %I WHERE id=$1 AND identity_id=$2 AND organization_id=$3',table_name||'_corrected') INTO v USING NEW.version_id,NEW.identity_id,NEW.organization_id;
  IF v.id IS NULL OR v.corrects_id IS DISTINCT FROM OLD.version_id OR v.version<>NEW.version OR v.result_id IS DISTINCT FROM NEW.result_id OR v.capture_id IS DISTINCT FROM NEW.capture_id OR v.semantic_hmac IS DISTINCT FROM NEW.semantic_hmac OR v.source_created_at IS DISTINCT FROM NEW.source_created_at THEN RAISE EXCEPTION 'history head predecessor mismatch'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_history_version_apply
CREATE OR REPLACE FUNCTION public.crm_family_history_version_apply()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
DECLARE h migration_family_refresh_history_head; key TEXT;
BEGIN
 SELECT * INTO h FROM migration_family_refresh_history_head WHERE organization_id=NEW.organization_id AND identity_id=NEW.identity_id FOR UPDATE;
 UPDATE migration_family_refresh_history_head SET version=NEW.version,version_id=NEW.id,result_id=NEW.result_id,capture_id=NEW.capture_id,semantic_hmac=NEW.semantic_hmac,source_created_at=NEW.source_created_at
 WHERE organization_id=NEW.organization_id AND identity_id=NEW.identity_id AND version=NEW.version-1 AND version_id IS NOT DISTINCT FROM NEW.corrects_id;
 IF NOT FOUND THEN RAISE EXCEPTION 'history correction lost predecessor'; END IF;
 key:=replace(TG_TABLE_NAME,'_corrected','_imported');
 IF (h.source_created_at IS NULL)<>(NEW.source_created_at IS NULL) THEN
  PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,key||CASE WHEN h.source_created_at IS NULL THEN '_unknown' ELSE '_known' END,-1);
  PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,key||CASE WHEN NEW.source_created_at IS NULL THEN '_unknown' ELSE '_known' END,1);
 ELSE
  PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,NULL,0);
 END IF;
 RETURN NEW;
END $function$


-- crm_family_history_version_guard
CREATE OR REPLACE FUNCTION public.crm_family_history_version_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE h migration_family_refresh_history_head; i migration_history_import_identity; r migration_family_refresh_result; u migration_family_refresh_manifest;
 p migration_family_refresh_plan; b migration_family_refresh_bundle; s migration_family_refresh_source; v_kind TEXT; v_family TEXT;
BEGIN
 PERFORM crm_workspace_shared(NEW.organization_id);
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=b.id AND organization_id=NEW.organization_id FOR SHARE;
 IF p.id IS NULL OR p.family<>'history' OR p.state<>'running' OR p.confirmed_at IS NULL OR b.confirmed_at IS NULL OR b.state NOT IN ('queued','running','paused')
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=NEW.organization_id)
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=NEW.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN RAISE EXCEPTION 'history correction lease invalid'; END IF;
 SELECT * INTO i FROM migration_history_import_identity WHERE id=NEW.identity_id AND organization_id=NEW.organization_id FOR UPDATE;
 SELECT * INTO h FROM migration_family_refresh_history_head WHERE identity_id=NEW.identity_id AND organization_id=NEW.organization_id FOR UPDATE;
 v_kind:=CASE TG_TABLE_NAME WHEN 'fub_event_record_corrected' THEN 'event' WHEN 'fub_call_record_corrected' THEN 'call' ELSE 'text' END;
 v_family:=CASE v_kind WHEN 'event' THEN 'events' WHEN 'call' THEN 'calls' ELSE 'text_messages' END;
 IF i.id IS NULL OR i.erased_at IS NOT NULL OR h.identity_id IS NULL OR h.family<>v_family OR h.person_id<>NEW.person_id OR h.original_fact_id<>NEW.original_fact_id
  OR NEW.corrects_id IS DISTINCT FROM h.version_id OR NEW.version<>h.version+1 OR NEW.semantic_hmac=h.semantic_hmac THEN RAISE EXCEPTION 'history correction predecessor invalid'; END IF;
 SELECT * INTO r FROM migration_family_refresh_result WHERE id=NEW.result_id AND organization_id=NEW.organization_id;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=NEW.manifest_id AND plan_id=p.id AND organization_id=NEW.organization_id;
 SELECT * INTO s FROM migration_family_refresh_source WHERE id=u.source_row_id AND bundle_id=b.id AND organization_id=NEW.organization_id;
 IF current_user='crm_app' AND (p.cancel_requested OR p.phase<>'apply' OR u.position<>p.apply_position+1 OR NEW.actor_kind<>'user' OR NEW.actor_user_id IS DISTINCT FROM b.executor_user_id OR NEW.on_behalf_of_user_id IS NOT NULL) THEN RAISE EXCEPTION 'history correction execution unit invalid'; END IF;
 IF r.id IS NULL OR u.id IS NULL OR s.id IS NULL OR r.manifest_id<>u.id OR r.disposition<>'applied' OR r.person_id IS DISTINCT FROM NEW.person_id OR r.target_id IS DISTINCT FROM NEW.original_fact_id
  OR u.disposition<>'correction' OR u.kind<>v_kind OR u.person_id IS DISTINCT FROM NEW.person_id OR u.target_id IS DISTINCT FROM NEW.original_fact_id OR u.expected_head_id IS DISTINCT FROM h.result_id
  OR u.source_key_hmac<>i.identity_hmac OR NOT s.qualified OR s.kind<>v_kind OR s.identity_hmac IS DISTINCT FROM i.identity_hmac OR s.semantic_hmac<>NEW.semantic_hmac
  OR NEW.capture_id IS DISTINCT FROM p.history_capture_id THEN RAISE EXCEPTION 'history correction proof invalid'; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_cohort c JOIN person person ON person.id=c.person_id AND person.organization_id=c.organization_id
  JOIN migration_import_identity pi ON pi.organization_id=c.organization_id AND pi.target_id=c.person_id AND pi.family='people' AND pi.source_account_id=b.source_account_id AND pi.source_id=c.source_person_id
  WHERE c.id=u.cohort_id AND c.bundle_id=b.id AND c.organization_id=NEW.organization_id AND c.person_id=NEW.person_id AND c.source_person_id=s.source_person_id) THEN RAISE EXCEPTION 'history correction Person unavailable'; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_history_capture raw JOIN migration_history_capture_run selected ON selected.id=raw.run_id AND selected.organization_id=raw.organization_id
  JOIN migration_history_capture_run prior ON prior.id=h.capture_id AND prior.organization_id=selected.organization_id
  WHERE raw.id=s.capture_id AND raw.organization_id=NEW.organization_id AND raw.sequence=s.capture_sequence AND raw.run_id=NEW.capture_id AND raw.family=v_family AND raw.classification='advancing' AND NOT raw.truncated
   AND raw.representation=s.representation AND selected.state='completed_with_gaps' AND selected.completed_at IS NOT NULL AND prior.completed_at IS NOT NULL AND selected.started_at>prior.completed_at
   AND selected.source_account_id=prior.source_account_id AND selected.parent_import_id=prior.parent_import_id) THEN RAISE EXCEPTION 'history correction source not newer'; END IF;
 IF EXISTS(SELECT 1 FROM migration_family_refresh_source sibling WHERE sibling.plan_id=p.id AND sibling.organization_id=NEW.organization_id AND sibling.kind=v_kind AND sibling.identity_hmac=i.identity_hmac
  AND (NOT sibling.qualified OR sibling.semantic_hmac<>s.semantic_hmac OR sibling.source_person_id IS DISTINCT FROM s.source_person_id OR sibling.representation<>s.representation)) THEN RAISE EXCEPTION 'history correction source conflict'; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_history_display d WHERE d.id=NEW.id AND d.organization_id=NEW.organization_id AND d.identity_id=NEW.identity_id AND d.result_id=NEW.result_id AND d.plan_id=NEW.plan_id AND d.bundle_id=NEW.bundle_id AND d.nonce IS NOT NULL) THEN RAISE EXCEPTION 'history correction display unavailable'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_accounting_complete
CREATE OR REPLACE FUNCTION public.crm_family_refresh_accounting_complete()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE v JSONB; b UUID; org UUID;
BEGIN
 IF current_user<>'crm_app' THEN RETURN NULL; END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD)); org:=(v->>'organization_id')::uuid;
 b:=(v->>CASE WHEN TG_TABLE_NAME='migration_family_refresh_bundle' THEN 'id' ELSE 'bundle_id' END)::uuid;
 IF EXISTS(SELECT 1 FROM migration_family_refresh_bundle WHERE id=b AND organization_id=org AND (payer_plan_id IS NULL OR shared_measured_bytes<>shared_retained_bytes))
  OR EXISTS(SELECT 1 FROM migration_family_refresh_plan p WHERE p.bundle_id=b AND p.organization_id=org
    AND (p.measured_bytes<>p.retained_bytes OR p.reserved_bytes<>COALESCE((SELECT sum(r.byte_count) FROM migration_family_refresh_reservation r WHERE r.plan_id=p.id AND r.organization_id=p.organization_id),0))) THEN
  RAISE EXCEPTION 'family accounting transaction incomplete';
 END IF;
 RETURN NULL;
END $function$


-- crm_family_refresh_activity_owned_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_activity_owned_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; candidate RECORD;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 IF OLD.family<>'activity' THEN
  IF ROW(NEW.owned_activity_kind,NEW.owned_activity_source_id) IS DISTINCT FROM ROW(OLD.owned_activity_kind,OLD.owned_activity_source_id) THEN RAISE EXCEPTION 'activity cursor on another family'; END IF;
  RETURN NEW;
 END IF;
 IF NEW.owned_after IS DISTINCT FROM OLD.owned_after THEN RAISE EXCEPTION 'history cursor on activity family'; END IF;
 IF ROW(NEW.owned_activity_kind,NEW.owned_activity_source_id,NEW.owned_walk_complete) IS NOT DISTINCT FROM ROW(OLD.owned_activity_kind,OLD.owned_activity_source_id,OLD.owned_walk_complete) THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR OLD.phase<>'classify' OR NOT OLD.source_walk_complete OR OLD.owned_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active')
  THEN RAISE EXCEPTION 'stale activity ownership walk'; END IF;
 IF ROW(NEW.owned_activity_kind,NEW.owned_activity_source_id) IS DISTINCT FROM ROW(OLD.owned_activity_kind,OLD.owned_activity_source_id) THEN
  SELECT * INTO candidate FROM crm_family_refresh_next_owned_activity(OLD.organization_id,OLD.bundle_id,OLD.owned_activity_kind,OLD.owned_activity_source_id);
  IF candidate.kind IS NULL OR ROW(candidate.kind,candidate.source_id) IS DISTINCT FROM ROW(NEW.owned_activity_kind,NEW.owned_activity_source_id)
   THEN RAISE EXCEPTION 'activity ownership walk skipped an identity'; END IF;
  IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_source s JOIN migration_family_refresh_manifest m ON m.source_row_id=s.id AND m.bundle_id=s.bundle_id AND m.organization_id=s.organization_id
   WHERE s.bundle_id=OLD.bundle_id AND s.organization_id=OLD.organization_id AND s.kind=candidate.kind AND s.source_id=candidate.source_id AND m.plan_id=OLD.id AND m.kind=candidate.kind)
   AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest m WHERE m.plan_id=OLD.id AND m.organization_id=OLD.organization_id AND m.kind=candidate.kind
    AND m.source_row_id IS NULL AND m.source_id=candidate.source_id AND m.cohort_id=candidate.cohort_id AND m.person_id=candidate.person_id AND m.disposition='held')
   THEN RAISE EXCEPTION 'activity ownership outcome missing'; END IF;
 END IF;
 IF NEW.owned_walk_complete AND EXISTS(SELECT 1 FROM crm_family_refresh_next_owned_activity(OLD.organization_id,OLD.bundle_id,NEW.owned_activity_kind,NEW.owned_activity_source_id)) THEN RAISE EXCEPTION 'activity ownership walk incomplete'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_activity_owner
CREATE OR REPLACE FUNCTION public.crm_family_refresh_activity_owner()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; u migration_family_refresh_manifest; native_row JSONB;
BEGIN
 IF NEW.refresh_plan_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.refresh_bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.refresh_plan_id AND bundle_id=b.id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=NEW.refresh_manifest_id AND plan_id=p.id AND bundle_id=b.id AND organization_id=NEW.organization_id;
 IF b.id IS NULL OR p.id IS NULL OR u.id IS NULL OR b.confirmed_at IS NULL OR p.confirmed_at IS NULL OR p.state<>'running' OR p.cancel_requested
  OR p.family<>'activity' OR p.phase<>'apply' OR b.state NOT IN ('queued','running','paused')
  OR p.lease_token IS NULL OR p.lease_expires_at IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR u.kind<>NEW.kind OR u.disposition<>'insert' OR u.position<>p.apply_position+1 OR u.target_id IS DISTINCT FROM NEW.target_id
  OR NEW.source_account_id<>b.source_account_id
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_source s WHERE s.id=u.source_row_id AND s.bundle_id=b.id AND s.organization_id=b.organization_id AND s.kind=NEW.kind AND s.source_id=NEW.source_id AND s.qualified)
  OR NOT EXISTS(SELECT 1 FROM migration_workspace w WHERE w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id)
  OR NOT EXISTS(SELECT 1 FROM organization_membership a WHERE a.organization_id=b.organization_id AND a.user_id=b.executor_user_id AND a.role='admin' AND a.status='active')
  OR EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE manifest_id=u.id AND organization_id=u.organization_id)
  THEN RAISE EXCEPTION 'unbound refresh activity identity'; END IF;
 IF NEW.kind='note' THEN SELECT to_jsonb(n) INTO native_row FROM note n WHERE n.id=NEW.target_id AND n.organization_id=NEW.organization_id;
 ELSE SELECT to_jsonb(t) INTO native_row FROM task t WHERE t.id=NEW.target_id AND t.organization_id=NEW.organization_id; END IF;
 IF native_row IS NULL OR (native_row->>'person_id')::uuid IS DISTINCT FROM u.person_id
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_write_proof w WHERE w.manifest_id=u.id AND w.plan_id=p.id AND w.organization_id=p.organization_id AND w.table_name=NEW.kind AND w.operation='INSERT' AND w.target_id=NEW.target_id AND w.after_hash=crm_family_refresh_native_digest(native_row))
  THEN RAISE EXCEPTION 'refresh activity identity native proof missing'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_activity_walk_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_activity_walk_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF OLD.family<>'activity' OR (NEW.checkpoint_id IS NOT DISTINCT FROM OLD.checkpoint_id AND NEW.source_walk_complete=OLD.source_walk_complete) THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR NOT OLD.mappings_complete
  OR OLD.phase NOT IN ('mappings','classify') OR OLD.source_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale activity source walk'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND
  (NEW.checkpoint_id IS NULL OR (OLD.checkpoint_id IS NOT NULL AND NEW.checkpoint_id<=OLD.checkpoint_id)
   OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE id=NEW.checkpoint_id AND bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('note','task'))
   OR EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('note','task') AND (OLD.checkpoint_id IS NULL OR id>OLD.checkpoint_id) AND id<NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'activity source walk skipped an occurrence'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND NOT EXISTS(
  SELECT 1 FROM migration_family_refresh_source s JOIN migration_family_refresh_manifest m
   ON m.plan_id=OLD.id AND m.organization_id=s.organization_id AND m.kind=s.kind
  JOIN migration_family_refresh_source original ON original.id=m.source_row_id AND original.bundle_id=s.bundle_id AND original.organization_id=s.organization_id AND original.kind=s.kind
  WHERE s.id=NEW.checkpoint_id AND s.bundle_id=OLD.bundle_id AND s.organization_id=OLD.organization_id AND s.kind IN ('note','task')
   AND (m.source_row_id=s.id OR (s.source_id IS NOT NULL AND original.source_id=s.source_id)))
  THEN RAISE EXCEPTION 'activity source walk outcome missing'; END IF;
 IF NEW.source_walk_complete AND EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('note','task') AND (NEW.checkpoint_id IS NULL OR id>NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'activity source walk incomplete'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_apply_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_apply_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' OR NEW.apply_position=OLD.apply_position THEN RETURN NEW; END IF;
 IF OLD.state<>'running' OR OLD.confirmed_at IS NULL OR OLD.phase<>'apply' OR OLD.cancel_requested
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at IS NULL OR OLD.lease_expires_at<=clock_timestamp()
  OR NEW.apply_position<>OLD.apply_position+1 OR NEW.apply_position>OLD.position
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest u JOIN migration_family_refresh_result r ON r.manifest_id=u.id AND r.organization_id=u.organization_id WHERE u.plan_id=OLD.id AND u.organization_id=OLD.organization_id AND u.position=NEW.apply_position)
  THEN RAISE EXCEPTION 'unsettled family execution checkpoint'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_boundary
CREATE OR REPLACE FUNCTION public.crm_family_refresh_boundary()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 PERFORM set_config('lock_timeout','2000ms',true);
 PERFORM pg_advisory_xact_lock(hashtextextended('crm-workspace-v1:'||NEW.organization_id::text,0));
 NEW.created_at:=clock_timestamp(); NEW.updated_at:=NEW.created_at;
 RETURN NEW;
END $function$


-- crm_family_refresh_bundle_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_bundle_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 PERFORM crm_family_refresh_capability(NEW.organization_id);
 IF TG_OP='INSERT' THEN
  IF NEW.state<>'preparing' OR NEW.confirmed_at IS NOT NULL OR NEW.revision<>1 THEN RAISE EXCEPTION 'invalid initial family bundle'; END IF;
 ELSE
  IF to_jsonb(NEW)-ARRAY['state','revision','confirmed_at','digest','updated_at','executor_user_id','payer_plan_id','shared_measured_bytes','shared_retained_bytes'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['state','revision','confirmed_at','digest','updated_at','executor_user_id','payer_plan_id','shared_measured_bytes','shared_retained_bytes'] THEN RAISE EXCEPTION 'family bundle ownership immutable'; END IF;
  IF OLD.payer_plan_id IS NOT NULL AND NEW.payer_plan_id IS DISTINCT FROM OLD.payer_plan_id THEN RAISE EXCEPTION 'family payer immutable'; END IF; IF OLD.state IN ('completed','cancelled') AND NEW.state<>OLD.state THEN RAISE EXCEPTION 'terminal family bundle'; END IF;
  IF NEW.executor_user_id IS DISTINCT FROM OLD.executor_user_id AND (NEW.executor_user_id::text IS DISTINCT FROM current_setting('crm.family_refresh_adopt_executor',true) OR NEW.revision<>OLD.revision+1 OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=NEW.organization_id AND user_id=NEW.executor_user_id AND role='admin' AND status='active')) THEN RAISE EXCEPTION 'invalid family executor adoption'; END IF; IF NEW.revision<OLD.revision OR NEW.revision>OLD.revision+1 THEN RAISE EXCEPTION 'invalid family bundle revision'; END IF;
  IF OLD.confirmed_at IS NOT NULL AND (NEW.confirmed_at IS DISTINCT FROM OLD.confirmed_at OR NEW.digest IS DISTINCT FROM OLD.digest) THEN RAISE EXCEPTION 'confirmed family bundle immutable'; END IF;
 END IF;
 IF NEW.confirmed_at IS NOT NULL AND (NEW.payer_plan_id IS NULL OR NEW.digest IS NULL OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=NEW.organization_id)) THEN RAISE EXCEPTION 'family confirmation capability missing'; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_workspace w JOIN migration_import i ON i.id=w.import_id AND i.organization_id=w.organization_id
  WHERE w.organization_id=NEW.organization_id AND w.import_id=NEW.parent_import_id AND w.plan_id=NEW.parent_plan_id AND i.state='completed' AND i.source_account_id=NEW.source_account_id AND i.confirmed_plan_id=w.plan_id) THEN RAISE EXCEPTION 'family parent workspace unavailable'; END IF;
 IF NEW.core_report_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_core_change_report r JOIN migration_snapshot s ON s.id=r.newer_snapshot_id AND s.organization_id=r.organization_id
  WHERE r.id=NEW.core_report_id AND r.organization_id=NEW.organization_id AND r.parent_import_id=NEW.parent_import_id AND r.parent_plan_id=NEW.parent_plan_id AND r.source_account_id=NEW.source_account_id AND r.newer_snapshot_id=NEW.core_snapshot_id AND r.state='completed' AND s.state IN ('completed','completed_with_gaps')) THEN RAISE EXCEPTION 'family core source unavailable'; END IF;
 IF NEW.history_capture_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_history_capture_run h WHERE h.id=NEW.history_capture_id AND h.organization_id=NEW.organization_id AND h.parent_import_id=NEW.parent_import_id AND h.parent_plan_id=NEW.parent_plan_id AND h.source_account_id=NEW.source_account_id AND h.state='completed_with_gaps' AND h.completed_at IS NOT NULL AND h.started_at IS NOT NULL
  AND (NEW.core_snapshot_id IS NULL OR h.started_at>(SELECT completed_at FROM migration_snapshot WHERE id=NEW.core_snapshot_id AND organization_id=NEW.organization_id))) THEN RAISE EXCEPTION 'family history source unavailable'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_capability
CREATE OR REPLACE FUNCTION public.crm_family_refresh_capability(org uuid)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=org)
  AND current_setting('crm.family_refresh_reader',true) IS DISTINCT FROM 'fub-family-refresh-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010G',MESSAGE='family_refresh_capability_required';
 END IF;
END $function$


-- crm_family_refresh_catalog_measure
CREATE OR REPLACE FUNCTION public.crm_family_refresh_catalog_measure()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF NEW.refresh_plan_id IS NOT NULL THEN
  UPDATE migration_family_refresh_plan SET measured_bytes=measured_bytes+256+octet_length(NEW.kind)+octet_length(NEW.source_key)+octet_length(NEW.evidence_nonce)+octet_length(NEW.evidence_ciphertext)
   WHERE id=NEW.refresh_plan_id AND organization_id=NEW.organization_id;
  IF NOT FOUND THEN RAISE EXCEPTION 'refresh catalog accounting owner missing'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_catalog_owner
CREATE OR REPLACE FUNCTION public.crm_family_refresh_catalog_owner()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; u migration_family_refresh_manifest; m migration_family_refresh_mapping;
BEGIN
 IF NEW.refresh_plan_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.refresh_bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.refresh_plan_id AND bundle_id=b.id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=NEW.refresh_manifest_id AND plan_id=p.id AND bundle_id=b.id AND organization_id=NEW.organization_id;
 SELECT * INTO m FROM migration_family_refresh_mapping WHERE id=u.mapping_id AND plan_id=p.id AND bundle_id=b.id AND organization_id=NEW.organization_id;
 IF b.id IS NULL OR p.id IS NULL OR u.id IS NULL OR m.id IS NULL OR b.confirmed_at IS NULL OR p.confirmed_at IS NULL OR p.state<>'running' OR p.cancel_requested
  OR p.family<>'metadata' OR p.phase<>'apply' OR b.state NOT IN ('queued','running','paused') OR p.lease_token IS NULL OR p.lease_expires_at IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR u.kind<>'catalog' OR u.disposition NOT IN ('insert','already_current') OR u.position<>p.apply_position+1
  OR NEW.source_account_id<>b.source_account_id OR NEW.kind<>m.kind OR NEW.source_key IS DISTINCT FROM m.source_key_hmac OR NEW.target_id IS DISTINCT FROM m.target_id
  OR NOT EXISTS(SELECT 1 FROM migration_workspace w WHERE w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id)
  OR NOT EXISTS(SELECT 1 FROM organization_membership a WHERE a.organization_id=b.organization_id AND a.user_id=b.executor_user_id AND a.role='admin' AND a.status='active')
  OR EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE manifest_id=u.id AND organization_id=u.organization_id)
  THEN RAISE EXCEPTION 'unbound refresh catalog claim'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_catalog_unit_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_catalog_unit_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; m migration_family_refresh_mapping; parent_unit migration_family_refresh_manifest;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' OR NEW.kind<>'catalog' THEN RETURN NEW; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO m FROM migration_family_refresh_mapping WHERE id=NEW.mapping_id AND plan_id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF p.id IS NULL OR b.id IS NULL OR m.id IS NULL OR p.family<>'metadata' OR p.state<>'preparing' OR p.phase NOT IN ('mappings','classify') OR NOT p.mappings_complete
  OR p.confirmed_at IS NOT NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  OR m.kind NOT IN ('tag','field','option') OR NEW.source_row_id IS DISTINCT FROM m.source_row_id OR NEW.source_key_hmac<>m.source_key_hmac
  OR NEW.cohort_id IS NOT NULL OR NEW.person_id IS NOT NULL OR NEW.baseline_result_id IS NOT NULL OR NEW.expected_head_id IS NOT NULL OR NEW.expected_revision IS NOT NULL
  THEN RAISE EXCEPTION 'invalid family catalog unit binding'; END IF;
 IF m.kind='option' THEN
  SELECT * INTO parent_unit FROM migration_family_refresh_manifest WHERE plan_id=m.plan_id AND organization_id=m.organization_id AND mapping_id=m.parent_id AND kind='catalog';
  IF parent_unit.id IS NULL OR (NEW.disposition<>'held' AND parent_unit.disposition NOT IN ('insert','already_current')) THEN RAISE EXCEPTION 'family catalog prerequisite missing'; END IF;
 END IF;
 IF NEW.disposition='held' THEN
  IF NEW.target_id IS NOT NULL OR NEW.reason IS NULL THEN RAISE EXCEPTION 'invalid held family catalog unit'; END IF;
 ELSIF NOT m.qualified OR (m.kind='tag' AND NOT b.mapping_capture_order) OR NEW.target_id IS DISTINCT FROM m.target_id OR NEW.target_id IS NULL OR NEW.reason IS NOT NULL
  OR NOT ((NEW.disposition='insert' AND m.disposition='create_matching') OR (NEW.disposition='already_current' AND m.disposition='existing'))
  OR NOT EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=b.organization_id AND state='ready' AND engine_version='fub-admitted-metadata-v1')
  THEN RAISE EXCEPTION 'invalid eligible family catalog unit'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_catalog_walk_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_catalog_walk_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; next_id UUID;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' OR ROW(NEW.catalog_after,NEW.catalog_walk_complete) IS NOT DISTINCT FROM ROW(OLD.catalog_after,OLD.catalog_walk_complete) THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL OR OLD.family<>'metadata' OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL
  OR OLD.phase NOT IN ('mappings','classify') OR NOT OLD.mappings_complete OR OLD.catalog_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  OR EXISTS(SELECT 1 FROM migration_family_refresh_mapping WHERE plan_id=OLD.id AND organization_id=OLD.organization_id AND kind IN ('tag','field','option') AND (source_sequence IS NULL OR source_ordinal IS NULL OR source_row_id IS NULL OR source_element IS NULL))
  THEN RAISE EXCEPTION 'stale family catalog walk'; END IF;
 next_id:=crm_family_refresh_next_catalog_mapping(OLD.organization_id,OLD.bundle_id,OLD.id,OLD.catalog_after);
 IF NEW.catalog_after IS DISTINCT FROM OLD.catalog_after THEN
  IF NEW.catalog_after IS NULL OR NEW.catalog_after IS DISTINCT FROM next_id
   OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=OLD.id AND organization_id=OLD.organization_id AND kind='catalog' AND mapping_id=NEW.catalog_after)
   THEN RAISE EXCEPTION 'family catalog walk skipped an outcome'; END IF;
 END IF;
 IF NEW.catalog_walk_complete AND crm_family_refresh_next_catalog_mapping(OLD.organization_id,OLD.bundle_id,OLD.id,NEW.catalog_after) IS NOT NULL
  THEN RAISE EXCEPTION 'family catalog walk incomplete'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_cohort_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_cohort_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  IF NEW.cohort_after IS NOT DISTINCT FROM OLD.cohort_after
   AND NEW.cohort_counts IS NOT DISTINCT FROM OLD.cohort_counts THEN RETURN NEW; END IF;
  IF current_user<>'crm_app' THEN RETURN NEW; END IF;
  p:=OLD;
  SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 ELSE
  SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
  IF NOT EXISTS(SELECT 1 FROM person WHERE id=NEW.person_id AND organization_id=NEW.organization_id)
   OR NOT EXISTS(SELECT 1 FROM migration_import_identity i WHERE i.organization_id=NEW.organization_id
    AND i.source_account_id=b.source_account_id AND i.family='people' AND i.source_id=NEW.source_person_id
    AND i.target_id=NEW.person_id AND i.import_id=b.parent_import_id AND i.created_at<=b.created_at)
   THEN RAISE EXCEPTION 'family cohort outside frozen identity boundary'; END IF;
  IF NEW.admission_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_people_admission a
   WHERE a.id=NEW.admission_id AND a.organization_id=NEW.organization_id
    AND ((a.state='completed' AND a.completed_at<=b.created_at)
      OR (a.state='cancelled' AND a.cancelled_at<=b.created_at)))
   THEN RAISE EXCEPTION 'family cohort outside frozen terminal boundary'; END IF;
  IF current_user<>'crm_app' THEN RETURN NEW; END IF;
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=b.payer_plan_id AND bundle_id=b.id AND organization_id=b.organization_id FOR SHARE;
 END IF;
 IF b.id IS NULL OR p.id IS NULL OR b.payer_plan_id IS DISTINCT FROM p.id
  OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR p.state<>'preparing' OR p.phase<>'cohort' OR p.confirmed_at IS NOT NULL
  OR p.lease_epoch<=0 OR p.lease_token IS NULL
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=b.organization_id
   AND m.user_id=b.executor_user_id AND m.status='active' AND m.role='admin')
  THEN RAISE EXCEPTION 'stale family cohort preparation claim'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_core_index_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_core_index_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; page migration_family_refresh_core_page;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  IF NEW.capture_checkpoint=OLD.capture_checkpoint THEN RETURN NEW; END IF;
  IF NEW.capture_checkpoint<OLD.capture_checkpoint OR OLD.phase<>'capture' THEN RAISE EXCEPTION 'invalid family capture checkpoint'; END IF;
  p:=OLD; IF p.family='history' THEN RETURN NEW; END IF;
 ELSE
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
  IF TG_TABLE_NAME='migration_family_refresh_source' THEN
   IF p.family='history' AND NEW.core_page_id IS NULL THEN RETURN NEW; END IF;
   SELECT * INTO page FROM migration_family_refresh_core_page WHERE id=NEW.core_page_id AND plan_id=p.id AND bundle_id=p.bundle_id AND organization_id=p.organization_id;
   IF page.id IS NULL OR page.capture_id<>NEW.capture_id OR page.capture_sequence<>NEW.capture_sequence THEN RAISE EXCEPTION 'invalid family source page binding'; END IF;
  ELSE
   IF NOT EXISTS(SELECT 1 FROM migration_snapshot_capture c WHERE c.id=NEW.capture_id AND c.snapshot_id=NEW.snapshot_id
    AND c.organization_id=NEW.organization_id AND c.sequence=NEW.capture_sequence AND c.checkpoint=NEW.checkpoint
    AND c.stream=NEW.stream AND c.accepted=NEW.accepted AND c.sequence>p.capture_checkpoint)
    THEN RAISE EXCEPTION 'invalid family capture reference'; END IF;
  END IF;
 END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id FOR SHARE;
 IF b.id IS NULL OR p.id IS NULL OR p.id IS DISTINCT FROM b.payer_plan_id OR p.source_snapshot_id IS DISTINCT FROM b.core_snapshot_id
  OR p.source_snapshot_id IS NULL OR p.family='history' OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR p.state<>'preparing' OR p.phase<>'capture' OR p.confirmed_at IS NOT NULL
  OR p.lease_epoch<=0 OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=b.organization_id AND m.user_id=b.executor_user_id AND m.status='active' AND m.role='admin')
  THEN RAISE EXCEPTION 'stale family capture claim'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_core_page' THEN
  IF NEW.snapshot_id<>p.source_snapshot_id OR NOT EXISTS(SELECT 1 FROM migration_core_change_report r WHERE r.id=b.core_report_id AND r.organization_id=b.organization_id AND r.state='completed' AND r.newer_snapshot_id=NEW.snapshot_id AND r.newer_sequence>=NEW.capture_sequence) THEN RAISE EXCEPTION 'family capture snapshot mismatch'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_head_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_head_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE r migration_family_refresh_result; u migration_family_refresh_manifest; b migration_family_refresh_bundle; p migration_family_refresh_plan;
BEGIN
 PERFORM crm_family_refresh_capability(NEW.organization_id);
 SELECT * INTO r FROM migration_family_refresh_result WHERE id=NEW.result_id AND organization_id=NEW.organization_id;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=r.bundle_id AND organization_id=r.organization_id FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=r.plan_id AND bundle_id=r.bundle_id AND organization_id=r.organization_id FOR SHARE;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=r.manifest_id AND plan_id=r.plan_id AND bundle_id=r.bundle_id AND organization_id=r.organization_id;
 IF r.id IS NULL OR r.disposition NOT IN ('applied','already_current') OR p.state<>'running' OR p.confirmed_at IS NULL
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR u.kind IS DISTINCT FROM NEW.kind OR u.source_key_hmac IS DISTINCT FROM NEW.source_key_hmac OR b.source_account_id IS DISTINCT FROM NEW.source_account_id
  OR r.person_id IS DISTINCT FROM NEW.person_id OR r.target_id IS DISTINCT FROM NEW.target_id OR u.person_id IS DISTINCT FROM NEW.person_id
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=NEW.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN RAISE EXCEPTION 'unproven family head'; END IF;
 IF TG_OP='INSERT' THEN
  IF NEW.version<>1 OR u.expected_head_id IS NOT NULL THEN RAISE EXCEPTION 'stale initial family head'; END IF;
 ELSE
  IF to_jsonb(NEW)-ARRAY['result_id','version'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['result_id','version'] OR NEW.version<>OLD.version+1 OR u.expected_head_id IS DISTINCT FROM OLD.result_id THEN RAISE EXCEPTION 'stale family head'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_head_storage
CREATE OR REPLACE FUNCTION public.crm_family_refresh_head_storage()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 SELECT plan_id INTO NEW.storage_plan_id FROM migration_family_refresh_result WHERE id=NEW.result_id AND organization_id=NEW.organization_id;
 RETURN NEW;
END $function$


-- crm_family_refresh_history_index_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_history_index_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; page migration_family_refresh_history_page;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  IF NEW.family<>'history' OR NEW.capture_checkpoint=OLD.capture_checkpoint THEN RETURN NEW; END IF;
  IF NEW.capture_checkpoint<OLD.capture_checkpoint OR OLD.phase<>'capture' THEN RAISE EXCEPTION 'invalid history checkpoint'; END IF;
  p:=OLD;
 ELSE
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
  IF TG_TABLE_NAME='migration_family_refresh_source' THEN
   IF p.family<>'history' THEN
    IF NEW.history_page_id IS NOT NULL THEN RAISE EXCEPTION 'history page on core source'; END IF;
    RETURN NEW;
   END IF;
   SELECT * INTO page FROM migration_family_refresh_history_page WHERE id=NEW.history_page_id AND plan_id=p.id AND bundle_id=p.bundle_id AND organization_id=p.organization_id;
   IF page.id IS NULL OR page.capture_id<>NEW.capture_id OR page.capture_sequence<>NEW.capture_sequence OR NEW.kind<>(CASE page.stream WHEN 'events' THEN 'event' WHEN 'calls' THEN 'call' ELSE 'text' END) THEN RAISE EXCEPTION 'invalid history source page binding'; END IF;
  ELSE
   IF NEW.run_id IS DISTINCT FROM p.history_capture_id OR NOT EXISTS(SELECT 1 FROM migration_history_capture c WHERE c.id=NEW.capture_id AND c.run_id=NEW.run_id AND c.organization_id=NEW.organization_id AND c.sequence=NEW.capture_sequence AND c.checkpoint=NEW.checkpoint AND c.family=NEW.stream AND (c.classification='advancing')=NEW.accepted AND c.sequence>p.capture_checkpoint) THEN RAISE EXCEPTION 'invalid history capture reference'; END IF;
  END IF;
 END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id FOR SHARE;
 IF b.id IS NULL OR p.id IS NULL OR p.family<>'history' OR p.history_capture_id IS DISTINCT FROM b.history_capture_id THEN RAISE EXCEPTION 'invalid history index owner'; END IF;
 -- Synthetic migrator fixtures may construct execution evidence, but must retain
 -- the exact page/run/source binding above. Application writes require a lease.
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 IF b.state<>'preparing' OR b.confirmed_at IS NOT NULL OR p.state<>'preparing' OR p.phase<>'capture' OR p.confirmed_at IS NOT NULL
  OR p.lease_epoch<=0 OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=b.organization_id AND m.user_id=b.executor_user_id AND m.status='active' AND m.role='admin') THEN RAISE EXCEPTION 'stale history capture claim'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_manifest_source_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_manifest_source_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE s migration_family_refresh_source; c migration_family_refresh_cohort;
BEGIN
 IF NEW.source_row_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO s FROM migration_family_refresh_source WHERE id=NEW.source_row_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF s.id IS NULL OR s.kind<>(CASE NEW.kind WHEN 'metadata' THEN 'person' WHEN 'catalog' THEN s.kind ELSE NEW.kind END) THEN RAISE EXCEPTION 'refresh source kind mismatch'; END IF;
 IF NEW.cohort_id IS NOT NULL THEN
  SELECT * INTO c FROM migration_family_refresh_cohort WHERE id=NEW.cohort_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
  IF c.id IS NULL OR c.person_id IS DISTINCT FROM NEW.person_id OR (NEW.disposition NOT IN ('held','excluded') AND s.source_person_id IS DISTINCT FROM c.source_person_id) THEN RAISE EXCEPTION 'refresh source Person mismatch'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_mapping_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_mapping_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; next_source UUID;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  IF ROW(NEW.mapping_after,NEW.mapping_current,NEW.mapping_offset,NEW.mappings_complete) IS NOT DISTINCT FROM ROW(OLD.mapping_after,OLD.mapping_current,OLD.mapping_offset,OLD.mappings_complete) THEN RETURN NEW; END IF;
  p:=OLD;
 ELSE
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id FOR SHARE;
 IF p.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR p.family NOT IN ('metadata','activity') OR p.state<>'preparing' OR p.phase<>'mappings' OR p.confirmed_at IS NOT NULL OR p.mappings_complete
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan owner WHERE owner.id=b.payer_plan_id AND owner.organization_id=b.organization_id AND owner.phase NOT IN ('cohort','capture'))
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale family mapping inventory'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  next_source:=crm_family_refresh_next_mapping_source(p.organization_id,p.bundle_id,p.family,p.mapping_after);
  IF NEW.mapping_after IS DISTINCT FROM OLD.mapping_after THEN
   IF NEW.mapping_after IS NULL OR NEW.mapping_after IS DISTINCT FROM next_source OR NEW.mapping_current IS NOT NULL OR NEW.mapping_offset<>0
    THEN RAISE EXCEPTION 'family mapping walk skipped a source'; END IF;
  ELSIF NEW.mapping_current IS DISTINCT FROM OLD.mapping_current OR NEW.mapping_offset<>OLD.mapping_offset THEN
   IF NEW.mapping_current IS NULL OR NEW.mapping_current IS DISTINCT FROM next_source OR NEW.mapping_offset<=OLD.mapping_offset OR NEW.mapping_offset>OLD.mapping_offset+50
    THEN RAISE EXCEPTION 'invalid family mapping element progress'; END IF;
  END IF;
  IF NEW.mappings_complete AND (NEW.mapping_current IS NOT NULL OR crm_family_refresh_next_mapping_source(p.organization_id,p.bundle_id,p.family,NEW.mapping_after) IS NOT NULL)
   THEN RAISE EXCEPTION 'family mapping inventory incomplete'; END IF;
 ELSE
  IF (p.family='metadata') IS DISTINCT FROM (NEW.kind IN ('tag','field','option'))
   OR (NEW.kind='option' AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_mapping parent WHERE parent.id=NEW.parent_id AND parent.plan_id=p.id AND parent.organization_id=p.organization_id AND parent.kind='field'))
   OR (NEW.kind<>'option' AND NEW.parent_id IS NOT NULL)
   OR (NEW.kind<>'timezone' AND (NEW.source_row_id IS NULL OR NEW.source_element IS NULL OR NOT EXISTS(
    SELECT 1 FROM migration_family_refresh_source s WHERE s.id=NEW.source_row_id AND s.bundle_id=p.bundle_id AND s.organization_id=p.organization_id
     AND ((NEW.kind='tag' AND s.kind='person') OR (NEW.kind IN ('field','option') AND s.kind='field') OR (NEW.kind='note_author' AND s.kind='note') OR (NEW.kind IN ('task_creator','task_assignee','task_kind') AND s.kind='task')))))
   THEN RAISE EXCEPTION 'family mapping source mismatch'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_mapping_order_projection
CREATE OR REPLACE FUNCTION public.crm_family_refresh_mapping_order_projection()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE s migration_family_refresh_source;
BEGIN
 IF NEW.source_row_id IS NULL THEN
  IF NEW.source_sequence IS NOT NULL OR NEW.source_ordinal IS NOT NULL THEN RAISE EXCEPTION 'unbound family mapping order'; END IF;
  RETURN NEW;
 END IF;
 SELECT * INTO s FROM migration_family_refresh_source WHERE id=NEW.source_row_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF s.id IS NULL OR (NEW.source_sequence IS NOT NULL AND NEW.source_sequence<>s.capture_sequence) OR (NEW.source_ordinal IS NOT NULL AND NEW.source_ordinal<>s.ordinal) THEN RAISE EXCEPTION 'invalid family mapping order'; END IF;
 NEW.source_sequence:=s.capture_sequence; NEW.source_ordinal:=s.ordinal;
 RETURN NEW;
END $function$


-- crm_family_refresh_measure
CREATE OR REPLACE FUNCTION public.crm_family_refresh_measure()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE v JSONB; delta BIGINT; owner_plan UUID; owner_bundle UUID; org UUID;
BEGIN
 delta:=crm_family_refresh_retained_size(CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)
       -crm_family_refresh_retained_size(CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta=0 THEN RETURN COALESCE(NEW,OLD); END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD)); org:=(v->>'organization_id')::uuid;
 IF TG_TABLE_NAME='migration_family_refresh_head' THEN
  owner_plan:=(v->>'storage_plan_id')::uuid;
 ELSIF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  owner_plan:=(v->>'id')::uuid;
 ELSE
  owner_plan:=(v->>'plan_id')::uuid;
 END IF;
 IF owner_plan IS NOT NULL THEN
  UPDATE migration_family_refresh_plan SET measured_bytes=measured_bytes+delta WHERE id=owner_plan AND organization_id=org;
 ELSE
  owner_bundle:=(v->>CASE WHEN TG_TABLE_NAME='migration_family_refresh_bundle' THEN 'id' ELSE 'bundle_id' END)::uuid;
  UPDATE migration_family_refresh_bundle SET shared_measured_bytes=shared_measured_bytes+delta WHERE id=owner_bundle AND organization_id=org;
 END IF;
 IF NOT FOUND THEN RAISE EXCEPTION 'family accounting owner unavailable'; END IF;
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_family_refresh_metadata_owned_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_metadata_owned_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE candidate UUID;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' OR OLD.family<>'metadata' OR ROW(NEW.owned_after,NEW.owned_walk_complete) IS NOT DISTINCT FROM ROW(OLD.owned_after,OLD.owned_walk_complete) THEN RETURN NEW; END IF;
 IF OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR OLD.phase<>'classify' OR NOT OLD.source_walk_complete OR NOT OLD.catalog_walk_complete OR OLD.owned_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_bundle b JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE b.id=OLD.bundle_id AND b.organization_id=OLD.organization_id AND b.state='preparing' AND b.confirmed_at IS NULL AND m.status='active' AND m.role='admin')
  THEN RAISE EXCEPTION 'stale Person metadata ownership walk'; END IF;
 SELECT id INTO candidate FROM migration_family_refresh_cohort WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND (OLD.owned_after IS NULL OR id>OLD.owned_after) ORDER BY id LIMIT 1;
 IF NEW.owned_after IS DISTINCT FROM OLD.owned_after AND (NEW.owned_after IS DISTINCT FROM candidate OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=OLD.id AND organization_id=OLD.organization_id AND kind='metadata' AND cohort_id=NEW.owned_after)) THEN RAISE EXCEPTION 'Person metadata walk skipped outcome'; END IF;
 IF NEW.owned_walk_complete AND EXISTS(SELECT 1 FROM migration_family_refresh_cohort WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND (NEW.owned_after IS NULL OR id>NEW.owned_after)) THEN RAISE EXCEPTION 'Person metadata ownership walk incomplete'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_metadata_walk_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_metadata_walk_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF OLD.family<>'metadata' OR (NEW.checkpoint_id IS NOT DISTINCT FROM OLD.checkpoint_id AND NEW.source_walk_complete=OLD.source_walk_complete) THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR NOT OLD.catalog_walk_complete
  OR OLD.phase NOT IN ('mappings','classify') OR OLD.source_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale metadata source walk'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND
  (NEW.checkpoint_id IS NULL OR (OLD.checkpoint_id IS NOT NULL AND NEW.checkpoint_id<=OLD.checkpoint_id)
   OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE id=NEW.checkpoint_id AND bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind='person')
   OR EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind='person' AND (OLD.checkpoint_id IS NULL OR id>OLD.checkpoint_id) AND id<NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'metadata source walk skipped an occurrence'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND NOT EXISTS(
  SELECT 1 FROM migration_family_refresh_source s JOIN migration_family_refresh_manifest m
   ON m.plan_id=OLD.id AND m.organization_id=s.organization_id AND m.kind='metadata'
  LEFT JOIN migration_family_refresh_source original ON original.id=m.source_row_id AND original.bundle_id=s.bundle_id AND original.organization_id=s.organization_id AND original.kind=s.kind
  WHERE s.id=NEW.checkpoint_id AND s.bundle_id=OLD.bundle_id AND s.organization_id=OLD.organization_id AND s.kind='person'
   AND (m.source_row_id=s.id OR (s.source_id IS NOT NULL AND (original.source_id=s.source_id OR m.source_id=s.source_id))))
  THEN RAISE EXCEPTION 'metadata source walk outcome missing'; END IF;
 IF NEW.source_walk_complete AND EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind='person' AND (NEW.checkpoint_id IS NULL OR id>NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'metadata source walk incomplete'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_missing_activity_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_missing_activity_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; candidate RECORD;
BEGIN
 IF NEW.kind='metadata' OR NEW.source_id IS NULL OR current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 IF p.id IS NULL OR p.family<>'activity' OR p.phase<>'classify' OR p.state<>'preparing' OR p.confirmed_at IS NOT NULL
  OR NOT p.source_walk_complete OR p.owned_walk_complete OR NEW.source_row_id IS NOT NULL OR NEW.disposition<>'held'
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  THEN RAISE EXCEPTION 'invalid missing activity proposal'; END IF;
 SELECT * INTO candidate FROM crm_family_refresh_next_owned_activity(p.organization_id,p.bundle_id,p.owned_activity_kind,p.owned_activity_source_id);
 IF candidate.kind IS NULL OR ROW(NEW.kind,NEW.source_id,NEW.cohort_id,NEW.person_id) IS DISTINCT FROM ROW(candidate.kind,candidate.source_id,candidate.cohort_id,candidate.person_id)
  OR EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=p.bundle_id AND organization_id=p.organization_id AND kind=NEW.kind AND source_id=NEW.source_id)
  THEN RAISE EXCEPTION 'missing activity ownership or absence mismatch'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_mutation_allowed
CREATE OR REPLACE FUNCTION public.crm_family_refresh_mutation_allowed(org uuid, table_name text, operation text, old_row jsonb, new_row jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
AS $function$
DECLARE source_record migration_family_refresh_source; native_metadata_revision BIGINT; proof migration_family_refresh_write_proof; unit migration_family_refresh_manifest; p migration_family_refresh_plan; b migration_family_refresh_bundle; v JSONB; target UUID; columns TEXT[];
BEGIN
 IF NULLIF(current_setting('crm.family_refresh_proof',true),'') IS NULL THEN RETURN false; END IF;
 SELECT * INTO proof FROM migration_family_refresh_write_proof WHERE id::text=current_setting('crm.family_refresh_proof',true) AND organization_id=org;
 IF proof.id IS NULL OR proof.table_name<>table_name OR proof.operation<>operation THEN RETURN false; END IF;
 SELECT * INTO unit FROM migration_family_refresh_manifest WHERE id=proof.manifest_id AND plan_id=proof.plan_id AND bundle_id=proof.bundle_id AND organization_id=org;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=unit.bundle_id AND organization_id=org FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=unit.plan_id AND bundle_id=unit.bundle_id AND organization_id=org FOR SHARE;
 IF p.id IS NULL OR b.id IS NULL OR p.state<>'running' OR p.confirmed_at IS NULL OR b.confirmed_at IS NULL OR b.state NOT IN ('queued','running','paused')
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE manifest_id=unit.id AND organization_id=org)
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=org AND user_id=b.executor_user_id AND role='admin' AND status='active')
  OR NOT EXISTS(SELECT 1 FROM migration_workspace WHERE organization_id=org AND import_id=b.parent_import_id AND plan_id=b.parent_plan_id) THEN RETURN false; END IF;
 IF p.cancel_requested OR p.phase<>'apply' OR unit.position<>p.apply_position+1 THEN RETURN false; END IF; IF unit.disposition NOT IN ('insert','update') THEN RETURN false; END IF;
 IF unit.kind<>'catalog' THEN
  IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_cohort c JOIN migration_import_identity i
   ON i.organization_id=c.organization_id AND i.source_id=c.source_person_id AND i.target_id=c.person_id AND i.family='people'
   WHERE c.id=unit.cohort_id AND c.bundle_id=b.id AND c.organization_id=org AND c.person_id=unit.person_id AND i.source_account_id=b.source_account_id) THEN RETURN false; END IF;
  SELECT metadata_revision INTO native_metadata_revision FROM person WHERE id=unit.person_id AND organization_id=org FOR UPDATE;
  IF native_metadata_revision IS NULL THEN RETURN false; END IF;
  IF (SELECT h.result_id FROM migration_family_refresh_head h WHERE h.organization_id=org AND h.source_account_id=b.source_account_id AND h.kind=unit.kind AND h.source_key_hmac=unit.source_key_hmac)
   IS DISTINCT FROM unit.expected_head_id THEN RETURN false; END IF;
 END IF;
 IF table_name IN ('person_tag','person_custom_field_value') AND native_metadata_revision IS DISTINCT FROM proof.expected_revision THEN RETURN false; END IF;
 IF crm_family_refresh_native_digest(old_row) IS DISTINCT FROM proof.before_hash OR crm_family_refresh_native_digest(new_row) IS DISTINCT FROM proof.after_hash THEN RETURN false; END IF;
 v:=COALESCE(new_row,old_row);
 IF (v->>'organization_id')::uuid IS DISTINCT FROM org THEN RETURN false; END IF;
 target:=CASE table_name WHEN 'person_tag' THEN (v->>'tag_id')::uuid WHEN 'person_custom_field_value' THEN (v->>'field_id')::uuid ELSE (v->>'id')::uuid END;
 IF target IS DISTINCT FROM proof.target_id THEN RETURN false; END IF;
 IF table_name IN ('note','task','person_tag','person_custom_field_value') AND (v->>'person_id')::uuid IS DISTINCT FROM unit.person_id THEN RETURN false; END IF;
 IF table_name IN ('note','task') THEN
  IF unit.kind<>table_name OR v->>'origin' IS DISTINCT FROM 'migration' OR v->>'deleted_at' IS NOT NULL OR target IS DISTINCT FROM unit.target_id THEN RETURN false; END IF;
  SELECT * INTO source_record FROM migration_family_refresh_source WHERE id=unit.source_row_id AND bundle_id=unit.bundle_id AND organization_id=org;
  IF source_record.id IS NULL OR NOT source_record.qualified OR source_record.source_id IS NULL OR source_record.kind<>table_name
   OR v->>'source' IS DISTINCT FROM 'fub' OR v->>'source_external_id' IS DISTINCT FROM ('v1:'||b.source_account_id::text||':'||source_record.source_id) THEN RETURN false; END IF;
  IF table_name='note' AND source_record.representation<>'fub-core-v1/notes/detail/replies,reactions' THEN RETURN false; END IF;
  IF EXISTS(SELECT 1 FROM migration_family_refresh_source sibling LEFT JOIN migration_family_refresh_core_page page ON page.id=sibling.core_page_id AND page.bundle_id=sibling.bundle_id AND page.organization_id=sibling.organization_id WHERE sibling.bundle_id=unit.bundle_id AND sibling.organization_id=org AND sibling.kind=unit.kind AND sibling.source_id=source_record.source_id GROUP BY sibling.representation HAVING count(DISTINCT sibling.semantic_hmac)>1 OR bool_or(NOT sibling.qualified) OR bool_or(sibling.source_person_id IS DISTINCT FROM source_record.source_person_id) OR (table_name='task' AND count(DISTINCT page.stream)>1)) THEN RETURN false; END IF;
  IF operation='UPDATE' AND NOT EXISTS(SELECT 1 FROM migration_activity_identity i WHERE i.organization_id=org AND i.source_account_id=b.source_account_id AND i.kind=unit.kind AND i.source_id=source_record.source_id AND i.target_id=target) THEN RETURN false; END IF;
  IF operation='INSERT' AND EXISTS(SELECT 1 FROM migration_activity_identity i WHERE i.organization_id=org AND i.source_account_id=b.source_account_id AND i.kind=unit.kind AND i.source_id=source_record.source_id) THEN RETURN false; END IF;
 END IF;
 IF table_name IN ('person_tag','person_custom_field_value') AND unit.kind<>'metadata' THEN RETURN false; END IF;
 IF table_name IN ('tag','custom_field','custom_field_option') AND (unit.kind<>'catalog' OR operation<>'INSERT') THEN RETURN false; END IF;
 IF operation='UPDATE' THEN
  columns:=CASE table_name WHEN 'note' THEN ARRAY['body','author_user_id','updated_at','correlation_id','revision']
   WHEN 'task' THEN ARRAY['title','kind','due_at','assignee_user_id','created_by_user_id','completed_at','completed_by_user_id','updated_at','correlation_id','revision']
   WHEN 'person_custom_field_value' THEN ARRAY['text_value','number_value','date_value','option_id','updated_by_user_id','updated_at','origin','correlation_id'] ELSE ARRAY[]::text[] END;
  IF old_row-columns IS DISTINCT FROM new_row-columns THEN RETURN false; END IF;
  IF table_name IN ('note','task') AND ((old_row->>'revision')::bigint IS DISTINCT FROM unit.expected_revision OR proof.expected_revision IS DISTINCT FROM unit.expected_revision) THEN RETURN false; END IF;
 END IF;
 IF table_name='task' AND new_row->>'completed_by_user_id' IS NOT NULL THEN RETURN false; END IF;
 RETURN true;
END $function$


-- crm_family_refresh_native_digest
CREATE OR REPLACE FUNCTION public.crm_family_refresh_native_digest(v jsonb)
 RETURNS bytea
 LANGUAGE sql
 IMMUTABLE
AS $function$
 SELECT CASE WHEN v IS NULL THEN NULL ELSE sha256(convert_to((v-ARRAY['revision','mobile_revision','metadata_revision'])::text,'UTF8')) END
$function$


-- crm_family_refresh_next_catalog_mapping
CREATE OR REPLACE FUNCTION public.crm_family_refresh_next_catalog_mapping(org uuid, bundle uuid, plan uuid, after_id uuid)
 RETURNS uuid
 LANGUAGE sql
 STABLE
AS $function$
 SELECT m.id FROM migration_family_refresh_mapping m WHERE m.organization_id=org AND m.bundle_id=bundle AND m.plan_id=plan
  AND m.kind IN ('tag','field','option') AND m.source_sequence IS NOT NULL AND m.source_ordinal IS NOT NULL AND m.source_row_id IS NOT NULL AND m.source_element IS NOT NULL
  AND (after_id IS NULL OR (m.source_sequence,m.source_ordinal,m.source_row_id,m.source_element,m.id)>
   (SELECT a.source_sequence,a.source_ordinal,a.source_row_id,a.source_element,a.id FROM migration_family_refresh_mapping a WHERE a.id=after_id AND a.plan_id=plan AND a.bundle_id=bundle AND a.organization_id=org))
 ORDER BY m.source_sequence,m.source_ordinal,m.source_row_id,m.source_element,m.id LIMIT 1
$function$


-- crm_family_refresh_next_mapping_source
CREATE OR REPLACE FUNCTION public.crm_family_refresh_next_mapping_source(org uuid, bundle uuid, family text, after_id uuid)
 RETURNS uuid
 LANGUAGE plpgsql
 STABLE
AS $function$
DECLARE capture_order BOOLEAN; next_id UUID;
BEGIN
 SELECT b.mapping_capture_order INTO capture_order FROM migration_family_refresh_bundle b WHERE b.id=bundle AND b.organization_id=org;
 IF capture_order IS NULL THEN RETURN NULL; END IF;
 IF NOT capture_order THEN
  SELECT s.id INTO next_id FROM migration_family_refresh_source s WHERE s.organization_id=org AND s.bundle_id=bundle
   AND ((family='metadata' AND s.kind IN ('person','field')) OR (family='activity' AND s.kind IN ('note','task')))
   AND (after_id IS NULL OR s.id>after_id) ORDER BY s.id LIMIT 1;
 ELSIF family='metadata' THEN
  SELECT s.id INTO next_id FROM migration_family_refresh_source s WHERE s.organization_id=org AND s.bundle_id=bundle
   AND s.kind IN ('person','field') AND (after_id IS NULL OR (s.capture_sequence,s.ordinal,s.id)>
    (SELECT a.capture_sequence,a.ordinal,a.id FROM migration_family_refresh_source a WHERE a.id=after_id AND a.organization_id=org AND a.bundle_id=bundle))
   ORDER BY s.capture_sequence,s.ordinal,s.id LIMIT 1;
 ELSIF family='activity' THEN
  SELECT s.id INTO next_id FROM migration_family_refresh_source s WHERE s.organization_id=org AND s.bundle_id=bundle
   AND s.kind IN ('note','task') AND (after_id IS NULL OR (s.capture_sequence,s.ordinal,s.id)>
    (SELECT a.capture_sequence,a.ordinal,a.id FROM migration_family_refresh_source a WHERE a.id=after_id AND a.organization_id=org AND a.bundle_id=bundle))
   ORDER BY s.capture_sequence,s.ordinal,s.id LIMIT 1;
 END IF;
 RETURN next_id;
END $function$


-- crm_family_refresh_next_owned_activity
CREATE OR REPLACE FUNCTION public.crm_family_refresh_next_owned_activity(org uuid, bundle uuid, after_kind text, after_source text)
 RETURNS TABLE(cohort_id uuid, person_id uuid, kind text, source_id text, target_id uuid)
 LANGUAGE sql
 STABLE
AS $function$
 SELECT * FROM crm_family_refresh_owned_activity(org,bundle) a
 WHERE after_kind IS NULL OR (a.kind,a.source_id)>(after_kind,after_source)
 ORDER BY a.kind,a.source_id LIMIT 1
$function$


-- crm_family_refresh_next_owned_history
CREATE OR REPLACE FUNCTION public.crm_family_refresh_next_owned_history(org uuid, bundle uuid, after_id uuid)
 RETURNS TABLE(identity_id uuid, cohort_id uuid, person_id uuid, kind text, identity_hmac bytea)
 LANGUAGE sql
 STABLE
AS $function$
 SELECT i.id,c.id,c.person_id,
  CASE i.family WHEN 'events' THEN 'event' WHEN 'calls' THEN 'call' ELSE 'text' END,i.identity_hmac
 FROM migration_family_refresh_bundle b
 JOIN migration_family_refresh_cohort c ON c.bundle_id=b.id AND c.organization_id=b.organization_id
 JOIN migration_history_import_identity i ON i.organization_id=c.organization_id AND i.person_id=c.person_id
 WHERE b.id=bundle AND b.organization_id=org AND i.fact_id IS NOT NULL
  AND (after_id IS NULL OR i.id>after_id)
  AND ((i.owner_run_id IS NOT NULL AND c.original_result_id IS NOT NULL AND EXISTS(
   SELECT 1 FROM migration_history_import_run r
   JOIN migration_history_import_plan p ON p.id=r.plan_id AND p.organization_id=r.organization_id
   WHERE r.id=i.owner_run_id AND r.organization_id=org AND r.parent_import_id=b.parent_import_id
    AND p.parent_plan_id=b.parent_plan_id AND p.source_account_id=b.source_account_id
    AND r.confirmed_at IS NOT NULL AND r.state IN ('completed','cancelled')
    AND COALESCE(r.completed_at,r.updated_at)<=b.created_at))
  OR (i.admitted_root_id IS NOT NULL AND c.admission_id IS NOT NULL AND EXISTS(
   SELECT 1 FROM migration_admitted_history_root r WHERE r.id=i.admitted_root_id AND r.organization_id=org
    AND r.parent_import_id=b.parent_import_id AND r.parent_plan_id=b.parent_plan_id
    AND r.source_account_id=b.source_account_id AND r.admission_id=c.admission_id
    AND r.confirmed_plan_id=i.admitted_plan_id AND r.state IN ('completed','cancelled')
    AND COALESCE(r.completed_at,r.updated_at)<=b.created_at))
 OR (i.refresh_bundle_id IS NOT NULL AND EXISTS(
  SELECT 1 FROM migration_family_refresh_bundle owner
  JOIN migration_family_refresh_plan p ON p.id=i.refresh_plan_id AND p.bundle_id=owner.id AND p.organization_id=org
  JOIN migration_family_refresh_manifest m ON m.id=i.refresh_manifest_id AND m.plan_id=p.id AND m.bundle_id=owner.id AND m.organization_id=org
  JOIN migration_family_refresh_cohort co ON co.id=m.cohort_id AND co.bundle_id=owner.id AND co.organization_id=org
  JOIN migration_family_refresh_result r ON r.manifest_id=m.id AND r.plan_id=p.id AND r.organization_id=org
  WHERE owner.id=i.refresh_bundle_id AND owner.organization_id=org AND owner.parent_import_id=b.parent_import_id AND owner.parent_plan_id=b.parent_plan_id AND owner.source_account_id=b.source_account_id
   AND owner.state IN ('completed','cancelled') AND p.state IN ('completed','cancelled') AND p.family='history' AND p.confirmed_at IS NOT NULL AND owner.confirmed_at IS NOT NULL
   AND owner.updated_at<=b.created_at AND r.committed_at<=b.created_at AND m.disposition='insert' AND r.disposition='applied' AND r.person_id=i.person_id AND r.target_id=i.fact_id
   AND co.person_id=c.person_id AND co.source_person_id=c.source_person_id AND co.original_result_id IS NOT DISTINCT FROM c.original_result_id AND co.admission_result_id IS NOT DISTINCT FROM c.admission_result_id AND co.creation_snapshot_id=c.creation_snapshot_id)))
 ORDER BY i.id LIMIT 1
$function$


-- crm_family_refresh_owned_activity
CREATE OR REPLACE FUNCTION public.crm_family_refresh_owned_activity(org uuid, bundle uuid)
 RETURNS TABLE(cohort_id uuid, person_id uuid, kind text, source_id text, target_id uuid)
 LANGUAGE sql
 STABLE
AS $function$
 SELECT c.id,c.person_id,i.kind,i.source_id,i.target_id
 FROM migration_family_refresh_bundle b
 JOIN migration_activity_identity i ON i.organization_id=b.organization_id AND i.source_account_id=b.source_account_id
 JOIN LATERAL (
  SELECT m.person_id,m.source_person_id,m.parent_result_id AS original_result_id,NULL::uuid AS admission_result_id
  FROM migration_activity_import a
  JOIN migration_activity_manifest m ON m.import_id=a.id AND m.organization_id=a.organization_id AND m.plan_id=a.confirmed_plan_id
  JOIN migration_activity_result r ON r.manifest_id=m.id AND r.plan_id=m.plan_id AND r.import_id=m.import_id AND r.organization_id=m.organization_id
  WHERE a.id=i.import_id AND a.organization_id=i.organization_id AND a.confirmed_plan_id=i.plan_id AND m.id=i.manifest_id
   AND a.parent_import_id=b.parent_import_id AND a.parent_plan_id=b.parent_plan_id AND a.source_account_id=b.source_account_id
   AND a.confirmed_at IS NOT NULL AND a.state IN ('completed','cancelled')
   AND (CASE WHEN a.state='completed' THEN a.completed_at ELSE a.updated_at END)<=b.created_at
   AND r.disposition='applied' AND r.kind=i.kind AND m.kind=i.kind AND r.source_id=i.source_id AND m.source_id=i.source_id
   AND r.target_id=i.target_id AND r.person_id=m.person_id AND r.committed_at<=b.created_at
  UNION ALL
  SELECT m.person_id,m.source_person_id,NULL::uuid,m.admission_result_id
  FROM migration_admitted_activity_import a
  JOIN migration_admitted_activity_manifest m ON m.import_id=a.id AND m.organization_id=a.organization_id AND m.plan_id=a.confirmed_plan_id
  JOIN migration_admitted_activity_result r ON r.manifest_id=m.id AND r.plan_id=m.plan_id AND r.import_id=m.import_id AND r.organization_id=m.organization_id
  WHERE a.id=i.admitted_import_id AND a.organization_id=i.organization_id AND a.confirmed_plan_id=i.admitted_plan_id AND m.id=i.admitted_manifest_id
   AND a.parent_import_id=b.parent_import_id AND a.parent_plan_id=b.parent_plan_id AND a.source_account_id=b.source_account_id
   AND a.confirmed_at IS NOT NULL AND a.state IN ('completed','cancelled')
   AND (CASE WHEN a.state='completed' THEN a.completed_at ELSE a.updated_at END)<=b.created_at
   AND r.disposition='applied' AND r.kind=i.kind AND m.kind=i.kind AND r.source_id=i.source_id AND m.source_id=i.source_id
   AND r.target_id=i.target_id AND r.person_id=m.person_id AND r.committed_at<=b.created_at
  UNION ALL
  SELECT m.person_id,prior.source_person_id,prior.original_result_id,prior.admission_result_id
  FROM migration_family_refresh_bundle previous
  JOIN migration_family_refresh_plan p ON p.bundle_id=previous.id AND p.organization_id=previous.organization_id
  JOIN migration_family_refresh_manifest m ON m.plan_id=p.id AND m.bundle_id=p.bundle_id AND m.organization_id=p.organization_id
  JOIN migration_family_refresh_cohort prior ON prior.id=m.cohort_id AND prior.bundle_id=m.bundle_id AND prior.organization_id=m.organization_id
  JOIN migration_family_refresh_result r ON r.manifest_id=m.id AND r.plan_id=p.id AND r.organization_id=m.organization_id
  WHERE previous.id=i.refresh_bundle_id AND p.id=i.refresh_plan_id AND m.id=i.refresh_manifest_id
   AND previous.organization_id=i.organization_id AND previous.parent_import_id=b.parent_import_id AND previous.parent_plan_id=b.parent_plan_id AND previous.source_account_id=b.source_account_id
   AND previous.confirmed_at IS NOT NULL AND previous.state IN ('completed','cancelled') AND previous.updated_at<=b.created_at
   AND p.family='activity' AND p.confirmed_at IS NOT NULL AND p.state IN ('completed','cancelled')
   AND m.disposition='insert' AND m.kind=i.kind AND r.disposition='applied'
   AND r.target_id=i.target_id AND r.person_id=m.person_id AND r.committed_at<=b.created_at
   AND EXISTS(SELECT 1 FROM migration_family_refresh_source s WHERE s.id=m.source_row_id AND s.bundle_id=m.bundle_id AND s.organization_id=m.organization_id AND s.kind=i.kind AND s.source_id=i.source_id)
 ) owner ON true
 JOIN migration_family_refresh_cohort c ON c.bundle_id=b.id AND c.organization_id=b.organization_id
  AND c.person_id=owner.person_id AND c.source_person_id=owner.source_person_id
  AND ((owner.original_result_id IS NOT NULL AND c.original_result_id=owner.original_result_id)
    OR (owner.admission_result_id IS NOT NULL AND c.admission_result_id=owner.admission_result_id))
 WHERE b.id=bundle AND b.organization_id=org
$function$


-- crm_family_refresh_owned_insert
CREATE OR REPLACE FUNCTION public.crm_family_refresh_owned_insert()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; v JSONB;
BEGIN
 v:=to_jsonb(NEW); PERFORM crm_family_refresh_capability(NEW.organization_id);
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 IF b.id IS NULL THEN RAISE EXCEPTION 'family refresh owner missing'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_cohort' THEN
  IF b.confirmed_at IS NOT NULL OR b.state<>'preparing' THEN RAISE EXCEPTION 'family cohort sealed'; END IF;
  IF NEW.original_result_id IS NOT NULL THEN
   IF NOT EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import_identity i ON i.organization_id=r.organization_id AND i.target_id=r.person_id AND i.source_id=r.source_id AND i.family='people'
    WHERE r.id=NEW.original_result_id AND r.organization_id=b.organization_id AND r.import_id=b.parent_import_id AND r.person_id=NEW.person_id AND r.source_id=NEW.source_person_id AND r.disposition IN ('imported','already_imported') AND i.source_account_id=b.source_account_id AND i.import_id=r.import_id AND i.plan_id=r.plan_id AND i.manifest_id=r.manifest_id
    AND EXISTS(SELECT 1 FROM migration_import parent WHERE parent.id=r.import_id AND parent.organization_id=r.organization_id AND parent.state='completed' AND parent.confirmed_plan_id=r.plan_id AND parent.snapshot_id=NEW.creation_snapshot_id)) THEN RAISE EXCEPTION 'unproven original cohort'; END IF;
  ELSE
   IF NOT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_people_admission a ON a.id=r.admission_id AND a.organization_id=r.organization_id
    JOIN migration_import_identity i ON i.organization_id=r.organization_id AND i.admission_result_id=r.id AND i.admission_item_id=r.item_id AND i.admission_id=a.id AND i.target_id=r.person_id
    WHERE r.id=NEW.admission_result_id AND r.admission_id=NEW.admission_id AND r.organization_id=b.organization_id AND r.person_id=NEW.person_id AND r.source_id=NEW.source_person_id AND r.disposition='settled' AND a.state IN ('completed','cancelled') AND a.parent_import_id=b.parent_import_id AND a.source_account_id=b.source_account_id AND i.source_account_id=b.source_account_id AND i.source_id=r.source_id AND i.family='people' AND NEW.creation_snapshot_id=a.confirmed_snapshot_id) THEN RAISE EXCEPTION 'unproven admitted cohort'; END IF;
  END IF;
 ELSE
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=(v->>'plan_id')::uuid AND bundle_id=b.id AND organization_id=b.organization_id FOR SHARE;
  IF p.id IS NULL THEN RAISE EXCEPTION 'family plan missing'; END IF;
  IF TG_TABLE_NAME='migration_family_refresh_result' THEN
   IF p.state<>'running' OR p.confirmed_at IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
    OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN
    RAISE EXCEPTION 'family result lease invalid';
   END IF;
  ELSIF p.state<>'preparing' OR p.confirmed_at IS NOT NULL THEN RAISE EXCEPTION 'family preparation sealed'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_owned_walk_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_owned_walk_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; candidate RECORD;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF OLD.family IN ('activity','metadata') THEN RETURN NEW; END IF; IF NEW.owned_after IS NOT DISTINCT FROM OLD.owned_after AND NEW.owned_walk_complete=OLD.owned_walk_complete THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.family<>'history' OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL
  OR OLD.phase<>'classify' OR NOT OLD.source_walk_complete OR OLD.owned_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale family owned history walk'; END IF;
 IF NEW.owned_after IS DISTINCT FROM OLD.owned_after THEN
  SELECT * INTO candidate FROM crm_family_refresh_next_owned_history(OLD.organization_id,OLD.bundle_id,OLD.owned_after);
  IF candidate.identity_id IS NULL OR candidate.identity_id IS DISTINCT FROM NEW.owned_after
   THEN RAISE EXCEPTION 'family owned history walk skipped an identity'; END IF;
  IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=OLD.id AND organization_id=OLD.organization_id AND kind=candidate.kind AND source_key_hmac=candidate.identity_hmac)
   THEN RAISE EXCEPTION 'family owned history walk outcome missing'; END IF;
 END IF;
 IF NEW.owned_walk_complete AND EXISTS(SELECT 1 FROM crm_family_refresh_next_owned_history(OLD.organization_id,OLD.bundle_id,NEW.owned_after))
  THEN RAISE EXCEPTION 'family owned history walk incomplete'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_patch_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_patch_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle;
BEGIN
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id FOR SHARE;
 IF p.id IS NULL OR p.phase<>'mappings' OR p.mappings_complete OR p.state<>'preparing' OR p.confirmed_at IS NOT NULL
  OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL OR p.predecessor_plan_id IS DISTINCT FROM NEW.source_plan_id
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR (p.family='metadata') IS DISTINCT FROM (NEW.kind IN ('tag','field','option')) OR p.family='history'
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active')
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_mapping m WHERE m.id=NEW.source_mapping_id AND m.plan_id=NEW.source_plan_id AND m.bundle_id=b.id AND m.organization_id=b.organization_id AND m.kind=NEW.kind AND m.source_key_hmac=NEW.source_key_hmac)
  OR (SELECT count(*) FROM migration_family_refresh_mapping_patch WHERE plan_id=p.id AND organization_id=p.organization_id)>=50
  THEN RAISE EXCEPTION 'invalid family mapping patch'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_person_metadata_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_person_metadata_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p migration_family_refresh_plan; c migration_family_refresh_cohort;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' OR NEW.kind<>'metadata' OR NEW.cohort_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
 SELECT * INTO c FROM migration_family_refresh_cohort WHERE id=NEW.cohort_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF p.id IS NULL OR c.id IS NULL OR p.family<>'metadata' OR p.state<>'preparing' OR p.confirmed_at IS NOT NULL OR NOT p.catalog_walk_complete
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR NEW.person_id IS DISTINCT FROM c.person_id OR NEW.source_id IS DISTINCT FROM c.source_person_id OR NEW.mapping_id IS NOT NULL
  OR NEW.disposition NOT IN ('update','already_current','held')
  OR (NEW.disposition='held' AND (NEW.reason IS NULL OR NEW.target_id IS NOT NULL OR NEW.baseline_result_id IS NOT NULL OR NEW.expected_head_id IS NOT NULL OR NEW.expected_revision IS NOT NULL))
  OR (NEW.disposition<>'held' AND (NEW.reason IS NOT NULL OR NEW.source_row_id IS NULL OR NEW.target_id IS DISTINCT FROM c.person_id OR NEW.baseline_result_id IS NULL OR NEW.expected_revision IS NULL OR NEW.expected_revision<=0))
  THEN RAISE EXCEPTION 'invalid family Person metadata unit'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_plan_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_plan_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle;
BEGIN
 PERFORM crm_family_refresh_capability(NEW.organization_id);
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 IF b.id IS NULL OR NEW.source_snapshot_id IS DISTINCT FROM (CASE WHEN NEW.family='history' THEN NULL ELSE b.core_snapshot_id END)
  OR NEW.history_capture_id IS DISTINCT FROM (CASE WHEN NEW.family='history' THEN b.history_capture_id ELSE NULL END) THEN RAISE EXCEPTION 'family source binding mismatch'; END IF;
 IF TG_OP='INSERT' THEN
  IF NEW.state<>'preparing' OR NEW.confirmed_at IS NOT NULL OR NEW.lease_token IS NOT NULL OR NEW.lease_epoch<>0 OR b.confirmed_at IS NOT NULL THEN RAISE EXCEPTION 'invalid initial family plan'; END IF;
 ELSE
  IF (to_jsonb(NEW)-ARRAY['remainder_after','remainder_stage','remainder_complete','inherited_position','state','phase','lease_token','lease_epoch','lease_expires_at','confirmed_at','expires_at','pause_reason','cancel_requested','checkpoint','checkpoint_id','source_walk_complete','owned_after','owned_activity_kind','owned_activity_source_id','owned_walk_complete','mapping_after','mapping_current','mapping_offset','mappings_complete','catalog_after','catalog_walk_complete','proof_after','proofs_complete','seal_after','seal_counts','position','apply_position','digest','counts','results','measured_bytes','retained_bytes','reserved_bytes','run_byte_limit','cohort_after','cohort_counts','capture_checkpoint'])
   IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['remainder_after','remainder_stage','remainder_complete','inherited_position','state','phase','lease_token','lease_epoch','lease_expires_at','confirmed_at','expires_at','pause_reason','cancel_requested','checkpoint','checkpoint_id','source_walk_complete','owned_after','owned_activity_kind','owned_activity_source_id','owned_walk_complete','mapping_after','mapping_current','mapping_offset','mappings_complete','catalog_after','catalog_walk_complete','proof_after','proofs_complete','seal_after','seal_counts','position','apply_position','digest','counts','results','measured_bytes','retained_bytes','reserved_bytes','run_byte_limit','cohort_after','cohort_counts','capture_checkpoint']) THEN RAISE EXCEPTION 'family plan ownership immutable'; END IF;
  IF OLD.confirmed_at IS NOT NULL AND (NEW.digest IS DISTINCT FROM OLD.digest OR NEW.counts IS DISTINCT FROM OLD.counts OR NEW.confirmed_at IS DISTINCT FROM OLD.confirmed_at) THEN RAISE EXCEPTION 'confirmed family plan immutable'; END IF;
  IF OLD.state IN ('cancelled','completed','superseded') AND to_jsonb(NEW)-ARRAY['measured_bytes','retained_bytes','reserved_bytes'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['measured_bytes','retained_bytes','reserved_bytes'] THEN RAISE EXCEPTION 'terminal family plan'; END IF;
  IF NEW.cohort_after<OLD.cohort_after THEN RAISE EXCEPTION 'family cohort cursor regression'; END IF; IF OLD.phase<>'cohort' AND (NEW.cohort_after IS DISTINCT FROM OLD.cohort_after OR NEW.cohort_counts IS DISTINCT FROM OLD.cohort_counts) THEN RAISE EXCEPTION 'family cohort sealed'; END IF; IF NEW.apply_position<OLD.apply_position OR NEW.position<OLD.position OR NEW.run_byte_limit<OLD.run_byte_limit THEN RAISE EXCEPTION 'family progress regression'; END IF;
  IF NEW.lease_token IS DISTINCT FROM OLD.lease_token AND NEW.lease_token IS NOT NULL THEN
   IF NEW.lease_epoch<>OLD.lease_epoch+1 OR (OLD.lease_token IS NOT NULL AND OLD.lease_expires_at>clock_timestamp()) OR NEW.state NOT IN ('preparing','running') THEN RAISE EXCEPTION 'invalid family lease claim'; END IF;
  ELSIF NEW.lease_epoch<>OLD.lease_epoch THEN RAISE EXCEPTION 'invalid family lease epoch'; END IF;
  IF NEW.lease_token IS NOT NULL AND NEW.lease_token IS NOT DISTINCT FROM OLD.lease_token AND NEW.lease_expires_at IS DISTINCT FROM OLD.lease_expires_at
   AND (OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()) THEN RAISE EXCEPTION 'stale family lease renewal'; END IF;
 END IF;
 IF NEW.lease_token IS NOT NULL AND (NEW.lease_expires_at<=clock_timestamp() OR NEW.lease_expires_at>clock_timestamp()+interval '60 seconds') THEN RAISE EXCEPTION 'invalid family lease duration'; END IF;
 IF NEW.confirmed_at IS NOT NULL AND (NEW.digest IS NULL OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=NEW.organization_id)) THEN RAISE EXCEPTION 'family confirmation capability missing'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_recipe_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_recipe_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF current_user='crm_app' AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan p JOIN migration_family_refresh_manifest u ON u.plan_id=p.id AND u.organization_id=p.organization_id WHERE p.id=NEW.plan_id AND p.bundle_id=NEW.bundle_id AND p.organization_id=NEW.organization_id AND u.id=NEW.manifest_id AND u.position=p.proof_after+1 AND p.source_walk_complete AND p.owned_walk_complete AND NOT p.proofs_complete AND p.state='preparing' AND p.confirmed_at IS NULL AND p.lease_token::text=current_setting('crm.family_refresh_lease',true) AND p.lease_expires_at>clock_timestamp() AND EXISTS(SELECT 1 FROM migration_family_refresh_bundle b JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE b.id=p.bundle_id AND b.organization_id=p.organization_id AND b.state='preparing' AND b.confirmed_at IS NULL AND m.role='admin' AND m.status='active') AND NEW.nonce IS NOT NULL AND NEW.ciphertext IS NOT NULL) THEN RAISE EXCEPTION 'unbound family native recipe'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_reclaim
CREATE OR REPLACE FUNCTION public.crm_family_refresh_reclaim(org uuid, bundle uuid, plan uuid, reservation uuid, epoch bigint)
 RETURNS boolean
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; r migration_family_refresh_reservation;
BEGIN
 PERFORM crm_workspace_shared(org);
 PERFORM m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle owner ON owner.organization_id=m.organization_id AND owner.executor_user_id=m.user_id
  WHERE owner.id=bundle AND owner.organization_id=org AND m.role='admin' AND m.status='active' FOR SHARE OF m;
 IF NOT FOUND THEN RAISE EXCEPTION 'family executor unavailable'; END IF;
 PERFORM organization_id FROM migration_snapshot_storage WHERE organization_id=org FOR UPDATE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=plan AND bundle_id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO r FROM migration_family_refresh_reservation WHERE token=reservation AND bundle_id=bundle AND plan_id=plan AND organization_id=org FOR UPDATE;
 IF p.id IS NULL OR b.id IS NULL OR p.lease_epoch<>epoch OR p.state NOT IN ('preparing','running')
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp() THEN RAISE EXCEPTION 'family reclaim lease invalid'; END IF;
 IF r.token IS NULL THEN RETURN false; END IF;
 IF r.purpose<>'unit' OR r.lease_epoch>=p.lease_epoch OR p.measured_bytes<>p.retained_bytes OR b.shared_measured_bytes<>b.shared_retained_bytes THEN RAISE EXCEPTION 'family reclaim evidence unsettled'; END IF;
 DELETE FROM migration_family_refresh_reservation WHERE token=reservation AND organization_id=org;
 UPDATE migration_family_refresh_plan SET reserved_bytes=reserved_bytes-r.byte_count WHERE id=plan AND organization_id=org;
 UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-r.byte_count WHERE organization_id=org;
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-r.byte_count WHERE id=p.source_snapshot_id AND organization_id=org; ELSE UPDATE migration_history_capture_run SET reserved_bytes=reserved_bytes-r.byte_count WHERE id=p.history_capture_id AND organization_id=org; END IF;
 RETURN true;
END $function$


-- crm_family_refresh_remainder_bundle_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_remainder_bundle_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE previous migration_family_refresh_bundle; source migration_family_refresh_bundle;
BEGIN
 IF NEW.predecessor_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO previous FROM migration_family_refresh_bundle WHERE id=NEW.predecessor_id AND organization_id=NEW.organization_id;
 SELECT * INTO source FROM migration_family_refresh_bundle WHERE id=NEW.remainder_source_bundle_id AND organization_id=NEW.organization_id;
 IF previous.id IS NULL OR source.id IS NULL OR previous.state<>'cancelled' OR source.state<>'cancelled'
  OR NEW.remainder_origin_bundle_id IS DISTINCT FROM COALESCE(previous.remainder_origin_bundle_id,previous.id)
  OR NEW.remainder_origin_bundle_id IS DISTINCT FROM COALESCE(source.remainder_origin_bundle_id,source.id)
  OR ROW(NEW.parent_import_id,NEW.parent_plan_id,NEW.source_account_id,NEW.core_report_id,NEW.core_snapshot_id,NEW.history_capture_id)
    IS DISTINCT FROM ROW(previous.parent_import_id,previous.parent_plan_id,previous.source_account_id,previous.core_report_id,previous.core_snapshot_id,previous.history_capture_id)
  OR ROW(NEW.parent_import_id,NEW.parent_plan_id,NEW.source_account_id,NEW.core_report_id,NEW.core_snapshot_id,NEW.history_capture_id)
    IS DISTINCT FROM ROW(source.parent_import_id,source.parent_plan_id,source.source_account_id,source.core_report_id,source.core_snapshot_id,source.history_capture_id)
 THEN RAISE EXCEPTION 'remainder lineage changed frozen sources'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_remainder_copy_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_remainder_copy_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; old_row JSONB; v JSONB:=to_jsonb(NEW); old_plan UUID; source_bundle UUID; ignored TEXT[];
BEGIN
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF NEW.remainder_source_id IS NULL THEN
  IF b.predecessor_id IS NOT NULL THEN RAISE EXCEPTION 'remainder requires frozen source'; END IF;
  RETURN NEW;
 END IF;
 IF b.predecessor_id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL THEN RAISE EXCEPTION 'invalid remainder copy owner'; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=COALESCE((v->>'plan_id')::uuid,b.payer_plan_id) AND bundle_id=b.id AND organization_id=b.organization_id;
 IF p.id IS NULL OR p.state<>'preparing' OR p.remainder_complete OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin') THEN RAISE EXCEPTION 'stale remainder copy'; END IF;
 IF NEW.remainder_source_id IS DISTINCT FROM crm_family_refresh_remainder_next(p) OR TG_TABLE_NAME IS DISTINCT FROM 'migration_family_refresh_'||(ARRAY['cohort','core_page','history_page','source','mapping','mapping','manifest','manifest'])[p.remainder_stage+1] THEN RAISE EXCEPTION 'unexpected remainder copy step'; END IF;
 IF p.id<>b.payer_plan_id AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE id=b.payer_plan_id AND organization_id=b.organization_id AND remainder_stage>=4) THEN RAISE EXCEPTION 'remainder payer incomplete'; END IF;
 source_bundle:=CASE WHEN TG_TABLE_NAME IN ('migration_family_refresh_mapping','migration_family_refresh_manifest','migration_family_refresh_history_page') OR (TG_TABLE_NAME='migration_family_refresh_source' AND p.family='history') THEN (SELECT bundle_id FROM migration_family_refresh_plan WHERE id=p.remainder_source_plan_id AND organization_id=p.organization_id) ELSE b.remainder_source_bundle_id END;
 -- Trigger installation below is a fixed table allowlist, never a caller name.
 EXECUTE format('SELECT to_jsonb(s) FROM %I s WHERE id=$1 AND bundle_id=$2 AND organization_id=$3',TG_TABLE_NAME)
  INTO old_row USING NEW.remainder_source_id,source_bundle,b.organization_id;
 IF old_row IS NULL THEN RAISE EXCEPTION 'missing frozen remainder row'; END IF;
 old_plan:=(old_row->>'plan_id')::uuid;
 IF TG_TABLE_NAME IN ('migration_family_refresh_mapping','migration_family_refresh_manifest') AND old_plan IS DISTINCT FROM p.remainder_source_plan_id THEN RAISE EXCEPTION 'wrong remainder family plan'; END IF;
 IF TG_TABLE_NAME IN ('migration_family_refresh_source','migration_family_refresh_core_page','migration_family_refresh_history_page') AND NOT EXISTS(
  SELECT 1 FROM migration_family_refresh_plan s JOIN migration_family_refresh_bundle source ON source.id=s.bundle_id AND source.organization_id=s.organization_id
  WHERE s.id=old_plan AND s.bundle_id=source_bundle AND s.organization_id=b.organization_id AND (s.id=source.payer_plan_id OR (s.id=p.remainder_source_plan_id AND s.family='history'))
 ) THEN RAISE EXCEPTION 'wrong remainder source owner'; END IF;
 ignored:=ARRAY['id','bundle_id','plan_id','nonce','ciphertext','remainder_source_id'];
 IF TG_TABLE_NAME='migration_family_refresh_source' THEN
  ignored:=ignored||ARRAY['core_page_id','history_page_id'];
  IF (old_row->>'history_page_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_history_page WHERE id=(v->>'history_page_id')::uuid AND bundle_id=b.id AND organization_id=b.organization_id AND remainder_source_id=(old_row->>'history_page_id')::uuid) THEN RAISE EXCEPTION 'wrong copied history page'; END IF;
  IF (old_row->>'core_page_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_core_page WHERE id=(v->>'core_page_id')::uuid AND bundle_id=b.id AND organization_id=b.organization_id AND remainder_source_id=(old_row->>'core_page_id')::uuid) THEN RAISE EXCEPTION 'wrong copied core page'; END IF;
 ELSIF TG_TABLE_NAME='migration_family_refresh_mapping' THEN
  ignored:=ignored||ARRAY['parent_id','source_row_id'];
  IF (old_row->>'parent_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_mapping WHERE id=(v->>'parent_id')::uuid AND plan_id=p.id AND organization_id=b.organization_id AND remainder_source_id=(old_row->>'parent_id')::uuid) THEN RAISE EXCEPTION 'wrong copied parent mapping'; END IF;
 ELSIF TG_TABLE_NAME='migration_family_refresh_manifest' THEN
  ignored:=ignored||ARRAY['position','cohort_id','source_row_id','mapping_id','counts','added_byte_bound','inherited_result_id'];
  IF NEW.inherited_result_id IS NOT NULL THEN
   IF NEW.kind<>'catalog' OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE id=NEW.inherited_result_id AND organization_id=b.organization_id
    AND (manifest_id=NEW.remainder_source_id OR id=(old_row->>'inherited_result_id')::uuid) AND disposition IN ('applied','already_current','held','excluded')) THEN RAISE EXCEPTION 'invalid inherited catalog result'; END IF;
   IF NEW.counts IS DISTINCT FROM (SELECT jsonb_object_agg(key,'0'::text) FROM jsonb_each(old_row->'counts')) THEN RAISE EXCEPTION 'inherited catalog cannot count as unfinished'; END IF;
  ELSE
   IF old_row->>'disposition' NOT IN ('insert','update','already_current','correction') OR (old_row->>'position')::bigint<=(SELECT apply_position FROM migration_family_refresh_plan WHERE id=p.remainder_source_plan_id)
    OR EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE manifest_id=NEW.remainder_source_id AND organization_id=b.organization_id)
    OR NEW.counts IS DISTINCT FROM old_row->'counts' THEN RAISE EXCEPTION 'remainder is not unfinished eligible work'; END IF;
  END IF;
  IF (old_row->>'cohort_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_cohort c JOIN migration_family_refresh_cohort original ON original.id=(old_row->>'cohort_id')::uuid AND original.organization_id=c.organization_id WHERE c.id=NEW.cohort_id AND c.bundle_id=b.id AND c.organization_id=b.organization_id AND to_jsonb(c)-ARRAY['id','bundle_id','remainder_source_id'] = to_jsonb(original)-ARRAY['id','bundle_id','remainder_source_id']) THEN RAISE EXCEPTION 'wrong copied cohort'; END IF;
  IF (old_row->>'mapping_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_mapping WHERE id=NEW.mapping_id AND plan_id=p.id AND organization_id=b.organization_id AND remainder_source_id=(old_row->>'mapping_id')::uuid) THEN RAISE EXCEPTION 'wrong copied mapping'; END IF;
 END IF;
 IF TG_TABLE_NAME IN ('migration_family_refresh_mapping','migration_family_refresh_manifest') AND (old_row->>'source_row_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_source c JOIN migration_family_refresh_source original ON original.id=(old_row->>'source_row_id')::uuid AND original.organization_id=c.organization_id WHERE c.id=(v->>'source_row_id')::uuid AND c.bundle_id=b.id AND c.organization_id=b.organization_id AND to_jsonb(c)-ARRAY['id','bundle_id','plan_id','remainder_source_id','core_page_id','history_page_id','nonce','ciphertext'] = to_jsonb(original)-ARRAY['id','bundle_id','plan_id','remainder_source_id','core_page_id','history_page_id','nonce','ciphertext']) THEN RAISE EXCEPTION 'wrong copied source'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_source' AND ((v->>'core_page_id') IS NULL)<>((old_row->>'core_page_id') IS NULL) THEN RAISE EXCEPTION 'copied page presence changed'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_mapping' AND ((v->>'parent_id') IS NULL)<>((old_row->>'parent_id') IS NULL) THEN RAISE EXCEPTION 'copied mapping parent changed'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_manifest' AND (((v->>'cohort_id') IS NULL)<>((old_row->>'cohort_id') IS NULL) OR ((v->>'mapping_id') IS NULL)<>((old_row->>'mapping_id') IS NULL)) THEN RAISE EXCEPTION 'copied unit references changed'; END IF;
 IF TG_TABLE_NAME IN ('migration_family_refresh_mapping','migration_family_refresh_manifest') AND ((v->>'source_row_id') IS NULL)<>((old_row->>'source_row_id') IS NULL) THEN RAISE EXCEPTION 'copied source presence changed'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_source' AND ((v->>'history_page_id') IS NULL)<>((old_row->>'history_page_id') IS NULL) THEN RAISE EXCEPTION 'copied history page presence changed'; END IF;
 IF v-ignored IS DISTINCT FROM old_row-ignored THEN RAISE EXCEPTION 'remainder changed frozen header'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_remainder_next
CREATE OR REPLACE FUNCTION public.crm_family_refresh_remainder_next(p migration_family_refresh_plan)
 RETURNS uuid
 LANGUAGE plpgsql
 STABLE
AS $function$
DECLARE b migration_family_refresh_bundle; source migration_family_refresh_plan; answer UUID;
BEGIN
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id;
 SELECT * INTO source FROM migration_family_refresh_plan WHERE id=p.remainder_source_plan_id AND organization_id=p.organization_id;
 IF source.id IS NULL THEN RETURN NULL; END IF;
 CASE p.remainder_stage
 WHEN 0 THEN
  IF p.id=b.payer_plan_id THEN SELECT id INTO answer FROM migration_family_refresh_cohort WHERE bundle_id=source.bundle_id AND organization_id=p.organization_id AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1; END IF;
 WHEN 1 THEN
  IF p.id=b.payer_plan_id AND p.family<>'history' THEN SELECT id INTO answer FROM migration_family_refresh_core_page WHERE bundle_id=source.bundle_id AND organization_id=p.organization_id AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1; END IF;
 WHEN 2 THEN
  IF p.family='history' THEN SELECT id INTO answer FROM migration_family_refresh_history_page WHERE plan_id=source.id AND organization_id=p.organization_id AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1; END IF;
 WHEN 3 THEN
  IF p.id=b.payer_plan_id OR p.family='history' THEN SELECT id INTO answer FROM migration_family_refresh_source WHERE bundle_id=source.bundle_id AND organization_id=p.organization_id AND (CASE WHEN p.family='history' THEN history_page_id IS NOT NULL AND plan_id=source.id ELSE core_page_id IS NOT NULL END) AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1; END IF;
 WHEN 4,5 THEN
  SELECT id INTO answer FROM migration_family_refresh_mapping WHERE plan_id=source.id AND organization_id=p.organization_id AND (parent_id IS NULL)=(p.remainder_stage=4) AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1;
 WHEN 6 THEN
  SELECT u.id INTO answer FROM migration_family_refresh_manifest u WHERE u.plan_id=source.id AND u.organization_id=p.organization_id AND u.kind='catalog' AND u.disposition IN ('insert','already_current')
   AND (u.inherited_result_id IS NOT NULL OR EXISTS(SELECT 1 FROM migration_family_refresh_result r WHERE r.manifest_id=u.id AND r.organization_id=u.organization_id AND r.disposition IN ('applied','already_current','held','excluded')))
   AND (p.remainder_after IS NULL OR u.position>(SELECT position FROM migration_family_refresh_manifest WHERE id=p.remainder_after AND plan_id=source.id AND organization_id=p.organization_id)) ORDER BY position LIMIT 1;
 WHEN 7 THEN
  SELECT u.id INTO answer FROM migration_family_refresh_manifest u WHERE u.plan_id=source.id AND u.organization_id=p.organization_id AND u.position>source.apply_position
   AND u.disposition IN ('insert','update','already_current','correction') AND u.inherited_result_id IS NULL
   AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_result r WHERE r.manifest_id=u.id AND r.organization_id=u.organization_id)
   AND (p.remainder_after IS NULL OR u.position>(SELECT position FROM migration_family_refresh_manifest WHERE id=p.remainder_after AND plan_id=source.id AND organization_id=p.organization_id)) ORDER BY position LIMIT 1;
 ELSE NULL;
 END CASE;
 RETURN answer;
END $function$


-- crm_family_refresh_remainder_plan_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_remainder_plan_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan;
BEGIN
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF NEW.remainder_source_plan_id IS NULL THEN
  IF b.predecessor_id IS NOT NULL OR NOT NEW.remainder_complete OR NEW.remainder_stage<>0 OR NEW.inherited_position<>0 THEN RAISE EXCEPTION 'invalid ordinary refresh plan'; END IF;
 ELSE
  IF TG_OP='INSERT' AND (NEW.remainder_complete OR NEW.remainder_stage<>0 OR NEW.remainder_after IS NOT NULL OR NEW.inherited_position<>0 OR NEW.position<>0 OR NEW.apply_position<>0) THEN RAISE EXCEPTION 'invalid initial remainder progress'; END IF;
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.remainder_source_plan_id AND organization_id=NEW.organization_id;
  IF b.predecessor_id IS NULL OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_bundle source WHERE source.id=p.bundle_id AND source.organization_id=b.organization_id AND COALESCE(source.remainder_origin_bundle_id,source.id)=b.remainder_origin_bundle_id AND source.parent_import_id=b.parent_import_id AND source.parent_plan_id=b.parent_plan_id AND source.source_account_id=b.source_account_id) OR p.family IS DISTINCT FROM NEW.family
   OR p.confirmed_at IS NULL OR p.state NOT IN ('cancelled','completed','superseded')
   OR p.source_snapshot_id IS DISTINCT FROM NEW.source_snapshot_id OR p.history_capture_id IS DISTINCT FROM NEW.history_capture_id THEN RAISE EXCEPTION 'invalid remainder source plan'; END IF;
  IF TG_OP='UPDATE' AND (NEW.remainder_stage<OLD.remainder_stage OR NEW.inherited_position<OLD.inherited_position
   OR (OLD.remainder_complete AND (NOT NEW.remainder_complete OR NEW.inherited_position<>OLD.inherited_position OR NEW.remainder_stage<>OLD.remainder_stage OR NEW.remainder_after IS DISTINCT FROM OLD.remainder_after))) THEN RAISE EXCEPTION 'remainder copy regression'; END IF;
  IF NEW.confirmed_at IS NOT NULL AND (NOT NEW.remainder_complete OR NEW.remainder_stage<>8) THEN RAISE EXCEPTION 'incomplete remainder confirmation'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_remainder_progress_guard
CREATE OR REPLACE FUNCTION public.crm_family_refresh_remainder_progress_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; next_id UUID; table_name TEXT; copied JSONB;
BEGIN
 IF OLD.remainder_source_plan_id IS NULL OR OLD.remainder_complete THEN RETURN NEW; END IF;
 IF ROW(NEW.remainder_stage,NEW.remainder_after,NEW.remainder_complete,NEW.inherited_position,NEW.position,NEW.apply_position,NEW.counts,NEW.source_walk_complete,NEW.owned_walk_complete,NEW.mappings_complete,NEW.catalog_walk_complete)
  IS NOT DISTINCT FROM ROW(OLD.remainder_stage,OLD.remainder_after,OLD.remainder_complete,OLD.inherited_position,OLD.position,OLD.apply_position,OLD.counts,OLD.source_walk_complete,OLD.owned_walk_complete,OLD.mappings_complete,OLD.catalog_walk_complete) THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id;
 IF OLD.state<>'preparing' OR b.state<>'preparing' OR OLD.confirmed_at IS NOT NULL
  OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at IS NULL OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale remainder progress'; END IF;
 next_id:=crm_family_refresh_remainder_next(OLD);
 IF NEW.remainder_stage<>OLD.remainder_stage THEN
  IF next_id IS NOT NULL OR NEW.remainder_stage<>OLD.remainder_stage+1 OR NEW.remainder_after IS NOT NULL OR NEW.position<>OLD.position OR NEW.counts<>OLD.counts OR NEW.inherited_position<>OLD.inherited_position THEN RAISE EXCEPTION 'remainder skipped frozen work'; END IF;
 ELSE
  IF next_id IS NULL OR NEW.remainder_after IS DISTINCT FROM next_id THEN RAISE EXCEPTION 'invalid remainder checkpoint'; END IF;
  table_name:=(ARRAY['cohort','core_page','history_page','source','mapping','mapping','manifest','manifest'])[OLD.remainder_stage+1];
  EXECUTE format('SELECT to_jsonb(c) FROM migration_family_refresh_%I c WHERE bundle_id=$1 AND organization_id=$2 AND remainder_source_id=$3',table_name) INTO copied USING OLD.bundle_id,OLD.organization_id,next_id;
  IF copied IS NULL OR (copied ? 'plan_id' AND (copied->>'plan_id')::uuid<>OLD.id) THEN RAISE EXCEPTION 'remainder checkpoint without copy'; END IF;
  IF OLD.remainder_stage IN (6,7) THEN
   IF NEW.position<>OLD.position+1 OR (copied->>'position')::bigint<>NEW.position OR NEW.inherited_position<>OLD.inherited_position+(CASE WHEN OLD.remainder_stage=6 THEN 1 ELSE 0 END)
    OR NEW.counts IS DISTINCT FROM (SELECT jsonb_object_agg(key,((value::text)::numeric+COALESCE((OLD.counts->>key)::numeric,0))::text) FROM jsonb_each_text(copied->'counts')) THEN RAISE EXCEPTION 'invalid remainder unit counts'; END IF;
  ELSIF NEW.position<>OLD.position OR NEW.counts<>OLD.counts OR NEW.inherited_position<>OLD.inherited_position THEN RAISE EXCEPTION 'invalid remainder progress counts'; END IF;
 END IF;
 IF NEW.remainder_stage=8 THEN
  IF NOT NEW.remainder_complete OR NOT NEW.source_walk_complete OR NOT NEW.owned_walk_complete OR NOT NEW.mappings_complete OR NOT NEW.catalog_walk_complete OR NEW.apply_position<>NEW.inherited_position THEN RAISE EXCEPTION 'incomplete remainder boundary'; END IF;
 ELSIF NEW.remainder_complete OR NEW.source_walk_complete OR NEW.owned_walk_complete OR NEW.mappings_complete OR NEW.catalog_walk_complete OR NEW.apply_position<>OLD.apply_position THEN RAISE EXCEPTION 'premature remainder boundary'; END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_reserve
CREATE OR REPLACE FUNCTION public.crm_family_refresh_reserve(org uuid, bundle uuid, plan uuid, reservation uuid, epoch bigint, amount bigint, purpose text, run_ceiling bigint, org_ceiling bigint)
 RETURNS boolean
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; l migration_snapshot_storage; s migration_snapshot; h migration_history_capture_run; shared_bytes BIGINT:=0;
BEGIN
 IF amount<512 OR (purpose='control' AND amount<8192) OR amount>67108864 OR purpose NOT IN ('control','unit') OR run_ceiling<=0 OR org_ceiling<=0 THEN RAISE EXCEPTION 'invalid family reservation'; END IF;
 PERFORM crm_workspace_shared(org);
 PERFORM m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle owner ON owner.organization_id=m.organization_id AND owner.executor_user_id=m.user_id
  WHERE owner.id=bundle AND owner.organization_id=org AND m.role='admin' AND m.status='active' FOR SHARE OF m;
 IF NOT FOUND THEN RAISE EXCEPTION 'family executor unavailable'; END IF;
 SELECT * INTO l FROM migration_snapshot_storage WHERE organization_id=org FOR UPDATE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=plan AND bundle_id=bundle AND organization_id=org FOR UPDATE;
 IF b.id IS NULL OR p.id IS NULL OR l.organization_id IS NULL OR b.payer_plan_id IS NULL OR b.state IN ('cancelled','completed') OR p.lease_epoch<>epoch
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=org AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN RAISE EXCEPTION 'family reservation owner unavailable'; END IF;
 IF purpose='unit' AND (p.state NOT IN ('preparing','running') OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()) THEN RAISE EXCEPTION 'family reservation lease invalid'; END IF;
 IF b.payer_plan_id=p.id THEN shared_bytes:=b.shared_measured_bytes; END IF;
 -- Measured evidence includes any writes already made in this transaction.
 -- Existing reservations cover unsettled bytes, so only admitted retained
 -- bytes and reservations are used for the outer capacity comparison.
 IF amount>LEAST(p.run_byte_limit,run_ceiling)-p.retained_bytes-p.reserved_bytes-(CASE WHEN b.payer_plan_id=p.id THEN b.shared_retained_bytes ELSE 0 END)
  OR amount>LEAST(l.byte_limit,org_ceiling)-l.retained_bytes-l.reserved_bytes
  OR p.measured_bytes-p.retained_bytes+shared_bytes-(CASE WHEN b.payer_plan_id=p.id THEN b.shared_retained_bytes ELSE 0 END)>amount-512 THEN RETURN false; END IF;
 IF p.source_snapshot_id IS NOT NULL THEN
  SELECT * INTO s FROM migration_snapshot WHERE id=p.source_snapshot_id AND organization_id=org FOR UPDATE;
  IF s.id IS NULL THEN RAISE EXCEPTION 'family source budget unavailable'; END IF;
  IF amount>LEAST(s.run_byte_limit,run_ceiling)-s.retained_bytes-s.reserved_bytes THEN RETURN false; END IF;
 ELSE
  SELECT * INTO h FROM migration_history_capture_run WHERE id=p.history_capture_id AND organization_id=org FOR UPDATE;
  IF h.id IS NULL THEN RAISE EXCEPTION 'family history source budget unavailable'; END IF;
  IF amount>LEAST(h.run_byte_limit,run_ceiling)-h.retained_bytes-h.reserved_bytes THEN RETURN false; END IF;
 END IF;
 INSERT INTO migration_family_refresh_reservation(token,organization_id,bundle_id,plan_id,lease_epoch,purpose,byte_count) VALUES(reservation,org,bundle,plan,epoch,purpose,amount);
 UPDATE migration_family_refresh_plan SET reserved_bytes=reserved_bytes+amount WHERE id=plan AND organization_id=org;
 UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+amount WHERE organization_id=org;
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+amount WHERE id=p.source_snapshot_id AND organization_id=org; ELSE UPDATE migration_history_capture_run SET reserved_bytes=reserved_bytes+amount WHERE id=p.history_capture_id AND organization_id=org; END IF;
 RETURN true;
END $function$


-- crm_family_refresh_retained_size
CREATE OR REPLACE FUNCTION public.crm_family_refresh_retained_size(v jsonb)
 RETURNS bigint
 LANGUAGE plpgsql
 IMMUTABLE
AS $function$
DECLARE total BIGINT:=256; k TEXT; value TEXT;
BEGIN
 IF v IS NULL THEN RETURN 0; END IF; total:=total+COALESCE((v->>'native_bytes')::bigint,0);
 FOR k,value IN SELECT e.key,e.value #>> '{}' FROM jsonb_each(v) e LOOP
  IF value IS NULL THEN CONTINUE; END IF;
  IF k IN ('source_nonce','source_ciphertext','digest','nonce','ciphertext','identity_hmac','semantic_hmac','source_key_hmac','field_name_hmac','input_digest','before_hash','after_hash') THEN
   total:=total+octet_length(decode(substring(value FROM 3),'hex'));
  ELSIF k IN ('source_person_id','source_id','owned_activity_kind','owned_activity_source_id','engine_version','state','family','phase','pause_reason','representation','kind','reason','disposition','action','purpose','capability','table_name','operation','counts','seal_counts','results','stream','cohort_after','cohort_counts','actor_kind','origin','source_time_basis') THEN
   total:=total+octet_length(value);
  END IF;
 END LOOP;
 RETURN total;
END $function$


-- crm_family_refresh_revision_installed
CREATE OR REPLACE FUNCTION public.crm_family_refresh_revision_installed(kind text)
 RETURNS timestamp with time zone
 LANGUAGE sql
 STABLE SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
  SELECT m.installed_on + (ceil(m.execution_time::numeric / 1000)::double precision * interval '1 microsecond')
  FROM public._sqlx_migrations m
  JOIN (VALUES
    ('task', 20260923000001::bigint, decode('021732320efa7c94279d8b29fa4a8be66bd9dcc0a9a1f7dbfb8de14542cf9d2c3e5677e2ab1825d653e3baf5795f1c28','hex')),
    ('note', 20260925000001::bigint, decode('76daa367b2303ed75aed0bf6f3c2b36a56201ece7eafc58aadc8211df07a378f3a9f1128ae85dffce1f59d06e1e42d05','hex')),
    ('metadata', 20261003000001::bigint, decode('6881596030626d64ba22819e8810a73b60e689542c7bcf476d0c4d6d3d0f2f73daf56dda068e0f30e79becb31fd9cea6','hex'))
  ) expected(kind,version,checksum) ON m.version=expected.version AND m.checksum=expected.checksum
  WHERE expected.kind=$1 AND m.success AND m.execution_time>=0;
$function$


-- crm_family_refresh_sealing_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_sealing_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_manifest' THEN
  IF EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND organization_id=NEW.organization_id AND source_walk_complete AND owned_walk_complete) THEN RAISE EXCEPTION 'family preparation unit set closed'; END IF;
  RETURN NEW;
 END IF;
 IF OLD.state='ready' AND ROW(NEW.expires_at,NEW.digest,NEW.counts,NEW.position,NEW.proof_after,NEW.proofs_complete,NEW.seal_after,NEW.seal_counts) IS DISTINCT FROM ROW(OLD.expires_at,OLD.digest,OLD.counts,OLD.position,OLD.proof_after,OLD.proofs_complete,OLD.seal_after,OLD.seal_counts) THEN RAISE EXCEPTION 'ready family plan immutable'; END IF;
 IF ROW(NEW.proof_after,NEW.proofs_complete,NEW.seal_after,NEW.seal_counts) IS DISTINCT FROM ROW(OLD.proof_after,OLD.proofs_complete,OLD.seal_after,OLD.seal_counts) OR (NEW.state='ready' AND OLD.state<>'ready') THEN
  IF OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR NOT OLD.source_walk_complete OR NOT OLD.owned_walk_complete
   OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()
   OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_bundle b JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE b.id=OLD.bundle_id AND b.organization_id=OLD.organization_id AND b.state='preparing' AND b.confirmed_at IS NULL AND m.role='admin' AND m.status='active')
   THEN RAISE EXCEPTION 'stale family sealing'; END IF;
  IF NEW.proof_after IS DISTINCT FROM OLD.proof_after AND (OLD.proofs_complete OR NEW.proof_after<>OLD.proof_after+1 OR NEW.proof_after>OLD.position) THEN RAISE EXCEPTION 'family proof cursor skipped unit'; END IF;
  IF NEW.proofs_complete AND NEW.proof_after<>OLD.position THEN RAISE EXCEPTION 'family proofs incomplete'; END IF;
  IF NEW.seal_after IS DISTINCT FROM OLD.seal_after AND (NOT OLD.proofs_complete OR NEW.seal_after<>OLD.seal_after+1 OR NEW.seal_after>OLD.position) THEN RAISE EXCEPTION 'family seal cursor skipped unit'; END IF;
  IF NEW.state='ready' AND (NOT NEW.proofs_complete OR NEW.seal_after<>NEW.position OR NEW.seal_counts<>NEW.counts OR NEW.digest IS NULL OR NEW.expires_at IS NULL OR NEW.expires_at<=clock_timestamp() OR NEW.expires_at>clock_timestamp()+interval '10 minutes') THEN RAISE EXCEPTION 'family seal incomplete'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_family_refresh_settle
CREATE OR REPLACE FUNCTION public.crm_family_refresh_settle(org uuid, bundle uuid, plan uuid, reservation uuid, epoch bigint, release_control boolean)
 RETURNS bigint
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; r migration_family_refresh_reservation; own_delta BIGINT; shared_delta BIGINT:=0; actual BIGINT; refund BIGINT;
BEGIN
 PERFORM crm_workspace_shared(org);
 PERFORM m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle owner ON owner.organization_id=m.organization_id AND owner.executor_user_id=m.user_id
  WHERE owner.id=bundle AND owner.organization_id=org AND m.role='admin' AND m.status='active' FOR SHARE OF m;

 PERFORM organization_id FROM migration_snapshot_storage WHERE organization_id=org FOR UPDATE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=plan AND bundle_id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO r FROM migration_family_refresh_reservation WHERE token=reservation AND plan_id=plan AND bundle_id=bundle AND organization_id=org FOR UPDATE;
 IF p.id IS NULL OR b.id IS NULL OR r.token IS NULL OR p.lease_epoch<>epoch OR (r.purpose='unit' AND r.lease_epoch<>epoch) THEN RAISE EXCEPTION 'family settlement owner unavailable'; END IF;
 IF r.purpose='unit' AND (p.state NOT IN ('preparing','running') OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()) THEN RAISE EXCEPTION 'family settlement lease invalid'; END IF;
 IF NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=org AND user_id=b.executor_user_id AND role='admin' AND status='active') AND NOT (r.purpose='control' AND NOT release_control AND b.state='paused' AND p.lease_token IS NULL AND ((p.state='paused' AND p.pause_reason='executor_revoked' AND p.measured_bytes-p.retained_bytes BETWEEN -1024 AND 1024) OR (b.payer_plan_id=p.id AND p.measured_bytes=p.retained_bytes)) AND (b.payer_plan_id<>p.id OR b.shared_measured_bytes-b.shared_retained_bytes BETWEEN -3 AND 1) AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_reservation u WHERE u.organization_id=org AND u.plan_id=plan AND u.purpose='unit')) AND NOT (r.purpose='control' AND NOT release_control AND EXISTS(SELECT 1 FROM migration_family_refresh_receipt c JOIN organization_membership m ON m.organization_id=c.organization_id AND m.user_id=c.actor_user_id WHERE c.organization_id=org AND c.bundle_id=bundle AND c.action='cancel' AND c.request_id::text=current_setting('crm.family_refresh_cancel_request',true) AND m.user_id::text=current_setting('crm.family_refresh_cancel_actor',true) AND m.role='admin' AND m.status='active')) THEN RAISE EXCEPTION 'family settlement executor unavailable'; END IF;
 own_delta:=p.measured_bytes-p.retained_bytes;
 IF b.payer_plan_id=p.id THEN shared_delta:=b.shared_measured_bytes-b.shared_retained_bytes; END IF;
 actual:=own_delta+shared_delta;
 IF actual>r.byte_count-512 THEN RAISE EXCEPTION 'family reservation exceeded'; END IF;
 IF r.purpose='control' AND NOT release_control THEN
  refund:=actual;
  UPDATE migration_family_refresh_reservation SET byte_count=byte_count-actual WHERE token=reservation AND organization_id=org;
 ELSE
  refund:=r.byte_count;
  DELETE FROM migration_family_refresh_reservation WHERE token=reservation AND organization_id=org;
 END IF;
 UPDATE migration_family_refresh_plan SET retained_bytes=measured_bytes,reserved_bytes=reserved_bytes-refund WHERE id=plan AND organization_id=org;
 IF b.payer_plan_id=p.id THEN UPDATE migration_family_refresh_bundle SET shared_retained_bytes=shared_measured_bytes WHERE id=bundle AND organization_id=org; END IF;
 UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+actual,reserved_bytes=reserved_bytes-refund WHERE organization_id=org;
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET retained_bytes=retained_bytes+actual,reserved_bytes=reserved_bytes-refund WHERE id=p.source_snapshot_id AND organization_id=org; ELSE UPDATE migration_history_capture_run SET retained_bytes=retained_bytes+actual,reserved_bytes=reserved_bytes-refund WHERE id=p.history_capture_id AND organization_id=org; END IF;
 RETURN actual;
END $function$


-- crm_family_refresh_walk_fence
CREATE OR REPLACE FUNCTION public.crm_family_refresh_walk_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE b migration_family_refresh_bundle;
BEGIN
 IF TG_OP='INSERT' AND to_jsonb(NEW)->>'remainder_source_id' IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='migration_family_refresh_plan' AND to_jsonb(OLD)->>'remainder_source_plan_id' IS NOT NULL AND NOT (to_jsonb(OLD)->>'remainder_complete')::boolean THEN RETURN NEW; END IF;
 IF OLD.family IN ('activity','metadata') THEN RETURN NEW; END IF; IF NEW.checkpoint_id IS NOT DISTINCT FROM OLD.checkpoint_id AND NEW.source_walk_complete=OLD.source_walk_complete THEN RETURN NEW; END IF;
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id FOR SHARE;
 IF b.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR OLD.family<>'history' OR OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL
  OR OLD.phase NOT IN ('mappings','classify') OR OLD.source_walk_complete
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale family source walk'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND
  (NEW.checkpoint_id IS NULL OR (OLD.checkpoint_id IS NOT NULL AND NEW.checkpoint_id<=OLD.checkpoint_id)
   OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE id=NEW.checkpoint_id AND bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('event','call','text'))
   OR EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('event','call','text') AND (OLD.checkpoint_id IS NULL OR id>OLD.checkpoint_id) AND id<NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'family source walk skipped an occurrence'; END IF;
 IF NEW.checkpoint_id IS DISTINCT FROM OLD.checkpoint_id AND NOT EXISTS(
  SELECT 1 FROM migration_family_refresh_source s JOIN migration_family_refresh_manifest m
   ON m.plan_id=OLD.id AND m.organization_id=s.organization_id AND m.kind=s.kind
    AND (m.source_row_id=s.id OR m.source_key_hmac=s.identity_hmac)
   WHERE s.id=NEW.checkpoint_id AND s.bundle_id=OLD.bundle_id AND s.organization_id=OLD.organization_id AND s.kind IN ('event','call','text'))
  THEN RAISE EXCEPTION 'family source walk outcome missing'; END IF;
 IF NEW.source_walk_complete AND EXISTS(SELECT 1 FROM migration_family_refresh_source WHERE bundle_id=OLD.bundle_id AND organization_id=OLD.organization_id AND kind IN ('event','call','text') AND (NEW.checkpoint_id IS NULL OR id>NEW.checkpoint_id))
  THEN RAISE EXCEPTION 'family source walk incomplete'; END IF;
 RETURN NEW;
END $function$


-- crm_history_complete_read
CREATE OR REPLACE FUNCTION public.crm_history_complete_read(org uuid)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
 PERFORM crm_workspace_shared(org);
 IF EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=org) OR EXISTS(SELECT 1 FROM migration_admitted_history_root WHERE organization_id=org AND confirmed_plan_id IS NOT NULL) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_review_required'; END IF;
END $function$


-- crm_history_display_owner_immutable
CREATE OR REPLACE FUNCTION public.crm_history_display_owner_immutable()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF (to_jsonb(OLD)-ARRAY['nonce','ciphertext']) IS DISTINCT FROM (to_jsonb(NEW)-ARRAY['nonce','ciphertext']) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_display_owner_immutable';END IF;
 RETURN NEW;
END $function$


-- crm_history_import_display_change
CREATE OR REPLACE FUNCTION public.crm_history_import_display_change()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
DECLARE name TEXT;r RECORD;m RECORD;v JSONB;
BEGIN
 v:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 UPDATE migration_history_import_run SET revision=revision+1,updated_at=clock_timestamp() WHERE organization_id=(v->>'organization_id')::uuid AND plan_id=(v->>'plan_id')::uuid;
 FOREACH name IN ARRAY ARRAY['fub_event_record_imported','fub_call_record_imported','fub_text_record_imported'] LOOP
  FOR r IN EXECUTE format('SELECT f.person_id,CASE WHEN h.identity_id IS NOT NULL THEN h.source_created_at ELSE f.source_created_at END AS source_created_at FROM %I f LEFT JOIN migration_family_refresh_history_head h ON h.identity_id=f.identity_id AND h.organization_id=f.organization_id JOIN migration_history_import_identity i ON i.id=f.identity_id AND i.organization_id=f.organization_id AND i.erased_at IS NULL WHERE f.organization_id=$1 AND COALESCE(f.manifest_id,f.admitted_manifest_id,f.refresh_manifest_id)=$2',name) USING (v->>'organization_id')::uuid,(v->>'id')::uuid LOOP
   PERFORM crm_history_review_delta((v->>'organization_id')::uuid,r.person_id,CASE WHEN TG_OP='DELETE' THEN name||CASE WHEN r.source_created_at IS NULL THEN '_unknown' ELSE '_known' END END,CASE WHEN TG_OP='DELETE' THEN -1 ELSE 0 END);
  END LOOP;
 END LOOP;
 IF TG_OP='DELETE' AND OLD.admitted_root_id IS NULL AND OLD.refresh_bundle_id IS NULL THEN
  SELECT * INTO m FROM migration_history_import_manifest WHERE id=OLD.id AND organization_id=OLD.organization_id;
  IF m.identity_hmac IS NOT NULL THEN
   INSERT INTO migration_history_import_identity(id,organization_id,owner_run_id,identity_hmac,semantic_hmac,person_id,fact_id,family,erased_at)
   VALUES(gen_random_uuid(),m.organization_id,m.owner_run_id,m.identity_hmac,m.semantic_hmac,m.person_id,NULL,m.family,clock_timestamp()) ON CONFLICT(organization_id,identity_hmac) DO NOTHING;
   UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=m.organization_id AND identity_hmac=m.identity_hmac AND erased_at IS NULL;
  END IF;
 END IF;
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_history_import_display_guard
CREATE OR REPLACE FUNCTION public.crm_history_import_display_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF EXISTS(SELECT 1 FROM migration_history_import_manifest m JOIN migration_history_import_identity i ON i.organization_id=m.organization_id AND i.identity_hmac=m.identity_hmac WHERE m.id=NEW.id AND m.organization_id=NEW.organization_id AND i.erased_at IS NOT NULL)
 THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_identity_erased';END IF;RETURN NEW;
END $function$


-- crm_history_import_erased_identity
CREATE OR REPLACE FUNCTION public.crm_history_import_erased_identity()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
DECLARE name TEXT;r RECORD;
BEGIN
 IF OLD.erased_at IS NULL AND NEW.erased_at IS NOT NULL THEN
  UPDATE migration_history_import_run attempt SET revision=revision+1,updated_at=clock_timestamp() WHERE attempt.organization_id=NEW.organization_id AND EXISTS(SELECT 1 FROM migration_history_import_manifest m WHERE m.organization_id=NEW.organization_id AND m.identity_hmac=NEW.identity_hmac AND m.plan_id=attempt.plan_id);
  FOREACH name IN ARRAY ARRAY['fub_event_record_imported','fub_call_record_imported','fub_text_record_imported'] LOOP
   FOR r IN EXECUTE format('SELECT f.person_id,CASE WHEN h.identity_id IS NOT NULL THEN h.source_created_at ELSE f.source_created_at END AS source_created_at FROM %I f LEFT JOIN migration_family_refresh_history_head h ON h.identity_id=f.identity_id AND h.organization_id=f.organization_id JOIN migration_history_import_display d ON d.id=COALESCE(f.manifest_id,f.admitted_manifest_id,f.refresh_manifest_id) AND d.organization_id=f.organization_id WHERE f.organization_id=$1 AND f.identity_id=$2',name) USING NEW.organization_id,NEW.id LOOP
    PERFORM crm_history_review_delta(NEW.organization_id,r.person_id,name||CASE WHEN r.source_created_at IS NULL THEN '_unknown' ELSE '_known' END,-1);
   END LOOP;
  END LOOP;
 END IF;
 RETURN NEW;
END $function$


-- crm_history_import_fact_count
CREATE OR REPLACE FUNCTION public.crm_history_import_fact_count()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,TG_TABLE_NAME||CASE WHEN NEW.source_created_at IS NULL THEN '_unknown' ELSE '_known' END,1);
 RETURN NEW;
END $function$


-- crm_history_import_fact_guard
CREATE OR REPLACE FUNCTION public.crm_history_import_fact_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF NEW.refresh_bundle_id IS NOT NULL THEN
  IF NOT COALESCE(crm_family_history_first_allowed(to_jsonb(NEW),TG_TABLE_NAME),false) THEN RAISE EXCEPTION 'family history first fact invalid'; END IF;
  RETURN NEW;
 END IF;
 IF NEW.admitted_root_id IS NOT NULL THEN
  IF NOT COALESCE(crm_admitted_history_fact_allowed(to_jsonb(NEW),TG_TABLE_NAME),false) THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_permit_required';END IF;
  RETURN NEW;
 END IF;
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
END $function$


-- crm_history_import_frozen
CREATE OR REPLACE FUNCTION public.crm_history_import_frozen()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE p UUID; o UUID;
BEGIN
 p:=(to_jsonb(OLD)->>CASE WHEN TG_TABLE_NAME='migration_history_import_plan' THEN 'id' ELSE 'plan_id' END)::uuid;
 o:=OLD.organization_id;
 IF EXISTS(SELECT 1 FROM migration_history_import_plan WHERE id=p AND organization_id=o AND state='ready') THEN
  RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_plan_immutable';
 END IF;
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_history_import_identity_immutable
CREATE OR REPLACE FUNCTION public.crm_history_import_identity_immutable()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF TG_OP='DELETE' OR (to_jsonb(NEW)-'erased_at') IS DISTINCT FROM (to_jsonb(OLD)-'erased_at') OR OLD.erased_at IS NOT NULL OR NEW.erased_at IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_identity_immutable';
 END IF;
 RETURN NEW;
END $function$


-- crm_history_import_measure
CREATE OR REPLACE FUNCTION public.crm_history_import_measure()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE delta BIGINT;owner_id UUID;org UUID;v JSONB;
BEGIN
 delta:=crm_history_import_retained_size(CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)-crm_history_import_retained_size(CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta=0 THEN RETURN COALESCE(NEW,OLD);END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD));org:=(v->>'organization_id')::uuid;
 IF v->>'refresh_plan_id' IS NOT NULL THEN
  IF delta<0 AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE id=(v->>'refresh_plan_id')::uuid AND organization_id=org AND measured_bytes=retained_bytes) THEN RAISE EXCEPTION 'history first display unsettled'; END IF;
  UPDATE migration_family_refresh_plan SET measured_bytes=measured_bytes+delta,retained_bytes=retained_bytes+LEAST(delta,0) WHERE id=(v->>'refresh_plan_id')::uuid AND organization_id=org;
  IF delta<0 THEN
   UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+delta WHERE organization_id=org;
   UPDATE migration_history_capture_run h SET retained_bytes=h.retained_bytes+delta FROM migration_family_refresh_plan p WHERE p.id=(v->>'refresh_plan_id')::uuid AND p.organization_id=org AND h.id=p.history_capture_id AND h.organization_id=org;
  END IF;
  RETURN COALESCE(NEW,OLD);
 END IF;
 IF v->>'admitted_root_id' IS NOT NULL THEN
  UPDATE migration_admitted_history_root SET retained_bytes=retained_bytes+delta WHERE id=(v->>'admitted_root_id')::uuid AND organization_id=org;
 ELSE
  IF TG_TABLE_NAME='migration_history_import_anchor' THEN SELECT owner_run_id INTO STRICT owner_id FROM migration_history_import_plan WHERE id=(v->>'plan_id')::uuid AND organization_id=org;
  ELSE owner_id:=(v->>CASE WHEN TG_TABLE_NAME='migration_history_import_run' THEN 'id' ELSE 'owner_run_id' END)::uuid;END IF;
  UPDATE migration_history_import_run SET retained_bytes=retained_bytes+delta WHERE id=owner_id AND organization_id=org;
 END IF;
 UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+delta WHERE organization_id=org;
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_history_import_person_erasure
CREATE OR REPLACE FUNCTION public.crm_history_import_person_erasure()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=OLD.organization_id AND person_id=OLD.id AND erased_at IS NULL;
 DELETE FROM migration_history_import_display d USING migration_history_import_manifest m
 WHERE d.id=m.id AND d.organization_id=m.organization_id AND m.organization_id=OLD.organization_id
 AND (m.person_id=OLD.id OR EXISTS(SELECT 1 FROM migration_history_person_link l WHERE l.observation_id=m.observation_id AND l.organization_id=m.organization_id AND l.person_id=OLD.id));
 RETURN OLD;
END $function$


-- crm_history_import_retained_size
CREATE OR REPLACE FUNCTION public.crm_history_import_retained_size(v jsonb)
 RETURNS bigint
 LANGUAGE plpgsql
 IMMUTABLE
AS $function$
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
END $function$


-- crm_history_review_call_link
CREATE OR REPLACE FUNCTION public.crm_history_review_call_link()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
DECLARE r JSONB;
BEGIN FOR r IN SELECT to_jsonb(OLD) WHERE TG_OP<>'INSERT' UNION ALL SELECT to_jsonb(NEW) WHERE TG_OP<>'DELETE' LOOP
 UPDATE migration_history_review_state s SET revision=revision+1 FROM contact_attempted c WHERE c.organization_id=(r->>'organization_id')::uuid AND c.causation_id=(r->>'id')::uuid AND s.organization_id=c.organization_id AND s.person_id=c.person_id;
 END LOOP; RETURN NULL; END $function$


-- crm_history_review_core_rows
CREATE OR REPLACE FUNCTION public.crm_history_review_core_rows()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 IF TG_OP<>'INSERT' THEN PERFORM crm_history_review_delta(OLD.organization_id,OLD.person_id,NULL,0); END IF;
 IF TG_OP<>'DELETE' THEN PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,NULL,0); END IF;
 RETURN NULL;
END $function$


-- crm_history_review_custom_field_label
CREATE OR REPLACE FUNCTION public.crm_history_review_custom_field_label()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$ BEGIN IF OLD IS DISTINCT FROM NEW THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,person_id FROM person_custom_field_value WHERE field_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $function$


-- crm_history_review_custom_field_option_label
CREATE OR REPLACE FUNCTION public.crm_history_review_custom_field_option_label()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$ BEGIN IF OLD IS DISTINCT FROM NEW THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,person_id FROM person_custom_field_value WHERE option_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $function$


-- crm_history_review_delta
CREATE OR REPLACE FUNCTION public.crm_history_review_delta(o uuid, p uuid, k text, d bigint)
 RETURNS void
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 IF p IS NULL THEN RETURN; END IF;
 UPDATE migration_history_review_state SET revision=revision+1,
 counts=CASE WHEN k IS NULL THEN counts ELSE jsonb_set(counts,ARRAY[k],to_jsonb((counts->>k)::bigint+d)) END
 WHERE organization_id=o AND person_id=p;
END $function$


-- crm_history_review_person
CREATE OR REPLACE FUNCTION public.crm_history_review_person()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 IF TG_OP='INSERT' THEN INSERT INTO migration_history_review_state(organization_id,person_id) VALUES(NEW.organization_id,NEW.id);
 ELSE PERFORM crm_history_review_delta(NEW.organization_id,NEW.id,NULL,0); END IF; RETURN NEW;
END $function$


-- crm_history_review_rows
CREATE OR REPLACE FUNCTION public.crm_history_review_rows()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
DECLARE r JSONB; k TEXT; sign BIGINT;
BEGIN
 FOR r,sign IN SELECT to_jsonb(OLD),-1::bigint WHERE TG_OP<>'INSERT' UNION ALL SELECT to_jsonb(NEW),1::bigint WHERE TG_OP<>'DELETE' LOOP
  k=CASE TG_TABLE_NAME WHEN 'inquiry' THEN 'inquiries' WHEN 'note' THEN CASE WHEN r->>'deleted_at' IS NULL THEN 'notes' END
    WHEN 'task' THEN CASE WHEN r->>'deleted_at' IS NULL THEN CASE WHEN r->>'completed_at' IS NULL THEN 'open_tasks' ELSE 'completed_tasks' END END
    WHEN 'correspondence_captured' THEN 'correspondence' ELSE TG_TABLE_NAME END;
  PERFORM crm_history_review_delta((r->>'organization_id')::uuid,(r->>'person_id')::uuid,k,sign);
 END LOOP; RETURN NULL;
END $function$


-- crm_history_review_stage_label
CREATE OR REPLACE FUNCTION public.crm_history_review_stage_label()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$ BEGIN IF OLD.name IS DISTINCT FROM NEW.name THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,id AS person_id FROM person WHERE stage_id=NEW.id UNION SELECT organization_id,person_id FROM stage_changed WHERE from_stage_id=NEW.id OR to_stage_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $function$


-- crm_history_review_tag_label
CREATE OR REPLACE FUNCTION public.crm_history_review_tag_label()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$ BEGIN IF OLD.name IS DISTINCT FROM NEW.name THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,person_id FROM person_tag WHERE tag_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $function$


-- crm_history_review_user_label
CREATE OR REPLACE FUNCTION public.crm_history_review_user_label()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$ BEGIN IF OLD.display_name IS DISTINCT FROM NEW.display_name THEN UPDATE migration_history_review_state s SET revision=revision+1 FROM (SELECT organization_id,id AS person_id FROM person WHERE assigned_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM inquiry_received WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM routing_decision WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM assignment_changed WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM stage_changed WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM contact_attempted WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM call_completed WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM routing_decision WHERE assignee_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM assignment_changed WHERE from_user_id=NEW.id OR to_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM correspondence_captured WHERE on_behalf_of_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_event_record_imported WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_call_record_imported WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_text_record_imported WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_event_record_corrected WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_call_record_corrected WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_text_record_corrected WHERE actor_user_id=NEW.id) p WHERE s.organization_id=p.organization_id AND s.person_id=p.person_id; END IF; RETURN NEW; END $function$


-- crm_mapping_repair_capability
CREATE OR REPLACE FUNCTION public.crm_mapping_repair_capability(org uuid)
 RETURNS void
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
BEGIN
 IF current_user='crm_app' AND EXISTS(SELECT 1 FROM migration_mapping_repair_requirement WHERE organization_id=org)
    AND current_setting('crm.mapping_repair_reader',true) IS DISTINCT FROM 'fub-people-mapping-repair-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010R',MESSAGE='mapping_repair_capability_required';
 END IF;
END $function$


-- crm_mapping_repair_head_write
CREATE OR REPLACE FUNCTION public.crm_mapping_repair_head_write()
 RETURNS trigger
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE b JSONB; p TEXT:=TG_ARGV[0];
BEGIN
 EXECUTE format('SELECT to_jsonb(b) FROM %I b WHERE id=$1 AND organization_id=$2',p||'_mapping_binding') INTO b USING NEW.binding_id,NEW.organization_id;
 IF b IS NULL OR (TG_OP='INSERT' AND (NEW.version<>1 OR b->>'previous_binding_id' IS NOT NULL))
 OR (TG_OP='UPDATE' AND ((to_jsonb(OLD)-ARRAY['binding_id','version']) IS DISTINCT FROM (to_jsonb(NEW)-ARRAY['binding_id','version']) OR NEW.version<>OLD.version+1 OR (b->>'previous_binding_id')::uuid IS DISTINCT FROM OLD.binding_id)) THEN RAISE EXCEPTION 'mapping head changed'; END IF;
 RETURN NEW;
END $function$


-- crm_mapping_repair_identity
CREATE OR REPLACE FUNCTION public.crm_mapping_repair_identity(owner_name text, root jsonb, item jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
 STABLE
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE valid BOOLEAN;
BEGIN
 IF owner_name='migration_people_refresh' THEN
  SELECT EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.import_id=r.import_id AND mi.manifest_id=r.manifest_id AND mi.source_id=r.source_id AND mi.target_id=r.person_id AND mi.family='people'
   WHERE r.id=(item->>'original_result_id')::uuid AND r.import_id=(root->>'parent_import_id')::uuid AND r.organization_id=(root->>'organization_id')::uuid AND r.source_id=item->>'source_id' AND r.person_id=(item->>'person_id')::uuid AND r.disposition='imported' AND mi.source_account_id=(root->>'source_account_id')::bigint) INTO valid;
 ELSIF owner_name='migration_admitted_people_refresh' THEN
  SELECT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.admission_result_id=r.id AND mi.admission_id=r.admission_id AND mi.admission_item_id=r.item_id AND mi.source_id=r.source_id AND mi.target_id=r.person_id AND mi.family='people'
   WHERE r.id=(item->>'admission_result_id')::uuid AND r.admission_id=(root->>'admission_id')::uuid AND r.organization_id=(root->>'organization_id')::uuid AND r.source_id=item->>'source_id' AND r.person_id=(item->>'person_id')::uuid AND r.disposition='settled' AND mi.source_account_id=(root->>'source_account_id')::bigint) INTO valid;
 ELSE RETURN false; END IF;
 RETURN valid AND EXISTS(SELECT 1 FROM migration_snapshot_record r JOIN migration_snapshot_capture c ON c.id=r.capture_id AND c.snapshot_id=r.snapshot_id AND c.organization_id=r.organization_id WHERE r.organization_id=(root->>'organization_id')::uuid AND r.snapshot_id=(root->>'newer_snapshot_id')::uuid AND r.source_id=item->>'source_id' AND r.family='people' AND r.capture_id=(item->>'source_capture_id')::uuid AND r.ordinal=(item->>'source_ordinal')::integer AND c.accepted AND NOT c.truncated AND c.classification='success' AND c.http_status BETWEEN 200 AND 299);
END $function$


-- crm_mapping_repair_mutation_evidence
CREATE OR REPLACE FUNCTION public.crm_mapping_repair_mutation_evidence(org uuid, owner_name text, permit text, table_name text)
 RETURNS boolean
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE unit UUID; lease UUID; item JSONB; root JSONB; baseline JSONB; cohort TEXT;
BEGIN
 IF owner_name NOT IN ('migration_people_refresh','migration_admitted_people_refresh') THEN RETURN false; END IF;
 PERFORM crm_mapping_repair_capability(org);
 BEGIN unit:=(permit::jsonb->>'item')::uuid; lease:=(permit::jsonb->>'lease')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 EXECUTE format('SELECT to_jsonb(i),to_jsonb(r) FROM %I i JOIN %I r ON r.id=i.refresh_id AND r.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2 AND r.lease_token=$3 AND r.state=''running'' AND r.lease_expires_at>clock_timestamp() AND i.plan_id=r.confirmed_refresh_plan_id AND i.settled_at IS NULL',owner_name||'_item',owner_name) INTO item,root USING unit,org,lease;
 IF item IS NULL OR item->>'mapping_evidence_nonce' IS NULL OR NOT crm_mapping_repair_identity(owner_name,root,item) THEN RETURN false; END IF;
 cohort:=CASE owner_name WHEN 'migration_people_refresh' THEN 'parent_import_id' ELSE 'admission_id' END;
 EXECUTE format('SELECT to_jsonb(b) FROM %I b WHERE organization_id=$1 AND %I=$2 AND source_id=$3 AND person_id=$4',owner_name||'_baseline',cohort) INTO baseline USING org,(root->>cohort)::uuid,item->>'source_id',(item->>'person_id')::uuid;
 IF baseline IS NULL OR baseline->>'version' IS DISTINCT FROM item->>'baseline_version' OR baseline->>'result_id' IS DISTINCT FROM item->>'baseline_result_id' THEN RETURN false; END IF;
 IF table_name='person' AND decode(substr(item->>'native_fingerprint',3),'hex') IS DISTINCT FROM crm_mapping_repair_native_fingerprint(org,(item->>'person_id')::uuid) THEN RETURN false; END IF;
 RETURN true;
END $function$


-- crm_mapping_repair_native_fingerprint
CREATE OR REPLACE FUNCTION public.crm_mapping_repair_native_fingerprint(org uuid, target uuid)
 RETURNS bytea
 LANGUAGE sql
 STABLE
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
 SELECT sha256(convert_to(jsonb_build_object('first_name',p.first_name,'last_name',p.last_name,
  'stage',p.stage_id,'assignee',p.assigned_user_id,'contacts',
  COALESCE((SELECT jsonb_agg(jsonb_build_array(c.id,c.kind,c.value,c.normalized_value,c.import_order) ORDER BY c.kind,c.import_order,c.id)
    FROM contact_method c WHERE c.person_id=p.id AND c.organization_id=p.organization_id),'[]'::jsonb))::text,'UTF8'))
 FROM person p WHERE p.id=target AND p.organization_id=org
$function$


-- crm_mapping_repair_owned_write
CREATE OR REPLACE FUNCTION public.crm_mapping_repair_owned_write()
 RETURNS trigger
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
#variable_conflict use_variable
DECLARE p TEXT:=TG_ARGV[0]; family TEXT:=TG_ARGV[1]; n JSONB:=to_jsonb(NEW);
 root JSONB; item JSONB; plan JSONB; source JSONB; binding JSONB; head JSONB;
 valid BOOLEAN; kind TEXT; choice JSONB; column_prefix TEXT; permit JSONB; cohort TEXT;
BEGIN
 IF family='root' THEN root:=n;
 ELSE
  EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE id=$1 AND organization_id=$2',p)
   INTO root USING (n->>'refresh_id')::uuid,(n->>'organization_id')::uuid;
 END IF;
 cohort:=CASE p WHEN 'migration_people_refresh' THEN 'parent_import_id' ELSE 'admission_id' END;
 IF family='root' THEN
  IF TG_OP='UPDATE' AND (to_jsonb(OLD)-ARRAY['state','pause_reason','lease_token','lease_epoch','lease_expires_at',
   'preparation_checkpoint_key','preparation_checkpoint_id','preparation_phase','checkpoint_id','lifecycle_revision',
   'retained_bytes','reserved_bytes','settled_items','updated_at','completed_at','cancelled_at',
   'confirmed_boundary','confirmed_snapshot_id','confirmed_started_at','confirmed_completed_at','confirmed_refresh_plan_id',
   'repair_draft_revision','repair_frozen_revision','repair_choices_digest','repair_candidate_count','repair_candidates_complete','repair_checkpoint_id'])
   IS DISTINCT FROM (n-ARRAY['state','pause_reason','lease_token','lease_epoch','lease_expires_at',
   'preparation_checkpoint_key','preparation_checkpoint_id','preparation_phase','checkpoint_id','lifecycle_revision',
   'retained_bytes','reserved_bytes','settled_items','updated_at','completed_at','cancelled_at',
   'confirmed_boundary','confirmed_snapshot_id','confirmed_started_at','confirmed_completed_at','confirmed_refresh_plan_id',
   'repair_draft_revision','repair_frozen_revision','repair_choices_digest','repair_candidate_count','repair_candidates_complete','repair_checkpoint_id']) THEN
    RAISE EXCEPTION 'refresh owner is immutable';
  END IF;
  IF n->>'repair_source_refresh_id' IS NOT NULL THEN
   EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE id=$1 AND organization_id=$2',p) INTO source
    USING (n->>'repair_source_refresh_id')::uuid,(n->>'organization_id')::uuid;
   IF source IS NULL OR source->>'parent_import_id' IS DISTINCT FROM n->>'parent_import_id'
    OR source->>'parent_plan_id' IS DISTINCT FROM n->>'parent_plan_id'
    OR source->>'source_account_id' IS DISTINCT FROM n->>'source_account_id'
    OR source->>'admission_id' IS DISTINCT FROM n->>'admission_id' THEN RAISE EXCEPTION 'repair owner mismatch'; END IF;
   IF TG_OP='INSERT' AND source->>'state' NOT IN ('completed','cancelled') THEN RAISE EXCEPTION 'repair source not terminal'; END IF;
   IF n->>'confirmed_refresh_plan_id' IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_mapping_repair_requirement WHERE organization_id=(n->>'organization_id')::uuid) THEN RAISE EXCEPTION 'repair admission required'; END IF;
   IF TG_OP='UPDATE' AND OLD.confirmed_refresh_plan_id IS NOT NULL AND
     (to_jsonb(OLD)->'repair_choices_digest' IS DISTINCT FROM n->'repair_choices_digest'
      OR to_jsonb(OLD)->'repair_frozen_revision' IS DISTINCT FROM n->'repair_frozen_revision') THEN RAISE EXCEPTION 'confirmed choices are immutable'; END IF;
  END IF;
  RETURN NEW;
 END IF;
 IF family='candidate' THEN
  EXECUTE format('SELECT to_jsonb(i) FROM %I i WHERE id=$1 AND refresh_id=$2 AND plan_id=$3 AND organization_id=$4',p||'_item') INTO item
   USING NEW.anchor_item_id,NEW.anchor_refresh_id,NEW.anchor_plan_id,NEW.organization_id;
  IF root->>'state'<>'preparing' OR (root->>'repair_candidates_complete')::boolean OR item IS NULL
   OR item->>'source_id' IS DISTINCT FROM n->>'source_id' OR item->>'person_id' IS DISTINCT FROM n->>'person_id'
   OR root->>'repair_source_refresh_id' IS DISTINCT FROM n->>'anchor_refresh_id'
   OR root->>'repair_source_plan_id' IS DISTINCT FROM n->>'anchor_plan_id' THEN RAISE EXCEPTION 'invalid repair candidate'; END IF;
  EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I z WHERE z.refresh_id=$1 AND z.item_id=$2 AND z.organization_id=$3 AND z.disposition IN (''held_mapping_gap'',''held_stale''))',p||'_result') INTO valid USING NEW.anchor_refresh_id,NEW.anchor_item_id,NEW.organization_id;
  EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE id=$1 AND organization_id=$2',p) INTO source USING NEW.anchor_refresh_id,NEW.organization_id;
  IF source->>'state'='cancelled' AND source->>'repair_source_refresh_id' IS NOT NULL AND source->>'confirmed_refresh_plan_id'=root->>'repair_source_plan_id' THEN
   IF source->>'report_id' IS DISTINCT FROM root->>'report_id' OR item->>'settled_at' IS NOT NULL OR item->>'disposition' NOT IN ('eligible','already_current') THEN RAISE EXCEPTION 'remainder requires unfinished frozen candidate'; END IF;
  ELSIF item->>'disposition'<>'held_mapping_gap' AND NOT (root->>'repair_anchor_kind'='results' AND valid) THEN RAISE EXCEPTION 'candidate is not held'; END IF;
  IF n->>'successful_result_id' IS DISTINCT FROM item->>(CASE p WHEN 'migration_people_refresh' THEN 'original_result_id' ELSE 'admission_result_id' END) THEN RAISE EXCEPTION 'candidate success mismatch'; END IF;
  IF p='migration_people_refresh' THEN
   SELECT EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.manifest_id=r.manifest_id AND mi.import_id=r.import_id AND mi.source_id=r.source_id AND mi.target_id=r.person_id AND mi.family='people'
    WHERE r.import_id=(root->>'parent_import_id')::uuid AND r.organization_id=NEW.organization_id AND r.person_id=NEW.person_id AND r.source_id=NEW.source_id AND r.disposition='imported' AND mi.source_account_id=(root->>'source_account_id')::bigint) INTO valid;
  ELSE
   SELECT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.admission_result_id=r.id AND mi.target_id=r.person_id AND mi.source_id=r.source_id AND mi.family='people'
    WHERE r.admission_id=(root->>'admission_id')::uuid AND r.organization_id=NEW.organization_id AND r.person_id=NEW.person_id AND r.source_id=NEW.source_id AND r.disposition='settled' AND mi.source_account_id=(root->>'source_account_id')::bigint) INTO valid;
  END IF;
  IF NOT valid THEN RAISE EXCEPTION 'candidate requires successful Person identity'; END IF;
 ELSIF family='plan' THEN
  IF TG_OP='UPDATE' AND OLD.sealed_at IS NOT NULL AND (to_jsonb(OLD)-'state') IS DISTINCT FROM (n-'state') THEN RAISE EXCEPTION 'sealed plan is immutable'; END IF;
  IF NEW.sealed_at IS NOT NULL AND root->>'repair_source_refresh_id' IS NOT NULL AND
    (n->>'repair_choices_revision' IS DISTINCT FROM root->>'repair_frozen_revision' OR n->>'repair_choices_digest' IS DISTINCT FROM root->>'repair_choices_digest' OR n->>'repair_candidate_count' IS DISTINCT FROM root->>'repair_candidate_count') THEN RAISE EXCEPTION 'plan choices mismatch'; END IF;
 ELSIF family='choice' THEN
  IF NEW.inherited_choice_id IS NOT NULL THEN
   EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE id=$1 AND organization_id=$2',p) INTO source USING NEW.inherited_refresh_id,NEW.organization_id;
   EXECUTE format('SELECT to_jsonb(c) FROM %I c WHERE id=$1 AND refresh_id=$2 AND organization_id=$3',p||'_repair_choice') INTO choice USING NEW.inherited_choice_id,NEW.inherited_refresh_id,NEW.organization_id;
   IF root->>'state'<>'preparing' OR (root->>'repair_candidates_complete')::boolean OR NEW.revision<>1
    OR root->>'repair_source_refresh_id' IS DISTINCT FROM source->>'id' OR source->>'state'<>'cancelled'
    OR source->>'confirmed_refresh_plan_id' IS DISTINCT FROM root->>'repair_source_plan_id'
    OR source->>'report_id' IS DISTINCT FROM root->>'report_id' OR choice IS NULL
    OR (choice->>'revision')::bigint>(source->>'repair_frozen_revision')::bigint
    OR choice->>'kind' IS DISTINCT FROM n->>'kind' OR choice->>'source_key_hmac' IS DISTINCT FROM n->>'source_key_hmac'
    OR choice->>'disposition' IS DISTINCT FROM n->>'disposition' OR choice->>'target_id' IS DISTINCT FROM n->>'target_id'
    OR NEW.approved_by_user_id<>(root->>'initiated_by_user_id')::uuid THEN RAISE EXCEPTION 'invalid inherited choice'; END IF;
   RETURN NEW;
  END IF;
  IF root->>'confirmed_refresh_plan_id' IS NOT NULL OR root->>'repair_source_refresh_id' IS NULL
   OR root->>'state' NOT IN ('paused','ready') OR NEW.revision<>(root->>'repair_draft_revision')::bigint+1
   OR NEW.approved_by_user_id<>(root->>'initiated_by_user_id')::uuid THEN RAISE EXCEPTION 'choice revision or owner mismatch'; END IF;
 ELSIF family='item' THEN
  EXECUTE format('SELECT to_jsonb(v) FROM %I v WHERE id=$1 AND refresh_id=$2 AND organization_id=$3',p||'_plan') INTO plan USING NEW.plan_id,NEW.refresh_id,NEW.organization_id;
  IF TG_OP='UPDATE' AND plan->>'state'<>'building' AND
   (to_jsonb(OLD)-ARRAY['settled_result_id','settled_at']) IS DISTINCT FROM (n-ARRAY['settled_result_id','settled_at']) THEN RAISE EXCEPTION 'sealed item is immutable'; END IF;
  -- A held result must remain recordable when its old mapping was invalidated.
  IF TG_OP='UPDATE' AND plan->>'state'<>'building' THEN
   IF OLD.settled_at IS NOT NULL AND (OLD.settled_result_id IS DISTINCT FROM NEW.settled_result_id OR OLD.settled_at IS DISTINCT FROM NEW.settled_at) THEN RAISE EXCEPTION 'settlement is immutable'; END IF;
   RETURN NEW;
  END IF;
  IF NEW.mapping_evidence_nonce IS NOT NULL THEN
   IF octet_length(NEW.mapping_evidence_nonce)<>24 OR octet_length(NEW.mapping_evidence_ciphertext)>131072 OR octet_length(NEW.native_fingerprint)<>32 THEN RAISE EXCEPTION 'invalid mapping evidence'; END IF;
   FOREACH kind IN ARRAY ARRAY['stage','assignee'] LOOP
    column_prefix:='repair_'||kind;
    IF n->>(column_prefix||'_choice_id') IS NOT NULL AND n->>(kind||'_mapping_id') IS NOT NULL THEN RAISE EXCEPTION 'mixed mapping authority'; END IF;
    IF n->>(kind||'_mapping_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_import_mapping m WHERE m.id=(n->>(kind||'_mapping_id'))::uuid AND m.organization_id=NEW.organization_id AND m.plan_id=(root->>'parent_plan_id')::uuid AND m.kind=kind AND m.qualified) THEN RAISE EXCEPTION 'wrong original mapping owner'; END IF;
    IF n->>(column_prefix||'_choice_id') IS NOT NULL THEN
     EXECUTE format('SELECT to_jsonb(c) FROM %I c WHERE id=$1 AND refresh_id=$2 AND organization_id=$3',p||'_repair_choice') INTO choice USING (n->>(column_prefix||'_choice_id'))::uuid,(n->>(column_prefix||'_choice_refresh_id'))::uuid,NEW.organization_id;
     IF choice IS NULL OR choice->>'kind'<>kind OR choice->>( 'source_key_hmac') IS DISTINCT FROM n->>(column_prefix||'_source_hmac') THEN RAISE EXCEPTION 'wrong repair key'; END IF;
     IF n->>(column_prefix||'_choice_refresh_id')=root->>'id' THEN
      EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I c WHERE c.refresh_id=$1 AND c.organization_id=$2 AND c.source_id=$3 AND c.person_id=$4 AND %I=$5)',p||'_repair_candidate',CASE kind WHEN 'stage' THEN 'stage_source_hmac' ELSE 'assignee_source_hmac' END) INTO valid USING NEW.refresh_id,NEW.organization_id,NEW.source_id,NEW.person_id,decode(substr(n->>(column_prefix||'_source_hmac'),3),'hex');
      IF NOT valid THEN RAISE EXCEPTION 'choice outside frozen candidates'; END IF;
     END IF;
    END IF;
   END LOOP;
  END IF;
 ELSIF family='baseline' AND TG_OP='UPDATE' THEN
  EXECUTE format('SELECT to_jsonb(i) FROM %I i JOIN %I z ON z.item_id=i.id AND z.refresh_id=i.refresh_id AND z.organization_id=i.organization_id WHERE z.id=$1 AND z.refresh_id=$2 AND z.organization_id=$3 AND z.disposition IN (''settled'',''settled_noop'')',p||'_item',p||'_result') INTO item USING NEW.result_id,NEW.refresh_id,NEW.organization_id;
  IF item IS NULL OR NEW.version<>OLD.version+1 OR item->>'baseline_result_id' IS DISTINCT FROM to_jsonb(OLD)->>'result_id'
   OR (item->>'baseline_version')::bigint<>OLD.version OR NEW.projection_row_id<>(item->>'id')::uuid
   OR NEW.person_id<>OLD.person_id OR NEW.source_id<>OLD.source_id OR n->>cohort IS DISTINCT FROM to_jsonb(OLD)->>cohort
   OR NEW.organization_id<>OLD.organization_id THEN RAISE EXCEPTION 'baseline successor mismatch'; END IF;
 ELSIF family IN ('result','binding') THEN
  EXECUTE format('SELECT to_jsonb(i) FROM %I i WHERE id=$1 AND refresh_id=$2 AND organization_id=$3',p||'_item') INTO item USING NEW.item_id,NEW.refresh_id,NEW.organization_id;
  IF family='result' AND n->>'disposition' NOT IN ('settled','settled_noop') THEN RETURN NEW; END IF;
  -- Legacy plans remain readable; compatible workers hold them for a fresh preview.
  IF item->>'mapping_evidence_nonce' IS NULL OR NOT crm_mapping_repair_identity(p,root,item) THEN RAISE EXCEPTION 'fresh mapping preview or source identity required'; END IF;
  BEGIN permit:=current_setting('crm.mapping_repair_settlement',true)::jsonb; EXCEPTION WHEN others THEN permit:=NULL; END;
  IF item IS NULL OR root->>'state'<>'running' OR root->>'lease_token' IS DISTINCT FROM permit->>'lease'
   OR item->>'id' IS DISTINCT FROM permit->>'item' OR (root->>'lease_expires_at')::timestamptz<=clock_timestamp()
   OR item->>'settled_at' IS NOT NULL OR item->>'plan_id' IS DISTINCT FROM root->>'confirmed_refresh_plan_id'
   OR item->>'person_id' IS DISTINCT FROM n->>'person_id' OR item->>'source_id' IS DISTINCT FROM n->>'source_id'
   OR NOT EXISTS(SELECT 1 FROM organization_membership m JOIN migration_workspace w ON w.organization_id=m.organization_id WHERE m.organization_id=NEW.organization_id AND m.user_id=(root->>'initiated_by_user_id')::uuid AND m.status='active' AND m.role='admin' AND w.import_id=(root->>'parent_import_id')::uuid AND w.plan_id=(root->>'parent_plan_id')::uuid) THEN RAISE EXCEPTION 'settlement lease or identity mismatch'; END IF;
  EXECUTE format('SELECT to_jsonb(b) FROM %I b WHERE organization_id=$1 AND %I=$2 AND source_id=$3 AND person_id=$4',p||'_baseline',cohort) INTO head USING NEW.organization_id,(root->>cohort)::uuid,NEW.source_id,NEW.person_id;
  IF head IS NULL OR head->>'result_id' IS DISTINCT FROM item->>'baseline_result_id' OR head->>'version' IS DISTINCT FROM item->>'baseline_version' THEN RAISE EXCEPTION 'baseline changed'; END IF;
  EXECUTE format('SELECT to_jsonb(r) FROM %I r WHERE organization_id=$1 AND %I=$2 AND confirmed_snapshot_id IS NOT NULL AND confirmed_completed_at IS NOT NULL ORDER BY confirmed_completed_at DESC,id DESC LIMIT 1',p,cohort) INTO source USING NEW.organization_id,(root->>cohort)::uuid;
  IF source IS NOT NULL AND (source->>'confirmed_snapshot_id'<>root->>'newer_snapshot_id' OR source->>'newer_sequence'<>root->>'newer_sequence')
    AND (root->>'newer_started_at')::timestamptz<=(source->>'confirmed_completed_at')::timestamptz THEN RAISE EXCEPTION 'stale source boundary'; END IF;
  FOREACH kind IN ARRAY ARRAY['stage','assignee'] LOOP
   IF family='binding' AND kind<>n->>'kind' THEN CONTINUE; END IF;
   column_prefix:='repair_'||kind;
   IF item->>(column_prefix||'_source_hmac') IS NOT NULL THEN
    EXECUTE format('SELECT to_jsonb(h) FROM %I h WHERE organization_id=$1 AND person_id=$2 AND source_account_id=$3 AND kind=$4 AND source_key_hmac=$5',p||'_mapping_head') INTO head USING NEW.organization_id,NEW.person_id,(root->>'source_account_id')::bigint,kind,decode(substr(item->>(column_prefix||'_source_hmac'),3),'hex');
    IF head->>'binding_id' IS DISTINCT FROM item->>(column_prefix||'_head_id') OR COALESCE((head->>'version')::bigint,0)<>(item->>(column_prefix||'_head_version'))::bigint THEN RAISE EXCEPTION 'mapping head changed'; END IF;
    IF item->>(column_prefix||'_choice_refresh_id')=root->>'id' THEN
     EXECUTE format('SELECT to_jsonb(c) FROM %I c WHERE refresh_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4 AND revision<=$5 ORDER BY revision DESC LIMIT 1',p||'_repair_choice') INTO choice USING NEW.refresh_id,NEW.organization_id,kind,decode(substr(item->>(column_prefix||'_source_hmac'),3),'hex'),(root->>'repair_frozen_revision')::bigint;
     IF choice->>'id' IS DISTINCT FROM item->>(column_prefix||'_choice_id') THEN RAISE EXCEPTION 'choice not in confirmed revision'; END IF;
    END IF;
   END IF;
  END LOOP;
  IF family='result' THEN
   IF NEW.disposition='settled_noop' AND (item->>'disposition'<>'already_current' OR decode(substr(item->>'native_fingerprint',3),'hex') IS DISTINCT FROM crm_mapping_repair_native_fingerprint(NEW.organization_id,NEW.person_id)) THEN RAISE EXCEPTION 'stale no-op'; END IF;
  ELSE
   EXECUTE format('SELECT EXISTS(SELECT 1 FROM %I z WHERE z.id=$1 AND z.refresh_id=$2 AND z.item_id=$3 AND z.organization_id=$4 AND z.disposition IN (''settled'',''settled_noop''))',p||'_result') INTO valid USING NEW.result_id,NEW.refresh_id,NEW.item_id,NEW.organization_id;
   IF NOT valid OR NEW.choice_refresh_id<>NEW.refresh_id OR NEW.plan_id<>(item->>'plan_id')::uuid OR NEW.source_account_id<>(root->>'source_account_id')::bigint THEN RAISE EXCEPTION 'binding requires owned success'; END IF;
   column_prefix:='repair_'||NEW.kind;
   IF n->>'choice_id' IS DISTINCT FROM item->>(column_prefix||'_choice_id') OR n->>'source_key_hmac' IS DISTINCT FROM item->>(column_prefix||'_source_hmac') OR n->>'previous_binding_id' IS DISTINCT FROM item->>(column_prefix||'_head_id') THEN RAISE EXCEPTION 'binding differs from preview'; END IF;
  END IF;
 END IF;
 RETURN NEW;
END $function$


-- crm_mapping_repair_target
CREATE OR REPLACE FUNCTION public.crm_mapping_repair_target(org uuid, kind text, target uuid)
 RETURNS jsonb
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE label TEXT;
BEGIN
 IF kind='stage' THEN
  SELECT name INTO label FROM stage WHERE organization_id=org AND id=target FOR SHARE;
  IF FOUND THEN RETURN jsonb_build_object('id',target,'name',label); END IF;
 ELSIF kind='assignee' THEN
  SELECT u.email INTO label FROM organization_membership m JOIN app_user u ON u.id=m.user_id
   WHERE m.organization_id=org AND m.user_id=target AND m.status='active' FOR SHARE OF m,u;
  IF FOUND THEN RETURN jsonb_build_object('id',target,'email',label); END IF;
 END IF;
 RETURN NULL;
END $function$


-- crm_mapping_repair_write_fence
CREATE OR REPLACE FUNCTION public.crm_mapping_repair_write_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE v JSONB;
BEGIN
 v:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 PERFORM crm_mapping_repair_capability((v->>'organization_id')::uuid);
 IF current_user='crm_app' AND v->>'repair_source_refresh_id' IS NOT NULL
    AND current_setting('crm.mapping_repair_reader',true) IS DISTINCT FROM 'fub-people-mapping-repair-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010R',MESSAGE='mapping_repair_capability_required';
 END IF;
 RETURN COALESCE(NEW,OLD);
END $function$


-- crm_metadata_identity_guard
CREATE OR REPLACE FUNCTION public.crm_metadata_identity_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF current_user='crm_app' AND EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=NEW.organization_id AND state='ready') AND NOT crm_metadata_insert_allowed(NEW.organization_id,'migration_metadata_identity',to_jsonb(NEW)) THEN RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='metadata_identity_permit_required'; END IF;
 RETURN NEW;
END $function$


-- crm_metadata_identity_key_matches
CREATE OR REPLACE FUNCTION public.crm_metadata_identity_key_matches(source_key bytea, row_value jsonb)
 RETURNS boolean
 LANGUAGE sql
 IMMUTABLE STRICT
AS $function$
 SELECT encode(source_key,'hex')=substring(row_value->>'source_key' from 3)
$function$


-- crm_metadata_insert_allowed
CREATE OR REPLACE FUNCTION public.crm_metadata_insert_allowed(org uuid, table_name text, row_value jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
AS $function$
DECLARE token TEXT:=current_setting('crm.metadata_token',true); unit_text TEXT:=current_setting('crm.metadata_unit',true); unit UUID; run UUID; plan UUID; proof TEXT:=current_setting('crm.metadata_claim_v1',true); parsed_proof JSONB; claims_ready BOOLEAN;
BEGIN
 IF token IS NULL OR token='' OR unit_text IS NULL OR unit_text='' THEN RETURN false; END IF;
 claims_ready:=EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=org AND state='ready');
 IF claims_ready THEN
  IF proof IS NULL OR proof='' THEN RETURN false; END IF;
  BEGIN parsed_proof:=proof::jsonb; EXCEPTION WHEN invalid_text_representation THEN RETURN false; END;
  IF jsonb_typeof(parsed_proof) IS DISTINCT FROM 'object' THEN RETURN false; END IF;
 END IF;
 BEGIN unit:=unit_text::uuid; EXCEPTION WHEN invalid_text_representation THEN RETURN false; END;
 SELECT i.id,i.confirmed_plan_id INTO run,plan FROM migration_metadata_import i
 JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id
 JOIN migration_import p ON p.id=i.parent_import_id AND p.organization_id=i.organization_id AND p.confirmed_plan_id=i.parent_plan_id
 JOIN organization o ON o.id=i.organization_id JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id
 WHERE i.organization_id=org AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp() AND p.state='completed' AND o.workspace_mode='migration_review' AND o.workspace_revision=i.workspace_revision AND m.role='admin' AND m.status='active';
 IF run IS NULL THEN RETURN false; END IF;
 IF EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=org AND state='ready')
    AND NOT (parsed_proof @> jsonb_build_object('organization_id',org::text,'root_id',run::text,'plan_id',plan::text,'lease',token,'unit_id',unit::text,'target_id',CASE WHEN table_name IN ('tag','custom_field') THEN row_value->>'id' WHEN table_name='custom_field_option' THEN row_value->>'id' WHEN table_name='person_tag' THEN row_value->>'tag_id' WHEN table_name='person_custom_field_value' THEN row_value->>'field_id' ELSE row_value->>'target_id' END)) THEN RETURN false; END IF;
 IF table_name IN ('tag','custom_field','custom_field_option') THEN RETURN EXISTS(SELECT 1 FROM migration_metadata_mapping a WHERE a.plan_id=plan AND a.import_id=run AND a.organization_id=org AND a.qualified AND a.disposition='create_matching' AND a.target_id=(row_value->>'id')::uuid AND ((table_name='tag' AND a.kind='tag' AND a.id=unit) OR (table_name='custom_field' AND a.kind='field' AND a.id=unit) OR (table_name='custom_field_option' AND a.kind='option' AND a.target_field_id=(row_value->>'field_id')::uuid AND (a.id=unit OR a.parent_mapping_id=unit))) AND (NOT EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness z WHERE z.organization_id=org AND z.state='ready') OR EXISTS(SELECT 1 FROM migration_metadata_catalog_claim c WHERE c.organization_id=org AND c.source_account_id=(SELECT source_account_id FROM migration_metadata_import WHERE id=run) AND c.kind=a.kind AND c.source_key=a.source_key AND c.target_id=a.target_id))); END IF;
 IF table_name IN ('person_tag','person_custom_field_value') THEN RETURN EXISTS(SELECT 1 FROM migration_metadata_manifest a JOIN migration_metadata_operation x ON x.manifest_id=a.id AND x.organization_id=a.organization_id LEFT JOIN migration_metadata_mapping m ON m.id=x.mapping_id AND m.organization_id=x.organization_id WHERE a.id=unit AND a.plan_id=plan AND a.import_id=run AND a.organization_id=org AND a.person_id=(row_value->>'person_id')::uuid AND x.disposition='eligible' AND ((table_name='person_tag' AND x.kind='tag_link' AND x.target_id=(row_value->>'tag_id')::uuid) OR(table_name='person_custom_field_value' AND x.kind='value' AND x.target_id=(row_value->>'field_id')::uuid)) AND (NOT EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness z WHERE z.organization_id=org AND z.state='ready') OR EXISTS(SELECT 1 FROM migration_metadata_catalog_claim c WHERE c.organization_id=org AND c.source_account_id=(SELECT source_account_id FROM migration_metadata_import WHERE id=run) AND c.kind=m.kind AND c.source_key=m.source_key AND c.target_id=x.target_id))); END IF;
 IF table_name='migration_metadata_identity' THEN RETURN EXISTS(SELECT 1 FROM migration_metadata_mapping a WHERE a.id=unit AND a.plan_id=plan AND a.import_id=run AND a.organization_id=org AND a.target_id=(row_value->>'target_id')::uuid AND a.kind=row_value->>'kind' AND crm_metadata_identity_key_matches(a.source_key,row_value) AND (NOT EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness z WHERE z.organization_id=org AND z.state='ready') OR EXISTS(SELECT 1 FROM migration_metadata_catalog_claim c WHERE c.organization_id=org AND c.source_account_id=(SELECT source_account_id FROM migration_metadata_import WHERE id=run) AND c.kind=a.kind AND c.source_key=a.source_key AND c.target_id=a.target_id))); END IF;
 RETURN false;
END $function$


-- crm_mobile_advance_metadata_catalog_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_advance_metadata_catalog_revision()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE o UUID; changed BOOLEAN:=false;
BEGIN
  IF TG_OP='INSERT' THEN o:=NEW.organization_id; changed:=true;
  ELSIF TG_OP='DELETE' THEN o:=OLD.organization_id; changed:=true;
  ELSIF TG_TABLE_NAME='tag' THEN
    o:=NEW.organization_id;
    changed:=ROW(NEW.organization_id,NEW.name) IS DISTINCT FROM ROW(OLD.organization_id,OLD.name);
  ELSIF TG_TABLE_NAME='custom_field' THEN
    o:=NEW.organization_id;
    changed:=ROW(NEW.organization_id,NEW.label,NEW.field_type,NEW.position,NEW.archived_at)
      IS DISTINCT FROM ROW(OLD.organization_id,OLD.label,OLD.field_type,OLD.position,OLD.archived_at);
  ELSIF TG_TABLE_NAME='custom_field_option' THEN
    o:=NEW.organization_id;
    changed:=ROW(NEW.organization_id,NEW.field_id,NEW.label,NEW.position,NEW.archived_at)
      IS DISTINCT FROM ROW(OLD.organization_id,OLD.field_id,OLD.label,OLD.position,OLD.archived_at);
  END IF;
  IF changed THEN
    UPDATE public.mobile_metadata_catalog SET revision=revision+1
      WHERE organization_id=o AND revision<9223372036854775807;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='metadata catalog revision overflow'; END IF;
  END IF;
  RETURN NULL;
END $function$


-- crm_mobile_advance_metadata_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_advance_metadata_revision()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE p UUID; o UUID;
BEGIN
  IF TG_OP='UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN RETURN NULL; END IF;
  IF TG_OP='INSERT' THEN p:=NEW.person_id; o:=NEW.organization_id;
  ELSE p:=OLD.person_id; o:=OLD.organization_id; END IF;
  UPDATE public.person SET mobile_revision=mobile_revision+1,
    metadata_revision=metadata_revision+1
    WHERE id=p AND organization_id=o AND mobile_revision<9223372036854775807
      AND metadata_revision<9223372036854775807;
  IF NOT FOUND AND EXISTS(SELECT 1 FROM public.person WHERE id=p AND organization_id=o) THEN
    RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='metadata revision overflow';
  END IF;
  RETURN NULL;
END $function$


-- crm_mobile_advance_stage_catalog_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_advance_stage_catalog_revision()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE
  org UUID;
  changed BOOLEAN := false;
BEGIN
  IF TG_TABLE_NAME <> 'stage' THEN
    RAISE EXCEPTION USING ERRCODE = '42501', MESSAGE = 'stage catalog trigger required';
  END IF;
  IF TG_OP = 'INSERT' THEN
    org := NEW.organization_id;
    changed := true;
  ELSIF TG_OP = 'DELETE' THEN
    org := OLD.organization_id;
    changed := true;
  ELSIF ROW(NEW.organization_id, NEW.name, NEW.position)
        IS DISTINCT FROM ROW(OLD.organization_id, OLD.name, OLD.position) THEN
    -- Stage ownership itself is immutable in normal application paths, but
    -- treating a schema-level move as a change preserves both catalogues.
    IF NEW.organization_id <> OLD.organization_id THEN
      UPDATE organization
        SET stage_catalog_revision = stage_catalog_revision + 1
        WHERE id = OLD.organization_id
          AND stage_catalog_revision < 9223372036854775807;
      IF NOT FOUND THEN
        RAISE EXCEPTION USING ERRCODE = '22003', MESSAGE = 'stage catalog revision overflow';
      END IF;
    END IF;
    org := NEW.organization_id;
    changed := true;
  END IF;
  IF changed THEN
    UPDATE organization
      SET stage_catalog_revision = stage_catalog_revision + 1
      WHERE id = org
        AND stage_catalog_revision < 9223372036854775807;
    IF NOT FOUND THEN
      RAISE EXCEPTION USING ERRCODE = '22003', MESSAGE = 'stage catalog revision overflow';
    END IF;
  END IF;
  RETURN NULL;
END $function$


-- crm_mobile_component_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_component_revision()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE p uuid; o uuid;
BEGIN
 IF TG_OP='UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN RETURN NULL; END IF;
 IF TG_OP<>'INSERT' THEN
   UPDATE public.person SET mobile_revision=mobile_revision+1 WHERE id=OLD.person_id AND organization_id=OLD.organization_id;
 END IF;
 IF TG_OP='INSERT' OR (TG_OP='UPDATE' AND ROW(NEW.person_id,NEW.organization_id) IS DISTINCT FROM ROW(OLD.person_id,OLD.organization_id)) THEN
   UPDATE public.person SET mobile_revision=mobile_revision+1 WHERE id=NEW.person_id AND organization_id=NEW.organization_id;
 END IF;
 RETURN NULL;
END $function$


-- crm_mobile_contact_details_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_contact_details_revision()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
BEGIN
  IF TG_OP='UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN RETURN NULL; END IF;
  IF TG_OP<>'INSERT' THEN
    UPDATE public.person SET details_revision=details_revision+1
      WHERE id=OLD.person_id AND organization_id=OLD.organization_id
        AND details_revision<9223372036854775807;
    IF NOT FOUND AND EXISTS(SELECT 1 FROM public.person
      WHERE id=OLD.person_id AND organization_id=OLD.organization_id) THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision overflow';
    END IF;
  END IF;
  IF TG_OP='INSERT' OR (TG_OP='UPDATE' AND
    ROW(NEW.person_id,NEW.organization_id) IS DISTINCT FROM
    ROW(OLD.person_id,OLD.organization_id)) THEN
    UPDATE public.person SET details_revision=details_revision+1
      WHERE id=NEW.person_id AND organization_id=NEW.organization_id
        AND details_revision<9223372036854775807;
    IF NOT FOUND AND EXISTS(SELECT 1 FROM public.person
      WHERE id=NEW.person_id AND organization_id=NEW.organization_id) THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision overflow';
    END IF;
  END IF;
  -- During a Person erasure cascade its row is already absent. There is no
  -- surviving profile token to invalidate, so contact deletion still succeeds.
  RETURN NULL;
END $function$


-- crm_mobile_create_metadata_catalog
CREATE OR REPLACE FUNCTION public.crm_mobile_create_metadata_catalog()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
BEGIN
  INSERT INTO public.mobile_metadata_catalog(organization_id) VALUES(NEW.id);
  RETURN NULL;
END $function$


-- crm_mobile_details_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_details_revision()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF ROW(NEW.first_name, NEW.last_name) IS DISTINCT FROM ROW(OLD.first_name, OLD.last_name) THEN
    IF OLD.details_revision = 9223372036854775807 THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision overflow';
    END IF;
    NEW.details_revision := OLD.details_revision + 1;
  ELSIF NEW.details_revision IS DISTINCT FROM OLD.details_revision THEN
    -- The contact AFTER trigger issues its parent UPDATE, so this BEFORE
    -- trigger is nested. App roles cannot create triggers or invoke a trigger
    -- function directly. No caller-controlled session setting grants this path.
    IF pg_trigger_depth() < 2 OR OLD.details_revision = 9223372036854775807 THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision is derived';
    END IF;
    IF NEW.details_revision <> OLD.details_revision + 1 THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision regression';
    END IF;
  END IF;
  RETURN NEW;
END $function$


-- crm_mobile_label_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_label_revision()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE p uuid;
BEGIN
 IF TG_TABLE_NAME='stage' THEN
   IF NEW.name IS NOT DISTINCT FROM OLD.name THEN RETURN NULL; END IF;
   FOR p IN SELECT id FROM public.person WHERE organization_id=NEW.organization_id AND stage_id=NEW.id ORDER BY id LOOP
     UPDATE public.person SET mobile_revision=mobile_revision+1 WHERE id=p;
   END LOOP;
 ELSE
   IF NEW.display_name IS NOT DISTINCT FROM OLD.display_name THEN RETURN NULL; END IF;
   FOR p IN SELECT id FROM public.person WHERE assigned_user_id=NEW.id
     UNION SELECT person_id FROM public.note WHERE author_user_id=NEW.id AND deleted_at IS NULL
     UNION SELECT person_id FROM public.task WHERE NEW.id IN (created_by_user_id,assignee_user_id,completed_by_user_id) AND deleted_at IS NULL
     ORDER BY 1 LOOP
     UPDATE public.person SET mobile_revision=mobile_revision+1 WHERE id=p;
   END LOOP;
 END IF;
 RETURN NULL;
END $function$


-- crm_mobile_metadata_catalog_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_metadata_catalog_revision(p_organization_id uuid)
 RETURNS bigint
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE value BIGINT;
BEGIN
  SELECT revision INTO value FROM public.mobile_metadata_catalog
    WHERE organization_id=p_organization_id FOR SHARE;
  RETURN value;
END $function$


-- crm_mobile_metadata_person_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_metadata_person_revision()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NEW.metadata_revision IS DISTINCT FROM OLD.metadata_revision THEN
    IF pg_trigger_depth() < 2 OR OLD.metadata_revision = 9223372036854775807
       OR NEW.metadata_revision <> OLD.metadata_revision + 1 THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='metadata revision is derived';
    END IF;
  END IF;
  RETURN NEW;
END $function$


-- crm_mobile_note_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_note_revision()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  -- Compare every persisted writer-visible field except the token itself.
  -- This covers ordinary edits, import repair/provenance writers and the
  -- tombstone path while leaving a byte-identical UPDATE at the same token.
  IF (to_jsonb(NEW) - 'revision') IS DISTINCT FROM (to_jsonb(OLD) - 'revision') THEN
    NEW.revision := OLD.revision + 1;
  ELSE
    NEW.revision := OLD.revision;
  END IF;
  IF NEW.revision < OLD.revision THEN
    RAISE EXCEPTION 'mobile note revision regression';
  END IF;
  RETURN NEW;
END $function$


-- crm_mobile_person_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_person_revision()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF ROW(NEW.first_name,NEW.last_name,NEW.stage_id,NEW.assigned_user_id,NEW.created_at)
 IS DISTINCT FROM ROW(OLD.first_name,OLD.last_name,OLD.stage_id,OLD.assigned_user_id,OLD.created_at) THEN
   NEW.mobile_revision := OLD.mobile_revision+1;
 END IF;
 IF NEW.mobile_revision < OLD.mobile_revision THEN RAISE EXCEPTION 'mobile revision regression'; END IF;
 RETURN NEW;
END $function$


-- crm_mobile_stage_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_stage_revision()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NEW.stage_id IS DISTINCT FROM OLD.stage_id THEN
    IF OLD.stage_revision = 9223372036854775807 THEN
      RAISE EXCEPTION USING ERRCODE = '22003', MESSAGE = 'stage revision overflow';
    END IF;
    NEW.stage_revision := OLD.stage_revision + 1;
  ELSE
    -- A client cannot reset, advance, or otherwise forge this derived value.
    NEW.stage_revision := OLD.stage_revision;
  END IF;
  RETURN NEW;
END $function$


-- crm_mobile_task_revision
CREATE OR REPLACE FUNCTION public.crm_mobile_task_revision()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF (to_jsonb(NEW)-'revision') IS DISTINCT FROM (to_jsonb(OLD)-'revision') THEN
   NEW.revision := OLD.revision + 1;
 ELSE NEW.revision := OLD.revision;
 END IF;
 RETURN NEW;
END $function$


-- crm_people_admission_contact_building
CREATE OR REPLACE FUNCTION public.crm_people_admission_contact_building()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM migration_people_admission_item i JOIN migration_people_admission_plan p ON p.id=i.plan_id AND p.admission_id=i.admission_id AND p.organization_id=i.organization_id WHERE i.id=NEW.item_id AND i.admission_id=NEW.admission_id AND i.organization_id=NEW.organization_id AND p.state='building') THEN
    RAISE EXCEPTION 'admission contact plan is sealed' USING ERRCODE='P0001';
  END IF;
  RETURN NEW;
END $function$


-- crm_people_admission_item_building
CREATE OR REPLACE FUNCTION public.crm_people_admission_item_building()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM migration_people_admission_plan p WHERE p.id=NEW.plan_id AND p.admission_id=NEW.admission_id AND p.organization_id=NEW.organization_id AND p.state='building') THEN
    RAISE EXCEPTION 'admission item plan is sealed' USING ERRCODE='P0001';
  END IF;
  RETURN NEW;
END $function$


-- crm_people_admission_item_immutable
CREATE OR REPLACE FUNCTION public.crm_people_admission_item_immutable()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF (to_jsonb(NEW) - ARRAY['disposition','settled_result_id','settled_at'])
       IS DISTINCT FROM (to_jsonb(OLD) - ARRAY['disposition','settled_result_id','settled_at']) THEN
    RAISE EXCEPTION 'admission item immutable' USING ERRCODE='P0001';
  END IF;
  RETURN NEW;
END $function$


-- crm_people_admission_lock_stage
CREATE OR REPLACE FUNCTION public.crm_people_admission_lock_stage(org uuid, target uuid)
 RETURNS boolean
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
BEGIN
  PERFORM 1 FROM stage WHERE id=target AND organization_id=org FOR SHARE;
  RETURN FOUND;
END $function$


-- crm_people_admission_mutation_allowed
CREATE OR REPLACE FUNCTION public.crm_people_admission_mutation_allowed(org uuid, permit text, table_name text, operation text, row_value jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
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
END $function$


-- crm_people_admission_plan_immutable
CREATE OR REPLACE FUNCTION public.crm_people_admission_plan_immutable()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_people_recovery_capability
CREATE OR REPLACE FUNCTION public.crm_people_recovery_capability(org uuid)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF EXISTS(SELECT 1 FROM migration_people_recovery_requirement WHERE organization_id=org)
    AND current_setting('crm.people_recovery_reader',true) IS DISTINCT FROM 'fub-people-recovery-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010R',MESSAGE='people_recovery_capability_required';
 END IF;
END $function$


-- crm_people_recovery_catalog_owned
CREATE OR REPLACE FUNCTION public.crm_people_recovery_catalog_owned()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM migration_people_admission a WHERE a.id=NEW.admission_id AND a.organization_id=NEW.organization_id
  AND a.mode='mapping_recovery' AND a.state='preparing' AND NOT a.recovery_catalog_complete AND NOT a.recovery_candidates_complete) THEN
  RAISE EXCEPTION 'recovery source catalog sealed' USING ERRCODE='P0001';
 END IF;
 RETURN NEW;
END $function$


-- crm_people_recovery_item_owned
CREATE OR REPLACE FUNCTION public.crm_people_recovery_item_owned()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_people_recovery_item_proof
CREATE OR REPLACE FUNCTION public.crm_people_recovery_item_proof(org uuid, unit uuid)
 RETURNS boolean
 LANGUAGE sql
 STABLE
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
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
$function$


-- crm_people_recovery_owned_write
CREATE OR REPLACE FUNCTION public.crm_people_recovery_owned_write()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_people_recovery_plan_immutable
CREATE OR REPLACE FUNCTION public.crm_people_recovery_plan_immutable()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF OLD.state<>'building' AND (OLD.recovery_candidate_count<>NEW.recovery_candidate_count OR OLD.recovery_unassigned_count<>NEW.recovery_unassigned_count) THEN
  RAISE EXCEPTION 'recovery preview counts immutable' USING ERRCODE='P0001';
 END IF;
 RETURN NEW;
END $function$


-- crm_people_recovery_previously_materialized
CREATE OR REPLACE FUNCTION public.crm_people_recovery_previously_materialized(org uuid, account bigint, source text)
 RETURNS boolean
 LANGUAGE sql
 STABLE
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
 SELECT EXISTS(SELECT 1 FROM migration_import_identity WHERE organization_id=org
   AND source_account_id=account AND family='people' AND source_id=source)
 OR EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import a
   ON a.id=r.import_id AND a.organization_id=r.organization_id
   WHERE r.organization_id=org AND a.source_account_id=account AND r.source_id=source
    AND r.disposition IN ('imported','already_imported'))
 OR EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_people_admission a
   ON a.id=r.admission_id AND a.organization_id=r.organization_id
   WHERE r.organization_id=org AND a.source_account_id=account AND r.source_id=source AND r.disposition='settled')
$function$


-- crm_people_recovery_refresh_approval
CREATE OR REPLACE FUNCTION public.crm_people_recovery_refresh_approval()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_people_recovery_result_owned
CREATE OR REPLACE FUNCTION public.crm_people_recovery_result_owned()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_people_recovery_write_fence
CREATE OR REPLACE FUNCTION public.crm_people_recovery_write_fence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_people_refresh_mutation_allowed
CREATE OR REPLACE FUNCTION public.crm_people_refresh_mutation_allowed(org uuid, permit text, table_name text, operation text, row_value jsonb)
 RETURNS boolean
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public', 'pg_temp'
AS $function$
DECLARE lease UUID; unit UUID; person UUID;
BEGIN
 IF NOT crm_mapping_repair_mutation_evidence(org,'migration_people_refresh',permit,table_name) THEN RETURN false; END IF;
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.person_id INTO person FROM migration_people_refresh_item i JOIN migration_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id JOIN migration_workspace w ON w.organization_id=r.organization_id AND w.import_id=r.parent_import_id AND w.plan_id=r.parent_plan_id JOIN organization_membership m ON m.organization_id=r.organization_id AND m.user_id=r.initiated_by_user_id WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND r.state='running' AND r.lease_token=lease AND r.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF person IS NULL THEN RETURN false; END IF;
 IF table_name='person' THEN RETURN operation='UPDATE' AND (row_value->>'id')::uuid=person; END IF;
 IF table_name='contact_method' THEN RETURN (row_value->>'person_id')::uuid=person AND EXISTS(SELECT 1 FROM migration_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.contact_id=(row_value->>'id')::uuid AND ((operation='INSERT' AND c.side='proposed') OR (operation='DELETE' AND c.side IN ('baseline','current')) OR (operation='UPDATE' AND c.side IN ('baseline','current','proposed')))); END IF;
 IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (row_value->>'person_id')::uuid=person AND row_value->>'origin'='migration' AND row_value->>'reason'='migration_refresh' AND (row_value->>'on_behalf_of_user_id')::uuid=(SELECT initiated_by_user_id FROM migration_people_refresh_item i JOIN migration_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id WHERE i.id=unit AND i.organization_id=org); END IF;
 RETURN false;
END $function$


-- crm_person_admitted_prepare_history
CREATE OR REPLACE FUNCTION public.crm_person_admitted_prepare_history()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 UPDATE migration_history_review_state SET counts=counts||jsonb_build_object('person_admitted',0)
 WHERE organization_id=NEW.organization_id AND person_id=NEW.person_id AND NOT counts ? 'person_admitted';
 RETURN NEW;
END $function$


-- crm_person_recovered_prepare_history
CREATE OR REPLACE FUNCTION public.crm_person_recovered_prepare_history()
 RETURNS trigger
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'public', 'pg_temp'
AS $function$
BEGIN
 UPDATE migration_history_review_state SET counts=counts||jsonb_build_object('person_recovered',0)
 WHERE organization_id=NEW.organization_id AND person_id=NEW.person_id AND NOT counts ? 'person_recovered';
 RETURN NEW;
END $function$


-- crm_workspace_mutation_guard
CREATE OR REPLACE FUNCTION public.crm_workspace_mutation_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE org UUID; row_value JSONB; token TEXT; terminal UUID; person UUID;
BEGIN
 IF current_user<>'crm_app' THEN RETURN COALESCE(NEW,OLD); END IF;
 row_value:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END; org:=(row_value->>'organization_id')::uuid;
 IF TG_TABLE_NAME='operator_task_proposal' THEN SELECT organization_id INTO org FROM operator_proposal WHERE id=(row_value->>'proposal_id')::uuid; END IF;
 IF org IS NULL THEN RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_scope_required'; END IF;
 PERFORM crm_workspace_shared(org); IF EXISTS(SELECT 1 FROM organization WHERE id=org AND workspace_mode='operational') THEN RETURN COALESCE(NEW,OLD); END IF;
 IF crm_family_refresh_mutation_allowed(org,TG_TABLE_NAME,TG_OP,CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END,CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END) THEN RETURN COALESCE(NEW,OLD); END IF; token:=current_setting('crm.admitted_metadata_permit',true);
 IF TG_OP='INSERT' AND crm_admitted_metadata_insert_allowed(org,token,row_value) THEN RETURN NEW; END IF;
 IF TG_OP='INSERT' AND (crm_metadata_insert_allowed(org,TG_TABLE_NAME,row_value) OR crm_activity_insert_allowed(org,TG_TABLE_NAME,row_value) OR crm_admitted_activity_insert_allowed(org,TG_TABLE_NAME,row_value)) THEN RETURN NEW; END IF;
 token:=current_setting('crm.import_token',true);
 IF token IS NOT NULL AND TG_TABLE_NAME IN ('person','contact_method','stage','assignment_changed','stage_changed','person_imported') AND (TG_TABLE_NAME<>'contact_method' OR TG_OP='INSERT') AND EXISTS(SELECT 1 FROM migration_workspace w JOIN migration_import i ON i.id=w.import_id AND i.organization_id=w.organization_id JOIN organization_membership m ON m.organization_id=i.organization_id AND m.user_id=i.executor_user_id WHERE w.organization_id=org AND w.plan_id=i.confirmed_plan_id AND i.state='running' AND i.lease_token::text=token AND i.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active') THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.people_refresh_permit',true); IF TG_TABLE_NAME IN ('person','contact_method','assignment_changed','stage_changed') AND crm_people_refresh_mutation_allowed(org,token,TG_TABLE_NAME,TG_OP,row_value) THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.people_admission_permit',true); IF TG_TABLE_NAME IN ('person','contact_method','assignment_changed','stage_changed','person_admitted','person_recovered') AND crm_people_admission_mutation_allowed(org,token,TG_TABLE_NAME,TG_OP,row_value) THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.admitted_people_refresh_permit',true); IF TG_TABLE_NAME IN ('person','contact_method','assignment_changed','stage_changed') AND crm_admitted_people_refresh_mutation_allowed(org,token,TG_TABLE_NAME,TG_OP,CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END,CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END) THEN RETURN COALESCE(NEW,OLD); END IF;
 token:=current_setting('crm.terminal_call',true); IF token IS NOT NULL AND token<>'' AND TG_TABLE_NAME IN ('call','call_completed','contact_attempted','person') THEN terminal:=token::uuid; SELECT person_id INTO person FROM call WHERE id=terminal AND organization_id=org; IF person IS NOT NULL AND ((TG_TABLE_NAME='call' AND (row_value->>'id')::uuid=terminal AND row_value->>'status' IN ('ended','failed','cancelled')) OR (TG_TABLE_NAME IN ('call_completed','contact_attempted') AND (row_value->>'person_id')::uuid=person) OR (TG_TABLE_NAME='person' AND TG_OP='UPDATE' AND (to_jsonb(OLD)-'updated_at'-'last_contact_at')=(to_jsonb(NEW)-'updated_at'-'last_contact_at'))) THEN RETURN COALESCE(NEW,OLD); END IF; END IF;
 RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
END $function$


-- crm_workspace_operational
CREATE OR REPLACE FUNCTION public.crm_workspace_operational(org uuid)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
 PERFORM crm_workspace_shared(org);
 IF NOT EXISTS(SELECT 1 FROM organization WHERE id=org AND workspace_mode='operational') THEN
  RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';
 END IF;
END $function$


-- crm_workspace_organization_guard
CREATE OR REPLACE FUNCTION public.crm_workspace_organization_guard()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- crm_workspace_read
CREATE OR REPLACE FUNCTION public.crm_workspace_read(org uuid, actor uuid, operational_only boolean)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
DECLARE mode TEXT;member_role TEXT;
BEGIN
 PERFORM crm_workspace_shared(org);
 SELECT o.workspace_mode,m.role INTO mode,member_role FROM organization o JOIN organization_membership m ON m.organization_id=o.id WHERE o.id=org AND m.user_id=actor AND m.status='active';
 IF mode IS NULL THEN RAISE EXCEPTION USING ERRCODE='P010A',MESSAGE='forbidden';END IF;
 IF mode<>'operational' AND (operational_only OR member_role<>'admin') THEN RAISE EXCEPTION USING ERRCODE='P010C',MESSAGE='workspace_in_migration_review';END IF;
 IF EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=org) AND current_setting('crm.history_reader',true) IS DISTINCT FROM 'fub-history-timeline-v1' THEN
  RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_reader_required';
 END IF;
END $function$


-- crm_workspace_shared
CREATE OR REPLACE FUNCTION public.crm_workspace_shared(org uuid)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF NOT pg_try_advisory_xact_lock_shared(hashtextextended('crm-workspace-v1:'||org::text,0)) THEN RAISE EXCEPTION USING ERRCODE='55P03',MESSAGE='workspace_busy'; END IF;
 IF current_user='crm_app' THEN
  IF EXISTS(SELECT 1 FROM migration_admitted_activity_import WHERE organization_id=org AND confirmed_plan_id IS NOT NULL) AND current_setting('crm.admitted_activity_reader',true) IS DISTINCT FROM 'fub-admitted-activity-v1' THEN RAISE EXCEPTION USING ERRCODE='P010F',MESSAGE='admitted_activity_handover_required'; END IF;
  IF EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE organization_id=org) AND current_setting('crm.history_reader',true) IS DISTINCT FROM 'fub-history-timeline-v1' THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='history_reader_required'; END IF;
  IF EXISTS(SELECT 1 FROM migration_admitted_history_root WHERE organization_id=org AND confirmed_plan_id IS NOT NULL) AND current_setting('crm.admitted_history_reader',true) IS DISTINCT FROM 'fub-admitted-history-v1' THEN RAISE EXCEPTION USING ERRCODE='P010H',MESSAGE='admitted_history_reader_required'; END IF;
  PERFORM crm_mapping_repair_capability(org); PERFORM crm_people_recovery_capability(org); PERFORM crm_family_refresh_capability(org);
 END IF;
END $function$


-- migration_core_change_bytes
CREATE OR REPLACE FUNCTION public.migration_core_change_bytes(v jsonb, table_name text)
 RETURNS bigint
 LANGUAGE plpgsql
 IMMUTABLE
AS $function$
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
END $function$


-- migration_core_change_charge
CREATE OR REPLACE FUNCTION public.migration_core_change_charge()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
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
END $function$


-- migration_core_change_immutable
CREATE OR REPLACE FUNCTION public.migration_core_change_immutable()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
 IF TG_TABLE_NAME='migration_core_change_report' THEN
   IF ROW(NEW.organization_id,NEW.parent_import_id,NEW.parent_plan_id,NEW.baseline_snapshot_id,NEW.newer_snapshot_id,NEW.baseline_sequence,NEW.newer_sequence,NEW.source_account_id,NEW.workspace_revision,NEW.initiated_by_user_id,NEW.engine_version,NEW.tuple_hmac,NEW.inputs_nonce,NEW.inputs_ciphertext) IS DISTINCT FROM ROW(OLD.organization_id,OLD.parent_import_id,OLD.parent_plan_id,OLD.baseline_snapshot_id,OLD.newer_snapshot_id,OLD.baseline_sequence,OLD.newer_sequence,OLD.source_account_id,OLD.workspace_revision,OLD.initiated_by_user_id,OLD.engine_version,OLD.tuple_hmac,OLD.inputs_nonce,OLD.inputs_ciphertext) THEN RAISE EXCEPTION 'immutable report inputs' USING ERRCODE='23514'; END IF;
   IF OLD.state IN ('completed','cancelled') AND ROW(NEW.state,NEW.phase,NEW.summary_nonce,NEW.summary_ciphertext,NEW.output_revision,NEW.capture_side,NEW.capture_checkpoint,NEW.group_checkpoint,NEW.captures_processed,NEW.observations_processed,NEW.groups_compared) IS DISTINCT FROM ROW(OLD.state,OLD.phase,OLD.summary_nonce,OLD.summary_ciphertext,OLD.output_revision,OLD.capture_side,OLD.capture_checkpoint,OLD.group_checkpoint,OLD.captures_processed,OLD.observations_processed,OLD.groups_compared) THEN RAISE EXCEPTION 'terminal report' USING ERRCODE='23514'; END IF;
 ELSE
   IF EXISTS(SELECT 1 FROM migration_core_change_report WHERE id=NEW.report_id AND organization_id=NEW.organization_id AND state IN ('completed','cancelled')) THEN RAISE EXCEPTION 'sealed report rows' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $function$


-- person_touch_correspondence
CREATE OR REPLACE FUNCTION public.person_touch_correspondence()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NEW.direction = 'inbound' THEN
    UPDATE person SET last_inbound_at = NEW.occurred_at
     WHERE id = NEW.person_id AND organization_id = NEW.organization_id
       AND (last_inbound_at IS NULL OR last_inbound_at < NEW.occurred_at);
  ELSIF NEW.direction = 'outbound' THEN
    UPDATE person SET last_outbound_at = NEW.occurred_at
     WHERE id = NEW.person_id AND organization_id = NEW.organization_id
       AND (last_outbound_at IS NULL OR last_outbound_at < NEW.occurred_at);
  END IF;
  RETURN NULL;
END $function$


-- person_touch_last_contact
CREATE OR REPLACE FUNCTION public.person_touch_last_contact()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  UPDATE person SET last_contact_at = NEW.occurred_at
   WHERE id = NEW.person_id AND organization_id = NEW.organization_id
     AND (last_contact_at IS NULL OR last_contact_at < NEW.occurred_at);
  RETURN NULL;
END $function$


-- person_touch_last_inquiry
CREATE OR REPLACE FUNCTION public.person_touch_last_inquiry()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  UPDATE person SET last_inquiry_at = NEW.received_at
   WHERE id = NEW.person_id AND organization_id = NEW.organization_id
     AND (last_inquiry_at IS NULL OR last_inquiry_at < NEW.received_at);
  RETURN NULL;
END $function$


-- reject_direct_mutation
CREATE OR REPLACE FUNCTION public.reject_direct_mutation()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
    IF TG_OP = 'DELETE' AND pg_trigger_depth() > 1
        AND NOT EXISTS (
            SELECT 1 FROM person
             WHERE id = OLD.person_id AND organization_id = OLD.organization_id
        )
    THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'inquiry is append-only';
END;
$function$


-- reject_mutation
CREATE OR REPLACE FUNCTION public.reject_mutation()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
    RAISE EXCEPTION 'history table % is append-only', TG_TABLE_NAME;
END;
$function$
