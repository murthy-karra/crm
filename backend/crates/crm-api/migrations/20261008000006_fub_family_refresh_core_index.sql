-- One authenticated core-capture walk, owned/charged by the bundle payer.
ALTER TABLE migration_family_refresh_plan ADD COLUMN capture_checkpoint BIGINT NOT NULL DEFAULT 0 CHECK(capture_checkpoint>=0);
CREATE TABLE migration_family_refresh_core_page (
 id UUID PRIMARY KEY, bundle_id UUID NOT NULL, plan_id UUID NOT NULL, organization_id UUID NOT NULL,
 snapshot_id UUID NOT NULL, capture_id UUID NOT NULL, capture_sequence BIGINT NOT NULL CHECK(capture_sequence>0),
 checkpoint BIGINT NOT NULL CHECK(checkpoint>=0), stream TEXT NOT NULL CHECK(stream IN ('people','custom_fields','users','notes','note_detail','tasks_open','tasks_completed')),
 accepted BOOLEAN NOT NULL, reason TEXT CHECK(octet_length(reason)<=96),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24), ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext) BETWEEN 16 AND 65552),
 UNIQUE(id,plan_id,bundle_id,organization_id), UNIQUE(bundle_id,organization_id,capture_id),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id),
 FOREIGN KEY(capture_id,snapshot_id,organization_id) REFERENCES migration_snapshot_capture(id,snapshot_id,organization_id)
);
CREATE UNIQUE INDEX family_refresh_core_cursor ON migration_family_refresh_core_page(bundle_id,organization_id,stream,checkpoint) WHERE accepted;
ALTER TABLE migration_family_refresh_source ADD COLUMN core_page_id UUID,
 ADD FOREIGN KEY(core_page_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_core_page(id,plan_id,bundle_id,organization_id);
CREATE FUNCTION crm_family_refresh_core_index_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; page migration_family_refresh_core_page;
BEGIN
 IF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  IF NEW.capture_checkpoint=OLD.capture_checkpoint THEN RETURN NEW; END IF;
  IF NEW.capture_checkpoint<OLD.capture_checkpoint OR OLD.phase<>'capture' THEN RAISE EXCEPTION 'invalid family capture checkpoint'; END IF;
  p:=OLD;
 ELSE
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
  IF TG_TABLE_NAME='migration_family_refresh_source' THEN
   IF p.family='history' AND NEW.core_page_id IS NULL THEN RETURN NEW; END IF;
   SELECT * INTO page FROM migration_family_refresh_core_page WHERE id=NEW.core_page_id AND plan_id=p.id AND bundle_id=p.bundle_id AND organization_id=p.organization_id;
   IF page.id IS NULL OR page.capture_id<>NEW.capture_id OR page.capture_sequence<>NEW.capture_sequence THEN RAISE EXCEPTION 'invalid family source page binding'; END IF;
  ELSE
   IF NOT EXISTS(SELECT 1 FROM migration_snapshot_capture c WHERE c.id=NEW.capture_id AND c.snapshot_id=NEW.snapshot_id
    AND c.organization_id=NEW.organization_id AND c.sequence=NEW.capture_sequence AND c.checkpoint=NEW.checkpoint
    AND c.stream=NEW.stream AND c.accepted=NEW.accepted AND c.sequence>p.capture_checkpoint)
    THEN RAISE EXCEPTION 'invalid family capture reference'; END IF;
  END IF;
 END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id FOR SHARE;
 IF b.id IS NULL OR p.id IS NULL OR p.id IS DISTINCT FROM b.payer_plan_id OR p.source_snapshot_id IS DISTINCT FROM b.core_snapshot_id
  OR p.source_snapshot_id IS NULL OR p.family='history' OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL
  OR p.state<>'preparing' OR p.phase<>'capture' OR p.confirmed_at IS NOT NULL
  OR p.lease_epoch<=0 OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true)
  OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=b.organization_id AND m.user_id=b.executor_user_id AND m.status='active' AND m.role='admin')
  THEN RAISE EXCEPTION 'stale family capture claim'; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_core_page' THEN
  IF NEW.snapshot_id<>p.source_snapshot_id OR NOT EXISTS(SELECT 1 FROM migration_core_change_report r WHERE r.id=b.core_report_id AND r.organization_id=b.organization_id AND r.state='completed' AND r.newer_snapshot_id=NEW.snapshot_id AND r.newer_sequence>=NEW.capture_sequence) THEN RAISE EXCEPTION 'family capture snapshot mismatch'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_core_page_guard BEFORE INSERT ON migration_family_refresh_core_page FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_core_index_guard();
CREATE TRIGGER family_refresh_core_source_guard BEFORE INSERT ON migration_family_refresh_source FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_core_index_guard();
CREATE TRIGGER family_refresh_core_progress_guard BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_core_index_guard();
CREATE TRIGGER family_refresh_core_page_immutable BEFORE UPDATE OR DELETE ON migration_family_refresh_core_page FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER family_refresh_measure AFTER INSERT ON migration_family_refresh_core_page FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_measure();
GRANT SELECT,INSERT ON migration_family_refresh_core_page TO crm_app;
REVOKE ALL ON FUNCTION crm_family_refresh_core_index_guard() FROM PUBLIC;
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('''cohort_counts'']' IN definition)=0 THEN RAISE EXCEPTION 'unexpected family plan guard'; END IF;
 EXECUTE replace(definition,'''cohort_counts'']','''cohort_counts'',''capture_checkpoint'']');
 SELECT pg_get_functiondef('crm_family_refresh_retained_size(jsonb)'::regprocedure) INTO definition;
 EXECUTE replace(definition,'''operation'',''counts'',''results''','''operation'',''counts'',''results'',''stream''');
END $$;
-- Baseline discovery must not scan all source keys for each native record.
CREATE INDEX family_refresh_head_target ON migration_family_refresh_head(organization_id,source_account_id,kind,target_id);
CREATE TRIGGER family_refresh_core_page_no_truncate BEFORE TRUNCATE ON migration_family_refresh_core_page FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
