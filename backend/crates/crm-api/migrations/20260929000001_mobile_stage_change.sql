-- Mobile004: stage-specific mobile concurrency and the bounded catalogue
-- snapshot.  The two revisions are deliberately independent from the broad
-- person mobile revision: notes/tasks still invalidate a downloaded Person
-- bundle, but cannot reject an otherwise current stage proposal.
ALTER TABLE person
  ADD COLUMN stage_revision BIGINT NOT NULL DEFAULT 1 CHECK (stage_revision > 0);

ALTER TABLE organization
  ADD COLUMN stage_catalog_revision BIGINT NOT NULL DEFAULT 1
  CHECK (stage_catalog_revision > 0);

CREATE FUNCTION crm_mobile_stage_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.stage_id IS DISTINCT FROM OLD.stage_id THEN
    IF OLD.stage_revision = 9223372036854775807 THEN
      RAISE EXCEPTION USING ERRCODE = '22003', MESSAGE = 'stage revision overflow';
    END IF;
    NEW.stage_revision := OLD.stage_revision + 1;
  ELSE
    -- A client cannot reset, advance, or otherwise forge this derived value.
    NEW.stage_revision := OLD.stage_revision;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER mobile_stage_revision
  BEFORE UPDATE ON person FOR EACH ROW EXECUTE FUNCTION crm_mobile_stage_revision();

-- This is intentionally the only path which can advance the Organization
-- catalogue revision.  The function is SECURITY DEFINER because crm_app has
-- no UPDATE privilege on organization; its trigger-only shape and the exact
-- OLD + 1 update retain the migration-review Organization guard boundary.
CREATE FUNCTION crm_mobile_advance_stage_catalog_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
  org UUID;
  changed BOOLEAN := false;
BEGIN
  IF TG_TABLE_NAME <> 'stage' THEN
    RAISE EXCEPTION USING ERRCODE = '42501', MESSAGE = 'stage catalog trigger required';
  END IF;
  IF TG_OP = 'INSERT' THEN
    org := NEW.organization_id;
    changed := true;
  ELSIF TG_OP = 'DELETE' THEN
    org := OLD.organization_id;
    changed := true;
  ELSIF ROW(NEW.organization_id, NEW.name, NEW.position)
        IS DISTINCT FROM ROW(OLD.organization_id, OLD.name, OLD.position) THEN
    -- Stage ownership itself is immutable in normal application paths, but
    -- treating a schema-level move as a change preserves both catalogues.
    IF NEW.organization_id <> OLD.organization_id THEN
      UPDATE organization
        SET stage_catalog_revision = stage_catalog_revision + 1
        WHERE id = OLD.organization_id
          AND stage_catalog_revision < 9223372036854775807;
      IF NOT FOUND THEN
        RAISE EXCEPTION USING ERRCODE = '22003', MESSAGE = 'stage catalog revision overflow';
      END IF;
    END IF;
    org := NEW.organization_id;
    changed := true;
  END IF;
  IF changed THEN
    UPDATE organization
      SET stage_catalog_revision = stage_catalog_revision + 1
      WHERE id = org
        AND stage_catalog_revision < 9223372036854775807;
    IF NOT FOUND THEN
      RAISE EXCEPTION USING ERRCODE = '22003', MESSAGE = 'stage catalog revision overflow';
    END IF;
  END IF;
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION crm_mobile_advance_stage_catalog_revision() FROM PUBLIC;
CREATE TRIGGER mobile_stage_catalog_revision
  AFTER INSERT OR UPDATE OR DELETE ON stage
  FOR EACH ROW EXECUTE FUNCTION crm_mobile_advance_stage_catalog_revision();

ALTER TABLE mobile_operation_receipt
  DROP CONSTRAINT mobile_operation_receipt_kind_check,
  DROP CONSTRAINT mobile_operation_receipt_resource_type_check,
  DROP CONSTRAINT mobile_operation_receipt_revision_shape_check,
  ADD CONSTRAINT mobile_operation_receipt_kind_check CHECK (kind IN (
    'add_note', 'create_task', 'complete_task', 'edit_note', 'update_task',
    'log_contact_attempt', 'change_person_stage'
  )),
  ADD CONSTRAINT mobile_operation_receipt_resource_type_check CHECK (
    resource_type IN ('note', 'task', 'contact_attempt', 'person_stage')
  ),
  ADD CONSTRAINT mobile_operation_receipt_revision_shape_check CHECK (
    (kind = 'add_note' AND resource_type = 'note' AND committed_revision IS NULL)
    OR (kind = 'edit_note' AND resource_type = 'note' AND committed_revision IS NOT NULL)
    OR (kind IN ('create_task', 'complete_task', 'update_task')
        AND resource_type = 'task' AND committed_revision IS NOT NULL)
    OR (kind = 'log_contact_attempt'
        AND resource_type = 'contact_attempt' AND committed_revision IS NULL)
    OR (kind = 'change_person_stage'
        AND resource_type = 'person_stage' AND resource_id = person_id
        AND committed_revision IS NOT NULL)
  );

ALTER TABLE mobile_reconciliation
  ADD COLUMN stage_catalog_revision BIGINT CHECK (stage_catalog_revision > 0);
CREATE TABLE mobile_reconciliation_stage (
  generation_id UUID NOT NULL REFERENCES mobile_reconciliation(id) ON DELETE CASCADE,
  stage_id UUID NOT NULL,
  name TEXT NOT NULL,
  position SMALLINT NOT NULL,
  PRIMARY KEY (generation_id, stage_id),
  UNIQUE (generation_id, position, stage_id)
);
CREATE INDEX mobile_reconciliation_stage_page
  ON mobile_reconciliation_stage(generation_id, position, stage_id);
GRANT SELECT, INSERT ON mobile_reconciliation_stage TO crm_app;
