-- D-082 review: relational provenance is tenant- and owner-bound even for
-- direct app-role persistence. These fixed-size owner keys do not duplicate or
-- re-encrypt immutable evidence and are excluded from logical payload billing.

ALTER TABLE migration_people_admission ADD CONSTRAINT am_admission_root_key UNIQUE(id,organization_id,parent_import_id,parent_plan_id,source_account_id,confirmed_admission_plan_id);

ALTER TABLE migration_people_admission_result ADD CONSTRAINT am_admission_result_owner_key UNIQUE(id,admission_id,item_id,organization_id,source_id);

ALTER TABLE migration_people_admission_result ADD CONSTRAINT am_admission_result_person_key UNIQUE(id,admission_id,item_id,organization_id,source_id,person_id);

ALTER TABLE migration_snapshot ADD CONSTRAINT am_snapshot_account_key UNIQUE(id,organization_id,source_account_id);

ALTER TABLE migration_core_change_report ADD CONSTRAINT am_report_root_key UNIQUE(id,organization_id,parent_import_id,parent_plan_id,source_account_id,newer_snapshot_id,newer_sequence);

ALTER TABLE migration_core_change_report ADD CONSTRAINT am_report_plan_key UNIQUE(id,organization_id,parent_import_id,parent_plan_id,source_account_id,newer_snapshot_id,output_revision,newer_sequence);

ALTER TABLE migration_metadata_import ADD CONSTRAINT am_original_account_key UNIQUE(id,organization_id,source_account_id);

ALTER TABLE migration_metadata_mapping ADD CONSTRAINT am_original_claim_key UNIQUE(id,plan_id,import_id,organization_id,kind,source_key,target_id);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_cohort_key UNIQUE(id,organization_id,admission_id,parent_import_id,parent_plan_id,source_account_id);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_admission_key UNIQUE(id,organization_id,admission_id);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_account_key UNIQUE(id,organization_id,source_account_id);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_chain_key UNIQUE(id,organization_id,admission_id,admission_plan_id,parent_import_id,parent_plan_id,source_account_id,source_report_id,snapshot_id,capture_sequence);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_successor_key UNIQUE(id,predecessor_import_id,organization_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_owner_key UNIQUE(id,plan_id,import_id,organization_id);

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_owner_key UNIQUE(id,plan_id,import_id,organization_id);

ALTER TABLE migration_admitted_metadata_source ADD CONSTRAINT am_source_owner_key UNIQUE(id,plan_id,import_id,organization_id);

ALTER TABLE migration_admitted_metadata_result ADD CONSTRAINT am_result_owner_key UNIQUE(id,plan_id,import_id,organization_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_person_key UNIQUE(id,plan_id,import_id,organization_id,expected_person_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_predecessor_key UNIQUE(id,plan_id,organization_id,admission_result_id,source_person_id);

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_claim_key UNIQUE(id,plan_id,import_id,organization_id,kind,source_key,target_id);

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_source_key UNIQUE(id,plan_id,organization_id,kind,source_key);

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_kind_key UNIQUE(id,plan_id,import_id,organization_id,kind);

ALTER TABLE migration_admitted_metadata_plan ADD COLUMN admission_id UUID, ADD COLUMN parent_import_id UUID, ADD COLUMN parent_plan_id UUID, ADD COLUMN source_account_id BIGINT;

ALTER TABLE migration_admitted_metadata_source ADD COLUMN snapshot_id UUID;

ALTER TABLE migration_admitted_metadata_observation ADD COLUMN snapshot_id UUID;

ALTER TABLE migration_admitted_metadata_manifest ADD COLUMN admission_id UUID, ADD COLUMN admission_item_id UUID, ADD COLUMN predecessor_plan_id UUID;

ALTER TABLE migration_admitted_metadata_mapping ADD COLUMN predecessor_plan_id UUID, ADD COLUMN evidence_plan_id UUID, ADD COLUMN dependency_plan_id UUID, ADD COLUMN dependency_import_id UUID;

ALTER TABLE migration_admitted_metadata_receipt ADD COLUMN plan_id UUID;

ALTER TABLE migration_admitted_metadata_result ADD COLUMN mapping_id UUID GENERATED ALWAYS AS (CASE WHEN kind<>'people' THEN unit_id END) STORED;

CREATE FUNCTION crm_admitted_metadata_owner_keys() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p RECORD; r RECORD; owner UUID; base UUID; source UUID; inherited UUID;
BEGIN
 IF TG_TABLE_NAME='migration_admitted_metadata_plan' THEN
  SELECT * INTO r FROM migration_admitted_metadata_import WHERE id=NEW.import_id AND organization_id=NEW.organization_id;
  NEW.admission_id:=COALESCE(NEW.admission_id,r.admission_id);
  NEW.parent_import_id:=COALESCE(NEW.parent_import_id,r.parent_import_id);
  NEW.parent_plan_id:=COALESCE(NEW.parent_plan_id,r.parent_plan_id);
  NEW.source_account_id:=COALESCE(NEW.source_account_id,r.source_account_id);
  IF NEW.remainder_plan_id IS NOT NULL OR NEW.evidence_plan_id IS NOT NULL THEN
   IF NEW.remainder_plan_id IS NULL OR NEW.evidence_plan_id IS NULL OR r.predecessor_import_id IS NULL THEN
    RAISE EXCEPTION 'admitted_metadata_plan_lineage' USING ERRCODE='23503';
   END IF;
   -- Both references must be confirmed plans of this actual predecessor chain.
   -- A cancelled partial copy may deliberately refer to an earlier ancestor.
   IF EXISTS(SELECT 1 FROM unnest(ARRAY[NEW.remainder_plan_id,NEW.evidence_plan_id]) wanted
     WHERE NOT EXISTS(WITH RECURSIVE ancestors AS (
       SELECT id,predecessor_import_id,confirmed_plan_id FROM migration_admitted_metadata_import WHERE id=r.predecessor_import_id AND organization_id=NEW.organization_id
       UNION SELECT i.id,i.predecessor_import_id,i.confirmed_plan_id FROM migration_admitted_metadata_import i JOIN ancestors a ON i.id=a.predecessor_import_id WHERE i.organization_id=NEW.organization_id
     ) SELECT 1 FROM ancestors WHERE confirmed_plan_id=wanted)) THEN
    RAISE EXCEPTION 'admitted_metadata_plan_lineage' USING ERRCODE='23503';
   END IF;
  END IF;
 ELSIF TG_TABLE_NAME IN ('migration_admitted_metadata_source','migration_admitted_metadata_observation') THEN
  SELECT snapshot_id INTO owner FROM migration_admitted_metadata_plan WHERE id=NEW.plan_id AND import_id=NEW.import_id AND organization_id=NEW.organization_id;
  NEW.snapshot_id:=COALESCE(NEW.snapshot_id,owner);
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_manifest' THEN
  SELECT admission_id,remainder_plan_id INTO p FROM migration_admitted_metadata_plan WHERE id=NEW.plan_id AND import_id=NEW.import_id AND organization_id=NEW.organization_id;
  NEW.admission_id:=COALESCE(NEW.admission_id,p.admission_id);
  SELECT item_id INTO owner FROM migration_people_admission_result WHERE id=NEW.admission_result_id AND admission_id=NEW.admission_id AND organization_id=NEW.organization_id;
  NEW.admission_item_id:=COALESCE(NEW.admission_item_id,owner);
  IF NEW.predecessor_manifest_id IS NOT NULL THEN NEW.predecessor_plan_id:=COALESCE(NEW.predecessor_plan_id,p.remainder_plan_id); END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_mapping' THEN
  SELECT remainder_plan_id,evidence_plan_id INTO p FROM migration_admitted_metadata_plan WHERE id=NEW.plan_id AND import_id=NEW.import_id AND organization_id=NEW.organization_id;
  IF NEW.predecessor_mapping_id IS NOT NULL THEN NEW.predecessor_plan_id:=COALESCE(NEW.predecessor_plan_id,p.remainder_plan_id); END IF;
  IF NEW.source_mapping_id IS NOT NULL THEN NEW.evidence_plan_id:=COALESCE(NEW.evidence_plan_id,p.evidence_plan_id); END IF;
  IF NEW.dependency_result_id IS NOT NULL THEN
   SELECT plan_id,import_id INTO r FROM migration_admitted_metadata_result WHERE id=NEW.dependency_result_id AND organization_id=NEW.organization_id;
   NEW.dependency_plan_id:=COALESCE(NEW.dependency_plan_id,r.plan_id);
   NEW.dependency_import_id:=COALESCE(NEW.dependency_import_id,r.import_id);
   SELECT COALESCE(m.dependency_result_id,x.id) INTO inherited FROM migration_admitted_metadata_mapping m
    LEFT JOIN migration_admitted_metadata_result x ON x.unit_id=m.id AND x.plan_id=m.plan_id AND x.import_id=m.import_id AND x.organization_id=m.organization_id
    WHERE m.id=NEW.predecessor_mapping_id AND m.plan_id=p.remainder_plan_id AND m.organization_id=NEW.organization_id;
   IF inherited IS DISTINCT FROM NEW.dependency_result_id THEN RAISE EXCEPTION 'admitted_metadata_dependency_owner' USING ERRCODE='23503'; END IF;
  END IF;
 ELSIF TG_TABLE_NAME='migration_admitted_metadata_receipt' THEN
  SELECT id INTO owner FROM migration_admitted_metadata_plan WHERE import_id=NEW.import_id AND organization_id=NEW.organization_id AND snapshot_id=NEW.snapshot_id ORDER BY revision DESC LIMIT 1;
  NEW.plan_id:=COALESCE(NEW.plan_id,owner);
 END IF;
 RETURN NEW;
END $$;

REVOKE ALL ON FUNCTION crm_admitted_metadata_owner_keys() FROM PUBLIC;

CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF import_id,organization_id,admission_id,parent_import_id,parent_plan_id,source_account_id,remainder_plan_id,evidence_plan_id ON migration_admitted_metadata_plan FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys();

CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF plan_id,import_id,organization_id,snapshot_id ON migration_admitted_metadata_source FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys();

CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF plan_id,import_id,organization_id,snapshot_id ON migration_admitted_metadata_observation FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys();

CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF plan_id,import_id,organization_id,admission_id,admission_item_id,admission_result_id,predecessor_manifest_id,predecessor_plan_id ON migration_admitted_metadata_manifest FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys();

CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF plan_id,import_id,organization_id,predecessor_mapping_id,predecessor_plan_id,source_mapping_id,evidence_plan_id,dependency_result_id,dependency_plan_id,dependency_import_id ON migration_admitted_metadata_mapping FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys();

CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF import_id,organization_id,snapshot_id,plan_id ON migration_admitted_metadata_receipt FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys();

UPDATE migration_admitted_metadata_plan SET admission_id=admission_id;

UPDATE migration_admitted_metadata_source SET snapshot_id=snapshot_id;

UPDATE migration_admitted_metadata_observation SET snapshot_id=snapshot_id;

UPDATE migration_admitted_metadata_manifest SET admission_id=admission_id;

UPDATE migration_admitted_metadata_mapping SET predecessor_plan_id=predecessor_plan_id;

UPDATE migration_admitted_metadata_receipt SET plan_id=plan_id;

-- Pre-009 source-changing replans moved the cancellation reservation's snapshot
-- but retained the previous plan key. Repair only that fixed-size child owner;
-- the token, capacity, encrypted receipts and all three ledgers stay unchanged.
UPDATE migration_admitted_metadata_reservation r
 SET plan_id=(SELECT p.id FROM migration_admitted_metadata_plan p
  WHERE p.import_id=r.import_id AND p.organization_id=r.organization_id AND p.snapshot_id=r.snapshot_id
  ORDER BY p.revision DESC LIMIT 1)
 WHERE r.purpose='cancel' AND NOT EXISTS(SELECT 1 FROM migration_admitted_metadata_plan p
  WHERE p.id=r.plan_id AND p.import_id=r.import_id AND p.organization_id=r.organization_id AND p.snapshot_id=r.snapshot_id);

ALTER TABLE migration_admitted_metadata_plan ALTER COLUMN admission_id SET NOT NULL, ALTER COLUMN parent_import_id SET NOT NULL, ALTER COLUMN parent_plan_id SET NOT NULL, ALTER COLUMN source_account_id SET NOT NULL;

ALTER TABLE migration_admitted_metadata_source ALTER COLUMN snapshot_id SET NOT NULL;

ALTER TABLE migration_admitted_metadata_observation ALTER COLUMN snapshot_id SET NOT NULL;

ALTER TABLE migration_admitted_metadata_manifest ALTER COLUMN admission_id SET NOT NULL, ALTER COLUMN admission_item_id SET NOT NULL;

ALTER TABLE migration_admitted_metadata_receipt ALTER COLUMN plan_id SET NOT NULL;

ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT am_plan_source_key UNIQUE(id,import_id,organization_id,snapshot_id);

ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT am_plan_cohort_source_key UNIQUE(id,organization_id,admission_id,snapshot_id,source_report_id,source_output_revision,capture_sequence);

ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT am_plan_remainder_key UNIQUE(id,import_id,organization_id,remainder_plan_id);

ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT am_plan_evidence_key UNIQUE(id,import_id,organization_id,evidence_plan_id);

ALTER TABLE migration_admitted_metadata_result ADD CONSTRAINT am_result_kind_owner_key UNIQUE(id,plan_id,import_id,organization_id,kind);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_admission_fk FOREIGN KEY(admission_id,organization_id,parent_import_id,parent_plan_id,source_account_id,admission_plan_id) REFERENCES migration_people_admission(id,organization_id,parent_import_id,parent_plan_id,source_account_id,confirmed_admission_plan_id);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_report_fk FOREIGN KEY(source_report_id,organization_id,parent_import_id,parent_plan_id,source_account_id,snapshot_id,capture_sequence) REFERENCES migration_core_change_report(id,organization_id,parent_import_id,parent_plan_id,source_account_id,newer_snapshot_id,newer_sequence);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_snapshot_fk FOREIGN KEY(snapshot_id,organization_id,source_account_id) REFERENCES migration_snapshot(id,organization_id,source_account_id);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_predecessor_fk FOREIGN KEY(predecessor_import_id,organization_id,admission_id,admission_plan_id,parent_import_id,parent_plan_id,source_account_id,source_report_id,snapshot_id,capture_sequence) REFERENCES migration_admitted_metadata_import(id,organization_id,admission_id,admission_plan_id,parent_import_id,parent_plan_id,source_account_id,source_report_id,snapshot_id,capture_sequence);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_successor_fk FOREIGN KEY(successor_import_id,id,organization_id) REFERENCES migration_admitted_metadata_import(id,predecessor_import_id,organization_id);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_root_chain_check CHECK(predecessor_import_id IS DISTINCT FROM id AND successor_import_id IS DISTINCT FROM id);

ALTER TABLE migration_admitted_metadata_import ADD CONSTRAINT am_executor_org_fk FOREIGN KEY(organization_id,executor_user_id) REFERENCES organization_membership(organization_id,user_id);

ALTER TABLE migration_metadata_catalog_readiness ADD CONSTRAINT am_readiness_actor_fk FOREIGN KEY(organization_id,activated_by_user_id) REFERENCES organization_membership(organization_id,user_id);

ALTER TABLE migration_admitted_metadata_receipt ADD CONSTRAINT am_receipt_actor_fk FOREIGN KEY(organization_id,actor_user_id) REFERENCES organization_membership(organization_id,user_id);

ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT am_plan_root_fk FOREIGN KEY(import_id,organization_id,admission_id,parent_import_id,parent_plan_id,source_account_id) REFERENCES migration_admitted_metadata_import(id,organization_id,admission_id,parent_import_id,parent_plan_id,source_account_id);

ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT am_plan_report_fk FOREIGN KEY(source_report_id,organization_id,parent_import_id,parent_plan_id,source_account_id,snapshot_id,source_output_revision,capture_sequence) REFERENCES migration_core_change_report(id,organization_id,parent_import_id,parent_plan_id,source_account_id,newer_snapshot_id,output_revision,newer_sequence);

ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT am_plan_remainder_plan_id_fk FOREIGN KEY(remainder_plan_id,organization_id,admission_id,snapshot_id,source_report_id,source_output_revision,capture_sequence) REFERENCES migration_admitted_metadata_plan(id,organization_id,admission_id,snapshot_id,source_report_id,source_output_revision,capture_sequence);

ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT am_plan_evidence_plan_id_fk FOREIGN KEY(evidence_plan_id,organization_id,admission_id,snapshot_id,source_report_id,source_output_revision,capture_sequence) REFERENCES migration_admitted_metadata_plan(id,organization_id,admission_id,snapshot_id,source_report_id,source_output_revision,capture_sequence);

ALTER TABLE migration_admitted_metadata_source ADD CONSTRAINT am_source_snapshot_owner_fk FOREIGN KEY(plan_id,import_id,organization_id,snapshot_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id,snapshot_id);

ALTER TABLE migration_admitted_metadata_observation ADD CONSTRAINT am_observation_snapshot_owner_fk FOREIGN KEY(plan_id,import_id,organization_id,snapshot_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id,snapshot_id);

ALTER TABLE migration_admitted_metadata_reservation ADD CONSTRAINT am_reservation_snapshot_owner_fk FOREIGN KEY(plan_id,import_id,organization_id,snapshot_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id,snapshot_id);

ALTER TABLE migration_admitted_metadata_receipt ADD CONSTRAINT am_receipt_snapshot_owner_fk FOREIGN KEY(plan_id,import_id,organization_id,snapshot_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id,snapshot_id);

ALTER TABLE migration_admitted_metadata_source ADD CONSTRAINT am_source_capture_fk FOREIGN KEY(capture_id,snapshot_id,organization_id) REFERENCES migration_snapshot_capture(id,snapshot_id,organization_id);

ALTER TABLE migration_admitted_metadata_observation ADD CONSTRAINT am_observation_source_fk FOREIGN KEY(source_row_id,plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_source(id,plan_id,import_id,organization_id);

ALTER TABLE migration_admitted_metadata_observation ADD CONSTRAINT am_observation_record_fk FOREIGN KEY(snapshot_record_id,snapshot_id,organization_id) REFERENCES migration_snapshot_record(id,snapshot_id,organization_id);

ALTER TABLE migration_admitted_metadata_alias ADD CONSTRAINT am_alias_source_fk FOREIGN KEY(source_row_id,plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_source(id,plan_id,import_id,organization_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_root_admission_fk FOREIGN KEY(import_id,organization_id,admission_id) REFERENCES migration_admitted_metadata_import(id,organization_id,admission_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_admission_fk FOREIGN KEY(admission_result_id,admission_id,admission_item_id,organization_id,source_person_id) REFERENCES migration_people_admission_result(id,admission_id,item_id,organization_id,source_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_expected_person_fk FOREIGN KEY(admission_result_id,admission_id,admission_item_id,organization_id,source_person_id,expected_person_id) REFERENCES migration_people_admission_result(id,admission_id,item_id,organization_id,source_id,person_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_live_person_check CHECK(person_id IS NULL OR (expected_person_id IS NOT NULL AND person_id=expected_person_id));

ALTER TABLE migration_admitted_metadata_operation ADD CONSTRAINT am_operation_manifest_fk FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_manifest(id,plan_id,import_id,organization_id);

ALTER TABLE migration_admitted_metadata_result ADD CONSTRAINT am_result_manifest_fk FOREIGN KEY(manifest_id,plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_manifest(id,plan_id,import_id,organization_id);

ALTER TABLE migration_admitted_metadata_result ADD CONSTRAINT am_result_person_fk FOREIGN KEY(manifest_id,plan_id,import_id,organization_id,person_id) REFERENCES migration_admitted_metadata_manifest(id,plan_id,import_id,organization_id,expected_person_id);

ALTER TABLE migration_admitted_metadata_result ADD CONSTRAINT am_result_mapping_fk FOREIGN KEY(mapping_id,plan_id,import_id,organization_id,kind) REFERENCES migration_admitted_metadata_mapping(id,plan_id,import_id,organization_id,kind);

ALTER TABLE migration_admitted_metadata_result ADD CONSTRAINT am_result_unit_check CHECK((kind='people' AND manifest_id IS NOT NULL AND unit_id=manifest_id) OR (kind<>'people' AND manifest_id IS NULL AND person_id IS NULL));

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_predecessor_check CHECK((predecessor_mapping_id IS NULL)=(predecessor_plan_id IS NULL));

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_base_plan_fk FOREIGN KEY(plan_id,import_id,organization_id,predecessor_plan_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id,remainder_plan_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_predecessor_check CHECK((predecessor_manifest_id IS NULL)=(predecessor_plan_id IS NULL));

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_base_plan_fk FOREIGN KEY(plan_id,import_id,organization_id,predecessor_plan_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id,remainder_plan_id);

ALTER TABLE migration_admitted_metadata_manifest ADD CONSTRAINT am_manifest_predecessor_fk FOREIGN KEY(predecessor_manifest_id,predecessor_plan_id,organization_id,admission_result_id,source_person_id) REFERENCES migration_admitted_metadata_manifest(id,plan_id,organization_id,admission_result_id,source_person_id);

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_predecessor_fk FOREIGN KEY(predecessor_mapping_id,predecessor_plan_id,organization_id,kind,source_key) REFERENCES migration_admitted_metadata_mapping(id,plan_id,organization_id,kind,source_key);

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_evidence_check CHECK((source_mapping_id IS NULL)=(evidence_plan_id IS NULL)), ADD CONSTRAINT am_mapping_dependency_check CHECK((dependency_result_id IS NULL AND dependency_plan_id IS NULL AND dependency_import_id IS NULL) OR (dependency_result_id IS NOT NULL AND dependency_plan_id IS NOT NULL AND dependency_import_id IS NOT NULL));

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_evidence_plan_fk FOREIGN KEY(plan_id,import_id,organization_id,evidence_plan_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id,evidence_plan_id);

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_source_fk FOREIGN KEY(source_mapping_id,evidence_plan_id,organization_id,kind,source_key) REFERENCES migration_admitted_metadata_mapping(id,plan_id,organization_id,kind,source_key);

ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT am_mapping_dependency_fk FOREIGN KEY(dependency_result_id,dependency_plan_id,dependency_import_id,organization_id,kind) REFERENCES migration_admitted_metadata_result(id,plan_id,import_id,organization_id,kind);

ALTER TABLE migration_metadata_catalog_claim ADD CONSTRAINT am_claim_owner_check CHECK((num_nonnulls(original_import_id,original_plan_id,original_mapping_id)=3 AND num_nonnulls(admitted_import_id,admitted_plan_id,admitted_mapping_id)=0) OR (num_nonnulls(original_import_id,original_plan_id,original_mapping_id)=0 AND num_nonnulls(admitted_import_id,admitted_plan_id,admitted_mapping_id)=3));

ALTER TABLE migration_metadata_catalog_claim ADD CONSTRAINT am_claim_original_mapping_fk FOREIGN KEY(original_mapping_id,original_plan_id,original_import_id,organization_id,kind,source_key,target_id) REFERENCES migration_metadata_mapping(id,plan_id,import_id,organization_id,kind,source_key,target_id);

ALTER TABLE migration_metadata_catalog_claim ADD CONSTRAINT am_claim_original_account_fk FOREIGN KEY(original_import_id,organization_id,source_account_id) REFERENCES migration_metadata_import(id,organization_id,source_account_id);

ALTER TABLE migration_metadata_catalog_claim ADD CONSTRAINT am_claim_admitted_mapping_fk FOREIGN KEY(admitted_mapping_id,admitted_plan_id,admitted_import_id,organization_id,kind,source_key,target_id) REFERENCES migration_admitted_metadata_mapping(id,plan_id,import_id,organization_id,kind,source_key,target_id);

ALTER TABLE migration_metadata_catalog_claim ADD CONSTRAINT am_claim_admitted_account_fk FOREIGN KEY(admitted_import_id,organization_id,source_account_id) REFERENCES migration_admitted_metadata_import(id,organization_id,source_account_id);

-- One exact admission source probe per tag-bearing record, including held native targets.
CREATE INDEX am_admission_tag_source ON migration_people_admission_result(admission_id,organization_id,source_id) WHERE disposition='settled';
