-- A successor preserves the exact encrypted comparison data, copying in
-- resumable units and retaining references to committed catalog dependencies.
ALTER TABLE migration_admitted_metadata_plan
 ADD COLUMN remainder_plan_id UUID REFERENCES migration_admitted_metadata_plan(id),
 ADD COLUMN evidence_plan_id UUID REFERENCES migration_admitted_metadata_plan(id);
ALTER TABLE migration_admitted_metadata_mapping
 ADD COLUMN predecessor_mapping_id UUID REFERENCES migration_admitted_metadata_mapping(id),
 ADD COLUMN source_mapping_id UUID REFERENCES migration_admitted_metadata_mapping(id),
 ADD COLUMN dependency_result_id UUID REFERENCES migration_admitted_metadata_result(id),
 ADD COLUMN execute_unit BOOLEAN NOT NULL DEFAULT true;
CREATE UNIQUE INDEX migration_admitted_metadata_mapping_predecessor ON migration_admitted_metadata_mapping(plan_id,organization_id,predecessor_mapping_id) WHERE predecessor_mapping_id IS NOT NULL;
CREATE INDEX migration_admitted_metadata_mapping_execute ON migration_admitted_metadata_mapping(import_id,plan_id,organization_id,kind,id) WHERE execute_unit;
ALTER TABLE migration_admitted_metadata_manifest ADD COLUMN predecessor_manifest_id UUID REFERENCES migration_admitted_metadata_manifest(id);
CREATE UNIQUE INDEX migration_admitted_metadata_manifest_predecessor ON migration_admitted_metadata_manifest(plan_id,organization_id,predecessor_manifest_id) WHERE predecessor_manifest_id IS NOT NULL;
ALTER TABLE migration_admitted_metadata_plan DROP CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check;
ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check CHECK(preparation_phase IN ('sources','fields','options','tags','choices','cohort','values','seal','complete','remainder_mappings','remainder_people','remainder_operations'));

ALTER TABLE migration_admitted_metadata_import ADD COLUMN held_settled_people BIGINT NOT NULL DEFAULT 0 CHECK(held_settled_people>=0);

-- SQL boolean expressions may evaluate either side first. Parse an original
-- claim proof only inside the ready branch, and fail closed on malformed JSON.
-- Preserve the compatible identity-key helper installed by 000003.
DO $migration$ DECLARE definition TEXT; old_guard TEXT; new_guard TEXT; BEGIN
 SELECT pg_get_functiondef('crm_metadata_insert_allowed(uuid,text,jsonb)'::regprocedure) INTO definition;
 definition:=replace(definition,'proof TEXT:=current_setting(''crm.metadata_claim_v1'',true);','proof TEXT:=current_setting(''crm.metadata_claim_v1'',true); parsed_proof JSONB; claims_ready BOOLEAN;');
 old_guard:=$guard$ IF EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=org AND state='ready')
    AND (proof IS NULL OR proof='' OR jsonb_typeof(proof::jsonb)<>'object') THEN RETURN false; END IF;$guard$;
 new_guard:=$guard$ claims_ready:=EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=org AND state='ready');
 IF claims_ready THEN
  IF proof IS NULL OR proof='' THEN RETURN false; END IF;
  BEGIN parsed_proof:=proof::jsonb; EXCEPTION WHEN invalid_text_representation THEN RETURN false; END;
  IF jsonb_typeof(parsed_proof) IS DISTINCT FROM 'object' THEN RETURN false; END IF;
 END IF;$guard$;
 IF strpos(definition,old_guard)=0 THEN RAISE EXCEPTION 'original metadata claim proof guard missing'; END IF;
 definition:=replace(definition,old_guard,new_guard);
 definition:=replace(definition,'AND NOT (proof::jsonb @>','AND NOT (parsed_proof @>');
 EXECUTE definition;
END $migration$;

CREATE INDEX migration_admitted_metadata_mapping_parent ON migration_admitted_metadata_mapping(parent_mapping_id,plan_id,organization_id,id) WHERE parent_mapping_id IS NOT NULL;
CREATE INDEX migration_admitted_metadata_plan_building ON migration_admitted_metadata_plan(import_id,organization_id,id) WHERE state='building';
