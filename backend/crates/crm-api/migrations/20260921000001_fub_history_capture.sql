-- D-070: retained history evidence only. No native-domain privileges or triggers.
CREATE TABLE migration_history_capture_run (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL,
 parent_import_id UUID NOT NULL, parent_plan_id UUID NOT NULL, snapshot_id UUID NOT NULL,
 parent_capture_sequence BIGINT NOT NULL, workspace_revision BIGINT NOT NULL,
 connection_id UUID NOT NULL, connection_revision INTEGER NOT NULL,
 source_account_id BIGINT NOT NULL, source_user_id BIGINT NOT NULL CHECK(source_user_id>0),
 parent_source_user_id BIGINT, source_user_evidence_revision INTEGER NOT NULL,
 profile_version TEXT NOT NULL CHECK(octet_length(profile_version)<=64),
 parser_version TEXT NOT NULL CHECK(octet_length(parser_version)<=64),
 schema_version TEXT NOT NULL CHECK(octet_length(schema_version)<=128),
 initiated_by_user_id UUID NOT NULL REFERENCES app_user(id),
 state TEXT NOT NULL CHECK(state IN ('proposed','queued','running','waiting_retry','paused','completed_with_gaps','cancelled')),
 revision BIGINT NOT NULL DEFAULT 1, pause_reason TEXT CHECK(octet_length(pause_reason)<=80),
 proposal_expires_at TIMESTAMPTZ NOT NULL, confirmed_at TIMESTAMPTZ, started_at TIMESTAMPTZ,completed_at TIMESTAMPTZ,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(),updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 original_run_byte_limit BIGINT NOT NULL CHECK(original_run_byte_limit>0),
 run_byte_limit BIGINT NOT NULL CHECK(run_byte_limit>=original_run_byte_limit),budget_revision BIGINT NOT NULL DEFAULT 1,
 budget_policy_revision TEXT NOT NULL CHECK(octet_length(budget_policy_revision)<=80),
 raw_bytes BIGINT NOT NULL DEFAULT 0 CHECK(raw_bytes>=0),retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0), capture_sequence BIGINT NOT NULL DEFAULT 0,
 identity_verified_at TIMESTAMPTZ, identity_required BOOLEAN NOT NULL DEFAULT true, identity_attempts INTEGER NOT NULL DEFAULT 0,
 lease_token UUID,lease_expires_at TIMESTAMPTZ,next_attempt_at TIMESTAMPTZ,
 UNIQUE(id,organization_id),
 FOREIGN KEY(parent_import_id,snapshot_id,organization_id) REFERENCES migration_import(id,snapshot_id,organization_id),
 FOREIGN KEY(parent_plan_id,parent_import_id,organization_id) REFERENCES migration_import_plan(id,import_id,organization_id),
 FOREIGN KEY(connection_id,organization_id) REFERENCES migration_connection(id,organization_id),
 FOREIGN KEY(organization_id) REFERENCES migration_snapshot_storage(organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE UNIQUE INDEX migration_history_one_active ON migration_history_capture_run(organization_id) WHERE state IN ('queued','running','waiting_retry');
CREATE INDEX migration_history_claim ON migration_history_capture_run(created_at,id) WHERE state IN ('queued','running','waiting_retry');
CREATE INDEX migration_history_runs ON migration_history_capture_run(organization_id,created_at DESC,id DESC);
CREATE INDEX migration_history_parent_runs ON migration_history_capture_run(organization_id,parent_import_id,created_at DESC,id DESC);
CREATE TABLE migration_history_stream (
 run_id UUID NOT NULL,organization_id UUID NOT NULL,family TEXT NOT NULL CHECK(family IN ('events','calls','text_messages')),
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','enumerated','terminal_uncertain')),
 checkpoint BIGINT NOT NULL DEFAULT 0,cycle_attempts INTEGER NOT NULL DEFAULT 0, attempts BIGINT NOT NULL DEFAULT 0,
 cursor_nonce BYTEA CHECK(octet_length(cursor_nonce)=24),cursor_ciphertext BYTEA CHECK(octet_length(cursor_ciphertext)<=8192),
 reported_total TEXT CHECK(reported_total ~ '^(0|[1-9][0-9]{0,18})$'),
 occurrences BIGINT NOT NULL DEFAULT 0,valid_occurrences BIGINT NOT NULL DEFAULT 0,invalid_occurrences BIGINT NOT NULL DEFAULT 0,
 unique_ids BIGINT NOT NULL DEFAULT 0,equal_repeats BIGINT NOT NULL DEFAULT 0,conflicting_variants BIGINT NOT NULL DEFAULT 0,
 linked BIGINT NOT NULL DEFAULT 0,parent_excluded BIGINT NOT NULL DEFAULT 0,no_parent_identity BIGINT NOT NULL DEFAULT 0,
 invalid_person_reference BIGINT NOT NULL DEFAULT 0,conflicting_reference BIGINT NOT NULL DEFAULT 0,
 PRIMARY KEY(run_id,organization_id,family),FOREIGN KEY(run_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id)
);
CREATE TABLE migration_history_capture (
 id UUID PRIMARY KEY,run_id UUID NOT NULL,organization_id UUID NOT NULL,family TEXT NOT NULL,
 sequence BIGINT NOT NULL,checkpoint BIGINT NOT NULL,classification TEXT NOT NULL CHECK(classification IN ('identity','advancing','diagnostic')),
 http_status INTEGER NOT NULL,representation TEXT NOT NULL CHECK(octet_length(representation)<=128),
 source_version TEXT CHECK(octet_length(source_version)<=64),raw_byte_len BIGINT NOT NULL CHECK(raw_byte_len BETWEEN 0 AND 4194304),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=4194320),
 content_hmac BYTEA NOT NULL CHECK(octet_length(content_hmac)=32),truncated BOOLEAN NOT NULL,
 captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(id,run_id,organization_id),UNIQUE(run_id,organization_id,sequence),
 FOREIGN KEY(run_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id)
);
CREATE UNIQUE INDEX migration_history_advancing_checkpoint ON migration_history_capture(run_id,organization_id,family,checkpoint) WHERE classification='advancing';
CREATE TABLE migration_history_observation (
 id UUID PRIMARY KEY,run_id UUID NOT NULL,organization_id UUID NOT NULL,capture_id UUID NOT NULL,
 capture_sequence BIGINT NOT NULL,ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 99),family TEXT NOT NULL,
 identity_hmac BYTEA CHECK(octet_length(identity_hmac)=32),semantic_hmac BYTEA NOT NULL CHECK(octet_length(semantic_hmac)=32),
 primary_person_hmac BYTEA CHECK(octet_length(primary_person_hmac)=32),person_id UUID,
 disposition TEXT NOT NULL CHECK(disposition IN ('linked','parent_excluded','no_parent_identity','invalid_person_reference')),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=16400),
 UNIQUE(id,run_id,organization_id),UNIQUE(capture_id,ordinal),
 FOREIGN KEY(capture_id,run_id,organization_id) REFERENCES migration_history_capture(id,run_id,organization_id)
);
CREATE INDEX migration_history_observation_page ON migration_history_observation(run_id,organization_id,capture_sequence,ordinal,id);
CREATE INDEX migration_history_family_page ON migration_history_observation(run_id,organization_id,family,capture_sequence,ordinal,id);
CREATE INDEX migration_history_link_page ON migration_history_observation(run_id,organization_id,disposition,family,capture_sequence,ordinal,id);
CREATE INDEX migration_history_identity_page ON migration_history_observation(run_id,organization_id,family,identity_hmac,capture_sequence,ordinal,id);
CREATE TABLE migration_history_identity (
 run_id UUID NOT NULL,organization_id UUID NOT NULL,family TEXT NOT NULL,identity_hmac BYTEA NOT NULL CHECK(octet_length(identity_hmac)=32),
 primary_person_hmac BYTEA CHECK(octet_length(primary_person_hmac)=32),
 disposition TEXT NOT NULL CHECK(disposition IN ('linked','parent_excluded','no_parent_identity','invalid_person_reference','conflicting_reference')),
 PRIMARY KEY(run_id,organization_id,family,identity_hmac),
 FOREIGN KEY(run_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id)
);
CREATE TABLE migration_history_person_link (
 observation_id UUID NOT NULL,run_id UUID NOT NULL,organization_id UUID NOT NULL,
 source_person_hmac BYTEA NOT NULL CHECK(octet_length(source_person_hmac)=32),person_id UUID,
 PRIMARY KEY(observation_id,source_person_hmac),
 FOREIGN KEY(observation_id,run_id,organization_id) REFERENCES migration_history_observation(id,run_id,organization_id)
);
CREATE INDEX migration_history_person_erasure ON migration_history_person_link(organization_id,person_id) WHERE person_id IS NOT NULL;
CREATE TABLE migration_history_seen (
 run_id UUID NOT NULL,organization_id UUID NOT NULL,family TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('page','token')),
 digest BYTEA NOT NULL CHECK(octet_length(digest)=32),PRIMARY KEY(run_id,organization_id,family,kind,digest),
 FOREIGN KEY(run_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id)
);
CREATE TABLE migration_history_reservation (
 token UUID PRIMARY KEY,run_id UUID NOT NULL,organization_id UUID NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('control','source')),
 byte_count BIGINT NOT NULL CHECK(byte_count>0),expires_at TIMESTAMPTZ,
 UNIQUE(run_id,organization_id,kind),FOREIGN KEY(run_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id)
);
CREATE TABLE migration_history_receipt (
 organization_id UUID NOT NULL,request_id UUID NOT NULL,run_id UUID NOT NULL,actor_user_id UUID NOT NULL,
 operation TEXT NOT NULL CHECK(octet_length(operation)<=32),digest BYTEA NOT NULL CHECK(octet_length(digest)=32),
 nonce BYTEA NOT NULL CHECK(octet_length(nonce)=24),ciphertext BYTEA NOT NULL CHECK(octet_length(ciphertext)<=4112),
 PRIMARY KEY(organization_id,request_id),FOREIGN KEY(run_id,organization_id) REFERENCES migration_history_capture_run(id,organization_id)
);
GRANT SELECT,INSERT,UPDATE ON migration_history_capture_run,migration_history_stream,migration_history_identity TO crm_app;
GRANT SELECT,INSERT ON migration_history_capture,migration_history_observation,migration_history_person_link,migration_history_seen,migration_history_receipt TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_history_reservation TO crm_app;
