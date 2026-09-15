-- Mobile006: aggregate metadata tokens and bounded opt-in catalog snapshots.
-- Tags and custom-field values remain their existing relational CRUD; these
-- triggers maintain only mobile read-model tokens after a real row change.
ALTER TABLE person
  ADD COLUMN metadata_revision BIGINT NOT NULL DEFAULT 1 CHECK (metadata_revision > 0);
CREATE TABLE mobile_metadata_catalog (
  organization_id UUID PRIMARY KEY REFERENCES organization(id),
  revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0)
);
INSERT INTO mobile_metadata_catalog(organization_id)
  SELECT id FROM organization;
GRANT SELECT ON mobile_metadata_catalog TO crm_app;

-- App callers may read the token but never lock or update the derived row.
-- The row lock is needed under repeatable read to reject a catalog change that
-- committed while the advisory admission was contended.
CREATE FUNCTION crm_mobile_metadata_catalog_revision(p_organization_id UUID) RETURNS BIGINT
LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE value BIGINT;
BEGIN
  SELECT revision INTO value FROM public.mobile_metadata_catalog
    WHERE organization_id=p_organization_id FOR SHARE;
  RETURN value;
END $$;
REVOKE ALL ON FUNCTION crm_mobile_metadata_catalog_revision(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_mobile_metadata_catalog_revision(UUID) TO crm_app;

CREATE FUNCTION crm_mobile_create_metadata_catalog() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
  INSERT INTO public.mobile_metadata_catalog(organization_id) VALUES(NEW.id);
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION crm_mobile_create_metadata_catalog() FROM PUBLIC;
CREATE TRIGGER mobile_metadata_catalog_organization
  AFTER INSERT ON organization FOR EACH ROW EXECUTE FUNCTION crm_mobile_create_metadata_catalog();

CREATE FUNCTION crm_mobile_metadata_person_revision() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.metadata_revision IS DISTINCT FROM OLD.metadata_revision THEN
    IF pg_trigger_depth() < 2 OR OLD.metadata_revision = 9223372036854775807
       OR NEW.metadata_revision <> OLD.metadata_revision + 1 THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='metadata revision is derived';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER mobile_metadata_person_revision
  BEFORE UPDATE ON person FOR EACH ROW EXECUTE FUNCTION crm_mobile_metadata_person_revision();

CREATE FUNCTION crm_mobile_advance_metadata_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE p UUID; o UUID;
BEGIN
  IF TG_OP='UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN RETURN NULL; END IF;
  IF TG_OP='INSERT' THEN p:=NEW.person_id; o:=NEW.organization_id;
  ELSE p:=OLD.person_id; o:=OLD.organization_id; END IF;
  UPDATE public.person SET mobile_revision=mobile_revision+1,
    metadata_revision=metadata_revision+1
    WHERE id=p AND organization_id=o AND mobile_revision<9223372036854775807
      AND metadata_revision<9223372036854775807;
  IF NOT FOUND AND EXISTS(SELECT 1 FROM public.person WHERE id=p AND organization_id=o) THEN
    RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='metadata revision overflow';
  END IF;
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION crm_mobile_advance_metadata_revision() FROM PUBLIC;
CREATE TRIGGER mobile_person_tag_metadata_revision
  AFTER INSERT OR UPDATE OR DELETE ON person_tag
  FOR EACH ROW EXECUTE FUNCTION crm_mobile_advance_metadata_revision();
CREATE TRIGGER mobile_person_custom_field_value_metadata_revision
  AFTER INSERT OR UPDATE OR DELETE ON person_custom_field_value
  FOR EACH ROW EXECUTE FUNCTION crm_mobile_advance_metadata_revision();

CREATE FUNCTION crm_mobile_advance_metadata_catalog_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE o UUID; changed BOOLEAN:=false;
BEGIN
  IF TG_OP='INSERT' THEN o:=NEW.organization_id; changed:=true;
  ELSIF TG_OP='DELETE' THEN o:=OLD.organization_id; changed:=true;
  ELSIF TG_TABLE_NAME='tag' THEN
    o:=NEW.organization_id;
    changed:=ROW(NEW.organization_id,NEW.name) IS DISTINCT FROM ROW(OLD.organization_id,OLD.name);
  ELSIF TG_TABLE_NAME='custom_field' THEN
    o:=NEW.organization_id;
    changed:=ROW(NEW.organization_id,NEW.label,NEW.field_type,NEW.position,NEW.archived_at)
      IS DISTINCT FROM ROW(OLD.organization_id,OLD.label,OLD.field_type,OLD.position,OLD.archived_at);
  ELSIF TG_TABLE_NAME='custom_field_option' THEN
    o:=NEW.organization_id;
    changed:=ROW(NEW.organization_id,NEW.field_id,NEW.label,NEW.position,NEW.archived_at)
      IS DISTINCT FROM ROW(OLD.organization_id,OLD.field_id,OLD.label,OLD.position,OLD.archived_at);
  END IF;
  IF changed THEN
    UPDATE public.mobile_metadata_catalog SET revision=revision+1
      WHERE organization_id=o AND revision<9223372036854775807;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='metadata catalog revision overflow'; END IF;
  END IF;
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION crm_mobile_advance_metadata_catalog_revision() FROM PUBLIC;
CREATE TRIGGER mobile_tag_catalog_revision AFTER INSERT OR UPDATE OR DELETE ON tag
  FOR EACH ROW EXECUTE FUNCTION crm_mobile_advance_metadata_catalog_revision();
CREATE TRIGGER mobile_custom_field_catalog_revision AFTER INSERT OR UPDATE OR DELETE ON custom_field
  FOR EACH ROW EXECUTE FUNCTION crm_mobile_advance_metadata_catalog_revision();
CREATE TRIGGER mobile_custom_field_option_catalog_revision AFTER INSERT OR UPDATE OR DELETE ON custom_field_option
  FOR EACH ROW EXECUTE FUNCTION crm_mobile_advance_metadata_catalog_revision();

ALTER TABLE mobile_reconciliation
  ADD COLUMN metadata_catalog_revision BIGINT CHECK (metadata_catalog_revision > 0);
ALTER TABLE mobile_reconciliation_person
  ADD COLUMN metadata_revision BIGINT CHECK (metadata_revision > 0);
CREATE TABLE mobile_reconciliation_metadata_tag (
  generation_id UUID NOT NULL REFERENCES mobile_reconciliation(id) ON DELETE CASCADE,
  tag_id UUID NOT NULL, name TEXT NOT NULL,
  PRIMARY KEY(generation_id,tag_id)
);
CREATE TABLE mobile_reconciliation_metadata_field (
  generation_id UUID NOT NULL REFERENCES mobile_reconciliation(id) ON DELETE CASCADE,
  field_id UUID NOT NULL, label TEXT NOT NULL, field_type TEXT NOT NULL,
  position INTEGER NOT NULL, archived_at TIMESTAMPTZ,
  PRIMARY KEY(generation_id,field_id)
);
CREATE TABLE mobile_reconciliation_metadata_option (
  generation_id UUID NOT NULL REFERENCES mobile_reconciliation(id) ON DELETE CASCADE,
  option_id UUID NOT NULL, field_id UUID NOT NULL, label TEXT NOT NULL,
  position INTEGER NOT NULL, archived_at TIMESTAMPTZ,
  PRIMARY KEY(generation_id,option_id)
);
CREATE INDEX mobile_reconciliation_metadata_option_page
  ON mobile_reconciliation_metadata_option(generation_id,field_id,position,option_id);
GRANT SELECT,INSERT ON mobile_reconciliation_metadata_tag,
  mobile_reconciliation_metadata_field,mobile_reconciliation_metadata_option TO crm_app;

ALTER TABLE mobile_operation_receipt
  DROP CONSTRAINT mobile_operation_receipt_kind_check,
  DROP CONSTRAINT mobile_operation_receipt_resource_type_check,
  DROP CONSTRAINT mobile_operation_receipt_revision_shape_check,
  ADD CONSTRAINT mobile_operation_receipt_kind_check CHECK (kind IN ('add_note','create_task','complete_task','edit_note','update_task','log_contact_attempt','change_person_stage','update_person_details','update_person_metadata')),
  ADD CONSTRAINT mobile_operation_receipt_resource_type_check CHECK (resource_type IN ('note','task','contact_attempt','person_stage','person_details','person_metadata')),
  ADD CONSTRAINT mobile_operation_receipt_revision_shape_check CHECK (
    (kind='add_note' AND resource_type='note' AND committed_revision IS NULL)
    OR (kind='edit_note' AND resource_type='note' AND committed_revision IS NOT NULL)
    OR (kind IN ('create_task','complete_task','update_task') AND resource_type='task' AND committed_revision IS NOT NULL)
    OR (kind='log_contact_attempt' AND resource_type='contact_attempt' AND committed_revision IS NULL)
    OR (kind='change_person_stage' AND resource_type='person_stage' AND resource_id=person_id AND committed_revision IS NOT NULL)
    OR (kind='update_person_details' AND resource_type='person_details' AND resource_id=person_id AND committed_revision IS NOT NULL)
    OR (kind='update_person_metadata' AND resource_type='person_metadata' AND resource_id=person_id AND committed_revision IS NOT NULL)
  );
