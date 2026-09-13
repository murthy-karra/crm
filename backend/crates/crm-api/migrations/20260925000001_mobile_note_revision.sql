-- Mobile 002: notes gain their own optimistic-concurrency token.  The
-- migration baseline is one for every historical row, including tombstones;
-- it intentionally does not attempt to infer historic edit counts.
ALTER TABLE note ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0);

CREATE FUNCTION crm_mobile_note_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  -- Compare every persisted writer-visible field except the token itself.
  -- This covers ordinary edits, import repair/provenance writers and the
  -- tombstone path while leaving a byte-identical UPDATE at the same token.
  IF (to_jsonb(NEW) - 'revision') IS DISTINCT FROM (to_jsonb(OLD) - 'revision') THEN
    NEW.revision := OLD.revision + 1;
  ELSE
    NEW.revision := OLD.revision;
  END IF;
  IF NEW.revision < OLD.revision THEN
    RAISE EXCEPTION 'mobile note revision regression';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER mobile_note_record_revision
BEFORE UPDATE ON note
FOR EACH ROW EXECUTE FUNCTION crm_mobile_note_revision();

-- The v1 receipt wire shape is retained, but new typed edit kinds may store
-- a committed note revision. Existing add-note rows and replay stay NULL.
ALTER TABLE mobile_operation_receipt
  DROP CONSTRAINT mobile_operation_receipt_kind_check,
  ADD CONSTRAINT mobile_operation_receipt_kind_check
    CHECK (kind IN ('add_note', 'create_task', 'complete_task', 'edit_note', 'update_task')),
  ADD CONSTRAINT mobile_operation_receipt_revision_shape_check CHECK (
    (kind = 'add_note' AND resource_type = 'note' AND committed_revision IS NULL)
    OR (kind = 'edit_note' AND resource_type = 'note' AND committed_revision IS NOT NULL)
    OR (kind IN ('create_task', 'complete_task', 'update_task')
        AND resource_type = 'task' AND committed_revision IS NOT NULL)
  );
