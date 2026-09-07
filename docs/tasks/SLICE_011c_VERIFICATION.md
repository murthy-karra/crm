# Slice 011c — Verification evidence

Status: **COMPLETE. Implemented and verified on the final tree (2026-09-06,
late evening, Claude takeover); the user then approved the §8 planner
amendment, accepted the Phase B pairing limitation and approved the local
commit and merge (no push or deployment).** Every claim below names its
runner and actual result.
The user approved the complete specification and brief on 2026-09-06 after
independent READY review. Implementation, tests and fixes are authorized;
commit, merge, push and deployment are outside the current scope.

## Target and runners

- Branch: `codex/slice-011c-today-sources`, based on local `main` at
  `9d62e8692fa52d97b206f45870c3fac0972738ad`.
  Implementation commit `6117b4a` (2026-09-06), merged to `main` as `929b6ab`
  and pushed to `origin/main` the same evening at the user's request.
- Primary implementation, tests and serialized database operations:
  `/root/slice_011c_implement`, `gpt-5.6-terra` / `xhigh`.
- Independent review: `/root/slice_011c_review`, `gpt-6-astra` / `xhigh`.
  The reviewer is read-only; it does not run concurrent database suites.
  Its support audit completed. After a tool thread-limit error prevented
  resuming that task, `/root/slice_011c_spec` (also `gpt-6-astra` / `xhigh`)
  took the independent code review role after its specification handoff.
- The coordinator owns shared documentation, amendment pointers and this
  evidence record. Production changes and generated SQLx metadata belong to
  the primary implementation lane.

**Takeover (2026-09-06, evening).** Codex usage ran out with the tree
uncommitted. From then on Claude Fable 5.1 is the coordinator, the sole gate
and database runner and the acceptance reviewer; Claude Sonnet 5 lanes A, A2
and W implemented the bounded corrections listed under
[the implementation brief's takeover section](SLICE_011c_IMPL.md#takeover-on-2026-09-06-evening-codex-usage-exhausted);
a read-only reviewer subagent produced the Web session-privacy review. The
Astra/Terra attributions above describe the earlier phases and remain
accurate for them.

## Completed planning evidence

The [specification review](SLICE_011c_SPEC_REVIEW.md) closed all three design
findings before approval. The [Phase A archive](../design/perf/slice-011c-2026-09-06/README.md)
records the isolated SQL feasibility experiment, including earlier failures,
522 complete candidate requests and original/candidate payload-order parity.
Its synthetic database and processes were removed. Those SQL results do not
establish application HTTP latency, browser behavior or final acceptance.

The reviewer completed a read-only consumer audit at implementation start.
It highlighted session-lifetime fences beyond query keys, all four Operator
Today consumers, untrusted list-name serialization, nullable rendering, test
registration, cancellation cleanup and rejecting partial 2xx benchmark results.
These are verification obligations, not passed implementation checks.

## Acceptance coverage

Every numbered criterion in [the approved specification §9](../specs/SLICE_011c.md#9-acceptance-criteria-and-verification)
must be mapped to actual final-tree evidence before completion. Pending rows
must not be interpreted as a test result.

| Criterion | Status / required evidence |
|---|---|
| 1. Schema and command boundary | Met. Command, grant, composite-FK and migration tests in the database gate (423 of 423 on the final tree); SQLx prepare and prepare-check clean. |
| 2. Five-source concurrency | Met. Contention, duplicate/stale enable, opposing toggles, delete race and tombstone-quota tests in the final gate. |
| 3. Privacy and tenant isolation | Met. Direct/HTTP member/admin/private/foreign cases and no-publisher-event assertions in the final gate; live two-Organization checks (Codex walkthrough). |
| 4. Filter parity | Met. Five common-clock parity tests in the final gate. |
| 5. Cap and complete reasons | Met. Bounded queue tests in the final gate. |
| 6. Built-in regression | Met. Four frozen-original comparisons in the final gate, re-run by lane P after the planner change; Phase B paired zero-source parity exact for all four books in run 1 and three of four in run 2 (see criterion 12). |
| 7. List ordering and actions | Met. Queue/HTTP/Operator tests in the final gate; live no-Inquiry and contact-label cases. |
| 8. Snapshot and failure | Met. Hooks, settings, deadline and controlled-failure tests in the final gate; settings restoration now also covers the proposed planner setting. |
| 9. Web controls and states | Met. 446 Vitest tests in 37 files in the final gate; live controls, recovery, narrow and keyboard checks; live blocked-session recovery. |
| 10. Private cache and recovery | Met with corrections I17–I20; ten added tests; live reset check. Three reviewer-listed scenarios remain untested (recorded in PROJECT_STATE), none a known defect. |
| 11. Operator parity | Met. Source-enabled HTTP/Operator tests and the cancellation test in the final gate; untrusted-name wrapping reviewed. |
| 12. Telemetry and performance | Telemetry: trace-sentinel test registered and passing. Performance: final arm meets every §8 limit in Phase B run 2 (built with the proposed planner change); run 1 failed on the pre-existing planner hazard and is retained. The concentrated book's HTTP pairing is established in run 1 only; run 2's baseline series for it was incomplete. The user approved the amendment and accepted that limitation on 2026-09-06. |
| 13. Final gates | Met on the final tree, run once by the coordinator: `./scripts/sqlx-prepare` (no cache change), `./scripts/check` (24 s), `./scripts/check-db` (423 of 423, 146 s). Independent acceptance review: coordinator plus the read-only Web reviewer. |
| 14. Live walkthrough | Met (Codex walkthrough plus the takeover's reset check). Runtime and generated databases cleaned up. |

## Automated checks

The [retained check archive](../design/qa/slice-011c-2026-09-06/checks/README.md)
contains the available intermediate source/baseline/failure and focused Web
outputs described below, including failures and interrupted runs. Its manifest
distinguishes raw copies, one summarized database-debug line and the previously
reconstructed initial command/result summary. Archival did not rerun commands.

The bounded test helper reports the following early Web checkpoint:

| Runner / command | Actual result / scope |
|---|---|
| `today_source_tests`: `pnpm --dir web exec vitest run src/api/todaySources.test.ts` | Passed, 7 targeted cache/recovery/mutation/session-isolation tests. This does not establish final-tree or full-suite acceptance. |
| `today_source_tests`: `pnpm --dir web exec eslint src/api/todaySources.test.ts` | Passed for that test file. |
| `today_source_tests`: `pnpm --dir web exec vitest run src/components/AppShell.test.ts src/components/OperatorPanel.test.ts` | Passed, 27 focused tests including actor/session isolation, delayed callbacks, proposal completion and deferred launcher regressions. Final production integration still requires rerun. |
| `today_source_tests`: `pnpm --dir web exec eslint src/components/AppShell.test.ts src/components/OperatorPanel.test.ts` | Passed for those two test files. |
| `today_source_tests`: `pnpm --dir web exec vitest run src/views/TodaySourcesView.test.ts src/views/PeopleTodaySources.test.ts` | Passed, 8 tests across 2 files after correcting stale-baseline enable and unknown-configuration gating. Initial behavior failures were retained and fixed, not removed. |
| `today_source_tests`: `pnpm --dir web exec eslint src/views/TodaySourcesView.test.ts src/views/PeopleTodaySources.test.ts` | Passed for both new component test files; their whitespace checks also passed. |
| `today_source_tests`: focused Vitest for `src/sessionLifecycle.concurrent.test.ts` | Passed, 3 tests with independently retained coordinators and explicit cookie/callback ordering. The file's whitespace check passed. Independent review and final-tree rerun remain required. |
| `today_source_tests`: focused Vitest for `src/sessionLifecycle.recovery.test.ts` | Passed, 3 tests for protected startup recovery, aborted pre-dispatch record cleanup and actual logout DELETE with unavailable storage. Whitespace check passed; final review/gates remain. |

The primary reports a later focused checkpoint with Web type checking, ESLint
and 100 tests across seven Web files passing. Backend formatting, 417
application unit tests, 59 Operator tests and compilation of the complete API
test binary also passed; an earlier checkpoint passed eight Operator explanation
tests. SQLx preparation passed after correcting static candidate metadata.
These counts overlap earlier
focused runs and must not be added together as unique test totals. Exact final
commands and final-tree gates remain pending.
The latest focused Web checkpoint passed **106 tests in 12 files** in 2.19
seconds, including both final I16 recovery assertions. The primary also reports
passing Web typecheck and targeted ESLint. The coordinator inspected the Vitest
summary in `web-i16-and-today-final.txt`. That output contains happy-dom fetch
abort messages and two localhost `/api/me` 401 lines; the primary is checking
that teardown has not allowed requests to escape controlled mocks. This is
not the final full Web/repository gate.
The primary identified those requests as test cross-contamination: dynamic
lifecycle imports left storage/focus listeners after module reset. The recovery
test now removes its listeners per case. Its isolated five-test run passed
without abort/network output. The router case also now defers the real replay
authorization and asserts its rendering gate remains raised until resolution.
The combined router/AppShell/recovery checkpoint passed **40 tests in 3 files**
without console fetch output; the coordinator inspected
`web-router-replay-and-isolation.txt`. Full gates remain required.

The primary's targeted database checkpoint ran
`cargo nextest run -p crm-api --test all --run-ignored only today_source --no-fail-fast`
with the existing development environment loaded, `DATABASE_URL` assigned from
`MIGRATION_DATABASE_URL`, and `SQLX_OFFLINE=true`. Result: 11 passed, 458 skipped.
This includes six source-command tests and five bounded queue acceptance tests.
The coordinator inspected the sanitized command/result summary in
`/tmp/crm-011c-impl/today-source-db-nextest.txt`. The first attempt lacked the
ignored-test annotation/flag; after adding the annotations, the primary reran
the proper ignored path successfully. These targeted results do not replace
the final full database gate or remaining failure/contract coverage.

A later primary run of the same targeted ignored-test command included 29
Today-source tests: 28 passed and one filter-parity fixture failed. Its Source
literals contained hyphens, which are rejected by the preserved v1 vocabulary.
The primary changed only those fixture literals to underscores and reran the
failed test successfully (1 passed). Raw outputs are retained privately as
`today-source-db-nextest-final.txt` and `today-source-parity-rerun.txt` in the
owned temporary evidence directory. A complete rerun of all 29 and the final
repository gates remain pending; the separate rerun is not recorded as a
29-test pass. The primary subsequently reran the full targeted suite under
the guarded isolated QA target: **29 passed, 458 skipped**. That raw run is
retained as `today-source-db-nextest-final-rerun.txt`. Frozen-baseline and
controlled failure tests were added afterwards and still require integration
and execution, as do the final full gates.

The subsequent source/baseline/controlled-failure run completed **34 of 35**
tests successfully. The failed membership-lock timeout test returned the right
partial snapshot but replaced the backend connection instead of reusing the
recoverable one. The primary retained that assertion and fixed the race by
reserving 10 ms for PostgreSQL cancellation before the outer source deadline,
including the reference-validation helper's SQL timeout. The targeted rerun
then passed (`today-source-recovery-rerun-2.txt`). The whole-source 500 ms and
combined cleanup 100 ms budgets remain unchanged. Raw failing-run output is
retained as `today-source-and-baseline-db-final.txt`; the full 35-case rerun
and final gates are still pending.
The primary then reran the complete guarded isolated source/baseline suite:
**35 passed, 479 skipped**, including the recovered backend-PID timeout case
and all four frozen comparisons. The raw result is retained as
`today-source-and-baseline-db-final-rerun.txt`. This closes that targeted
failure; it does not replace the remaining AC8/HTTP/full-gate work.

The first four hooked snapshot/failure tests ran with **2 passed, 2 failed**.
The snapshot fixture initially asserted membership for a built-in in a different
stage from its filter; it was corrected to assert the matching source-only row.
Two subsequent complete four-case runs each passed three and failed the backend
termination case. A further attempt failed to compile because closing a borrowed
SQLx connection would require ownership. These attempts are retained as
`today-source-hooks-db.txt`, `today-source-hooks-db-rerun.txt`,
`today-source-hooks-db-rerun-2.txt` and `today-source-hooks-db-rerun-3.txt`.
The termination fixture is under investigation: a control-connection SQL error
must not be mistaken for successful termination of the tested backend. At that
checkpoint, the fatal connection test had not passed.
After moving terminal control signaling outside the hook's recoverable error
path, the registered hooks/settings/deadlines checkpoint passed **8 of 8**
tests, with 514 skipped, in 11.613 seconds. This includes actual backend loss,
Operator timeout cancellation and setting/deadline cases before the stronger
assertions requested below. The coordinator inspected the retained raw summary
in `today-source-hooks-settings-deadlines-db-final.txt`.
An earlier malformed nextest filter selected zero tests and exited 4; its raw
log was overwritten and is unavailable. This is a reconstructed command/result
report, not a retained raw artifact. A subsequent broad-filter run was
interrupted after 29 of 38 tests passed, with no completed suite result;
`today-source-hooks-settings-deadlines-db-rerun.txt` retains that partial run.
`./scripts/sqlx-prepare` passed before the 8-test checkpoint, with output in
`sqlx-prepare-after-hooks-and-comments.txt`. New assertion changes and final
full gates remain outstanding.
The next strengthened run passed seven of eight; its stale expectation removed
a built-in that the revised fixture no longer contacted. The primary fixed that
assertion, passed the snapshot-only rerun, then passed all eight in **11.886
seconds**, with 514 skipped. Retained logs are
`today-source-hooks-settings-deadlines-db-strengthened.txt`,
`today-source-hooks-snapshot-rerun.txt` and
`today-source-hooks-settings-deadlines-db-strengthened-rerun.txt`; the coordinator
inspected the final summary. A final timing-assertion refinement remains: the
early consumed source allowance must exceed the permitted scheduling/recovery
tolerance, so a renewed late-phase budget cannot satisfy the assertion.
The primary then increased the controlled early hold to 300 ms and adjusted
the remaining-budget assertion. The targeted deadline test passed **1 of 1**
in 1.885 seconds; `today-source-deadline-300ms-rerun.txt` retains the sanitized
output, whose summary the coordinator inspected. A renewed late-stage 500 ms
allowance now exceeds the test's allowed disposition margin.

### Coordinator takeover: gate runs on the tree as the Codex lanes left it

Run by the coordinator on 2026-09-06 between 17:48 and 18:05 local, before
any takeover edit, to establish the real state:

| Command | Actual result |
|---|---|
| `./scripts/check` | **Failed at clippy** with exactly three lints, all in helper-written test files that had only ever been formatted and run, never linted: `too_many_arguments` at `db_today_source_filter_parity.rs:189`, `bool_assert_comparison` at `db_today_source_races.rs:325`, `needless_lifetimes` at `db_today_source_hooks.rs:194`. `cargo fmt --check` passed. |
| `cargo clippy --workspace --all-targets --locked --keep-going -- -D warnings` | The same three lints and nothing else. |
| The remaining Rust steps of `./scripts/check`, run individually in the script's order | Production-shape `cargo check` passed; crate-boundary fences passed; `cargo nextest run --workspace --locked` 675 passed (one pre-existing leaky telephony test); `cargo test --doc` 5 passed. |
| The Web half of `./scripts/check`, run individually | Lint, typecheck, 436 Vitest tests in 36 files, build and 9 email-worker tests all passed. |
| `./scripts/check-db` | **Passed**: prepare-check clean (with the expected "potentially unused queries" warning for test-target cache files), 422 of 422 ignored database tests, 172 s. That is 43 new 011c tests over `main`'s 379. |
| `cargo check -p crm-api --test db_today_http_perf --features perf-harness` | **Failed**: one error at `db_today_http_perf.rs:339`, `format!("{:x}", Sha256::digest(..))`, no `LowerHex` for the digest type. |

Findings from that pass, dispositions in the review section below:
`tests/db_today_source_telemetry.rs` was handed back but never registered or
compiled; the harness did not compile; `scripts/sqlx-prepare` caches
test-target queries with `--all-targets` while `scripts/check-db` checks
without it (verified in sqlx-cli 0.8.6 source: extra cache files only warn,
so the mismatch is benign, but test-only queries are not schema-checked
there).

### Lane A, A2 and W results (Claude Sonnet 5), each verified by the coordinator

- **Lane A:** the three lints fixed with the assertions retained;
  `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `cargo fmt --all --check` clean. The telemetry test registered in
  `tests/all.rs`; on first compile it had a variable-shadowing error and on
  first run it expected `source_count="0"` on the metadata-unavailable span,
  whereas production deliberately leaves that field unset so zero never claims
  an observed count; the test now asserts the field is absent. Final targeted
  run: 1 passed, 522 skipped. No production file changed. `sha256_hex` now
  hex-encodes the digest bytes; the harness compiles.
- **Lane A2:** eleven unused driver items deleted (including
  `RECOVERY_GRACE_BUDGET`, see disposition below) and two style lints fixed;
  `cargo clippy -p crm-api --test db_today_http_perf --features perf-harness
  --locked -- -D warnings` clean; workspace clippy and `cargo fmt --check`
  still clean. Incomplete response bodies remain detected: a failed full-body
  read is retained as a `body_read` client failure, which fails the
  normal-completion gate.
- **Lane W:** corrections I17–I20 below with ten new Vitest cases;
  `pnpm run lint`, `pnpm run typecheck` and the full suite (**446 tests in 37
  files**, run three times) passed; `git diff --check` clean.

The final-tree gate order `./scripts/sqlx-prepare`, `./scripts/check`,
`./scripts/check-db` is run once by the coordinator after Phase B and is
recorded in its own subsection below.

## Live browser and HTTP performance

The normal live walkthrough is complete. Authenticated HTTP performance was
executed by the coordinator during the takeover; every attempt is retained.

### Phase B run 1 (2026-09-06 19:39 local): FAILED acceptance, root cause found

Optimized build `9a6ef7a5…c20722` (release profile, `perf-harness` feature,
zero warnings; per-file source hashes and host facts in the archive), executed
once by the coordinator as the sole database lane on a quiet machine (no other
suite, build or browser), sqlx-managed ephemeral database, 234 s. The harness
retained the full safe artifact and the coordinator's independent recount
agrees with it: 215 waves, 1,595 attempts, 0 failed joins, sentinel checks
passed, all four paired zero-source parity comparisons exact.

| Result | Detail |
|---|---|
| Passed series (11 of 15) | Both zero-source arms for all four books, the typical book with five sources, and the partial-builtins and empty-builtins books with five sources: every attempt HTTP 200 and complete, request p95 within its cap at 1/10/20, whole-source p95 ≤ 295 ms, enumeration within budget, paired regression within max(25 ms, 10%). |
| Failed series (4 of 15) | Concentrated 30,000-Person book with one dense source, one absence source, five overlapping sources, and the independent five-source repeat: in every concurrency-20 wave exactly 10 of 20 requests returned HTTP 503 after a 2.0 s feed-pool acquisition timeout; serial p95 2,177 ms and 2,528 ms for the absence and five-source cases against the 1,250 ms cap; concurrency-10 p95 2,824–4,012 ms against 2,500 ms. |
| Not the source SQL | Recorded whole-source evaluation p95 stayed at 152–295 ms in the failing series; membership probes were sub-millisecond. |

Root cause, established afterwards on the retained fixture database with the
exact prepared statements (diagnosis transcripts in the archive):

- The built-in candidates statement ran at ~205 ms through the first two
  concentrated series and then ~1,800–2,200 ms from the middle of the
  one-dense series onward, on identical data and parameters. The boundary
  coincides with the fixture's first autovacuum pass (19:40:24 local, about
  64 s after seeding), which fills the visibility map and invalidates cached
  plans.
- In that state PostgreSQL 18 replans the per-Person effective-contact
  LATERAL from a Nested Loop Anti Join into a **Merge Anti Join whose inner
  side is an index-only scan of the entire `contact_attempted_corrects_once`
  index once per Person** (about 1,500 entries × 27,282 loops, 679k buffer
  hits). The choice is a knife-edge cost tie: forcing the visibility count to
  zero, or a later manual VACUUM that re-estimated `reltuples`, flipped it back
  to the 210 ms plan. The frozen original query shows 2.6–2.7 s in the same
  state, so this is a pre-existing Slice 003/009 Today hazard, not a 011c
  regression, and it also means production tables (always vacuumed) sit on
  the slow side of that tie for large books.
- `SET enable_mergejoin = off` for the transaction yields the fast plan
  (212–223 ms) regardless of visibility state and leaves the source
  statements' plans and timings unchanged (dense 124 ms, absence 60 ms,
  combined 121 ms, no-phone 27 ms). Disabling index scans also avoids it
  (300 ms) but is broader.

Disposition: the coordinator recorded a **proposed fourth transaction-local
planning change** in specification §8 (pending the user's approval), had
lane P implement it beside the JIT setting with restoration assertions, and
reran Phase B on the resulting build; that run has its own subsection below.
No budget, cap, pool size, fixture or workload was changed, and run 1 is
retained in full.

### Phase B run 2 (2026-09-06 20:07 local): final arm passed; one baseline pairing invalid

Optimized build `c07e21b0…3a76ce` of the tree with the proposed planning
change, same fixture generator, protocol, pool and budgets, 144 s, quiet
machine. Full tables in the [Phase B archive](../design/perf/slice-011c-http-2026-09-06/README.md).

- **Final arm, all eleven series:** 522 of 522 measured attempts HTTP 200 and
  complete; 1,201 whole-source evaluations as expected; request p95 within
  its cap in every case at concurrency 1, 10 and 20 (concentrated five
  sources: 680 / 1,332 / 2,710 ms against 1,250 / 2,500 / 4,500); whole-source
  p95 152–321 ms, maximum 391 ms; enumeration maximum 25 ms; feed-pool wait
  maximum 1,586 ms (headroom 414 ms); the independent concentrated
  five-source repeat completed 40 of 40 with 200 of 200 evaluations, wait
  p95/max 1,488/1,557 ms.
- **Paired zero-source regression:** exact payload and hash parity and p95
  within the allowed limit for the typical, partial-built-ins and empty
  built-ins books. For the concentrated book the frozen original series
  completed only 48 of 68 attempts (its concurrency-20 waves hit the same
  merge-join plan and timed out at the pool), so that pairing is invalid in
  run 2; run 1 holds a complete, exact pairing for it (726→235,
  1,260→439, 2,372→786 ms, within allowed), and the toggle cannot change
  payloads. Reported as a limitation, not a passed same-run check.
- The harness's own gate therefore still reports failure (it requires every
  series, including the baseline's, to complete); the coordinator's recount
  and the summary tables are the acceptance evidence for the final arm.
- Both fixture databases were dropped after analysis; no shared data was
  touched.

The [HTTP protocol](../design/perf/slice-011c-http-2026-09-06/PROTOCOL.md)
declares fixture, matrix, raw attempt accounting and acceptance limits before
execution. It contains no passing measurements.

Independent review of the stable HTTP harness confirmed the declared matrix,
real cookie authentication, separate original/final arms and exact normalized
payload/hash comparison structure. It found four measurement defects: timing
continued after body completion; late server telemetry could be lost after
client/task failures; per-attempt outcome/count checks were insufficient;
and warm-up failures did not fail acceptance. Independent rereview closed all
four helper corrections. It also caught a collector scope-registration race,
fixed before closure. Late snapshots are reconciled into retained metadata,
initial terminal state survives, summaries recompute, and join-failure telemetry
has its own serialized records. Final orchestration must invoke that
reconciliation before writing evidence. Fixture integration and actual
execution remain pending. The test-only collector is not evidence that
production trace output excludes private data.

The primary reports two passing collector lifecycle tests: a failure drain
retains active server events, and an expired drain retains later scope data
under the same capture. Exact command:
`cargo test -p crm-app --lib domain::today::test_support::tests --features test-support`.
This is targeted helper evidence, not an HTTP measurement.

The next fixture review found the one-absence case selected the fifth filter
(`has_phone=false`) instead of Phase A's third filter (Stage plus never
contacted). The primary corrected that selection. The reviewer also identified
that the original router needed the source-control routes used by fixture
setup, and that the initial four-file source digest did not identify the whole
measured path. These integration/provenance corrections must close before
acceptance. Review otherwise found matching ordinal history distributions,
clock and pool settings; actual fixture counts still require execution.

The final orchestration rereview closed the source-control route issue:
only Today GET uses the frozen query; setup uses current authenticated source
routes. Setup/auth failures preserve earlier waves in the final artifact.
Post-stop reconciliation is invoked before serialization/acceptance, and each
arm's pool is explicitly closed before the next is created. Original-arm
absolute caps are correctly exempted while completeness and paired regression
remain required. Full source/binary provenance is the remaining review item;
this review did not run the benchmark or establish performance acceptance.

The coordinator owns the live walkthrough against a separate synthetic runtime
provided by the database lane. A dedicated loopback hostname must keep its
session cookies separate from the existing development and deployed app tabs.
Load measurements pause while the browser is exercising this runtime.
After the pending-attempt implementation's live retest, the coordinator signed
out of the synthetic account and closed both active walkthrough tabs to stop
their passive Today refreshes before load measurement. The initial connection-
error tab contains no active application. Runtime/database cleanup remains
with the primary lane at the final handoff.

Five synthetic screenshots and the two-tab session-replacement observations,
including the narrow-layout correction and partial-source notice, are retained in the
[browser evidence archive](../design/qa/slice-011c-2026-09-06/README.md).
They represent the recorded intermediate checkpoints; final privacy and gate
acceptance remains separate.

| Browser scenario | Observation |
|---|---|
| Enable a personal list, navigate to Today, reload and verify persistence | Observed Alice's five seeded sources; removing one immediately showed four. Created/enabled shared Me through UI to reach five; reload retained choices. |
| Enable shared Me as two different agents; verify each viewer's matches | Alice's four assigned People gained the shared Me reason; Carol-owned Marcus did not. As Carol, the same shared list matched only Marcus. Carol could enable it but had no shared-definition Save/Delete controls. |
| Broad list includes colleague-assigned and unassigned People | Observed Marcus (Carol) through broad source; newly added unassigned no-Inquiry fixture appeared after refresh. |
| Built-in admission, overlapping list links, no-Inquiry/review-only row and real contact label | Observed preserved four high built-ins and multiple distinct list links, then an unassigned zero-Inquiry/zero-contact-method row with Review person. Contacted Jordan later displayed actual “just now” in the list band. Full cap pressure is covered separately by DB tests. |
| Contact removes an activity-based match while a broad matching reason remains | Logged synthetic contact through the normal dialog: Jordan lost Never-contacted/built-in reasons but remained via shared Me/email lists. Marcus exited Alice's Today after his last activity-based source cleared. |
| Dirty preview leaves Today unchanged; saving updates current criteria/name | On stable I13 UI, dirty no-phone preview showed two matches while a separately refreshed Today tab retained all six saved broad reasons. Reset draft, saved Never-contacted criteria and new name; Today replaced all six old-name reasons. An earlier attempt was interrupted by HMR and was repeated, not counted. |
| Five-source capacity, sixth-source recovery, disable/re-enable and deletion | Five/sixth-source conflict and persistence observed. I13 live fix verified: cap recovery opens `/today?sources=1` with manager already visible. Removed sources down to zero without removing built-ins. Deleted an enabled personal list through its normal dialog: definition and source disappeared, built-ins remained. Re-enabling a valid remaining list restored its matching reasons. |
| Known invalid source, partial failure/recovery and unknown configuration | Sole DB lane temporarily made one synthetic enabled definition unsupported. Today kept five available People with a persistent named partial notice; Retry preserved the honest notice. Its detail page hid unsupported criteria/results but allowed Remove from Today. Removing it cleared the notice. The DB lane restored exact saved JSON/revision; UI re-enable then produced a complete Today with three email-list reasons and four People. No source corruption remains. Initial unknown-configuration outage remains component-tested, not yet live-tested. |
| Member/admin personal privacy, second Organization and logout/login | Alice (Acme admin) opening Carol's known private ID saw generic unavailability, no private name/criteria/table; library omitted it. Carol likewise could not read or discover Alice's personal list. Bob in Best Realty had zero lists/sources and received generic unavailability for Acme's shared list ID. Revised I16 passed live Alice-to-Carol and Carol-to-Carol session replacement: logout elsewhere immediately cleared the receiving tab; navigation after login showed the current actor and empty Operator. Concurrent/fallback review remains open. |
| Keyboard focus, action labels, narrow layout and source-manager recovery | At 390px browser width (375px document content), manager rows fit without document overflow. I14 fixed and visually rechecked: header actions stack cleanly. I15 corrected and live rechecked: keyboard removal moves focus to the next named Remove control; removing the last source moves focus to Lists. Viewport override was reset afterwards. |
| Blocked session recovery (I17, coordinator takeover, 19:39 local) | With Alice signed in on the same synthetic origin, a foreign never-settled auth-attempt record was written into storage. Reloading Today rendered only the fail-closed copy and the Reset and continue control, no private navigation. Reset removed the record, published a settled boundary, re-verified `/me` and restored the four-item Today; storage then held only the settled marker. Screenshot `session-reset-blocked.png` in the browser archive. The QA hostname's Centrifugo handshake returns 403 (allowed-origin configuration of that isolated runtime, unchanged from the Codex walkthrough); it does not affect the checked behavior. |

## Review, cleanup and delivery

Early WIP review found C011C-I1: source hydration joins occurred before the
ordered candidate limit, unlike the measured prototype. The reviewer requested
restoring the bounded prefix before detail hydration. Independent re-review
closed I1 in code: ordered prefix selection now precedes full hydration.
Final tests and HTTP measurements remain pending.

The reviewer also supplied a read-only connection-ownership design to preserve
captured work and discard unsettled connections within the approved cleanup
budget. SQLx's graceful pool close alone has a longer timeout, so final tests
must establish the actual cancellation/discard behavior. This support review
does not establish that the eventual implementation meets those guarantees.

The early Web checkpoint identified additional WIP defects, sent to the
primary writer for correction:

| Finding | Trigger / correction required | Disposition |
|---|---|---|
| C011C-I2 · P1 | Same-actor/Organization session replacement could retain or replay Operator state and accept late callbacks; proposal completion also needed fencing. Preserve normal drawer behavior within one session. | Closed in independent code review: auth generation, preserved drawer lifetime and callback fences; component regressions present. Final integration gates pending. |
| C011C-I3 · P2 | List-only `waiting_since: null` rendered as Never contacted even when `last_contact_attempt` existed. | Closed in independent code re-review: actual last contact is displayed. Tests/browser pending. |
| C011C-I4 · P1 | Partial/unavailable empty Today rendered the complete empty-state claim; source-specific recovery details were missing. | Closed in independent code re-review: truthful incomplete-empty state, issue names and Retry/Manage controls. Tests/browser pending. |
| C011C-I5 · P2 | Saved-list mutations/reconciliation did not invalidate Today/configuration; force-fresh entry/focus/manual/timer and session cleanup were incomplete. | Closed in latest independent code re-review, including invalid-list source-config refresh. Final tests/browser pending. |
| C011C-I6 · P2 | Enable used newer metadata rather than the retained saved baseline while the UI had a dirty draft; unsaved repair also affected enable validity. | Closed in independent code review for enable revision/stored validity. Invalid disable is separately tracked as I9; final tests pending. |
| C011C-I7 · P2 | Lost enable/disable responses had no configuration reconciliation; removal errors and delayed callbacks lacked recovery/session guards. | Closed in independent code re-review after view/session fences and error clearing. Final tests/browser pending. |
| C011C-I8 · P1 | Operator reason JSON serialized a list name as a bare string despite fixed explanatory text. | Closed in independent code re-review: wrapped names and shared source adapters are present. Final tests pending. |
| C011C-I9 · P2 | An enabled unsupported filter has no saved baseline, causing its visible Remove action to return early. | Closed in independent code re-review. Regression/browser pending. |
| C011C-I10 · P1 | Repeated reference reads could each receive the same timeout and exceed the whole-source budget; enumeration/transaction cleanup awaited operations outside their deadline. | Closed in independent code re-review after preserving the recovery deadline through trailing malformed definitions. Controlled tests pending. |
| C011C-I11 · P2 | Named-list detail omitted the required N-of-5 capacity and linked management recovery; unknown source configuration disabled Enable without an explanation/retry. | Closed in coordinator code re-review: capacity, management link and settings retry present. Primary reports focused typecheck/lint and 11 People/Today manager tests passing. Browser pending. |
| C011C-I12 · P2 | Today source-manager enumeration failure with no cached data also claimed no sources were enabled; a removal notice hid the source rows and their retry controls. | Closed in coordinator code re-review: unknown configuration has retry and no false empty claim; removal notices preserve the rows. Focused component regressions passed in the primary's 11-test run. Browser pending. |
| C011C-I13 · P2 | The cap alert's Manage sources link landed on Today with the manager closed. | Closed in live browser recheck: link now reaches `/today?sources=1` and opens removal controls directly. Primary's focused 12-test/typecheck/lint run passed. |
| C011C-I14 · P2 | At narrow width the two Today header actions squeezed update text into many one-word lines. | Closed in live screenshot recheck: Today opts into narrow stacked PageHeader actions; document width remains 375px. Primary reports focused typecheck/lint and seven Today-source tests passing. |
| C011C-I15 · P2 | Keyboard removal succeeded but discarded the focused button and left focus on BODY. | Closed in live keyboard recheck: focus advances to the next named Remove control or Lists after the last removal. Primary's focused typecheck/lint and eight component tests passed. |
| C011C-I16 · P1 | Logout/login in one browser tab replaced the shared cookie while another tab retained the previous actor's private Operator draft and cached identity. | Closed in independent code review after the pending-attempt, response, route and storage fixes. Normal cross-tab actor-change and same-actor relogin passed live. Both final recovery assertions passed in the 106-test checkpoint; assertion review and full gates remain. No Operator request was sent. |

I1–I16 are closed in code review; final regression execution remains required.

### Acceptance review at takeover (coordinator, 2026-09-06 evening)

The coordinator read the backend Today orchestration, source persistence and
commands, both source SQL files, the built-in query change, routes, models,
Operator adapters and prompts, and the Web rendering, cache and session code
against specification §§2–8, and commissioned a read-only Web session-privacy
review. No blocking backend or Operator defect was found: the snapshot and
clock, enumeration and per-source budgets, savepoint recovery and
owned-connection discard, the K+1 prefix bound, merge order, reason placement,
untrusted-name wrapping and bounded absence match §§4–6; the built-in SQL
change is exactly the three approved predicates; the source SQL predicate
matrix is textually identical to the People query's apart from the bound
snapshot clock and the ID constraint. Findings:

| Finding | Trigger / correction | Disposition |
|---|---|---|
| C011C-I17 · P1 (availability) | An auth attempt that never settles (its tab closed, reloaded or crashed mid-login/logout) left a durable pending record that blocked verification in every tab forever; a fresh login elsewhere could not clear it and the paused copy offered no control, so the browser profile was unusable until site data was cleared. | Fixed by lane W: user-driven `resetSessionCoordination()` plus AppShell copy and a Reset and continue control; no timer releases a pause. Unit-tested for foreign and own orphan records; verified live (browser table above). Spec §6 carries the amendment. |
| C011C-I18 · P3 | Ordinary verification of the persisted settled marker on every cold load and same-tab login rendered the alarming "session change has not finished" copy for the `/me` round trip. | Fixed by lane W: in-flight verification renders "Loading your session…" with no control; the blocked copy is reserved for an outstanding attempt. Component-tested. |
| C011C-I19 · P3 | A second session boundary completing during an in-flight route replay skipped the role re-check. | Fixed by lane W: the replay is re-run once when requested during a replay, with the gating flag held. Router-tested. |
| C011C-I20 · P3 | The public-route pending check ran before storage synchronization, so an undelivered `changing` marker could park a `/login` navigation. | Fixed by lane W: synchronization precedes the check. Router-tested. |
| R1 · P2 (residual, user decision) | The docked call host (Slice 006) is not fenced on the session boundary: an active call and its Person name survive an actor or Organization change in another tab; outcome writes then 403 server-side. Pre-existing, outside this lane's files; the smallest fix hangs up an in-progress call on identity change, which is customer-visible. | Not changed. Recorded in PROJECT_STATE for decision. |

Accepted deviations and dispositions recorded by the reviewer:

- The 011a People query gained a nullable reference-clock bind
  (`COALESCE($21, now())`) as a test-support-only common-clock seam for
  filter parity fixtures. Production passes NULL, so behavior is unchanged
  and the 011a filter tests still pass; the SQLx cache entry was regenerated.
  Accepted, noting §8 enumerated only the Today SQL changes.
- `RECOVERY_GRACE_BUDGET` in the harness driver was unused and was deleted.
  Normal benchmark samples never enter recovery; the 100 ms grace is enforced
  by production constants and pinned by the hooks/deadline database tests, so
  no benchmark assertion is missing.
- Three Web coverage gaps the reviewer listed remain open and are not
  defects: a fake-clock 60-second timer test for Today/sources, a same-actor
  relogin test through the real coordinator wiring, and a cancellation-race
  test (the reviewer traced the race and found no render).
The I16 correction history follows. Its first correction
cleared Alice's identity and unsent draft in the receiving tab immediately
after logout elsewhere. That tab stayed safely at Sign in and recovered Carol
with an empty Operator after reload. This partial result does not close I16:
independent review found overlapping auth-response ordering, a same-key `/me`
verification promise cycle, missing late private-response fencing, absent
storage-failure fallback and startup handling of an existing transition marker.
The primary is correcting these with multi-coordinator/deferred-response tests;
the subsequent corrective batch passed focused typecheck, ESLint and 50 tests
across six files. The coordinator's live actor-change and same-actor relogin
rechecks both passed: the receiving tab immediately showed Sign in, without
the previous identity, private list name or draft. After login and direct Today
navigation it showed the current actor and an empty Operator. The receiving
tab safely remains at Sign in until navigation/reload; automatic redirection
was not asserted. Independent review of the remaining concurrent/deferred/
fallback cases is still pending, so I16 is not yet closed.
The next independent review identified a further slow-auth race: the orphan
recovery timer could settle and verify an old cookie while the original login
was still running; its eventual response reused the already-settled marker,
so other tabs could miss that cookie replacement. This requires a fresh
completion boundary and a controlled delayed-auth regression before closure.
Further review established that elapsed time cannot prove an in-flight cookie
replacement is over: known outstanding auth attempts must continue gating
private state, including overlapping attempts whose response callbacks are
delayed. The primary is replacing automatic trust restoration with explicit
pending-attempt handling. Readable-but-unwritable storage and a failure first
observed after private transport also require unavailable-state fencing. These
were sent for correction; ordinary live account-switch passes do not prove them.
The primary's next batch uses separate opaque pending records per auth attempt,
removes the timer's automatic release, checks durable read/write capability,
and fences private transport before and after its response. Its focused run
passed 53 tests plus typecheck/ESLint. The coordinator fully reloaded both QA
tabs and again passed Carol-to-Alice and Alice-to-Alice relogin checks, with
an empty Operator and no previous private list name. Independent review of
the concurrent/storage cases remains pending.
Independent review then found the main privacy races closed in code and in
the independent-coordinator tests. The last recovery batch addresses protected
startup previously waiting before AppShell could mount, aborted pre-dispatch
record cleanup and actual logout transport under unavailable storage. The
helper's three recovery tests passed; final review, including role routing after
pending verification, is still outstanding.
The primary's subsequent focused run passed 102 tests in 12 Web files plus
typecheck and targeted ESLint; the backend test-support hooks compiled in the
offline API test binary. Independent review still found two recovery gaps:
role authorization must run again after a protected route was admitted in a
pending state, and retiring an aborted attempt's storage key must also release
its local/peer pending verification state. The initial recovery tests asserted
router readiness and key deletion, not those eventual outcomes. The primary
is correcting both and extending the behavior assertions.
The next focused run passed 104 tests across 12 Web files plus typecheck and
targeted ESLint. Review found two remaining recovery details: AppShell must
keep private content gated until asynchronous route authorization finishes,
and locally known completed auth attempts must retry retirement after a
transient storage failure, including logout. Unknown or still-running attempts
must never be retired on that basis. These remain open; the passing focused
checkpoint does not establish their correction.
The latest independent Astra review found both production corrections complete:
the synchronous route-replay flag gates AppShell through authorization, and
retry retirement admits only this coordinator's completed or aborted attempts,
including a retained logout epoch. Unknown or still-running attempts stay
blocked. No further concrete production defect was found in that bounded
rereview. Two behavior assertions remain to be added and run: holding a
restricted component unmounted through a deferred route guard, and recovering
after post-response cleanup failure while retaining another pending record.
The primary subsequently added both assertions, and they passed in the
106-test focused Web checkpoint. The coordinator's subsequent inspection
found that AppShell's new case manually toggles the replay flag; the router
case checks only the eventual redirect. Together they still do not prove
that the actual asynchronous replay keeps its flag raised until authorization
finishes. That wiring assertion and final independent review remain required.
The primary then added that real deferred-authorization assertion and corrected
listener cleanup in the recovery tests. The resulting three-file, 40-test run
passed cleanly; final independent review remains.
Independent Astra review then closed both final I16 evidence gaps. The real
guard is deferred for denied member routes and the allowed admin route; the
paired component case proves gated setup does not run. The completed-attempt
recovery assertion retains another live key and withholds verification until
it settles. The reviewer inspected the clean 40-test log without rerunning it.
No Operator request was sent during these privacy checks.

Independent read-only review verified all four frozen original-query hashes
against `git show 9d62e86` and confirmed that only Rust import/module paths
changed. The new parity helper compares full serialized results at a common
clock and removes only the asserted empty-complete source envelope. That review
found no baseline-code defect, but requested reply/inquiry/outcome precedence,
multiple-viewer/tenant exclusions and exact 199/200/201 boundaries in addition
to the initial mixed and overflow fixtures. Those cases were subsequently
added and all four frozen comparisons passed in the 35-case run above.

Independent review also checked the task-local, test-support-only Today fault
checkpoints. Source, recovery and commit checkpoints retain their actual
absolute deadlines and owned-connection guard; no public endpoint or
environment-triggered fault mechanism was added. The API test binary compiled
with the feature, and `cargo check -p crm-app` passed without it. The four
controlled snapshot/failure tests have the incomplete execution results
recorded above; they are not a passed acceptance suite.
The subsequent AC8/settings/deadline review found no concrete production
defect but identified four evidence gaps: the snapshot fixture must mutate a
source-read contact and delete a source; late-source failure must discard a
staged source-only Person; deadline tests must consume part of the allowance
and prove actual expiry at the original deadline; settings tests must begin
with read-write mode, observe the active transaction settings and include a
successful enabled source. Strengthening these assertions and executing the
resulting tests remain required. The existing real Operator cancellation test
was confirmed to reach the owned Today path and subsequently passed in the
8-test checkpoint. The stronger reviewed assertions are not yet recorded as
executed.
On the next read-only checkpoint, staged source-only candidate discard and
shared recovery/commit expiry assertions were accepted. Remaining corrections
are one stale snapshot expectation, consumption of early whole-source budget
before late expiry, and observing active transaction settings. The initial
eight passing tests do not establish those later additions.
The final bounded rereview closed all four assertion gaps after inspecting
the strengthened 8/8 run and the targeted 1/1 deadline run. It confirmed
300 ms of early budget consumption, expiry against the original deadline,
discard of staged source-only work, contact/delete snapshot isolation, active
transaction settings and exact restoration on pooled reuse. The reviewer
did not rerun tests or operate the database.

Subsequent component tests exposed two
additional People-control defects: disabling Enable when newer metadata arrived
behind a dirty baseline, and allowing Enable with unknown source configuration.
The primary corrected both; all eight new component tests then passed with
the original assertions retained. Final acceptance still requires the remaining
DB/failure/HTTP/browser/performance evidence.

At this WIP checkpoint, Operator source-status DTOs, bounded absence and ordering
amendments were initially incomplete. At the subsequent full code checkpoint,
the reviewer found the shared adapters, DTOs, ordering, bounded absence and name
wrapping consistent on inspection. Source-enabled HTTP/Operator parity and
failure/cap/name-safety integration tests remain required under AC11.

The reviewer's coverage inventory also identified missing source-specific DB
and HTTP evidence beyond the initial command tests: exhaustive privacy/grants
and body/error cases, contention/tombstone races, v1 projection parity,
reservation/reason completeness, original-query payload/order comparison,
controlled failure/snapshot/deadline cleanup, new UI states and live behavior.
Existing built-in tests and Phase A evidence do not substitute for those cases.
The primary and bounded acceptance-test helper are filling this inventory.

Pending final implementation review and owned-runtime/data cleanup. Shared
development API 3000 and Vite 5173 are to be preserved. The credential exception
below must remain disclosed; do not claim shared development data was untouched.
No 011c commit, merge, push or deployment has been performed.


### Final-tree gates and file audit (coordinator, 2026-09-06 20:11–20:14 local)

Run once, in the required order, after Phase B run 2 and after every lane had
finished, with no other suite, build or browser active:

| Command | Actual result |
|---|---|
| `./scripts/sqlx-prepare` | Passed (8 s); regenerated the offline cache with `--all-targets`; no tracked cache file changed, the eleven new cache files below are unchanged. |
| `./scripts/check` | Passed, 24 s: fmt, clippy `-D warnings`, production-shape check, crate fences, 675 non-ignored Rust tests, 5 doctests, Web lint, typecheck, 446 Vitest tests in 37 files, Web build, 9 email-worker tests. |
| `./scripts/check-db` | Passed, 146 s: Centrifugo reachable, prepare-check clean (expected warning about test-target cache files), 423 of 423 ignored database tests (422 as the Codex lanes left them plus the registered telemetry test). |
| `git diff --check` | Clean. |

Cleanup performed by the coordinator: the QA runtime (crm-api on 31011 and
Vite on 51011) stopped by exact PID; the generated databases
`crm_011c_qa_20260906_153436_68680` and `crm_011c_perf_20260906_140624_50335`
dropped; both Phase B fixture databases dropped; the debug Chrome instance
and the MCP scratch directory removed. The shared development API on 3000 and
Vite on 5173 were left running as before; `crm_dev` was not touched. The
private Codex evidence directory `/tmp/crm-011c-impl/` was left for the user
to delete (it holds the synthetic QA credentials, now useless).

Working tree at handoff (`git status`, 105 paths; base `9d62e86`):

Modified tracked files (54):

- `backend/crates/crm-api/Cargo.toml`
- `backend/crates/crm-api/src/auth/session.rs`
- `backend/crates/crm-api/src/error.rs`
- `backend/crates/crm-api/src/lib.rs`
- `backend/crates/crm-api/src/operator/backend.rs`
- `backend/crates/crm-api/src/operator/explain.rs`
- `backend/crates/crm-api/src/routes/today.rs`
- `backend/crates/crm-api/tests/all.rs`
- `backend/crates/crm-api/tests/db_operator.rs`
- `backend/crates/crm-api/tests/db_schema.rs`
- `backend/crates/crm-app/src/domain/person/filter.rs`
- `backend/crates/crm-app/src/domain/person/queries.rs`
- `backend/crates/crm-app/src/domain/saved_list/commands.rs`
- `backend/crates/crm-app/src/domain/saved_list/error.rs`
- `backend/crates/crm-app/src/domain/saved_list/mod.rs`
- `backend/crates/crm-app/src/domain/today/mod.rs`
- `backend/crates/crm-app/src/domain/today/model.rs`
- `backend/crates/crm-app/src/domain/today/queries.rs`
- `backend/crates/crm-app/src/domain/today/rank.rs`
- `backend/crates/crm-operator/prompts/system.md`
- `backend/crates/crm-operator/src/lib.rs`
- `backend/crates/crm-operator/src/service.rs`
- `backend/crates/crm-operator/src/tools.rs`
- `backend/crates/crm-operator/src/views.rs`
- `backend/crates/crm-operator/tests/snapshots/tool_definitions.json`
- `docs/decisions/DECISION_LOG.md`
- `docs/plans/PROJECT_STATE.md`
- `docs/plans/SLICE_011_LADDER.md`
- `docs/specs/SLICE_003.md`
- `docs/specs/SLICE_005.md`
- `docs/specs/SLICE_006b.md`
- `docs/specs/SLICE_006c.md`
- `docs/specs/SLICE_009.md`
- `docs/specs/SLICE_011a.md`
- `docs/specs/SLICE_011b.md`
- `scripts/sqlx-prepare`
- `web/src/api/client.test.ts`
- `web/src/api/client.ts`
- `web/src/api/queries.test.ts`
- `web/src/api/queries.ts`
- `web/src/api/types.ts`
- `web/src/components/AppShell.test.ts`
- `web/src/components/AppShell.vue`
- `web/src/components/OperatorPanel.test.ts`
- `web/src/components/OperatorPanel.vue`
- `web/src/components/PageHeader.vue`
- `web/src/lib/errors.ts`
- `web/src/router.test.ts`
- `web/src/router.ts`
- `web/src/views/LoginView.vue`
- `web/src/views/PeopleView.vue`
- `web/src/views/TodayView.test.ts`
- `web/src/views/TodayView.vue`
- `web/vite.config.ts`

Deleted tracked file (1): the pre-011c cache entry for the People
filter query, replaced by its regenerated hash below.

- `backend/.sqlx/query-8b9bcedf1368c7e8299a93024256861758e9efbe262ff0caf00f575bc4a6363e.json`

New files (50):

- `backend/.sqlx/query-00eb1b48fd56a11a650d130450625971cd7cb8ee1eb379b2902e362ced1b1c32.json`
- `backend/.sqlx/query-03c8969a041715648abeaffcb854fe70a58501e6b7b9409c2e6139a55d0bfba6.json`
- `backend/.sqlx/query-2df0c7341478dff4c422266d7aba6b1e3afe8117dfe918f757434ff5893dd0ad.json`
- `backend/.sqlx/query-304f8ccff93e3bfeeaaa4f8b2f4cb67a5794a2b13bf8681b857f592d26dfa9e3.json`
- `backend/.sqlx/query-3b7e3084a11a2d1cd5afa7f9d854ff33ab7ae5180bfd9d7538fe689cfcf81577.json`
- `backend/.sqlx/query-572cddb1d2f5572e9fef3baaa9f937d9d2702bcd49f85781122c97c250a4d997.json`
- `backend/.sqlx/query-7864d74e7cfdc6aa294ad743e809221e85fa7efc451650fa44147f6074d6239c.json`
- `backend/.sqlx/query-803a36b8b5d4d8e8ec06f8dea33f01cb1b41c9c791d63c8b1802ec2b1975abed.json`
- `backend/.sqlx/query-900128e0718b8e97fad6aa90c5cff8cb133b4319940de39d9f763f944ff954c9.json`
- `backend/.sqlx/query-c79bde7ca2174cae1534b074d9033855d27113d4e7ea66d14d0834345799bd96.json`
- `backend/.sqlx/query-eef4a7eb49fbf6b41f5ccf3e87607c94f1bf64e261fc63ee3a8a2b4b436d6be2.json`
- `backend/crates/crm-api/migrations/20260906000002_today_work_source.sql`
- `backend/crates/crm-api/tests/db_today_builtin_parity.rs`
- `backend/crates/crm-api/tests/db_today_http_perf.rs`
- `backend/crates/crm-api/tests/db_today_source_acceptance.rs`
- `backend/crates/crm-api/tests/db_today_source_contracts.rs`
- `backend/crates/crm-api/tests/db_today_source_deadlines.rs`
- `backend/crates/crm-api/tests/db_today_source_failures.rs`
- `backend/crates/crm-api/tests/db_today_source_filter_parity.rs`
- `backend/crates/crm-api/tests/db_today_source_hooks.rs`
- `backend/crates/crm-api/tests/db_today_source_operator.rs`
- `backend/crates/crm-api/tests/db_today_source_races.rs`
- `backend/crates/crm-api/tests/db_today_source_settings.rs`
- `backend/crates/crm-api/tests/db_today_source_telemetry.rs`
- `backend/crates/crm-api/tests/db_today_sources.rs`
- `backend/crates/crm-api/tests/fixtures/today_9d62e86/`
- `backend/crates/crm-api/tests/fixtures/today_http_perf_driver.rs`
- `backend/crates/crm-api/tests/fixtures/today_http_perf_fixture.rs`
- `backend/crates/crm-api/tests/fixtures/today_http_perf_manifest.txt`
- `backend/crates/crm-app/src/domain/today/source_candidates.sql`
- `backend/crates/crm-app/src/domain/today/source_membership.sql`
- `backend/crates/crm-app/src/domain/today/sources.rs`
- `backend/crates/crm-app/src/domain/today/test_support.rs`
- `docs/design/perf/slice-011c-2026-09-06/`
- `docs/design/perf/slice-011c-http-2026-09-06/`
- `docs/design/qa/`
- `docs/specs/SLICE_011c.md`
- `docs/specs/SLICE_011c_EXPLAINED.md`
- `docs/tasks/SLICE_011c_IMPL.md`
- `docs/tasks/SLICE_011c_SPEC_REVIEW.md`
- `docs/tasks/SLICE_011c_VERIFICATION.md`
- `web/src/api/today.test.ts`
- `web/src/api/todaySources.test.ts`
- `web/src/sessionLifecycle.concurrent.test.ts`
- `web/src/sessionLifecycle.recovery.test.ts`
- `web/src/sessionLifecycle.test.ts`
- `web/src/sessionLifecycle.ts`
- `web/src/testSetup.ts`
- `web/src/views/PeopleTodaySources.test.ts`
- `web/src/views/TodaySourcesView.test.ts`

### Post-verification edits before commit

After the gates and both Phase B runs, the user approved the three decisions.
The only edit to a file hashed by the Phase B manifest after run 2 is the
five-line comment above the planner setting in
`backend/crates/crm-app/src/domain/today/mod.rs` (wording changed from
proposed to approved; no code change): SHA-256 `f1558485575fa7c766ee7645ec81594875b4e6ae6832e1354b7e785500239f03` at run 2, `46b4c68ccba878572d79d199e7d86712633f6013468b55a9fd2be1c3ecb5db86`
at commit. The coordinator re-ran `cargo fmt --all --check` and
`cargo check --workspace --locked` after that edit; the full gates above were
not repeated for a comment. Documentation files were updated to record the
approvals.

### Shared development credential incident

During isolated QA setup, the primary reported that its first
`bootstrap-platform-admin` invocation used the inherited shared
`MIGRATION_DATABASE_URL`, although `DATABASE_URL` already pointed to QA.
The CLI explicitly uses the migration URL for that subcommand. It rewrote the
existing development owner's `local_credential.password_hash` and `updated_at`
using `CRM_DEV_SEED_PASSWORD`. The coordinator disclosed this to the user and
paused further database work until the primary verifies both target URLs and
the command's exact effects. No credential value or hash is recorded here.

The previous salted hash was not retained, so it cannot be restored. Supplying
the seed password does **not** establish that the previous plaintext password
was the same; that remains unknown without prior verification. No further
credential reset or attempted shared-data repair is authorized.

Coordinator source inspection confirms the command performs find-or-create
user, credential upsert and insert-platform-admin-if-absent operations. Those
helpers do not update existing display names or revoke sessions. The primary
recorded incident timing, affected-row evidence and isolated runtime target
verification before resuming. Its audit established
that the existing owner user and platform-admin grant both date to 2026-08-25;
neither was created by the mistaken command. The credential timestamp changed
to `2026-09-06T22:34:48.429550Z`. Prior plaintext equality remains unknown.
Both QA URL variables now target the guarded generated database, and the
coordinator authorized resuming only isolated work with executable target checks.
The first working QA browser login occurred after that verification.
The earlier generic statement
that shared data was untouched is superseded by this incident record.
