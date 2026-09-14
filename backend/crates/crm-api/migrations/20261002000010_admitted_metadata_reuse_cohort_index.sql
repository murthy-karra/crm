-- Reuse the identical settled-cohort source index installed by 010f2.
-- Preserve applied migration checksums and remove only the redundant 010f3 copy.
DROP INDEX am_admission_tag_source;
