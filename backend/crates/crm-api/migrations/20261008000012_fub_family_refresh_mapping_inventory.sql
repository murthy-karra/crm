-- D-092: bounded mapping discovery over the shared immutable core index.
ALTER TABLE migration_family_refresh_plan
 ADD COLUMN mapping_after UUID,
 ADD COLUMN mapping_current UUID,
 ADD COLUMN mapping_offset INTEGER NOT NULL DEFAULT 0 CHECK(mapping_offset>=0),
 ADD COLUMN mappings_complete BOOLEAN NOT NULL DEFAULT false,
 ADD CHECK(mapping_current IS NOT NULL OR mapping_offset=0);
ALTER TABLE migration_family_refresh_mapping
 ADD COLUMN source_row_id UUID,
 ADD COLUMN source_element INTEGER CHECK(source_element>=0),
 ADD COLUMN qualified BOOLEAN NOT NULL DEFAULT false,
 ADD FOREIGN KEY(source_row_id,bundle_id,organization_id) REFERENCES migration_family_refresh_source(id,bundle_id,organization_id);
CREATE INDEX family_refresh_core_mapping_walk ON migration_family_refresh_source(bundle_id,organization_id,id) WHERE kind IN ('person','field','note','task');
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('''owned_walk_complete''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family plan guard'; END IF;
 EXECUTE replace(definition,'''owned_walk_complete''','''owned_walk_complete'',''mapping_after'',''mapping_current'',''mapping_offset'',''mappings_complete''');
END $$;
CREATE FUNCTION crm_family_refresh_next_mapping_source(org UUID,bundle UUID,family TEXT,after_id UUID)
 RETURNS UUID LANGUAGE sql STABLE AS $$
 SELECT id FROM migration_family_refresh_source WHERE organization_id=org AND bundle_id=bundle
  AND ((family='metadata' AND kind IN ('person','field')) OR (family='activity' AND kind IN ('note','task')))
  AND (after_id IS NULL OR id>after_id) ORDER BY id LIMIT 1
$$;
REVOKE ALL ON FUNCTION crm_family_refresh_next_mapping_source(UUID,UUID,TEXT,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_next_mapping_source(UUID,UUID,TEXT,UUID) TO crm_app;
CREATE FUNCTION crm_family_refresh_mapping_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; next_source UUID;
BEGIN
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  IF ROW(NEW.mapping_after,NEW.mapping_current,NEW.mapping_offset,NEW.mappings_complete) IS NOT DISTINCT FROM ROW(OLD.mapping_after,OLD.mapping_current,OLD.mapping_offset,OLD.mappings_complete) THEN RETURN NEW; END IF;
  p:=OLD;
 ELSE
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id FOR SHARE;
 IF p.id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR p.family NOT IN ('metadata','activity') OR p.state<>'preparing' OR p.phase<>'mappings' OR p.confirmed_at IS NOT NULL OR p.mappings_complete
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan owner WHERE owner.id=b.payer_plan_id AND owner.organization_id=b.organization_id AND owner.phase NOT IN ('cohort','capture'))
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale family mapping inventory'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  next_source:=crm_family_refresh_next_mapping_source(p.organization_id,p.bundle_id,p.family,p.mapping_after);
  IF NEW.mapping_after IS DISTINCT FROM OLD.mapping_after THEN
   IF NEW.mapping_after IS NULL OR NEW.mapping_after IS DISTINCT FROM next_source OR NEW.mapping_current IS NOT NULL OR NEW.mapping_offset<>0
    THEN RAISE EXCEPTION 'family mapping walk skipped a source'; END IF;
  ELSIF NEW.mapping_current IS DISTINCT FROM OLD.mapping_current OR NEW.mapping_offset<>OLD.mapping_offset THEN
   IF NEW.mapping_current IS NULL OR NEW.mapping_current IS DISTINCT FROM next_source OR NEW.mapping_offset<=OLD.mapping_offset OR NEW.mapping_offset>OLD.mapping_offset+50
    THEN RAISE EXCEPTION 'invalid family mapping element progress'; END IF;
  END IF;
  IF NEW.mappings_complete AND (NEW.mapping_current IS NOT NULL OR crm_family_refresh_next_mapping_source(p.organization_id,p.bundle_id,p.family,NEW.mapping_after) IS NOT NULL)
   THEN RAISE EXCEPTION 'family mapping inventory incomplete'; END IF;
 ELSE
  IF (p.family='metadata') IS DISTINCT FROM (NEW.kind IN ('tag','field','option'))
   OR (NEW.kind='option' AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_mapping parent WHERE parent.id=NEW.parent_id AND parent.plan_id=p.id AND parent.organization_id=p.organization_id AND parent.kind='field'))
   OR (NEW.kind<>'option' AND NEW.parent_id IS NOT NULL)
   OR (NEW.kind<>'timezone' AND (NEW.source_row_id IS NULL OR NEW.source_element IS NULL OR NOT EXISTS(
    SELECT 1 FROM migration_family_refresh_source s WHERE s.id=NEW.source_row_id AND s.bundle_id=p.bundle_id AND s.organization_id=p.organization_id
     AND ((NEW.kind='tag' AND s.kind='person') OR (NEW.kind IN ('field','option') AND s.kind='field') OR (NEW.kind='note_author' AND s.kind='note') OR (NEW.kind IN ('task_creator','task_assignee','task_kind') AND s.kind='task')))))
   THEN RAISE EXCEPTION 'family mapping source mismatch'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_mapping_fence BEFORE INSERT ON migration_family_refresh_mapping FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_mapping_fence();
CREATE TRIGGER family_refresh_mapping_walk_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_mapping_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_mapping_fence() FROM PUBLIC;
