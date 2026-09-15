-- D-092: one immutable payer for shared bundle evidence. Family-owned rows
-- remain on their own plan. This migration adds measurement, not a new worker.
ALTER TABLE migration_family_refresh_bundle
 ADD COLUMN payer_plan_id UUID,
 ADD COLUMN shared_measured_bytes BIGINT NOT NULL DEFAULT 0 CHECK(shared_measured_bytes>=0),
 ADD COLUMN shared_retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(shared_retained_bytes>=0),
 ADD CONSTRAINT family_refresh_payer_fk FOREIGN KEY(payer_plan_id,id,organization_id)
 REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE migration_family_refresh_head ADD COLUMN storage_plan_id UUID NOT NULL,
 ADD CONSTRAINT family_refresh_head_storage_fk FOREIGN KEY(storage_plan_id,organization_id) REFERENCES migration_family_refresh_plan(id,organization_id);

-- The permanent requirement is charged to the bundle that first installed it.
ALTER TABLE migration_family_refresh_requirement ADD COLUMN bundle_id UUID NOT NULL,
 ADD CONSTRAINT family_refresh_requirement_bundle_fk FOREIGN KEY(bundle_id,organization_id) REFERENCES migration_family_refresh_bundle(id,organization_id);

DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_bundle_guard()'::regprocedure) INTO definition;
 definition:=replace(definition,'''state'',''revision'',''confirmed_at'',''digest'',''updated_at''','''state'',''revision'',''confirmed_at'',''digest'',''updated_at'',''payer_plan_id'',''shared_measured_bytes'',''shared_retained_bytes''');
 definition:=replace(definition,'IF OLD.state IN (''completed'',''cancelled'')',
  'IF OLD.payer_plan_id IS NOT NULL AND NEW.payer_plan_id IS DISTINCT FROM OLD.payer_plan_id THEN RAISE EXCEPTION ''family payer immutable''; END IF; IF OLD.state IN (''completed'',''cancelled'')');
 definition:=replace(definition,'IF NEW.confirmed_at IS NOT NULL AND (NEW.digest IS NULL',
  'IF NEW.confirmed_at IS NOT NULL AND (NEW.payer_plan_id IS NULL OR NEW.digest IS NULL');
 EXECUTE definition;
END $$;

CREATE FUNCTION crm_family_refresh_head_storage() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 SELECT plan_id INTO NEW.storage_plan_id FROM migration_family_refresh_result WHERE id=NEW.result_id AND organization_id=NEW.organization_id;
 RETURN NEW;
END $$;
CREATE TRIGGER family_refresh_head_account BEFORE INSERT ON migration_family_refresh_head FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_head_storage();

CREATE FUNCTION crm_family_refresh_measure() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v JSONB; delta BIGINT; owner_plan UUID; owner_bundle UUID; org UUID;
BEGIN
 delta:=crm_family_refresh_retained_size(CASE WHEN TG_OP='DELETE' THEN NULL ELSE to_jsonb(NEW) END)
       -crm_family_refresh_retained_size(CASE WHEN TG_OP='INSERT' THEN NULL ELSE to_jsonb(OLD) END);
 IF delta=0 THEN RETURN COALESCE(NEW,OLD); END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD)); org:=(v->>'organization_id')::uuid;
 IF TG_TABLE_NAME='migration_family_refresh_head' THEN
  owner_plan:=(v->>'storage_plan_id')::uuid;
 ELSIF TG_TABLE_NAME='migration_family_refresh_plan' THEN
  owner_plan:=(v->>'id')::uuid;
 ELSE
  owner_plan:=(v->>'plan_id')::uuid;
 END IF;
 IF owner_plan IS NOT NULL THEN
  UPDATE migration_family_refresh_plan SET measured_bytes=measured_bytes+delta WHERE id=owner_plan AND organization_id=org;
 ELSE
  owner_bundle:=(v->>CASE WHEN TG_TABLE_NAME='migration_family_refresh_bundle' THEN 'id' ELSE 'bundle_id' END)::uuid;
  UPDATE migration_family_refresh_bundle SET shared_measured_bytes=shared_measured_bytes+delta WHERE id=owner_bundle AND organization_id=org;
 END IF;
 IF NOT FOUND THEN RAISE EXCEPTION 'family accounting owner unavailable'; END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['bundle','plan','cohort','source','mapping','manifest','result','head','receipt','requirement','write_proof'] LOOP
  EXECUTE format('CREATE TRIGGER family_refresh_measure AFTER INSERT OR UPDATE OR DELETE ON migration_family_refresh_%I FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_measure()',name);
 END LOOP;
END $$;
REVOKE ALL ON FUNCTION crm_family_refresh_measure(),crm_family_refresh_head_storage() FROM PUBLIC;

CREATE FUNCTION crm_family_refresh_reserve(org UUID,bundle UUID,plan UUID,reservation UUID,epoch BIGINT,amount BIGINT,purpose TEXT,run_ceiling BIGINT,org_ceiling BIGINT) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; l migration_snapshot_storage; s migration_snapshot; shared_bytes BIGINT:=0;
BEGIN
 IF amount<512 OR (purpose='control' AND amount<8192) OR amount>67108864 OR purpose NOT IN ('control','unit') OR run_ceiling<=0 OR org_ceiling<=0 THEN RAISE EXCEPTION 'invalid family reservation'; END IF;
 PERFORM crm_workspace_shared(org);
 PERFORM m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle owner ON owner.organization_id=m.organization_id AND owner.executor_user_id=m.user_id
  WHERE owner.id=bundle AND owner.organization_id=org AND m.role='admin' AND m.status='active' FOR SHARE OF m;
 IF NOT FOUND THEN RAISE EXCEPTION 'family executor unavailable'; END IF;
 SELECT * INTO l FROM migration_snapshot_storage WHERE organization_id=org FOR UPDATE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=plan AND bundle_id=bundle AND organization_id=org FOR UPDATE;
 IF b.id IS NULL OR p.id IS NULL OR l.organization_id IS NULL OR b.payer_plan_id IS NULL OR b.state IN ('cancelled','completed') OR p.lease_epoch<>epoch
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=org AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN RAISE EXCEPTION 'family reservation owner unavailable'; END IF;
 IF purpose='unit' AND (p.state NOT IN ('preparing','running') OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()) THEN RAISE EXCEPTION 'family reservation lease invalid'; END IF;
 IF b.payer_plan_id=p.id THEN shared_bytes:=b.shared_measured_bytes; END IF;
 -- Measured evidence includes any writes already made in this transaction.
 -- Existing reservations cover unsettled bytes, so only admitted retained
 -- bytes and reservations are used for the outer capacity comparison.
 IF amount>LEAST(p.run_byte_limit,run_ceiling)-p.retained_bytes-p.reserved_bytes-(CASE WHEN b.payer_plan_id=p.id THEN b.shared_retained_bytes ELSE 0 END)
  OR amount>LEAST(l.byte_limit,org_ceiling)-l.retained_bytes-l.reserved_bytes
  OR p.measured_bytes-p.retained_bytes+shared_bytes-(CASE WHEN b.payer_plan_id=p.id THEN b.shared_retained_bytes ELSE 0 END)>amount-512 THEN RETURN false; END IF;
 IF p.source_snapshot_id IS NOT NULL THEN
  SELECT * INTO s FROM migration_snapshot WHERE id=p.source_snapshot_id AND organization_id=org FOR UPDATE;
  IF s.id IS NULL THEN RAISE EXCEPTION 'family source budget unavailable'; END IF;
  IF amount>LEAST(s.run_byte_limit,run_ceiling)-s.retained_bytes-s.reserved_bytes THEN RETURN false; END IF;
 END IF;
 INSERT INTO migration_family_refresh_reservation(token,organization_id,bundle_id,plan_id,lease_epoch,purpose,byte_count) VALUES(reservation,org,bundle,plan,epoch,purpose,amount);
 UPDATE migration_family_refresh_plan SET reserved_bytes=reserved_bytes+amount WHERE id=plan AND organization_id=org;
 UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+amount WHERE organization_id=org;
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+amount WHERE id=p.source_snapshot_id AND organization_id=org; END IF;
 RETURN true;
END $$;

CREATE FUNCTION crm_family_refresh_settle(org UUID,bundle UUID,plan UUID,reservation UUID,epoch BIGINT,release_control BOOLEAN) RETURNS BIGINT LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; r migration_family_refresh_reservation; own_delta BIGINT; shared_delta BIGINT:=0; actual BIGINT; refund BIGINT;
BEGIN
 PERFORM crm_workspace_shared(org);
 PERFORM m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle owner ON owner.organization_id=m.organization_id AND owner.executor_user_id=m.user_id
  WHERE owner.id=bundle AND owner.organization_id=org AND m.role='admin' AND m.status='active' FOR SHARE OF m;
 IF NOT FOUND THEN RAISE EXCEPTION 'family executor unavailable'; END IF;
 PERFORM organization_id FROM migration_snapshot_storage WHERE organization_id=org FOR UPDATE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=plan AND bundle_id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO r FROM migration_family_refresh_reservation WHERE token=reservation AND plan_id=plan AND bundle_id=bundle AND organization_id=org FOR UPDATE;
 IF p.id IS NULL OR b.id IS NULL OR r.token IS NULL OR p.lease_epoch<>epoch OR (r.purpose='unit' AND r.lease_epoch<>epoch) THEN RAISE EXCEPTION 'family settlement owner unavailable'; END IF;
 IF r.purpose='unit' AND (p.state NOT IN ('preparing','running') OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()) THEN RAISE EXCEPTION 'family settlement lease invalid'; END IF;
 IF NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=org AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN RAISE EXCEPTION 'family settlement executor unavailable'; END IF;
 own_delta:=p.measured_bytes-p.retained_bytes;
 IF b.payer_plan_id=p.id THEN shared_delta:=b.shared_measured_bytes-b.shared_retained_bytes; END IF;
 actual:=own_delta+shared_delta;
 IF actual>r.byte_count-512 THEN RAISE EXCEPTION 'family reservation exceeded'; END IF;
 IF r.purpose='control' AND NOT release_control THEN
  refund:=actual;
  UPDATE migration_family_refresh_reservation SET byte_count=byte_count-actual WHERE token=reservation AND organization_id=org;
 ELSE
  refund:=r.byte_count;
  DELETE FROM migration_family_refresh_reservation WHERE token=reservation AND organization_id=org;
 END IF;
 UPDATE migration_family_refresh_plan SET retained_bytes=measured_bytes,reserved_bytes=reserved_bytes-refund WHERE id=plan AND organization_id=org;
 IF b.payer_plan_id=p.id THEN UPDATE migration_family_refresh_bundle SET shared_retained_bytes=shared_measured_bytes WHERE id=bundle AND organization_id=org; END IF;
 UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+actual,reserved_bytes=reserved_bytes-refund WHERE organization_id=org;
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET retained_bytes=retained_bytes+actual,reserved_bytes=reserved_bytes-refund WHERE id=p.source_snapshot_id AND organization_id=org; END IF;
 RETURN actual;
END $$;
REVOKE ALL ON FUNCTION crm_family_refresh_reserve(UUID,UUID,UUID,UUID,BIGINT,BIGINT,TEXT,BIGINT,BIGINT),crm_family_refresh_settle(UUID,UUID,UUID,UUID,BIGINT,BOOLEAN) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_reserve(UUID,UUID,UUID,UUID,BIGINT,BIGINT,TEXT,BIGINT,BIGINT),crm_family_refresh_settle(UUID,UUID,UUID,UUID,BIGINT,BOOLEAN) TO crm_app;

-- Application transactions cannot commit partially charged evidence or orphan
-- a reservation. Migrator-only synthetic fixture construction remains possible.
CREATE FUNCTION crm_family_refresh_accounting_complete() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v JSONB; b UUID; org UUID;
BEGIN
 IF current_user<>'crm_app' THEN RETURN NULL; END IF;
 v:=COALESCE(to_jsonb(NEW),to_jsonb(OLD)); org:=(v->>'organization_id')::uuid;
 b:=(v->>CASE WHEN TG_TABLE_NAME='migration_family_refresh_bundle' THEN 'id' ELSE 'bundle_id' END)::uuid;
 IF EXISTS(SELECT 1 FROM migration_family_refresh_bundle WHERE id=b AND organization_id=org AND (payer_plan_id IS NULL OR shared_measured_bytes<>shared_retained_bytes))
  OR EXISTS(SELECT 1 FROM migration_family_refresh_plan p WHERE p.bundle_id=b AND p.organization_id=org
    AND (p.measured_bytes<>p.retained_bytes OR p.reserved_bytes<>COALESCE((SELECT sum(r.byte_count) FROM migration_family_refresh_reservation r WHERE r.plan_id=p.id AND r.organization_id=p.organization_id),0))) THEN
  RAISE EXCEPTION 'family accounting transaction incomplete';
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER family_refresh_accounting_complete AFTER INSERT OR UPDATE ON migration_family_refresh_bundle DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_accounting_complete();
CREATE CONSTRAINT TRIGGER family_refresh_accounting_complete AFTER INSERT OR UPDATE ON migration_family_refresh_plan DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_accounting_complete();
CREATE CONSTRAINT TRIGGER family_refresh_accounting_complete AFTER INSERT OR UPDATE OR DELETE ON migration_family_refresh_reservation DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_accounting_complete();
REVOKE ALL ON FUNCTION crm_family_refresh_accounting_complete() FROM PUBLIC;

-- A takeover may refund an old unit reservation only after obtaining a newer
-- lease. Deferred accounting guarantees that the old transaction left no
-- uncharged writes behind. The durable cancellation reservation is untouched.
CREATE FUNCTION crm_family_refresh_reclaim(org UUID,bundle UUID,plan UUID,reservation UUID,epoch BIGINT) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; r migration_family_refresh_reservation;
BEGIN
 PERFORM crm_workspace_shared(org);
 PERFORM m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle owner ON owner.organization_id=m.organization_id AND owner.executor_user_id=m.user_id
  WHERE owner.id=bundle AND owner.organization_id=org AND m.role='admin' AND m.status='active' FOR SHARE OF m;
 IF NOT FOUND THEN RAISE EXCEPTION 'family executor unavailable'; END IF;
 PERFORM organization_id FROM migration_snapshot_storage WHERE organization_id=org FOR UPDATE;
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=plan AND bundle_id=bundle AND organization_id=org FOR UPDATE;
 SELECT * INTO r FROM migration_family_refresh_reservation WHERE token=reservation AND bundle_id=bundle AND plan_id=plan AND organization_id=org FOR UPDATE;
 IF p.id IS NULL OR b.id IS NULL OR p.lease_epoch<>epoch OR p.state NOT IN ('preparing','running')
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp() THEN RAISE EXCEPTION 'family reclaim lease invalid'; END IF;
 IF r.token IS NULL THEN RETURN false; END IF;
 IF r.purpose<>'unit' OR r.lease_epoch>=p.lease_epoch OR p.measured_bytes<>p.retained_bytes OR b.shared_measured_bytes<>b.shared_retained_bytes THEN RAISE EXCEPTION 'family reclaim evidence unsettled'; END IF;
 DELETE FROM migration_family_refresh_reservation WHERE token=reservation AND organization_id=org;
 UPDATE migration_family_refresh_plan SET reserved_bytes=reserved_bytes-r.byte_count WHERE id=plan AND organization_id=org;
 UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-r.byte_count WHERE organization_id=org;
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-r.byte_count WHERE id=p.source_snapshot_id AND organization_id=org; END IF;
 RETURN true;
END $$;
REVOKE ALL ON FUNCTION crm_family_refresh_reclaim(UUID,UUID,UUID,UUID,BIGINT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_reclaim(UUID,UUID,UUID,UUID,BIGINT) TO crm_app;
