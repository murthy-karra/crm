-- D-063: additive, encrypted read-only source snapshots and frozen previews.
CREATE TABLE migration_snapshot_storage (
 organization_id UUID PRIMARY KEY REFERENCES organization(id),
 byte_limit BIGINT NOT NULL CHECK(byte_limit>0), budget_revision BIGINT NOT NULL DEFAULT 1,
 budget_policy_revision TEXT NOT NULL DEFAULT 'v1-2147483648-4294967296',
 retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0)
);
CREATE TABLE migration_snapshot (
 id UUID PRIMARY KEY, organization_id UUID NOT NULL, connection_id UUID NOT NULL,
 connection_revision INTEGER NOT NULL, source_account_id BIGINT NOT NULL,
 profile_version TEXT NOT NULL, schema_version TEXT NOT NULL,
 initiated_by_user_id UUID NOT NULL REFERENCES app_user(id),
 state TEXT NOT NULL CHECK(state IN ('proposed','queued','running','waiting_retry','paused','completed','completed_with_gaps','cancelled','expired')),
 pause_reason TEXT, proposal_expires_at TIMESTAMPTZ NOT NULL,
 original_run_byte_limit BIGINT NOT NULL CHECK(original_run_byte_limit>0),
 run_byte_limit BIGINT NOT NULL CHECK(run_byte_limit>=original_run_byte_limit), budget_revision BIGINT NOT NULL DEFAULT 1,
 budget_policy_revision TEXT NOT NULL DEFAULT 'v1-2147483648-4294967296',
 raw_bytes BIGINT NOT NULL DEFAULT 0 CHECK(raw_bytes>=0), retained_bytes BIGINT NOT NULL DEFAULT 0 CHECK(retained_bytes>=0),
 reserved_bytes BIGINT NOT NULL DEFAULT 0 CHECK(reserved_bytes>=0),
 capture_sequence BIGINT NOT NULL DEFAULT 0, accepted_captures BIGINT NOT NULL DEFAULT 0,
 identity_required BOOLEAN NOT NULL DEFAULT true, source_session UUID,
 next_attempt_at TIMESTAMPTZ, lease_token UUID, lease_expires_at TIMESTAMPTZ,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), confirmed_at TIMESTAMPTZ, started_at TIMESTAMPTZ, completed_at TIMESTAMPTZ, updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(id,organization_id),
 FOREIGN KEY(connection_id,organization_id) REFERENCES migration_connection(id,organization_id),
 FOREIGN KEY(organization_id) REFERENCES migration_snapshot_storage(organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE UNIQUE INDEX migration_snapshot_one_active ON migration_snapshot(organization_id) WHERE state IN ('queued','running','waiting_retry');
CREATE INDEX migration_snapshot_claim ON migration_snapshot(next_attempt_at,created_at) WHERE state IN ('queued','running','waiting_retry');
CREATE INDEX migration_snapshot_latest ON migration_snapshot(organization_id,created_at DESC,id DESC);
CREATE INDEX migration_snapshot_completed ON migration_snapshot(organization_id,created_at DESC,id DESC) WHERE state IN ('completed','completed_with_gaps');
CREATE TABLE migration_snapshot_stream (
 snapshot_id UUID NOT NULL, organization_id UUID NOT NULL, stream TEXT NOT NULL, family TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending', cursor_nonce BYTEA, cursor_ciphertext BYTEA,
 checkpoint BIGINT NOT NULL DEFAULT 0, attempts BIGINT NOT NULL DEFAULT 0, cycle_attempts INTEGER NOT NULL DEFAULT 0,
 returned_items BIGINT NOT NULL DEFAULT 0, accepted_captures BIGINT NOT NULL DEFAULT 0, content_gaps BIGINT NOT NULL DEFAULT 0,
 reported_total TEXT, error_code TEXT, observed_at TIMESTAMPTZ,
 PRIMARY KEY(snapshot_id,organization_id,stream),
 FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id)
);
CREATE TABLE migration_snapshot_capture (
 id UUID PRIMARY KEY, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL, stream TEXT NOT NULL,
 sequence BIGINT NOT NULL, checkpoint BIGINT NOT NULL, request_fingerprint BYTEA NOT NULL,
 representation TEXT NOT NULL, http_status INTEGER NOT NULL, raw_byte_len BIGINT NOT NULL,
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, source_version TEXT,
 classification TEXT NOT NULL, truncated BOOLEAN NOT NULL, accepted BOOLEAN NOT NULL,
 captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(snapshot_id,organization_id,sequence), UNIQUE(id,snapshot_id,organization_id),
 FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id)
);
CREATE UNIQUE INDEX migration_snapshot_settled_checkpoint ON migration_snapshot_capture(snapshot_id,organization_id,stream,checkpoint) WHERE accepted;
CREATE INDEX migration_snapshot_request_lookup ON migration_snapshot_capture(snapshot_id,organization_id,stream,request_fingerprint) WHERE accepted;
CREATE TABLE migration_snapshot_record (
 id UUID PRIMARY KEY, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL, capture_id UUID NOT NULL,
 capture_sequence BIGINT NOT NULL, ordinal INTEGER NOT NULL, family TEXT NOT NULL, source_id TEXT,
 representation TEXT NOT NULL, semantic_hmac BYTEA NOT NULL,
 projection_nonce BYTEA NOT NULL, projection_ciphertext BYTEA NOT NULL, content_gap BOOLEAN NOT NULL,
 UNIQUE(capture_id,ordinal), UNIQUE(id,snapshot_id,organization_id),
 FOREIGN KEY(capture_id,snapshot_id,organization_id) REFERENCES migration_snapshot_capture(id,snapshot_id,organization_id)
);
CREATE INDEX migration_snapshot_record_page ON migration_snapshot_record(snapshot_id,organization_id,family,source_id,capture_sequence);
CREATE INDEX migration_snapshot_record_boundary ON migration_snapshot_record(snapshot_id,organization_id,capture_sequence);
CREATE TABLE migration_snapshot_contact_key (
 snapshot_id UUID NOT NULL, organization_id UUID NOT NULL, record_id UUID NOT NULL,
 capture_sequence BIGINT NOT NULL, source_id TEXT NOT NULL, kind TEXT NOT NULL, key_hmac BYTEA NOT NULL,
 PRIMARY KEY(record_id,kind,key_hmac),
 FOREIGN KEY(record_id,snapshot_id,organization_id) REFERENCES migration_snapshot_record(id,snapshot_id,organization_id)
);
CREATE INDEX migration_snapshot_contact_group ON migration_snapshot_contact_key(snapshot_id,organization_id,kind,key_hmac,source_id,capture_sequence);
CREATE INDEX migration_snapshot_contact_source ON migration_snapshot_contact_key(snapshot_id,organization_id,source_id,capture_sequence,kind,key_hmac);
CREATE TABLE migration_snapshot_note_detail (
 snapshot_id UUID NOT NULL, organization_id UUID NOT NULL, source_id TEXT NOT NULL,
 ordinal BIGINT GENERATED ALWAYS AS IDENTITY, settled BOOLEAN NOT NULL DEFAULT false,
 PRIMARY KEY(snapshot_id,organization_id,source_id),
 FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id)
);
CREATE INDEX migration_snapshot_note_pending ON migration_snapshot_note_detail(snapshot_id,organization_id,ordinal) WHERE NOT settled;
CREATE TABLE migration_snapshot_preview (
 id UUID PRIMARY KEY, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 requested_by_user_id UUID NOT NULL REFERENCES app_user(id), engine_version TEXT NOT NULL, input_version TEXT NOT NULL,
 capture_sequence BIGINT NOT NULL, input_nonce BYTEA NOT NULL, input_ciphertext BYTEA NOT NULL, destination_fingerprint BYTEA NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('queued','running','paused','completed','failed')),
 pause_reason TEXT, checkpoint_family TEXT NOT NULL DEFAULT '', checkpoint_source_id TEXT NOT NULL DEFAULT '',
 groups_built BOOLEAN NOT NULL DEFAULT false, checkpoint_group_kind TEXT NOT NULL DEFAULT '', checkpoint_group_hash BYTEA NOT NULL DEFAULT ''::bytea, lease_token UUID, lease_expires_at TIMESTAMPTZ,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), completed_at TIMESTAMPTZ, input_observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(id,snapshot_id,organization_id),
 FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 CHECK((lease_token IS NULL)=(lease_expires_at IS NULL))
);
CREATE INDEX migration_snapshot_preview_latest ON migration_snapshot_preview(snapshot_id,organization_id,created_at DESC,id DESC);
CREATE INDEX migration_snapshot_preview_claim ON migration_snapshot_preview(created_at) WHERE state IN ('queued','running');
CREATE TABLE migration_snapshot_reservation (
 token UUID PRIMARY KEY, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL, preview_id UUID,
 byte_count BIGINT NOT NULL CHECK(byte_count>0), expires_at TIMESTAMPTZ NOT NULL,
 FOREIGN KEY(snapshot_id,organization_id) REFERENCES migration_snapshot(id,organization_id),
 FOREIGN KEY(preview_id,snapshot_id,organization_id) REFERENCES migration_snapshot_preview(id,snapshot_id,organization_id)
);
CREATE TABLE migration_snapshot_preview_record (
 id UUID PRIMARY KEY, preview_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 family TEXT NOT NULL, source_id TEXT NOT NULL, disposition TEXT NOT NULL,
 nonce BYTEA NOT NULL, ciphertext BYTEA NOT NULL, overlap_group_count BIGINT NOT NULL DEFAULT 0,
 UNIQUE(preview_id,organization_id,family,source_id), UNIQUE(id,preview_id,snapshot_id,organization_id),
 FOREIGN KEY(preview_id,snapshot_id,organization_id) REFERENCES migration_snapshot_preview(id,snapshot_id,organization_id)
);
CREATE INDEX migration_snapshot_preview_record_page ON migration_snapshot_preview_record(preview_id,organization_id,family,disposition,source_id);
CREATE TABLE migration_snapshot_preview_group (
 id UUID PRIMARY KEY, preview_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 kind TEXT NOT NULL, key_hmac BYTEA NOT NULL, member_count BIGINT NOT NULL,
 UNIQUE(preview_id,organization_id,kind,key_hmac), UNIQUE(id,preview_id,snapshot_id,organization_id),
 FOREIGN KEY(preview_id,snapshot_id,organization_id) REFERENCES migration_snapshot_preview(id,snapshot_id,organization_id)
);
CREATE INDEX migration_snapshot_preview_group_page ON migration_snapshot_preview_group(preview_id,organization_id,id);
CREATE TABLE migration_snapshot_preview_issue (
 preview_id UUID NOT NULL, snapshot_id UUID NOT NULL, organization_id UUID NOT NULL,
 family TEXT NOT NULL, issue_code TEXT NOT NULL CHECK(length(issue_code) BETWEEN 1 AND 80),
 record_count BIGINT NOT NULL CHECK(record_count>0),
 PRIMARY KEY(preview_id,organization_id,family,issue_code),
 FOREIGN KEY(preview_id,snapshot_id,organization_id) REFERENCES migration_snapshot_preview(id,snapshot_id,organization_id)
);
GRANT SELECT,INSERT,UPDATE ON migration_snapshot_preview_issue TO crm_app;
GRANT SELECT,INSERT,UPDATE ON migration_snapshot_storage,migration_snapshot,migration_snapshot_stream,migration_snapshot_note_detail,migration_snapshot_preview TO crm_app;
GRANT SELECT,INSERT ON migration_snapshot_capture,migration_snapshot_record,migration_snapshot_contact_key,migration_snapshot_preview_record,migration_snapshot_preview_group TO crm_app;
GRANT SELECT,INSERT,DELETE ON migration_snapshot_reservation TO crm_app;
GRANT USAGE ON SEQUENCE migration_snapshot_note_detail_ordinal_seq TO crm_app;
