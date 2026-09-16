-- Fixed-width ordering projections over existing immutable source references.
-- No choice, encrypted value or logical retained-byte charge is rewritten.
ALTER TABLE migration_family_refresh_mapping
 ADD COLUMN source_sequence BIGINT CHECK(source_sequence>=0),
 ADD COLUMN source_ordinal INTEGER CHECK(source_ordinal>=0),
 ADD CHECK((source_sequence IS NULL)=(source_ordinal IS NULL));
ALTER TABLE migration_family_refresh_mapping DISABLE TRIGGER family_refresh_immutable;
UPDATE migration_family_refresh_mapping m SET source_sequence=s.capture_sequence,source_ordinal=s.ordinal
 FROM migration_family_refresh_source s WHERE s.id=m.source_row_id AND s.bundle_id=m.bundle_id AND s.organization_id=m.organization_id;
ALTER TABLE migration_family_refresh_mapping ENABLE TRIGGER family_refresh_immutable;
CREATE FUNCTION crm_family_refresh_mapping_order_projection() RETURNS trigger LANGUAGE plpgsql AS $$
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
END $$;
CREATE TRIGGER family_refresh_catalog_order_projection BEFORE INSERT ON migration_family_refresh_mapping FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_mapping_order_projection();
REVOKE ALL ON FUNCTION crm_family_refresh_mapping_order_projection() FROM PUBLIC;
CREATE INDEX family_refresh_catalog_mapping_order ON migration_family_refresh_mapping
 (plan_id,organization_id,source_sequence,source_ordinal,source_row_id,source_element,id) WHERE kind IN ('tag','field','option');
ALTER TABLE migration_family_refresh_plan ADD COLUMN catalog_after UUID, ADD COLUMN catalog_walk_complete BOOLEAN NOT NULL DEFAULT false;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('''mappings_complete''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family plan guard'; END IF;
 EXECUTE replace(definition,'''mappings_complete''','''mappings_complete'',''catalog_after'',''catalog_walk_complete''');
END $$;
CREATE FUNCTION crm_family_refresh_next_catalog_mapping(org UUID,bundle UUID,plan UUID,after_id UUID)
 RETURNS UUID LANGUAGE sql STABLE AS $$
 SELECT m.id FROM migration_family_refresh_mapping m WHERE m.organization_id=org AND m.bundle_id=bundle AND m.plan_id=plan
  AND m.kind IN ('tag','field','option') AND m.source_sequence IS NOT NULL AND m.source_ordinal IS NOT NULL AND m.source_row_id IS NOT NULL AND m.source_element IS NOT NULL
  AND (after_id IS NULL OR (m.source_sequence,m.source_ordinal,m.source_row_id,m.source_element,m.id)>
   (SELECT a.source_sequence,a.source_ordinal,a.source_row_id,a.source_element,a.id FROM migration_family_refresh_mapping a WHERE a.id=after_id AND a.plan_id=plan AND a.bundle_id=bundle AND a.organization_id=org))
 ORDER BY m.source_sequence,m.source_ordinal,m.source_row_id,m.source_element,m.id LIMIT 1
$$;
REVOKE ALL ON FUNCTION crm_family_refresh_next_catalog_mapping(UUID,UUID,UUID,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_next_catalog_mapping(UUID,UUID,UUID,UUID) TO crm_app;
CREATE FUNCTION crm_family_refresh_catalog_walk_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; next_id UUID;
BEGIN
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
END $$;
CREATE TRIGGER family_refresh_catalog_walk_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_catalog_walk_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_catalog_walk_fence() FROM PUBLIC;
