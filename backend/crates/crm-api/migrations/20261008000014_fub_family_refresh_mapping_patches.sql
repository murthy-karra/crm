-- D-092: bounded explicit choices belong to a replacement plan. The previous
-- mapping remains immutable, and rebuilding its inventory can recover choices
-- without copying an entire catalog inside the command transaction.
CREATE TABLE migration_family_refresh_mapping_patch (
 id UUID PRIMARY KEY,bundle_id UUID NOT NULL,plan_id UUID NOT NULL,organization_id UUID NOT NULL,
 source_mapping_id UUID NOT NULL,source_plan_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('tag','field','option','note_author','task_creator','task_assignee','task_kind')),
 source_key_hmac BYTEA NOT NULL CHECK(octet_length(source_key_hmac)=32),
 disposition TEXT NOT NULL CHECK(disposition IN ('hold','existing','create_matching','unassigned','kind')),
 target_id UUID,nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=65552),
 UNIQUE(plan_id,organization_id,kind,source_key_hmac),UNIQUE(id,plan_id,bundle_id,organization_id),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id),
 FOREIGN KEY(source_mapping_id,source_plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_mapping(id,plan_id,bundle_id,organization_id)
);
GRANT SELECT,INSERT ON migration_family_refresh_mapping_patch TO crm_app;
CREATE TRIGGER family_refresh_patch_immutable BEFORE UPDATE OR DELETE ON migration_family_refresh_mapping_patch FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER family_refresh_patch_no_truncate BEFORE TRUNCATE ON migration_family_refresh_mapping_patch FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER family_refresh_patch_measure AFTER INSERT ON migration_family_refresh_mapping_patch FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_measure();
CREATE TRIGGER family_refresh_patch_owner BEFORE INSERT ON migration_family_refresh_mapping_patch FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_owned_insert();
CREATE FUNCTION crm_family_refresh_patch_fence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle;
BEGIN
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=p.bundle_id AND organization_id=p.organization_id FOR SHARE;
 IF p.id IS NULL OR p.phase<>'mappings' OR p.mappings_complete OR p.state<>'preparing' OR p.confirmed_at IS NOT NULL
  OR b.state<>'preparing' OR b.confirmed_at IS NOT NULL OR p.predecessor_plan_id IS DISTINCT FROM NEW.source_plan_id
  OR p.lease_token IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR (p.family='metadata') IS DISTINCT FROM (NEW.kind IN ('tag','field','option')) OR p.family='history'
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=b.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active')
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_mapping m WHERE m.id=NEW.source_mapping_id AND m.plan_id=NEW.source_plan_id AND m.bundle_id=b.id AND m.organization_id=b.organization_id AND m.kind=NEW.kind AND m.source_key_hmac=NEW.source_key_hmac)
  OR (SELECT count(*) FROM migration_family_refresh_mapping_patch WHERE plan_id=p.id AND organization_id=p.organization_id)>=50
  THEN RAISE EXCEPTION 'invalid family mapping patch'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_patch_fence BEFORE INSERT ON migration_family_refresh_mapping_patch FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_patch_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_patch_fence() FROM PUBLIC;
