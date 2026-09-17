-- Exact successors retain a fixed source plan. Copying is bounded preparation,
-- never a fresh discovery or permission to replay a settled native mutation.
ALTER TABLE migration_family_refresh_bundle
 ADD COLUMN remainder_source_bundle_id UUID,
 ADD COLUMN remainder_origin_bundle_id UUID,
 ADD FOREIGN KEY(remainder_source_bundle_id,organization_id) REFERENCES migration_family_refresh_bundle(id,organization_id),
 ADD FOREIGN KEY(remainder_origin_bundle_id,organization_id) REFERENCES migration_family_refresh_bundle(id,organization_id),
 ADD CHECK((predecessor_id IS NULL)=(remainder_source_bundle_id IS NULL)),
 ADD CHECK((predecessor_id IS NULL)=(remainder_origin_bundle_id IS NULL));
ALTER TABLE migration_family_refresh_plan
 ADD COLUMN remainder_source_plan_id UUID,
 ADD COLUMN remainder_stage SMALLINT NOT NULL DEFAULT 0 CHECK(remainder_stage BETWEEN 0 AND 8),
 ADD COLUMN remainder_after UUID,
 ADD COLUMN remainder_complete BOOLEAN NOT NULL DEFAULT true,
 ADD COLUMN inherited_position BIGINT NOT NULL DEFAULT 0 CHECK(inherited_position>=0),
 ADD FOREIGN KEY(remainder_source_plan_id,organization_id) REFERENCES migration_family_refresh_plan(id,organization_id);
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['cohort','core_page','history_page','source','mapping','manifest'] LOOP
  EXECUTE format('ALTER TABLE migration_family_refresh_%I ADD COLUMN remainder_source_id UUID',name);
  EXECUTE format('CREATE UNIQUE INDEX family_refresh_%s_remainder_copy ON migration_family_refresh_%I(bundle_id,organization_id,remainder_source_id) WHERE remainder_source_id IS NOT NULL',name,name);
 END LOOP;
END $$;
ALTER TABLE migration_family_refresh_manifest ADD COLUMN inherited_result_id UUID,
 ADD FOREIGN KEY(inherited_result_id,organization_id) REFERENCES migration_family_refresh_result(id,organization_id),
 ADD CHECK(inherited_result_id IS NULL OR (kind='catalog' AND remainder_source_id IS NOT NULL));

CREATE FUNCTION crm_family_refresh_remainder_plan_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan;
BEGIN
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF NEW.remainder_source_plan_id IS NULL THEN
  IF b.predecessor_id IS NOT NULL OR NOT NEW.remainder_complete OR NEW.remainder_stage<>0 OR NEW.inherited_position<>0 THEN RAISE EXCEPTION 'invalid ordinary refresh plan'; END IF;
 ELSE
  IF TG_OP='INSERT' AND (NEW.remainder_complete OR NEW.remainder_stage<>0 OR NEW.remainder_after IS NOT NULL OR NEW.inherited_position<>0 OR NEW.position<>0 OR NEW.apply_position<>0) THEN RAISE EXCEPTION 'invalid initial remainder progress'; END IF;
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.remainder_source_plan_id AND organization_id=NEW.organization_id;
  IF b.predecessor_id IS NULL OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_bundle source WHERE source.id=p.bundle_id AND source.organization_id=b.organization_id AND COALESCE(source.remainder_origin_bundle_id,source.id)=b.remainder_origin_bundle_id AND source.parent_import_id=b.parent_import_id AND source.parent_plan_id=b.parent_plan_id AND source.source_account_id=b.source_account_id) OR p.family IS DISTINCT FROM NEW.family
   OR p.confirmed_at IS NULL OR p.state NOT IN ('cancelled','completed','superseded')
   OR p.source_snapshot_id IS DISTINCT FROM NEW.source_snapshot_id OR p.history_capture_id IS DISTINCT FROM NEW.history_capture_id THEN RAISE EXCEPTION 'invalid remainder source plan'; END IF;
  IF TG_OP='UPDATE' AND (NEW.remainder_stage<OLD.remainder_stage OR NEW.inherited_position<OLD.inherited_position
   OR (OLD.remainder_complete AND (NOT NEW.remainder_complete OR NEW.inherited_position<>OLD.inherited_position OR NEW.remainder_stage<>OLD.remainder_stage OR NEW.remainder_after IS DISTINCT FROM OLD.remainder_after))) THEN RAISE EXCEPTION 'remainder copy regression'; END IF;
  IF NEW.confirmed_at IS NOT NULL AND (NOT NEW.remainder_complete OR NEW.remainder_stage<>8) THEN RAISE EXCEPTION 'incomplete remainder confirmation'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_remainder_plan_guard BEFORE INSERT OR UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_remainder_plan_guard();
REVOKE ALL ON FUNCTION crm_family_refresh_remainder_plan_guard() FROM PUBLIC;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('IF (to_jsonb(NEW)-ARRAY[' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family plan guard'; END IF;
 definition:=replace(definition,'IF (to_jsonb(NEW)-ARRAY[','IF (to_jsonb(NEW)-ARRAY[''remainder_after'',''remainder_stage'',''remainder_complete'',''inherited_position'',');
 definition:=replace(definition,'IS DISTINCT FROM (to_jsonb(OLD)-ARRAY[','IS DISTINCT FROM (to_jsonb(OLD)-ARRAY[''remainder_after'',''remainder_stage'',''remainder_complete'',''inherited_position'',');
 EXECUTE definition;
END $$;

CREATE FUNCTION crm_family_refresh_remainder_copy_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; old_row JSONB; v JSONB:=to_jsonb(NEW); old_plan UUID; source_bundle UUID; ignored TEXT[];
BEGIN
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id;
 IF NEW.remainder_source_id IS NULL THEN
  IF b.predecessor_id IS NOT NULL THEN RAISE EXCEPTION 'remainder requires frozen source'; END IF;
  RETURN NEW;
 END IF;
 IF b.predecessor_id IS NULL OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL THEN RAISE EXCEPTION 'invalid remainder copy owner'; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=COALESCE((v->>'plan_id')::uuid,b.payer_plan_id) AND bundle_id=b.id AND organization_id=b.organization_id;
 IF p.id IS NULL OR p.state<>'preparing' OR p.remainder_complete OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin') THEN RAISE EXCEPTION 'stale remainder copy'; END IF;
 IF NEW.remainder_source_id IS DISTINCT FROM crm_family_refresh_remainder_next(p) OR TG_TABLE_NAME IS DISTINCT FROM 'migration_family_refresh_'||(ARRAY['cohort','core_page','history_page','source','mapping','mapping','manifest','manifest'])[p.remainder_stage+1] THEN RAISE EXCEPTION 'unexpected remainder copy step'; END IF;
 IF p.id<>b.payer_plan_id AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE id=b.payer_plan_id AND organization_id=b.organization_id AND remainder_stage>=4) THEN RAISE EXCEPTION 'remainder payer incomplete'; END IF;
 source_bundle:=CASE WHEN TG_TABLE_NAME IN ('migration_family_refresh_mapping','migration_family_refresh_manifest','migration_family_refresh_history_page') OR (TG_TABLE_NAME='migration_family_refresh_source' AND p.family='history') THEN (SELECT bundle_id FROM migration_family_refresh_plan WHERE id=p.remainder_source_plan_id AND organization_id=p.organization_id) ELSE b.remainder_source_bundle_id END;
 -- Trigger installation below is a fixed table allowlist, never a caller name.
 EXECUTE format('SELECT to_jsonb(s) FROM %I s WHERE id=$1 AND bundle_id=$2 AND organization_id=$3',TG_TABLE_NAME)
  INTO old_row USING NEW.remainder_source_id,source_bundle,b.organization_id;
 IF old_row IS NULL THEN RAISE EXCEPTION 'missing frozen remainder row'; END IF;
 old_plan:=(old_row->>'plan_id')::uuid;
 IF TG_TABLE_NAME IN ('migration_family_refresh_mapping','migration_family_refresh_manifest') AND old_plan IS DISTINCT FROM p.remainder_source_plan_id THEN RAISE EXCEPTION 'wrong remainder family plan'; END IF;
 IF TG_TABLE_NAME IN ('migration_family_refresh_source','migration_family_refresh_core_page','migration_family_refresh_history_page') AND NOT EXISTS(
  SELECT 1 FROM migration_family_refresh_plan s JOIN migration_family_refresh_bundle source ON source.id=s.bundle_id AND source.organization_id=s.organization_id
  WHERE s.id=old_plan AND s.bundle_id=source_bundle AND s.organization_id=b.organization_id AND (s.id=source.payer_plan_id OR (s.id=p.remainder_source_plan_id AND s.family='history'))
 ) THEN RAISE EXCEPTION 'wrong remainder source owner'; END IF;
 ignored:=ARRAY['id','bundle_id','plan_id','nonce','ciphertext','remainder_source_id'];
 IF TG_TABLE_NAME='migration_family_refresh_source' THEN
  ignored:=ignored||ARRAY['core_page_id','history_page_id'];
  IF (old_row->>'history_page_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_history_page WHERE id=(v->>'history_page_id')::uuid AND bundle_id=b.id AND organization_id=b.organization_id AND remainder_source_id=(old_row->>'history_page_id')::uuid) THEN RAISE EXCEPTION 'wrong copied history page'; END IF;
  IF (old_row->>'core_page_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_core_page WHERE id=(v->>'core_page_id')::uuid AND bundle_id=b.id AND organization_id=b.organization_id AND remainder_source_id=(old_row->>'core_page_id')::uuid) THEN RAISE EXCEPTION 'wrong copied core page'; END IF;
 ELSIF TG_TABLE_NAME='migration_family_refresh_mapping' THEN
  ignored:=ignored||ARRAY['parent_id','source_row_id'];
  IF (old_row->>'parent_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_mapping WHERE id=(v->>'parent_id')::uuid AND plan_id=p.id AND organization_id=b.organization_id AND remainder_source_id=(old_row->>'parent_id')::uuid) THEN RAISE EXCEPTION 'wrong copied parent mapping'; END IF;
 ELSIF TG_TABLE_NAME='migration_family_refresh_manifest' THEN
  ignored:=ignored||ARRAY['position','cohort_id','source_row_id','mapping_id','counts','added_byte_bound','inherited_result_id'];
  IF NEW.inherited_result_id IS NOT NULL THEN
   IF NEW.kind<>'catalog' OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE id=NEW.inherited_result_id AND organization_id=b.organization_id
    AND (manifest_id=NEW.remainder_source_id OR id=(old_row->>'inherited_result_id')::uuid) AND disposition IN ('applied','already_current','held','excluded')) THEN RAISE EXCEPTION 'invalid inherited catalog result'; END IF;
   IF NEW.counts IS DISTINCT FROM (SELECT jsonb_object_agg(key,'0'::text) FROM jsonb_each(old_row->'counts')) THEN RAISE EXCEPTION 'inherited catalog cannot count as unfinished'; END IF;
  ELSE
   IF old_row->>'disposition' NOT IN ('insert','update','already_current','correction') OR (old_row->>'position')::bigint<=(SELECT apply_position FROM migration_family_refresh_plan WHERE id=p.remainder_source_plan_id)
    OR EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE manifest_id=NEW.remainder_source_id AND organization_id=b.organization_id)
    OR NEW.counts IS DISTINCT FROM old_row->'counts' THEN RAISE EXCEPTION 'remainder is not unfinished eligible work'; END IF;
  END IF;
  IF (old_row->>'cohort_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_cohort c JOIN migration_family_refresh_cohort original ON original.id=(old_row->>'cohort_id')::uuid AND original.organization_id=c.organization_id WHERE c.id=NEW.cohort_id AND c.bundle_id=b.id AND c.organization_id=b.organization_id AND to_jsonb(c)-ARRAY['id','bundle_id','remainder_source_id'] = to_jsonb(original)-ARRAY['id','bundle_id','remainder_source_id']) THEN RAISE EXCEPTION 'wrong copied cohort'; END IF;
  IF (old_row->>'mapping_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_mapping WHERE id=NEW.mapping_id AND plan_id=p.id AND organization_id=b.organization_id AND remainder_source_id=(old_row->>'mapping_id')::uuid) THEN RAISE EXCEPTION 'wrong copied mapping'; END IF;
 END IF;
 IF TG_TABLE_NAME IN ('migration_family_refresh_mapping','migration_family_refresh_manifest') AND (old_row->>'source_row_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_source c JOIN migration_family_refresh_source original ON original.id=(old_row->>'source_row_id')::uuid AND original.organization_id=c.organization_id WHERE c.id=(v->>'source_row_id')::uuid AND c.bundle_id=b.id AND c.organization_id=b.organization_id AND to_jsonb(c)-ARRAY['id','bundle_id','plan_id','remainder_source_id','core_page_id','history_page_id','nonce','ciphertext'] = to_jsonb(original)-ARRAY['id','bundle_id','plan_id','remainder_source_id','core_page_id','history_page_id','nonce','ciphertext']) THEN RAISE EXCEPTION 'wrong copied source'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_source' AND ((v->>'core_page_id') IS NULL)<>((old_row->>'core_page_id') IS NULL) THEN RAISE EXCEPTION 'copied page presence changed'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_mapping' AND ((v->>'parent_id') IS NULL)<>((old_row->>'parent_id') IS NULL) THEN RAISE EXCEPTION 'copied mapping parent changed'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_manifest' AND (((v->>'cohort_id') IS NULL)<>((old_row->>'cohort_id') IS NULL) OR ((v->>'mapping_id') IS NULL)<>((old_row->>'mapping_id') IS NULL)) THEN RAISE EXCEPTION 'copied unit references changed'; END IF;
 IF TG_TABLE_NAME IN ('migration_family_refresh_mapping','migration_family_refresh_manifest') AND ((v->>'source_row_id') IS NULL)<>((old_row->>'source_row_id') IS NULL) THEN RAISE EXCEPTION 'copied source presence changed'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_source' AND ((v->>'history_page_id') IS NULL)<>((old_row->>'history_page_id') IS NULL) THEN RAISE EXCEPTION 'copied history page presence changed'; END IF;
 IF v-ignored IS DISTINCT FROM old_row-ignored THEN RAISE EXCEPTION 'remainder changed frozen header'; END IF;
 RETURN NEW;
END $$;
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['cohort','core_page','history_page','source','mapping','manifest'] LOOP
  EXECUTE format('CREATE TRIGGER family_refresh_remainder_copy_guard BEFORE INSERT ON migration_family_refresh_%I FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_remainder_copy_guard()',name);
 END LOOP;
END $$;
REVOKE ALL ON FUNCTION crm_family_refresh_remainder_copy_guard() FROM PUBLIC;

-- A single keyset successor defines each copy step. Advancing a cursor requires
-- its exact row; changing stages requires exhaustion. No whole-source rescan is
-- accepted as a replacement for the original frozen manifest.
CREATE FUNCTION crm_family_refresh_remainder_next(p migration_family_refresh_plan) RETURNS UUID LANGUAGE plpgsql STABLE AS $$
DECLARE b migration_family_refresh_bundle; source migration_family_refresh_plan; answer UUID;
BEGIN
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id;
 SELECT * INTO source FROM migration_family_refresh_plan WHERE id=p.remainder_source_plan_id AND organization_id=p.organization_id;
 IF source.id IS NULL THEN RETURN NULL; END IF;
 CASE p.remainder_stage
 WHEN 0 THEN
  IF p.id=b.payer_plan_id THEN SELECT id INTO answer FROM migration_family_refresh_cohort WHERE bundle_id=source.bundle_id AND organization_id=p.organization_id AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1; END IF;
 WHEN 1 THEN
  IF p.id=b.payer_plan_id AND p.family<>'history' THEN SELECT id INTO answer FROM migration_family_refresh_core_page WHERE bundle_id=source.bundle_id AND organization_id=p.organization_id AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1; END IF;
 WHEN 2 THEN
  IF p.family='history' THEN SELECT id INTO answer FROM migration_family_refresh_history_page WHERE plan_id=source.id AND organization_id=p.organization_id AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1; END IF;
 WHEN 3 THEN
  IF p.id=b.payer_plan_id OR p.family='history' THEN SELECT id INTO answer FROM migration_family_refresh_source WHERE bundle_id=source.bundle_id AND organization_id=p.organization_id AND (CASE WHEN p.family='history' THEN history_page_id IS NOT NULL AND plan_id=source.id ELSE core_page_id IS NOT NULL END) AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1; END IF;
 WHEN 4,5 THEN
  SELECT id INTO answer FROM migration_family_refresh_mapping WHERE plan_id=source.id AND organization_id=p.organization_id AND (parent_id IS NULL)=(p.remainder_stage=4) AND (p.remainder_after IS NULL OR id>p.remainder_after) ORDER BY id LIMIT 1;
 WHEN 6 THEN
  SELECT u.id INTO answer FROM migration_family_refresh_manifest u WHERE u.plan_id=source.id AND u.organization_id=p.organization_id AND u.kind='catalog' AND u.disposition IN ('insert','already_current')
   AND (u.inherited_result_id IS NOT NULL OR EXISTS(SELECT 1 FROM migration_family_refresh_result r WHERE r.manifest_id=u.id AND r.organization_id=u.organization_id AND r.disposition IN ('applied','already_current','held','excluded')))
   AND (p.remainder_after IS NULL OR u.position>(SELECT position FROM migration_family_refresh_manifest WHERE id=p.remainder_after AND plan_id=source.id AND organization_id=p.organization_id)) ORDER BY position LIMIT 1;
 WHEN 7 THEN
  SELECT u.id INTO answer FROM migration_family_refresh_manifest u WHERE u.plan_id=source.id AND u.organization_id=p.organization_id AND u.position>source.apply_position
   AND u.disposition IN ('insert','update','already_current','correction') AND u.inherited_result_id IS NULL
   AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_result r WHERE r.manifest_id=u.id AND r.organization_id=u.organization_id)
   AND (p.remainder_after IS NULL OR u.position>(SELECT position FROM migration_family_refresh_manifest WHERE id=p.remainder_after AND plan_id=source.id AND organization_id=p.organization_id)) ORDER BY position LIMIT 1;
 ELSE NULL;
 END CASE;
 RETURN answer;
END $$;
REVOKE ALL ON FUNCTION crm_family_refresh_remainder_next(migration_family_refresh_plan) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_remainder_next(migration_family_refresh_plan) TO crm_app;

CREATE FUNCTION crm_family_refresh_remainder_progress_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; next_id UUID; table_name TEXT; copied JSONB;
BEGIN
 IF OLD.remainder_source_plan_id IS NULL OR OLD.remainder_complete THEN RETURN NEW; END IF;
 IF ROW(NEW.remainder_stage,NEW.remainder_after,NEW.remainder_complete,NEW.inherited_position,NEW.position,NEW.apply_position,NEW.counts,NEW.source_walk_complete,NEW.owned_walk_complete,NEW.mappings_complete,NEW.catalog_walk_complete)
  IS NOT DISTINCT FROM ROW(OLD.remainder_stage,OLD.remainder_after,OLD.remainder_complete,OLD.inherited_position,OLD.position,OLD.apply_position,OLD.counts,OLD.source_walk_complete,OLD.owned_walk_complete,OLD.mappings_complete,OLD.catalog_walk_complete) THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=OLD.bundle_id AND organization_id=OLD.organization_id;
 IF OLD.state<>'preparing' OR b.state<>'preparing' OR OLD.confirmed_at IS NOT NULL
  OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at IS NULL OR OLD.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND status='active' AND role='admin')
  THEN RAISE EXCEPTION 'stale remainder progress'; END IF;
 next_id:=crm_family_refresh_remainder_next(OLD);
 IF NEW.remainder_stage<>OLD.remainder_stage THEN
  IF next_id IS NOT NULL OR NEW.remainder_stage<>OLD.remainder_stage+1 OR NEW.remainder_after IS NOT NULL OR NEW.position<>OLD.position OR NEW.counts<>OLD.counts OR NEW.inherited_position<>OLD.inherited_position THEN RAISE EXCEPTION 'remainder skipped frozen work'; END IF;
 ELSE
  IF next_id IS NULL OR NEW.remainder_after IS DISTINCT FROM next_id THEN RAISE EXCEPTION 'invalid remainder checkpoint'; END IF;
  table_name:=(ARRAY['cohort','core_page','history_page','source','mapping','mapping','manifest','manifest'])[OLD.remainder_stage+1];
  EXECUTE format('SELECT to_jsonb(c) FROM migration_family_refresh_%I c WHERE bundle_id=$1 AND organization_id=$2 AND remainder_source_id=$3',table_name) INTO copied USING OLD.bundle_id,OLD.organization_id,next_id;
  IF copied IS NULL OR (copied ? 'plan_id' AND (copied->>'plan_id')::uuid<>OLD.id) THEN RAISE EXCEPTION 'remainder checkpoint without copy'; END IF;
  IF OLD.remainder_stage IN (6,7) THEN
   IF NEW.position<>OLD.position+1 OR (copied->>'position')::bigint<>NEW.position OR NEW.inherited_position<>OLD.inherited_position+(CASE WHEN OLD.remainder_stage=6 THEN 1 ELSE 0 END)
    OR NEW.counts IS DISTINCT FROM (SELECT jsonb_object_agg(key,((value::text)::numeric+COALESCE((OLD.counts->>key)::numeric,0))::text) FROM jsonb_each_text(copied->'counts')) THEN RAISE EXCEPTION 'invalid remainder unit counts'; END IF;
  ELSIF NEW.position<>OLD.position OR NEW.counts<>OLD.counts OR NEW.inherited_position<>OLD.inherited_position THEN RAISE EXCEPTION 'invalid remainder progress counts'; END IF;
 END IF;
 IF NEW.remainder_stage=8 THEN
  IF NOT NEW.remainder_complete OR NOT NEW.source_walk_complete OR NOT NEW.owned_walk_complete OR NOT NEW.mappings_complete OR NOT NEW.catalog_walk_complete OR NEW.apply_position<>NEW.inherited_position THEN RAISE EXCEPTION 'incomplete remainder boundary'; END IF;
 ELSIF NEW.remainder_complete OR NEW.source_walk_complete OR NEW.owned_walk_complete OR NEW.mappings_complete OR NEW.catalog_walk_complete OR NEW.apply_position<>OLD.apply_position THEN RAISE EXCEPTION 'premature remainder boundary'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_remainder_progress_guard BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_remainder_progress_guard();
REVOKE ALL ON FUNCTION crm_family_refresh_remainder_progress_guard() FROM PUBLIC;

-- These discovery-only fences defer to the stricter frozen-copy fences above.
-- Native-write, result, ownership, source-binding and sealing fences stay active.
DO $$ DECLARE name TEXT; definition TEXT; branch TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['cohort_fence','core_index_guard','history_index_guard','mapping_fence','catalog_unit_fence','person_metadata_fence','walk_fence','owned_walk_fence','activity_walk_fence','activity_owned_fence','catalog_walk_fence','metadata_walk_fence','metadata_owned_fence','apply_fence'] LOOP
  SELECT pg_get_functiondef(('crm_family_refresh_'||name||'()')::regprocedure) INTO definition;
  branch:=E'BEGIN\n IF TG_OP=''INSERT'' AND to_jsonb(NEW)->>''remainder_source_id'' IS NOT NULL THEN RETURN NEW; END IF;\n IF TG_OP=''UPDATE'' AND TG_TABLE_NAME=''migration_family_refresh_plan'' AND to_jsonb(OLD)->>''remainder_source_plan_id'' IS NOT NULL AND NOT (to_jsonb(OLD)->>''remainder_complete'')::boolean THEN RETURN NEW; END IF;';
  IF position(E'BEGIN\n' IN definition)=0 THEN RAISE EXCEPTION 'unexpected discovery fence %',name; END IF;
  EXECUTE replace(definition,E'BEGIN\n',branch||E'\n');
 END LOOP;
END $$;
CREATE INDEX family_refresh_remainder_core_pages ON migration_family_refresh_core_page(bundle_id,organization_id,id);
CREATE INDEX family_refresh_remainder_history_pages ON migration_family_refresh_history_page(plan_id,organization_id,id);
CREATE INDEX family_refresh_remainder_sources ON migration_family_refresh_source(bundle_id,organization_id,id);

CREATE INDEX family_refresh_remainder_source_reference ON migration_family_refresh_source(bundle_id,organization_id,capture_id,ordinal,kind);

CREATE FUNCTION crm_family_refresh_remainder_bundle_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE previous migration_family_refresh_bundle; source migration_family_refresh_bundle;
BEGIN
 IF NEW.predecessor_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO previous FROM migration_family_refresh_bundle WHERE id=NEW.predecessor_id AND organization_id=NEW.organization_id;
 SELECT * INTO source FROM migration_family_refresh_bundle WHERE id=NEW.remainder_source_bundle_id AND organization_id=NEW.organization_id;
 IF previous.id IS NULL OR source.id IS NULL OR previous.state<>'cancelled' OR source.state<>'cancelled'
  OR NEW.remainder_origin_bundle_id IS DISTINCT FROM COALESCE(previous.remainder_origin_bundle_id,previous.id)
  OR NEW.remainder_origin_bundle_id IS DISTINCT FROM COALESCE(source.remainder_origin_bundle_id,source.id)
  OR ROW(NEW.parent_import_id,NEW.parent_plan_id,NEW.source_account_id,NEW.core_report_id,NEW.core_snapshot_id,NEW.history_capture_id)
    IS DISTINCT FROM ROW(previous.parent_import_id,previous.parent_plan_id,previous.source_account_id,previous.core_report_id,previous.core_snapshot_id,previous.history_capture_id)
  OR ROW(NEW.parent_import_id,NEW.parent_plan_id,NEW.source_account_id,NEW.core_report_id,NEW.core_snapshot_id,NEW.history_capture_id)
    IS DISTINCT FROM ROW(source.parent_import_id,source.parent_plan_id,source.source_account_id,source.core_report_id,source.core_snapshot_id,source.history_capture_id)
 THEN RAISE EXCEPTION 'remainder lineage changed frozen sources'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_remainder_bundle_guard BEFORE INSERT ON migration_family_refresh_bundle FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_remainder_bundle_guard();
REVOKE ALL ON FUNCTION crm_family_refresh_remainder_bundle_guard() FROM PUBLIC;
CREATE INDEX family_refresh_remainder_eligible_tail ON migration_family_refresh_manifest(plan_id,organization_id,position)
 WHERE disposition IN ('insert','update','already_current','correction') AND inherited_result_id IS NULL;
