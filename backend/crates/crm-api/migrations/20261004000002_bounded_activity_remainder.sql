-- An inherited confirmed remainder is assembled in bounded retained worker units.
-- Keep a stable complete source plan even if copying is cancelled and continued.
ALTER TABLE migration_admitted_activity_import
 ADD COLUMN remainder_source_import_id UUID,
 ADD COLUMN remainder_source_plan_id UUID,
 ADD CONSTRAINT admitted_activity_remainder_source_pair CHECK (
   (remainder_source_import_id IS NULL) = (remainder_source_plan_id IS NULL)),
 ADD CONSTRAINT admitted_activity_remainder_source_fk
   FOREIGN KEY(remainder_source_plan_id,remainder_source_import_id,organization_id)
   REFERENCES migration_admitted_activity_plan(id,import_id,organization_id);
