# Slice 010f2 — Notes and tasks in the migration review workspace

**APPROVED FOR IMPLEMENTATION — D-068, 2026-09-11.** The user approved the complete
independently reviewed specification, policies, shared contracts and isolated
synthetic verification with “Ok go ahead and implement 010f2.” D-067 records the
two earlier source-interpretation choices. Baseline main `cd3b010`; deployed source
`e36ce36`. Live source/customer processing and deployment remain outside scope.

Inputs: [decisions](../decisions/DECISION_LOG.md) D-012/015/027/050/053/054/063–067,
[architecture](../architecture/ARCHITECTURE_BASELINE.md), [010b](SLICE_010b.md),
[010c](SLICE_010c.md), [010f1](SLICE_010f1.md), [notes](SLICE_015.md),
[tasks](SLICE_016.md), [code/source evidence](../research/SLICE_010f2_CODE_CONTRACTS.md)
and [execution brief](../tasks/SLICE_010f2_IMPL.md).

## 1. Outcome and boundaries

An Organization admin can preview, map and explicitly confirm native notes and
tasks from the retained snapshot that produced a **completed 010c People import**.
Only those exact imported People can receive activity. The original People plan,
results, identities and review binding remain unchanged. 010f2 is a sibling of
010f1; metadata-child completion is not a prerequisite and no new FUB read occurs.

One activity child per parent for this first step, with independent source-note
and source-task units. A terminal cancellation is final for this child and keeps
committed records. A later repair/delta slice owns additions after terminal
completion/cancellation. D-068 accepts this lifetime rule.

In scope: main-note bodies/subjects, qualified source authors and timestamps,
open and completed tasks, explicit user/type/date policies, retry-safe inserts,
reconciliation, retained-source provenance and bounded administrator review.
Preserve the 10,000-character note and 500-character task-title limits.

Replies/reactions remain inspectable retained evidence, not flattened into new
native notes. Attachments, reminders, recurrence/action plans, task descriptions,
external calendar behavior and unknown properties are explicitly classified as
source-only or held. A native main note/task does not imply these are imported.
Notes/tasks may proceed independently after counted source-only/held exclusions
are explicitly acknowledged. No arbitrary first-N records or silent truncation.

No new People, contacts, Inquiries, historical contact attempts, task-completion
facts, communications, Operator mutation tools, native mobile UI, activation,
customer-data processing, general operational timeline redesign or infrastructure.
The admin review hold continues to block ordinary edits, Today, Operator and
outbound actions, including overdue imported tasks. Existing operational
Organizations retain their current behavior.

## 2. Declared shared-contract changes

D-068 assigns the 010f2 implementation owner the following shared-contract
changes. This table is the approved AGENTS §11 declaration.

| Current contract | Proposed contract and reason | Consumers, compatibility and amendment |
|---|---|---|
| 010c imports People; 010f1 owns a separate metadata child | Add activity child commands, plans, manifests, identity, results, receipts and reservations, scoped to the same immutable parent | Rust/API/Web and tests; one additive migration, no applied-migration edits. Amend 010b/010c/010f1 by pointers for sibling accounting and review reads, not by rewriting their historical shapes. |
| `AddNote`/`CreateTask` require operational workspace, current actors/time and ordinary defaults | Narrow internal typed `ImportRetainedNote`/`ImportRetainedTask` operations accept only a validated frozen manifest and a private activity permit | Domain/DB worker only; ordinary commands and their HTTP bodies unchanged. No `Origin::Migration` bypass. |
| Note/task native source pairs are nullable; earlier schema-only import examples proposed a bare FUB record ID | Store `source=fub`, account-qualified application key `v1:<account-id>:<record-id>`; keep actual vendor ID separately in scoped activity provenance | 015 §2 / 016 §2 future-import examples amended. Existing source pairs remain untouched; ambiguous legacy bare-ID bindings are held, not adopted or duplicated. |
| Person detail and direct history/open-task reads fetch all notes/completed/open tasks | Add a distinct bounded `MigrationReviewPerson` representation and paged native activity reads; after first activity confirmation, legacy complete reads return `409 activity_review_required` before loading activity | API/Web shared contract; scoped to review workspaces with a durable confirmed activity child. Older clients get an explicit error, never an apparently empty complete history. 002/015/016 and 010c review-read pointers amended. |
| D-053 body exposure sites include current detail/receipt and bounded Operator view | Add admin-only paged native review note/body reads; retained raw evidence stays under existing migration authorization | D-068 accepts this D-053 read-surface amendment; no broader audience, realtime/ledger/body-log exposure or plaintext provenance history. |
| Preflight knows workspace and metadata capabilities | Add `fub-activity-import-v1` for activity worker **and bounded review readers**, with separate activity readiness and bound-state recovery checks | Existing preflight/API startup/operator-owned report; old reports cannot confirm activity. Existing metadata/People readiness fields stay compatible. No new workspace mode or generic release framework. |

Ordinary Note/Task mutation DTOs, native `kind` enum, Today ranking and Operator
schemas remain unchanged. Review responses expose `can_manage=false`. New
readiness/error variants and Web cache keys must be frozen in the concrete
contract before implementation of clients begins.

## 3. Source qualification and frozen boundary

Require the same Org/account/snapshot/final capture sequence/parent confirmed
plan/original workspace revision as the completed parent. The retained snapshot
must be terminal `completed` or `completed_with_gaps`, with completed `people`,
`users`, `notes`, `note_detail`, `tasks_open` and `tasks_completed` streams.
Both activity families must finish enumeration before this combined child is
preparable; there is no implicit notes-only fallback from incomplete tasks.
Settled note-detail item gaps may be held individually. Custom-field stream
completion is not an additional activity prerequisite. Parent exclusions remain
exclusions; missing, held or erased parent People cannot be recreated.

Requalify exact encrypted raw captures and record links: profile, request/stream,
representation, response status, capture/item identity, scoped AEAD and HMAC,
canonical source ID and parent binding. Never execute the clipped 010b preview,
its `reviewable` disposition, arbitrary JSON from the browser, or a source URL.
Use the existing bounded lossless parser: no duplicate decoded keys, no lossy
numbers and no source text in diagnostic output. IDs remain positive decimal
strings, bounded by the existing 128-digit rule; account IDs use the established
validated account representation. No floating-point/JavaScript-number IDs.

Source family and request stream are distinct: `note_detail` belongs to `notes`,
and both task streams belong to `tasks`. Notes list and enriched detail are
complementary representations, not competing versions. Only ID equality is
qualified across those representations today. A successfully captured,
unambiguous detail is the executable note source; never fill absent detail
body/author/time fields from list data. Keep list provenance and detail gaps
visible. Restricted detail 404 is inaccessible content, not a deleted note.
Link a settled negative capture to its requested ID through its retained request
fingerprint; a completed detail stream alone proves no body availability.

All observations within the same representation participate. Equal canonical
records may collapse; differing variants, including a task changing between
open/completed partitions, are held. Capture order, `updated` timestamps and
stream order cannot select a winner. Invalid-ID occurrences remain countable by
capture/ordinal without invented vendor IDs. Unexpected fields are retained and
classified explicitly; differing unknown-field variants are not erased by a
narrow projection. Broken/missing retained evidence pauses work rather than
being mislabeled as a valid but unsupported source record.

## 4. Native mapping policies

### Notes

D-067 accepts a readable plain-text version with the exact original retained.
`isHtml=false` uses the existing `NoteBody` normalization/validation. A missing,
null or nonboolean format flag is held until its source representation is
qualified; text containing `<` is not HTML merely because of that character.
Missing/null/non-string body is held. Empty body with a nonempty qualified subject
may produce a subject-only note; an entirely empty result is held.
`showContent=false`, an explicit inaccessible-content marker or contradictory
visibility metadata holds the note even if a body string happens to be present.
Retain these restrictions for later activation review; source capture does not
authorize wider access. A subject must be a qualified string and its plain-text
normalization is disclosed; do not interpret subject markup as executable HTML.

For HTML, use a bounded non-executing parser, not regular-expression stripping,
a browser DOM, remote resource loading or an LLM. Freeze a versioned conversion
profile before implementation: paragraph/division/heading and `<br>` boundaries
become line breaks; list items retain bullets/order; emphasis retains its text;
entities decode; `<pre>` preserves spacing; anchors retain visible text and the
literal destination as readable text. Never fetch or execute a destination.
Whitespace changes and loss of styling are shown in preview. Subject becomes
`Subject: <subject>` followed by two newlines and the converted body when both
exist. Normalize once through `NoteBody`; display the exact proposed output.

Unsupported content-bearing elements such as images, tables, scripts, styles,
embedded frames/SVG or malformed/over-budget structure hold that main note in
this version. Preserve their original bytes and explain the reason. Do not
discard content to get under the native cap. Replies/reactions are retained
separately with counts and inspectable exact data, never flattened or treated as
already imported. Their presence requires source-only exclusion acknowledgement
before an otherwise representable main note is executable.

### Tasks

`name` must pass `TaskTitle`; no shortening, multiline flattening or concatenation
of a description. Preserve actual source type. Exact qualified `Call`, `Email`,
`Text`, `Follow Up` types may suggest native counterparts. Admin explicitly
accepts mappings; other source types remain held unless mapped to a native kind
(including `other`) with the original type visible. Mapping an `Appointment`
task creates a task, not a calendar appointment. No new enum variants.

Accept documented boolean or exact integer 0/1 completion forms; distinguish
missing/null/invalid. State must agree with its capture partition. Open tasks
require no non-null completion time. Completed tasks require a valid source
`completed` instant; never use `updated`, due time or import time as completion.
Missing/contradictory completion data is held. `updatedBy` is not a qualified
completer identity: store `completed_by_user_id=NULL` unless a separately
qualified explicit source-ID field exists in the frozen profile. No completion
actor is invented and ordinary completion commands are not called.

Due policies distinguish undated, timed and date-only work:

- Both due fields absent/null means a deliberately undated task, disclosed and
  stored with `due_at=NULL`. Empty/malformed fields are not treated as absent.
- A valid explicit-offset `dueDateTime` supplies the instant. A naive timestamp
  is held; do not assume UTC, browser time or server time. When both due fields
  exist, retain both; a date inconsistent with the instant in the explicitly
  confirmed source timezone is held. If checking that date requires a zone not
  yet confirmed, hold the task with that reason. Do not silently pick one field.
- D-067 accepts date-only conversion to the end of that date in an
  **admin-confirmed IANA source timezone**. Freeze the zone, tzdb version,
  conversion profile and each resulting instant into the plan. A timezone shown
  on a captured user is only a suggestion. The admin must confirm that the zone
  applies to the selected date-only records; conflicting captured zone evidence
  is held in this first version rather than applying an unapproved per-user
  fallback. A missing zone keeps date-only tasks held; independent tasks can
  proceed through explicit subset confirmation.
- End of day means the last whole second before the next local date begins,
  verified to belong to the source date. Test normal days and DST transitions.
  Invalid/skipped dates or a boundary that cannot be resolved deterministically
  are held. A replan changing the zone recomputes instants; retry never recomputes
  a confirmed instant with a newer tzdb. Sub-microsecond timestamps that cannot
  be represented exactly in PostgreSQL are held, not silently rounded.

Descriptions, reminder offsets, external task/calendar references, priority or
recurrence metadata have no new native behavior here. Retain them, classify them
as source-only and require counted acknowledgement when meaningful. Do not claim
future Action Plan tasks were captured from already materialized task rows.

### People, users and timestamps

Resolve People exclusively through the completed parent's scoped source identity,
committed result and live native target. Do not match by name/contact or parent
assignment. Different source note/task IDs with identical text stay separate.

Mapping groups identify source user IDs and role (`note_author`, `task_creator`,
`task_assignee`; no invented completer role). Captured user evidence and the
parent's approved choices may suggest targets, but each activity-role mapping
requires explicit confirmation. Never auto-match display names. Historical
authors/creators can map to existing same-Org active or inactive memberships;
task assignees must map to a currently active member or explicitly remain
unassigned. Revalidate under locks. Inactive/missing targets hold dependent
records when the frozen role requires active membership. No account/invitation
or authentication identity is created.

An explicit `leave_unmapped` stores nullable native actor/assignee fields and
retains exact source attribution in encrypted provenance. A name without a
qualified user ID cannot become a CRM user mapping; show it as source-only
attribution, with `author`/`created_by` null. Do not substitute the importer or
Person assignee. The preview explains that future note-author/task-owner mapping
affects ordinary edit rights after activation; the current hold overrides them.

Require a valid explicit-offset `created`. A valid `updated >= created` is
preserved. If `updated` is absent/null, native initial `updated_at=created_at`
is an explicit initialization rule, with source update time marked unknown;
it is not represented as proof the source was never edited. Invalid/inconsistent
timestamps are held. Completed time may be later than `updated`; that is not a
conflict. Keep import/settlement time separately in the child result. Native
CRUD history remains a projection, never a new immutable body-bearing event.

## 5. Plans, identity and atomic writes

The child freezes source/account/boundary, parent/plan/workspace revision,
extractor/converter/tzdb versions, role/type/date choices, observed destination
state, dispositions, native target UUIDs and byte inventory in immutable plan
revisions. Patches create a new revision; no hidden mutable approved mapping.
Suggestions are non-executable. Dirty Web choices must be applied/discarded before
confirmation. Replanning is allowed only before confirmation; no post-confirmed
mapping repair or source takeover. Show separate record counts, source-only
components, invalid occurrences, unavailable bodies and unresolved references.
Fresh confirmation requires at least one eligible native note/task unit, including
a verified `already_present` unit. Reject an all-held plan without consuming its
confirmation; the admin can replan. Zero committed inserts after cancellation or
post-confirmation revalidation remain valid recorded outcomes.

One atomic work unit is **one source note or one source task**, not an entire
Person's activity history. A note/task never writes partially. Different valid
records can execute after explicit acknowledgement of held/source-only items.
Every source record receives a terminal primary result (`applied`,
`already_present`, `held`) or remains explicitly unprocessed after cancellation;
nonexclusive issue counts do not inflate record totals. Completion with held
records is not a complete migration/cutover claim.

One additive migration uses separate `migration_activity_*` ownership:

| Table suffix | Required owned state / keys |
|---|---|
| import | Org, unique parent, parent confirmed plan, snapshot/account/capture sequence, original workspace revision, executor, lifecycle/phase/lease, confirmed/latest plan, progress/activity revisions and work/control counters |
| plan | Immutable revision, source/representation/conversion/tzdb versions, encrypted choices/destination observations, checkpoints, counts, digest/expiry; composite child/Org key |
| source | Exact retained record/capture/representation or linked negative evidence, source family/ID, bounded variant references and encrypted derived content; no list/detail synthesis |
| mapping | Scoped server mapping ID, user-role/type group, frozen explicit choice/target and validation reasons; encrypted source labels/choice detail |
| manifest | One source note/task and parent Person identity/result, deterministic native target, account-qualified source pair, approved mapped IDs, encrypted exact native attributes, disposition/holds and byte bound |
| identity | Unique Org/account/kind/source-ID to native target and immutable child/manifest provenance, without target cascade; no content in durable tombstones |
| result | One unique settled manifest result per child/Org, target IDs, primary outcome, closed issue counts, encrypted necessary provenance, actual bytes and import time; indexes for family/status/Person paging |
| receipt | Org/actor/action/request ID, exact input digest and encrypted immutable committed response |
| reservation | Child/plan/snapshot/Org/lease owner, work/cancel purpose, bounded bytes and fence; independently releasable |
| issue | Closed nonexclusive issue-code counts by plan/Org, settled with its unit |

Every tenant reference uses composite Org keys. Index scans must advance with
bounded keysets; supporting variant/choice references may use child-owned rows
where needed rather than an unbounded JSON array. Do not expand old parent/source
family CHECKs, change raw capture storage, or make the new identity depend on
native rows remaining present. The concrete contract must enumerate every counted
variable column and every new grant/guard before implementation.

Identity is unique on Org/source-account/entity-kind/source-ID. Native source
pairs use the account-qualified key from §2; root note and task IDs inhabit their
separate native tables. Identity retains its target without a cascading target
FK so erasure/deletion cannot authorize resurrection. Missing native targets and
native tombstones are held. A bare legacy source key is an ambiguous binding,
never adopted solely by comparing text or the current account. Do not rewrite it.

For an account-qualified existing source pair, require matching Person, origin,
source pair, normalized body/title, mapped actors, kind, due/completion and
created/updated timestamps before recording `already_present`. Differing local
content, reassignment, snooze, completion/reopen or timestamp edits are held;
never UPDATE the native row. A matching row may establish the missing child
identity/result atomically without modifying it. An existing identity pointing
elsewhere or an equal-looking row without a compatible source pair is not a
deduplication match. Original author/source data remains in retained provenance.

Create through private typed import operations using existing validators and a
closed frozen manifest. The DB permit checks Org, activity child/confirmed plan,
executor membership, unexpired lease/token, exact note/task UUID, source pair,
migration origin, frozen mapped user IDs and Person,
parent identity/result and unchanged review binding. Permit only the planned
native INSERT; no general UPDATE/DELETE, sibling metadata write, new Person,
fact, stage or ordinary mutation bypass. `origin=migration` alone grants nothing.
Keep all existing workspace/metadata/terminal-call guard cases intact.

Within a unit, native INSERT or equality classification, durable identity,
encrypted result/provenance, progress revision and exact reservation settlement
commit together. Injected failure rolls all of them back. Unique constraints,
row locks and lease fencing provide correctness under retries/worker overlap.
Use existing workspace → current membership → Organization → child/plan → Person
and native locks, with deterministic ordering for multiple membership IDs.
Metadata and activity units may coexist but serialize shared Org/retention
admissions; cancelling one cannot release the other's reservation or modify its
results. No new deployable service or source downloader is needed.

## 6. Lifecycle, budgets and recovery

Follow the existing child lifecycle: preparing → ready → queued → running →
completed; paused work needs an explicit retry; cancellation is terminal.
Retain preparation/phase checkpoints and fenced leases. Current admin authority,
source-free retained-read access and parent eligibility are checked on every
command/read and again before commit. Disconnecting FUB does not delete evidence
or prevent retained processing. Demotion/deactivation pauses work before another
write. Explicit Retry adopts the currently authorized admin as executor, following
010f1's declared rules; a worker cannot silently take over or adopt another admin.
Receipt replay always rechecks current authority.

Reuse request receipts bound to Org/actor/action/request ID and digest of exact
validated inputs. Same input replays the immutable committed receipt only after
current access checks; different input conflicts. A successful confirm replay
does not require a still-fresh old plan/report or repeat writes. Fresh confirmation
requires an unexpired plan, unchanged mapped targets/policies and fresh activity
capability evidence, plus at least one eligible native unit under §5. Retry
resumes the same confirmed bytes; budget increase and
resume remain separate authorized actions.

Define the exact retained-byte inventory before coding: encrypted derived source,
choice snapshots, manifests, provenance/results, receipts, variable control
columns, identifiers and reservation overhead. Reuse the snapshot and Org ledgers,
010b approved allowances/operator ceilings, bounded per-unit reservation (existing
64 MiB maximum) and separately reserved cancellation/receipt capacity. Native
note/task row/index bytes are measured and reported separately; they are not
silently charged as retained-source allowance or presented as physical disk quotas.
No new customer quota or storage product. Admission checks use current ceilings;
settlement uses actual bytes, releases only its own reservation and never makes
counters negative. Exhausted cancellation must still stop unstarted units.

Add a distinct release capability `fub-activity-import-v1`; API/worker artifacts
must support both typed activity execution and bounded review reads. Preserve
workspace/metadata capabilities and existing preflight response meanings; add
activity-specific readiness/reasons. First activity confirmation takes the
**existing exclusive workspace barrier** to install the durable read boundary
in §7. A direct legacy reader holding a shared guard finishes before that boundary;
a later reader fails before loading activity. Worker units use ordinary shared
workspace/Org synchronization. This prevents a check-then-fetch race.

Persist the boundary via `confirmed_plan_id IS NOT NULL`, including completed or
cancelled children with zero writes. After that boundary exists, startup/launch
preflight must reject old activity-incapable API/readers as well as workers, even
if old workspace/metadata gates pass. A five-minute operator report permits
confirmation only while fresh; no auto-renewal. Recovery preserves schema, rows,
source, identities and review binding and uses verified activity-capable artifacts.
Never clear a binding/confirmed plan or restore the whole DB to run old software.

## 7. Bounded administrator review and HTTP

Add `/api/migrations/fub/activity-imports` with typed prepare, replan, confirm,
retry, cancel, status/list, source-record, mapping, result and full-field reads.
Use the established 010f1 route/error/receipt conventions with distinct IDs,
crypto purposes and query keys; freeze exact DTOs in the owned concrete contract
before Web work. Proposed command bodies:

| Action | Body and effect |
|---|---|
| `POST /activity-imports` — `PrepareActivityImport` | `{request_id,parent_import_id}` → child plus asynchronous immutable initial plan; no native writes |
| `POST /activity-imports/{id}/plans` — `PlanActivityImport` | `{request_id,expected_plan_id,choices:[{mapping_id,choice,target_id?}],source_timezone?}`; at most 50 patches, creates revision; explicit null zone clears pre-confirm choice; zone covers date-only conversion and dual-field consistency checks |
| `POST /activity-imports/{id}/confirm` — `ConfirmActivityImport` | `{request_id,plan_id,expected_revision,acknowledge_held,acknowledge_source_only}`; exact counted preview, at least one eligible native unit, barrier/capability check, queue; all-held plans return 409 and remain replannable |
| `POST /activity-imports/{id}/retry` / `/cancel` | `{request_id,expected_revision}`; resume only permitted paused phase, or terminal cancellation retaining settled units |
| `GET /activity-imports` / `/{id}` / `/{id}/records` / `/{id}/mappings` / `/{id}/results` | Scoped stable pages/status; filter by closed family/disposition/issue tokens; record/mapping field reads preserve exact source via bounded UTF-8 segments |

Every new endpoint revalidates current active Org admin. Use 401 for unauthenticated/
platform-only, 403 for member, 404 for foreign/missing scoped resource, 400 malformed
strict body/UUID/cursor, 409 stale/conflicting state or incompatible evidence,
503 unavailable/corrupt retained evidence. Preserve existing parse/auth precedence
where routes already define it. All source/native review responses are no-store.
Mutation body ≤64 KiB; pages ≤50 and ≤512 KiB; individual source summaries
≤128 KiB; full fields stream 4–65,536 UTF-8 bytes with scalar-boundary progress.
Never hide missing content behind an ellipsis with no exact-read path. Cursor AEAD
binds Org/child/plan/revision/endpoint/filter/record/field as applicable.

Native review uses distinct additive routes:

- `GET /api/people/{id}/migration-review` returns
  `MigrationReviewPerson {person,contact_methods,inquiries,core_history,tags,
  custom_fields,activity:{notes_count,open_tasks_count,completed_tasks_count,
  activity_revision,notes_url,tasks_url}}`. `core_history` explicitly excludes
  notes and completed tasks; there is no falsely complete `history`/`tasks` array.
- `/migration-review/notes` lists native note summaries/excerpts in
  `(created_at,id)` order; `/notes/{note_id}` returns a bounded native body and
  import/provenance references. No unbounded bodies in list queries.
- `/migration-review/tasks?state=open|completed` lists native tasks, ordered by
  `(due_at NULLS LAST,id)` or `(completed_at,id)`. Per-item provenance is scoped to
  that Person and actual source identity. Source authors are labeled separately
  from linked CRM users; no fabricated UserRef or edit authority.

These routes require current admin plus the real review binding and scoped live
Person, also before activity confirmation; members/foreign resources remain
denied. The DTO bounds and row limits above apply to native pages too; note
excerpts are at most 512 code points and full bodies stay within native limits.
Counts are decimal strings. Page cursors bind Person, state and observed activity
revision; a committed activity change invalidates an older cursor with explicit
refresh-required 409. Web discards that page series and refetches; it never merges
different source revisions or claims a running import has a complete stable list.
Terminal unchanged imports can be paged fully without omissions or duplicates.

Once any activity child is confirmed in that review workspace, legacy
`GET /api/people/{id}` and complete direct-domain activity/history paths reject
with `activity_review_required` **inside their shared workspace transaction and
before any unbounded fetch**. New bounded core queries never call those paths.
Existing operational reads and review-before-activity legacy behavior remain
compatible. No client parameter grants a review bypass. Future activation must
first establish bounded operational readers; it cannot merely remove this guard.

## 8. Web, privacy and telemetry

Migration UI adds “Import notes and tasks” for a completed People parent. Present
per-family coverage, source-only replies/settings, exact text conversions, user
and kind mappings, a named source-timezone confirmation and source→native date
preview. Show unmapped roles and resulting future edit rights. A named final
dialog states eligible/held/source-only counts and retained-work cancellation.
Plan preparation or selecting a suggestion does not confirm native writes.

Review Person UI uses the distinct bounded representation for the migration
workspace, with separate read-only Notes, Open tasks and Completed tasks cards,
explicit core-history scope and full note/source inspectors. Ordinary composer,
edit/complete/snooze/delete buttons remain absent, even for admins. Source-only
content is visible without interpreting HTML or loading external URLs. This is
responsive Web; no SwiftUI/Compose implementation is implied.

Reuse current workspace refresh, Org/actor/role fencing and same-identity uncertain
request replay. Poll only active work; stop on pause/terminal. Refetch authoritative
status/results after reconnect and on focus; never trust missed realtime events
as state. Reject late pages and receipts after identity changes, and discard
changed-revision page series. No source payload enters the Operator. UI content,
HTML, names and URLs remain untrusted text.

Use the existing plaintext erasable native note/task columns and encrypted
retained-source/control storage. Results keep scoped source references, hashes,
closed codes and encrypted necessary provenance, not permanent plaintext bodies,
subjects or source author labels. Minimal durable identity tombstones contain no
content; deletion/erasure cannot resurrect it. Add all new stores/read models to
future D-015/O-013 erasure inventory without pretending that runbook is complete.

Instrument bounded phases, IDs where safe, closed pause/hold reasons, lease/retry
outcomes, source/destination counts, retained bytes and conversion version.
Never log body/title/HTML/subject/URL/author text, raw records, credentials,
ciphertext or cursors. No bodies in immutable facts, realtime or Operator ledger.
Worker writes do not fan out notification/reminder/call/text jobs. Status queries
and explicit refetch recover progress using the existing in-process workload.

## 9. Acceptance and verification

| ID | Observable requirement | Required evidence |
|---|---|---|
| A1 | Completed parent, correct streams/account/boundary and immutable review binding; sibling independence | Domain/DB/HTTP tests: incomplete family, settled detail404, wrong stream/family, changed parent, disconnected source, missing target, zero new source calls |
| A2 | Exact source qualification; no projection or last-wins import | Raw encrypted fixtures: duplicate keys, invalid IDs, clipped bodies, note list/detail mismatch, same-ID task partition variants, unknown-only variants, missing/wrong key/capture; corrupt evidence pauses |
| A3 | Readable note conversion with exact retained original and no silent content loss | Pure fixtures for literal text, entities, paragraph/list/pre/link/subject, Unicode/control chars, unsupported HTML/attachments, replies/reactions, malformed/parser budgets and 10,000/10,001 limits; exact UTF-8 segmented reads |
| A4 | Task state/kind/date mapping is explicit and faithful | Fixtures for boolean/0/1 states, completed time absent/valid/contradictory, non-completer updatedBy, undated/timed/date-only, source-zone mismatch, DST/skipped dates and tzdb-frozen replay, 500/501 title limits |
| A5 | User/Person identity preserves attribution without inventing access | Actual foreign-Org users/People/resources and random IDs; name-only and unmatched roles, inactive historical authors, assignee deactivation in flight, explicit null mapping, no importer/default substitution |
| A6 | One atomic note/task insert; no overwrite, duplicates or resurrection | Replays, equal rows, local edits/snooze/reopen/completion, native tombstones and missing identity targets, account-qualified/bare-key collisions; failure injection around insert/identity/result/ledger commit |
| A7 | Confirm/retry/cancel/receipts are fenced and recoverable | Zero-eligible fresh confirmation rejects without consuming the child; verified already-present unit qualifies; zero inserts after cancellation/revalidation remain supported. Lost confirmation response with identical/different retry bytes, expired plan/report, executor role loss and explicit retry adoption, lease takeover, cancel-vs-worker and terminal replay; persisted partial results and original binding unchanged |
| A8 | Shared retained-byte ceilings and independent cancellation stay correct | Small-budget admission, actual settlement, lower deployment ceiling, explicit increase then retry, cancellation at exhaustion, concurrent metadata/activity reservations; native storage measured separately |
| A9 | Every read/write enforces Org/current authority and ordinary review hold | Direct-domain and DB permit negative/positive controls, forged Origin/token/targets, note/task edit/Today/Operator/outbound blocked, metadata/People guards unchanged |
| A10 | Large activity books remain fully reviewable through bounded pages | Many notes/tasks on one Person, native/source byte bounds, exact total traversal after completion, revision-change409, no note/task fetch_all in core read, held per-item access; old-reader-vs-exclusive-confirm race and after-cancel boundary |
| A11 | Real Web workflow and content/identity safety | Synthetic ordinary API plus production Web: mapping/HTML/date previews, held/source-only acknowledgement, confirm/lost response/reload, progress/cancel/budget resume, source disconnect, admin demotion/member hold, full note/provenance, desktop/390px/named-dialog checks; exact native audit |
| A12 | Contracts, compatibility, erasure/log boundaries and required checks | Fresh/populated additive migration, tenant FKs/index/grants, preflight/startup before/after activity binding and old-fleet rejection, no-content log sentinels with positive controls, final gates, query plans/paired reader check |

Follow D-050: 25,000 People, 50 members, at most five concurrent Today loads;
authorization/fidelity fail closed everywhere. Record realistic and concentrated
note/task densities, including ≥500 full-size notes on one Person to exercise
the known unpaginated-reader trigger. Measure one actual EXPLAIN per new/changed
hot SQL and bounded response/iteration behavior. One paired operational
Person-detail regression check is required because the shared complete-reader
guard changes; compare equal payloads under the same fixture/clock/build, p95
within max(25 ms,10%). Other unchanged readers reuse prior applicable evidence.
No new absolute latency gate, hardware capacity claim or speculative load matrix.
At most two bounded implementation review/fix rounds and final sequential scripts
per the brief. Planning review is distinct from executed implementation proof.

## 10. Approval handoff

D-067 settles plain-text conversion with preserved originals and confirmed-zone
date-only deadlines. D-068 accepts the complete specification and brief,
including: one completed-parent child/lifetime, both-family exhaustion,
counted independent subset execution; subject conversion and source-only
replies/reactions/settings; role/type/null mappings, timestamp holds and unchanged
native limits; account-qualified identity/no overwrite; the bounded review API,
legacy-reader409 barrier and activity-capable recovery requirement.

The user accepted these policies under D-068. Alternatives include richer native
notes/tasks, thread models, per-user date policies, completion-with-unknown-time
support and repair/delta imports; each requires its own concrete contract rather
than guessed behavior during coding. Implementation and isolated synthetic
verification are authorized. Live FUB validation stays user-deferred.

## D-070 amendment — independent historical capture

D-070's [010d1](SLICE_010d1.md) captures historical source evidence separately.
It does not alter this notes/tasks child's lifetime, original source/People
binding, native rows or paged review-reader contract. History has independently
owned reservations in the shared Org ledger. `fub-history-capture-v1` proves only
capture compatibility and does not replace `fub-activity-import-v1` or qualify
future timeline readers.
