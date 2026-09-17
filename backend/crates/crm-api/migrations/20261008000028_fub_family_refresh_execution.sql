-- Native growth and encrypted result evidence settle with the same unit.
ALTER TABLE migration_family_refresh_result ADD CONSTRAINT family_native_bytes_bound CHECK(native_bytes<=67108864);
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_retained_size(jsonb)'::regprocedure) INTO definition;
 IF position('IF v IS NULL THEN RETURN 0; END IF;' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family byte inventory'; END IF;
 EXECUTE replace(definition,'IF v IS NULL THEN RETURN 0; END IF;','IF v IS NULL THEN RETURN 0; END IF; total:=total+COALESCE((v->>''native_bytes'')::bigint,0);');
 SELECT pg_get_functiondef('crm_family_refresh_mutation_allowed(uuid,text,text,jsonb,jsonb)'::regprocedure) INTO definition;
 IF position('IF unit.disposition NOT IN' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family native fence'; END IF;
 EXECUTE replace(definition,'IF unit.disposition NOT IN','IF p.cancel_requested OR p.phase<>''apply'' OR unit.position<>p.apply_position+1 THEN RETURN false; END IF; IF unit.disposition NOT IN');
END $$;
CREATE FUNCTION crm_family_refresh_apply_fence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_user<>'crm_app' OR NEW.apply_position=OLD.apply_position THEN RETURN NEW; END IF;
 IF OLD.state<>'running' OR OLD.confirmed_at IS NULL OR OLD.phase<>'apply' OR OLD.cancel_requested
  OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at IS NULL OR OLD.lease_expires_at<=clock_timestamp()
  OR NEW.apply_position<>OLD.apply_position+1 OR NEW.apply_position>OLD.position
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_manifest u JOIN migration_family_refresh_result r ON r.manifest_id=u.id AND r.organization_id=u.organization_id WHERE u.plan_id=OLD.id AND u.organization_id=OLD.organization_id AND u.position=NEW.apply_position)
  THEN RAISE EXCEPTION 'unsettled family execution checkpoint'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_apply_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_apply_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_apply_fence() FROM PUBLIC;
