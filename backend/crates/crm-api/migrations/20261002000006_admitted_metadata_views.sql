-- Fixed-size ownership and cached counters keep polling independent of cohort size.
ALTER TABLE migration_admitted_metadata_import
 ADD COLUMN admission_plan_id UUID,
 ADD COLUMN settled_people BIGINT NOT NULL DEFAULT 0 CHECK(settled_people>=0),
 ADD COLUMN counts JSONB,
 ADD COLUMN settled_eligible_people BIGINT NOT NULL DEFAULT 0 CHECK(settled_eligible_people>=0),
 ADD COLUMN confirmed_at TIMESTAMPTZ,
 ADD COLUMN completed_at TIMESTAMPTZ;
UPDATE migration_admitted_metadata_import i SET admission_plan_id=a.confirmed_admission_plan_id,
 settled_people=(SELECT count(*) FROM migration_people_admission_result r WHERE r.admission_id=i.admission_id AND r.organization_id=i.organization_id AND r.disposition='settled')
 FROM migration_people_admission a WHERE a.id=i.admission_id AND a.organization_id=i.organization_id;
ALTER TABLE migration_admitted_metadata_import ALTER COLUMN admission_plan_id SET NOT NULL,
 ADD FOREIGN KEY(admission_plan_id,admission_id,organization_id) REFERENCES migration_people_admission_plan(id,admission_id,organization_id);
-- Alias rows reference the original retained spelling and ordinal without
-- copying source contents or transferring source ownership.
CREATE TABLE migration_admitted_metadata_alias (
 id UUID PRIMARY KEY, import_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 mapping_id UUID NOT NULL, source_row_id UUID NOT NULL, ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 UNIQUE(mapping_id,source_row_id,ordinal),
 FOREIGN KEY(plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id),
 FOREIGN KEY(mapping_id,plan_id,organization_id) REFERENCES migration_admitted_metadata_mapping(id,plan_id,organization_id),
 FOREIGN KEY(source_row_id) REFERENCES migration_admitted_metadata_source(id)
);
CREATE INDEX migration_admitted_metadata_alias_page ON migration_admitted_metadata_alias(plan_id,organization_id,mapping_id,id);
CREATE INDEX migration_admitted_metadata_result_page ON migration_admitted_metadata_result(import_id,organization_id,id);
CREATE INDEX migration_admitted_metadata_person_result_page ON migration_admitted_metadata_result(person_id,organization_id,id) WHERE person_id IS NOT NULL;
GRANT SELECT,INSERT ON migration_admitted_metadata_alias TO crm_app;
ALTER TABLE migration_admitted_metadata_mapping
 ADD COLUMN dependent_count BIGINT NOT NULL DEFAULT 0 CHECK(dependent_count>=0),
 ADD COLUMN alias_count BIGINT NOT NULL DEFAULT 0 CHECK(alias_count>=0);
CREATE FUNCTION crm_admitted_metadata_mapping_counts() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM migration_admitted_metadata_plan WHERE id=NEW.plan_id AND organization_id=NEW.organization_id AND state='building') THEN
  IF TG_TABLE_NAME='migration_admitted_metadata_alias' THEN
   UPDATE migration_admitted_metadata_mapping SET alias_count=alias_count+1 WHERE id=NEW.mapping_id AND plan_id=NEW.plan_id AND organization_id=NEW.organization_id;
  ELSE
   UPDATE migration_admitted_metadata_mapping SET dependent_count=dependent_count+1 WHERE id=NEW.mapping_id AND plan_id=NEW.plan_id AND organization_id=NEW.organization_id;
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER admitted_metadata_mapping_counts AFTER INSERT ON migration_admitted_metadata_alias FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_mapping_counts();
CREATE TRIGGER admitted_metadata_mapping_counts AFTER INSERT ON migration_admitted_metadata_operation FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_mapping_counts();
REVOKE ALL ON FUNCTION crm_admitted_metadata_mapping_counts() FROM PUBLIC;
CREATE INDEX migration_admitted_metadata_mapping_page ON migration_admitted_metadata_mapping(plan_id,organization_id,id);
CREATE INDEX migration_admitted_metadata_manifest_page ON migration_admitted_metadata_manifest(plan_id,organization_id,id);
CREATE INDEX migration_admitted_metadata_operation_read ON migration_admitted_metadata_operation(manifest_id,organization_id,id);
CREATE INDEX migration_admitted_metadata_manifest_disposition_page ON migration_admitted_metadata_manifest(plan_id,organization_id,disposition,id);
CREATE INDEX migration_admitted_metadata_result_kind_page ON migration_admitted_metadata_result(import_id,organization_id,kind,id);
CREATE INDEX migration_admitted_metadata_result_disposition_page ON migration_admitted_metadata_result(import_id,organization_id,disposition,id);
CREATE INDEX migration_admitted_metadata_result_filtered_page ON migration_admitted_metadata_result(import_id,organization_id,kind,disposition,id);

-- A cancelled unconfirmed attempt remains readable with immutable receipts;
-- it no longer occupies the sole active root for its admission. Confirmed roots
-- retain uniqueness forever and can only continue through an exact successor.
DROP INDEX migration_admitted_metadata_one_root;
CREATE UNIQUE INDEX migration_admitted_metadata_one_root ON migration_admitted_metadata_import(organization_id,admission_id) WHERE predecessor_import_id IS NULL AND (state!='cancelled' OR confirmed_plan_id IS NOT NULL);

CREATE INDEX migration_admitted_metadata_root_seek ON migration_admitted_metadata_import(organization_id,id);
CREATE INDEX migration_admitted_metadata_cohort_seek ON migration_admitted_metadata_import(organization_id,admission_id,id);

GRANT UPDATE(count) ON migration_admitted_metadata_issue TO crm_app;
