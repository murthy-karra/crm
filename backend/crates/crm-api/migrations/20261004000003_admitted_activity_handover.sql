-- Existing shared barrier is also the old-binary compatibility boundary.
-- Owners retain maintenance access; crm_app must declare current reader support.
CREATE OR REPLACE FUNCTION crm_workspace_shared(org UUID) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF NOT pg_try_advisory_xact_lock_shared(hashtextextended('crm-workspace-v1:'||org::text,0)) THEN RAISE EXCEPTION USING ERRCODE='55P03',MESSAGE='workspace_busy'; END IF;
 IF current_user='crm_app' AND EXISTS(SELECT 1 FROM migration_admitted_activity_import WHERE organization_id=org AND confirmed_plan_id IS NOT NULL)
    AND current_setting('crm.admitted_activity_reader',true) IS DISTINCT FROM 'fub-admitted-activity-v1'
 THEN RAISE EXCEPTION USING ERRCODE='P010F',MESSAGE='admitted_activity_handover_required'; END IF;
END $$;
