# 010f2 concrete contract

D-068 implementation-owned contract, 2026-09-11. Implements the approved
[specification](../specs/SLICE_010f2.md); ordinary mutations are unchanged.

## HTTP and client seam

All routes require current active Organization admin, strict body ≤64 KiB and
`Cache-Control: no-store`. IDs are UUID strings; counts/revisions/source IDs are
decimal strings. Every mutation returns `{import: ActivityImport}`; GET detail
returns `ActivityImport`. New base: `/api/migrations/fub/activity-imports`.

| Route | Strict input |
|---|---|
| POST base | `{request_id,parent_import_id}` |
| POST `/{id}/plans` | `{request_id,expected_plan_id,choices:[{mapping_id,choice}],source_timezone?}`; max50 choices; omitted zone inherits, null clears |
| POST `/{id}/confirm` | `{request_id,plan_id,expected_revision,acknowledge_held,acknowledge_source_only}`; acknowledgements are exact displayed decimal counts |
| POST `/{id}/retry` or `/cancel` | `{request_id,expected_revision}` |
| GET base | optional `parent_import_id,cursor,limit` |
| GET `/{id}` | no body |
| GET `/{id}/records`, `/mappings`, `/results`, `/targets` | optional `plan_id,cursor,limit,kind,disposition,issue` as applicable |
| GET `/{id}/records/{row}/observations` | optional `plan_id,cursor,limit`; every retained occurrence, including linked negative detail evidence |
| GET `/{id}/{records\|mappings\|results\|observations}/{row}/fields/{field}` | optional `plan_id,cursor,limit` (field segment size4–65536 UTF-8 bytes) |

Choice is one of `{kind:"hold"}`, `{kind:"leave_unmapped"}`,
`{kind:"map_existing",target_id}` (user roles), or
`{kind:"map_kind",native_kind:"call"|"email"|"text"|"follow_up"|"other"}`
(task-kind groups). Source role tokens: `note_author`, `task_creator`,
`task_assignee`, `task_kind`. Suggestions never count as choices.

ActivityImport contains `id,parent_import_id,parent_plan_id,snapshot_id,
source_account_id,capture_sequence,workspace_revision,revision,activity_revision,
engine_version,state,phase,pause_reason,created_at,updated_at,confirmed_at,
confirmed_plan_id,completed_at,retained_bytes,reserved_bytes,native_row_bytes,
cancellation_reserved_bytes,release_ready,counts,policy,coverage,latest_plan,actions`.
States: preparing,ready,queued,running,paused,completed,cancelled.
Phases: preparation,records,complete. `actions` has replan/confirm/retry/cancel.
`latest_plan` contains id/revision/state/phase/expires_at/counts/source_timezone/
source_engine/html_profile/time_profile/tzdb_version/confirmation_digest/
max_added_byte_bound. Plan states: building,ready,paused,superseded.

Counts contain notes/tasks each with planned/eligible/applied/already_present/held/
pending, plus held_count/source_only_count/invalid_occurrences/unavailable_bodies.
`policy` preserves 010f1's run/org limits, current ceilings, retained/reserved
totals, unit limit and policy revision. Native row bytes are a separate measured
value; PostgreSQL indexes are measured in verification, not charged to retention.

Pages return `{items,next_cursor}`; child list uses `{imports,next_cursor}`.
Record rows include id,kind,source_id,person_id,target_id,disposition,source_summary,
preview, `field_url` and `observations_url`. Preview includes exact normalized native text/times,
native_kind,reasons,transformations,source_only and observation count. Task preview
source_type is bounded to1024 code points with source_type_abbreviated and
source_type_full_utf8_bytes; exact source attribution remains in retained fields. Mapping
rows include id,role,source_value (first1024 code points),
source_value_abbreviated,source_value_full_utf8_bytes,choice,suggestions,
suggested_kind,dependent_count,
source_summary and field URL. Targets expose same-Org member id/display_name/
status/role. Result rows include id,kind,source_id,person_id,target_id,disposition,
committed_at,reasons,source_summary,field URL. Every page ≤50/512KiB; summaries
≤128KiB, full retained fields remain exact through authenticated UTF-8 segments.
Observation rows include id,capture_id,record_id,stream,representation,negative,
source_id,field_url,raw_url. The observation raw field segments the exact UTF-8
capture bytes; all other source fields retain the exact source values in their
structured provenance envelope.
Field responses: `{text,offset,next_offset,total_utf8_bytes,next_cursor}`.
`field=all` preserves source plus operation/preview structure; summaries use
existing hashed field-key convention. Cursors bind Org, child, frozen plan,
revision, endpoint, filters, row and field as applicable.

401 unauthenticated/platform-only;403 current member;404 missing/foreign scoped
resource;400 malformed body/UUID/choice/cursor;409 stale revision, expired plan,
incompatible evidence, all-held fresh confirmation or mismatched receipt input;
503 unavailable/corrupt retained evidence. Successful receipt replay rechecks
authority before returning the immutable response and does not recheck freshness.

Native review routes are `/api/people/{id}/migration-review`, its `/notes`,
`/notes/{note_id}`, and `/tasks`. Core returns
`{person,contact_methods,inquiries,core_history,tags,custom_fields,activity}`;
`activity` has decimal `notes_count,open_tasks_count,completed_tasks_count,activity_revision`
and `notes_url,tasks_url`. Core contains no note bodies or task arrays;
core_history contains the existing `person_imported`, `inquiry_received`,
`routing_decision`, `assignment_changed`, `stage_changed`, `contact_attempted`,
`call_completed` and `correspondence` facts. This inventory correction under
D-072 documents existing wire behavior; it adds no 010f2 fact family. The new
010d2 v2 representation pages these families; a confirmed history anchor fences
this complete core route. Review
eligibility requires a valid same-Org original review binding, including a
partially completed/cancelled People parent; preparing a new activity child
requires the stricter completed-parent gate.

Native note/task pages return `{items,next_cursor,activity_revision}`; limits1–50,
default25. Tasks require `state=open|completed`. A note summary contains
`id,created_at,updated_at,author:UserRef|null,can_manage:false,provenance:Ref|null,
excerpt,has_more`; excerpt≤512 code points. A full note replaces excerpt/has_more
with `body`, within the unchanged native limit. Task items contain
`id,title,kind,due_at,completed_at,created_at,updated_at,assignee,created_by,
completed_by,can_manage:false,provenance:Ref|null`; user references are nullable.
Ref is `{activity_import_id,result_id,source_account_id,source_id,source_url}`;
source IDs are decimal and source_url is the exact result `/fields/all` above.
A stale native cursor is409 `activity_refresh_required`; malformed/foreign cursor
is400. All authenticated errors and success responses are no-store, including
workspace/current-admin middleware rejection. Legacy complete reads raise SQLSTATE P010F →409 activity_review_required
under the shared guard once any child confirmed_plan_id exists. First confirm
takes the exclusive workspace barrier. Capability `fub-activity-import-v1` covers
worker and bounded readers; five-minute operator report remains fail-closed.

## Extraction, conversions and persistence

Every command, page and worker validates the child's exact parent plan, snapshot,
preview, source account, capture sequence and original workspace revision against
the eligible completed parent.

Profile `fub-activity-source-v1` requalifies lossless raw captures, source record
links, stream/family/representation, identity, full semantic HMAC, status/length
and encrypted tenant/purpose scope. Every collection request fingerprint is
reconstructed from its authenticated prior accepted checkpoint; encrypted child
observations carry request/next-cursor context, including users next tokens and
local positions. Snapshot persistence is unchanged. Parseable observations from
rejected invalid-ID/no-progress/loop attempts remain inspectable, with invalid
occurrences and variants explicitly held; an exact accepted observation can
supply the native projection after a source retry. Notes list/detail remain separate; tasks
share one representation. Every occurrence has a source row; variants are
queried with bounded keysets, never stored as an unbounded JSON array. Negative
detail404 is linked through the retained request fingerprint, not a guessed ID.

HTML profile `fub-note-readable-v1/html5ever-0.39.0` uses exact pinned html5ever
0.39.0 tokenizer, strict bounded element stack, no DOM/network. Bounds:4MiB input,
100k tokens,64 nesting levels,128 attributes/tag,40k output bytes before native
10k-code-point validation. Supported text structures and holds follow §4.
Time profile `fub-task-time-v1/chrono-tz-0.10.4` pins chrono-tz0.10.4 and records
IANA_TZDB_VERSION `2025b` in each plan. Explicit offsets preserve exact
PostgreSQL microseconds; unqualified/naive/unknown-offset/leap-second values hold.
Date conversion freezes zone/profile/tzdb and resulting exact UTC instant.

Migration `20260920000001_fub_activity_import.sql` owns import,plan,choice,source,
mapping,manifest,manifest_issue,identity,result,result_issue,receipt,reservation,issue. All references include
Organization. Identity unique(org,account,kind,source_id) has no native-target FK;
native source key is `v1:<account>:<source-id>`. Choice rows support bounded
inheritance; all raw observations remain linked without source payload rewrites.

Logical retained-byte inventory is measured by additive PostgreSQL triggers:
256 bytes per durable row covers fixed identifiers/numbers/timestamps and row
control overhead; exact bytea lengths cover encrypted nonce+ciphertext for plan
patch/destination, choices, observations, mappings, manifests, results, receipts,
and semantic/source/input/confirmation digests. Exact UTF-8 text lengths cover
state/phase/pause reason, family/source IDs/native keys/stream/representation,
kind/disposition/action/issue codes and canonical JSON counts. Encrypted source
and reason/operation structures are counted within their ciphertext. The durable
stores include supporting manifest-issue and immutable committed-result-issue
rows. Result issue filters use committed execution reasons, preserving independent
plan issue filters. The closed `source_only_counts` JSON projection is counted
exactly as canonical JSON text; it contains only reason codes and counts derived
from requalified retained observations. Exclusions count components across
distinct observations: exact canonical duplicates within a representation collapse,
while complementary list/detail observations remain separate, with exact raw
inspection. They are not globally unique attachment/reply counts.
Changing a measured variable
column applies its delta once. `measured_bytes` is the internal cumulative
measurement, settled to public `retained_bytes` and shared snapshot/Org totals
inside the same unit transaction. Fixed numeric/revision columns do not increase with decimal display width;
serialized JSON count lengths are measured on every change.

Transient reservations are excluded from the retained inventory: admitted
capacity covers their descriptor until it is deleted on settlement. Only the
owning child's work/cancellation reservation can be released. Preparation reserves
64KiB cancellation capacity; bounded pause control state consumes it, clearing a
pause refunds it, and final cancellation settles its immutable receipt and
releases only that child's remaining capacity. New work is admitted against the
existing shared snapshot/Org limits and current operator ceilings; one native
note/task atomic unit is bounded by64MiB. Native `pg_column_size` row bytes and
verification index sizes are separate from retained import storage.

Application role gets select/insert/update only on mutable import/plan/mapping/
issue, select/insert on immutable choice/source/manifest/manifest-issue/identity/
result/receipt, and select/insert/delete plus narrow UPDATE(byte_count) on child
reservation for bounded pause/refund settlement. Receipt append-only trigger stays.
`crm_activity_insert_allowed` accepts only exact planned note/task INSERT with
confirmed parent/result/identity/livePerson, current executor, unexpired lease,
exact native UUID/Org/source pair/migration origin/frozen nullable actors and
active assignee. Existing 010c, metadata and terminal-call guard branches persist.
No new body-bearing facts/realtime/Operator/notification work is emitted.
