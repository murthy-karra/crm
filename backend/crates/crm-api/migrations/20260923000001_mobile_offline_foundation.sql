-- D-074: native operation receipts and bounded read projections. No customer
-- content is duplicated into receipts or reconciliation metadata.
ALTER TABLE person ADD COLUMN mobile_revision BIGINT NOT NULL DEFAULT 1 CHECK (mobile_revision > 0);
ALTER TABLE task ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0);
CREATE FUNCTION crm_mobile_task_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-'revision') IS DISTINCT FROM (to_jsonb(OLD)-'revision') THEN
   NEW.revision := OLD.revision + 1;
 ELSE NEW.revision := OLD.revision;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_task_revision BEFORE UPDATE ON task FOR EACH ROW EXECUTE FUNCTION crm_mobile_task_revision();
CREATE FUNCTION crm_mobile_person_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF ROW(NEW.first_name,NEW.last_name,NEW.stage_id,NEW.assigned_user_id,NEW.created_at)
 IS DISTINCT FROM ROW(OLD.first_name,OLD.last_name,OLD.stage_id,OLD.assigned_user_id,OLD.created_at) THEN
   NEW.mobile_revision := OLD.mobile_revision+1;
 END IF;
 IF NEW.mobile_revision < OLD.mobile_revision THEN RAISE EXCEPTION 'mobile revision regression'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER mobile_person_revision BEFORE UPDATE ON person FOR EACH ROW EXECUTE FUNCTION crm_mobile_person_revision();
CREATE FUNCTION crm_mobile_component_revision() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p uuid; o uuid;
BEGIN
 IF TG_OP='UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN RETURN NULL; END IF;
 IF TG_OP<>'INSERT' THEN
   UPDATE person SET mobile_revision=mobile_revision+1 WHERE id=OLD.person_id AND organization_id=OLD.organization_id;
 END IF;
 IF TG_OP='INSERT' OR (TG_OP='UPDATE' AND ROW(NEW.person_id,NEW.organization_id) IS DISTINCT FROM ROW(OLD.person_id,OLD.organization_id)) THEN
   UPDATE person SET mobile_revision=mobile_revision+1 WHERE id=NEW.person_id AND organization_id=NEW.organization_id;
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER mobile_note_revision AFTER INSERT OR UPDATE OR DELETE ON note FOR EACH ROW EXECUTE FUNCTION crm_mobile_component_revision();
CREATE TRIGGER mobile_task_person_revision AFTER INSERT OR UPDATE OR DELETE ON task FOR EACH ROW EXECUTE FUNCTION crm_mobile_component_revision();
CREATE TRIGGER mobile_contact_revision AFTER INSERT OR UPDATE OR DELETE ON contact_method FOR EACH ROW EXECUTE FUNCTION crm_mobile_component_revision();
CREATE FUNCTION crm_mobile_label_revision() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p uuid;
BEGIN
 IF TG_TABLE_NAME='stage' THEN
   IF NEW.name IS NOT DISTINCT FROM OLD.name THEN RETURN NULL; END IF;
   FOR p IN SELECT id FROM person WHERE organization_id=NEW.organization_id AND stage_id=NEW.id ORDER BY id LOOP
     UPDATE person SET mobile_revision=mobile_revision+1 WHERE id=p;
   END LOOP;
 ELSE
   IF NEW.display_name IS NOT DISTINCT FROM OLD.display_name THEN RETURN NULL; END IF;
   FOR p IN SELECT id FROM person WHERE assigned_user_id=NEW.id
     UNION SELECT person_id FROM note WHERE author_user_id=NEW.id AND deleted_at IS NULL
     UNION SELECT person_id FROM task WHERE NEW.id IN (created_by_user_id,assignee_user_id,completed_by_user_id) AND deleted_at IS NULL
     ORDER BY 1 LOOP
     UPDATE person SET mobile_revision=mobile_revision+1 WHERE id=p;
   END LOOP;
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER mobile_stage_label AFTER UPDATE ON stage FOR EACH ROW EXECUTE FUNCTION crm_mobile_label_revision();
CREATE TRIGGER mobile_user_label AFTER UPDATE ON app_user FOR EACH ROW EXECUTE FUNCTION crm_mobile_label_revision();
CREATE INDEX mobile_contact_page ON contact_method(organization_id,person_id,id);
CREATE INDEX mobile_note_page ON note(organization_id,person_id,id) WHERE deleted_at IS NULL;
CREATE INDEX mobile_task_page ON task(organization_id,person_id,id) WHERE deleted_at IS NULL;
CREATE INDEX mobile_assigned_selection ON person(organization_id,assigned_user_id,id);

CREATE TABLE mobile_context (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL REFERENCES organization(id),
 actor_user_id UUID NOT NULL REFERENCES app_user(id), installation_id UUID NOT NULL,
 protocol TEXT NOT NULL CHECK(protocol='mobile-v1'), authorized_at TIMESTAMPTZ NOT NULL,
 offline_access_expires_at TIMESTAMPTZ NOT NULL,
 UNIQUE(organization_id,actor_user_id,installation_id), UNIQUE(id,organization_id,actor_user_id)
);
CREATE TABLE mobile_operation_receipt (
 organization_id UUID NOT NULL REFERENCES organization(id), actor_user_id UUID NOT NULL REFERENCES app_user(id),
 operation_id UUID NOT NULL, context_id UUID NOT NULL,
 digest_version INTEGER NOT NULL CHECK(digest_version=1), digest_key_id TEXT NOT NULL,
 payload_digest BYTEA NOT NULL CHECK(octet_length(payload_digest)=32),
 kind TEXT NOT NULL CHECK(kind IN ('add_note','create_task','complete_task')),
 person_id UUID NOT NULL, resource_type TEXT NOT NULL CHECK(resource_type IN ('note','task')),
 resource_id UUID NOT NULL, committed_revision BIGINT CHECK(committed_revision>0),
 person_revision BIGINT NOT NULL CHECK(person_revision>0), changed BOOLEAN NOT NULL,
 accepted_at TIMESTAMPTZ NOT NULL,
 PRIMARY KEY(organization_id,actor_user_id,operation_id),
 FOREIGN KEY(context_id,organization_id,actor_user_id) REFERENCES mobile_context(id,organization_id,actor_user_id)
);
CREATE TABLE mobile_reconciliation (
 id UUID PRIMARY KEY, context_id UUID NOT NULL,
 organization_id UUID NOT NULL REFERENCES organization(id), actor_user_id UUID NOT NULL REFERENCES app_user(id),
 role TEXT NOT NULL, workspace_revision BIGINT NOT NULL,
 evaluated_at TIMESTAMPTZ NOT NULL, expires_at TIMESTAMPTZ NOT NULL,
 complete BOOLEAN NOT NULL, selected_count INTEGER NOT NULL CHECK(selected_count BETWEEN 0 AND 25000),
 pinned_person_ids UUID[] NOT NULL CHECK(cardinality(pinned_person_ids)<=25000),
 today_digest BYTEA NOT NULL CHECK(octet_length(today_digest)=32),
 FOREIGN KEY(context_id,organization_id,actor_user_id) REFERENCES mobile_context(id,organization_id,actor_user_id)
);
CREATE INDEX mobile_reconciliation_admission ON mobile_reconciliation(organization_id,actor_user_id,context_id,expires_at);
CREATE TABLE mobile_reconciliation_person (
 generation_id UUID NOT NULL REFERENCES mobile_reconciliation(id) ON DELETE CASCADE,
 person_id UUID NOT NULL, revision BIGINT NOT NULL CHECK(revision>0), reasons TEXT[] NOT NULL,
 PRIMARY KEY(generation_id,person_id)
);
GRANT SELECT,INSERT,UPDATE,DELETE ON mobile_context,mobile_reconciliation,mobile_reconciliation_person TO crm_app;
GRANT SELECT,INSERT ON mobile_operation_receipt TO crm_app;
-- Updating this trusted serialization row prevents RR admission write skew.
CREATE TABLE mobile_admission (organization_id UUID PRIMARY KEY REFERENCES organization(id), revision BIGINT NOT NULL DEFAULT 1);
GRANT SELECT,INSERT,UPDATE ON mobile_admission TO crm_app;
