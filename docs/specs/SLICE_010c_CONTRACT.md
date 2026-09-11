# Slice 010c concrete backend contract

Frozen implementation detail under D-065 and the approved 010c specification,
2026-09-11. This is the backend contract checkpoint; it does not authorize release,
activation, real customer processing or source requests.

## Workspace and authority

`organization.workspace_mode` is `operational` (default) or `migration_review`;
`workspace_revision` starts at 1 and increases only on the exclusive transition.
The session Organization adds these two fields (revision is a decimal string).
There is no release/toggle endpoint. `migration_workspace` uniquely binds one
Organization to an import and confirmed plan. A cancelled/completed import retains
the binding. Platform privileges do not imply tenant review access.

The namespaced advisory transaction barrier is `crm-workspace-v1:<org UUID>`.
Lock order is workspace, current membership, Organization row,
snapshot/storage ledger, import/plan, then source/Person/domain rows. Shared locks
protect ordinary transactions and complete protected response assembly; exclusive
confirmation drains them before checking emptiness and entering review. Lock and
pool waits are bounded. Ordinary guards always require operational mode, regardless
of Origin or actor role. Review reads require a currently active admin; Today,
Operator and operational realtime require operational mode for every role.
Import shared command/worker transactions install a two-second transaction-local
row-lock timeout before membership or Organization locks; the initial worker claim
uses the same bound. Contention returns a closed retryable failure and rolls back
the transaction, releasing its workspace guard and uncommitted reservations.

Database guard triggers on the frozen ordinary-write inventory provide a second
line of enforcement for direct persistence callers. Import permits are private
typed values backed by a transaction-local token validated against the persistent
workspace/import/plan/executor/lease; arbitrary admin or migration Origin is not a
permit. Governance setup and terminal call settlement are separately scoped.
No request, environment setting or Operator tool accepts such a token.

Entry rejects any tenant row in person, contact_method, inquiry,
inquiry_received, routing_decision, assignment_changed, stage_changed,
contact_attempted, person_tag, person_custom_field_value, note, task, call,
call_completed, raw_payload, intake_extraction, correspondence_raw,
correspondence_captured, capture_message or operator_task_proposal. Parent joins
scope children without organization_id. It also rejects active admissions,
unexpired proposed or any claimed Operator proposals and any existing workspace.
Terminal IDs-only Operator audit and expired/terminal proposal metadata are allowed.
Definitions/settings, identity/governance, seeded stages and prior migration
evidence are allowed at entry. Ordinary edits to those settings remain blocked
during review; identity/security/membership administration remains allowed.

`workspace_operation_admission` stores only id, Org, actor, operation kind,
created_at and deadline. Operator registers under a shared operational guard
before spawning. One absolute local deadline includes scheduling/tool execution;
admission deletion happens after termination. Expiry never grants further work.
The terminal Operator ledger remains append-only. No transaction spans inference.
Call admission remains its existing durable call row; any call prevents initial
entry. Signed terminal settlement, authorized owned hangup and expiry/failure
cleanup remain allowed, but dialing and ordinary outcome correction do not.

Held business requests return 409 `workspace_in_migration_review`; missing session
401, non-admin migration access 403 and foreign resources 404 remain unchanged.
Inbound intake/capture returns 503 with `workspace_in_migration_review` before raw
persistence; Retry-After is 5 seconds. It does not acknowledge a queued message.
Import emits no operational notifications. Admin review uses ordinary read APIs
plus import/provenance APIs, all guarded by current authoritative permission.

## Exact retained evidence and executable values

Engine `fub-people-import-v1` accepts only final `fub-core-v1` snapshots in completed
or completed_with_gaps with exhausted people/users/stages and a completed preview
at the final capture sequence. Account identity is the snapshot's account, never
credential revision. No reader/provider is injected into the import worker.

One capture is decrypted per preparation unit. Its scoped records are checked
against losslessly parsed collection items at exact ordinals, IDs, representation
and tenant semantic HMAC. Capture acceptance, successful classification, 2xx and
nontruncation are mandatory. All observations for People/stage/user IDs participate
in qualification. Distinct canonical HMACs, unqualified/mismatched evidence or
invalid source encoding hold the candidate; invalid IDs have separate counts.
Supporting stage/user records obey the identical rules. Display JSON is never an
executable input. Source IDs are exact positive decimal strings of at most 128 digits.

The executable Person contains first_name/last_name, ordered contacts, a source
stage key and source assignee key plus encrypted source provenance. A null source
stage is the explicit mapping key `missing`; a non-null source assignment is an
exact source user ID or a namespaced pond key; unknown shapes are held. Mapping
keys never encode client-provided labels. Null source assignment requires no choice.
First/last names remain exact except whitespace-only becomes null with a receipt.
Nonempty invalid contact/native strings hold the entire Person. Within-kind
normalization uses the existing CRM functions; duplicates retain the representative
selected after primary ordering and retain every original spelling in provenance.
Only numeric isPrimary 1/0 is recognized. Other values flag preference uncertainty.
One primary wins; otherwise original array order wins. Native NUL is rejected;
normalized contacts and stage names for creation are limited to 2048 UTF-8 bytes.
Trash is held. Other flags and unknown fields remain disclosed, never permission.
Supporting stage labels and user emails containing NUL skip only native-text
suggestion queries. Their exact encrypted evidence remains inspectable, and valid
explicit existing-stage/member/unassigned choices remain available when qualified.

`contact_method.import_order` is nullable, nonnegative and unique within
Person/kind when nonnull. All primary readers use `import_order NULLS LAST,
created_at,id`; ordinary writes leave it null. Person creation/activity semantics
and Inquiry-source filtering otherwise stay unchanged.

## Persistence and progress

One additive migration owns workspace mode/binding/admission, import run, immutable
plan revisions, plan mapping choices, qualified source items, executable manifest,
identity maps, results/provenance, command receipts, reservations and person_imported.
Every child has scoped composite references; source identity is unique on
Organization/account/family/source_id. Identity survives missing targets and never
cascades into recreating a Person. Source capture/record references remain exact.

Run states are proposed, queued, running, paused, completed, cancelled, expired.
Plan states are building, ready, paused, failed, superseded. Preparation phases
are copying_choices, captures, mappings, people, ready; each phase checkpoints
bounded work. A ready plan is immutable and expires after ten minutes. A replacement
revision copies choices in keyset batches, applies at most 50 request patches, then
rebuilds its own evidence/manifest. Only one plan builds. Replacement fences the
prior plan; cancelled imports cannot be replanned. Confirmation fixes one plan
forever in 010c. Retry resumes preparation or execution as shown by `phase`.

Leases last 60 seconds, are claimed in short transactions and have fresh UUID
fences. An expired claim can be recovered only after current admin and matching
plan/mode checks. Preparation uses at most one raw page; mapping/manifest/execute
discovery pages contain at most 50 IDs/descriptors, followed by one item payload
at a time. Each Person transaction includes contacts, source identity/contact maps,
encrypted provenance, PII-free imported/initial stage/assignment facts, result,
cursor and byte settlement. Stage creation and its mapping/receipt settle atomically
and replay safely before dependent People. No import calls intake identify.

Facts use System/Migration, actual local commit time, import ID correlation and
current executor on-behalf-of actor. Initial reasons are `migration`. Import fact
fields are local IDs, source-record/capture IDs and plan/run IDs, never source
strings/IDs/contact values. Reconciliation has imported/already_imported/held/pending
dispositions. Confirm replay returns its original receipt; altered actor/action/
request/input conflicts. Cancel fences future work and retains every committed row.

## Byte inventory and cryptographic scope

Import reservations have separate owners from source/preview reservations and
charge the existing snapshot and Organization ledgers. Source cancellation cannot
release them. Lock ledgers in the established migration order. Preparation reserves
64 MiB per capture/copy unit. Before readiness every executable Person/stage has a
proven added-byte bound no greater than 64 MiB. Execution reserves that item bound,
settles exact variable bytes atomically and releases unused admission; >2 MiB is
supported. Intrinsic excess is held as `import_item_byte_limit`; allowance shortage
pauses as `storage_limit` before business writes. Budget increase never resumes work.
Every admission receives the current server `SnapshotPolicy`; the effective run
and Organization allowances are the lesser of their stored approved limits and
current deployment ceilings. Lowering a ceiling never rewrites an approved stored
allowance. Restoring capacity still requires explicit retry of a paused run.

Count nonce and ciphertext for choices, source items, manifest, provenance and
mutation receipts; source IDs/choice keys/cursor strings, semantic/request/digest
hashes, variable representation/field keys and source/contact identity metadata.
Fixed UUIDs/timestamps/enums/counters and physical row/index/TOAST/WAL/replication
overhead are excluded. Business Person/contact bytes are excluded. Bound arithmetic
includes JSON escaping, encryption tags/nonces, duplicated indexed identifiers
and all result/identity/provenance metadata; exact settlement uses serialized bytes
plus persisted variable-column octet lengths. Retained historical revisions remain
charged. Replaced transient encrypted values settle net byte deltas, never double
count them. Receipts are counted when first persisted, not when replayed. Detailed
mutation receipts include their own exact final retained-byte count, sized to a
bounded fixed point before settlement; returned and replayed cancellation counters
show zero remaining work/control reservation and the committed retained total.

Reuse retained crypto with distinct `import-v1:{plan}:{purpose}` purposes, scoped
by Org, snapshot and row UUID. Closed purposes distinguish source, choice, manifest,
provenance, receipt and cursor. Cursor AEAD additionally binds endpoint, import,
plan/revision, filter and selected field/Person; stale/foreign/malformed cursors fail.
Display hashes and tokens never authorize writes. No raw page is copied as a new
provenance object; per-item selected source text/contact metadata is retained with
raw pointers. Missing keys pause; they never cause source refetch.

## HTTP DTOs

All mutation bodies deny unknown fields and are limited to 64 KiB. UUIDs,
timestamps and all counts/bytes/revisions/source IDs are strings. Responses are
no-store. The approved endpoint list is unchanged. Closed mapping request variants:

- Stage: `{source_key,choice:{kind:"existing",stage_id}}`,
  `{source_key,choice:{kind:"create"}}` or `{source_key,choice:{kind:"hold"}}`.
- Assignee: `{source_key,choice:{kind:"member",user_id}}`,
  `{source_key,choice:{kind:"unassigned"}}` or `{source_key,choice:{kind:"hold"}}`.

Patch arrays default empty, reject duplicate keys and have at most 50 combined
entries. Untouched choices inherit the named revision. Source labels/payloads,
authority and destination Org cannot be submitted.

Create/replan responses contain import_id, plan_id and state. Other mutation
responses contain `import` (the current detail summary); confirmation also carries
its durable receipt and workspace_mode/workspace_revision. The create body is
request_id/snapshot_id/preview_id. Replan adds expected_plan_revision and mapping
arrays. Confirm has request_id/plan_id/plan_revision/confirmation_digest and
acknowledgments exactly `{held_count,review_only:true,remaining_data:true}`.
Retry/cancel bodies contain request_id only. Replays require byte-equivalent
canonical inputs and the same trusted actor/action.

Import summary: id, snapshot_id, preview_id, source_account_id, capture_sequence,
state, phase, pause_reason, latest_plan_id, confirmed_plan_id, executor_user_id,
created_at, updated_at, counts, retained_bytes, reserved_bytes,
cancellation_reserved_bytes, actions and current `release_ready` on HTTP reads.
Detail adds plan, workspace, coverage, source_window, policy and engine_version.
Plan: id, revision, state, phase, confirmation_digest, expires_at, counts,
required_reservation_bytes, expired, created_at, completed_at. Counts are decimal strings
for source_people, eligible_people, held_people, invalid_ids, contacts,
overlap_people, stages_to_create, assigned_people, unassigned_people,
imported_people, imported_contacts and pending_people, plus `reasons`, an object
of closed nonexclusive reason counters. Never add heterogeneous family totals.
Policy fields are run_byte_limit, org_byte_limit, run_ceiling_bytes,
org_ceiling_bytes, unit_ceiling_bytes and policy_revision; all byte values are
decimal strings. `cancellation_reserved_bytes` is included in `reserved_bytes`.

List envelopes are imports/records/mappings/results with next_cursor. Limits are
1–50. Mapping pages require kind=stage|assignee; record pages optionally filter
eligible|held; result pages optionally filter imported|already_imported|held|pending.
Records have id, source_id, disposition, held_reasons, transformations, proposed
core values, abbreviated fields and overlap_count. Mappings have source_key,
qualified, source display, choice, suggestions, target, reasons and dependent_count.
Result rows add person_id, contact_count and committed_at when applicable.
Result discovery retains source-ID keyset ordering and indexed per-manifest
result lookups. The constant `OFFSET 0` inside the lateral lookup preserves that
optimization boundary; it does not skip records or paginate by offset. Filtered
pages may inspect the remaining manifest tail when matches are rare or absent;
the populated-fixture page bound is not a universal bound for empty filters.

Record/provenance serialized page ceiling is 512 KiB, achieved by smaller actual
pages and explicitly abbreviated display fields, never truncating stored values.
Each abbreviated source field has value, abbreviated, full_utf8_bytes and field_key.
`GET .../plans/{plan}/records/{record}/fields/{field}`,
`GET .../plans/{plan}/mappings/{mapping}/fields/{field}` and
`GET /api/people/{id}/import-provenance/fields/{field}` accept cursor/limit (UTF-8
bytes, 4–65536), return text/offset/full_utf8_bytes/next_cursor and split only at
UTF-8 boundaries. They return escaped text from an approved field allowlist, not
a raw capture download. Mapping field reads decrypt the exact retained supporting
record embedded in that mapping, use the same full-field allowlist and require
current admin plus matching Org/import/plan/mapping. Their cursor purpose is
`import-mapping-field:{plan}:{mapping}:{field}`, separate from manifest/provenance
cursors. Provenance summary links snapshot/plan/import/source record,
shows source attribution/date/contact metadata and exposes these field links.

Error codes are closed: 400 malformed_request for invalid JSON/UUID/cursor;
401 unauthenticated; 403 forbidden; 404 not_found; 409 import_conflict,
import_busy, import_expired, workspace_not_empty, workspace_in_migration_review;
422 invalid_import_choice/source_not_eligible; 413 payload_too_large;
503 unavailable/workspace_in_migration_review for deferred ingress. Pauses include
storage_limit, retained_key_unavailable, source_evidence_unavailable,
authority_changed, mapping_target_changed, target_unavailable and storage_unavailable.

## Compatibility checkpoint

Gate version is `crm-workspace-v1`. Launch/release preflight inventories persistent
bindings and requires a known compatible artifact and retirement acknowledgment
for old API/worker/CLI processes before enabling first confirmation. Once bindings
exist, unknown or pre-gate artifacts fail closed. No binding deletion, mode reset,
whole-database restore or old 010b rollback shortcut is offered. Synthetic artifact/
state tests exercise the boundary; deployment remains outside this assignment.

Display arrays include at most 20 entries and explicit total/abbreviation metadata.
Full collections are inspectable as escaped canonical text through field segments;
segment limit is at least 4 UTF-8 bytes, guaranteeing progress for every scalar.
Fleet retirement is an operator release-preflight fact, never a tenant/client claim.

Cancellation admission reserves 65,536 bytes when the run is first proposed,
with reservation purpose `cancel` and an infinite deadline. The bounded receipt
contains fixed summary fields and closed reason counts (no source text), so this
covers JSON escaping, nonce/tag and its 32-byte request digest. Work-reservation
reclamation and source cancellation cannot release it. Cancel settles its exact
receipt bytes and releases the unused portion; successful completion releases it
with zero added bytes. Thus exhausted allowance never prevents fencing a run.
Paused and ready runs expose these reserved control bytes separately from work;
all reservations are zero after cancel or successful completion.

`CRM_MIGRATION_RELEASE_REPORT` is a private, operator-managed preflight report
path. API startup checks the durable gate/schema before workers or listening;
confirmation additionally validates the report's known current executable hash,
database and inventory `evidence_expires_at`. Current report contents are re-read
for readiness and confirmation, so replacing the report requires no API restart.
Absent, expired or invalid evidence gives `release_ready:false`, disables the
Confirm action and returns 503 `unavailable` if requested. No tenant field can
supply this readiness. An already committed confirmation receipt is resolved under
current admin/tenant/actor/input authority before consulting current readiness;
expired or missing reports cannot invalidate its replay. A genuinely new
confirmation still requires current readiness inside its exclusive transaction.
Synthetic fixtures use a test-support-only constructor.
