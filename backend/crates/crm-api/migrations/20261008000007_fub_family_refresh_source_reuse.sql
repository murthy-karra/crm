-- D-092: one immutable source index per bundle, shared across core families and
-- subsequent mapping revisions. The source's original AEAD scope/payer remains.
CREATE INDEX family_refresh_source_lookup ON migration_family_refresh_source(bundle_id,organization_id,kind,source_id,representation,id);
ALTER TABLE migration_family_refresh_source ADD CONSTRAINT family_refresh_source_bundle_key UNIQUE(id,bundle_id,organization_id);
DO $$ DECLARE constraint_name TEXT; BEGIN
 SELECT conname INTO STRICT constraint_name FROM pg_constraint WHERE conrelid='migration_family_refresh_manifest'::regclass AND confrelid='migration_family_refresh_source'::regclass AND contype='f';
 EXECUTE format('ALTER TABLE migration_family_refresh_manifest DROP CONSTRAINT %I',constraint_name);
END $$;
ALTER TABLE migration_family_refresh_manifest ADD CONSTRAINT family_refresh_manifest_source_bundle_fk FOREIGN KEY(source_row_id,bundle_id,organization_id) REFERENCES migration_family_refresh_source(id,bundle_id,organization_id);
-- A shared source must still have the unit's kind and frozen Person binding.
CREATE FUNCTION crm_family_refresh_manifest_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
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
END $$;
CREATE TRIGGER family_refresh_manifest_source_guard BEFORE INSERT ON migration_family_refresh_manifest FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_manifest_source_guard();
REVOKE ALL ON FUNCTION crm_family_refresh_manifest_source_guard() FROM PUBLIC;
DO $$ DECLARE definition TEXT; old TEXT; replacement TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_mutation_allowed(uuid,text,text,jsonb,jsonb)'::regprocedure) INTO definition;
 old:='id=unit.source_row_id AND plan_id=unit.plan_id AND bundle_id=unit.bundle_id';
 IF position(old IN definition)=0 THEN RAISE EXCEPTION 'unexpected refresh source guard'; END IF;
 definition:=replace(definition,old,'id=unit.source_row_id AND bundle_id=unit.bundle_id');
 old:='IF EXISTS(SELECT 1 FROM migration_family_refresh_source sibling WHERE sibling.plan_id=unit.plan_id AND sibling.organization_id=org AND sibling.kind=unit.kind AND sibling.identity_hmac=source_record.identity_hmac AND (NOT sibling.qualified OR sibling.semantic_hmac<>source_record.semantic_hmac OR sibling.source_person_id IS DISTINCT FROM source_record.source_person_id)) THEN RETURN false; END IF;';
 replacement:='IF table_name=''note'' AND source_record.representation<>''fub-core-v1/notes/detail/replies,reactions'' THEN RETURN false; END IF;
  IF EXISTS(SELECT 1 FROM migration_family_refresh_source sibling LEFT JOIN migration_family_refresh_core_page page ON page.id=sibling.core_page_id AND page.bundle_id=sibling.bundle_id AND page.organization_id=sibling.organization_id WHERE sibling.bundle_id=unit.bundle_id AND sibling.organization_id=org AND sibling.kind=unit.kind AND sibling.source_id=source_record.source_id GROUP BY sibling.representation HAVING count(DISTINCT sibling.semantic_hmac)>1 OR bool_or(NOT sibling.qualified) OR bool_or(sibling.source_person_id IS DISTINCT FROM source_record.source_person_id) OR (table_name=''task'' AND count(DISTINCT page.stream)>1)) THEN RETURN false; END IF;';
 IF position(old IN definition)=0 THEN RAISE EXCEPTION 'unexpected refresh occurrence guard'; END IF;
 EXECUTE replace(definition,old,replacement);
 SELECT pg_get_functiondef('crm_family_history_version_guard()'::regprocedure) INTO definition;
 old:='id=u.source_row_id AND plan_id=p.id AND organization_id=NEW.organization_id';
 IF position(old IN definition)=0 THEN RAISE EXCEPTION 'unexpected history source guard'; END IF;
 EXECUTE replace(definition,old,'id=u.source_row_id AND bundle_id=b.id AND organization_id=NEW.organization_id');
END $$;
