-- D-092: complete-catalog equality without decrypting all definitions for every
-- Person. Every occurrence participates, including conflicting names for one ID.
ALTER TABLE migration_family_refresh_source
 ADD COLUMN field_name_hmac BYTEA CHECK(field_name_hmac IS NULL OR (kind='field' AND octet_length(field_name_hmac)=32)),
 ADD COLUMN catalog_names_indexed BOOLEAN NOT NULL DEFAULT false CHECK(NOT catalog_names_indexed OR kind='field');
CREATE INDEX family_refresh_field_name ON migration_family_refresh_source
 (bundle_id,organization_id,field_name_hmac,source_id) WHERE kind='field';
CREATE INDEX family_refresh_field_name_incomplete ON migration_family_refresh_source
 (bundle_id,organization_id) WHERE kind='field' AND NOT catalog_names_indexed;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_retained_size(jsonb)'::regprocedure) INTO definition;
 IF position('''source_key_hmac''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family byte inventory'; END IF;
 EXECUTE replace(definition,'''source_key_hmac''','''source_key_hmac'',''field_name_hmac''');
END $$;
-- Old immutable source indices remain unchanged and cannot qualify catalog
-- names. Prepare a new bundle; never backfill unauthenticated derived evidence.
