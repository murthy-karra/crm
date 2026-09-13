-- An approved activity import can INSERT a note/task while ordinary Person
-- writes remain blocked. Its derived revision bump inherits only that permitted
-- trigger-bound write, not a client authority token. These trigger functions can
-- only increment public.person.mobile_revision; original BEFORE guards still
-- authorize the originating business mutation. No request-callable entry point.
-- Fixed search_path and qualified tables exclude caller temporary-object shadowing.
CREATE OR REPLACE FUNCTION crm_mobile_component_revision() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE p uuid; o uuid;
BEGIN
 IF TG_OP='UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN RETURN NULL; END IF;
 IF TG_OP<>'INSERT' THEN
   UPDATE public.person SET mobile_revision=mobile_revision+1 WHERE id=OLD.person_id AND organization_id=OLD.organization_id;
 END IF;
 IF TG_OP='INSERT' OR (TG_OP='UPDATE' AND ROW(NEW.person_id,NEW.organization_id) IS DISTINCT FROM ROW(OLD.person_id,OLD.organization_id)) THEN
   UPDATE public.person SET mobile_revision=mobile_revision+1 WHERE id=NEW.person_id AND organization_id=NEW.organization_id;
 END IF;
 RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION crm_mobile_label_revision() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE p uuid;
BEGIN
 IF TG_TABLE_NAME='stage' THEN
   IF NEW.name IS NOT DISTINCT FROM OLD.name THEN RETURN NULL; END IF;
   FOR p IN SELECT id FROM public.person WHERE organization_id=NEW.organization_id AND stage_id=NEW.id ORDER BY id LOOP
     UPDATE public.person SET mobile_revision=mobile_revision+1 WHERE id=p;
   END LOOP;
 ELSE
   IF NEW.display_name IS NOT DISTINCT FROM OLD.display_name THEN RETURN NULL; END IF;
   FOR p IN SELECT id FROM public.person WHERE assigned_user_id=NEW.id
     UNION SELECT person_id FROM public.note WHERE author_user_id=NEW.id AND deleted_at IS NULL
     UNION SELECT person_id FROM public.task WHERE NEW.id IN (created_by_user_id,assignee_user_id,completed_by_user_id) AND deleted_at IS NULL
     ORDER BY 1 LOOP
     UPDATE public.person SET mobile_revision=mobile_revision+1 WHERE id=p;
   END LOOP;
 END IF;
 RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION crm_mobile_component_revision(),crm_mobile_label_revision() FROM PUBLIC;
