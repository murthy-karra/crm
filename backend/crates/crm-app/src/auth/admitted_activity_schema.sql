-- One shared readiness inventory for Rust startup/confirmation and release preflight.
-- Function body hashes pin the definitions in 20261004000001/003. They detect
-- incomplete replacement as well as missing functions; this is not a secret hash.
SELECT COALESCE(
  EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_activity_source') AND attname='source_person_id' AND atttypid='text'::regtype AND NOT attisdropped)
  AND EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass('public.migration_admitted_activity_source') AND conname='migration_admitted_activity_source_family_check' AND convalidated AND pg_get_constraintdef(oid) LIKE '%people%')
  AND EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass('public.migration_admitted_activity_manifest') AND conname='migration_admitted_activity_manifest_disposition_check' AND convalidated AND pg_get_constraintdef(oid) LIKE '%excluded%')
  AND
  (SELECT bool_and(to_regclass('public.'||name) IS NOT NULL)
   FROM (VALUES ('migration_admitted_activity_import'),
    ('migration_admitted_activity_plan'),
    ('migration_admitted_activity_choice'),
    ('migration_admitted_activity_source'),
    ('migration_admitted_activity_mapping'),
    ('migration_admitted_activity_manifest'),
    ('migration_admitted_activity_manifest_issue'),
    ('migration_admitted_activity_result'),
    ('migration_admitted_activity_result_issue'),
    ('migration_admitted_activity_receipt'),
    ('migration_admitted_activity_reservation'),
    ('migration_admitted_activity_issue')) required(name))
  AND (SELECT bool_and(has_table_privilege('crm_app',to_regclass('public.'||name),privilege))
   FROM (VALUES ('migration_admitted_activity_import'),
    ('migration_admitted_activity_plan'),
    ('migration_admitted_activity_choice'),
    ('migration_admitted_activity_source'),
    ('migration_admitted_activity_mapping'),
    ('migration_admitted_activity_manifest'),
    ('migration_admitted_activity_manifest_issue'),
    ('migration_admitted_activity_result'),
    ('migration_admitted_activity_result_issue'),
    ('migration_admitted_activity_receipt'),
    ('migration_admitted_activity_reservation'),
    ('migration_admitted_activity_issue')) required(name)
   CROSS JOIN (VALUES ('SELECT'),('INSERT')) access(privilege))
  AND (SELECT bool_and(has_table_privilege('crm_app',to_regclass('public.'||name),'UPDATE'))
   FROM (VALUES ('migration_admitted_activity_import'),
    ('migration_admitted_activity_plan'),
    ('migration_admitted_activity_mapping'),
    ('migration_admitted_activity_issue')) required(name))
  AND has_table_privilege('crm_app',to_regclass('public.migration_admitted_activity_reservation'),'DELETE')
  AND has_column_privilege('crm_app',to_regclass('public.migration_admitted_activity_reservation'),'byte_count','UPDATE')
  AND (SELECT bool_and(NOT has_table_privilege('crm_app',to_regclass('public.'||name),privilege))
   FROM (VALUES ('migration_admitted_activity_choice'),
    ('migration_admitted_activity_source'),
    ('migration_admitted_activity_manifest'),
    ('migration_admitted_activity_manifest_issue'),
    ('migration_admitted_activity_result'),
    ('migration_admitted_activity_result_issue'),
    ('migration_admitted_activity_receipt'),
    ('migration_activity_identity')) immutable(name)
   CROSS JOIN (VALUES ('UPDATE'),('DELETE'),('TRUNCATE')) forbidden(privilege))
  AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_proc p WHERE p.oid=to_regprocedure('public.'||signature)
      AND md5(p.prosrc)=body_md5 AND NOT p.prosecdef
      AND has_function_privilege('crm_app',p.oid,'EXECUTE')
      AND NOT EXISTS(SELECT 1 FROM aclexplode(COALESCE(p.proacl,acldefault('f',p.proowner))) a WHERE a.grantee=0 AND a.privilege_type='EXECUTE')))
    FROM (VALUES ('crm_admitted_activity_retained_size(text,jsonb)','7b2209180deaa25ae2bfe13c28b46a8e'),
    ('crm_admitted_activity_measure_row()','f8cf014971be4c64058a1260467d9ddf'),
    ('crm_activity_measure_row()','2031135b8362b8bb918114f24d60be23'),
    ('crm_activity_complete_read(uuid)','0f665c63735a5ed020023edaa19a5ffe'),
    ('crm_admitted_activity_insert_allowed(uuid,text,jsonb)','4201639b79f89301e915cb6efeb584cf'),
    ('crm_workspace_mutation_guard()','c46e8f6b68fc6eadde2ee20a1b7c6507'),
    ('crm_workspace_shared(uuid)','2923ac107ebd11ea2e1c82c1a42bb32f')) expected(signature,body_md5))
  AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass('public.'||name)
      AND t.tgname=name||'_measure' AND t.tgenabled IN ('O','A') AND t.tgtype=29
      AND t.tgfoid=to_regprocedure('public.crm_admitted_activity_measure_row()')))
    FROM (VALUES ('migration_admitted_activity_import'),
    ('migration_admitted_activity_plan'),
    ('migration_admitted_activity_choice'),
    ('migration_admitted_activity_source'),
    ('migration_admitted_activity_mapping'),
    ('migration_admitted_activity_manifest'),
    ('migration_admitted_activity_manifest_issue'),
    ('migration_admitted_activity_result'),
    ('migration_admitted_activity_result_issue'),
    ('migration_admitted_activity_receipt'),
    ('migration_admitted_activity_issue')) measured(name))
  AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass('public.'||name)
      AND t.tgenabled IN ('O','A') AND t.tgtype=31
      AND t.tgfoid=to_regprocedure('public.crm_workspace_mutation_guard()')))
    FROM (VALUES ('note'),('task')) native(name))
  AND EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass('public.migration_activity_identity')
      AND tgenabled IN ('O','A') AND tgfoid=to_regprocedure('public.crm_activity_measure_row()'))
  AND EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass('public.migration_admitted_activity_receipt')
      AND tgname='migration_admitted_activity_receipt_append_only' AND tgenabled IN ('O','A')
      AND tgfoid=to_regprocedure('public.reject_mutation()'))
  AND EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass('public.migration_activity_identity')
      AND conname='migration_activity_identity_exclusive_owner' AND contype='c' AND convalidated
      AND pg_get_constraintdef(oid) LIKE '%admitted_import_id IS NOT NULL%'
      AND pg_get_constraintdef(oid) LIKE '%import_id IS NULL%'
      AND pg_get_constraintdef(oid) LIKE '%admitted_manifest_id IS NULL%')
  AND EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass('public.migration_activity_identity')
      AND conname='migration_activity_identity_admitted_owner_fk' AND contype='f' AND convalidated
      AND confrelid=to_regclass('public.migration_admitted_activity_manifest')
      AND array_length(conkey,1)=4 AND array_length(confkey,1)=4)
  AND (SELECT count(*)=3 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_activity_identity')
      AND attname IN ('admitted_import_id','admitted_plan_id','admitted_manifest_id')
      AND atttypid='uuid'::regtype AND NOT attisdropped)
  AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_index i WHERE i.indexrelid=to_regclass('public.'||name) AND i.indisvalid AND i.indisready))
    FROM (VALUES ('migration_admitted_activity_one_root'),('migration_admitted_activity_import_claim'),
      ('migration_admitted_activity_result_page'),('migration_admitted_activity_manifest_issue_page'),
      ('migration_admitted_activity_result_issue_page')) expected(name))
  AND (SELECT count(*)=2 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_activity_import')
      AND attname IN ('remainder_source_import_id','remainder_source_plan_id') AND atttypid='uuid'::regtype AND NOT attisdropped)
  AND EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass('public.migration_admitted_activity_import')
      AND conname='admitted_activity_remainder_source_fk' AND contype='f' AND convalidated
      AND confrelid=to_regclass('public.migration_admitted_activity_plan'))
, false)
