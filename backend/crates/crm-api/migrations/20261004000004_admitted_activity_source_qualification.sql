-- F4-I1/I2: the selected People stream shares the bounded retained index.
ALTER TABLE migration_admitted_activity_source
 DROP CONSTRAINT migration_admitted_activity_source_family_check,
 ADD CONSTRAINT migration_admitted_activity_source_family_check
 CHECK (family IN ('people','notes','tasks','users')),
 ADD COLUMN source_person_id TEXT CHECK(length(source_person_id)<=128);
ALTER TABLE migration_admitted_activity_manifest
 DROP CONSTRAINT migration_admitted_activity_manifest_disposition_check,
 ADD CONSTRAINT migration_admitted_activity_manifest_disposition_check
 CHECK (disposition IN ('eligible','already_present','held','excluded'));
