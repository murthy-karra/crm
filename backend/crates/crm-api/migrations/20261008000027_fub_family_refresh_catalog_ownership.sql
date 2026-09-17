-- Refresh catalog results use the existing immutable registry with an exclusive
-- third owner tuple. Original/admitted claims retain their exact owner/evidence.
ALTER TABLE migration_metadata_catalog_claim
 ADD COLUMN refresh_bundle_id UUID,
 ADD COLUMN refresh_plan_id UUID,
 ADD COLUMN refresh_manifest_id UUID,
 ADD CONSTRAINT family_catalog_owner_fk FOREIGN KEY(refresh_manifest_id,refresh_plan_id,refresh_bundle_id,organization_id)
 REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id);
ALTER TABLE migration_metadata_catalog_claim DROP CONSTRAINT migration_metadata_catalog_claim_check;
ALTER TABLE migration_metadata_catalog_claim DROP CONSTRAINT am_claim_owner_check;
ALTER TABLE migration_metadata_catalog_claim ADD CONSTRAINT am_claim_owner_check CHECK (
 (num_nonnulls(original_import_id,original_plan_id,original_mapping_id)=3 AND num_nonnulls(admitted_import_id,admitted_plan_id,admitted_mapping_id,refresh_bundle_id,refresh_plan_id,refresh_manifest_id)=0)
 OR (num_nonnulls(admitted_import_id,admitted_plan_id,admitted_mapping_id)=3 AND num_nonnulls(original_import_id,original_plan_id,original_mapping_id,refresh_bundle_id,refresh_plan_id,refresh_manifest_id)=0)
 OR (num_nonnulls(refresh_bundle_id,refresh_plan_id,refresh_manifest_id)=3 AND num_nonnulls(original_import_id,original_plan_id,original_mapping_id,admitted_import_id,admitted_plan_id,admitted_mapping_id)=0)
);
CREATE FUNCTION crm_family_refresh_catalog_owner() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE p migration_family_refresh_plan; b migration_family_refresh_bundle; u migration_family_refresh_manifest; m migration_family_refresh_mapping;
BEGIN
 IF NEW.refresh_plan_id IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.refresh_bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.refresh_plan_id AND bundle_id=b.id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=NEW.refresh_manifest_id AND plan_id=p.id AND bundle_id=b.id AND organization_id=NEW.organization_id;
 SELECT * INTO m FROM migration_family_refresh_mapping WHERE id=u.mapping_id AND plan_id=p.id AND bundle_id=b.id AND organization_id=NEW.organization_id;
 IF b.id IS NULL OR p.id IS NULL OR u.id IS NULL OR m.id IS NULL OR b.confirmed_at IS NULL OR p.confirmed_at IS NULL OR p.state<>'running' OR p.cancel_requested
  OR p.family<>'metadata' OR p.phase<>'apply' OR b.state NOT IN ('queued','running','paused') OR p.lease_token IS NULL OR p.lease_expires_at IS NULL OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at<=clock_timestamp()
  OR u.kind<>'catalog' OR u.disposition NOT IN ('insert','already_current') OR u.position<>p.apply_position+1
  OR NEW.source_account_id<>b.source_account_id OR NEW.kind<>m.kind OR NEW.source_key IS DISTINCT FROM m.source_key_hmac OR NEW.target_id IS DISTINCT FROM m.target_id
  OR NOT EXISTS(SELECT 1 FROM migration_workspace w WHERE w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id)
  OR NOT EXISTS(SELECT 1 FROM organization_membership a WHERE a.organization_id=b.organization_id AND a.user_id=b.executor_user_id AND a.role='admin' AND a.status='active')
  OR EXISTS(SELECT 1 FROM migration_family_refresh_result WHERE manifest_id=u.id AND organization_id=u.organization_id)
  THEN RAISE EXCEPTION 'unbound refresh catalog claim'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_catalog_owner BEFORE INSERT ON migration_metadata_catalog_claim FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_catalog_owner();
-- Charge this registry body exactly once to its refresh plan. Older owners keep
-- their established original/admitted accounting path.
CREATE FUNCTION crm_family_refresh_catalog_measure() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.refresh_plan_id IS NOT NULL THEN
  UPDATE migration_family_refresh_plan SET measured_bytes=measured_bytes+256+octet_length(NEW.kind)+octet_length(NEW.source_key)+octet_length(NEW.evidence_nonce)+octet_length(NEW.evidence_ciphertext)
   WHERE id=NEW.refresh_plan_id AND organization_id=NEW.organization_id;
  IF NOT FOUND THEN RAISE EXCEPTION 'refresh catalog accounting owner missing'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_catalog_measure AFTER INSERT ON migration_metadata_catalog_claim FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_catalog_measure();
REVOKE ALL ON FUNCTION crm_family_refresh_catalog_owner(),crm_family_refresh_catalog_measure() FROM PUBLIC;

CREATE TRIGGER family_refresh_catalog_immutable BEFORE UPDATE OR DELETE ON migration_metadata_catalog_claim
 FOR EACH ROW WHEN(OLD.refresh_plan_id IS NOT NULL) EXECUTE FUNCTION reject_mutation();
ALTER TABLE migration_metadata_catalog_claim ADD CONSTRAINT family_catalog_evidence_bound CHECK(refresh_plan_id IS NULL OR (octet_length(evidence_nonce)=24 AND octet_length(evidence_ciphertext)<=4194320));
