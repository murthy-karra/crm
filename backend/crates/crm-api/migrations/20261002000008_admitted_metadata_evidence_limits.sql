-- Reader/execution limits are visible frozen holds, not failed native attempts.
ALTER TABLE migration_admitted_metadata_manifest ADD COLUMN oversized BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE migration_admitted_metadata_mapping ADD COLUMN field_name_key BYTEA CHECK(field_name_key IS NULL OR octet_length(field_name_key)=32);
CREATE INDEX migration_admitted_metadata_field_name ON migration_admitted_metadata_mapping(plan_id,organization_id,field_name_key,id) WHERE field_name_key IS NOT NULL;
ALTER TABLE migration_admitted_metadata_plan DROP CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check;
ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check CHECK(preparation_phase IN ('sources','fields','options','tags','choices','cohort','values','extra_values','seal','complete','remainder_mappings','remainder_people','remainder_operations'));
DO $migration$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_admitted_metadata_preparation_bytes()'::regprocedure) INTO definition;
 definition:=replace(definition,'added:=octet_length(NEW.source_key)+octet_length(NEW.source_id)+octet_length(NEW.nonce)+octet_length(NEW.ciphertext);','added:=octet_length(NEW.source_key)+octet_length(NEW.source_id)+octet_length(NEW.nonce)+octet_length(NEW.ciphertext)+COALESCE(octet_length(NEW.field_name_key),0);');
 definition:=replace(definition,'prior:=octet_length(OLD.source_key)+octet_length(OLD.source_id)+octet_length(OLD.nonce)+octet_length(OLD.ciphertext);','prior:=octet_length(OLD.source_key)+octet_length(OLD.source_id)+octet_length(OLD.nonce)+octet_length(OLD.ciphertext)+COALESCE(octet_length(OLD.field_name_key),0);');
 EXECUTE definition;
END $migration$;
