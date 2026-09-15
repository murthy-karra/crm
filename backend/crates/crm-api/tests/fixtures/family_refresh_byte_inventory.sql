-- Catalog-driven check: adding a variable-width column without accounting for
-- it must fail, even when ordinary fixture rows happen to leave that field NULL.
DO $$
DECLARE column_info RECORD; payload JSONB; expected BIGINT; observed BIGINT;
BEGIN
 FOR column_info IN
  SELECT DISTINCT a.attname,a.atttypid
  FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace
  WHERE n.nspname='public' AND (c.relname LIKE 'migration_family_refresh_%' OR c.relname IN ('fub_event_record_corrected','fub_call_record_corrected','fub_text_record_corrected')) AND c.relkind IN ('r','p')
    AND a.attnum>0 AND NOT a.attisdropped AND a.atttypid IN ('text'::regtype,'bytea'::regtype,'jsonb'::regtype)
 LOOP
  IF column_info.atttypid='bytea'::regtype THEN
   payload:=to_jsonb(decode('deadbeef','hex')); expected:=4;
  ELSIF column_info.atttypid='jsonb'::regtype THEN
   payload:=jsonb_build_object('synthetic','é'); expected:=octet_length(payload::text);
  ELSE
   payload:=to_jsonb('synthetic é'::text); expected:=octet_length('synthetic é');
  END IF;
  observed:=crm_family_refresh_retained_size(jsonb_build_object(column_info.attname,payload))-256;
  IF observed<>expected THEN RAISE EXCEPTION 'family byte inventory missing column %',column_info.attname; END IF;
 END LOOP;
END $$;
