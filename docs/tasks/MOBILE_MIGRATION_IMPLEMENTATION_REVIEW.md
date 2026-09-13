# Mobile 001 / 010e1 — Bounded implementation review

Completed bounded review, 2026-09-12. Approval is D-074/D-075. This record distinguishes
implementation review from the earlier approved planning reviews and from actual
test execution. The coordinator independently reviews the delegated domain code;
coordinator-authored registration/configuration changes are covered by checks,
not claimed as independently reviewed by that same coordinator.

## Mobile backend — round 1 and targeted recheck

Reviewed typed-command transaction factoring, receipts and replay authorization,
context and generation admission, revision inventory, component pagination,
selection/seal consistency and router errors. Corrections requested:

1. Bound statement execution and total HTTP duration, not only lock waits.
2. Replace quadratic pin membership checks at the 25,000-Person bound.
3. Normalize path/query errors to the frozen JSON error envelope.
4. Convert nullable attribution permission expressions to explicit false.

Targeted source recheck found the 5-second statement and 20-second request
budgets, sorted-pin lookup, JSON rejection handling and COALESCE booleans present.
The implementer reports passing failure/rollback, null-attribution, physical
deletion and 25,000-Person tests. Full gate evidence and performance attribution
are recorded separately before foundation integration. Native persistence,
acknowledgment/promotion and actual app evidence remain platform work.

## Migration 010e1 — round 1 findings

Reviewed the five new retained comparison modules, report route and additive SQL,
including raw qualification, grouping, tuple receipts, authorization, budgets,
lease ownership, output sealing and bounded cursors. Requested targeted fixes:

1. Runtime-role permission for pause-reserve adjustment; preserve cancellation
   when ordinary storage admission is exhausted.
2. An old worker must not pause a different worker's expired lease.
3. Validate lease expiry immediately before terminal publication and bound
   statement/unit execution.
4. Bound identity ciphertext bytes before fetching/decrypting many observations.
5. Always label evidence references representative, per the approved contract.
6. Include the implemented variant aggregation table in the frozen contract.

All six corrections passed targeted recheck and the final five real-role DB/API
tests. Item 1 was also observed as an actual PostgreSQL permission failure before
the narrow grant. Four-process report handoff passed. The alleged inherited core
snapshot checkpoint failure did not reproduce across five independent processes;
no existing collector change was made. See the attributed 010e1 verification record.

## Shared Web integration gate

Coordinator ran lint, type checking, all Web tests and production build in the
migration worktree. One initial lint warning in the coordinator's component
attribute ordering was corrected. The completed run passed 86 files / 1,181
tests, type checking, lint and build. Vite retained its existing large-chunk
advisory. The subsequent actual API desktop/390px walkthrough passed, including an
accepted-request/lost-response replay and authorized retained evidence navigation.
Its source/runtime attribution and the initial harness selector correction are
recorded in the 010e1 evidence directory.

No third broad implementation review is planned. Any second pass is restricted
to these findings and concrete test failures under D-050.

## Native clients — round 1 and targeted rechecks

The coordinator read each platform's database, secure storage, protocol client,
sync orchestration and UI against the approved native brief. Actual native build,
persistence and UI evidence belongs to the respective platform verification docs.
This round identified concrete implementation gaps. Each correction was reviewed
against its original finding; no third broad review was added.

### iOS

- Use a sleep-inclusive monotonic clock for the seven-day lease and generation
  expiry. `ProcessInfo.systemUptime` measures awake time;
  [Apple's continuous clock](https://developer.apple.com/documentation/kernel/1646199-mach_continuous_time)
  also advances during system sleep.
- A failed credential replacement during sign-out must not allow reopening the
  previous unlocked credential after restart; retain an independent durable lock.
- Authoritative reconciliation/read denial must lock revoked access; a per-task
  permission failure requires a current-authority check before classification.
- A revision-conflict completion must offer explicit review of current state and
  a separately chosen new action, preserving its original immutable operation.
- Reclaim obsolete server cache generations/pages/bundles without touching the
  active or staged cache and protected drafts/outbox.
- Keep update-required distinct from a user-controlled sync pause.

Also restore a saved task draft's actual due date in the composer, and show
Today's evaluation time and local pending badges. The implementer accepted these
corrections. All 20 final storage/model tests passed, as did the actual API
100-action retry proof, SwiftUI terminate/relaunch workflow, five post-patch
refreshes and device Release compilation. The separate
[iOS review](MOBILE_001_IOS_REVIEW.md) records targeted source hashes and evidence.

### Android

- After failed/debounced draft saves, block accidental dismissal of uncommitted
  input and use expected draft revisions to reject stale asynchronous writes.
- Persist an independent fail-closed lock so failed encrypted registry/database
  updates cannot restore an account the user signed out of after restart.
- Lock authoritative read/bootstrap/reconciliation403; distinguish per-task
  permission failure through a current-authority check.
- Remove the unbounded whole-Organization People download used for online search;
  use the accepted explicit known-Person pin workflow within the frozen adapter.
- Remove absent server People from the active cache after a complete seal while
  preserving drafts/outbox separately with a visible removal conflict.
- Look up a staged manifest member by its indexed generation/Person key instead
  of loading/scanning all25,000 entries for every component page.

All six Android source corrections passed targeted recheck, including the
additional unavailable-Person draft-read guard. Actual instrumented storage and
account-boundary checks, lost-response retry, native force-stop/relaunch and
emulator reboot proof passed. Final targeted instrumentation passed seven cases,
including reclaimed-generation recovery, mandatory retry pacing and retry-checkpoint
storage failure. Android Debug/test builds, two JVM tests and lint passed (zero
errors, five documented advisories). Platform commands and remaining limits are in
[Android verification](MOBILE_001_ANDROID_VERIFICATION.md); physical-device/cellular
durability is not claimed.

## Repeated-sync failure — bounded correction

Actual concurrent native testing exposed successful generations consuming the
retained-row capacity until expiry. The assigned backend writer added an explicit
sealed timestamp and bounded cleanup of at most two sealed/expired generations
per admission or worker pass. All retained rows continue to count until deletion;
legacy rows are not guessed to be sealed. Pending generations, operation receipts
and business rows are preserved. Ten affected real-database tests passed, including
six cycles on each of two installations and cleanup failure/locking. Native
generation-404 recovery discards staging only and preserves complete cache and
saved work; capacity retry delays cannot be bypassed by repeated manual sync.
See [lifecycle verification](MOBILE_001_SEALED_GENERATION_VERIFICATION.md).

## Verification artifact isolation

The coordinator detected both Web output and executable-path collisions with the
existing shared-development launcher. All released assets and binaries were
restored to exact release hashes; the shared API process was never restarted.
`AGENTS.md` now requires explicit isolated Cargo and Web output directories when
shared launchers use checkout artifacts. This is a documentation correction for
an observed verification side effect, not a deployment or runtime change.
