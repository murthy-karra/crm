# Slice 010d2 — Imported historical timeline

**IMPLEMENTED / VERIFIED WITH ISOLATED SYNTHETIC DATA — 2026-09-12, D-072.**
See [implementation verification](../tasks/SLICE_010d2_VERIFICATION.md).
Source remains uncommitted and undeployed. The
[focused review](../tasks/SLICE_010d2_REVIEW.md) is READY after one correction pass.
The user approved the specification, execution brief and declared shared contracts.
Baseline: `27f3fe4654ace5840412e1364ea36974a1266ad8`.
[010d1 is deployed](../tasks/SLICE_010d1_RELEASE.md); its capture semantics stay
frozen. Read the [history ladder](SLICE_010d.md),
[code findings](../research/SLICE_010d2_CODE_CONTRACTS.md) and
[execution brief](../tasks/SLICE_010d2_IMPL.md) together.

## 1. Outcome and approved policy

An Organization admin can preview and confirm an import from one completed
010d1 capture, then inspect attributed, source-labeled historical records on an
imported Person. Large histories use bounded pages. Coverage and held records
remain visible beside the imported results.

The approved initial policies are:

| Topic | Approved policy |
|---|---|
| Meaning | Three separate typed external facts: FUB event record imported, FUB call record imported, FUB text record imported. They do not create native Inquiry/call/contact-attempt/correspondence/consent facts or change Today maxima, filters, outreach credit, assignments or stage. |
| Time | Chronology uses explicitly labeled **FUB record-created time**, with a separate undated page for unknown dates. Source sent/updated times retain their own labels; none becomes a guessed occurrence time. |
| Attribution | Import executor, source access user, attributed source user, creator and editor are distinct. Missing/deleted source users remain unresolved. No automatic mapping to the importing admin. |
| Input | Only a `completed_with_gaps` capture with its exact completed People parent is eligible. The first confirmation pins one capture, interpretation version and immutable plan for that parent. Additional attempts may continue that same plan; another capture or changed interpretation requires later repair/delta policy. |
| Conflicts | Equal observations collapse by qualified stable identity; conflicting variants and ambiguous/group relationships are held. No latest-wins choice, phone matching, participant fan-out or resurrection. |
| Audience/content | **User-selected: metadata first.** Admin timeline/detail expose metadata only, preserving 010d1's no-body boundary. New qualified metadata fields are declared below; readable bodies require a later approved contract. |

D-064's migration-review hold and Organization-wide PersonVisibilityScope remain
separate checks. Operational Person/Today/Operator/outbound behavior and future
activation permissions are unchanged. Live FUB validation remains deferred.
Email, media, recordings/transcripts, source writes, deltas and activation are
outside this slice; remaining fidelity gaps stay in the migration roadmap.

## 2. Declared contract changes

These are approved owned changes; implementation does not deploy them:

| Current contract/behavior | Proposed contract and reason | Affected components / compatibility |
|---|---|---|
| 010d1 exposes encrypted evidence through metadata-only capture APIs; it has no imported facts. | Add independently confirmed retained-history import commands, immutable typed facts, reconciliation and a separate timeline. | New application/API/store/worker/Web modules and additive migration; 010d1 source/profile/projections unchanged. |
| Legacy `GET /api/people/{id}` assembles complete inquiries/history. The 010f2 review core also loads all inquiries/core history. | New `/api/people/{id}/migration-review/v2` returns bounded core summaries and page URLs. New inquiry and timeline routes page the existing native facts and imported external facts. | Existing success DTOs stay unchanged. Both complete-reader paths in the upgraded API return a typed 409 once a history plan is confirmed; new Web uses v2. Older artifacts fail closed at the common DB boundary and must be retired. No silent truncation or new fields pretending to be complete arrays. |
| Existing activity capability fences complete reads only for an activity binding. | Add durable history binding and independent `fub-history-timeline-v1` capability. Fence old complete readers before the first external fact can commit. | Workspace DB guard, HTTP errors, startup/worker admission, operator-owned preflight/inventory and Web recovery handling. Old compatible-with-capture-only artifacts cannot serve/recover a confirmed timeline workspace. |
| 010f2 concrete contract describes core history as only assignment/stage/inquiry; actual code includes more native fact families. | Correct that descriptive inventory when this contract is approved. V2 explicitly covers the actual eight families listed in §6. | Existing 010f2 wire behavior is preserved before the new fence; this draft records the discrepancy instead of silently narrowing it. |

D-072 approves these contracts and the policy table for implementation.
Concrete Rust DTOs, SQL constraints, closed errors and cursor format are frozen
in an implementation contract before parallel client work begins. No migration
may alter previously applied SQL files.

## 3. Interpretation and immutable input

A DB-only preparation command creates a child import. A bounded worker verifies
the same Organization, source account, completed parent import/plan/snapshot,
original workspace revision and both fixed capture sequences. Pin the capture
ID, terminal revision, profile/schema/parser versions, interpretation version,
parent mapping and coverage report. Connected source credentials and fresh FUB
identity are not required for retained reads or this import; no source reader
is callable from its worker.

Interpret authenticated original capture bytes plus exact ordinal using the
existing lossless parser discipline, with a separately versioned interpretation
profile. Recheck source family, representation, account, request chain, source
ID and semantic binding. Import only accepted advancing, nontruncated captures.
Diagnostics/identity pages remain evidence, never substitute record input.
Do not interpret the lossy 010d1 metadata projection as a complete source record.

Build per-occurrence manifest rows in bounded batches. Preserve every occurrence
and its outcome; never retain all observations/variants in one JSON array.

| Qualification | Result |
|---|---|
| Valid positive source identity, one canonical variant, exact live parent mapping, unambiguous relationship | Eligible external fact. Missing dates or attributed user are explicit unknown metadata, not automatic exclusions. |
| Equal repeat of that identity and complete canonical record | Same candidate/fact, with repeated-evidence counts and references. |
| Distinct canonical variants, including differences in unknown retained fields | Hold the entire identity; no chosen winner. |
| Invalid/zero/missing source ID, excluded/missing/erased parent, changed binding, uncertain/nested/group references | Visible held/source-only disposition and reason; no attachment to another Person. |
| Integrity/key/profile/ordinal mismatch or extraction bound exceeded | Pause preparation; preserve evidence and reason. Never quietly skip corrupt input. |

New stable identity keys bind Organization, source account, family, vendor ID
and representation using an import-owned HMAC purpose. 010d1 HMACs include the
capture-run UUID and cannot deduplicate across captures. Canonical fingerprints
use full lossless records, not display summaries. Key-purpose/version identifiers
are part of the frozen plan. Permanent identity/tombstone records prevent exact
replay, cancellation/restart or later repair from resurrecting erased facts.

The first confirmed plan becomes the parent's immutable import anchor. Only one
execution attempt is runnable for that parent. A cancelled attempt keeps committed
facts; a newly previewed/confirmed attempt may finish the exact same anchored
plan and skip existing identities. Changing source capture, selected variants,
linkage or interpretation after the anchor exists is rejected as repair-required.
Before first confirmation an admin may discard an unconfirmed child and prepare
a different capture. This does not reconcile independent capture attempts or
claim delta support.

## 4. Fact meaning, time and provenance

Use three typed append-only fact tables, with DB-enforced append-only rules and
the D-015 standard envelope. Their occurrence is the **local import observation**:
`origin=migration`, actor is the current responsible executor established by
explicit Confirm or Resume for that committed unit, `occurred_at` is the local
materialization time, and `recorded_at` is the DB-recorded time. The source
historical timestamp is a separate nullable field with a closed basis. This does
not assert that the importer performed the historical event.

Immutable rows contain local IDs, closed kinds/time bases, qualified times,
source-reference IDs and integrity identifiers. Customer text, source free-form
type/outcome labels, names, addresses, phone numbers and source actor labels belong
in an encrypted, deletable bounded display payload, never arbitrary immutable
JSON. Keep source access/attributed/creator/editor IDs separated in that payload;
never join the importer as a historical sender. A source user reference may be
shown unresolved without creating or granting an authenticated User identity.

- Events remain “FUB event record,” preserving source classification. An Inquiry,
  Incoming Call or Unsubscribed label creates no native business or consent fact.
  `noteId` is a provenance reference, not a merge with an existing 010f2 note.
- Calls retain qualified `isIncoming`, nonnegative finite source duration values
  and their original units, attributed user and encrypted outcome label. Unknown
  types/values remain unknown. No provider room, local outcome, answered state,
  occurrence time, consent or recording is synthesized.
- Texts retain source status and direction as source claims, with `sent` separate
  from `created`/`updated`. Status is open source text, not native delivery state;
  a captured message does not establish delivery, an outreach attempt or consent.

Qualify each date independently: explicit-offset RFC3339 convertible losslessly
to PostgreSQL microseconds. Naive/unknown-offset, leap-second, out-of-range or
precision-losing values remain raw/unknown; do not round or infer a timezone.
A malformed `updated` value does not invalidate a qualified `created` value.
Known source-created values sort descending; ties use immutable recorded time,
fixed family rank and local ID, matching the cursor total order.
Unknown-created records appear in a separately labeled, paginated “Date unknown”
group, ordered by stable import position only. Never substitute capture/import
clock values into historical chronology. Show original source and import times
separately in provenance.

Every result links to its plan/attempt, typed fact, exact capture/page/ordinal,
source account/family/identity/representation, parent mapping and both parser
versions. Preserve source-access scope, API-inaccessible count=`unknown`, missing
detail/media and non-atomic snapshot warnings after import. Completing an import
means all planned outcomes are reconciled, not complete account history.

## 5. Content, privacy and erasure

The user-selected metadata-first scope returns no message/note/description body, subject,
HTML, phone endpoints, arbitrary URLs, attachments or full raw capture in the
new timeline or detail API. It may show 010d1's bounded source labels and the
qualified body-free call/text metadata above. `returned_in_raw` is labeled only
as returned data, never proof of readable or complete content. No automatic
summaries, embeddings, source instructions, external link previews or URL fetches.

A future approved content variant must specify a separate bounded on-demand
endpoint, source audience/redaction interpretation, explicit missing/oversize
behavior and erasure linkage before implementation. It must not simply expose
raw pages or assume a placeholder string establishes access. D-062 email
placement and O-012 recording/transcript prerequisites are not resolved here.

All original raw/projection payloads remain owned and charged by 010d1; do not
copy them wholesale into timeline storage. Extend the erasure inventory to new
display payloads, manifests, stable identity indexes, facts and page projections.
Erasure suppresses reads/rebuilds/replay and deletes/shreds applicable content;
retained facts carry only orphanable local references and an erasure marker.
Shared raw pages can contain several People: use the existing explicit erasure
hold/inventory rather than claiming a new per-Person key or shredding unrelated
records. Metadata-only access does not close D-015's first-customer erasure runbook
or O-012/O-013. Verify synthetic suppression/no-resurrection now; real-data
erasure/key/backup policy remains a separate readiness gate.

## 6. Bounded readers and Web behavior

V2 core serves every valid migration-review Person, including partially completed
People parents, under current admin and original workspace binding checks. History
import still requires the stricter completed parent. Core returns existing
Person/contact/tag/custom-field summaries and the bounded 010f2 activity counters/
URLs; adds inquiry/history counts, revision and URLs; removes complete inquiry and
history arrays by using the new endpoint rather than altering old success DTOs.
Its retained variable fields and response remain explicitly bounded to 512 KiB.

New routes:

- `GET /api/people/{id}/migration-review/v2`
- `GET /api/people/{id}/migration-review/inquiries?cursor=&limit=`
- `GET /api/people/{id}/migration-review/timeline?family=&dated=&cursor=&limit=`
- `GET /api/people/{id}/migration-review/timeline/{kind}/{entry_id}`

Detail keys include a closed fact kind and local ID; resolve both under the
current Organization/Person binding, never by a cross-table UUID search.

Pages default 25, maximum 50; fetch at most 51 candidate rows per fixed family,
then merge bounded candidates in the server. Never load full history to sort or
apply the limit after decryption. DTO summaries are ≤4 KiB per row and total
response ≤512 KiB. Body-free detail is ≤16 KiB. Overflow is an explicit closed
error with preserved source evidence, not a partial response labeled complete.

Timeline covers external events/calls/texts and the actual native core fact kinds:
`person_imported`, `inquiry_received`, `routing_decision`, `assignment_changed`,
`stage_changed`, `contact_attempted`, `call_completed`, `correspondence` (the
last kind projects `correspondence_captured`). Notes/tasks retain
010f2's separate bounded pages and counts; the UI names this division clearly.
Native rows are read-only fact entries with their real occurrence time and kind.
Do not reuse the operational call-history client fold or outcome-management
controls on independently paged facts: completion and correction rows may be on
different pages. Grouped operational call presentation is outside this review UI.

`family` is a closed filter; `dated=known|unknown` separates source-created
chronology from undated imported records. Native facts preserve the existing
display-time rule: contact-attempt corrections use recorded time; other native
facts use occurrence time. Native order uses
(display time, recorded time, existing kind rank, ID). The new endpoint explicitly
reverses this complete tuple for newest-first presentation; external kinds get
fixed distinct ranks. Preserve both actual source/native timestamps in detail.
Local cursor encryption binds endpoint, Organization, Person,
workspace binding, read revision, filters, page size and the last total-order key.
A DB-owned Person history-read revision changes atomically with every included
native/external fact addition, displayed-metadata change and content suppression.
The concrete contract inventories every fact and joined-display writer; no
rendered mutation may escape revision maintenance. A changed revision returns
409 `history_refresh_required`; Refresh starts a new series. No backdated insert
can silently appear halfway through a series. Reuse D-052's read-model mechanism
for revision maintenance, with coverage of every represented writer and no new
business facts from triggers. Counts must be transactionally maintained summaries,
not a full history count scan on every page. Revision, counts and every family's
candidate query for one response must share a consistent DB snapshot or equivalent
locking. A commit between family queries must not produce mixed-revision rows
under an earlier revision. Core summaries use the same consistency rule; each
subsequent request rechecks current authorization and cursor revision.

Both legacy complete readers must be fenced before complete queries as soon as
the first history plan is confirmed. The anchor persists through zero committed
facts, partial application, cancellation and erasure. Use the exclusive workspace
barrier at first confirmation, paired with the readers' shared guard.

The deployed 010f2 core bypasses `crm_activity_complete_read`; both complete paths
do encounter `crm_workspace_read`. The additive migration extends that common
DB guard to require `fub-history-timeline-v1` reader capability for an anchored
workspace, after its existing membership/workspace checks. Upgraded server-owned
read helpers set the capability transaction-locally on each actual connection,
before calling the guard, including nested SQLx reads. An outer middleware permit
or Rust task-local alone is insufficient because handlers use other connections.
Use explicit transactions/savepoints; no capability may leak through pooled reuse.
This is compiled artifact compatibility, never client-supplied authorization.

The upgraded API additionally fences both complete representations and maps the
dedicated SQLSTATE to 409 `history_review_required` with safe v2 route guidance.
Older binaries lack the capability and fail closed at the common DB guard; their
exact HTTP error is not promised to become the new 409. Existing old startups
cannot discover a new anchor table. Trusted deployment inventory must drain and
retire unsupported API/worker artifacts before import is enabled and select only
compatible recovery candidates, including artifacts predating the common guard.
Do not infer this retirement from a fresh report or claim old code self-detects
the new schema. V2 can be introduced before the anchor so new Web works on
existing review workspaces. Native/Operator clients receive no new history
write/read tool or privileged bypass.

The Web migration card shows selected capture, frozen counts, coverage, held
reasons, byte allowance, confirmation, progress, Resume and Cancel. Person review
shows History with source/date labels, imported badges, known/unknown date counts,
page controls, metadata provenance and a clear link to separately paged notes/
tasks. No complete-history browser cache or client-side unbounded merge. Identity,
Organization, workspace and Person changes clear state; late results are fenced.
Revision changes show Refresh. Reconnect/focus refetch current state; completion
never activates the workspace. Verify desktop and 390px behavior using the actual
API and a populated synthetic import.

## 7. Commands, lifecycle and accounting

Add `/api/migrations/fub/history-imports` and typed `PrepareFubHistoryImport`,
`ConfirmFubHistoryImport`, `ResumeFubHistoryImport`, `CancelFubHistoryImport` and
`IncreaseFubHistoryImportBudget`. Preparation is asynchronous retained-data work;
HTTP only admits bounded work and returns a compact receipt. Lists/details and
manifest/result pages are bounded separately. Preview must be complete before
confirmation; it freezes exact added-byte bound, outcome counts and coverage.

States: `preparing|ready|queued|running|paused|completed|cancelled`; preparation
and application checkpoints are separate. Plans expire 10 minutes after becoming
ready, before their first confirmation. Reprepare cannot alter an anchored plan.
Confirmation requires exact plan/run/workspace/policy revisions and explicit
acknowledgement of external-fact meaning, date uncertainty, coverage/held rows and
review-only use. All-held/zero-eligible plans remain reviewable but confirmation
is rejected; no empty import is presented as useful completion.

All new routes require current server-derived Organization/admin context and
return `Cache-Control: no-store` through outer middleware, including errors.
Strict request objects reject unknown fields; mutation bodies are ≤8 KiB. Counts,
bytes and revisions are decimal strings; limits are integers 1–50. Errors retain
401 anonymous/platform-only, 403 member, 404 missing/foreign scoped resources,
400 malformed query/body/cursor, 409 stale/held/conflicting/release/budget state,
and 503 unavailable or corrupt evidence. No source content appears in errors.

Every mutation uses actor-bound request UUID/body receipts, current admin checks
before replay, exact expected revisions and same-Org resource resolution. Receipts
are ≤4 KiB and reference a refetched detail. Current admin may prepare, confirm,
resume or cancel retained imports; source initiator privileges do not grant
retained-import authority and a revoked responsible executor pauses its worker.
Another current admin can explicitly take responsibility through Resume, recorded
with that actor's receipt/envelope; no automatic reassignment of source actors.

A bounded worker claims via DB-clock leases and token-fenced checkpoints. At most
50 manifest records or one bounded raw capture per unit; no network operation,
unbounded transaction, transaction-spanning wait or fixed per-process ownership
that prevents another compatible worker from taking the next unit. Recheck current
actor, workspace, parent/anchor, key/profile, lease and policy before committing
facts, stable identity, results, counters and checkpoint atomically. Unique keys
serialize overlapping attempts. A crash/retry after commit is an exact no-op for
existing identities. Cancellation serializes with commit, releases only owned
reservations and retains committed facts; it never rolls back People/core imports.

The new import owns its derived-byte run allowance and charges the shared Org
ledger without changing the completed core or history capture's charges. Use
existing deployment ceilings, proposed initial min(2 GiB, configured run ceiling)
and existing Org allowance; admin increases stay monotonic within operator limits
and require separate Resume. Reserve before every preparation/application unit;
charge exact variable bytes (display ciphertext/nonces, HMAC indexes, manifest/
result/provenance fields, encrypted cursor/checkpoint state and receipts).
Labels use UTF-8-correct bounded previews with explicit truncation flags; full
canonical evidence remains retained and drives identity comparisons. No copied
full raw payload or body. Prove a ≤32 KiB variable-byte per-record
allowance before implementation commits; otherwise revise the bound explicitly.
Max 50 records per reserved batch. Fixed UUID/enums/times and measured physical
indexes remain separate from logical retained-byte accounting.

Reserve a bounded cancellation/control allowance at proposal, retain it while
paused and settle exactly on terminal completion/cancellation. Receipt replay
must not re-charge. Expired lease reservations can be reclaimed only under their
own owner/ledger lock. Budget failure pauses without dropping records or taking
sibling allowance. Account for discarded unconfirmed preparations and erased
display bytes with explicit terminal/erasure accounting, never negative ledgers.

## 8. Release and acceptance gates

`fub-history-timeline-v1` is independent of `fub-history-capture-v1` and
`fub-activity-import-v1`. Confirmed history anchors require it for API, worker and
recovery candidates even after cancellation or erasure. Schema without a binding
is additive; it grants no readiness. Actual workload inventory/preflight and
upgraded startup classify unsupported profiles/anchors and refuse incompatible
recovery. Older startup code is excluded by the trusted inventory/retirement
requirement in §6; it is not retroactively upgraded by this migration.
Fresh confirmation/explicit Resume and worker first/recovery admission require
observed readiness; an already admitted bounded execution does not expire merely
because a five-minute report ages out. Fence new work by lease/binding/actor state.
Recovery preserves anchors, identities, facts and raw/erasure references; never
erase a binding to launch an old artifact. Deployment remains a separate task.

| Gate | Required proof after implementation |
|---|---|
| T1 — authority | Positive same-Org admin controls; anonymous/member/platform/foreign-Org/resource rejection, current revocation before receipt replay and body-free/no-store errors through outer middleware. |
| T2 — fidelity | Lossless raw+ordinal interpretation; all three families; independent dates, unknown actor/status/outcome, diagnostic exclusion, full canonical variants, invalid IDs/groups/erased parents and exact outcome reconciliation. |
| T3 — effects | Before/after exact native/Person/Today/routing/contact/notes/task/correspondence rowsets and original parent/capture hashes unchanged apart from declared history-read revision. No source/provider/outbound calls. |
| T4 — retry and races | Crash before/after commit, two workers, expired lease, exact replay, changed actor/body, cancelled partial attempt followed by same-plan completion, changed capture/plan refusal, tombstone no-resurrection. |
| T5 — storage/erasure | Byte bound and exact run/Org reconciliation; sibling reservations unaffected, full-budget cancellation, corrupt/missing key pause, display suppression and no reconstructed erased content. |
| T6 — readers | Fence both complete paths before first fact; v2 all native families; 50-row bounds, mixed equal/unknown/backdated dates, cross-scope/stale cursors, partial import and suppression/metadata refresh. Force a commit between family reads and prove one consistent revision/count/row snapshot. No operational call-fold assumptions. |
| T7 — compatibility | Test actual old artifact DB denial for confirmed anchors including cancelled/empty states; prove capability isolation on separate and reused pooled connections, trusted unsupported-artifact retirement and compatible recovery selection. Fresh observed report at required boundaries; source disconnect does not block retained reads/import; ordinary operational read regressions checked. |
| T8 — Web | Actual API populated synthetic preview/confirm/progress/partial/cancel/resume, honest coverage, pagination, stale identity/workspace responses, keyboard and desktop/390px review. No body/provenance leakage. |

Apply D-050: 25,000 People, 50 members, five concurrent Today loads, one active
tab; trust failures always fail closed. One paired regression/plan-shape run on a
realistic historical distribution, including dense Person history and a sparse
family filter; use 010d1's 75,000 observations as a concrete baseline, not a
universal capacity claim or maximum. Measure actual rows/loops/buffers as well as
returned rows; no constant-work claim from LIMIT alone. Run SQLx preparation,
repository service-free and DB gates once on the final implementation tree;
repeat only affected checks for later changes. At most two implementation
review/fix rounds, with one focused planning review for this draft. Keep concise
results and original failures, without turning release packaging into a new audit.
