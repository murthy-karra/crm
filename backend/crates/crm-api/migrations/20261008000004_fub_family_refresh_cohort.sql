-- Freeze the discovery boundary after every earlier workspace writer commits.
-- Later admission completion timestamps cannot enter this bundle's cohort.
CREATE FUNCTION crm_family_refresh_boundary() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM set_config('lock_timeout','2000ms',true);
 PERFORM pg_advisory_xact_lock(hashtextextended('crm-workspace-v1:'||NEW.organization_id::text,0));
 NEW.created_at:=clock_timestamp(); NEW.updated_at:=NEW.created_at;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_00_boundary BEFORE INSERT ON migration_family_refresh_bundle FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_boundary();
REVOKE ALL ON FUNCTION crm_family_refresh_boundary() FROM PUBLIC;
ALTER TABLE migration_family_refresh_plan
 ADD COLUMN cohort_after TEXT NOT NULL DEFAULT '' CHECK(octet_length(cohort_after)<=128),
 ADD COLUMN cohort_counts JSONB;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('''run_byte_limit'']' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family plan guard'; END IF;
 definition:=replace(definition,'''run_byte_limit'']','''run_byte_limit'',''cohort_after'',''cohort_counts'']');
 definition:=replace(definition,'IF NEW.apply_position<OLD.apply_position',
  'IF NEW.cohort_after<OLD.cohort_after THEN RAISE EXCEPTION ''family cohort cursor regression''; END IF; IF OLD.phase<>''cohort'' AND (NEW.cohort_after IS DISTINCT FROM OLD.cohort_after OR NEW.cohort_counts IS DISTINCT FROM OLD.cohort_counts) THEN RAISE EXCEPTION ''family cohort sealed''; END IF; IF NEW.apply_position<OLD.apply_position');
 EXECUTE definition;
 SELECT pg_get_functiondef('crm_family_refresh_retained_size(jsonb)'::regprocedure) INTO definition;
 EXECUTE replace(definition,'''operation'',''counts'',''results''','''operation'',''counts'',''results'',''cohort_after'',''cohort_counts''');
END $$;
