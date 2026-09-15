-- 010e5: one shared runtime and preflight compatibility inventory.
SELECT COALESCE(
 (SELECT bool_and(EXISTS(SELECT 1 FROM pg_proc p WHERE p.oid=to_regprocedure('public.'||signature) AND md5(p.prosrc)=body_md5)) FROM (VALUES
 ('crm_mapping_repair_identity(text,jsonb,jsonb)','30b642d32b5c77f08e06e82647f897ec'),
 ('crm_mapping_repair_mutation_evidence(uuid,text,text,text)','f66cf2440267a6fa8a4ee3f853b66cbc'),
 ('crm_mapping_repair_native_fingerprint(uuid,uuid)','7382febeaf9c47a243131080f2b3e8ed'),
 ('crm_mapping_repair_target(uuid,text,uuid)','f8c735c3b2292ef5238256cae121cf49'),
 ('crm_mapping_repair_owned_write()','04b7b1e067d213e9b121e650c2fe84dc'),
 ('crm_mapping_repair_head_write()','cd7471fa3e11f9ca4ed9a315492e7d5a'),
 ('crm_mapping_repair_capability(uuid)','2304c820dcef6dbca2a656adc1c05b05'),
 ('crm_workspace_shared(uuid)','c35c0e9aa618dcf87998215725a1b98a'),
 ('crm_mapping_repair_write_fence()','653877ec38a79e5dbac2cef61c8d80c4'),
 ('crm_people_refresh_mutation_allowed(uuid,text,text,text,jsonb)','66b6271e33f3320844097953a59d4328'),
 ('crm_admitted_people_refresh_mutation_allowed(uuid,text,text,text,jsonb,jsonb)','0195d2a8bf721ea25939bffcf71724b6')) expected(signature,body_md5))
 AND to_regclass('public.migration_mapping_repair_requirement') IS NOT NULL
 AND (SELECT bool_and(to_regclass('public.'||p||s) IS NOT NULL) FROM
 (VALUES ('migration_people_refresh'),('migration_admitted_people_refresh')) roots(p)
 CROSS JOIN (VALUES ('_repair_key'),('_repair_choice'),('_repair_candidate'),('_mapping_binding'),('_mapping_head')) stores(s))
 AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass('public.'||p||s) AND t.tgfoid=to_regprocedure('public.crm_mapping_repair_write_fence()') AND t.tgenabled IN ('O','A') AND t.tgtype=31)) FROM
 (VALUES ('migration_people_refresh'),('migration_admitted_people_refresh')) roots(p)
 CROSS JOIN (VALUES (''),('_plan'),('_item'),('_result'),('_baseline'),('_repair_key'),('_repair_choice'),('_repair_candidate'),('_mapping_binding'),('_mapping_head')) stores(s))
 AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass('public.'||p||s) AND t.tgfoid=to_regprocedure('public.crm_mapping_repair_owned_write()') AND t.tgenabled IN ('O','A') AND t.tgtype=23)) FROM
 (VALUES ('migration_people_refresh'),('migration_admitted_people_refresh')) roots(p)
 CROSS JOIN (VALUES (''),('_plan'),('_item'),('_result'),('_baseline'),('_repair_choice'),('_repair_candidate'),('_mapping_binding')) stores(s))
 AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass('public.'||p||'_mapping_head') AND t.tgfoid=to_regprocedure('public.crm_mapping_repair_head_write()') AND t.tgenabled IN ('O','A') AND t.tgtype=23)) FROM (VALUES ('migration_people_refresh'),('migration_admitted_people_refresh')) roots(p))
 AND NOT EXISTS(SELECT 1 FROM pg_constraint c WHERE c.conrelid IN (SELECT to_regclass('public.'||p||s) FROM (VALUES ('migration_people_refresh'),('migration_admitted_people_refresh')) roots(p) CROSS JOIN (VALUES (''),('_item'),('_repair_choice'),('_repair_candidate'),('_mapping_binding'),('_mapping_head')) stores(s)) AND NOT c.convalidated)
 AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=to_regclass('public.'||p||'_item') AND a.attname=n AND NOT a.attisdropped)) FROM
 (VALUES ('migration_people_refresh'),('migration_admitted_people_refresh')) roots(p)
 CROSS JOIN (VALUES ('mapping_evidence_nonce'),('native_fingerprint'),('repair_stage_choice_id'),('repair_assignee_choice_id'),('baseline_version')) columns(n))

 AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass('public.'||p||s) AND t.tgfoid=to_regprocedure('public.reject_mutation()') AND t.tgenabled IN ('O','A') AND t.tgtype=27)) FROM
 (VALUES ('migration_people_refresh'),('migration_admitted_people_refresh')) roots(p)
 CROSS JOIN (VALUES ('_repair_key'),('_repair_choice'),('_repair_candidate'),('_mapping_binding')) stores(s))
 AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_constraint c WHERE c.contype='f' AND c.convalidated
  AND c.conrelid=to_regclass('public.'||p||s) AND c.confrelid=to_regclass('public.'||p||target)
  AND (SELECT array_agg(a.attname::text ORDER BY k.ord) FROM unnest(c.conkey) WITH ORDINALITY k(n,ord) JOIN pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.n)=columns
  AND (SELECT array_agg(a.attname::text ORDER BY k.ord) FROM unnest(c.confkey) WITH ORDINALITY k(n,ord) JOIN pg_attribute a ON a.attrelid=c.confrelid AND a.attnum=k.n)=target_columns)) FROM
 (VALUES ('migration_people_refresh'),('migration_admitted_people_refresh')) roots(p)
 CROSS JOIN (VALUES
 ('_item',ARRAY['settled_result_id','refresh_id','id','organization_id'],'_result',ARRAY['id','refresh_id','item_id','organization_id']),
 ('_repair_candidate',ARRAY['anchor_item_id','anchor_refresh_id','organization_id'],'_item',ARRAY['id','refresh_id','organization_id']),
 ('_repair_candidate',ARRAY['anchor_plan_id','anchor_refresh_id','organization_id'],'_plan',ARRAY['id','refresh_id','organization_id']),
 ('_repair_choice',ARRAY['inherited_choice_id','inherited_refresh_id','organization_id'],'_repair_choice',ARRAY['id','refresh_id','organization_id']),
 ('_mapping_binding',ARRAY['result_id','refresh_id','item_id','organization_id'],'_result',ARRAY['id','refresh_id','item_id','organization_id']),
 ('_mapping_binding',ARRAY['choice_id','choice_refresh_id','organization_id'],'_repair_choice',ARRAY['id','refresh_id','organization_id']),
 ('_mapping_head',ARRAY['binding_id','organization_id','person_id','source_account_id','kind','source_key_hmac'],'_mapping_binding',ARRAY['id','organization_id','person_id','source_account_id','kind','source_key_hmac'])
 ) required(s,columns,target,target_columns))
 AND EXISTS(SELECT 1 FROM pg_proc WHERE oid=to_regprocedure('public.crm_mapping_repair_target(uuid,text,uuid)') AND prosecdef AND proconfig @> ARRAY['search_path=pg_catalog, public, pg_temp'])
 ,false)
