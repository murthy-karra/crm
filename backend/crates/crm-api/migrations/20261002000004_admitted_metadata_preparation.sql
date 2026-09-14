-- Preparation persists one bounded retained-source/catalog/Person step at a time.
ALTER TABLE migration_admitted_metadata_plan
 ADD COLUMN preparation_phase TEXT NOT NULL DEFAULT 'sources' CHECK(preparation_phase IN ('sources','fields','options','tags','cohort','values','seal','complete')),
 ADD COLUMN preparation_sequence BIGINT NOT NULL DEFAULT 0,
 ADD COLUMN preparation_ordinal INTEGER NOT NULL DEFAULT -1,
 ADD COLUMN preparation_key UUID,
 ADD COLUMN preparation_parent UUID,
 ADD COLUMN preparation_manifest UUID,
 ADD COLUMN preparation_bytes BIGINT NOT NULL DEFAULT 0 CHECK(preparation_bytes BETWEEN 0 AND 67108864),
 ADD COLUMN fields_processed BIGINT NOT NULL DEFAULT 0,
 ADD COLUMN people_processed BIGINT NOT NULL DEFAULT 0;
ALTER TABLE migration_admitted_metadata_source
 ADD COLUMN qualified BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN conflict BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN observations BIGINT NOT NULL DEFAULT 1,
 ADD COLUMN capture_id UUID,
 ADD COLUMN capture_sequence BIGINT,
 ADD COLUMN ordinal INTEGER;
CREATE TABLE migration_admitted_metadata_observation (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 source_row_id UUID, snapshot_record_id UUID NOT NULL, semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 qualified BOOLEAN NOT NULL,
 UNIQUE(plan_id,organization_id,snapshot_record_id),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(source_row_id) REFERENCES migration_admitted_metadata_source(id),
 FOREIGN KEY(snapshot_record_id) REFERENCES migration_snapshot_record(id)
);
CREATE INDEX migration_admitted_metadata_source_work ON migration_admitted_metadata_source(plan_id,organization_id,family,id);
CREATE INDEX migration_admitted_metadata_source_order ON migration_admitted_metadata_source(plan_id,organization_id,family,capture_sequence,ordinal,id);
CREATE INDEX migration_admitted_metadata_observation_source ON migration_admitted_metadata_observation(plan_id,organization_id,source_row_id,id);
CREATE INDEX migration_admitted_metadata_preparation_work ON migration_admitted_metadata_import(created_at,id) WHERE state='proposed';
CREATE INDEX migration_admitted_metadata_admission_work ON migration_people_admission_result(admission_id,organization_id,id) WHERE disposition='settled';
GRANT SELECT,UPDATE ON migration_admitted_metadata_source TO crm_app;
GRANT SELECT,INSERT ON migration_admitted_metadata_observation TO crm_app;
-- A logical byte accumulator is O(1) per changed row. It is private staging
-- bookkeeping, settled with its lease/checkpoint in the same transaction.
CREATE FUNCTION crm_admitted_metadata_preparation_bytes() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE added BIGINT; prior BIGINT:=0;
BEGIN
 IF TG_TABLE_NAME='migration_admitted_metadata_source' THEN
  added:=octet_length(NEW.source_id)+octet_length(NEW.semantic_hmac)+octet_length(NEW.nonce)+octet_length(NEW.ciphertext);
  IF TG_OP='UPDATE' THEN prior:=octet_length(OLD.source_id)+octet_length(OLD.semantic_hmac)+octet_length(OLD.nonce)+octet_length(OLD.ciphertext); END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_mapping' THEN
  added:=octet_length(NEW.source_key)+octet_length(NEW.source_id)+octet_length(NEW.nonce)+octet_length(NEW.ciphertext);
  IF TG_OP='UPDATE' THEN prior:=octet_length(OLD.source_key)+octet_length(OLD.source_id)+octet_length(OLD.nonce)+octet_length(OLD.ciphertext); END IF;
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
END $$;
REVOKE ALL ON FUNCTION crm_admitted_metadata_preparation_bytes() FROM PUBLIC;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['migration_admitted_metadata_source','migration_admitted_metadata_mapping','migration_admitted_metadata_manifest','migration_admitted_metadata_operation','migration_admitted_metadata_observation'] LOOP
  EXECUTE format('CREATE TRIGGER admitted_metadata_preparation_bytes AFTER INSERT OR UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_preparation_bytes()',t);
 END LOOP;
END $$;
-- A preview must not prevent ordinary erasure or silently lose the successful
-- admission result. The expected identity remains an evidence value.
ALTER TABLE migration_admitted_metadata_manifest ADD COLUMN expected_person_id UUID;
UPDATE migration_admitted_metadata_manifest SET expected_person_id=person_id;
ALTER TABLE migration_admitted_metadata_manifest ALTER COLUMN person_id DROP NOT NULL;
DO $$ DECLARE c RECORD; BEGIN
 FOR c IN SELECT conname FROM pg_constraint WHERE conrelid='migration_admitted_metadata_manifest'::regclass AND confrelid='person'::regclass LOOP
  EXECUTE format('ALTER TABLE migration_admitted_metadata_manifest DROP CONSTRAINT %I',c.conname);
 END LOOP;
 FOR c IN SELECT conname FROM pg_constraint WHERE conrelid='migration_admitted_metadata_result'::regclass AND confrelid='person'::regclass LOOP
  EXECUTE format('ALTER TABLE migration_admitted_metadata_result DROP CONSTRAINT %I',c.conname);
 END LOOP;
END $$;
ALTER TABLE migration_admitted_metadata_manifest ADD FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id) ON DELETE SET NULL (person_id);
ALTER TABLE migration_admitted_metadata_result ADD FOREIGN KEY(person_id,organization_id) REFERENCES person(id,organization_id) ON DELETE SET NULL (person_id);
