-- D-092: current-version readers and exclusive first-owner adapters are installed.
-- Enable only typed guarded history writes; ciphertext deletion remains with the
-- existing narrowly scoped erasure functions.
DO $$ DECLARE definition TEXT; needle TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_history_version_guard()'::regprocedure) INTO definition;
 needle:='IF r.id IS NULL OR u.id IS NULL OR s.id IS NULL';
 IF position(needle in definition)=0 THEN RAISE EXCEPTION 'history correction execution fence drift'; END IF;
 definition:=replace(definition,needle,E'IF current_user=''crm_app'' AND (p.cancel_requested OR p.phase<>''apply'' OR u.position<>p.apply_position+1 OR NEW.actor_kind<>''user'' OR NEW.actor_user_id IS DISTINCT FROM b.executor_user_id OR NEW.on_behalf_of_user_id IS NOT NULL) THEN RAISE EXCEPTION ''history correction execution unit invalid''; END IF;\n '||needle);
 EXECUTE definition;
END $$;
GRANT INSERT,UPDATE ON migration_family_refresh_history_head TO crm_app;
GRANT INSERT ON migration_family_refresh_history_display,fub_event_record_corrected,fub_call_record_corrected,fub_text_record_corrected TO crm_app;

-- A corrected entry may have a different executor from its first import.
-- Renaming that executor invalidates page revisions just like initial actors.
DO $$ DECLARE definition TEXT; needle TEXT; BEGIN
 SELECT pg_get_functiondef('crm_history_review_user_label()'::regprocedure) INTO definition;
 needle:='UNION SELECT organization_id,person_id AS person_id FROM fub_text_record_imported WHERE actor_user_id=NEW.id';
 IF position(needle in definition)=0 THEN RAISE EXCEPTION 'history actor revision projection drift'; END IF;
 EXECUTE replace(definition,needle,needle||' UNION SELECT organization_id,person_id AS person_id FROM fub_event_record_corrected WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_call_record_corrected WHERE actor_user_id=NEW.id UNION SELECT organization_id,person_id AS person_id FROM fub_text_record_corrected WHERE actor_user_id=NEW.id');
END $$;

-- Keep the count mutation helper inaccessible as a direct application call.
-- This trigger runs only after the guarded immutable correction was inserted.
ALTER FUNCTION crm_family_history_version_apply() SECURITY DEFINER;
ALTER FUNCTION crm_family_history_version_apply() SET search_path=public,pg_temp;
