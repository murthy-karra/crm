# Slice 010c implementation review

2026-09-11; worktree `codex/slice-010c-people-import`, base `c6c5930`.
This records implementation review, separately from the approved plan review.

## Round 1 — backend

Independent read-only reviewer: `backend_review`. Verdict: **fixes required**.
The reviewer verified all 147 source hashes in [the frozen checkpoint](BACKEND_CHECKPOINT.md),
its SHA256 and all six supplied verification-log hashes. No reviewer edits,
Cargo, database, runtime or network operations occurred. Supplied passing tests
are author-executed evidence, not reviewer-executed tests.

All five findings are P2, within the approved operating envelope:

| ID / tag | Failure | Required correction | Disposition |
|---|---|---|---|
| I010-R1-1 TRUST | Supporting stage labels/user emails containing NUL reach PostgreSQL text suggestion queries and pause the whole plan | Skip unrepresentable native-text suggestions; preserve raw evidence and valid explicit mappings; prove unaffected People continue | Fixed; targeted READY |
| I010-R1-2 CONTRACT | HTTP release readiness precheck rejects an already committed confirmation replay after evidence expires/disappears | Current authority and durable receipt resolution before readiness for a new confirmation; exact replay, altered-input conflict and new-confirmation rejection tests | Fixed; targeted READY |
| I010-R1-3 BOUNDARY | Shared import transactions bound pool/workspace waits but not subsequent membership/Organization row locks | Set transaction-local metadata lock bound before acquisition, including initial worker claim; controlled contention returns closed error and releases resources | Fixed; targeted READY |
| I010-R1-4 CONTRACT | Cancel response and stored replay snapshot counters before reservation/receipt settlement | Return and persist settled counters; compare response/replay to GET and committed ledger | Fixed; targeted READY |
| I010-R1-5 CONTRACT | Abbreviated supporting mapping fields have no full-field accessor; record accessor only accepts manifest IDs | Add mapping-scoped bounded field route with exact retained evidence, scoped authority and distinct cursor; long UTF-8/foreign-ID/cursor regressions | Fixed; targeted READY |

The fifth finding was confirmed in a targeted continuation after Web preparation
identified the interface gap; it is part of round 1. The additive endpoint is
documented in the owned specification/contract under D-065. No policy or source
profile changes are implied. Targeted correction confirmation returned **READY
for Web implementation**. All five findings are resolved, with no remaining
blocker or introduced defect found in this targeted review.

Root and reviewer independently verified the
[correction checkpoint](BACKEND_R1_CORRECTIONS.md): eight replacement plus 139
unchanged source hashes (147 total), three correction log hashes and checkpoint
SHA256 `9e9fd19f5d3abf2333b74124d4611d8cfb69f86d56526321f7f7d212aae93a49`.
The author-run suite passed 27/27 in 116.47s; scoped Clippy passed in 18.96s and
formatting passed. Reviewer-run whitespace validation passed. No reviewer builds
or database operations occurred. This closes correction verification within
round 1; no third review round was introduced.

Reviewer commands: Git status/branch/base/stat and scoped diff, `git diff --check`
(passed), checkpoint SHA256 plus read-only Python file/log hash verification, and
`rg`/`sed`/`nl` source/contract inspection. See the checkpoint for exact frozen
sources and author-run commands/results.

## Round 2 — integrated backend and Web (complete)

The independent reviewer was reactivated after the [integrated checkpoint](R2_CHECKPOINT.md)
was frozen. Root independently verified its 207 source hashes and eight log
hashes, checkpoint SHA256 `7a35c6fd7c06a3443632a5c98342f4f7b31e7216b0c5cb342b46de7a1cf52b2a`
and manifest SHA256 `f35f3d5e6487debb107776898c59b4b1d801260d8a25f8cc9478d3be8b37773f`.
Review covers the integrated implementation against c6c5930, including current
workspace authority, role/late-response fences, confirmation recovery, import
controls and the post-round-1 helper registrations. No edits or runtime checks
were performed by the reviewer. Verdict: **three corrections required**, all P2
BOUNDARY findings within the approved operating envelope:

| ID | Failure | Required correction | Disposition |
|---|---|---|---|
| I010-R2-1 | Dropdowns show unapplied mapping drafts while confirmation can submit the previous frozen plan | Propagate dirty state and require Apply or explicit discard before confirmation; real-child integrated regression | Fixed; targeted READY |
| I010-R2-2 | Ordinary routes remount while workspace verification is pending; slow successful `/me` leaves their queries failed after retries | Defer ordinary mounting/query enablement through successful authority verification, preserving intended import recovery state | Fixed; targeted READY |
| I010-R2-3 | Visible results retain Pending rows after parent polling reports completion | Refresh visible result pages on committed progress and terminal transitions using the existing parent poll; integrated progression regression | Fixed; targeted READY |

The reviewer independently matched 207/207 source hashes, 8/8 log hashes and
both checkpoint hashes; reviewer whitespace validation passed. Findings came
from source/dependency tracing, without runtime reproduction. No additional
production-backend defect was found. Targeted correction verification remains
within this second/final review round.

Final verification followed review: the sequential gates, comprehensive plans
and single paired performance run passed, as recorded in the
[verification log](../../../tasks/SLICE_010c_VERIFICATION.md). Browser-discovered
corrections and their final checks are recorded below. No release or customer-data
authorization follows from review.

Targeted correction confirmation returned **READY for final validation**. Root
and reviewer independently verified all 208 current source hashes, seven
correction logs and both [correction checkpoint](R2_CORRECTIONS.md) hashes.
The 143 backend entries and all eight original logs remain unchanged. The
69 authority/route tests and 39 import tests pass, along with current TypeScript
and scoped lint. The delayed-verification regression failed before the fix and
passes afterward. Reviewer whitespace validation passed. No introduced defect
was found; all three findings are resolved within round 2. No third round or
reviewer build/database operation occurred.

## Corrections found by executed query plans

After both full review rounds, actual D-050 plans identified a claim predicate
that hid a partial index and result pagination that scanned the whole import.
The reviewer separately confirmed the bounded corrections by exact source/query
hashes and rollback-only plan diagnostics. The claim predicate is equivalent;
the nonunique result index and lateral lookup preserve all matching rows,
unmatched manifests, tenant/filter/order/bind/cursor semantics. Grants and
constraints are unchanged. Constant `OFFSET 0` preserves the lookup boundary;
it is not offset pagination. No reviewer build or DB operation occurred.

The test collector also corrected fixture-specific user and filtered-row bounds,
retaining index/no-spill checks and recording empty-filter tail work separately.
Its complete execution now passes 59 plans/258 checks. These are targeted
verification corrections, not another full implementation review. The three
sequential scripts and single paired HTTP reader regression subsequently passed.

## Corrections found by the executed browser walkthrough

The production Web walkthrough exposed an unnamed confirmation dialog (its
custom visible header lacked accessible-name wiring), stale Migration page copy,
and a held member's migration GET returning generic workspace 409 before its
contracted admin-only 403. Access stayed blocked. The fixes connect the real
PrimeVue root to its visible heading, accurately describe explicit import, and
apply the existing admin authorization to matched migration/provenance read
routes before generic member review denial. Authorized reads retain current
database authority and the complete shared response permit. Ordinary People and
Today paths are unchanged.

Independent targeted confirmation returned **READY for final validation** after
matching all five supplied source hashes and inspecting the exact corrections.
The HTTP regression checks eight held-member migration/provenance 403 responses,
alongside admin success and ordinary member People 409. Three real PrimeVue
regressions check import/budget/cancellation dialog names. The reviewer also read
the refreshed repository log: 877 Rust and 1,000 Web tests passed, all checks
passed in 64s. The final DB run was active at that review checkpoint; its eventual
result and complete browser evidence belong to the verification record.

No edits, builds, runtime or DB work were performed by the reviewer. This was
bounded correction confirmation, not a third full implementation review.

The subsequent delayed-response walkthrough found a further Web defect: a
role change within the same cookie generation did not trigger a new authority
read on focus. A standalone trace and red regression reproduced it. AppShell
now refreshes workspace authority on visible app-return events, using the
existing synchronous request/cache fence and coalesced verification. It excludes
descendant input focus and hidden/public/unsettled sessions, removes its listeners
on unmount, and preserves session generation/lifetime and pending confirmations.

Independent targeted confirmation matched both supplied hashes, reversed the
patch to the previous AppShell bytes and returned **READY**. No actionable defect
was found. The reviewer inspected the red trace/test, 53 passing focused tests
and final repository log (877 Rust, 1,002 Web tests; all checks passed in 31s).
Root then passed the standalone real-browser demotion/late-response check.
This remains bounded verification of a browser-found correction; no additional
full review, reviewer edit/build/runtime or database operation occurred.
