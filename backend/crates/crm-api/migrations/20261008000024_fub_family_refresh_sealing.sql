-- Full native recipes stay encrypted. Old proof fixtures remain readable, but
-- only newly authenticated recipes may be produced by the application runner.
ALTER TABLE migration_family_refresh_write_proof
 ADD COLUMN nonce BYTEA CHECK(nonce IS NULL OR octet_length(nonce)=24),
 ADD COLUMN ciphertext BYTEA CHECK(ciphertext IS NULL OR octet_length(ciphertext)<=4194320),
 ADD CHECK((nonce IS NULL)=(ciphertext IS NULL));
ALTER TABLE migration_family_refresh_plan
 ADD COLUMN proof_after BIGINT NOT NULL DEFAULT 0 CHECK(proof_after>=0),
 ADD COLUMN proofs_complete BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN seal_after BIGINT NOT NULL DEFAULT 0 CHECK(seal_after>=0),
 ADD COLUMN seal_counts JSONB NOT NULL DEFAULT '{}';
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_plan_guard()'::regprocedure) INTO definition;
 IF position('''catalog_walk_complete''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected plan guard'; END IF;
 EXECUTE replace(definition,'''catalog_walk_complete''','''catalog_walk_complete'',''proof_after'',''proofs_complete'',''seal_after'',''seal_counts''');
 SELECT pg_get_functiondef('crm_family_refresh_retained_size(jsonb)'::regprocedure) INTO definition;
 IF position('''counts''' IN definition)=0 THEN RAISE EXCEPTION 'unexpected byte inventory'; END IF;
 EXECUTE replace(definition,'''counts''','''counts'',''seal_counts''');
END $$;
CREATE FUNCTION crm_family_refresh_sealing_fence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_user<>'crm_app' THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='migration_family_refresh_manifest' THEN
  IF EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND organization_id=NEW.organization_id AND source_walk_complete AND owned_walk_complete) THEN RAISE EXCEPTION 'family preparation unit set closed'; END IF;
  RETURN NEW;
 END IF;
 IF OLD.state='ready' AND ROW(NEW.expires_at,NEW.digest,NEW.counts,NEW.position,NEW.proof_after,NEW.proofs_complete,NEW.seal_after,NEW.seal_counts) IS DISTINCT FROM ROW(OLD.expires_at,OLD.digest,OLD.counts,OLD.position,OLD.proof_after,OLD.proofs_complete,OLD.seal_after,OLD.seal_counts) THEN RAISE EXCEPTION 'ready family plan immutable'; END IF;
 IF ROW(NEW.proof_after,NEW.proofs_complete,NEW.seal_after,NEW.seal_counts) IS DISTINCT FROM ROW(OLD.proof_after,OLD.proofs_complete,OLD.seal_after,OLD.seal_counts) OR (NEW.state='ready' AND OLD.state<>'ready') THEN
  IF OLD.state<>'preparing' OR OLD.confirmed_at IS NOT NULL OR NOT OLD.source_walk_complete OR NOT OLD.owned_walk_complete
   OR OLD.lease_token IS NULL OR OLD.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR OLD.lease_expires_at<=clock_timestamp()
   OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_bundle b JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE b.id=OLD.bundle_id AND b.organization_id=OLD.organization_id AND b.state='preparing' AND b.confirmed_at IS NULL AND m.role='admin' AND m.status='active')
   THEN RAISE EXCEPTION 'stale family sealing'; END IF;
  IF NEW.proof_after IS DISTINCT FROM OLD.proof_after AND (OLD.proofs_complete OR NEW.proof_after<>OLD.proof_after+1 OR NEW.proof_after>OLD.position) THEN RAISE EXCEPTION 'family proof cursor skipped unit'; END IF;
  IF NEW.proofs_complete AND NEW.proof_after<>OLD.position THEN RAISE EXCEPTION 'family proofs incomplete'; END IF;
  IF NEW.seal_after IS DISTINCT FROM OLD.seal_after AND (NOT OLD.proofs_complete OR NEW.seal_after<>OLD.seal_after+1 OR NEW.seal_after>OLD.position) THEN RAISE EXCEPTION 'family seal cursor skipped unit'; END IF;
  IF NEW.state='ready' AND (NOT NEW.proofs_complete OR NEW.seal_after<>NEW.position OR NEW.seal_counts<>NEW.counts OR NEW.digest IS NULL OR NEW.expires_at IS NULL OR NEW.expires_at<=clock_timestamp() OR NEW.expires_at>clock_timestamp()+interval '10 minutes') THEN RAISE EXCEPTION 'family seal incomplete'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_sealing_fence BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_sealing_fence();
CREATE TRIGGER family_refresh_sealed_units BEFORE INSERT ON migration_family_refresh_manifest FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_sealing_fence();
CREATE FUNCTION crm_family_refresh_recipe_fence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_user='crm_app' AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan p JOIN migration_family_refresh_manifest u ON u.plan_id=p.id AND u.organization_id=p.organization_id WHERE p.id=NEW.plan_id AND p.bundle_id=NEW.bundle_id AND p.organization_id=NEW.organization_id AND u.id=NEW.manifest_id AND u.position=p.proof_after+1 AND p.source_walk_complete AND p.owned_walk_complete AND NOT p.proofs_complete AND p.state='preparing' AND p.confirmed_at IS NULL AND p.lease_token::text=current_setting('crm.family_refresh_lease',true) AND p.lease_expires_at>clock_timestamp() AND EXISTS(SELECT 1 FROM migration_family_refresh_bundle b JOIN organization_membership m ON m.organization_id=b.organization_id AND m.user_id=b.executor_user_id WHERE b.id=p.bundle_id AND b.organization_id=p.organization_id AND b.state='preparing' AND b.confirmed_at IS NULL AND m.role='admin' AND m.status='active') AND NEW.nonce IS NOT NULL AND NEW.ciphertext IS NOT NULL) THEN RAISE EXCEPTION 'unbound family native recipe'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_recipe_fence BEFORE INSERT ON migration_family_refresh_write_proof FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_recipe_fence();
REVOKE ALL ON FUNCTION crm_family_refresh_sealing_fence(),crm_family_refresh_recipe_fence() FROM PUBLIC;
