-- Every preview owns its retained source boundary. A replacement can select a
-- different report without changing the AAD or provenance of older evidence.
ALTER TABLE migration_admitted_metadata_plan
 ADD COLUMN snapshot_id UUID,
 ADD COLUMN source_report_id UUID,
 ADD COLUMN source_output_revision UUID,
 ADD COLUMN capture_sequence BIGINT,
 ADD COLUMN previous_plan_id UUID,
 ADD COLUMN preparation_kind TEXT NOT NULL DEFAULT '';
UPDATE migration_admitted_metadata_plan p SET snapshot_id=i.snapshot_id,
 source_report_id=i.source_report_id,source_output_revision=r.output_revision,
 capture_sequence=i.capture_sequence FROM migration_admitted_metadata_import i
 JOIN migration_core_change_report r ON r.id=i.source_report_id AND r.organization_id=i.organization_id
 WHERE i.id=p.import_id AND i.organization_id=p.organization_id;
ALTER TABLE migration_admitted_metadata_plan
 ALTER COLUMN snapshot_id SET NOT NULL, ALTER COLUMN source_report_id SET NOT NULL,
 ALTER COLUMN source_output_revision SET NOT NULL, ALTER COLUMN capture_sequence SET NOT NULL,
 ADD FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 ADD FOREIGN KEY(source_report_id,organization_id) REFERENCES migration_core_change_report(id,organization_id),
 ADD FOREIGN KEY(previous_plan_id,import_id,organization_id) REFERENCES migration_admitted_metadata_plan(id,import_id,organization_id);
ALTER TABLE migration_admitted_metadata_plan DROP CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check;
ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check
 CHECK(preparation_phase IN ('sources','fields','options','tags','choices','cohort','values','seal','complete'));
ALTER TABLE migration_admitted_metadata_receipt ADD COLUMN snapshot_id UUID;
UPDATE migration_admitted_metadata_receipt x SET snapshot_id=i.snapshot_id FROM migration_admitted_metadata_import i WHERE i.id=x.import_id AND i.organization_id=x.organization_id;
ALTER TABLE migration_admitted_metadata_receipt ALTER COLUMN snapshot_id SET NOT NULL,
 ADD FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id);
CREATE UNIQUE INDEX migration_admitted_metadata_selected_target ON migration_admitted_metadata_mapping(plan_id,organization_id,kind,target_id) WHERE kind IN ('field','option') AND target_id IS NOT NULL;
CREATE INDEX migration_admitted_metadata_mapping_prepare ON migration_admitted_metadata_mapping(plan_id,organization_id,kind,id);
