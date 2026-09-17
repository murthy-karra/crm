-- Mapping labels must use the first retained capture/item/element occurrence.
-- Keep existing drafts on their old cursor semantics: changing an in-progress
-- UUID walk to capture order could silently skip unvisited source records.
ALTER TABLE migration_family_refresh_bundle ADD COLUMN mapping_capture_order BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE migration_family_refresh_bundle ALTER COLUMN mapping_capture_order SET DEFAULT true;
CREATE INDEX family_refresh_metadata_mapping_capture ON migration_family_refresh_source
 (bundle_id,organization_id,capture_sequence,ordinal,id) WHERE kind IN ('person','field');
CREATE INDEX family_refresh_activity_mapping_capture ON migration_family_refresh_source
 (bundle_id,organization_id,capture_sequence,ordinal,id) WHERE kind IN ('note','task');
CREATE OR REPLACE FUNCTION crm_family_refresh_next_mapping_source(org UUID,bundle UUID,family TEXT,after_id UUID)
 RETURNS UUID LANGUAGE plpgsql STABLE AS $$
DECLARE capture_order BOOLEAN; next_id UUID;
BEGIN
 SELECT b.mapping_capture_order INTO capture_order FROM migration_family_refresh_bundle b WHERE b.id=bundle AND b.organization_id=org;
 IF capture_order IS NULL THEN RETURN NULL; END IF;
 IF NOT capture_order THEN
  SELECT s.id INTO next_id FROM migration_family_refresh_source s WHERE s.organization_id=org AND s.bundle_id=bundle
   AND ((family='metadata' AND s.kind IN ('person','field')) OR (family='activity' AND s.kind IN ('note','task')))
   AND (after_id IS NULL OR s.id>after_id) ORDER BY s.id LIMIT 1;
 ELSIF family='metadata' THEN
  SELECT s.id INTO next_id FROM migration_family_refresh_source s WHERE s.organization_id=org AND s.bundle_id=bundle
   AND s.kind IN ('person','field') AND (after_id IS NULL OR (s.capture_sequence,s.ordinal,s.id)>
    (SELECT a.capture_sequence,a.ordinal,a.id FROM migration_family_refresh_source a WHERE a.id=after_id AND a.organization_id=org AND a.bundle_id=bundle))
   ORDER BY s.capture_sequence,s.ordinal,s.id LIMIT 1;
 ELSIF family='activity' THEN
  SELECT s.id INTO next_id FROM migration_family_refresh_source s WHERE s.organization_id=org AND s.bundle_id=bundle
   AND s.kind IN ('note','task') AND (after_id IS NULL OR (s.capture_sequence,s.ordinal,s.id)>
    (SELECT a.capture_sequence,a.ordinal,a.id FROM migration_family_refresh_source a WHERE a.id=after_id AND a.organization_id=org AND a.bundle_id=bundle))
   ORDER BY s.capture_sequence,s.ordinal,s.id LIMIT 1;
 END IF;
 RETURN next_id;
END $$;
-- The fixed-width flag is immutable under the existing bundle guard. Old
-- bundles must be freshly prepared before tag catalog choices can qualify.
