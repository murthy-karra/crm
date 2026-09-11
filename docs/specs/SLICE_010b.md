# Slice 010b — Core FUB snapshot and migration preview

**DRAFT, 2026-09-11. Planning only; implementation is not approved.**
D-061 accepts core records first, with remaining data explicitly tracked.
010a is deployed to shared development; authorized live FUB validation remains
user-deferred for a few days. Neither this draft nor synthetic fixtures establish
live endpoint access or permission to read a customer's book.

Inputs: [decisions](../decisions/DECISION_LOG.md) D-012–016, D-050, D-059–061,
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
and budget; replay cannot start a second run.

`completed` means all selected enumeration/enrichment streams ended with valid
evidence. It does not mean all FUB data, unrestricted account access, absence
of source drift or readiness to cut over. Known denied/opaque content produces
`completed_with_gaps`; unproven pagination exhaustion or a resource budget stop
stays `paused`. Cancellation never reports a completed family that was unfinished.

## 3. Versioned source profile — proposed `fub-core-v1`

The [source record](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md) distinguishes
published examples from live guarantees. Every request is an allowlisted GET
to the existing fixed HTTPS API origin. No redirect, arbitrary URL, nextLink,
attachment URL, note HTML link or source-provided hostname is executed.

| Family | Proposed request profile | Coverage caveats |
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

Using `allFields` is a deliberate snapshot proposal to avoid reducing source
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
- Stable identity is `(Organization, FUB account, family, source ID)`. Across
  pages retain all observed content versions and count each source ID once.
  Equal IDs with changed bytes are source drift, not permission to overwrite
  historical evidence. Invalid/missing IDs retain their raw page/ordinal and
  an issue; they do not count as identified source records.
- Every family reports its first/last observation and totals with their basis.
  Changing totals, duplicate IDs and overlap across task partitions are explicit
  drift signals. No source-write lock or atomic whole-account snapshot is claimed.
- `updatedAfter` cannot cover related records through People alone. Delta and
  deletion reconciliation stay separate in 010e; absence is not a deletion rule.

## 4. Persistence, trust and recovery

Extend the existing migration module, not intake `raw_payload`, the Operator,
or a new service. These are proposed logical schema contracts; the implementation
lane owns one additive migration and must freeze its exact DDL before coding.

| Record | Required scope and invariants |
|---|---|
| `migration_snapshot` | UUID, Organization/connection composite FK, frozen account/credential revision, profile/schema versions, initiating actor, proposal expiry, state, counters/budgets, lease/due times and timestamps. Profile cannot change after confirmation. |
| `migration_snapshot_stream` | Composite run/Organization key plus family/partition; encrypted cursor, pagination mode, sequence, classified coverage, attempt cycle and retry deadline. Note-detail work uses durable IDs/ordinals. |
| `migration_snapshot_capture` | Append-only encrypted HTTP bytes, tenant/run/stream/request identity, capture time, HTTP status, byte length, purpose-bound nonce/ciphertext/HMAC, safe version metadata, truncation/classification and accepted-page receipt. Unique successful receipt per checkpoint; failed attempts remain distinct. |
| `migration_snapshot_record` | Source ID (nullable only for classified invalid items), capture FK and JSON ordinal, observed version HMAC, encrypted bounded projection. Uniqueness prevents duplicate replay indexes without discarding changed source versions. |
| `migration_snapshot_contact_key` | Run/Organization/record FK, email-or-phone kind and purpose/tenant-bound HMAC of the existing normalization result; no plaintext normalized contact column. Indexed for overlap queries; not an identity decision. |
| `migration_snapshot_preview` | Immutable report revision, run/profile, destination-observation timestamp and configuration fingerprint, state, resumable generation cursor/lease and safe aggregate counts. Separate encrypted child pages keyed by report/Organization/page; scope-matching FKs and indexed stable pagination. |

Use `migration_request_receipt` with separate closed operation names for proposal,
confirm, retry and preview requests. Authorization is checked before replay;
same key/input returns the same scoped receipt, changed input conflicts. Receipt
payloads contain safe identifiers/status only, never content, credentials or cursors.

Application commands recheck trusted active Organization admin authority. Worker
pre-request and commit paths recheck the original initiating admin and current
connection status/revision. Org-scoped queries and composite FKs enforce tenant
boundaries even if the caller supplies a foreign UUID. Platform-admin status alone
is not a bypass. Reads, preview generation and pagination apply the same rules.

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

429/transport/5xx use 010a's header-aware, durable bounded retry cycle (three
attempts, then pause). Credential/permission rejection requires repair rather
than repeated automatic requests. Existing successful captures survive retries.
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

### Proposed operating bounds — review required

Initial page size 100; 10-second request timeout; 4 MiB decoded response cap;
one source request in flight shared with 010a; preview pages at most 50 records.
Proposed safety stops: 25,000 People, 500,000 combined note/task records and
2 GiB captured bytes per run. Bound aggregate Organization storage as well;
the proposed initial cap is 4 GiB across retained runs. These are local limits,
not vendor guarantees or accepted customer-retention policy. Freeze the limits
in the proposal and enforce them before admission/commit with safe overshoot
handling for a single bounded response. Budget exhaustion pauses visibly and
preserves committed evidence; it never excludes records or reports completion.
Exact caps and how operators raise them are open review items (§9).

## 5. Preview semantics

Generate a deterministic versioned report only from completed, completed-with-gaps,
paused or cancelled runs with at least one accepted capture; other states return
409. Pin the accepted capture-set revision so a later retry cannot change the
report input. Generation uses restartable bounded batches and immutable encrypted
report pages; publish the completed report pointer only after all pages settle.
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
  strong validity checks or E.164 proof. Show every candidate, shared-household
  and multi-record overlap; do not call intake `identify`, which chooses a single
  winner, and do not merge by names or collapse a connected group automatically.
- **Custom fields:** propose source-key/type-compatible destinations; flag missing
  options, recurring dates, label/value/precision/date-window limits and unknown
  kinds. Call existing validators read-only. Never truncate or coerce values.
- **Notes:** preserve HTML, subject, replies, reactions, author fields and timestamps
  in encrypted captures. Preview whether the existing plain-text destination can
  represent the content; formatting conversion and reply flattening are decisions,
  not silent transformations. Missing/inaccessible body stays a coverage gap.
- **Tasks:** retain original type, completion and due fields. Calendar-date-only
  deadlines or unknown timezone, unmatched assignee and unsupported kinds need
  review. No invented due instant, default assignee or overdue Today flood.
- **Tags/addresses/relationships and other inline data:** retain and describe
  observed embedded coverage. Do not infer complete standalone records from it.

Each source ID has one primary preview disposition: `needs_decision`,
`unsupported_value`, `unresolved_reference`, `reviewable`, with deterministic
precedence in that order and separate nonexclusive issue counts. Duplicate IDs
with changed versions require a decision. Invalid-ID items are counted separately
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
that storage remain later work; this does not change this slice's proposed
storage for the six core families.

## 6. Proposed commands and HTTP contracts

**Current:** 010a exposes connections, bounded assessments and the four-field
`FubSummary`; its reader has only `identity` and fixed `probe` calls. No snapshot,
cursor, record-review or import API exists.

**Proposed:** additive snapshot commands/queries and one schema migration. The
new typed pagination interface accepts closed family/request types, not URLs.
Keep 010a's six checks, response envelopes and one-MiB probe bound unchanged.
Extend replacement/disconnect fencing and symmetric active-job exclusion as
declared in §4. Existing 010a conflicts remain in its existing error envelope.

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
| `GET /snapshots/{id}` | `{snapshot,streams,coverage}` with timestamps, safe counts, bounds, pause reason and supported actions. |
| `POST /snapshots/{id}/retry` — `RetryCoreSnapshot` | `{request_id}` → 202 `{snapshot}`. Same frozen source revision/profile, original initiator still authorized, next attempt cycle only from paused. |
| `POST /snapshots/{id}/cancel` — `CancelCoreSnapshot` | Empty body → 200 `{snapshot}`; repeated cancel is idempotent. May cancel proposed/active/paused work without deleting evidence. |
| `POST /snapshots/{id}/previews` — `GenerateCorePreview` | `{request_id}` → 202 `{preview_id,state}`; freezes capture set and destination observation; DB-only bounded background work. |
| `GET /snapshots/{id}/previews/{preview_id}` | `{preview,coverage,counts,destination_stale}`; current authorization checked again. |
| `GET /snapshots/{id}/previews/{preview_id}/records` | `family`, optional disposition, server-issued opaque local cursor, `limit<=50` → `{records,next_cursor}`; escaped, bounded decrypted projections only. |

Run summaries include IDs, profile/version, state, connection revision, observation
times, completion reason, proposal expiry, limits and safe counters. Local cursor
tokens are scoped to Organization/run/report/filter and cannot redirect requests.
Do not expose an unrestricted raw-body endpoint or arbitrary SQL/filter language.

Affected components: `crm-app` migration domain, `crm-api` config/state/worker/router,
SQLx/schema, Web Migration view/API/query keys/tests. No native or Operator contract
changes. Old clients continue to read assessments; new Web/API must ship together
for snapshots. Rollback leaves additive captured data intact; never drop tables
or repurpose a retained snapshot to make an older executable start.

Required amendments on approval: 010a §§3–6 pointers for the new pagination seam,
cross-job serialization and replacement/disconnect behavior; migration summary,
ladder, implementation brief and project state. D-061 accepts family scope only;
this draft owns proposed contracts, not permission to implement them.

## 7. Web and observability

Use the existing admin route, components, scoped TanStack keys and session lifetime
fences. Clear decrypted pages and late responses on Organization/session change.
No note HTML rendering, automatic link previews, browser credential storage,
query-cache raw captures or AI-generated migration summaries. Source text is
escaped; any display clipping is labelled and never changes captured data.

Confirmation states what will be read/stored. Poll two seconds while active,
back off on failures and stop on terminal/paused states. Keyboard-focusable
retry/cancel/refresh, restrained progress announcements, clear unknown values,
pagination and narrow layouts are required. A zero total is not a loading state.

Instrument proposal/confirm/claim/capture/preview/cancel with safe actor,
Organization, snapshot, stream, request/correlation IDs and closed outcome codes.
Record request latency, bytes, accepted pages, retries, lease loss, budget stops
and preview timing. Keep content/URLs/cursors/keys/source error bodies out of spans.
No realtime contract: recover from PostgreSQL on focus/reconnect.

## 8. Acceptance and verification

1. Admin preparation performs no source read; only explicit confirmation starts
   the frozen proposal. Expired/stale/foreign proposals and replay-input changes
   fail correctly. Prove concurrent confirm produces one run.
2. Deny member/platform-only/cross-Organization reads and mutations, forged local
   cursors, preview IDs and direct-domain calls; revoke actor authority during
   source I/O and before preview access. Composite FKs reject scope rebinding.
3. Synthetic source records prove each core family/partition/notes detail path,
   large IDs, missing/unknown fields, Trash, deleted users and both task states.
   All returned raw bytes survive encrypted without a business-table write.
4. Qualify next/offset exhaustion per endpoint. Exercise repeated tokens, hostile
   nextLink, changing totals, repeated/changed IDs, missing IDs, false empty pages,
   revoked access, denied note detail, malformed JSON and oversized responses.
   None may invent completeness, source absence or preserved unavailable content.
5. Crash after response/before commit, failed evidence commit, lease reclaim,
   duplicate delivery and bounded retry preserve atomic checkpoints and exact
   source-version evidence. No DB connection is held across upstream latency.
6. Cross-job tests prove assessment and snapshot exclusion, shared source pacing,
   cancellation and credential replacement/disconnect fencing in both directions.
   Revoked initiator cannot be bypassed by retry from another member.
7. Prove wrong key/purpose/tenant/run/capture rejection and captured-log redaction.
   Malformed content is encrypted evidence, never raw SQL/Operator/log input.
8. Preview tests prove deterministic precedence/counts, no names-only Person merge,
   normalization-only overlap labels, ambiguous author/assignee/stage mappings,
   HTML/recurring-date/numeric/date-only cases and destination-stale reporting.
9. Real-browser synthetic walkthrough: proposal/confirm, progress, reload, pause/
   retry/cancel, old report, per-record review, member denial, Organization switch
   and 390px layout. Account/schema-qualified live testing is separately pending.
10. Query plans over the D-050 envelope show indexed claim/latest/page/record/contact
    lookups and bounded preview reads. Benchmark only if a changed hot path triggers
    D-050; no repeated 019b benchmark or above-envelope concurrency exercise.
11. Run targeted unit/Web/DB tests, `./scripts/sqlx-prepare`, `./scripts/check`,
    `./scripts/check-db` once on the frozen implementation; never overlap DB gates.
    Map all acceptance items and failures to the verification record. No tests in
    this section have been executed for 010b; only planning/document checks apply.

## 9. Open review items and next gate

Accepted: core-first sequence, explicit remaining-family coverage (D-061),
new-Organization-first eventual import (D-059), and live-validation deferral.
Everything else above is a reviewable proposal, not an accepted product policy.

- **Scope/profile qualification:** verify exact field/flag behavior and pagination
  on each core collection and note detail. Public metadata is available; live
  behavior is not yet tested. Unexpected source restrictions stay visible and
  must not be replaced with scraping or an undocumented endpoint.
- **Storage/recovery defaults:** review the proposed snapshot/Organization budgets,
  cap-increase mechanism and retained-run lifecycle. Recommendation: bounded
  capture that pauses, immutable committed evidence, no automatic deletion.
  Alternative: smaller staging budgets requiring more operator intervention.
  Retention/erasure of real customer data remains its own D-015/O-012/O-013 gate.
- **Preview-only decisions:** proposed matches and normalization overlaps do not
  approve import matching, author substitution, date conversions, formatting loss,
  source deletion handling, rollback or Today behavior. Resolve those with 010c+
  specifications using this report; do not implement an implicit import policy.
- **Review gate:** independent plan/contract review has not yet been performed.
  Follow `docs/prompts/04-review-plan.md`, reconcile findings, then obtain explicit
  approval of this specification/brief before schema or application changes.

Planning does not await the user's live test to remain useful. Implementation
must be fixture-driven until source validation is resumed explicitly; no fixture
result closes the separate live-validation criterion or customer-data prerequisites.
