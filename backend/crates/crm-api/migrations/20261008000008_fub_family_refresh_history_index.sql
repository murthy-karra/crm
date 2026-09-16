-- Retained history pages have their own cursor proof and source-run binding.
CREATE TABLE migration_family_refresh_history_page (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 run_id UUID NOT NULL, capture_id UUID NOT NULL, capture_sequence BIGINT NOT NULL CHECK(capture_sequence>0),
 checkpoint BIGINT NOT NULL CHECK(checkpoint>=0), stream TEXT NOT NULL CHECK(stream IN ('events','calls','text_messages')),
 accepted BOOLEAN NOT NULL, reason TEXT CHECK(octet_length(reason)<=96),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24), ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext) BETWEEN 16 AND 65552),
 UNIQUE(id,plan_id,bundle_id,organization_id), UNIQUE(bundle_id,organization_id,capture_id),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id),
 FOREIGN KEY(capture_id,run_id,organization_id) REFERENCES migration_history_capture(id,run_id,organization_id)
);
CREATE UNIQUE INDEX family_refresh_history_cursor ON migration_family_refresh_history_page(bundle_id,organization_id,stream,checkpoint) WHERE accepted;
CREATE INDEX family_refresh_history_identity_capture ON migration_history_capture(run_id,organization_id,sequence DESC) WHERE classification='identity';
ALTER TABLE migration_family_refresh_source ADD COLUMN history_page_id UUID,
 ADD FOREIGN KEY(history_page_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_history_page(id,plan_id,bundle_id,organization_id),
 ADD CHECK(core_page_id IS NULL OR history_page_id IS NULL);
CREATE FUNCTION crm_family_refresh_history_index_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; page migration_family_refresh_history_page;
BEGIN
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  IF NEW.family<>'history' OR NEW.capture_checkpoint=OLD.capture_checkpoint THEN RETURN NEW; END IF;
  IF NEW.capture_checkpoint<OLD.capture_checkpoint OR OLD.phase<>'capture' THEN RAISE EXCEPTION 'invalid history checkpoint'; END IF;
  p:=OLD;
 ELSE
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
  IF TG_TABLE_NAME='migration_family_refresh_source' THEN
   IF p.family<>'history' THEN
    IF NEW.history_page_id IS NOT NULL THEN RAISE EXCEPTION 'history page on core source'; END IF;
    RETURN NEW;
   END IF;
   SELECT * INTO page FROM migration_family_refresh_history_page WHERE id=NEW.history_page_id AND plan_id=p.id AND bundle_id=p.bundle_id AND organization_id=p.organization_id;
   IF page.id IS NULL OR page.capture_id<>NEW.capture_id OR page.capture_sequence<>NEW.capture_sequence OR NEW.kind<>(CASE page.stream WHEN 'events' THEN 'event' WHEN 'calls' THEN 'call' ELSE 'text' END) THEN RAISE EXCEPTION 'invalid history source page binding'; END IF;
  ELSE
   IF NEW.run_id IS DISTINCT FROM p.history_capture_id OR NOT EXISTS(SELECT 1 FROM migration_history_capture c WHERE c.id=NEW.capture_id AND c.run_id=NEW.run_id AND c.organization_id=NEW.organization_id AND c.sequence=NEW.capture_sequence AND c.checkpoint=NEW.checkpoint AND c.family=NEW.stream AND (c.classification='advancing')=NEW.accepted AND c.sequence>p.capture_checkpoint) THEN RAISE EXCEPTION 'invalid history capture reference'; END IF;
  END IF;
 END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id FOR SHARE;
 IF b.id IS NULL OR p.id IS NULL OR p.family<>'history' OR p.history_capture_id IS DISTINCT FROM b.history_capture_id THEN RAISE EXCEPTION 'invalid history index owner'; END IF;
 -- Synthetic migrator fixtures may construct execution evidence, but must retain
 -- the exact page/run/source binding above. Application writes require a lease.
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 IF b.state<>'preparing' OR b.confirmed_at IS NOT NULL OR p.state<>'preparing' OR p.phase<>'capture' OR p.confirmed_at IS NOT NULL
  OR p.lease_epoch<=0 OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=b.organization_id AND m.user_id=b.executor_user_id AND m.status='active' AND m.role='admin') THEN RAISE EXCEPTION 'stale history capture claim'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_history_page_guard BEFORE INSERT ON migration_family_refresh_history_page FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_history_index_guard();
CREATE TRIGGER family_refresh_history_source_guard BEFORE INSERT ON migration_family_refresh_source FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_history_index_guard();
CREATE TRIGGER family_refresh_history_progress_guard BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_history_index_guard();
CREATE TRIGGER family_refresh_history_page_immutable BEFORE UPDATE OR DELETE ON migration_family_refresh_history_page FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER family_refresh_history_page_no_truncate BEFORE TRUNCATE ON migration_family_refresh_history_page FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER family_refresh_measure AFTER INSERT ON migration_family_refresh_history_page FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_measure();
GRANT SELECT,INSERT ON migration_family_refresh_history_page TO crm_app;
REVOKE ALL ON FUNCTION crm_family_refresh_history_index_guard() FROM PUBLIC;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_core_index_guard()'::regprocedure) INTO definition;
 IF position('p:=OLD;' IN definition)=0 THEN RAISE EXCEPTION 'unexpected core progress guard'; END IF;
 EXECUTE replace(definition,'p:=OLD;','p:=OLD; IF p.family=''history'' THEN RETURN NEW; END IF;');
END $$;
