# Slice 010e4 — Refresh People admitted after the original import

**APPROVED FOR IMPLEMENTATION — D-080, 2026-09-13.** The user accepted the
first sequential slice: core refresh of successful 010e3 admissions, its declared
contracts and isolated synthetic implementation. Proposal wording below records
the accepted design; subsequent families and release remain separate. [Brief](../tasks/SLICE_010e4_IMPL.md),
[coordinated plan](../plans/MOBILE_004_010e4_PARALLEL_LAUNCH.md),
[planning review](../tasks/MOBILE_004_010e4_PLANNING_REVIEW.md).

Read AGENTS §13, D-015/D-050/D-059/D-064–079 and relevant O-012/O-013/O-015,
[010e2](SLICE_010e2.md) and its [contract](../tasks/SLICE_010e2_CONTRACT.md),
[010e3](SLICE_010e3.md) and its [contract](../tasks/SLICE_010e3_CONTRACT.md).
The [family sequence](../plans/SLICE_010_ADMITTED_PEOPLE_LADDER.md) retains all
remaining fidelity work. Source reads, real customer data and activation stay deferred.

## 1. Outcome and boundary

An admin chooses one terminal admission run and a later sealed 010e1 report,
previews updates to the People successfully committed by that admission, then
confirms the exact plan. Original records and locally changed values remain
protected. The workspace stays bound to its original completed 010c parent in
`migration_review`.

Refresh names, complete qualified email/phone collections and primary order,
stage and assignee. Reuse original approved mappings and the conservative 010e2
whole-Person conflict/clear rules. No new Person, mapping, stage/member, identity
adoption, merge, source deletion, force overwrite or resurrection.

Tags/custom fields, notes/tasks and history extensions are subsequent slices,
not hidden acceptance requirements of this one. Show those gaps on admitted
Person review and refresh results. Existing 010c/010e2 and family-import success
shapes/semantics remain unchanged; no fabricated original-import result.

## 2. Declared shared-contract changes

| Current | Proposed change and reason | Components / compatibility and amendment |
|---|---|---|
| 010e2 seeds only from original results; its baseline has an original-result FK | Separate admitted-People refresh domain/state, explicitly anchored to a successful admission result | New Rust/HTTP/DB/Web capability; keep old baseline/table constraints and DTOs unchanged; extend 010e3's following-work boundary through this spec |
| Admission is INSERT-only with immutable provenance | Derive an exact initial refresh baseline from admission projection and contact rows; later baseline from atomic new refresh results | New owned baseline/result stores; original admission/result/provenance and global identity registry remain immutable |
| No writer can refresh an admitted Person under review hold | Private item/lease-scoped admitted-refresh permit | Workspace guard and grants; only exact Person UPDATE, owned contact operations and migration-refresh stage/assignment fact INSERT; old tokens acquire no privileges |
| Preflight lacks admitted refresh | Independent `fub-admitted-people-refresh-v1` reader/writer/recovery capability | API/worker/admin/migrator launch checks, operator inventory/report and Web; incompatible executables must fail closed before work is confirmed/recovered |
| Existing refresh UI targets original People | Separate bounded admitted-refresh preview/results with admission origin | New endpoints and panel; do not silently broaden old routes or mislabel an admitted record as original |

A dedicated store avoids weakening 010e2's original-only foreign keys. Reuse its
lossless interpreter, contact diff and mapping checks through small explicit
helpers where justified; do not copy a second unrestricted mutation implementation
or build a generic multi-source migration framework. On acceptance the lane owns
these concrete contracts, with DTO/schema checkpoints before Web implementation.

## 3. Cohort and qualified source

A cohort is the immutable set of successful `settled` results from **one** 010e3
admission run, terminal `completed` or `cancelled`, with no live writer. Cancelled
runs can contain valid committed People; include those results, never unsettled
items. A later admission/remainder is a separate cohort. No combined source book
or mutable query selection is accepted as the cohort identity.

Require matching trusted Org, source account, original parent/confirmed plan,
workspace revision, admission/item/result, global `people` identity and live
same-Org target. Freeze the result set by terminal run plus bounded manifest/digest.
An existing identity or provenance disagreement is a hold; corrupt/missing keys
pause preparation. Erased targets/tombstones are not recreated. Shared contacts
never establish Person identity.

The report must be completed/sealed and belong to that same original parent/account.
Freeze output revision, original/newer snapshot IDs/final sequences, capture
intervals and parser/engine versions. Its newer capture must begin strictly after
the admission source capture completed and after the latest confirmed refresh
source interval for this cohort. Compare source interval boundaries, never a
record's `updated` timestamp or source-ID numeric order as chronology.

010e1 compares against the original snapshot, so an admitted Person can remain
classified as `new`. Walk **every successful cohort identity**, independently
lookup all its exact retained newer observations, and account for missing IDs;
report disposition labels cannot choose the mutation candidates. Do not require
a newly admitted Person to appear in original `migration_import_result`.

Use the existing qualified raw parser/HMAC/AEAD and full representation rules.
Require completed relevant People/users/stages evidence and complete contact
collections for removal. Missing fields are `no_instruction`; missing People are
`not_seen_again`, not deletion. Ambiguous observations, new mapping keys and invalid
mapping targets hold the entire Person. Revalidate source-account and original
mapping evidence on prepare and commit, including name/email target snapshots.
No source calls, credentials or FUB write capability exist in this worker.

## 4. Baselines, local changes and settlement

Initial B is reconstructed from the successful admission item's encrypted native
projection, exact `migration_people_admission_contact` rows (their UUIDs are the
committed contact UUIDs), qualified mapping references and immutable result/
identity/provenance. Never initialize B from today's Person or contacts. If the
bound evidence cannot prove every owned field/contact, hold instead of adopting
current values. Referenced retained evidence must remain available for recovery.

Later B comes only from this feature's settled immutable results and owned
baseline pointer. N is the qualified source instruction overlaid on B using
010e2's missing-versus-explicit-clear rules; C is locked native state.

| Condition | Disposition |
|---|---|
| C = B and N differs | Eligible atomic update, with all clears/removals previewed |
| C = B and N = B with exact contact ownership | Already current; no business fact |
| C differs from B, including C = N, or any contact is unowned/missing/replaced | Hold the whole Person; no adoption, deletion or partial update |
| Missing/erased target, inconsistent identity/mapping or unsupported evidence | Closed held/excluded reason; preserve source and no baseline advancement |

Unrelated note/task/tag/history changes do not affect equality. As in 010e2,
current equality cannot prove that a core value was never edited and reverted;
do not reinterpret broad mobile revisions as that history. Mobile 004's separate
stage revision is also not a migration source baseline or force-override token.

Under shared parent serialization, then owned run/baseline/Person/contact locks,
recheck current admin, review binding, source boundary, manifest, lease, origin,
baseline pointer/version, previewed native fingerprint and mapping eligibility.
Any intervening relevant change produces `held_stale` with no mutation.

One bounded transaction updates all allowed Person/contact fields, appends the
new encrypted before/after provenance and immutable settlement, updates baseline/
contact ownership, advances progress and settles reservation bytes. Stage and
assignment transitions use existing migration-refresh reasons, System/Migration
attribution, initiating admin and server business time. No Inquiry/contact credit,
new original-source attribution or operational activation/notification is created.
Keep all older People, admission facts, original family children and baselines intact.

## 5. Lifetime, source ordering and retained bytes

Reuse refresh states: preparing, ready, queued, running, paused, completed,
cancelled. Immutable ready plans expire in ten minutes; re-preview makes a new
revision. Require at least one eligible business update at confirmation; all-held/
all-current previews remain inspectable without queuing a write.

Only one active admitted refresh per admission cohort. Serialize cross-feature
critical transactions through the existing parent lock, not a second unrelated
advisory namespace. Distinct cohorts retain independent source watermarks; 010e2's
original-People watermark and 010e3's admission watermark remain unchanged.

A first confirmation advances the cohort's immutable confirmed source boundary.
Older/overlapping captures cannot confirm. A cancelled/completed attempt may be
followed by an explicitly previewed remainder on the **exact latest confirmed
report/snapshot/sequence/interpretation**; it recomputes against current settled B
and revalidates all evidence. It cannot select an older boundary after a successor
confirms. This explicit same-boundary exception prevents cancellation from
stranding unsettled People; it does not revise the original 010e2 contract.

Initiator alone confirms/resumes; any current Org admin reads/cancels. Revocation
pauses work. Another admin may cancel and prepare their own plan; no automatic
executor adoption. Cancel retains committed data and baselines and fences old
leases. Exact request replay is actor/Org/action/body-bound; changed input conflicts.

Use the shared snapshot/Org byte ledger with separately owned reservations,
including a cancellation allowance. Charge each new encrypted plan/item/contact/
result/baseline/request receipt envelope and stored variable key/checkpoint once;
shared raw/admission evidence keeps its existing charge. Transfer baseline byte
ownership before replacing its pointer, as in 010e2; never debit another run or
release another lease's reservation. Freeze exact new table/column accounting in
the contract and independently reconcile measured stored bytes in tests. Raw read
units stay at 16 MiB and total retained work units at 64 MiB; page the cohort.
Extend the C1/C2 erasure inventory for the new stores without selecting retention
or claiming per-Person crypto-shred with the development key.

## 6. Bounded HTTP, Web and compatibility

Proposed base `/api/migrations/fub/admitted-people-refreshes`. Current admin,
trusted active Org, CSRF for mutations, no-store for all responses; unknown request
keys and foreign resources use existing closed validation/disclosure conventions.

| Route | Contract |
|---|---|
| POST / | `{request_id, admission_id, report_id}`; server resolves/fences all trusted bindings |
| GET / | Required admission_id; cursor, limit ≤20 |
| GET /{id} | Origin, source intervals, lifecycle, counts, actions and plan identity |
| GET /{id}/items and /results | Keyset cursor, limit ≤50; explicit missing/held/already-current/settled counts |
| GET /{id}/items/{item_id} | Bounded before/current/proposed scalar prefixes and contact counts |
| GET /{id}/items/{item_id}/contacts | Endpoint-bound pages of owned contact identity/order and proposed changes |
| GET /{id}/items/{item_id}/fields/{side}/{field} | Authorized exact scalar fragments, never executable display prefixes |
| POST /{id}/plans | request_id + expected_plan_revision |
| POST /{id}/confirm | request_id, exact plan ID/revision/digest, eligible count, coverage/exclusion/review-hold acknowledgements and exact independent clear/removal counts |
| POST /{id}/retry or /cancel | request_id + expected_lifecycle_revision |

Match existing 128-KiB resource/256-KiB item-contact-result response bounds and
16-KiB UTF-8 field fragments. Bind authenticated cursors to actor/Org/resource/
endpoint/plan/revision/filter/limit and a frozen traversal upper key. Sparse pages
must bound rows examined before decrypting. Worker preparation/claim uses indexed
cohort keys; never rescan every original Person per candidate.

Web lets admins select terminal admissions with successful results, then a later
qualified report, inspect complete coverage and clears, confirm and recover/reload.
Use explicit origin labels and server-returned availability. Add the feature to
admission review; keep old original-People refresh and family buttons unchanged.
No field prefix or browser-generated ID is executed as source evidence.

New capability gates cover readers, writers, launch and recovery. Old binaries
must be rejected before serving/recovering a workspace confirmed under this
capability; a UI-only guard or unknown-column tolerance is insufficient. Require
fresh actual-workload compatibility evidence before confirmation. Preserve all
existing admission/refresh/activity/history reader fences and token limits.

## 7. Acceptance and completion

- Completed and partially cancelled cohorts; zero-success cohort rejection;
  untouched original/unsettled/sibling People; duplicate-contact distinct identity.
- Admission B reconstructed exactly; C≠B held even at C=N; owned contacts removed/
  replaced/unowned; explicit clears versus missing data; source return to initial
  values after a refresh; all-current/all-held previews cannot queue writes.
- Wrong Org/account/parent/report/admission/result/target; tombstones; raw corruption;
  changed mapping targets; report `new` labels; missing newer Person; no resurrection.
- Source interval ordering, overlapping/older rejection, independent cohort and
  original/admission watermarks, cancellation and exact-boundary remainder, stale
  lease/authority/capacity recovery and replay after lost response.
- One-transaction business/result/baseline/byte settlement; rollback/fault injection;
  exact retained-byte audit and preservation of original/admission/family records.
- Permit abuse: old original/refresh/admission tokens cannot authorize the new
  operation, and the new permit cannot touch another item/Person, notes/tasks,
  catalog, Inquiry/contact facts or ordinary mobile writes in a review workspace.
- Real API/production-Web desktop and 390px preview→confirm→partial cancel→remainder→
  result/provenance→reload, with exact native row reconciliation and no console errors.
- One D-050 realistic 25k-Person query-plan pass (sparse pages and worker claims),
  relevant paired Person read, old 010e2/010e3 contracts and combined mobile-stage
  trigger/permission regressions. Required final repository/SQLx/DB gates once.

Maximum two review/fix rounds. Implementation completion does not imply live
qualification, all-family migration fidelity, activation, deployment or restore proof.
