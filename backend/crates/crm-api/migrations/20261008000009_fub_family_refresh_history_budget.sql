-- D-092: history-derived evidence shares the selected retained capture budget.
-- Existing evidence is charged once to its original capture; raw capture bytes
-- and the Organization ledger are already charged and must not be duplicated.
WITH charges AS (
 SELECT p.organization_id,p.history_capture_id,
  sum(p.retained_bytes+CASE WHEN b.payer_plan_id=p.id THEN b.shared_retained_bytes ELSE 0 END)::bigint AS retained,
  sum(p.reserved_bytes)::bigint AS reserved
 FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id
 WHERE p.source_snapshot_id IS NULL AND p.history_capture_id IS NOT NULL
 GROUP BY p.organization_id,p.history_capture_id
)
UPDATE migration_history_capture_run h SET retained_bytes=h.retained_bytes+c.retained,reserved_bytes=h.reserved_bytes+c.reserved
 FROM charges c WHERE h.id=c.history_capture_id AND h.organization_id=c.organization_id;

CREATE OR REPLACE FUNCTION crm_family_refresh_reserve(org UUID,bundle UUID,plan UUID,reservation UUID,epoch BIGINT,amount BIGINT,purpose TEXT,run_ceiling BIGINT,org_ceiling BIGINT) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
DECLARE b migration_family_refresh_bundle; p migration_family_refresh_plan; l migration_snapshot_storage; s migration_snapshot; h migration_history_capture_run; shared_bytes BIGINT:=0;
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
 ELSE
  SELECT * INTO h FROM migration_history_capture_run WHERE id=p.history_capture_id AND organization_id=org FOR UPDATE;
  IF h.id IS NULL THEN RAISE EXCEPTION 'family history source budget unavailable'; END IF;
  IF amount>LEAST(h.run_byte_limit,run_ceiling)-h.retained_bytes-h.reserved_bytes THEN RETURN false; END IF;
 END IF;
 INSERT INTO migration_family_refresh_reservation(token,organization_id,bundle_id,plan_id,lease_epoch,purpose,byte_count) VALUES(reservation,org,bundle,plan,epoch,purpose,amount);
 UPDATE migration_family_refresh_plan SET reserved_bytes=reserved_bytes+amount WHERE id=plan AND organization_id=org;
 UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+amount WHERE organization_id=org;
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+amount WHERE id=p.source_snapshot_id AND organization_id=org; ELSE UPDATE migration_history_capture_run SET reserved_bytes=reserved_bytes+amount WHERE id=p.history_capture_id AND organization_id=org; END IF;
 RETURN true;
END $$;

CREATE OR REPLACE FUNCTION crm_family_refresh_settle(org UUID,bundle UUID,plan UUID,reservation UUID,epoch BIGINT,release_control BOOLEAN) RETURNS BIGINT LANGUAGE plpgsql AS $$
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
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET retained_bytes=retained_bytes+actual,reserved_bytes=reserved_bytes-refund WHERE id=p.source_snapshot_id AND organization_id=org; ELSE UPDATE migration_history_capture_run SET retained_bytes=retained_bytes+actual,reserved_bytes=reserved_bytes-refund WHERE id=p.history_capture_id AND organization_id=org; END IF;
 RETURN actual;
END $$;

CREATE OR REPLACE FUNCTION crm_family_refresh_reclaim(org UUID,bundle UUID,plan UUID,reservation UUID,epoch BIGINT) RETURNS BOOLEAN LANGUAGE plpgsql AS $$
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
 IF p.source_snapshot_id IS NOT NULL THEN UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-r.byte_count WHERE id=p.source_snapshot_id AND organization_id=org; ELSE UPDATE migration_history_capture_run SET reserved_bytes=reserved_bytes-r.byte_count WHERE id=p.history_capture_id AND organization_id=org; END IF;
 RETURN true;
END $$;

CREATE OR REPLACE FUNCTION crm_family_history_erasure() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE r RECORD;
BEGIN
 IF OLD.erased_at IS NOT NULL OR NEW.erased_at IS NULL THEN RETURN NEW; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_history_display WHERE organization_id=NEW.organization_id AND identity_id=NEW.id AND nonce IS NOT NULL) THEN RETURN NEW; END IF;
 -- All retained displays were settled before this administrative erasure.
 -- Negative deltas refund their original plan and Org exactly once, including
 -- completed plans. The immutable fixed-width ownership survives suppression.
 PERFORM organization_id FROM migration_snapshot_storage WHERE organization_id=NEW.organization_id FOR UPDATE;
 FOR r IN SELECT d.plan_id,sum(octet_length(d.nonce)+octet_length(d.ciphertext)) AS bytes
  FROM migration_family_refresh_history_display d WHERE d.organization_id=NEW.organization_id AND d.identity_id=NEW.id AND d.nonce IS NOT NULL GROUP BY d.plan_id ORDER BY d.plan_id LOOP
  PERFORM id FROM migration_family_refresh_plan WHERE id=r.plan_id AND organization_id=NEW.organization_id AND measured_bytes=retained_bytes FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'history erasure evidence unsettled'; END IF;
  UPDATE migration_family_refresh_history_display SET nonce=NULL,ciphertext=NULL WHERE organization_id=NEW.organization_id AND identity_id=NEW.id AND plan_id=r.plan_id AND nonce IS NOT NULL;
  UPDATE migration_family_refresh_plan SET retained_bytes=retained_bytes-r.bytes WHERE id=r.plan_id AND organization_id=NEW.organization_id;
  UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes-r.bytes WHERE organization_id=NEW.organization_id;
  UPDATE migration_history_capture_run h SET retained_bytes=h.retained_bytes-r.bytes
   FROM migration_family_refresh_plan p WHERE p.id=r.plan_id AND p.organization_id=NEW.organization_id
    AND h.id=p.history_capture_id AND h.organization_id=p.organization_id;
 END LOOP;
 RETURN NEW;
END $$;
