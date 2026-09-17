-- D-092: typed, append-only corrections. This storage stage deliberately grants
-- no application writes; execution admission is enabled with the version-aware
-- reader/worker stage. Existing imports and their first owners are untouched.
CREATE TABLE migration_family_refresh_history_head (
 organization_id UUID NOT NULL, identity_id UUID NOT NULL,
 person_id UUID NOT NULL, family TEXT NOT NULL CHECK(family IN ('events','calls','text_messages')),
 original_fact_id UUID NOT NULL,
 plan_id UUID NOT NULL, bundle_id UUID NOT NULL,
 version BIGINT NOT NULL DEFAULT 1 CHECK(version>0), version_id UUID, result_id UUID,
 capture_id UUID NOT NULL, semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 source_created_at TIMESTAMPTZ,
 PRIMARY KEY(organization_id,identity_id),
 FOREIGN KEY(identity_id,organization_id) REFERENCES migration_history_import_identity(id,organization_id),
 FOREIGN KEY(plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_plan(id,bundle_id,organization_id),
 FOREIGN KEY(capture_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id),
 FOREIGN KEY(result_id,organization_id) REFERENCES migration_family_refresh_result(id,organization_id),
 CHECK((version=1 AND version_id IS NULL AND result_id IS NULL) OR (version>1 AND version_id IS NOT NULL AND result_id IS NOT NULL))
);
CREATE INDEX family_refresh_history_known ON migration_family_refresh_history_head(organization_id,person_id,family,source_created_at DESC,identity_id DESC) WHERE source_created_at IS NOT NULL;
CREATE INDEX family_refresh_history_unknown ON migration_family_refresh_history_head(organization_id,person_id,family,identity_id DESC) WHERE source_created_at IS NULL;

-- Ciphertext is deletable; immutable facts contain only provenance and hashes.
CREATE TABLE migration_family_refresh_history_display (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL, identity_id UUID NOT NULL,
 plan_id UUID NOT NULL, bundle_id UUID NOT NULL, result_id UUID NOT NULL,
 nonce BYTEA CHECK(octet_length(nonce)=24), ciphertext BYTEA CHECK(octet_length(ciphertext) BETWEEN 16 AND 4112),
 UNIQUE(id,identity_id,organization_id), UNIQUE(result_id,organization_id),
 FOREIGN KEY(identity_id,organization_id) REFERENCES migration_history_import_identity(id,organization_id),
 FOREIGN KEY(result_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_result(id,plan_id,bundle_id,organization_id),
 CHECK((nonce IS NULL)=(ciphertext IS NULL))
);
CREATE INDEX family_refresh_history_display_identity ON migration_family_refresh_history_display(organization_id,identity_id,plan_id) WHERE nonce IS NOT NULL;
DO $$ DECLARE stem TEXT; BEGIN
 FOREACH stem IN ARRAY ARRAY['event','call','text'] LOOP
  EXECUTE format('ALTER TABLE fub_%s_record_imported ADD UNIQUE(id,identity_id,organization_id)',stem);
  EXECUTE format('CREATE TABLE fub_%s_record_corrected (
   id UUID PRIMARY KEY, organization_id UUID NOT NULL, identity_id UUID NOT NULL,
   original_fact_id UUID NOT NULL, person_id UUID NOT NULL,
   actor_kind TEXT NOT NULL CHECK(actor_kind IN (''user'',''system'')), actor_user_id UUID REFERENCES app_user(id),
   on_behalf_of_user_id UUID REFERENCES app_user(id), origin TEXT NOT NULL CHECK(origin=''migration''),
   occurred_at TIMESTAMPTZ NOT NULL, recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
   correlation_id UUID NOT NULL, causation_id UUID,
   corrects_id UUID, version BIGINT NOT NULL CHECK(version>=2),
   plan_id UUID NOT NULL, bundle_id UUID NOT NULL, result_id UUID NOT NULL, manifest_id UUID NOT NULL,
   capture_id UUID NOT NULL, semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
   source_created_at TIMESTAMPTZ, source_time_basis TEXT NOT NULL CHECK(source_time_basis IN (''fub_record_created'',''unknown'')),
   CHECK((source_created_at IS NULL)=(source_time_basis=''unknown'')),
   CHECK((actor_kind=''user'')=(actor_user_id IS NOT NULL)),
   CHECK((version=2)=(corrects_id IS NULL)),
   UNIQUE(id,identity_id,organization_id), UNIQUE(organization_id,identity_id,version), UNIQUE(result_id,organization_id),
   FOREIGN KEY(original_fact_id,identity_id,organization_id) REFERENCES fub_%s_record_imported(id,identity_id,organization_id),
   FOREIGN KEY(corrects_id,identity_id,organization_id) REFERENCES fub_%s_record_corrected(id,identity_id,organization_id),
   FOREIGN KEY(id,identity_id,organization_id) REFERENCES migration_family_refresh_history_display(id,identity_id,organization_id),
   FOREIGN KEY(result_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_result(id,plan_id,bundle_id,organization_id),
   FOREIGN KEY(manifest_id,plan_id,bundle_id,organization_id) REFERENCES migration_family_refresh_manifest(id,plan_id,bundle_id,organization_id),
   FOREIGN KEY(capture_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id)
  )',stem,stem,stem);
  EXECUTE format('CREATE TRIGGER history_correction_immutable BEFORE UPDATE OR DELETE ON fub_%s_record_corrected FOR EACH ROW EXECUTE FUNCTION reject_mutation()',stem);
  EXECUTE format('CREATE TRIGGER history_correction_no_truncate BEFORE TRUNCATE ON fub_%s_record_corrected FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation()',stem);
 END LOOP;
END $$;

-- Initial projection can only copy exact immutable first-import provenance.
CREATE FUNCTION crm_family_history_head_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE i migration_history_import_identity; f RECORD; p migration_family_refresh_plan; table_name TEXT; v RECORD;
BEGIN
 SELECT * INTO i FROM migration_history_import_identity WHERE id=NEW.identity_id AND organization_id=NEW.organization_id FOR UPDATE;
 IF i.id IS NULL OR i.erased_at IS NOT NULL OR i.person_id IS DISTINCT FROM NEW.person_id OR i.fact_id IS DISTINCT FROM NEW.original_fact_id OR i.family<>NEW.family
  OR NOT EXISTS(SELECT 1 FROM person WHERE id=NEW.person_id AND organization_id=NEW.organization_id) THEN RAISE EXCEPTION 'history head identity unavailable'; END IF;
 table_name:=CASE i.family WHEN 'events' THEN 'fub_event_record' WHEN 'calls' THEN 'fub_call_record' WHEN 'text_messages' THEN 'fub_text_record' END;
 IF TG_OP='INSERT' THEN
  SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=NEW.bundle_id AND organization_id=NEW.organization_id;
  IF p.id IS NULL OR p.family<>'history' OR NEW.version<>1 OR NEW.semantic_hmac<>i.semantic_hmac THEN RAISE EXCEPTION 'invalid history bootstrap'; END IF;
  EXECUTE format('SELECT f.person_id,f.source_created_at,COALESCE(p.capture_id,a.history_capture_id) AS capture_id FROM %I f
   JOIN migration_history_import_display d ON d.id=COALESCE(f.manifest_id,f.admitted_manifest_id) AND d.organization_id=f.organization_id
   LEFT JOIN migration_history_import_plan p ON p.id=f.plan_id AND p.organization_id=f.organization_id
   LEFT JOIN migration_admitted_history_root a ON a.id=f.admitted_root_id AND a.organization_id=f.organization_id
   WHERE f.id=$1 AND f.identity_id=$2 AND f.organization_id=$3 AND
    ((f.attempt_id=$4 AND f.admitted_root_id IS NULL AND d.owner_run_id=$4 AND d.plan_id=f.plan_id)
     OR (f.attempt_id IS NULL AND f.admitted_root_id=$5 AND f.admitted_plan_id=$6 AND f.admitted_attempt_id=$7 AND f.admitted_manifest_id=$8
      AND d.admitted_root_id=$5 AND d.admitted_plan_id=$6 AND d.admitted_attempt_id=$7))',table_name||'_imported')
   INTO f USING i.fact_id,i.id,i.organization_id,i.owner_run_id,i.admitted_root_id,i.admitted_plan_id,i.admitted_attempt_id,i.admitted_manifest_id;
  IF f.capture_id IS NULL OR f.capture_id IS DISTINCT FROM NEW.capture_id OR f.person_id IS DISTINCT FROM NEW.person_id OR f.source_created_at IS DISTINCT FROM NEW.source_created_at THEN RAISE EXCEPTION 'unproven history bootstrap'; END IF;
  IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_bundle b JOIN migration_history_capture_run c ON c.parent_import_id=b.parent_import_id AND c.source_account_id=b.source_account_id AND c.organization_id=b.organization_id WHERE b.id=NEW.bundle_id AND b.organization_id=NEW.organization_id AND c.id=NEW.capture_id) THEN RAISE EXCEPTION 'foreign history bootstrap'; END IF;
 ELSE
  IF to_jsonb(NEW)-ARRAY['version','version_id','result_id','capture_id','semantic_hmac','source_created_at'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['version','version_id','result_id','capture_id','semantic_hmac','source_created_at']
   OR NEW.version<>OLD.version+1 THEN RAISE EXCEPTION 'history head ownership immutable'; END IF;
  EXECUTE format('SELECT * FROM %I WHERE id=$1 AND identity_id=$2 AND organization_id=$3',table_name||'_corrected') INTO v USING NEW.version_id,NEW.identity_id,NEW.organization_id;
  IF v.id IS NULL OR v.corrects_id IS DISTINCT FROM OLD.version_id OR v.version<>NEW.version OR v.result_id IS DISTINCT FROM NEW.result_id OR v.capture_id IS DISTINCT FROM NEW.capture_id OR v.semantic_hmac IS DISTINCT FROM NEW.semantic_hmac OR v.source_created_at IS DISTINCT FROM NEW.source_created_at THEN RAISE EXCEPTION 'history head predecessor mismatch'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER history_head_guard BEFORE INSERT OR UPDATE ON migration_family_refresh_history_head FOR EACH ROW EXECUTE FUNCTION crm_family_history_head_guard();
CREATE TRIGGER history_head_no_delete BEFORE DELETE ON migration_family_refresh_history_head FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER history_head_no_truncate BEFORE TRUNCATE ON migration_family_refresh_history_head FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE FUNCTION crm_family_history_display_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  IF NEW.nonce IS NULL OR NOT EXISTS(SELECT 1 FROM migration_history_import_identity WHERE id=NEW.identity_id AND organization_id=NEW.organization_id AND erased_at IS NULL AND fact_id IS NOT NULL) THEN RAISE EXCEPTION 'history display identity unavailable'; END IF;
 ELSE
  IF to_jsonb(NEW)-ARRAY['nonce','ciphertext'] IS DISTINCT FROM to_jsonb(OLD)-ARRAY['nonce','ciphertext'] OR OLD.nonce IS NULL OR NEW.nonce IS NOT NULL
   OR NOT EXISTS(SELECT 1 FROM migration_history_import_identity WHERE id=NEW.identity_id AND organization_id=NEW.organization_id AND erased_at IS NOT NULL) THEN RAISE EXCEPTION 'history display only erasable'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER history_display_guard BEFORE INSERT OR UPDATE ON migration_family_refresh_history_display FOR EACH ROW EXECUTE FUNCTION crm_family_history_display_guard();
CREATE TRIGGER history_display_no_delete BEFORE DELETE ON migration_family_refresh_history_display FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER history_display_no_truncate BEFORE TRUNCATE ON migration_family_refresh_history_display FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

-- Validate and serialize the successor against the current identity. The first
-- owner is never rewritten. Both the raw occurrence and frozen unit must agree.
CREATE FUNCTION crm_family_history_version_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE h migration_family_refresh_history_head; i migration_history_import_identity; r migration_family_refresh_result; u migration_family_refresh_manifest;
 p migration_family_refresh_plan; b migration_family_refresh_bundle; s migration_family_refresh_source; v_kind TEXT; v_family TEXT;
BEGIN
 PERFORM crm_workspace_shared(NEW.organization_id);
 SELECT * INTO b FROM migration_family_refresh_bundle WHERE id=NEW.bundle_id AND organization_id=NEW.organization_id FOR SHARE;
 SELECT * INTO p FROM migration_family_refresh_plan WHERE id=NEW.plan_id AND bundle_id=b.id AND organization_id=NEW.organization_id FOR SHARE;
 IF p.id IS NULL OR p.family<>'history' OR p.state<>'running' OR p.confirmed_at IS NULL OR b.confirmed_at IS NULL OR b.state NOT IN ('queued','running','paused')
  OR p.lease_token::text IS DISTINCT FROM current_setting('crm.family_refresh_lease',true) OR p.lease_expires_at IS NULL OR p.lease_expires_at<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM migration_family_refresh_requirement WHERE organization_id=NEW.organization_id)
  OR NOT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=NEW.organization_id AND user_id=b.executor_user_id AND role='admin' AND status='active') THEN RAISE EXCEPTION 'history correction lease invalid'; END IF;
 SELECT * INTO i FROM migration_history_import_identity WHERE id=NEW.identity_id AND organization_id=NEW.organization_id FOR UPDATE;
 SELECT * INTO h FROM migration_family_refresh_history_head WHERE identity_id=NEW.identity_id AND organization_id=NEW.organization_id FOR UPDATE;
 v_kind:=CASE TG_TABLE_NAME WHEN 'fub_event_record_corrected' THEN 'event' WHEN 'fub_call_record_corrected' THEN 'call' ELSE 'text' END;
 v_family:=CASE v_kind WHEN 'event' THEN 'events' WHEN 'call' THEN 'calls' ELSE 'text_messages' END;
 IF i.id IS NULL OR i.erased_at IS NOT NULL OR h.identity_id IS NULL OR h.family<>v_family OR h.person_id<>NEW.person_id OR h.original_fact_id<>NEW.original_fact_id
  OR NEW.corrects_id IS DISTINCT FROM h.version_id OR NEW.version<>h.version+1 OR NEW.semantic_hmac=h.semantic_hmac THEN RAISE EXCEPTION 'history correction predecessor invalid'; END IF;
 SELECT * INTO r FROM migration_family_refresh_result WHERE id=NEW.result_id AND organization_id=NEW.organization_id;
 SELECT * INTO u FROM migration_family_refresh_manifest WHERE id=NEW.manifest_id AND plan_id=p.id AND organization_id=NEW.organization_id;
 SELECT * INTO s FROM migration_family_refresh_source WHERE id=u.source_row_id AND plan_id=p.id AND organization_id=NEW.organization_id;
 IF r.id IS NULL OR u.id IS NULL OR s.id IS NULL OR r.manifest_id<>u.id OR r.disposition<>'applied' OR r.person_id IS DISTINCT FROM NEW.person_id OR r.target_id IS DISTINCT FROM NEW.original_fact_id
  OR u.disposition<>'correction' OR u.kind<>v_kind OR u.person_id IS DISTINCT FROM NEW.person_id OR u.target_id IS DISTINCT FROM NEW.original_fact_id OR u.expected_head_id IS DISTINCT FROM h.result_id
  OR u.source_key_hmac<>i.identity_hmac OR NOT s.qualified OR s.kind<>v_kind OR s.identity_hmac IS DISTINCT FROM i.identity_hmac OR s.semantic_hmac<>NEW.semantic_hmac
  OR NEW.capture_id IS DISTINCT FROM p.history_capture_id THEN RAISE EXCEPTION 'history correction proof invalid'; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_cohort c JOIN person person ON person.id=c.person_id AND person.organization_id=c.organization_id
  JOIN migration_import_identity pi ON pi.organization_id=c.organization_id AND pi.target_id=c.person_id AND pi.family='people' AND pi.source_account_id=b.source_account_id AND pi.source_id=c.source_person_id
  WHERE c.id=u.cohort_id AND c.bundle_id=b.id AND c.organization_id=NEW.organization_id AND c.person_id=NEW.person_id AND c.source_person_id=s.source_person_id) THEN RAISE EXCEPTION 'history correction Person unavailable'; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_history_capture raw JOIN migration_history_capture_run selected ON selected.id=raw.run_id AND selected.organization_id=raw.organization_id
  JOIN migration_history_capture_run prior ON prior.id=h.capture_id AND prior.organization_id=selected.organization_id
  WHERE raw.id=s.capture_id AND raw.organization_id=NEW.organization_id AND raw.sequence=s.capture_sequence AND raw.run_id=NEW.capture_id AND raw.family=v_family AND raw.classification='advancing' AND NOT raw.truncated
   AND raw.representation=s.representation AND selected.state='completed_with_gaps' AND selected.completed_at IS NOT NULL AND prior.completed_at IS NOT NULL AND selected.started_at>prior.completed_at
   AND selected.source_account_id=prior.source_account_id AND selected.parent_import_id=prior.parent_import_id) THEN RAISE EXCEPTION 'history correction source not newer'; END IF;
 IF EXISTS(SELECT 1 FROM migration_family_refresh_source sibling WHERE sibling.plan_id=p.id AND sibling.organization_id=NEW.organization_id AND sibling.kind=v_kind AND sibling.identity_hmac=i.identity_hmac
  AND (NOT sibling.qualified OR sibling.semantic_hmac<>s.semantic_hmac OR sibling.source_person_id IS DISTINCT FROM s.source_person_id OR sibling.representation<>s.representation)) THEN RAISE EXCEPTION 'history correction source conflict'; END IF;
 IF NOT EXISTS(SELECT 1 FROM migration_family_refresh_history_display d WHERE d.id=NEW.id AND d.organization_id=NEW.organization_id AND d.identity_id=NEW.identity_id AND d.result_id=NEW.result_id AND d.plan_id=NEW.plan_id AND d.bundle_id=NEW.bundle_id AND d.nonce IS NOT NULL) THEN RAISE EXCEPTION 'history correction display unavailable'; END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION crm_family_history_version_apply() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE h migration_family_refresh_history_head; key TEXT;
BEGIN
 SELECT * INTO h FROM migration_family_refresh_history_head WHERE organization_id=NEW.organization_id AND identity_id=NEW.identity_id FOR UPDATE;
 UPDATE migration_family_refresh_history_head SET version=NEW.version,version_id=NEW.id,result_id=NEW.result_id,capture_id=NEW.capture_id,semantic_hmac=NEW.semantic_hmac,source_created_at=NEW.source_created_at
 WHERE organization_id=NEW.organization_id AND identity_id=NEW.identity_id AND version=NEW.version-1 AND version_id IS NOT DISTINCT FROM NEW.corrects_id;
 IF NOT FOUND THEN RAISE EXCEPTION 'history correction lost predecessor'; END IF;
 key:=replace(TG_TABLE_NAME,'_corrected','_imported');
 IF (h.source_created_at IS NULL)<>(NEW.source_created_at IS NULL) THEN
  PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,key||CASE WHEN h.source_created_at IS NULL THEN '_unknown' ELSE '_known' END,-1);
  PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,key||CASE WHEN NEW.source_created_at IS NULL THEN '_unknown' ELSE '_known' END,1);
 ELSE
  PERFORM crm_history_review_delta(NEW.organization_id,NEW.person_id,NULL,0);
 END IF;
 RETURN NEW;
END $$;
DO $$ DECLARE stem TEXT; BEGIN
 FOREACH stem IN ARRAY ARRAY['event','call','text'] LOOP
  EXECUTE format('CREATE TRIGGER history_version_guard BEFORE INSERT ON fub_%s_record_corrected FOR EACH ROW EXECUTE FUNCTION crm_family_history_version_guard()',stem);
  EXECUTE format('CREATE TRIGGER history_version_apply AFTER INSERT ON fub_%s_record_corrected FOR EACH ROW EXECUTE FUNCTION crm_family_history_version_apply()',stem);
 END LOOP;
END $$;

-- Erasure decrements the current date bucket, not the superseded initial one.
DO $$ DECLARE name TEXT; definition TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['crm_history_import_erased_identity()','crm_history_import_display_change()'] LOOP
  SELECT pg_get_functiondef(name::regprocedure) INTO definition;
  IF position('SELECT f.person_id,f.source_created_at FROM %I f' IN definition)=0 THEN RAISE EXCEPTION 'unexpected history erasure definition'; END IF;
  definition:=replace(definition,'SELECT f.person_id,f.source_created_at FROM %I f','SELECT f.person_id,CASE WHEN h.identity_id IS NOT NULL THEN h.source_created_at ELSE f.source_created_at END AS source_created_at FROM %I f LEFT JOIN migration_family_refresh_history_head h ON h.identity_id=f.identity_id AND h.organization_id=f.organization_id');
  EXECUTE definition;
 END LOOP;
END $$;

-- Add every new variable-width column to the shared exact inventory.
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_functiondef('crm_family_refresh_retained_size(jsonb)'::regprocedure) INTO definition;
 EXECUTE replace(definition,'''operation'',''counts'',''results''','''operation'',''counts'',''results'',''actor_kind'',''origin'',''source_time_basis''');
END $$;
DO $$ DECLARE name TEXT; BEGIN
 FOREACH name IN ARRAY ARRAY['migration_family_refresh_history_head','migration_family_refresh_history_display','fub_event_record_corrected','fub_call_record_corrected','fub_text_record_corrected'] LOOP
  EXECUTE format('CREATE TRIGGER family_refresh_measure AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION crm_family_refresh_measure()',name);
  EXECUTE format('REVOKE ALL ON %I FROM crm_app',name);
  EXECUTE format('GRANT SELECT ON %I TO crm_app',name);
 END LOOP;
END $$;

CREATE FUNCTION crm_family_history_erasure() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
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
 END LOOP;
 RETURN NEW;
END $$;
CREATE TRIGGER family_history_erasure AFTER UPDATE ON migration_history_import_identity FOR EACH ROW EXECUTE FUNCTION crm_family_history_erasure();
REVOKE ALL ON FUNCTION crm_family_history_head_guard(),crm_family_history_display_guard(),crm_family_history_version_guard(),crm_family_history_version_apply(),crm_family_history_erasure() FROM PUBLIC;
