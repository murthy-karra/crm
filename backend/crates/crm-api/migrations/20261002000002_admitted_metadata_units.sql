-- Each admitted unit has one durable result/checkpoint. Operation outcomes are
-- retained inside that immutable encrypted result, not inferred from native rows.
ALTER TABLE migration_admitted_metadata_result ADD COLUMN unit_id UUID;
CREATE UNIQUE INDEX migration_admitted_metadata_result_unit ON migration_admitted_metadata_result(import_id,organization_id,unit_id);
ALTER TABLE migration_admitted_metadata_import ADD COLUMN checkpoint_id UUID;
ALTER TABLE migration_admitted_metadata_import ADD COLUMN pause_reason TEXT;
GRANT UPDATE ON migration_admitted_metadata_mapping,migration_admitted_metadata_operation TO crm_app;
GRANT SELECT ON migration_metadata_identity TO crm_app;
CREATE INDEX migration_admitted_metadata_mapping_work ON migration_admitted_metadata_mapping(import_id,plan_id,organization_id,kind,id);
CREATE INDEX migration_admitted_metadata_manifest_work ON migration_admitted_metadata_manifest(import_id,plan_id,organization_id,id);
CREATE INDEX migration_admitted_metadata_operation_unit ON migration_admitted_metadata_operation(manifest_id,organization_id,id);

CREATE OR REPLACE FUNCTION crm_admitted_metadata_insert_allowed(org UUID, permit TEXT, row_value JSONB) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE lease UUID; unit UUID; root UUID; plan UUID; account BIGINT; target UUID; claim_kind TEXT; claim_key TEXT; native_table TEXT;
BEGIN
 IF permit IS NULL OR permit='' THEN RETURN false; END IF;
 BEGIN lease:=(permit::jsonb->>'lease')::uuid; unit:=(permit::jsonb->>'unit_id')::uuid; root:=(permit::jsonb->>'root_id')::uuid; plan:=(permit::jsonb->>'plan_id')::uuid; target:=(permit::jsonb->>'target_id')::uuid; claim_kind:=permit::jsonb->>'claim_kind'; claim_key:=permit::jsonb->>'claim_key'; native_table:=permit::jsonb->>'native_table'; EXCEPTION WHEN others THEN RETURN false; END;
 IF native_table NOT IN ('tag','custom_field','custom_field_option','person_tag','person_custom_field_value') THEN RETURN false; END IF;
 SELECT i.source_account_id INTO account FROM migration_admitted_metadata_import i
 JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id
 JOIN migration_import p ON p.id=i.parent_import_id AND p.organization_id=org AND p.confirmed_plan_id=i.parent_plan_id AND p.state='completed'
 JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=org AND a.parent_import_id=p.id AND a.parent_plan_id=p.confirmed_plan_id AND a.state IN ('completed','cancelled')
 JOIN organization o ON o.id=org AND o.workspace_revision=i.workspace_revision AND o.workspace_mode='migration_review'
 JOIN organization_membership m ON m.organization_id=org AND m.user_id=i.executor_user_id AND m.role='admin' AND m.status='active'
 JOIN migration_metadata_catalog_readiness r ON r.organization_id=org AND r.state='ready'
 WHERE i.id=root AND i.organization_id=org AND i.confirmed_plan_id=plan AND i.state='running' AND i.lease_token=lease AND i.lease_expires_at>clock_timestamp() AND i.engine_version='fub-admitted-metadata-v1';
 IF account IS NULL OR claim_key IS NULL OR claim_kind IS NULL THEN RETURN false; END IF;
 IF native_table IN ('tag','custom_field','custom_field_option') THEN
 RETURN EXISTS(SELECT 1 FROM migration_admitted_metadata_mapping x JOIN migration_metadata_catalog_claim c ON c.organization_id=org AND c.source_account_id=account AND c.kind=x.kind AND c.source_key=x.source_key AND c.target_id=x.target_id WHERE x.id=unit AND x.import_id=root AND x.plan_id=plan AND x.organization_id=org AND x.kind=claim_kind AND x.target_id=target AND encode(x.source_key,'hex')=claim_key AND x.disposition='create_matching' AND (row_value->>'id')::uuid=target AND ((native_table='tag' AND x.kind='tag') OR(native_table='custom_field' AND x.kind='field') OR(native_table='custom_field_option' AND x.kind='option' AND x.target_field_id=(row_value->>'field_id')::uuid)));
 END IF;
 RETURN EXISTS(SELECT 1 FROM migration_admitted_metadata_manifest m
 JOIN migration_admitted_metadata_operation x ON x.manifest_id=m.id AND x.import_id=root AND x.plan_id=plan AND x.organization_id=org
 JOIN migration_admitted_metadata_mapping a ON a.id=x.mapping_id AND a.plan_id=plan AND a.organization_id=org AND a.kind=claim_kind AND encode(a.source_key,'hex')=claim_key
 JOIN migration_metadata_catalog_claim c ON c.organization_id=org AND c.source_account_id=account AND c.kind=a.kind AND c.source_key=a.source_key AND c.target_id=x.target_id
 JOIN migration_people_admission_result ar ON ar.id=m.admission_result_id AND ar.organization_id=org AND ar.person_id=m.person_id AND ar.disposition='settled'
 JOIN migration_import_identity i ON i.organization_id=org AND i.source_account_id=account AND i.family='people' AND i.source_id=m.source_person_id AND i.target_id=m.person_id AND i.admission_result_id=ar.id AND i.admission_id=ar.admission_id AND i.admission_item_id=ar.item_id
 WHERE m.id=unit AND m.import_id=root AND m.plan_id=plan AND m.organization_id=org AND m.disposition='eligible' AND x.target_id=target AND x.disposition='eligible' AND (row_value->>'person_id')::uuid=m.person_id AND ((native_table='person_tag' AND x.kind='tag_link' AND a.kind='tag' AND (row_value->>'tag_id')::uuid=target) OR(native_table='person_custom_field_value' AND x.kind='value' AND a.kind='field' AND (row_value->>'field_id')::uuid=target)));
END $$;
-- Bind the invocation's table, not just the caller-supplied proof. This preserves
-- the established guard and every independently scoped older permit.
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_workspace_mutation_guard()'::regprocedure) INTO definition;
 definition:=replace(definition, 'IF TG_OP=''INSERT'' AND crm_admitted_metadata_insert_allowed(org,token,row_value) THEN', 'IF TG_OP=''INSERT'' AND TG_TABLE_NAME IN (''tag'',''custom_field'',''custom_field_option'',''person_tag'',''person_custom_field_value'') AND token IS NOT NULL AND token<>'''' AND token::jsonb->>''native_table''=TG_TABLE_NAME AND crm_admitted_metadata_insert_allowed(org,token,row_value) THEN');
 EXECUTE definition;
END $$;
