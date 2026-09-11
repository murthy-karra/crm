# Slice 010b concrete backend contract

Implementation detail under D-063 and SLICE_010b; frozen before coding, 2026-09-11.

## Source profile

`fub-core-v1`, page size 100, decoded response bound 4 MiB, timeout 10 seconds.
Users explicitly select `fields=allFields,calling&includeDeleted=true`; People
select `fields=allFields&includeTrash=true&includeUnclaimed=true`. Unclaimed is
the source user's offered scope, not an account-wide promise. Stages, customFields,
notes and both tasks completion partitions retain the published selected fields.
Notes detail adds both threaded replies and reactions. Collection keys must match
metadata.collection. Offset and total accept lossless nonnegative decimals;
next tokens are bounded, encoded query values, never URLs. Empty/short pages alone
do not establish exhaustion. Tasks pagination remains a live qualification gap.
Semantic records use a duplicate-key-rejecting lossless JSON parser, sorted object
keys and unchanged array order. Note list and detail are separate representations.

## Persistence and accounting

One additive migration, 20260917000001. Snapshot, stream, capture, record,
contact-key, preview and Organization storage ledger are the seven logical types.
Children are durable reservations, note-detail work, preview records and overlap
groups. Composite run/Organization references prevent scope rebinding. Capture
sequence is assigned under the run lock. Source records/contact observations are
append-only. Preview input includes an immutable source sequence boundary and
actual encrypted destination/coverage observation, comparison engine version 1.

Count each encrypted payload's ciphertext plus 24-byte nonce; variable indexed
source IDs, semantic/contact hashes, representation/request profile keys and cursor
payloads are included in the logical retained ledger. Fixed IDs/timestamps/state
columns and physical rows/index/WAL/replica overhead are operational measurements,
not this logical allowance. Reservations are 64 MiB for a source page (4 MiB raw,
bounded derived indexing and failure evidence) and 2 MiB per 50-record preview
batch. Actual deltas settle atomically; unused admission is released. A source
response that cannot produce bounded derived indices is retained as classified
failure evidence without advancing its checkpoint. Reservations belong to one
fenced lease and are released only after fencing, cancellation or settlement.

## Configuration

`CRM_FUB_SNAPSHOT_RUN_CEILING_BYTES` defaults 2147483648;
`CRM_FUB_SNAPSHOT_ORG_CEILING_BYTES` defaults 4294967296.
Positive decimal i64 values only. Initial approvals are min(default, ceiling).
Policy revision is a deterministic opaque fingerprint of both ceilings; changing
configuration changes it. Existing reservations retain their bounded admission.

## HTTP JSON

All integer byte/count/source-ID values below are decimal strings. UUID IDs and
RFC3339 timestamps use strings. Resource operations have the routes/status codes
in SLICE_010b §6, with strict unknown-field rejection and no-store responses.

`Snapshot`: id, connection_id, connection_revision (integer), source_account_id,
profile_version, state, pause_reason (nullable), created_at, started_at (nullable),
completed_at (nullable), proposal_expires_at, raw_bytes, retained_bytes,
reserved_bytes, accepted_captures, capture_sequence, original_run_byte_limit,
run_byte_limit, org_byte_limit, org_retained_bytes, org_reserved_bytes,
run_budget_revision, org_budget_revision, policy_revision, run_ceiling_bytes,
org_ceiling_bytes, required_reservation_bytes, actions (string array), preview_ids
(array of up to 20 latest UUIDs). Actions: confirm, retry, cancel, increase_budget,
generate_preview, with source authority separately checked.

Propose response `{snapshot,proposal}`; proposal has families, profile_version,
expires_at, run_byte_limit, org_byte_limit and source_scope. Other source command
responses are `{snapshot}`. Budget response adds `budget` with old/new limits.
Detail is `{snapshot,streams,coverage}`. Stream rows have stream, family, state,
reported_total (nullable decimal), returned_items, accepted_captures, content_gaps,
attempts, error_code (nullable), observed_at (nullable). Coverage always includes
core and remaining families with family, state, reason, next_action. Summary list
has snapshots, next_cursor, active_snapshot_id, latest_completed_snapshot_id.

Preview start/retry: `{preview_id,state}`. Preview detail: `{preview,coverage,
counts,destination_stale}`. Preview has id, snapshot_id, state, pause_reason,
engine_version, capture_sequence, created_at, completed_at, input_observed_at,
actions. Counts are per family and disposition plus invalid-ID/content-gap totals,
never a heterogeneous migration total. Records endpoint requires family and
accepts disposition, cursor, limit (1–50); group/member endpoints accept cursor
and limit, groups optionally record_id. Page envelopes have records/groups/members
and next_cursor. A record has id, source_id, family, disposition, issues,
projection (bounded escaped JSON data), overlap_group_count. A group has id,
kind and member_count. A member has source_id and record_id. Local cursor tokens
are AEAD sealed with endpoint/Organization/run/preview/filter/group scope.

Preview projections are read-only review evidence, never import instructions.
Retained reads and preview requests require a current Organization admin and do
not require a connected credential or the original source initiator.

## Implemented representation and accounting bounds

Positive source IDs are decimal strings up to 128 digits. Integral projection
values outside JavaScript's exact range use the explicit lossless-number wrapper;
source-derived wrapper collisions are flagged and cannot supply candidates.
`_snapshot.flags` identifies clipped/omitted projection content, with exact raw
bytes retained separately. Public metadata does not qualify equality of note
list/detail content fields; v1 compares content variants within each representation
only. Detail preference never fills missing fields from list. Captures retain the
selected source version header, and runs pin the SHA256 of the qualified source
schema manifest (`e136daf321fae96c7feb895fe662e0e925c501e059b8fd0faff6074cccff5c79`).

A page holds at most 100 records and 4 MiB decoded bytes; projections at most 16 KiB
per record; extracted normalized contacts at most 16,384/page, with 128-digit IDs,
4 KiB/key and 4,096/record. Consequently capture + projection + repeated
(source-ID/kind/HMAC) indexing remains below 10 MiB; the 64 MiB reservation also
covers cursor, negative/failure evidence and future bounded envelope overhead.
This is an admission bound, not an amount billed or allocated per request.
Preview 50-record batches retain at most 36 KiB per encrypted record envelope,
plus small group/checkpoint deltas, below their 2 MiB reservation.

The exact ledger columns are checked by `retained_ledger_matches_all_counted_payloads_after_capture_preview_and_cancel`:
capture nonce/ciphertext/request fingerprint/representation/source-version;
record nonce/ciphertext/semantic HMAC/representation/source-ID;
contact source-ID/kind/HMAC; note-detail source-ID; stream cursor nonce/ciphertext;
preview input nonce/ciphertext/destination fingerprint and source/group checkpoints;
preview record nonce/ciphertext/source-ID; group kind/HMAC. Fixed closed metadata,
IDs, timestamps, state, counters and bounded command receipts are operational
metadata; physical row/index/TOAST/WAL/replica overhead is outside logical accounting.

`counts` is `{records:[{family,disposition,count}],invalid_ids:[{family,count}],
streams:[Stream]}`. Stream adds distinct_ids and first_observed_at. Preview detail
also has first_import_requires_new_empty_organization=true and
destination_has_people. Record DTOs add candidates:{member_id,stage_id,
custom_field_id}; only unique matches have UUID values, all others are null.
This evidence is advisory; no mapping is accepted or applied.

## R1 corrections

Contact groups and overlap counts/issues apply only to People. Another family's
coincident numeric source ID never inherits a Person's contact membership.

Each report stores nonexclusive issue counts in
`migration_snapshot_preview_issue`, keyed by report/Organization/family/closed
issue code. Each completed record batch increments those counters atomically
with its encrypted records and checkpoint. Failed batches increment nothing;
resume cannot double count. `counts.issues` is an array of
`{family,issue,count}` (decimal count strings), separate from the primary
`counts.records` disposition counts. Issue keys and counters are closed
operational metadata; encrypted detailed record payloads remain byte-accounted.

Run and Organization ledgers retain `budget_policy_revision` at approval.
Snapshot DTOs expose `run_budget_policy_revision` and
`org_budget_policy_revision`. The budget receipt includes
`approved_by_user_id`, `old_run_budget_revision`, `run_budget_revision`,
`old_org_budget_revision`, `org_budget_revision`, `old_run_policy_revision`,
`old_org_policy_revision` and the new `policy_revision`, alongside its original
old/new limit fields. The trusted approving actor is also included in the request
digest: another current admin cannot replay that actor's request ID as their own
approval. Policy strings/actor IDs are bounded audit metadata, not source payloads.

An expired running source lease starts a fresh identity check even when the
replacement worker runs in the same process. Existing durable checkpoints and
source-attempt limits remain intact.
