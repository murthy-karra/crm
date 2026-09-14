-- Mobile005: a positive profile-specific revision and additive receipt data.
ALTER TABLE person ADD COLUMN details_revision BIGINT NOT NULL DEFAULT 1 CHECK (details_revision > 0);

CREATE FUNCTION crm_mobile_details_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF ROW(NEW.first_name, NEW.last_name) IS DISTINCT FROM ROW(OLD.first_name, OLD.last_name) THEN
    IF OLD.details_revision = 9223372036854775807 THEN RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision overflow'; END IF;
    NEW.details_revision := OLD.details_revision + 1;
  ELSE
    -- Contact trigger-derived increments arrive as an ordinary parent UPDATE.
    -- Preserve exactly one positive increment, while rejecting resets, skips,
    -- and client-supplied arbitrary values on unrelated Person writes.
    IF (OLD.details_revision = 9223372036854775807 AND NEW.details_revision <> OLD.details_revision)
       OR (OLD.details_revision < 9223372036854775807 AND NEW.details_revision NOT IN (OLD.details_revision, OLD.details_revision + 1)) THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision regression';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER mobile_details_revision BEFORE UPDATE ON person FOR EACH ROW EXECUTE FUNCTION crm_mobile_details_revision();

-- This mirrors the existing broad mobile component derived-write posture:
-- the originating contact mutation still passes the workspace guard; only the
-- trigger-bound parent revision update runs as definer so original/admission
-- review permits do not need a broader arbitrary Person UPDATE capability.
CREATE FUNCTION crm_mobile_contact_details_revision() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE p UUID; o UUID;
BEGIN
  IF TG_OP='UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN RETURN NULL; END IF;
  IF TG_OP<>'INSERT' THEN
    UPDATE public.person SET details_revision=details_revision+1 WHERE id=OLD.person_id AND organization_id=OLD.organization_id AND details_revision<9223372036854775807;
  END IF;
  IF TG_OP='INSERT' OR (TG_OP='UPDATE' AND ROW(NEW.person_id,NEW.organization_id) IS DISTINCT FROM ROW(OLD.person_id,OLD.organization_id)) THEN
    UPDATE public.person SET details_revision=details_revision+1 WHERE id=NEW.person_id AND organization_id=NEW.organization_id AND details_revision<9223372036854775807;
  END IF;
  RETURN NULL;
END $$;
CREATE TRIGGER mobile_contact_details_revision AFTER INSERT OR UPDATE OR DELETE ON contact_method FOR EACH ROW EXECUTE FUNCTION crm_mobile_contact_details_revision();
REVOKE ALL ON FUNCTION crm_mobile_contact_details_revision() FROM PUBLIC;

ALTER TABLE mobile_operation_receipt
  ADD COLUMN added_contact_ids JSONB NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(added_contact_ids)='array'),
  DROP CONSTRAINT mobile_operation_receipt_kind_check,
  DROP CONSTRAINT mobile_operation_receipt_resource_type_check,
  DROP CONSTRAINT mobile_operation_receipt_revision_shape_check,
  ADD CONSTRAINT mobile_operation_receipt_kind_check CHECK (kind IN ('add_note','create_task','complete_task','edit_note','update_task','log_contact_attempt','change_person_stage','update_person_details')),
  ADD CONSTRAINT mobile_operation_receipt_resource_type_check CHECK (resource_type IN ('note','task','contact_attempt','person_stage','person_details')),
  ADD CONSTRAINT mobile_operation_receipt_revision_shape_check CHECK (
    (kind='add_note' AND resource_type='note' AND committed_revision IS NULL)
    OR (kind='edit_note' AND resource_type='note' AND committed_revision IS NOT NULL)
    OR (kind IN ('create_task','complete_task','update_task') AND resource_type='task' AND committed_revision IS NOT NULL)
    OR (kind='log_contact_attempt' AND resource_type='contact_attempt' AND committed_revision IS NULL)
    OR (kind='change_person_stage' AND resource_type='person_stage' AND resource_id=person_id AND committed_revision IS NOT NULL)
    OR (kind='update_person_details' AND resource_type='person_details' AND resource_id=person_id AND committed_revision IS NOT NULL)
  );

-- The latest admitted-refresh permit compares derived Person columns. Keep
-- its exact authorization predicates, allowing only this trigger-owned field.
CREATE OR REPLACE FUNCTION crm_admitted_people_refresh_mutation_allowed(org UUID, permit TEXT, table_name TEXT, operation TEXT, old_row JSONB, new_row JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; target UUID; actor UUID;
BEGIN
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.person_id,r.initiated_by_user_id INTO target,actor FROM migration_admitted_people_refresh_item i JOIN migration_admitted_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id AND r.confirmed_refresh_plan_id=i.plan_id AND i.source_account_id=r.source_account_id JOIN migration_people_admission_result ar ON ar.id=i.admission_result_id AND ar.admission_id=r.admission_id AND ar.organization_id=i.organization_id AND ar.person_id=i.person_id AND ar.disposition='settled' JOIN migration_import_identity mi ON mi.organization_id=i.organization_id AND mi.source_account_id=i.source_account_id AND mi.family='people' AND mi.source_id=i.source_id AND mi.target_id=i.person_id AND mi.admission_id=r.admission_id AND mi.admission_item_id=i.admission_item_id AND mi.admission_result_id=ar.id JOIN migration_admitted_people_refresh_baseline b ON b.organization_id=i.organization_id AND b.admission_id=r.admission_id AND b.source_id=i.source_id AND b.person_id=i.person_id AND b.admission_result_id=ar.id AND b.version=i.baseline_version AND b.result_id IS NOT DISTINCT FROM i.baseline_result_id JOIN migration_workspace w ON w.organization_id=r.organization_id AND w.import_id=r.parent_import_id AND w.plan_id=r.parent_plan_id JOIN organization o ON o.id=r.organization_id AND o.workspace_revision=r.workspace_revision JOIN organization_membership m ON m.organization_id=r.organization_id AND m.user_id=r.initiated_by_user_id WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND r.state='running' AND r.lease_token=lease AND r.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF target IS NULL THEN RETURN false; END IF;
 IF table_name='person' THEN RETURN operation='UPDATE' AND (old_row->>'id')::uuid=target AND (new_row->>'id')::uuid=target AND (old_row->>'organization_id')::uuid=org AND (new_row->>'organization_id')::uuid=org AND (old_row - ARRAY['first_name','last_name','stage_id','assigned_user_id','updated_at','stage_revision','mobile_revision','details_revision'])=(new_row - ARRAY['first_name','last_name','stage_id','assigned_user_id','updated_at','stage_revision','mobile_revision','details_revision']); END IF;
 IF table_name='contact_method' THEN RETURN CASE operation WHEN 'INSERT' THEN (new_row->>'person_id')::uuid=target AND (new_row->>'organization_id')::uuid=org AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side='proposed' AND c.contact_id=(new_row->>'id')::uuid AND c.kind=new_row->>'kind') WHEN 'UPDATE' THEN (old_row->>'id')=(new_row->>'id') AND (old_row->>'person_id')::uuid=target AND (new_row->>'person_id')::uuid=target AND (old_row->>'organization_id')=(new_row->>'organization_id') AND (old_row->>'kind')=(new_row->>'kind') AND (old_row - ARRAY['value','normalized_value','import_order'])=(new_row - ARRAY['value','normalized_value','import_order']) AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side IN ('baseline','current') AND c.contact_id=(old_row->>'id')::uuid AND c.kind=old_row->>'kind') WHEN 'DELETE' THEN (old_row->>'person_id')::uuid=target AND (old_row->>'organization_id')::uuid=org AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side IN ('baseline','current') AND c.contact_id=(old_row->>'id')::uuid AND c.kind=old_row->>'kind') ELSE false END; END IF;
 IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (new_row->>'person_id')::uuid=target AND new_row->>'actor_kind'='system' AND new_row->>'origin'='migration' AND new_row->>'reason'='migration_refresh' AND (new_row->>'on_behalf_of_user_id')::uuid=actor; END IF;
 RETURN false;
END $$;
