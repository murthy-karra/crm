# Slice 010b — Core FUB snapshot and migration preview

**Approved consumer amendment — 010f2 (D-068, 2026-09-11):**
[010f2](SLICE_010f2.md) requalifies retained raw notes/detail and open/completed
task captures for native activity import onto completed 010c People. It preserves
this source schema and final snapshot boundary, distinguishing stream from family
and list from detail; clipped preview data is never execution input. Its child
reservations share the existing snapshot/Organization logical-byte ledgers and
operator ceilings with 010c/010f1. Cancellation releases only its own reservations.
Native note/task row/index storage is separately measured, not a retained quota.

**APPROVED FOR IMPLEMENTATION, 2026-09-11 (D-063).** The user accepted this
reviewed specification/brief, including storage allowances and delegated
increases, and requested implementation. The six review findings are resolved.
D-061 accepts core records first, with remaining data explicitly tracked.
010a is deployed to shared development; authorized live FUB validation remains
user-deferred for a few days. Neither this specification nor synthetic fixtures establish
live endpoint access or permission to read a customer's book.

Inputs: [decisions](../decisions/DECISION_LOG.md) D-012–016, D-050, D-059–063,
[architecture](../architecture/ARCHITECTURE_BASELINE.md),
[010a](SLICE_010a.md), [migration plan](../plans/SLICE_010_MIGRATION_SUMMARY.md),
[source qualification](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md), and
[execution brief](../tasks/SLICE_010b_IMPL.md).

## 1. Outcome and boundaries

An Organization admin uses the saved FUB connection to capture the core records,
resume an interrupted capture, and inspect an evidence-backed preview. They see
what was actually retrieved, possible mappings, overlapping contact identities,
unsupported values and remaining gaps before any CRM import is possible.

010a answers bounded access questions. 010b enumerates six core families:
People, users, stages, custom-field definitions, notes and tasks. Preserve every
returned field, including embedded tags/addresses/relationships and metadata,
even when the preview cannot interpret it. Embedded coverage never becomes a
claim that the corresponding standalone family was fully captured.

Only migration-control, encrypted source and preview records are written.
No Person, Inquiry, stage, member, field, note, task or contact attempt is created
or updated. No source writes, invitations, email/SMS, calls, AI extraction,
Operator tools, exports/downloads, media fetches, delta import or cutover.
Person creation dates do not become Inquiries; lastCommunication does not become
a fabricated contact attempt. Notes/tasks remain source data, not live CRM items.

No existing-Organization merge is designed here. A populated destination may
be assessed, but the preview explicitly says the first import requires a new,
empty Organization. It must not call the current destination import-ready.
Precise import emptiness, duplicate resolution and rerun rules belong to 010c.

## 2. Admin flow and meaning of completion

1. Manage → Migration retains the existing assessment and shows a separate
   **Core snapshot** section. Connection and access evidence remain visible.
2. **Prepare snapshot** creates a short-lived proposal with the fixed families,
   selected source scope, encrypted-storage explanation and current operational
   limits. The proposal performs no upstream request. The admin explicitly
   confirms before any bulk source capture starts.
3. Progress shows the active family, committed pages, unique IDs observed,
   captured bytes and observation window. No percentage uses an unverified
   total. Paused work displays a concrete cause and permitted retry/cancel action.
4. Each terminal family shows enumeration status, content/access gaps and its
   own query/time scope. A moving FUB book is not a transactional snapshot.
5. **Generate preview** derives a versioned report from committed records and
   a bounded read of current destination configuration. It does not write mappings
   or mutate business data. Partial captures can produce clearly partial previews.
6. The last completed capture/report remains inspectable during a new run.
   Reload/reconnect recovers durable state. There is no Import button in 010b.

Run states: `proposed`, `queued`, `running`, `waiting_retry`, `paused`,
`completed`, `completed_with_gaps`, `cancelled`, `expired`.
`proposed` expires after ten minutes. Confirming an expired proposal conflicts.
Successful confirmation freezes its profile, source account, credential revision
and original budget; replay cannot start a second run. Later budget increases
are separately authorized revisions (§4); the original proposal is retained.

`completed` means all selected enumeration/enrichment streams ended with valid
evidence and no unresolved content-retrieval gaps. It does not mean all FUB data,
unrestricted account access, absence of source variation or readiness to cut over.
The qualified item gaps in §4 can produce `completed_with_gaps` after all other
work ends. Unproven enumeration, unclassified access denial and resource stops
remain `paused`. Cancellation never marks unfinished enumeration complete.

## 3. Versioned source profile — `fub-core-v1`

The [source record](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md) distinguishes
published examples from live guarantees. Every request is an allowlisted GET
to the existing fixed HTTPS API origin. No redirect, arbitrary URL, nextLink,
attachment URL, note HTML link or source-provided hostname is executed.

| Family | Approved request profile | Coverage caveats |
|---|---|---|
| Identity | Existing `/identity` at confirmation execution and after a paused/restarted source session | Freeze account ID and source-user evidence; unknown role is never elevated by inference. |
| Users | `/users`, `includeDeleted=true`, paginate | Preserve inactive/deleted source users for attribution; they are not authenticated CRM accounts. Record actual field selection. |
| Stages | `/stages`, paginate | Preserve IDs, labels, order and flags; no stage auto-creation. |
| Custom fields | `/customFields`, paginate; collection key `customfields` | Preserve source `name`, kind, choices, recurring-date and display metadata, including unknown fields. |
| People | `/people`, `includeTrash=true`, `includeUnclaimed=true`, `fields=allFields`, paginate | Preserve complete returned bytes; explicitly qualify nested relationships/other field coverage. Do not sum 010a's overlapping Trash counts. |
| Notes | `/notes`, paginate; then `/notes/{id}` for each captured ID with `includeThreadedReplies=true&includeReactions=true` | Preserve list and detail evidence. Restricted detail 404 is not proof of deletion. Authorship names alone do not identify a CRM user. |
| Tasks | `/tasks` in explicit `isCompleted=false` and `isCompleted=true` streams, paginate each | Preserve both states, source dates/timezones, IDs and original task kinds. Deduplicate by source ID while retaining changed versions. |

The six families contain multiple streams; notes detail work and both task
partitions count toward their family's completion. Do not silently skip deleted
users, Trash, completed tasks, note replies/reactions or fields omitted by a
source response. Selected flags/field coverage must be confirmed with the public
profile and later authorized live tests. A rejected parameter pauses the stream;
do not silently remove it to obtain a successful response.

Using `allFields` is a deliberate snapshot choice to avoid reducing source
fidelity to the current CRM model. Bound each response and the run, preserve the
request profile, and report field-level unknowns. It is not a promise that FUB
returns every private field. External file bytes and standalone non-core
collections remain outside this rung even when their references appear inline.

### Enumeration and replay

- Begin at the fixed first-page request. Prefer `_metadata.next` when supported;
  reconstruct requests from the compiled family path and frozen query, encoding
  only the validated opaque token. Never follow `_metadata.nextLink` as a URL.
- For a family whose published profile only supports offset pagination, persist
  an offset mode explicitly. Do not assume common parameters are supported on
  every endpoint. A deep-offset refusal pauses with `pagination_unsupported`;
  no guessed ID range or synthetic continuation is allowed.
- Endpoint-specific parser/fixture contracts must define exhaustion. A missing
  token alone is insufficient if the response declares more records or omits
  required pagination evidence. Unknown/inconsistent totals remain unknown.
- Track repeated continuation fingerprints, duplicate pages and no-progress
  replies. Preserve evidence and pause on a loop rather than looping indefinitely.
- Stable identity is `(Organization, FUB account, family, source ID)`. Count
  each source ID once, independently of how many observations it has. Each
  observation also has a closed representation key: endpoint kind, selected
  fields/enrichment flags and profile version. In particular, notes list and
  detail-with-replies/reactions are different representations, not source drift.
  Task completion partitions are provenance for the same task representation;
  state/content changes between partitions remain visible.
- Preserve every raw capture exactly. Compare record content only within the
  same representation, using a purpose/tenant-bound HMAC over deterministic JSON
  values: stable object-key ordering, original array order, lossless numbers and
  all returned fields, including unknown fields. Whitespace/key ordering alone
  does not create a variant. Pin the encoding and large-number fixtures with the
  profile; duplicate JSON keys or unrepresentable values are classified rather
  than silently discarded. Raw byte hashes are integrity evidence, not the
  semantic-variation test. Do not compare page pagination metadata as record data.
- Retain distinct observed variants without overwriting evidence or silently
  choosing a winner. Invalid/missing IDs retain their capture/ordinal and issue;
  they do not count as identified source records.
- Every family reports its first/last observation and totals with their basis.
  Changing totals and partition overlap are observation warnings. Repeating an
  unchanged ID is not itself a content conflict. No source-write lock or atomic
  whole-account snapshot is claimed.
- `updatedAfter` cannot cover related records through People alone. Delta and
  deletion reconciliation stay separate in 010e; absence is not a deletion rule.

## 4. Persistence, trust and recovery

Extend the existing migration module, not intake `raw_payload`, the Operator,
or a new service. These are approved logical schema contracts; the implementation
lane owns one additive migration and must freeze its exact DDL before coding.

| Record | Required scope and invariants |
|---|---|
| `migration_snapshot` | UUID, Organization/connection composite FK, frozen account/credential revision, profile/schema versions, initiating actor, proposal expiry, state, raw/retained-byte counters, original/effective run budgets and budget revision, lease/due times and timestamps. Profile cannot change after confirmation. |
| `migration_snapshot_stream` | Composite run/Organization key plus family/partition; encrypted cursor, pagination mode, sequence, classified coverage, attempt cycle and retry deadline. Note-detail work uses durable IDs/ordinals. |
| `migration_snapshot_capture` | Append-only encrypted HTTP bytes, tenant/run/stream/request and representation identity, transactionally assigned monotonic run capture sequence, capture time, HTTP status, raw byte length, purpose-bound nonce/ciphertext/HMAC, safe version metadata and truncation/classification. Unique settled receipt per checkpoint distinguishes successful page/detail from qualified negative item result; failed attempts remain distinct. |
| `migration_snapshot_record` | Source ID (nullable only for classified invalid items), capture FK and JSON ordinal, closed representation key, semantic-variant HMAC and encrypted bounded projection. Uniqueness prevents duplicate replay indexes without discarding observations or distinct variants. |
| `migration_snapshot_contact_key` | Run/Organization/record FK, capture-sequence provenance, email-or-phone kind and purpose/tenant-bound HMAC of existing normalization; no plaintext normalized contact column. Indexed group membership, never all candidate pairs. Distinct source IDs, observations and variants remain distinguishable; not an identity decision. |
| `migration_snapshot_preview` | Report/input schema and comparison-engine versions, run/profile, immutable source-capture sequence boundary, encrypted frozen coverage and actual destination inputs with observation timestamp/fingerprint. Mutable execution state/cursor/lease/requesting actor, safe aggregate counts; immutable encrypted report/group-summary child pages keyed by report/Organization/page. Scoped FKs and keyset pagination. |
| `migration_snapshot_storage` | One scoped Organization ledger: effective retained-byte allowance, allowance revision, committed and reserved logical bytes. Run and preview reservations are durable, lease-bound and reclaimed only after fencing the old writer. Updates serialize with capture/preview commits and budget changes. |

Use `migration_request_receipt` with separate closed operation names for proposal,
confirm, retry, budget increase, preview and preview-retry requests. Authorization is checked before replay;
same key/input returns the same scoped receipt, changed input conflicts. Receipt
payloads contain safe identifiers/status only, never content, credentials or cursors.

All commands/queries recheck the caller's trusted active Organization admin
authority. Org-scoped queries and composite FKs reject foreign UUIDs;
platform-admin status alone is not a bypass. Apply these distinct rules:

| Operation | Additional authority and connection conditions |
|---|---|
| Propose/confirm/source retry; source worker pre-request and commit | Current connected source revision and original initiating admin remain valid. Confirmation/retry must be by that initiator; no implicit takeover of 010b source work. |
| Retained snapshot/report reads, pagination, cancellation and budget increase | Any current admin of the owning Organization. No live source connection, matching current credential revision or continued membership of the original initiator is required. Cancellation fences pending writes; it does not erase evidence. |
| DB-only preview generation/retry | Attributed to the requesting current admin, checked at claim/commit. No source connection or original capture-initiator requirement. A retry can assign preview execution to a new current admin only after fencing the previous preview lease. |

Credential rotation/disconnect never turns retained-read authorization into
upstream access. A revoked caller still loses read access immediately, and a
preview worker whose own requesting admin is revoked cannot commit new pages.

Proposal/confirm/start commands take the existing Organization lock. Permit only
one active source job per Organization across 010a assessments and 010b snapshots;
update both paths so the rule is symmetric. Expired proposals do not block work.
Share 010a's in-process source permit/cooldown across validation, assessment and
snapshot requests. Multiple replicas and distributed pacing remain outside the
current single-process deployment.

Claim one bounded request with a renewable/fenced lease; acquire pacing permission
before the DB claim. No database connection or transaction is held during source
I/O. In one short commit transaction, recheck admin/connection/lease, save encrypted
bytes and accepted indexes, save the next checkpoint, and release the lease.
A commit failure advances nothing. Crash recovery replays the same checkpoint;
idempotent receipts prevent a second accepted page and duplicate index rows.

Malformed/oversized responses save a classified encrypted bounded capture where
possible and pause without advancing. A truncated prefix is never labelled a
preserved complete record. A failed raw capture cannot yield a successful page
or preview item. Source content stays untrusted and is never sent to an LLM.

The frozen profile owns a closed classification matrix; arbitrary status codes
must not be converted into completeness claims:

| Evidence / failure | Settlement and recovery |
|---|---|
| Qualified note-detail 404 under the documented restriction behavior | Preserve bounded error evidence and an item-gap receipt; settle that detail work item exactly once and continue. Keep list evidence; no deletion inference or fabricated detail. It is not a successfully retrieved record/page. |
| Successful response with explicitly identified opaque/missing content | Preserve returned bytes and record the content gap. Valid enumeration may continue; the unavailable content is never counted as captured. |
| Collection 403/404 or any unqualified item denial | Pause with unclassified access/endpoint failure; do not declare exhaustion. No collection-level denial is allowlisted as terminal coverage in this profile. New qualification needs a reviewed profile, not an improvised fallback. |
| 401, rejected account/credential, changed/disconnected connection or revoked source initiator | Pause/fence source work, without automatic retry. Repair may require a new snapshot under a new credential revision; retained evidence stays readable to current admins. |
| Rejected query parameter, malformed response or uncertain pagination | Preserve classified evidence and pause at the unadvanced checkpoint. Do not drop flags or guess continuation. |
| 429, transport failure or 5xx | Existing header-aware, durable three-attempt cycle, then pause. Honor Retry-After before another request. |

Only a qualified negative item receipt may advance detail work without a
successful content capture. It advances no collection cursor and does not
increase retrieved-record/page counts. Its write, evidence and next detail
checkpoint settle atomically; replay neither double-counts the gap nor repeats
settled work. If that evidence cannot be committed, pause without advancing.
An already-settled gap is not silently re-fetched in the same frozen preview;
later source recovery is captured in a new run. Existing captures survive retries.
Cancel and replacement/disconnect fence in-flight results. A new credential
revision requires a new snapshot rather than mixing access scopes in an old one;
the old partial snapshot remains inspectable. No automatic admin takeover:
revoked initiator authority pauses the run; another admin can cancel/start anew.

Use existing development key material with new, non-interchangeable snapshot
and preview AEAD/HMAC purposes bound to Organization, run/capture/record and
version. Store raw content, display text and cursors encrypted. Plaintext metadata
is limited to safe IDs, versions, states, time windows and counts. Never put PII,
credentials, normalized contacts, raw responses or source links in logs/history.
Separate crypto/erasure production policy remains O-012/O-013; no real-customer
data before those prerequisites. No automatic retention deletion is introduced.

### Approved development operating bounds

Initial page size 100; 10-second request timeout; 4 MiB decoded response cap;
one source request in flight shared with 010a; preview pages at most 50 records.
The approved initial synthetic-development allowances are 2 GiB per run and
4 GiB per Organization across retained 010b runs/reports. These are adjustable
logical-storage guards, not customer quotas, retention policy or physical-disk
estimates. Values and delegation below were accepted under D-063.
The earlier 25,000-People/500,000-note-task hard stops are removed from this
proposal: D-050 still defines the supported performance envelope, not a source
record truncation rule or evidence of larger-book capacity.

Keep three measures separate:

- Source bytes: decoded HTTP bytes observed/committed, for capture statistics.
- Retained logical bytes: variable stored payloads for all 010b captures
  (including failures), encrypted projections/cursors/frozen preview inputs,
  report/group-summary pages and their
  crypto overhead. Count partial, cancelled and historical runs/reports. Freeze
  the counted-column inventory in DDL; update actual byte deltas transactionally.
- Physical PostgreSQL use: rows/indexes/TOAST, WAL, free space and replicas are
  not measured by the logical allowance. Production capacity monitoring and
  admission headroom remain a separate readiness requirement.

Before source I/O or a preview batch, atomically reserve its worst-case retained
bytes against both run and Organization availability. Bound raw and derived
output so its combined maximum is known, including enough room for a classified
failure. A writer may commit only within its reservation; record actual bytes,
release unused reservation and advance its checkpoint in the same transaction.
Other runs/previews count active reservations. Fence an expired lease before
reclaiming space, and never hold a DB connection while awaiting source I/O.

If admission fails, pause visibly with unchanged checkpoint and required
additional allowance; no bytes are dropped or counted as complete. Existing
capture summaries and completed report pages stay readable when no new preview
fits; source-content inspection requires generated preview pages, since there
is no raw-body endpoint. Failed/cancelled work
continues to count; there is no deletion endpoint or automatic cleanup to make
room in 010b. A preview can pause and resume its existing report revision rather
than repeatedly creating additional partial reports.

**Approved recovery mechanism:** deployment configuration publishes maximum
per-run/per-Organization allowances and a policy revision. Defaults do not exceed
the initial allowances until intentionally changed. An authorized deployment
operator may change those ceilings through the recorded configuration/release
process; that role gains no tenant-content access. A current Organization admin
can then explicitly confirm a typed budget increase within those ceilings (§6).
The command locks the Organization ledger, checks expected run/Organization budget
revisions and the current policy, only increases allowances, and stores an
immutable receipt with actor, old/new limits and revisions. Original proposal
limits remain unchanged. An over-ceiling request is rejected with safe limits.

Increasing a budget never starts a source request or preview. Explicit source
retry still checks original initiator/account/credential/profile; preview retry
uses the separate DB-only authority above. A lower deployment ceiling blocks new
reservations above it without deleting data; no ordinary admin can override it.
The effective bound for new admission is the lesser of approved allowance and
deployment ceiling. An already-granted reservation remains valid for its bounded
commit after a ceiling reduction, unless authority or its lease was fenced;
lowering configuration must not cause that in-flight response to be discarded.

## 5. Preview semantics

Generate a deterministic versioned report only from completed, completed-with-gaps,
paused or cancelled runs with at least one accepted capture; other states return
409. Pin the committed capture/coverage revision, including settled item gaps,
so a later retry cannot change report input. Generation states are `queued`,
`running`, `paused`, `completed`, `failed`; budget stops preserve its checkpoint.
Use restartable bounded batches and immutable encrypted report pages; publish
the completed report pointer only after all pages settle. A preview retry
resumes the same frozen input/report, not a fresh destination observation.

Before queuing generation, reserve space and atomically persist the report's
inputs from one short consistent database observation: actual destination stages,
active-member mapping inputs, custom-field definitions/options and Person-presence
result used by the preview; frozen source coverage/gaps; and the highest committed
capture sequence included. All customer/configuration content is encrypted with
preview-specific purposes. Source records/contact memberships are append-only
and constrained to that boundary, so a later source retry cannot change the set.
Fingerprint/timestamp alone cannot reconstruct these inputs. Bound their rows
and serialized size; insufficient allowance conflicts without creating a partial
input set. Generation starts only after input persistence succeeds.

All batches read those saved inputs, never fresh destination configuration or
mutable stream coverage. Pin input/comparison-engine versions; a worker unable
to interpret them pauses rather than mixing algorithms. Input and group-summary
storage count toward reservations. Runtime job state may change while the input
set and completed output pages remain immutable.
Record the current destination observation and fingerprint; mark it stale when
relevant stages/members/fields/Person presence changes. Refresh creates a new
report revision, retaining the previous report. Do not silently relabel one as
a plan against a newer destination. A future import must independently revalidate.

- **Users and stages:** propose unique matches using source IDs/provenance when
  present, otherwise exact normalized member email or exact trimmed stage label.
  Match only active destination members; ambiguous/missing/inactive matches need
  review. Display-name-only authorship stays unresolved. No invitations/role changes.
- **People:** list overlaps of existing `contact::normalize_email` and
  `normalize_phone` keys within the source. Those helpers are normalization, not
  strong validity checks or E.164 proof. Represent each shared normalized key as
  an overlap group with an opaque report-scoped ID and distinct-source-ID count.
  Group membership references the frozen contact-key index. Compute group counts
  once per report, not by scanning all members for every source record. A Person
  with several keys may belong to several groups; none is a merged identity.
  Record pages carry only bounded group metadata/counts; paginate a record's
  group list and each group's members independently (§6). Show every candidate
  through pagination, never a quadratic set of all Person pairs or an unbounded
  nested array. Do not call intake `identify`, choose a winner by name, or merge
  a shared-household/office-phone group automatically.
- **Custom fields:** propose source-key/type-compatible destinations; flag missing
  options, recurring dates, label/value/precision/date-window limits and unknown
  kinds. Call existing validators read-only. Never truncate or coerce values.
- **Notes:** preserve HTML, subject, replies, reactions, author fields and timestamps
  in encrypted captures. Preview whether the existing plain-text destination can
  represent the content; formatting conversion and reply flattening are decisions,
  not silent transformations. List and detail are complementary representations
  of one source note. Prefer a successfully captured, unambiguous detail projection
  for review, retain list provenance, and show the list projection with an explicit
  detail gap when detail is unavailable. Never fill missing detail fields from
  the list as if they had been returned by detail. Expose contradictions in
  overlapping fields only where the profile qualifies them as comparable, and
  expose comparable variants without silently selecting a winning version.
- **Tasks:** retain original type, completion and due fields. Calendar-date-only
  deadlines or unknown timezone, unmatched assignee and unsupported kinds need
  review. No invented due instant, default assignee or overdue Today flood.
- **Tags/addresses/relationships and other inline data:** retain and describe
  observed embedded coverage. Do not infer complete standalone records from it.

Each source ID has one primary preview disposition: `needs_decision`,
`unsupported_value`, `unresolved_reference`, `reviewable`, with deterministic
precedence in that order and separate nonexclusive issue counts. Distinct content
variants within a comparable representation, or contradictory profile-qualified
comparable fields between representations, require a decision; expected enrichment and
format-only changes do not. Invalid-ID items are counted separately
by capture/ordinal. Inaccessible content and not-captured families are separate
coverage measures, not fabricated records. No `imported`, `unchanged`, percent
importable or cutover-ready total is produced before import rules exist.

Report dimensions stay separate: enumerated distinct IDs; returned page items;
accepted captures; content-retrieval gaps; representation issues; remaining work.
All potentially large counts/source IDs use validated decimal strings on new
HTTP DTOs. Unknown totals are null. Totals never sum heterogeneous entities or
overlapping source scopes into an invented migration total.

The report always retains rows for history/inquiries, calls, texts, emails,
recordings, standalone tags, addresses, relationships, appointments, deals,
automation/settings, and external attachments/files. Mark each `not_captured`
with its next snapshot work, or `embedded_only` where evidence justifies that.
The future plan must explicitly schedule these sources before a cutover-fidelity
decision; D-061 did not remove them from the migration.

D-062 places future email bulk content outside PostgreSQL, with metadata and
storage references in PostgreSQL. Email capture and selection/integration of
that storage remain later work; this does not change this slice's approved
storage for the six core families.

## 6. Approved commands and HTTP contracts

**Current:** 010a exposes connections, bounded assessments and the four-field
`FubSummary`; its reader has only `identity` and fixed `probe` calls. No snapshot,
cursor, record-review or import API exists.

**Approved addition:** snapshot commands/queries and one schema migration. The
new typed pagination interface accepts closed family/request types, not URLs.
Keep 010a's six checks, response envelopes and one-MiB probe bound unchanged.
Extend replacement/disconnect fencing and symmetric active-job exclusion as
declared in §4. Existing 010a conflicts remain in its existing error envelope.
Preserve 010a's existing retry behavior, which adopts the retrying admin; 010b's
no-source-takeover policy does not silently change it.

Every path below begins `/api/migrations/fub`. All require active Org admin;
unauthenticated 401, member 403, foreign/missing scoped resource 404, malformed
body/UUID/cursor 400, expired/stale/conflicting state 409, unavailable service 503.
Use existing closed error shapes and `Cache-Control: no-store`. Unknown fields
are rejected. No response contains credentials, encrypted bytes or upstream cursors.

| Route / typed action | Request and response |
|---|---|
| `POST /snapshots` — `ProposeCoreSnapshot` | `{request_id,connection_id,expected_revision}` → 201 `{snapshot,proposal}`. Server chooses fixed profile, families, limits and ten-minute expiry; no source read. |
| `POST /snapshots/{id}/confirm` — `ConfirmCoreSnapshot` | `{request_id}` → 202 `{snapshot}`. Same actor/Organization, unexpired frozen proposal/current connection required; queued after explicit confirmation. Replay returns original scoped receipt. |
| `GET /snapshots` | Stable keyset page of at most 20 summaries, plus separately identified active/latest completed IDs; no change to 010a `FubSummary`. |
| `GET /snapshots/{id}` | `{snapshot,streams,coverage}` with timestamps, source/retained/reserved byte counts, original/effective budgets, run/Organization budget revisions, current policy ceilings/revision, pause reason and supported actions. |
| `POST /snapshots/{id}/retry` — `RetryCoreSnapshot` | `{request_id}` → 202 `{snapshot}`. Same frozen source revision/profile, original initiator still authorized, next attempt cycle only from paused. |
| `POST /snapshots/{id}/budget` — `IncreaseCoreSnapshotBudget` | `{request_id,expected_run_budget_revision,expected_org_budget_revision,expected_policy_revision,run_byte_limit,org_byte_limit}` → 200 `{snapshot,budget}`. Confirmed runs, including retained terminal runs; current admin, explicit review/confirmation, monotonic increase within server ceilings, no job start. Stale/over-ceiling values conflict; receipt retains old/new limits. Byte values are validated decimal strings. |
| `POST /snapshots/{id}/cancel` — `CancelCoreSnapshot` | Empty body → 200 `{snapshot}`; repeated cancel is idempotent. May cancel proposed/active/paused work without deleting evidence. |
| `POST /snapshots/{id}/previews` — `GenerateCorePreview` | `{request_id}` → 202 `{preview_id,state}`; freezes capture set and destination observation; DB-only bounded background work. |
| `POST /snapshots/{id}/previews/{preview_id}/retry` — `RetryCorePreview` | `{request_id}` → 202 `{preview_id,state}`; paused only, same frozen inputs/checkpoint, current requesting admin takes a newly fenced preview lease. Requires sufficient approved capacity; no FUB access. |
| `GET /snapshots/{id}/previews/{preview_id}` | `{preview,coverage,counts,destination_stale}`; current authorization checked again. |
| `GET /snapshots/{id}/previews/{preview_id}/records` | `family`, optional disposition, server-issued opaque local cursor, `limit<=50` → `{records,next_cursor}`; escaped, bounded decrypted projections only. |
| `GET /snapshots/{id}/previews/{preview_id}/overlap-groups` | Optional report-local `record_id`, opaque scoped cursor, `limit<=50` → `{groups,next_cursor}`. Each group has an opaque ID, contact kind and distinct-source-ID count; no member array or exposed HMAC. |
| `GET /snapshots/{id}/previews/{preview_id}/overlap-groups/{group_id}/members` | Opaque group-scoped cursor, `limit<=50` → `{members,next_cursor}`. Distinct source IDs and report-local record references from the frozen capture boundary; no all-pairs expansion. |

Run summaries include IDs, profile/version, state, connection revision, observation
times, completion reason, proposal expiry, limits and safe counters. Local cursor
tokens are scoped to Organization/run/report/filter and cannot redirect requests.
Group membership cursors also bind group ID; record/group identifiers are scoped
references, not authorization. Group/member pages and record group counts remain
consistent with the report's immutable source boundary. Bound strings and total
serialized response bytes as well as row counts; never embed all groups/members
inside a record. Query plans must use the contact-key/group lookup indexes.
Do not expose an unrestricted raw-body endpoint or arbitrary SQL/filter language.

Affected components: `crm-app` migration domain, `crm-api` config/state/worker/router,
SQLx/schema, Web Migration view/API/query keys/tests. No native or Operator contract
changes. Old clients continue to read assessments; new Web/API must ship together
for snapshots. Rollback leaves additive captured data intact; never drop tables
or repurpose a retained snapshot to make an older executable start.

Required amendments: 010a §§3–6 pointers for the new pagination seam,
cross-job serialization and replacement/disconnect behavior; migration summary,
ladder, implementation brief and project state. D-061 accepts family scope;
D-063 accepts these contracts and implementation.

## 7. Web and observability

Use the existing admin route, components, scoped TanStack keys and session lifetime
fences. Clear decrypted pages and late responses on Organization/session change.
No note HTML rendering, automatic link previews, browser credential storage,
query-cache raw captures or AI-generated migration summaries. Source text is
escaped; any display clipping is labelled and never changes captured data.

Confirmation states what will be read/stored. Budget review shows old/new run
and shared Organization allowances, actual retained/reserved usage, deployment
ceilings and that increasing space does not resume work. The UI confirms exact
values/revisions before the typed request; stale state requires a refreshed
review. If the ceiling is insufficient, explain that an operator must change
configuration; do not offer evidence deletion or an unbounded bypass.
Show separate resume actions and authority for source capture and DB-only
preview. Retained reads remain available after credential rotation/disconnect.
Poll two seconds while active,
back off on failures and stop on terminal/paused states. Keyboard-focusable
retry/cancel/refresh, restrained progress announcements, clear unknown values,
pagination and narrow layouts are required. A zero total is not a loading state.

Instrument proposal/confirm/claim/capture/budget/preview/preview-retry/cancel with safe actor,
Organization, snapshot, stream, request/correlation IDs and closed outcome codes.
Record request latency, source and retained/reserved bytes separately, accepted
pages, settled content gaps, retries, lease loss, budget stops/changes and preview
timing. Keep content/URLs/cursors/keys/source error bodies out of spans.
No realtime contract: recover from PostgreSQL on focus/reconnect.

## 8. Acceptance and verification

1. Admin preparation performs no source read; only explicit confirmation starts
   the frozen proposal. Expired/stale/foreign proposals and replay-input changes
   fail correctly. Prove concurrent confirm produces one run.
2. Deny member/platform-only/cross-Organization reads and mutations, forged local
   cursors, preview IDs and direct-domain calls; revoke actor authority during
   source I/O and before preview access. Composite FKs reject scope rebinding.
   A different current admin can inspect old captures and generate/retry a DB-only
   preview after source-initiator revocation or credential rotation/disconnect,
   but cannot take over 010b upstream work. Revoke the preview's requesting admin
   before claim/commit and prove old leases cannot write after reassignment.
3. Synthetic source records prove each core family/partition/notes detail path,
   large IDs, missing/unknown fields, Trash, deleted users and both task states.
   All returned raw bytes survive encrypted without a business-table write.
   Note list/detail enrichment and format-only differences produce one source
   note without false conflict; genuine comparable changes and conflicting
   overlapping fields retain variants and require a decision. Pin lossless large
   numbers, unknown fields, array order and duplicate-key rejection behavior.
4. Qualify next/offset exhaustion per endpoint. Exercise repeated tokens, hostile
   nextLink, changing totals, repeated/changed IDs, missing IDs, false empty pages,
   revoked access, denied note detail, malformed JSON and oversized responses.
   None may invent completeness, source absence or preserved unavailable content.
   A qualified note-detail 404 advances only its detail checkpoint once with
   a gap receipt; unqualified item/collection denials pause. Failed gap-evidence
   commits do not advance, and negative receipts cannot increase success counts.
5. Crash after response/before commit, failed evidence commit, lease reclaim,
   duplicate delivery and bounded retry preserve atomic checkpoints and exact
   source-version evidence. No DB connection is held across upstream latency.
6. Cross-job tests prove assessment and snapshot exclusion, shared source pacing,
   cancellation and credential replacement/disconnect fencing in both directions.
   Revoked initiator cannot be bypassed by retry from another member.
   Preserve the different existing 010a retry/initiator contract explicitly.
7. Prove wrong key/purpose/tenant/run/capture rejection and captured-log redaction.
   Malformed content is encrypted evidence, never raw SQL/Operator/log input.
8. Preview tests prove deterministic precedence/counts, no names-only Person merge,
   normalization-only overlap labels, ambiguous author/assignee/stage mappings,
   HTML/recurring-date/numeric/date-only cases and destination-stale reporting.
   Include frozen gap coverage, readable list provenance when detail is denied,
   pause/resume of the same report, and no automatic work after a budget increase.
   Crash/pause after one page, modify destination stages/members/fields and resume
   source capture: the existing report uses the original saved inputs and becomes
   stale; a fresh report observes new inputs. A changed comparison-engine version
   cannot resume old pages with different semantics.
   One shared office phone across a within-envelope book yields one navigable
   group with every distinct source ID, bounded group/member pages and no
   quadratic pair storage or repeated full-group scan per record. Include
   duplicate contact observations and foreign/stale group cursors.
9. Real-browser synthetic walkthrough: proposal/confirm, progress, reload, pause/
   retry/cancel, old report, per-record review, member denial, Organization switch
   and 390px layout. Review/confirm a budget increase, reject stale/over-ceiling
   changes, and resume capture versus preview with their distinct authority.
   Account/schema-qualified live testing is separately pending.
10. Query plans over the D-050 envelope show indexed claim/latest/page/record/contact
    lookups and bounded preview reads. Benchmark only if a changed hot path triggers
    D-050; no repeated 019b benchmark or above-envelope concurrency exercise.
11. Run targeted unit/Web/DB tests, `./scripts/sqlx-prepare`, `./scripts/check`,
    `./scripts/check-db` once on the frozen implementation; never overlap DB gates.
    Map all acceptance items and failures to the verification record. Implementation
    subsequently passed the required checks; see [verification](../tasks/SLICE_010b_VERIFICATION.md)
    for the exact tested tree, results and deferred external source qualification.
12. Account raw/derived/failure/report bytes separately from physical storage.
    Concurrent capture/preview reservations cannot exceed run/Organization limits;
    crash/reclaim fences the former writer. Verify partial/cancelled retention,
    unused reservation release, immutable original budgets and revisioned increases.
    Repeated receipt replay cannot change capacity twice; lower server ceilings
    stop new admission safely. Use small synthetic limits, not multi-GiB fixtures.

## 9. Accepted scope and remaining qualification

Accepted: core-first sequence, explicit remaining-family coverage (D-061),
new-Organization-first eventual import (D-059), live-validation deferral, and
this reviewed specification/brief with its allowance/delegation policy (D-063).

- **Scope/profile qualification:** verify exact field/flag behavior and pagination
  on each core collection and note detail. Public metadata is available; live
  behavior is not yet tested. Unexpected source restrictions stay visible and
  must not be replaced with scraping or an undocumented endpoint.
- **Storage/recovery policy:** D-063 accepts §4's initial synthetic-development
  allowances and delegation: current admins may increase allowances
  only within deployment-operator ceilings, followed by a separate resume action.
  No customer quota, automatic deletion or retention period is selected. The
  accounting/recovery mechanism, values and authority are approved for this
  slice. Retention/erasure remains its own D-015/O-012/O-013 gate.
- **Preview-only decisions:** proposed matches and normalization overlaps do not
  approve import matching, author substitution, date conversions, formatting loss,
  source deletion handling, rollback or Today behavior. Resolve those with 010c+
  specifications using this report; do not implement an implicit import policy.
- **Review gate:** four coordinator findings and two independent findings have
  proposed corrections; the targeted independent confirmation returned **READY**.
  See the [review record](../tasks/SLICE_010b_REVIEW.md). The subsequent user
  approval is recorded in D-063; implementation may proceed. Follow the brief's
  backend checkpoint and final verification; deployment remains separate.

Planning does not await the user's live test to remain useful. Implementation
must be fixture-driven until source validation is resumed explicitly; no fixture
result closes the separate live-validation criterion or customer-data prerequisites.
