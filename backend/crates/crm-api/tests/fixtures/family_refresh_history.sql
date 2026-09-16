-- Migrator-only constructed refresh evidence over real synthetic first imports
-- and captures. Exercises storage invariants, not the unfinished source worker.
DO $$
DECLARE org UUID:=current_setting('test.family_org')::uuid; actor UUID:=current_setting('test.family_actor')::uuid;
 parent UUID:=current_setting('test.family_parent')::uuid; capture UUID; round INTEGER:=0;
 bundle UUID:=gen_random_uuid(); plan UUID:=gen_random_uuid(); lease UUID:=gen_random_uuid(); control UUID:=gen_random_uuid();
 c RECORD; f RECORD; raw RECORD; h RECORD; page UUID; source UUID; manifest UUID; outcome UUID; display UUID; cohort UUID; position BIGINT:=0;
 rejected BOOLEAN; frozen TEXT; after_frozen TEXT; before_counts JSONB; after_counts JSONB; before_bytes BIGINT; after_bytes BIGINT; charged BIGINT;
BEGIN
 FOREACH capture IN ARRAY ARRAY[current_setting('test.family_capture')::uuid,current_setting('test.family_successor_capture')::uuid] LOOP
 round:=round+1; bundle:=gen_random_uuid(); plan:=gen_random_uuid(); lease:=gen_random_uuid(); control:=gen_random_uuid(); position:=0;
 PERFORM set_config('crm.family_refresh_reader','fub-family-refresh-v1',true);
 PERFORM set_config('crm.history_reader','fub-history-timeline-v1',true);
 PERFORM set_config('crm.admitted_history_reader','fub-admitted-history-v1',true);
 PERFORM set_config('crm.family_refresh_lease',lease::text,true);
 SELECT * INTO c FROM migration_history_capture_run WHERE id=capture AND organization_id=org;
 SELECT md5(jsonb_agg(to_jsonb(i) ORDER BY i.id)::text) INTO frozen FROM migration_history_import_identity i WHERE organization_id=org;
 INSERT INTO migration_family_refresh_bundle(id,organization_id,parent_import_id,parent_plan_id,source_account_id,executor_user_id,engine_version,history_capture_id,state,source_nonce,source_ciphertext)
 VALUES(bundle,org,parent,c.parent_plan_id,c.source_account_id,actor,'fub-family-refresh-v1',capture,'preparing',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'));
 INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,history_capture_id,nonce,ciphertext)
 VALUES(plan,bundle,org,'history',1,'preparing','cohort',capture,decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'));
 UPDATE migration_family_refresh_bundle SET payer_plan_id=plan WHERE id=bundle;
 INSERT INTO migration_family_refresh_cohort(id,bundle_id,organization_id,source_person_id,person_id,original_result_id,creation_snapshot_id)
 SELECT gen_random_uuid(),bundle,org,r.source_id,r.person_id,r.id,i.snapshot_id FROM migration_import_result r JOIN migration_import i ON i.id=r.import_id AND i.organization_id=r.organization_id
 WHERE r.import_id=parent AND r.organization_id=org AND r.disposition='imported';
 FOR f IN SELECT 'event' AS kind,'events' AS family,x.*,p.capture_id AS original_capture,i.identity_hmac,i.semantic_hmac FROM fub_event_record_imported x JOIN migration_history_import_plan p ON p.id=x.plan_id JOIN migration_history_import_identity i ON i.id=x.identity_id WHERE x.organization_id=org
  UNION ALL SELECT 'call','calls',x.*,p.capture_id,i.identity_hmac,i.semantic_hmac FROM fub_call_record_imported x JOIN migration_history_import_plan p ON p.id=x.plan_id JOIN migration_history_import_identity i ON i.id=x.identity_id WHERE x.organization_id=org
  UNION ALL SELECT 'text','text_messages',x.*,p.capture_id,i.identity_hmac,i.semantic_hmac FROM fub_text_record_imported x JOIN migration_history_import_plan p ON p.id=x.plan_id JOIN migration_history_import_identity i ON i.id=x.identity_id WHERE x.organization_id=org LOOP
  IF round=1 THEN
  INSERT INTO migration_family_refresh_history_head(organization_id,identity_id,person_id,family,original_fact_id,plan_id,bundle_id,capture_id,semantic_hmac,source_created_at)
  VALUES(org,f.identity_id,f.person_id,f.family,f.id,plan,bundle,f.original_capture,f.semantic_hmac,f.source_created_at);
  END IF;
  rejected:=false;
  BEGIN
   UPDATE migration_family_refresh_history_head SET source_created_at=NULL WHERE organization_id=org AND identity_id=f.identity_id;
  EXCEPTION WHEN OTHERS THEN rejected:=true; END;
  IF NOT rejected THEN RAISE EXCEPTION 'head allowed metadata rewrite without version'; END IF;
  SELECT * INTO raw FROM migration_history_capture WHERE run_id=capture AND organization_id=org AND family=f.family AND classification='advancing' LIMIT 1;
  SELECT id INTO cohort FROM migration_family_refresh_cohort WHERE bundle_id=bundle AND organization_id=org AND person_id=f.person_id;
  position:=position+1; source:=gen_random_uuid(); manifest:=gen_random_uuid();
  page:=gen_random_uuid();
  INSERT INTO migration_family_refresh_history_page(id,bundle_id,plan_id,organization_id,run_id,capture_id,capture_sequence,checkpoint,stream,accepted,nonce,ciphertext)
  VALUES(page,bundle,plan,org,capture,raw.id,raw.sequence,raw.checkpoint,raw.family,true,decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'));
  INSERT INTO migration_family_refresh_source(id,bundle_id,plan_id,organization_id,history_page_id,capture_id,capture_sequence,ordinal,representation,kind,source_id,source_person_id,identity_hmac,semantic_hmac,qualified,nonce,ciphertext)
  SELECT source,bundle,plan,org,page,raw.id,raw.sequence,0,raw.representation,f.kind,'1',co.source_person_id,f.identity_hmac,sha256(convert_to(f.kind||round::text,'UTF8')),true,decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex') FROM migration_family_refresh_cohort co WHERE co.id=cohort;
  INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,source_row_id,position,kind,source_key_hmac,person_id,target_id,expected_head_id,disposition,counts,nonce,ciphertext,added_byte_bound)
  VALUES(manifest,bundle,plan,org,cohort,source,position,f.kind,f.identity_hmac,f.person_id,f.id,(SELECT result_id FROM migration_family_refresh_history_head WHERE organization_id=org AND identity_id=f.identity_id),'correction','{}',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'),8192);
 END LOOP;
 IF position<>3 THEN RAISE EXCEPTION 'fixture needs all three typed facts'; END IF;
 IF round=1 THEN
  INSERT INTO migration_family_refresh_requirement(organization_id,capability,bundle_id) VALUES(org,'fub-family-refresh-v1',bundle);
 END IF;
 UPDATE migration_family_refresh_bundle SET state='running',confirmed_at=clock_timestamp(),digest=decode(repeat('00',32),'hex') WHERE id=bundle;
 UPDATE migration_family_refresh_plan SET state='running',confirmed_at=clock_timestamp(),digest=decode(repeat('00',32),'hex'),phase='apply',lease_token=lease,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=plan;
 IF NOT crm_family_refresh_reserve(org,bundle,plan,control,1,65536,'control',2147483648,4294967296) THEN RAISE EXCEPTION 'fixture capacity'; END IF;
 PERFORM crm_family_refresh_settle(org,bundle,plan,control,1,false);
 SELECT jsonb_object_agg(person_id,counts) INTO before_counts FROM migration_history_review_state WHERE organization_id=org;
 FOR f IN SELECT m.*,s.semantic_hmac FROM migration_family_refresh_manifest m JOIN migration_family_refresh_source s ON s.id=m.source_row_id WHERE m.plan_id=plan ORDER BY m.position LOOP
  SELECT * INTO h FROM migration_family_refresh_history_head WHERE organization_id=org AND original_fact_id=f.target_id;
  outcome:=gen_random_uuid(); display:=gen_random_uuid();
  INSERT INTO migration_family_refresh_result(id,bundle_id,plan_id,organization_id,manifest_id,disposition,person_id,target_id,nonce,ciphertext)
  VALUES(outcome,bundle,plan,org,f.id,'applied',f.person_id,f.target_id,decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'));
  INSERT INTO migration_family_refresh_history_display(id,organization_id,identity_id,plan_id,bundle_id,result_id,nonce,ciphertext)
  VALUES(display,org,h.identity_id,plan,bundle,outcome,decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'));
  -- Fault before the fact must not advance a head or counts.
  rejected:=false;
  BEGIN
   EXECUTE format('INSERT INTO fub_%s_record_corrected(id,organization_id,identity_id,original_fact_id,person_id,actor_kind,origin,occurred_at,correlation_id,version,corrects_id,plan_id,bundle_id,result_id,manifest_id,capture_id,semantic_hmac,source_time_basis)
    VALUES($1,$2,$3,$4,$5,''system'',''migration'',clock_timestamp(),gen_random_uuid(),$12,$13,$6,$7,$8,$9,$10,$11,''unknown'')',f.kind)
    USING display,org,h.identity_id,f.target_id,gen_random_uuid(),plan,bundle,outcome,f.id,capture,f.semantic_hmac,h.version+1,h.version_id;
  EXCEPTION WHEN OTHERS THEN rejected:=true; END;
  IF NOT rejected THEN RAISE EXCEPTION 'foreign Person accepted'; END IF;
  IF (SELECT version FROM migration_family_refresh_history_head WHERE organization_id=org AND identity_id=h.identity_id)<>h.version THEN RAISE EXCEPTION 'failed write advanced head'; END IF;
  EXECUTE format('INSERT INTO fub_%s_record_corrected(id,organization_id,identity_id,original_fact_id,person_id,actor_kind,origin,occurred_at,correlation_id,version,corrects_id,plan_id,bundle_id,result_id,manifest_id,capture_id,semantic_hmac,source_created_at,source_time_basis)
   VALUES($1,$2,$3,$4,$5,''system'',''migration'',clock_timestamp(),gen_random_uuid(),$14,$15,$6,$7,$8,$9,$10,$11,$12,$13)',f.kind)
   USING display,org,h.identity_id,f.target_id,f.person_id,plan,bundle,outcome,f.id,capture,f.semantic_hmac,
    CASE WHEN f.kind='event' THEN NULL::timestamptz ELSE '2026-01-04T00:00:00Z'::timestamptz END,
    CASE WHEN f.kind='event' THEN 'unknown' ELSE 'fub_record_created' END,h.version+1,h.version_id;
  rejected:=false;
  BEGIN
   EXECUTE format('UPDATE fub_%s_record_corrected SET source_created_at=clock_timestamp() WHERE id=$1',f.kind) USING display;
  EXCEPTION WHEN OTHERS THEN rejected:=true; END;
  IF NOT rejected THEN RAISE EXCEPTION 'immutable correction updated'; END IF;
  rejected:=false;
  BEGIN
   UPDATE migration_family_refresh_history_head SET version=h.version+2 WHERE organization_id=org AND identity_id=h.identity_id;
  EXCEPTION WHEN OTHERS THEN rejected:=true; END;
  IF NOT rejected THEN RAISE EXCEPTION 'head advanced without successor'; END IF;
 END LOOP;
 PERFORM crm_family_refresh_settle(org,bundle,plan,control,1,false);
 SELECT md5(jsonb_agg(to_jsonb(i) ORDER BY i.id)::text) INTO after_frozen FROM migration_history_import_identity i WHERE organization_id=org;
 IF frozen<>after_frozen THEN RAISE EXCEPTION 'first owner or original semantic changed'; END IF;
 IF (SELECT count(*) FROM migration_family_refresh_history_head WHERE organization_id=org AND version=round+1)<>3 THEN RAISE EXCEPTION 'typed heads not advanced'; END IF;
 SELECT jsonb_object_agg(person_id,counts) INTO after_counts FROM migration_history_review_state WHERE organization_id=org;
 FOR h IN SELECT * FROM migration_family_refresh_history_head WHERE organization_id=org LOOP
  IF h.family='events' AND ((after_counts->h.person_id::text->>'fub_event_record_imported_known')::bigint<>0 OR (after_counts->h.person_id::text->>'fub_event_record_imported_unknown')::bigint<>1) THEN RAISE EXCEPTION 'event count not moved'; END IF;
  IF h.family='text_messages' AND ((after_counts->h.person_id::text->>'fub_text_record_imported_known')::bigint<>1 OR (after_counts->h.person_id::text->>'fub_text_record_imported_unknown')::bigint<>0) THEN RAISE EXCEPTION 'text count not moved'; END IF;
  IF before_counts->h.person_id::text->'contact_attempted' IS DISTINCT FROM after_counts->h.person_id::text->'contact_attempted' OR before_counts->h.person_id::text->'call_completed' IS DISTINCT FROM after_counts->h.person_id::text->'call_completed' THEN RAISE EXCEPTION 'correction created native contact credit'; END IF;
 END LOOP;
 UPDATE migration_family_refresh_plan SET state='completed',lease_token=NULL,lease_expires_at=NULL WHERE id=plan;
 UPDATE migration_family_refresh_bundle SET state='completed' WHERE id=bundle;
 PERFORM crm_family_refresh_settle(org,bundle,plan,control,1,true);
 END LOOP;
 SELECT retained_bytes INTO before_bytes FROM migration_snapshot_storage WHERE organization_id=org;
 UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=org AND family='events' AND erased_at IS NULL;
 SELECT retained_bytes INTO after_bytes FROM migration_snapshot_storage WHERE organization_id=org;
 IF before_bytes-after_bytes<>80 THEN RAISE EXCEPTION 'erasure did not refund exact display bytes: %',before_bytes-after_bytes; END IF;
 IF EXISTS(SELECT 1 FROM migration_family_refresh_history_display d JOIN migration_history_import_identity i ON i.id=d.identity_id AND i.organization_id=d.organization_id WHERE d.organization_id=org AND i.family='events' AND d.nonce IS NOT NULL) THEN RAISE EXCEPTION 'erasure retained display'; END IF;
 IF EXISTS(SELECT 1 FROM migration_history_review_state WHERE organization_id=org AND ((counts->>'fub_event_record_imported_known')::bigint<>0 OR (counts->>'fub_event_record_imported_unknown')::bigint<>0)) THEN RAISE EXCEPTION 'erasure decremented initial date bucket'; END IF;
 rejected:=false;
 BEGIN
  UPDATE migration_family_refresh_history_display SET nonce=decode(repeat('00',24),'hex'),ciphertext=decode(repeat('00',16),'hex') WHERE organization_id=org AND nonce IS NULL;
 EXCEPTION WHEN OTHERS THEN rejected:=true; END;
 IF NOT rejected THEN RAISE EXCEPTION 'erased display restored'; END IF;
 -- Deleting an initial display also suppresses all successors and decrements
 -- the current known bucket of an initially unknown text.
 DELETE FROM migration_history_import_display d USING fub_text_record_imported fact WHERE d.id=fact.manifest_id AND d.organization_id=fact.organization_id AND d.organization_id=org;
 IF EXISTS(SELECT 1 FROM migration_history_review_state WHERE organization_id=org AND ((counts->>'fub_text_record_imported_known')::bigint<>0 OR (counts->>'fub_text_record_imported_unknown')::bigint<>0)) THEN RAISE EXCEPTION 'suppression counts wrong'; END IF;
 IF EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE organization_id=org AND measured_bytes<>retained_bytes) THEN RAISE EXCEPTION 'erasure left unsettled evidence'; END IF;
 IF has_table_privilege('crm_app','fub_event_record_corrected','INSERT') OR has_table_privilege('crm_app','migration_family_refresh_history_head','UPDATE') THEN RAISE EXCEPTION 'unfinished execution exposed'; END IF;
END $$;
