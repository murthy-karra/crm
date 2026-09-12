# Slice 010d1 — Historical source capture and coverage

**Release follow-up:** D-070 subsequently authorizes commit/merge/push, owned
cleanup and shared-development deployment. The implementation-time restrictions
below retain their historical scope; current release status is recorded in
[SLICE_010d1_RELEASE.md](../tasks/SLICE_010d1_RELEASE.md).

**APPROVED FOR IMPLEMENTATION — D-070, 2026-09-12.** The user said “Ok go for it”
after the complete plan and READY [independent review](../tasks/SLICE_010d1_REVIEW.md)
were presented. This accepts the complete specification, policies/shared contracts
and isolated synthetic verification. D-069 accepts the capture/timeline split.
Baseline main `028d6133e1b7c3f81642275e030f63e98b2cca49`.

Inputs: AGENTS; [decisions](../decisions/DECISION_LOG.md)
D-012/015/050/059–069; [architecture](../architecture/ARCHITECTURE_BASELINE.md);
[010b](SLICE_010b.md) and [concrete contract](SLICE_010b_CONTRACT.md);
[010c](SLICE_010c.md), [010f1](SLICE_010f1.md), [010f2](SLICE_010f2.md);
[source qualification](../research/SLICE_010d_FUB_SOURCE_CONTRACT.md),
[code discovery](../research/SLICE_010d_CODE_CONTRACTS.md),
[ladder](SLICE_010d.md) and [execution brief](../tasks/SLICE_010d1_IMPL.md).

## 1. User outcome and scope

A current Organization admin can prepare and explicitly confirm a new historical
capture against the FUB account that produced a completed 010c People import.
The app retains the exact returned evidence, resumes safely, and shows a bounded
report of captured events, calls and text messages, records that cannot be linked
to the original imported People, conflicting observations and source restrictions.

The result is **retained source evidence for later interpretation**. No Inquiry,
call, contact attempt, text conversation, correspondence, Person, note, task,
maxima, routing/assignment, notification, realtime history or Operator fact is
created or changed. Existing review Person endpoints do not read these tables.
The durable admin review hold remains; no activation or communication permission
is granted. No live source/customer access is part of this implementation plan.

Three fixed historical streams are included, with identity validation. Current
user-directory recapture is excluded: preserve numeric source user identifiers
and any source-provided names as evidence. Original core users may be shown only
as explicitly older supporting evidence, never as proof of a current actor/name.
010d2 decides whether fresher user capture is needed for attribution.

No ordinary emails, marketing campaigns/events, attachments, recording audio,
transcripts, call summaries, media URLs, source writes, CSV fallback, mapping
repair, delta imports, ongoing sync, native mobile or new services. Returned
text/inquiry content belongs to encrypted capture; this does not authorize a new
body-reading product surface. Capture preserves unknown JSON fields as evidence.
It does not retrieve the resources to which those fields refer.

## 2. Declared shared-contract changes

Under D-070, the 010d1 implementation owner owns the following approved changes.
This is the AGENTS §11 declaration; no existing accepted shape changes silently.

| Current contract | Proposed contract / reason | Affected components, compatibility and amendment |
|---|---|---|
| `fub-core-v1` has six closed families and eight streams; `FubReader::snapshot` takes its closed request | Add separate `fub-history-v1` request/profile and reader method for three collections; history is absent from core | Rust reader/mocks/worker; retain core enums/requests/representations. Add pointers in 010b §3/§4 and concrete contract. |
| Completed People/metadata/activity parents bind the original core sequence | Add an independent history capture run bound to the immutable completed People parent and same source account | Additive tenant-keyed tables/typed commands; no parent snapshot append/repoint, child uniqueness change or workspace rebinding. Pointers in 010c/f1/f2. |
| Assessment and core capture enumerate each other for source admission/disconnect | Include history in every source-admission, lease/credential invalidation and cancellation path | Both old and new Rust/store paths change together under Org lock; existing HTTP bodies unchanged. 010a/010b source-lifecycle pointers. |
| Existing Org retained-byte ledger includes core and import children | History owns run allowance/reservations and charges the same Org ledger | Migration schema/store/config and sibling cancellation/accounting tests; preserve all prior balances. 010b §4 accounting inventory amendment. |
| Existing migration API/Web cannot inspect historical evidence | Add scoped history capture commands, detail/coverage and paged record projections | New additive HTTP namespace and Web card; existing Person, native and Operator contracts unchanged. This spec §6 and new concrete contract own fields/errors/cursors. |
| Preflight knows workspace/metadata/activity capabilities | Add `fub-history-capture-v1`, independent confirmation readiness and durable confirmed-history count | Preflight/report/API startup/worker; missing capability fails history confirmation/recovery. No history-reader capability or timeline claim in 010d1. §8 owns additive fields and launch consequences. |

One new migration, no edits to applied SQL. Concrete column/DTO/error definitions,
constraint/index names and byte inventory are recorded in
`docs/tasks/SLICE_010d1_CONTRACT.md` before client implementation. Implementation
may choose reversible internal naming, not weaker fidelity or new policy.

## 3. Frozen source profile

Use the public-doc-qualified rules in the source report and an implementation
manifest containing retrieval date, official URLs, response schema digests,
profile version, parser version and exact request forms. Public documentation
and synthetic fixtures establish the supported contract, not actual account
coverage. A later live mismatch pauses for qualification; never broaden requests
or waive fidelity in order to finish a run.

`fub-history-v1` uses the existing fixed `https://api.followupboss.com/v1/` origin,
deployment-owned system headers and saved encrypted connection. No redirects,
arbitrary path/query, executable `nextLink`, source body URL or client credential.
Retain the existing 10-second deadline, 4-MiB decoded response cap and one shared
in-process source permit/Retry-After pacing across assessment/core/history.

| Stream | Request representation | Meaning retained |
|---|---|---|
| `events` | GET `/events`, documented default collection fields | Vendor lead-event record and original values; not a native Inquiry or proof of its actual occurrence time |
| `calls` | GET `/calls`, documented default collection fields | Vendor call log; duration/outcome/direction/user/time are source values, not local LiveKit lifecycle or reached/sent classification |
| `text_messages` | GET `/textMessages`, documented default collection fields | Vendor message record; no inferred delivery/consent, unread state or outbound attempt |

All collection requests use `limit=100&offset=0` initially, no explicit sort,
then the exact continuation below. No Person/user/time filter, `fields` expansion
or detail GET is added. This **requests** API-visible account scope even when
some People are absent from the older parent. Unfiltered text enumeration and
its pagination parameters are inferred from general pagination guidance plus
the text response metadata; they are not endpoint-specific public guarantees or
live-qualified behavior. Approval accepts this conservative provisional profile,
not a claim that an actual account will support it. Unlinked records are retained
and counted; do not make new People or match on contacts.

Freeze these rules as constructed fixtures, preserving documentation weaknesses:

| Stream | Collection key / continuation policy |
|---|---|
| Events | `events`; offset mode until an explicit valid nonempty `next` token; then token mode only |
| Calls | `calls`; offset mode only. A nonempty token or source demand for unsupported deep pagination pauses |
| Text messages | `textmessages`; same conditional token policy as events, explicitly provisional as described above |

Every accepted page requires matching metadata collection, limit 100 and current
offset (including cumulative returned position in token mode), ≤100 items, and a
canonical nonnegative exact total. Freeze the first total; changed/missing totals
pause without advancing, including after Resume. Let `p` be prior returned item
count plus this page's item count. Require `p ≤ total`. In offset mode a valid
nonempty token may be adopted only for an allowed stream with items and `p < total`;
otherwise `p < total` requires a full 100-item page and continuation by offset `p`.
A short nonterminal page, empty nonterminal page, repeated full-page semantic
digest or any previously used token pauses. Retain token/page digests in bounded
indexed rows charged to this run; do not accumulate them in an unbounded cursor.

For `p == total`, token mode requires explicit `next:null` and null/absent
`nextLink`; offset mode allows absent/null `next` but requires null/absent
`nextLink`. A token or link at the terminal count is inconsistent. An initial
empty response is exhausted only with offset 0, total 0 and that terminal shape.
Never fall back from token to offset mode. Reconstruct token requests as
`limit=100&next=<percent-encoded token>`; keep tokens ≤2,048 bytes and validate
them under core's control-character rule. Ignore the value of a nonterminal
`nextLink` as transport instructions; it cannot authorize following its URL.

Before marking any stream enumerated, reconcile the **distinct valid record
IDs from advancing pages** against the frozen total, and require zero invalid-ID
occurrences and zero repeated IDs within/across those pages. Returned occurrence
count alone is insufficient. For example, total 200 with pages 1–100 and 100–199
must pause `enumeration_identity_uncertain`, even though the pages differ and
200 rows were returned. Preserve equal repeats, conflicting variants and invalid
occurrences for review, but do not mark this stream exhausted. Failed/diagnostic
captures and refetches that never advanced a checkpoint are outside this count
basis and cannot fill a missing ID. Store the advancing/diagnostic classification
immutably with each capture.

On terminal reconciliation failure retain the terminal candidate checkpoint and
counts; Resume may revalidate retained evidence but cannot fix this by fetching
the same terminal page again, rewinding or ignoring records. A changed source or
new profile requires a separately confirmed new run. Explicit future subset
import from incomplete capture, if desired, belongs to 010d2. Unique-ID equality
still cannot prove a stable point-in-time snapshot; that known limitation remains.

Unqualified/malformed metadata, ignored request parameters, unsupported deep
offset, token loops or no progress pause with closed reasons such as
`pagination_unqualified`; preserve bounded failed evidence. No ID guessing,
time-window partitioning, reduced page size, automatic restart or fallback
endpoint. Unexpected live behavior needs later explicit qualification/profile
approval, not a relaxed fixture. These conservative rules may pause an otherwise
usable vendor stream. Partial capture never becomes a completion claim.

GET `created`/`updated` remain vendor fields; POST-only occurrence fields do not
prove GET semantics. Freeze capture start/end, each response time and each
reported total with its basis. Do not claim an atomic snapshot, deletion/change
feed or consistency with the older People snapshot. Detail-only fields/media
and UI-only records are explicit gaps even after enumeration succeeds.

## 4. Capture boundary, identity and lifecycle

### 4.1 Parent and source authority

Require the same Org, completed People import, original account, confirmed
People plan, original snapshot/final capture sequence and current unchanged
review workspace binding. A completed f1/f2 child is not required. Store these
references immutably; use composite Org foreign keys and validate the parent
under the established lock order before proposal/confirmation/commit.

Proposal is DB-only and pins connection ID/revision and source account evidence.
At source confirmation, startup/restart/resume validate live identity with the
current connection; match the account to the parent. Different source-user
access from the older snapshot is allowed only with explicit presentation and
acknowledgement on the proposal; within a run, freeze both account and source
user. Unverifiable identity or changed account/user/credential revision pauses
and fences source work. A replaced credential requires a new proposal/run.
Pin the proposal's source account/user from the current connection's retained
validated identity. The first worker identity response must equal that exact
acknowledged pair; it cannot choose a fresh user baseline after confirmation.
Identity validation is the first queued worker request, not source I/O inside
the confirmation HTTP transaction. A mismatch retains only bounded failure
evidence, performs no collection request and requires a new proposal.

Only the initiating current admin may confirm or resume source work with the
same connected credential revision. Check current membership/admin authority,
connection, parent and fenced lease before request and before commit. Current
admins may read retained reports after disconnect, cancel and increase budgets;
these rights do not confer source access or takeover of another initiator's run.

### 4.2 Lifetime and state transitions

Allow multiple independent capture attempts per parent over time: a terminal
capture is immutable; a fresh confirmed capture is explicitly another source
read and never merges/replaces earlier observations. This is necessary after
credential replacement or terminal cancellation. There is no default “latest
capture wins” and no native history child in 010d1. Later import freezes one
named capture boundary under its own approved contract.

At most one active source job per Org across assessment/core/history. Queued,
running and waiting-retry history jobs participate symmetrically in old/new
admission checks. Paused/proposed runs do not hold the source slot; every resume
rechecks it. Org locks and bounded row waits precede domain/job locks. No DB
connection or transaction is held during source I/O.

| State / transition | Observable rule |
|---|---|
| `proposed` → `queued` on explicit confirmation | Freeze source profile/boundary, actor-bound request receipt, disclosure acknowledgement, budget and durable capability requirement. No source I/O in the HTTP transaction. |
| `queued` → `running` | Worker claims fenced lease/reservation, verifies identity and current authority, then sequentially captures streams. |
| `running` ↔ `waiting_retry` | Durable three-attempt cycle for transport/429/5xx, honoring valid Retry-After. Exhaustion pauses; no busy loop or automatic new cycle. |
| Any source failure requiring review → `paused` | Preserve committed evidence/checkpoint, bounded reason and recovery action. 401/revision/demotion fences; collection 403/404 never means empty or complete. |
| `paused` → `queued` on explicit Resume | Original initiator, same revision/account/user, fresh readiness/admission/budget checks. No automatic resume when budget/config/connection changes. |
| All three streams enumerated → `completed_with_gaps` | Freeze final capture sequence and coverage counters. The profile has known source/content restrictions; no unrestricted `completed` or “all history preserved” claim. |
| Nonterminal → `cancelled` | Any current admin, idempotent receipt; fence in-flight work, release only this run's reservations, retain all committed evidence and durable capability requirement. Terminal state cannot resume. |

Exact same actor/request ID/body replays the original receipt; changed body or
actor with the same request ID conflicts. Authorization is rechecked before
returning replayed sensitive receipts. Uncertain network responses reuse the
same ID/body. Commit capture/index/checkpoint/byte settlement/counters atomically
under lease fencing; an expired worker cannot settle or release another worker's
reservation. A response that could not be committed may be refetched, with no
exactly-once external-request claim.

### 4.3 Raw evidence and reconciliation

Retain exact bounded response bytes and immutable capture sequence, validated
request representation, HTTP status, profile/schema/parser versions, times and
integrity. Reject duplicate decoded keys/lossy numbers/excessive nesting/nodes
using the existing bounded lossless parser. IDs are canonical positive decimal
strings ≤128 digits, not JavaScript numbers. Invalid/missing IDs stay linked by
capture and ordinal as invalid observations, never discarded or invented.

Identity is `(Org, source account, family, vendor ID, representation)`. Compare
semantic HMACs including unknown fields with canonical object keys and preserved
array order. Equal replay observations do not add unique records. Different
variants stay retained with no preferred winner; timestamp-based last-wins is
forbidden. Source IDs and identities in raw/body/cursor data remain encrypted
or tenant/run-purpose HMAC indexed as appropriate under existing conventions.

Capture counts, valid-ID occurrences, invalid-ID occurrences, unique IDs, equal
repeat observations and distinct conflicting variants are separate measures.
For valid unique IDs the link dispositions are disjoint: exact imported-parent
Person link, source Person excluded/held by parent, no parent identity, invalid/
missing Person reference, or conflicting reference. A “linked” status proves
only identity to a live same-Org imported Person; do not label it importable.
If the original Person is erased, retained read rechecks it and presents an
unavailable reference; it must not reveal an erased native name or recreate it.

For group texts, the primary `personId` link is not sole ownership of the message.
Retain all participant/reference structure in raw evidence, flag multi-Person or
unqualified relationship references in the projection and inventory every parsed
Person linkage with a bounded per-page derivation limit. Overflow pauses; do not
truncate the linkage index and call it complete. No participant fan-out, deduced
contact match or native import eligibility is decided in this rung.

Retain bounded error/oversize prefixes as encrypted failed captures, flagged
incomplete; never index them as complete records or advance the stream. Counts
of unknown UI-only/inaccessible records are unknown, not zero. Family coverage
is separate from enumeration, content availability and parent-link disposition.

## 5. Storage, quotas and bounded work

Reuse the existing encrypted migration store conventions with independent AEAD/
HMAC purposes for history run/capture/projection/checkpoint/receipt. No body,
subject, address, phone number, user name or vendor outcome label in append-only
facts, audit logs, traces or metrics. No new plaintext content table or generic
raw/body download endpoint. Exact call/text/event JSON may reside in encrypted
PostgreSQL migration storage under current non-email raw-source rules; D-062
does not authorize fetching bulk emails into it.

History has its own approved run limit (initial min of 2 GiB and configured run
ceiling) and uses the existing approved Org retained-byte limit (initial min of
4 GiB and configured Org ceiling if absent). It never resets an existing limit
or balance. Use the current snapshot ceiling configuration and report policy
revision/effective minimum; these are synthetic logical allowances, not customer
quotas or physical DB capacity. All historical, cancelled, failed and partial
captures and reports continue to count. Monotonic, revision-checked budget
increases record an actor-bound receipt and require separate explicit Resume.

Reserve before source I/O. The proposed 16-MiB history request reservation must
be proved from concrete counted columns before implementation is complete:
≤4 MiB exact capture, ≤100 encrypted observation projections of ≤16 KiB,
bounded IDs/hashes/index keys/checkpoint and encryption overhead must settle
within it. Do not store a second full canonical record if its bytes are already
recoverable from capture+ordinal; compute HMACs transiently. Parser limits bound
input/depth/nodes before allocation. Identity requests and bounded failures are
included in admission/settlement. Derived overflow retains a bounded encrypted
failure and pauses without advancing. Actual byte accounting, not reservations,
charges retained usage; no metadata is exempt merely because it is encrypted.

DB-only variable writes also require admission in the same transaction. At
proposal reserve a separate 8-KiB control allowance for one bounded terminal
cancellation receipt (including envelope/encryption); prove this bound in the
concrete inventory so a full data allowance cannot prevent cancellation. Other
control receipts must admit their exact bounded bytes before mutation; repeated
receipt replay consumes no extra bytes. Budget increases may admit their receipt
against the newly approved limits atomically. Release unused control allowance
only at a terminal state after settling its receipt. All receipt sizes and
statement/input bounds must be frozen; insufficient budget cannot silently drop
an audit receipt or make the run exceed its approved retained+reserved limit.

Record the full byte inventory, maximum fixed fields and constraints in the
concrete contract. Count variable ciphertext/nonces, HMACs, stored source IDs,
request representations, checkpoint state and retained receipts/projections.
Fixed-size IDs/counters/state/timestamps are explicitly bounded operational
metadata. Shared Org admission and sibling cancellation must be tested both
ways with assessment/core/import/f1/f2 activity; one owner cannot release or
spend another's reservation. Native tables/physical storage are separate totals.

Pages use keyset indexes, never decrypt/filter all source rows or fetch all
history before a size check. Projection responses are ≤50 records and ≤512 KiB;
each rendered record ≤8 KiB including envelope. Clip source labels with an
explicit `preview_truncated` marker while retaining exact raw bytes. Full message
bodies and subject text are not in these projections. Detail is one bounded
projection plus provenance, not unrestricted raw JSON. Capture/run listing is
≤50 rows. Counts come from transactional counters, not full decrypting scans.

Add every new retained store/key/Person linkage/receipt/cursor/cache to the
erasure inventory. A raw response can contain multiple People; document that
linkage and existing unresolved selective-erasure/backup limitations. This is
not a new erasure policy or completion of first-customer readiness gates. Real
customer processing remains blocked by its existing prerequisites.

## 6. Typed commands, HTTP and retained queries

All routes are new under `/api/migrations/fub/history-captures`, same-origin,
current server-derived Org/admin, strict unknown-field rejection and `no-store`.
No model-supplied authority, organization override or privileged Operator path.

| Method / suffix | Typed operation / required inputs |
|---|---|
| POST collection | `ProposeFubHistoryCapture`: request UUID, completed parent import UUID, connection UUID/expected revision; DB-only |
| GET collection | Bounded run list, optional exact parent filter and local cursor |
| GET `/{id}` | Run state/profile/parent/source-account summary, per-stream coverage/checkpoints/counters, budget/policy/revision, capabilities and next action |
| POST `/{id}/confirm` | `ConfirmFubHistoryCapture`: request UUID, expected run revision and acknowledgement of named API-visible account scope/coverage/source-user differences |
| POST `/{id}/retry` | `ResumeFubHistoryCapture`: request UUID and expected run revision; original source initiator only |
| POST `/{id}/cancel` | `CancelFubHistoryCapture`: request UUID and expected run revision; any current admin |
| POST `/{id}/budget` | `IncreaseFubHistoryCaptureBudget`: request UUID, expected run/Org budget/policy revisions, decimal run and Org limits |
| GET `/{id}/records` | ≤50 projected observations, family/link-disposition filter, optional local record UUID and local cursor |
| GET `/{id}/records/{record_id}` | One bounded observation projection, retained-reference/version/digest status and variant counts; use the records route filtered by this local record UUID for paged same-source-identity observations; no body/raw endpoint |

Freeze acknowledgement as named booleans/closed codes and source evidence
revision in the concrete contract; it cannot be a generic “I accept everything”
string. No arbitrary family selection that silently omits a required stream.

Anonymous 401; nonadmin 403; foreign/missing resources 404; malformed 400;
stale/terminal/incompatible/active-source conflicts 409 with closed codes;
unconfigured service/key readiness 503. Errors never contain source bodies,
credentials, URLs with tokens or foreign resource existence. Counts/bytes/vendor
IDs/revisions use decimal strings; resource IDs UUID strings, dates RFC3339 when
qualified. Unknown vendor timestamps remain flagged source strings in encrypted
evidence rather than a fabricated normalized time.

Local AEAD cursors bind endpoint, Org, run, family/filter, page size, stable
keyset and frozen committed capture sequence. Later source commits do not enter
an in-progress page series; refresh starts a new series. A terminal run keeps
its final sequence. Record linkage/native-name lookup rechecks current authority
and erasure on every response. Never expose upstream tokens or execute a cursor
as SQL/source requests. Cross-Org/run/filter/corrupt cursors fail closed.

Rows are immutable observations ordered by `(capture_sequence, ordinal, local
observation UUID)`; invalid-ID observations remain independently pageable.
The optional `record_id` filter resolves an observation in the same Org/run and
selects only its source-identity variants. For an invalid-ID observation it
selects that single observation. Counts returned with a page are explicitly
labeled current-run counters or bounded-to-series counts; never mix those bases
under a single total. Link status and variant annotations must derive only from
observations at/before the cursor's fixed sequence.

## 7. Web review flow

In the completed People import's migration page, add **Historical capture** with
Events, Calls and Text messages coverage rows. Explain that this retrieves
API-visible historical evidence and that timeline import is a later step.
Show the named account/parent, older People capture period, current connection
source-user evidence, proposed full-account scope, logical allowance and known
API/content limits before a named confirmation. Preparing causes no source call.

Show active progress, retained usage, paused reason/recovery, explicit Resume,
separate budget controls and retained cancellation results. Poll only active
work. Use existing session/Org/revision fences, clear stale data on identity
switch, reject late responses and preserve exact uncertain-request replay.
Reload reconstructs state from the server, not local job ownership assumptions.

The evidence table pages observations with source kind/date labels, known/unknown
actor identifiers, linked/held/unlinked status, duplicate/variant warnings and
content availability. Do not display source HTML, bodies, recording links or
labels as executable content. No URLs automatically fetched or linkified into
media playback. Explicit truncation and “retained, not imported” language are
required. No native CRM edit button on a source row.

A terminal report says **Capture finished with coverage gaps**, with separate
enumerated counts, API-restricted unknown counts, content not fetched, unresolved
parent links and newer-than-parent timing. A paused/cancelled report says partial.
No green “migration complete”, fabricated zeroes or inferred Inquiry outcome.
Keep ordinary member hold and current Person review behavior unchanged.

## 8. Release, observability and recovery

Add `fub-history-capture-v1` to current operator-owned artifact/preflight reports,
plus `history_capture_confirmation_ready`, schema presence and confirmed-history
run count. Fresh confirmation/resume requires current matching observed evidence
for active and selected API/worker artifacts, extending existing freshness and
artifact hashing rules. Missing/unknown capability is not inferred from activity
readiness. Proposed/never-confirmed runs do not require the new launch capability;
once confirmed, the durable requirement survives completion/cancellation.

The updated preflight rejects incompatible recovery candidates when that durable
count is nonzero. Startup/worker validates the new state/profile before processing.
Older binaries do not learn new compatibility rules automatically: inventory and
retire incompatible API/worker candidates before confirmation; never claim an
old embedded preflight can enforce this requirement. No report refresh, retirement
or shared runtime deployment happens during this planning/implementation task.

Log/trace only safe Org/run/capture IDs, closed operation/state/reason, lease/retry
counts, durations and byte/counter totals. Keep high-cardinality IDs out of metric
labels. No source content in request-debug/error/SQL bind logging. Retain receipts
and capture integrity for reconciliation without logging customer text. Include
sentinel leak tests with positive controls.

Crash recovery reclaims expired leases under fencing, reconciles only owned
reservations and resumes exact committed checkpoints after authority/readiness
checks. No network work while disconnected/demoted; retained reads continue for
current admins. Key/profile/integrity failures pause without partial native work.
There is no destructive rollback: preserve captures/byte accounting, restore a
compatible artifact and use explicit recovery. Do not delete evidence to make an
old runtime launch. Real-data erasure/restore readiness remains separate.

## 9. Acceptance criteria and verification

| ID | Observable acceptance | Required proof |
|---|---|---|
| A1 | DB-only proposal against completed same-account parent; parent/siblings/workspace are unchanged | Domain/DB/HTTP positive and cross-Org, incomplete parent, changed binding/account negatives; exact before/after reconciliation |
| A2 | Only qualified fixed historical GETs; no source writes, URLs, details or email/media fetch | Reader fixture/request-recorder tests for all paths/parameters; public schema manifest; adversarial redirect/token/body URL cases |
| A3 | Honest pagination, bounded parsing, raw fidelity and variants | Exact bytes/HMAC/large-ID/duplicate-key/unknown-field tests; multi-page/empty/short/change-total/no-progress/deep-offset cases; overlapping nonidentical pages (1–100, 100–199), invalid IDs and unique-ID terminal reconciliation; no false exhaustion |
| A4 | Safe source authority and one active job across all source kinds | Direct-domain/HTTP/current-role tests, concurrent old/new admission, reconnect/revision/demotion/cancel/in-flight commit races, no connection held during I/O |
| A5 | Crash/retry/uncertain response is idempotent and recoverable | Lost-response receipt replay, changed-body/actor rejection, lease takeover, partial commit, retry cycle/Retry-After, explicit Resume and terminal-cancel tests |
| A6 | Exact shared-byte settlement stays within logical budgets and owner bounds | Concrete worst-case reservation proof and DB octet-length reconciliation; caps/oversize/encryption overhead; sibling reservations, cancel/crash and lowered ceiling/increase races |
| A7 | Coverage distinguishes enumeration, unique records, variants, invalid IDs, API restrictions, content and parent linkage | Table-driven fixture reconciliation with exact disjoint counts; absent/erased/excluded parent People; unknown count is never zero |
| A8 | Retained reads are bounded, admin-only, source-free and correctly paged | Direct/HTTP tenant and cursor negatives; frozen-sequence page traversal during capture; ≥25k source rows per family and ≥500 records for one Person; EXPLAIN of actual queries, bounded decrypt/response sizes |
| A9 | No business history/Today/Operator/provider/outbound side effects | Exact native row/maxima/binding before/after checks, side-effect spy counters zero, unchanged legacy admin reads and ordinary-member hold |
| A10 | Web handles the full flow and names limitations truthfully | Focused Web tests plus production-Web/synthetic-real-API walkthrough: prepare/confirm, pause/resume/budget, variants/unlinked, uncertain replay, reload/demotion/Org switch, cancellation; desktop and 390px inspection |
| A11 | Release capability and retained-data recovery fail closed | Preflight unit/CLI tests with observed artifact fixtures; stale/forged/missing capability, confirmed cancelled runs, mixed candidates and restart/profile/key failures |
| A12 | Content stays encrypted and new stores are inventoried | DB ciphertext/AEAD isolation and log-sentinel tests; inspect no body/read endpoint or URL fetch; erasure inventory records all raw-page/Person links and remaining customer-data gates |

Use isolated synthetic FUB fixtures and disposable Postgres/Centrifugo. Test
unauthorized cases against actual foreign resources with positive controls;
random nonexistent UUIDs alone do not prove tenant isolation. Follow D-050's
25k-People/50-member envelope and two implementation review rounds. No full
timeline/Today benchmark is warranted when those readers do not change. Measure
the new capture/page queries and a single paired check of any shared read path
actually modified. Required commands and evidence are in the execution brief.

## 10. Approval and later decisions

010d1 proposes multiple separately confirmed captures, full API-visible account
scope, metadata-only admin projections, fail-closed enumeration and a new durable
capture capability. These are concrete reviewable choices under the accepted
split, not implicit changes to previously approved import lifetimes or fidelity.

Independent planning review is READY; D-070 accepts this complete spec/brief
for implementation under AGENTS §11. Live FUB/customer processing,
commit/merge/push/deployment and activation retain separate authorization. 010d2
owns historical interpretation, body exposure, native projection/Today policy,
import lifetime and bounded timeline compatibility; no such policy is guessed
here. D-062/O-012/O-002 content dependencies remain visible in the ladder.
