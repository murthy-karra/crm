-- Mobile 003: durable receipts for the additive manual contact operation.
-- Facts and their append-only/derived-activity registration already belong to
-- the existing contact_attempted table; this migration owns only the mobile
-- receipt vocabulary and cross-column shape.
ALTER TABLE mobile_operation_receipt
  DROP CONSTRAINT mobile_operation_receipt_kind_check,
  DROP CONSTRAINT mobile_operation_receipt_resource_type_check,
  DROP CONSTRAINT mobile_operation_receipt_revision_shape_check,
  ADD CONSTRAINT mobile_operation_receipt_kind_check
    CHECK (kind IN (
      'add_note', 'create_task', 'complete_task', 'edit_note', 'update_task',
      'log_contact_attempt'
    )),
  ADD CONSTRAINT mobile_operation_receipt_resource_type_check
    CHECK (resource_type IN ('note', 'task', 'contact_attempt')),
  ADD CONSTRAINT mobile_operation_receipt_revision_shape_check CHECK (
    (kind = 'add_note' AND resource_type = 'note' AND committed_revision IS NULL)
    OR (kind = 'edit_note' AND resource_type = 'note' AND committed_revision IS NOT NULL)
    OR (kind IN ('create_task', 'complete_task', 'update_task')
        AND resource_type = 'task' AND committed_revision IS NOT NULL)
    OR (kind = 'log_contact_attempt'
        AND resource_type = 'contact_attempt' AND committed_revision IS NULL)
  );
