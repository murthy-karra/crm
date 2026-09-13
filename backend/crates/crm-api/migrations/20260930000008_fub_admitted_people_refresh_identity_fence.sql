-- The import-identity registry has a natural tenant/source-account key. Store
-- that immutable account on each new executable item; legacy ready plans have
-- NULL and deliberately fail closed until re-previewed.
ALTER TABLE migration_admitted_people_refresh_item
  ADD COLUMN source_account_id BIGINT;

CREATE OR REPLACE FUNCTION crm_admitted_people_refresh_mutation_allowed(org UUID, permit TEXT, table_name TEXT, operation TEXT, old_row JSONB, new_row JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; target UUID; actor UUID;
BEGIN
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'item')::uuid; EXCEPTION WHEN others THEN RETURN false; END;
 SELECT i.person_id,r.initiated_by_user_id INTO target,actor FROM migration_admitted_people_refresh_item i
 JOIN migration_admitted_people_refresh r ON r.id=i.refresh_id AND r.organization_id=i.organization_id AND r.confirmed_refresh_plan_id=i.plan_id AND i.source_account_id=r.source_account_id
 JOIN migration_people_admission_result ar ON ar.id=i.admission_result_id AND ar.admission_id=r.admission_id AND ar.organization_id=i.organization_id AND ar.person_id=i.person_id AND ar.disposition='settled'
 JOIN migration_import_identity mi ON mi.organization_id=i.organization_id AND mi.source_account_id=i.source_account_id AND mi.family='people' AND mi.source_id=i.source_id AND mi.target_id=i.person_id AND mi.admission_id=r.admission_id AND mi.admission_item_id=i.admission_item_id AND mi.admission_result_id=ar.id
 JOIN migration_admitted_people_refresh_baseline b ON b.organization_id=i.organization_id AND b.admission_id=r.admission_id AND b.source_id=i.source_id AND b.person_id=i.person_id AND b.admission_result_id=ar.id AND b.version=i.baseline_version AND b.result_id IS NOT DISTINCT FROM i.baseline_result_id
 JOIN migration_workspace w ON w.organization_id=r.organization_id AND w.import_id=r.parent_import_id AND w.plan_id=r.parent_plan_id
 JOIN organization o ON o.id=r.organization_id AND o.workspace_revision=r.workspace_revision
 JOIN organization_membership m ON m.organization_id=r.organization_id AND m.user_id=r.initiated_by_user_id
 WHERE i.id=unit AND i.organization_id=org AND i.disposition='eligible' AND i.settled_at IS NULL AND r.state='running' AND r.lease_token=lease AND r.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active';
 IF target IS NULL THEN RETURN false; END IF;
 IF table_name='person' THEN RETURN operation='UPDATE' AND (old_row->>'id')::uuid=target AND (new_row->>'id')::uuid=target AND (old_row->>'organization_id')::uuid=org AND (new_row->>'organization_id')::uuid=org AND (old_row - ARRAY['first_name','last_name','stage_id','assigned_user_id','updated_at','stage_revision','mobile_revision'])=(new_row - ARRAY['first_name','last_name','stage_id','assigned_user_id','updated_at','stage_revision','mobile_revision']); END IF;
 IF table_name='contact_method' THEN RETURN CASE operation WHEN 'INSERT' THEN (new_row->>'person_id')::uuid=target AND (new_row->>'organization_id')::uuid=org AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side='proposed' AND c.contact_id=(new_row->>'id')::uuid AND c.kind=new_row->>'kind') WHEN 'UPDATE' THEN (old_row->>'id')=(new_row->>'id') AND (old_row->>'person_id')::uuid=target AND (new_row->>'person_id')::uuid=target AND (old_row->>'organization_id')=(new_row->>'organization_id') AND (old_row->>'kind')=(new_row->>'kind') AND (old_row - ARRAY['value','normalized_value','import_order'])=(new_row - ARRAY['value','normalized_value','import_order']) AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side IN ('baseline','current') AND c.contact_id=(old_row->>'id')::uuid AND c.kind=old_row->>'kind') WHEN 'DELETE' THEN (old_row->>'person_id')::uuid=target AND (old_row->>'organization_id')::uuid=org AND EXISTS(SELECT 1 FROM migration_admitted_people_refresh_contact c WHERE c.item_id=unit AND c.organization_id=org AND c.side IN ('baseline','current') AND c.contact_id=(old_row->>'id')::uuid AND c.kind=old_row->>'kind') ELSE false END; END IF;
 IF table_name IN ('assignment_changed','stage_changed') THEN RETURN operation='INSERT' AND (new_row->>'person_id')::uuid=target AND new_row->>'actor_kind'='system' AND new_row->>'origin'='migration' AND new_row->>'reason'='migration_refresh' AND (new_row->>'on_behalf_of_user_id')::uuid=actor; END IF;
 RETURN false;
END $$;
