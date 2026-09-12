# Slice 010d2 — Concrete implementation contract

D-072 implements the approved metadata-only retained historical import. This
document freezes the integrated import, storage and bounded-reader contracts.
Original 010d1 bytes,
projection/parser versions and native domain semantics remain unchanged.

## Import HTTP

All routes below are under `/api/migrations/fub/history-imports`, require current
Organization admin authority, and are `Cache-Control: no-store`, including outer
middleware errors. JSON requests deny unknown fields and are at most 8192 bytes.
UUIDs are strings; counts, byte amounts and revisions are canonical decimal
strings. Mutation receipts are at most 4096 bytes:
`{import_id,plan_id,revision,state}`. Refetch detail after every receipt; replay is
actor/request/operation/body bound and rechecks current authority first.

| Route | Exact request / response |
|---|---|
| `POST /` | Prepare `{request_id,parent_import_id,capture_id,expected_capture_revision,expected_workspace_revision,expected_policy_revision}` →201 receipt. A cancelled anchored attempt can be replaced by preparing the same capture; the new attempt reuses the exact frozen plan. |
| `POST /{id}/confirm` | `{request_id,plan_id,expected_revision,expected_plan_revision,expected_workspace_revision,expected_policy_revision,acknowledgements:{external_facts,date_uncertainty,coverage_and_holds,review_only}}` →202 receipt. All acknowledgements must be true. |
| `POST /{id}/resume` | `{request_id,expected_revision,expected_policy_revision}` →202 receipt; paused only, current admin becomes responsible executor. |
| `POST /{id}/cancel` | `{request_id,expected_revision}` →200 receipt. Terminal replay with a new request remains a bounded no-op; committed facts and the anchor remain. |
| `POST /{id}/budget` | `{request_id,expected_revision,expected_run_budget_revision,expected_org_budget_revision,expected_policy_revision,run_byte_limit,org_byte_limit}` →200 receipt; monotonic within deployment ceilings, then separate Resume. |
| `GET /?parent_import_id=&limit=&cursor=` | `{imports:[ImportDetail],next_cursor}`; default 25/max 50, frozen upper creation bound and descending creation/ID cursor. |
| `GET /{id}` | `ImportDetail` below. |
| `GET /{id}/records?limit=&cursor=&family=&disposition=` | `{records:[ManifestItem],next_cursor,plan_id,revision}`. |
| `GET /{id}/results?limit=&cursor=&family=&disposition=` | `{results:[ResultItem],next_cursor,plan_id,revision}`. |

`ImportDetail` is `{id,plan_id,parent_import_id,capture_id,state,phase,revision,
plan_revision,workspace_revision,interpretation_version,reader_version,
capture_revision,capture_sequence,parent_capture_sequence,executor_user_id,
created_at,updated_at,confirmed_at,completed_at,plan_expires_at,pause_reason,
preview_complete,counts,coverage,added_byte_bound,retained_bytes,reserved_bytes,
run_byte_limit,run_budget_revision,org_byte_limit,org_budget_revision,
policy_revision,run_byte_ceiling,org_byte_ceiling,release_ready,
actions:{confirm,resume,cancel,increase_budget,prepare_same_plan}}`.
Counts contains decimal `occurrences,eligible,equal_repeats,held,processed,
inserted,already_imported,application_held`. Preview counts freeze when ready;
execution counts belong to the selected attempt. Coverage is the exact bounded
three-stream retained capture report plus closed warnings; inaccessible account
history remains unknown. Coverage has `{streams,warnings,api_inaccessible_count:null,
enumeration_is_complete_account_history:false}`. Each of the three streams has
`family,state,reported_total,checkpoint,occurrences,valid_occurrences,
invalid_occurrences,unique_ids,equal_repeats,conflicting_variants,linked,
parent_excluded,no_parent_identity,invalid_person_reference,conflicting_reference,
attempts,api_inaccessible_count:null,count_basis:"advancing_pages",
content_scope:"exact_returned_json_only",enumeration_is_complete_account_history:false}`.
Stream counts and reported total are decimal strings. Warnings are exactly
`api_restricted_records_unknown,detail_content_not_fetched,not_atomic_snapshot,
external_facts_only`. Prepare uses the same deployment policy revision exposed
by the selected capture.

`ManifestItem` is `{id,position,family,disposition,reason,person_id,metadata,
capture_id,ordinal,observation_id}`. `ordinal` is a JSON integer 0–99.
`ResultItem` adds `{result_id,fact_id}`.
Family is `events|calls|text_messages`; manifest disposition is
`eligible|equal_repeat|held`; result disposition is
`imported|already_imported|equal_repeat|held`. Closed reasons include
`invalid_identity,ambiguous_relationship,parent_excluded,no_parent_identity,
parent_erased,conflicting_variants,identity_erased,identity_conflict`.
Metadata can be null after suppression. Executor is always a UUID;
confirmed/completed/expiry times, pause reason, per-record reason and fact ID
are nullable where inapplicable. No list/detail returns original raw,
bodies, subjects, HTML, endpoints, names, arbitrary URLs or source credentials.
Page rows are at most 4096 bytes and pages at most 512 KiB; row overflow fails closed.
Preparation/application changes, display updates/deletion and identity erasure
invalidate all referencing attempts’ page cursors with 409; cursors bind Org,
attempt,plan,endpoint,revision,filters,limit and last stable position.

Errors use existing closed 401/403/404/400/409/503 mappings; conflict and budget
errors never echo source fields. Explicit release-state rejection is 409;
corrupt retained content and oversized read responses are 503. Command budget
exhaustion remains 409. Pauses use closed `retained_integrity_failed,
source_binding_changed,executor_not_authorized,release_not_ready,storage_limit,
interpretation_bound_exceeded` codes. States are
`preparing|ready|queued|running|paused|completed|cancelled`; phases are
`capture|classify|apply|finished`. First-confirm plans expire 10 minutes after ready;
an anchored plan never expires or changes. Zero eligible plans cannot confirm.

## Storage and reader seam

The single additive migration owns new `migration_history_import_*` plan, run,
anchor, stream, manifest, candidate, display, identity, result, reservation and
receipt tables. The parent anchor key is `(organization_id,parent_import_id)`;
it pins `plan_id,interpretation_version='fub-history-interpretation-v1',
reader_version='fub-history-timeline-v1'` and survives cancellation/erasure.

Facts are three append-only tables `fub_event_record_imported`,
`fub_call_record_imported`, `fub_text_record_imported`. Each has the standard D-015
envelope, `person_id,plan_id,attempt_id,manifest_id,identity_id,source_created_at,
source_time_basis,stable_position`. Source time basis is `fub_record_created` or
`unknown`. The local user actor/occurred_at/recorded_at describe import observation;
they never claim that user performed the external historical action. Native
Inquiry/call/contact/correspondence/Today rows receive no import writes.

`migration_history_import_display` keys `id=manifest_id,organization_id,plan_id`
and stores `owner_run_id,nonce,ciphertext`; plaintext is at most 4096 bytes.
`history_import::display(conn,key,org,plan,manifest)` opens it with the new
`timeline-import-display-v1` purpose and returns only the approved metadata DTO.
Identity rows have `id,organization_id,identity_hmac,semantic_hmac,person_id,
fact_id,family,erased_at`; stable HMAC purpose binds Org/account/family/vendorID/
representation without capture-run UUID. Identity erasure is permanent.
Missing Person, missing display or erased identity suppresses reads and replay.
An erased primary or previously mapped group participant discovered before
preparation creates a permanent identity tombstone without deriving new display
content or historical dates. Every referencing attempt revision changes when
existing display or identity visibility changes; the frozen plan never mutates.
No immutable fact contains free-form source text. Original raw remains charged
only to 010d1; its shared-page erasure hold/inventory is not replaced.

Metadata fields: `source_id,source_person_id,source_access_user_id,
source_attributed_user_id,source_creator_user_id,source_editor_user_id,
source_created,source_updated,source_sent,source_event_type,source_note_id,
source_is_incoming,source_duration,source_duration_unit,source_outcome,
source_status,content_availability,preview_truncated`. Unknown fields are null;
duration is an exact bounded nonnegative finite decimal and its unit is the
qualified source field unit. Free labels are inert UTF-8 previews at most 256 bytes
each with explicit truncation; role IDs are independently parsed positive IDs.
Dates require explicit-offset RFC3339 without leap seconds, unknown offset or
microsecond precision loss. Invalid updated never discards valid created.
Conflicting creator/editor ID aliases remain unknown; equal canonical aliases
retain their qualified role. Decimal strings represent the exact canonical
value, so source `1.5` may render as `15e-1`.

## Units, budgets and compatibility

No source reader enters the worker API. One unit interprets one retained raw page
and at most 50 occurrences, or classifies/applies at most 50 manifest rows. Partial
page positions and per-family encrypted continuation preserve the exact source
chain. Complete canonical records drive variant equality; projections never do.
Preparation classifies all retained occurrences before confirmation. Current
010d1 terminal qualification requires zero invalid IDs and exact equality of
valid occurrences, distinct IDs and the reported total in each stream. Thus
repeat/conflicting-variant/nonobject-ID captures remain ineligible for Prepare;
the importer does not bypass that restriction or claim successful end-to-end
imports of those captures. Its full canonical candidate logic still holds
conflicting identities rather than selecting a winner. Unique identities and
parent anchoring prevent concurrent or repeated writes; attempts only resume the
same frozen plan. Confirm/Resume executor authority is rechecked before every
atomic commit under a DB-clock token lease.

Derived variable-byte bound is 32 KiB per record. The display contributes at most
4096+40 bytes; occurrence/candidate/identity HMACs and bounded representations,
reasons, result/provenance fields contribute less than 2 KiB per occurrence; the
remaining margin covers bounded control/receipt changes. Canonical raw records
are transient and never copied. Each 50-record batch reserves at most 1.6 MiB,
plus bounded stream/control overhead. Proposal reserves 8 KiB cancellation capacity.
Exact SQL measurement covers all new variable text/JSON/bytea columns, with fixed
UUID/enums/times/index physical bytes reported separately. Inserts/updates/deletes
adjust only owning-run and shared-Org charges. Control remains reserved while
paused and pays cancellation receipt even at a full budget.

`crm_workspace_read` checks transaction-local `crm.history_reader` after existing
authority checks when an anchor exists. Upgraded helpers set it on every actual
connection. `crm_history_complete_read` rejects anchored complete reads with P010H;
only upgraded API maps that to 409 `history_review_required`. Capability is
`fub-history-timeline-v1`, independent of capture/activity. Fresh readiness is
required at Confirm/Resume and worker session/recovery admission, not on every
ordinary admitted page. Deployment inventory must retire unsupported artifacts;
old startup cannot discover the new table. Reader gates ship before fact writes.
## Reader HTTP and DTO freeze

GET `/api/people/{id}/migration-review/v2` returns existing `person`,
`contact_methods`, `tags`, `custom_fields` shapes, except
`person.inquiry_count` is an exact decimal string. It removes the complete
`inquiries` array and `core_history` array by using this new endpoint.
`inquiries` is now `{count:string,url:string}`. `activity` retains
`notes_count,open_tasks_count,completed_tasks_count,activity_revision,notes_url,
tasks_url`; its counts/revision are decimal strings. `history` is
`{read_revision:string,known_count:string,unknown_count:string,
counts:Record<Kind,string>,timeline_url:string}` with all eleven Kind keys,
including zero counts. Native facts contribute to known_count. Existing separate
notes/tasks readers are unchanged; activity_revision uses their existing revision
namespace, while all v2 readers use the new per-Person read_revision.

GET `/api/people/{id}/migration-review/inquiries?cursor=&limit=` returns
`{items:[{id,source,source_external_id:null|string,received_at}],next_cursor:null|string,
read_revision:string}`. It never selects Inquiry.message. Order is
`received_at DESC,id DESC`.

GET `/api/people/{id}/migration-review/timeline?family=&dated=&cursor=&limit=`
returns `{items:[TimelineSummary],next_cursor:null|string,read_revision:string}`.
`family=all(default)|native|events|calls|text_messages` and
`dated=known(default)|unknown` are closed filters. Native+unknown is an empty
page. No implicit date-range or outcome filter exists. Limits default 25/max 50,
minimum 1. Unknown query keys, invalid enums or malformed cursors are 400.

`TimelineSummary={kind,id,display_at:null|RFC3339,occurred_at,recorded_at,
actor:null|{id,display_name},origin,metadata:object,detail_url:string}`.
Kind is exactly `person_imported|inquiry_received|routing_decision|
assignment_changed|stage_changed|contact_attempted|call_completed|correspondence|
fub_event_record_imported|fub_call_record_imported|fub_text_record_imported`.
GET `/api/people/{id}/migration-review/timeline/{kind}/{entry_id}` returns the
same envelope plus `read_revision`, `correlation_id` and `provenance`.
Native provenance is null;
external provenance is `{plan_id,attempt_id,manifest_id,identity_id,
source_time_basis,stable_position:string}`. Kind+ID must resolve in the current
Org/Person binding; no cross-table UUID search. Source time basis is
`fub_record_created|unknown`. The native occurrence remains actual occurred_at;
contact-attempt correction display_at is recorded_at. External occurred_at and
actor describe the local import, never the source action. External display_at is
qualified source_created_at; unknown is null.

Native metadata matches the existing eight core HistoryEntry.detail allowlists:

- person_imported: import_id,plan_id,source_record_id,capture_id,on_behalf_of_user_id.
- inquiry_received: inquiry_id,source,person_created,matched_by.
- routing_decision: inquiry_id,strategy,assignee.
- assignment_changed: from,to,reason.
- stage_changed: from_stage,to_stage,reason.
- contact_attempted: channel,outcome,call_id,corrects_id,superseded.
- call_completed: call_id,outcome,talk_seconds,answered_at.
- correspondence: direction,agent,captured_at,via,backdated.

References use existing `{id,display_name}` or stage `{id,name}` shapes.
External metadata is exactly the approved Storage seam allowlist in this
contract; no body, subject, endpoint, message/thread ID, HTML, URL or media reader
is introduced. No operational call fold/outcome action belongs to these pages.

Known sorting reverses `(display_at,recorded_at,existing_rank,id,kind)` completely;
existing ranks are person_imported0,inquiry_received0,routing1,assignment2,stage3,
contact4,call5,correspondence6; external events9,calls10,texts11. The final kind
string discriminates otherwise-identical rank-zero UUIDs without changing the
existing tuple's order. Unknown sorting reverses
`(stable_position,recorded_at,rank,id,kind)` completely and never uses local import
time as historical source time.

Cursor AEAD binds purpose/Org/Person/original-parent/snapshot plus encrypted
endpoint,workspace_revision,read_revision,family,dated,limit,last full sort key.
A page-size/filter/endpoint/scope mismatch is 400; changed workspace/read revision
is 409 `history_refresh_required`. Refresh means discard cursor and start a new
series. Every request rechecks current admin membership and real imported-review
binding; cancelled/partial People parents remain readable. V2 works before any
history anchor. Source connection revocation does not block retained reads.
Anonymous/platform-only401,member403,foreign/missing404, malformed400,
stale409, corrupt/oversize503; all routes/errors no-store.

Core response maximum 512 KiB. Core variable collections are loaded in at most 51-row
chunks under the same snapshot, each SQL row caps serialized bytes at 512 KiB;
accumulated payload overflow is closed 503, never a truncated success. Inquiry and
timeline query at most limit+1<=51 candidates per fixed family, merge only bounded
candidates, decrypt only returned external rows; every summary <=4096 bytes,
page<=512KiB and detail<=16384 bytes. Oversize fails closed with retained evidence.
Core, revision/count lookup and all family queries run in one REPEATABLE READ
transaction with current membership/Org shared locks and the compiled capability
installed on the actual guarded connection. No task-local-only capability claim.

## Exact reader persistence inventory

`migration_history_review_state(organization_id,person_id,revision BIGINT,counts JSONB)`
is DB-owned. Exactly 18 nonnegative integer keys: inquiries,notes,open_tasks,
completed_tasks,person_imported,inquiry_received,routing_decision,
assignment_changed,stage_changed,contact_attempted,call_completed,correspondence,
fub_event_record_imported_known,fub_event_record_imported_unknown,
fub_call_record_imported_known,fub_call_record_imported_unknown,
fub_text_record_imported_known,fub_text_record_imported_unknown.
Readers convert each count to exact decimal strings and sum this fixed set only;
no read-time history/inquiry counts or MAX scan. Initial migration backfills with
fixed grouped scans; new People get state in the Person insert transaction.

Counts/revision writers: all eight native tables, inquiry, note (live only), task
(live open/completed), three external fact insertions and visibility suppression.
Contact-attempt corrector insertion also changes superseded and bumps the same
Person revision. Display removal, identity erasure and missing Person suppress
external facts; external deletion/suppression accounting belongs to import owner.
Display replacement bumps revision even when count is unchanged.
Core/metadata revision writers: Person, contact_method, person_tag,
person_custom_field_value; tag.name; custom_field label/type/position/archive;
custom_field_option label/references; app_user.display_name for Person assignee,
native actor/assignment/routing/correspondence-agent and external import executor;
stage.name for Person stage and from/to stage_changed references. Existing call
row existence/ID affects contact_attempted.call_id and invalidates affected People.
No routing-rule join is rendered. Original import binding is immutable and
workspace binding changes carry workspace_revision. Notes/tasks content stays in
its existing separate namespace; this revision covers their displayed counts.

Indexes: every native family `(organization_id,person_id,display_expression DESC,
recorded_at DESC,id DESC)`, contact display expression is CASE correction=>recorded
ELSE occurred. Inquiry `(organization_id,person_id,received_at DESC,id DESC)`;
primary contact `(organization_id,person_id,kind,import_order NULLS LAST,created_at,id)`.
Each external known index `(organization_id,person_id,source_created_at DESC,
recorded_at DESC,id DESC)` where nonnull; unknown replaces source_created_at with
stable_position where source_created_at null. Identity/display joins are tenant-
and-reference qualified before LIMIT. These bounds describe returned candidates;
actual scanned rows/loops/buffers require the opt-in EXPLAIN harness, not an
unqualified constant-work claim from LIMIT.
