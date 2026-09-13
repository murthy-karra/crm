# Mobile 001 — Frozen backend/native contract

Frozen implementation contract under D-074; backend verification is recorded in
[MOBILE_001_BACKEND_VERIFICATION.md](MOBILE_001_BACKEND_VERIFICATION.md). Protocol `mobile-v1`; all routes are under
`/api/mobile/v1`, use the existing session cookie, return `Cache-Control: no-store`
and never accept actor or Organization identifiers as authority. Native stores
must verify every response against their current actor/Organization/context.

## Requests and responses

JSON keys are snake_case. UUIDs are strings. All revisions are positive canonical
decimal strings. Timestamps are RFC3339 UTC. Unknown JSON fields are rejected.
All requests after bootstrap include `X-Mobile-Context: <uuid>`; operation bodies
also include context_id and must match the header. Contexts never authenticate.

- POST bootstrap: `{protocol, installation_id}` returns `{protocol, context_id,
  installation_id, actor_user_id, organization_id, workspace_revision,
  authorized_at, offline_access_expires_at, server_time, capabilities, bounds}`.
  Same installation/actor/Organization reuses its context UUID. Successful
  authorization renews seven days; other requests never renew the lease.
- POST operations: `{context_id, operation_id, kind, device_recorded_at, payload}`.
  kind and payload exactly follow the approved spec. complete_task uses
  `{person_id, target: {task_id, expected_revision}}` OR
  `{person_id, target: {created_by_operation_id}}`. Nullable fields are explicit.
  Reply is the approved content-free receipt with `person_revision`.
- GET operations/{id}: header context; returns the same receipt with replayed=true.
- POST reconciliations: `{protocol, installation_id, pinned_person_ids}` returns
  `{generation_id, context_id, evaluated_at, expires_at, complete, selected_count,
  manifest: {items: [{person_id, revision, reasons}], next_cursor, complete}}`.
  Reasons are sorted `assigned`, `pinned`, `today`. Invalid/foreign pins return
  generic not_found; duplicates normalize to one pin. Pins max 25,000.
- GET reconciliations/{id}/manifest: optional opaque cursor; same manifest shape.
- GET reconciliations/{id}/people/{person_id}/{summary|notes|tasks}: optional cursor;
  returns `{generation_id, person_id, revision, section, summary, items,
  next_cursor, complete}`. summary is null for notes/tasks. The summary section
  carries `{id,first_name,last_name,display_name,stage,assigned_user,created_at}`
  plus paged contact-method items `{id,kind,value}`. Notes and tasks retain their
  existing public item fields; tasks add decimal-string revision. All component
  items use ascending UUID order. A bundle is complete only after all three
  sections end; server seal does not attest that the device downloaded them.
- POST reconciliations/{id}/seal: empty JSON `{}`; returns `{generation_id,
  context_id, sealed_at, evaluated_at, selected_count, today}`. Today is the
  existing bounded Today DTO evaluated at the original trusted generation time.
  Only a complete unchanged generation can seal; repeated seal revalidates.

Existing `{error: code}` envelope. Codes: unauthenticated (401), forbidden and
workspace_in_migration_review (403), not_found (404); operation_payload_mismatch,
revision_conflict, dependency_pending, generation_changed, generation_expired,
protocol_unsupported (409); malformed_request (400), invalid_input/over_limit/
invalid_assignee (422), mobile_capacity (429, Retry-After: 30), unavailable and
mobile_unavailable (503). mobile_unavailable means disabled receipt-key configuration
or unavailable historical verification key; it never authorizes a new operation ID.
Sealing a changed projection additionally returns content-free `changes` counts
`{changed,added,removed,today_changed}`. Authority/partial-source errors may omit
counts because no complete comparable projection is available.
No error includes customer text or foreign identifiers. Unknown protocol requires
an app update; 404 on a receipt never proves the action did not execute.

## Atomic writes, keys and locks

The exact normalized payload includes the original dependency form, context,
protocol, kind and normalized RFC3339 device time. Keyed SHA-256 digest version 1
uses canonical serde structs, domain separation, and a configured key ring;
active key ID is stored with each accepted receipt. The configuration is
`CRM_MOBILE_RECEIPT_KEYS=id:base64[,id:base64...]`: 1–8 unique key IDs, each 1–32
ASCII letters/digits/underscore/hyphen, each key exactly 32 decoded bytes. Older verification keys must
remain configured through rotation; missing keys return unavailable, never retry
execution. Receipts contain IDs/result/revisions/time only and have no resource
foreign key that could delete the consumed marker with a resource.

Lock order is workspace shared operational guard, trusted admission Organization
advisory lock (context/generation admission only), context row, operation advisory
lock, Person row, task row, current membership. Receipt replay authorizes current
identity/workspace/Person/resource visibility before returning its prior outcome;
new execution additionally checks current write authority. All writes and the
receipt commit once; invalidation publishes only after commit. Context-row lock
serializes uploads for the installation. Storage is bounded by trusted identities,
not caller UUIDs. Transaction lock_timeout is 2 seconds and statement_timeout is 5 seconds,
installed before workspace checks. HTTP requests have a 20-second total deadline;
SQLx transaction cancellation rolls back. The Today savepoint restores the outer
statement timeout after its own bounded query/recovery work.

## Schema and bounded reads

Migrations 20260923000001/20260923000002 add task.revision and person.mobile_revision, mobile
contexts, operation receipts, reconciliation generations and manifest entries.
Task before-update revision covers every changed persisted task field. Person
summary changes, contact methods, all native note/task changes (including physical
and tombstone deletion), stage names and user display-name fanout advance Person
revision. User references include assigned agent, note author, task creator,
assignee and completer. Old Web/Operator DTOs remain unchanged. Mobile origin is
`mobile_session`; note/task origin TEXT columns have no closed origin CHECK.
Origin decode is extended; no mobile fact-writing command is introduced.
The component/label revision trigger functions run as their schema owner with
qualified public tables, fixed search_path including pg_temp last, and revoked
PUBLIC EXECUTE. Their only write increments the derived Person revision. This
preserves permitted import inserts under a review hold while ordinary note/task
and direct Person-revision writes remain denied by existing BEFORE guards.

Generation selection and sealing run in short repeatable-read transactions using
the same transaction-compatible Today core. Store IDs/revisions/pins and a Today
fingerprint, never note/task bodies. Generation authority records current role
and workspace revision. Per-page revision and authority checks use a consistent
snapshot. Opaque HMAC-signed cursors bind context, generation, Person, section,
revision and last UUID. Selection max 25,000; manifest pages 250; component pages
100 rows and 512 KiB; one upload and two concurrent downloads per context.

Admission is serialized by trusted Organization and updates a trusted
`mobile_admission` row, so repeatable-read callers cannot admit against a stale
pre-lock snapshot; serialization failure is retryable unavailable. Limits include retained expired
rows until deletion: 10 contexts per actor/Organization, 2 generations/context,
4/actor/Organization, 20/Organization. Each admission reclaims at most two expired
generations (max 50,000 manifest rows) in its Organization. An API-started worker
with configured mobile keys also reclaims at most two expired generations globally
every 60 seconds, including while no mobile requests arrive. TTL 30 minutes. The
receipt table is outside cleanup. Failure to reclaim never raises these limits.
New installation IDs cannot bypass actor or Organization bounds.

A partial Today source or incomplete selection cannot seal. Devices persist data
and page checkpoints atomically and preserve old active cache on errors. Accepted
local overlays remain until an active complete bundle reaches receipt.person_revision.
Acknowledgment/promotion serialize in SQLite; lower revision bundles never replace
higher ones, and stale removal never erases accepted local work. These native
rules are tested by both native lanes using shared fixtures.
