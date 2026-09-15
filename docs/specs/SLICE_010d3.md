# Slice 010d3 — Historical metadata for admitted People

**IMPLEMENTATION ACCEPTED — D-086, 2026-09-15.** The user accepted both plans
and declared contracts for implementation and isolated synthetic verification.
Independent planning review is in progress before code work. Compatible review
corrections are owned work; materially different policy and publication/deployment
remain separate. [Current implementation status](../tasks/MOBILE_007_010d3_IMPLEMENTATION_STATUS.md)
owns progress and evidence; planning-era wording below does not limit D-086.

**DRAFT — planning authorized, 2026-09-15 (D-085).** Numeric assignment and
contracts are proposals. [Brief](../tasks/SLICE_010d3_IMPL.md),
[paired plan](../plans/MOBILE_007_010d3_PLAN.md).

## 1. Outcome and inherited meaning

An Organization admin previews and confirms first historical event/call/text
metadata coverage for People created by one terminal admission cohort. Import
only qualified records into the exact admitted People, once; preserve visible
coverage, held reasons and an exact recoverable unprocessed remainder.

Keep D-071/072 and [010d2](SLICE_010d2.md) fact/time/privacy meaning: three typed
external imported record kinds, encrypted deletable display metadata, original
raw evidence retained, no native contact credit or Today changes. Chronology is
explicitly **FUB record-created time**, not a guessed call occurrence. Unknown
created dates go to a separate paged Date unknown group; source sent/updated and
local materialization times retain separate labels. No source user becomes the
importing admin, and no source label establishes consent or native call state.

Use one root per terminal completed/cancelled admission. Only committed successful
admission results qualify; each admission remainder is its own cohort. Metadata,
activity and core-refresh completion are not technical prerequisites. Keep the
administrator workspace review hold. No live source access, new downloader, body/
media reader, refresh of already imported history, settled-hold repair, activation,
ongoing synchronization or original-child reopening is included.

## 2. Concrete inspected gaps and shared-contract changes

Current `history_import_worker.rs` authenticates raw captures, computes stable
`timeline-import-identity-v1` HMACs and re-resolves through the original parent's
mapping. `migration_history_import_identity` is globally unique per Org/HMAC but
its owner FK requires an original import run. The three existing fact tables,
display manifests, charging triggers and provenance readers also assume original
plan/run ownership. `history_review.rs` already resolves admitted People through
`activity_review::review_binding`; its owner/provenance paths still need coverage.

| Current | Proposed change / reason | Ownership and compatibility |
|---|---|---|
| Original parent-only history plan/anchor | Separate admitted root/plan/attempt/manifests/results tied to exact successful admission identities and one retained history capture | Migration owns additive schema/domain/HTTP/Web; original anchors, captures and outcomes unchanged |
| Identity and fact provenance only accept original owners | Extend the existing global identity registry and three fact tables with an exclusive admitted-owner tuple | Preserve stable HMAC purpose/input, old rows/FKs and fact meaning; update original and new consumers together, no competing identity table |
| Capture's original-parent links may label a later Person unlinked | Derive admitted linkage from authenticated raw occurrence + exact admission identity in the new plan | Do not rewrite old capture links/projections, use labels as data or fabricate original results |
| Original anchor installs timeline reader boundary | Admitted confirmation durably requires bounded timeline readers and `fub-admitted-history-v1` compatibility | Shared DB guards, original/new workers, preflight/inventory/recovery; zero-write cancellation retains boundary |
| Timeline joins original display/provenance owners | Resolve exclusive original/admitted owner and use the existing Person read revision/counts | Bounded admin review/API/Web; unchanged operational/native/Operator visibility and fact semantics |
| No admitted-history commands | New `/api/migrations/fub/admitted-history-imports` family | Typed retained-only application commands, strict scoped DTOs/receipts and worker admission |

This draft proposes explicit amendments to 010d2 §§2/3/6/7/8 and the admitted
family ladder. Existing source capture 010d1 stays frozen. Freeze exact SQL owner
shape, grants, charge dispatch, cursor/errors and fixtures before implementation.

## 3. Source and cohort qualification

Prepare takes `{request_id, admission_id, history_capture_id}`. Require a terminal
admission with successful committed results and the same original completed
People parent, trusted Org/account and unchanged migration-review binding.
Freeze admission plan/report/source snapshot/final sequence and successful result
IDs. Resolve targets through those results plus account-qualified global Person
identity and live same-Org Person; no name/contact matching, current assignment
inference or importing of newly discovered People.

Select one existing 010d1 `completed_with_gaps` capture from that same original
parent/account. Proposed temporal rule: history capture start must be strictly
later than completion of the newer core snapshot used by that admission. Freeze
capture ID/revision/final sequence/interval, all source/parser/schema versions,
source access user, identity key purpose/version and interpretation version.
This is a conservative first-coverage qualification, not a claim that these
non-atomic snapshots are simultaneous or contain all account history.

No join of records from multiple history captures or “latest wins.” A prior
original history anchor may pin a different capture; this new root does not
change it. The global identity registry arbitrates overlap. Missing qualified
capture remains an explicit prerequisite for separately authorized existing
capture work; Prepare must not start source I/O or demand connected credentials.

Require successful exhaustion/reconciliation of all three existing history streams.
Interpret authenticated accepted advancing raw capture bytes and exact ordinals,
using the current lossless parser and 010d2 interpretation rules. Verify account,
family, representation, chain, sequence, source IDs, AEAD/HMAC and bounds. Diagnostic,
truncated, identity-only or rejected pages are not executable input. Integrity/key/
profile failure pauses preparation; it is not an individual record to silently skip.

Index the selected evidence once in bounded units across the cohort. Include every
occurrence when comparing a source identity, including occurrences referring to
other People. Conflicting complete canonical variants or ambiguous/group/nested
relationships hold the entire identity. Equal repeats collapse with provenance;
no cross-record field filling, participant fan-out or phone-based linkage. A valid
single source Person ID can be newly linked via the admission even when 010d1's
unchanged original-parent projection says unlinked. Missing/erased/tombstoned or
mismatched targets are held. Out-of-cohort records remain excluded coverage;
invalid-ID occurrences remain individually countable. A source 404 or absence
never instructs deletion.

## 4. Identity, facts and atomic application

Reuse `migration_history_import_identity` and exactly the existing stable key:
Org plus HMAC of `(account, family, representation, source record ID)` under
`timeline-import-identity-v1`. Canonical HMAC covers the entire lossless record
with the existing account/family/representation purpose. Capture UUID remains
part of capture-level evidence only, not global identity.

Proposed schema extension: original ownership columns become conditionally
required alongside nullable admitted root/plan/attempt/manifest references. A
CHECK requires exactly one complete owner shape, with composite tenant FKs for
both. Existing original column values, IDs and FKs remain intact; never repoint
an existing registry owner. Apply equivalent exclusive provenance to the existing
three typed fact tables and bounded display references. Do not update/delete old
immutable fact contents or synthesize original runs to satisfy FKs. Keep global
uniqueness and append-only enforcement. Review the exact grant/trigger change
before coding; this is a schema contract, not permission for arbitrary fact writes.

An existing live identical identity/canonical record on the same Person/family
is already-present after verifying its fact and display/provenance integrity.
Different target/canonical variant, erased identity, missing formerly materialized
fact or incompatible ownership is held. A surviving deletion marker wins over
replay. Never UPDATE native truth, move history to another Person, adopt ownership
or reconstruct erased display content. Existing original workers must explicitly
handle admitted ownership without joining it to an original-only run.

One new private typed permit binds current admin, Org/workspace, root/plan/attempt,
successful admission item/result, manifest, exact Person/fact/identity, budget and
unexpired fenced lease. It authorizes only the planned history inserts/results;
`origin=migration` is not authority. Lease/authority/dependencies are rechecked at
commit. Identity resolution, fact/display/provenance, result, read revision/count,
checkpoint and reservation settlement commit atomically. Concurrent original/new
imports use the same Org/identity serialization. No network or inference occurs.

## 5. Plan lifetime and accounting

States: preparing → ready → queued → running → completed; recoverable failure
pauses the owned phase. Ready plans expire after the inherited ten minutes before
first confirmation. Before confirmation, cancel/reprepare can choose a different
qualified capture. Confirmation freezes all source/cohort/interpretation bytes,
requires exact plan/workspace/budget revisions, current admin, compatible release
readiness, counted hold/coverage acknowledgement and at least one eligible or
verified already-present unit. An all-held preview cannot claim useful import.

Resume explicitly adopts the current authorized admin as executor; no automatic
resume when capacity/keys return. Authorized exact request/body replay occurs
before fresh expiry checks. Changed input with the same request ID conflicts.
Cancellation serializes with unit commit and retains committed facts; late leases
cannot write. A confirmed cancelled attempt can have only one successor over its
exact never-settled units. Same source/cohort/interpretation/target IDs; settled
holds/applied/already-present results stay settled. One active attempt per root.
The original import's immutable anchor and retry semantics remain unchanged.

Report unique planned identity units separately from repeated occurrences,
invalid occurrences, excluded out-of-cohort records and nonexclusive coverage
warnings. Across the chain, planned = imported + already-present + held +
never-settled. Every raw occurrence has an inspectable outcome; equal repeats
never inflate inserted totals. Remainder carries original coverage and counts.

Reuse 010d2's independent history-derived run/Org accounting, operator ceilings,
initial min(2 GiB, configured run ceiling), reserved cancellation capacity and
bounded 50-record units / one bounded raw capture walk. Measure and freeze the
new owner's complete variable-byte inventory; inherited 32-KiB per-record bound
must cover new fields or be explicitly revised before code. Raw evidence remains
charged to its capture, not copied/recharged. Each measured row dispatches to its
exclusive original/admitted owner; old owner charges remain unchanged. Account
for control, receipts, discarded preparations, remainder and erasure exactly.
No new customer quota, retention period, key system or storage product is chosen.

## 6. Bounded review, compatibility and privacy

Keep existing admin Person review v2, known/unknown date grouping, family filters,
source labels and metadata-only details. Preserve actual current native family
inventory, including `person_admitted`; do not regress to 010d2's historical list.
Notes/tasks stay separately paged. No body, subject, endpoint phone, raw payload,
URL preview or automatic AI summary is exposed. Source actor roles remain distinct;
unresolved references need no new local identity or cross-capture user-label join.

Use existing 25/default, 50/max pages, <=51 candidates per family, 4-KiB summaries,
16-KiB metadata details and 512-KiB responses. Queries resolve the exclusive owner
without duplicate joins. Existing Person history revision/counts must advance
atomically for new facts and display suppression; cursor scope remains endpoint,
Org/Person/workspace/revision/filter/size/total-order key. Use one consistent DB
snapshot for counts, revision and all family rows; stale traversal requires Refresh.
No full-history sorting/decryption/count scan followed by client pagination.

First confirmation takes the exclusive workspace barrier and installs a durable
admitted-history predicate before any fact commit. Extend the common DB read guard,
legacy complete-reader rejection, typed permits, worker admission and release
preflight with `fub-admitted-history-v1`. Compatibility stamp is server-owned and
transaction-local, never a business permission or caller header. Cover existing
original workers and actual pooled connections. Confirmed cancelled/erased/zero-write
roots remain fenced. Trusted inventory retires unsupported artifacts, including
ones older than the common guard. Forward recovery preserves anchors/owners/data;
never remove a binding to start an old binary.

Fresh explicit confirmation/Resume and new/recovery worker admission require
current observed release evidence as in 010d2. Already admitted bounded work does
not lose authority solely because that report ages; normal lease/actor/binding
fences still apply. This plan does not renew the shared-development report or
perform a release. Mobile search must deny every held migration workspace.

Extend erasure inventory with admitted manifests/displays/identity/provenance,
results/remainders and caches. Synthetic suppression must prevent resurrection.
Shared raw-page erasure and customer key/backup/retention policy remain D-015 and
O-012/O-013/O-015 readiness work, not solved by calling this metadata-only.

## 7. Commands and Web

Separate strict scoped routes under `/api/migrations/fub/admitted-history-imports`:
Prepare as §3; Confirm `{request_id,plan_id,expected_revision,acknowledge_held,
acknowledge_coverage}`; Resume/Cancel `{request_id,expected_revision}`;
Remainder `{request_id,attempt_id,expected_revision}`; monotonic Budget increase
with expected revision followed by separate Resume. Read root/list/detail and
paged manifests/results/issues. No raw-field/body route is added.

Inherit 010d2's <=8-KiB bodies, <=4-KiB receipts, decimal vendor IDs/counts/revisions,
strict unknown-field rejection, no-store responses and safe 401/403/404/400/409/503
error distinction. UUID resource IDs remain UUIDs; vendor numeric IDs are decimal
strings. Freeze DTO fixtures and cursor purpose at the owned contract checkpoint.

Web shows admission cohort, selected capture/interval/source access, known source
gaps, unique/occurrence counts, dated/undated and held/excluded outcomes, then
explicit confirmation. Explain that cancellation retains already imported facts.
Resume and exact remainder are distinct actions; unknown responses reuse request
identities. Stop polling on terminal/paused states; refetch on reconnect/focus.
Clear state on actor/Org/workspace/root/Person change and ignore late responses.
Use existing product styling and desktop/390px layouts; cream applies to the
planning artifact, not a CRM redesign.

## 8. Acceptance

| ID | Required proof |
|---|---|
| H3-01 | Completed/cancelled admission successes, separate remainder cohorts, same parent/account, later history interval and exact immutable capture qualification; no cross-capture joins or source requests |
| H3-02 | All families, originally unlinked later Person, same-ID equal repeats/conflicting variants, invalid IDs/groups/out-of-cohort/erased targets; exact occurrence and unique-unit reconciliation |
| H3-03 | Existing original/admitted global identity overlap, owner-shape FKs, tombstones, original-first and admitted-first races, no owner adoption/native updates/resurrection |
| H3-04 | Exact old native/Person/Today/contact/notes/tasks and capture/anchor preservation; source record-created/unknown/sent/updated and executor clocks remain distinct |
| H3-05 | Crash/lost response, current auth before replay, changed body, cancellation/late lease, explicit executor takeover and exact single remainder; no duplicated facts or settled holds |
| H3-06 | Complete byte/owner reconciliation, budget/full-budget cancel, corrupt/key pause, sibling reservations unaffected, suppression and immutable fact protections |
| H3-07 | Correct admitted provenance and actual native families, consistent revision/count/pages, dense history/ties/unknown/backdated/suppressed records and stale/cross-scope cursors |
| H3-08 | Actual old-reader/worker rejection at zero-write confirmation/cancellation; capability isolation/reused connections, observed release readiness and compatible recovery |
| H3-09 | Current admin versus member/platform/foreign/revoked/held ordinary mobile access; metadata-only/no-store including outer errors; no PII in logs |
| H3-10 | Real populated synthetic API/Web preview/confirm/progress/hold/cancel/remainder at desktop/390px, keyboard states and honest coverage; paired regression and bounded realistic query plans |

Use D-050 review/performance limits and the paired final gate sequence. This draft
has author inspection only; implementation tests and independent review are pending.
