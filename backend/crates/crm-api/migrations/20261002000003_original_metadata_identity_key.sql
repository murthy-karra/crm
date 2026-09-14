-- PostgreSQL to_jsonb(bytea) uses one backslash in its hex representation.
-- Compare the encoded bytes directly; string-literal backslash substitution
-- previously rejected compatible original identity inserts after readiness.
CREATE FUNCTION crm_metadata_identity_key_matches(source_key BYTEA,row_value JSONB)
RETURNS BOOLEAN LANGUAGE sql IMMUTABLE STRICT AS $$
 SELECT encode(source_key,'hex')=substring(row_value->>'source_key' from 3)
$$;
REVOKE ALL ON FUNCTION crm_metadata_identity_key_matches(BYTEA,JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_metadata_identity_key_matches(BYTEA,JSONB) TO crm_app;
DO $patch$
DECLARE definition TEXT; patched TEXT;
BEGIN
 SELECT pg_get_functiondef('crm_metadata_insert_allowed(uuid,text,jsonb)'::regprocedure) INTO definition;
 patched:=replace(definition,$old$a.source_key=decode(replace(row_value->>'source_key','\\x',''),'hex')$old$,$new$crm_metadata_identity_key_matches(a.source_key,row_value)$new$);
 IF patched=definition THEN RAISE EXCEPTION 'original_metadata_identity_guard_shape_changed'; END IF;
 EXECUTE patched;
END $patch$;
