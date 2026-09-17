-- Revocation must leave a durable pause even after the old executor loses its
-- role. This exception settles only a small control-only pause; it grants no
-- lease, unit reservation, result, head, or native mutation authority.
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_settle(uuid,uuid,uuid,uuid,bigint,boolean)'::regprocedure) INTO definition;
 IF position('IF NOT FOUND THEN RAISE EXCEPTION ''family executor unavailable''; END IF;' IN definition)=0
 OR position('THEN RAISE EXCEPTION ''family settlement executor unavailable''; END IF;' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family settlement'; END IF;
 definition:=replace(definition,'IF NOT FOUND THEN RAISE EXCEPTION ''family executor unavailable''; END IF;','');
 definition:=replace(definition,'THEN RAISE EXCEPTION ''family settlement executor unavailable''; END IF;',
 'AND NOT (r.purpose=''control'' AND NOT release_control AND b.state=''paused'' AND p.lease_token IS NULL AND ((p.state=''paused'' AND p.pause_reason=''executor_revoked'' AND p.measured_bytes-p.retained_bytes BETWEEN -1024 AND 1024) OR (b.payer_plan_id=p.id AND p.measured_bytes=p.retained_bytes)) AND (b.payer_plan_id<>p.id OR b.shared_measured_bytes-b.shared_retained_bytes BETWEEN -3 AND 1) AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_reservation u WHERE u.organization_id=org AND u.plan_id=plan AND u.purpose=''unit'')) THEN RAISE EXCEPTION ''family settlement executor unavailable''; END IF;');
 EXECUTE definition;
END $$;
