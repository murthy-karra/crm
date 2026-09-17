-- A cancelled fixed payer may finish only shared cohort/source preparation
-- required by surviving siblings. Its own native family work stays cancelled.
ALTER TABLE migration_family_refresh_plan ADD COLUMN cancel_requested BOOLEAN NOT NULL DEFAULT false;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('''pause_reason''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family plan guard'; END IF;
 EXECUTE replace(definition,'''pause_reason''','''pause_reason'',''cancel_requested''');
 SELECT pg_get_functiondef('crm_family_refresh_bundle_guard()'::regprocedure) INTO definition;
 IF position('''updated_at''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family bundle guard'; END IF;
 definition:=replace(definition,'''updated_at''','''updated_at'',''executor_user_id''');
 definition:=replace(definition,'IF NEW.revision<OLD.revision',
 'IF NEW.executor_user_id IS DISTINCT FROM OLD.executor_user_id AND (NEW.executor_user_id::text IS DISTINCT FROM current_setting(''crm.family_refresh_adopt_executor'',true) OR NEW.revision<>OLD.revision+1 OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=NEW.organization_id AND user_id=NEW.executor_user_id AND role=''admin'' AND status=''active'')) THEN RAISE EXCEPTION ''invalid family executor adoption''; END IF; IF NEW.revision<OLD.revision');
 EXECUTE definition;
END $$;
-- Another current administrator can cancel after executor revocation without
-- adopting execution. Settlement remains control-only and receipt-bound.
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_settle(uuid,uuid,uuid,uuid,bigint,boolean)'::regprocedure) INTO definition;
 IF position('THEN RAISE EXCEPTION ''family settlement executor unavailable''; END IF;' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family settlement'; END IF;
 EXECUTE replace(definition,'THEN RAISE EXCEPTION ''family settlement executor unavailable''; END IF;',
 'AND NOT (r.purpose=''control'' AND NOT release_control AND EXISTS(SELECT 1 FROM migration_family_refresh_receipt c JOIN organization_membership m ON m.organization_id=c.organization_id AND m.user_id=c.actor_user_id WHERE c.organization_id=org AND c.bundle_id=bundle AND c.action=''cancel'' AND c.request_id::text=current_setting(''crm.family_refresh_cancel_request'',true) AND m.user_id::text=current_setting(''crm.family_refresh_cancel_actor'',true) AND m.role=''admin'' AND m.status=''active'')) THEN RAISE EXCEPTION ''family settlement executor unavailable''; END IF;');
END $$;
