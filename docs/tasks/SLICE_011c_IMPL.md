# Slice 011c — Lists feed Today implementation brief

**APPROVED — implementation authorized by the user on 2026-09-06.**
The user requested 011c next on 2026-09-06, with **extra high (`xhigh`)**
instead of ultra. D-047's five-source limit and partial availability are
explicitly approved. The concrete contract is in
[SLICE_011c.md](../specs/SLICE_011c.md); the
[plain-language companion](../specs/SLICE_011c_EXPLAINED.md) explains its effects.

Branch: `codex/slice-011c-today-sources`, based on local `main` at `9d62e86`.
011b implementation `2af023c` is merged into that base and remains unchanged.
Implementation, tests and fixes are now authorized. Commit, merge, push and
deployment remain outside this approval.

## Assignment and authority

Use **Astra / xhigh** for specification and independent review, and
**Terra / xhigh** for implementation, tests and fixes. Start new runs with
these actual settings; do not reuse the earlier ultra agents unchanged.
Observed specification launches: `/root/slice_011c_spec` (`gpt-6-astra`, `xhigh`) and
`/root/slice_011c_feasibility` (`gpt-5.6-terra`, `xhigh`) and independent
`/root/slice_011c_review` (`gpt-6-astra`, `xhigh`). Following approval,
`/root/slice_011c_implement` launched with `gpt-5.6-terra` / `xhigh` as primary
production writer and sole database operator. The feasibility checkpoint and
its database cleanup are complete. The Astra reviewer remains read-only.
At the first implementation checkpoint, the original review task could not
be resumed because the agent tool reported a thread limit. The completed
specification agent `/root/slice_011c_spec` (`gpt-6-astra`, `xhigh`) therefore
took independent read-only implementation review after handing all docs to
the coordinator. Implementation remains exclusively Terra's responsibility.

Read AGENTS.md, DECISION_LOG (especially D-010/022/033/042/043/046/047), the
architecture baseline, the 011 ladder, the approved 011c specification and
`docs/prompts/05-implement.md`. Relevant contracts are in SLICE_003, SLICE_005,
SLICE_006c, SLICE_009, SLICE_011a and SLICE_011b. Inspect current code before
writing and report any genuine unresolved contract or decision conflict.

The user requested 011c ahead of the separate 011b-sort follow-up. Do not
implement saved-list sorting as an implicit dependency. Preserve the current
People filter vocabulary, saved-definition privacy and limits, and the
existing built-in Today eligibility and call-outcome behavior.

## Outcome and boundaries

An agent can connect or disconnect their visible saved lists as sources for
their own Today queue. Read the live saved definitions and current matching
People from PostgreSQL, preserving symbolic Me. Do not silently intersect
the list with an extra assignee filter or change anyone's assignment.

Today keeps existing built-in work, adds deduplicated source matches with
list reasons and an honest ordering explanation, and distinguishes incomplete
results from a fully evaluated empty queue. HTTP, Web and existing read-only
Operator tools must agree. Source-only People may have no Inquiry or contact
method; use the specification's explicit nullable fields and action semantics.

Out of scope: changing built-in rules into editable feeds (011d), tags (011e),
org-pushed sources, source selection for other users, new Operator tools,
mobile, sorting, search, snooze/dismiss, automated outreach, membership history,
new infrastructure, dependencies or a general query-builder framework.

## Single implementation lane

One primary Terra writer owns production files, migrations and serialized
database execution. It may delegate bounded test-only files after recording
exclusive ownership; no two writers edit shared files concurrently.

The primary delegated `backend/crates/crm-api/tests/db_today_sources.rs` only
to `/root/slice_011c_implement/today_source_tests`. That helper handed back
four command/privacy/quota/deletion tests after formatting, without running
database checks. Registration in `tests/all.rs`, integration, further changes
after handoff and all database execution remain with the primary writer.
The same helper subsequently owned only `web/src/api/todaySources.test.ts`,
adding seven cache/recovery/mutation/session tests and running that targeted
Vitest file plus its ESLint check. It has handed the file back; production
hooks and all subsequent integration remain with the primary writer.
It then owned only `web/src/components/AppShell.test.ts` and
`web/src/components/OperatorPanel.test.ts` for session-boundary regressions,
running both focused Vitest files (27 tests) and their ESLint check. These
files are handed back; final integration and gate coverage remain with the
primary writer.

For the remaining queue acceptance coverage, the coordinator then assigned
that helper one new exclusive file:
`backend/crates/crm-api/tests/db_today_source_acceptance.rs`. It covers bounded
filter/ordering/cap/nullable/overlap scenarios without database execution or
module registration. The helper has handed back five tests for ordering/ties,
overlap/reasons, 199/200/201 reservation, beyond-People-cap matching and
two-viewer Me/other/unassigned parity. The primary now owns registration,
integration and execution in the sole database lane.

The helper also authored two new Web acceptance test files only:
`web/src/views/TodaySourcesView.test.ts` and
`web/src/views/PeopleTodaySources.test.ts`. Its tests exposed two People source
control defects, corrected by the primary without weakening assertions. The
combined focused Vitest run (8 tests), ESLint and whitespace checks then passed.
These files are handed back to the primary for final integration and gates.

The coordinator subsequently assigned the same helper one new exclusive file,
`backend/crates/crm-api/tests/db_today_source_contracts.rs`, for remaining
strict HTTP validation, privacy, membership, grants/composite foreign keys and
five-tombstone quota recovery coverage. The helper may format the file but
must not execute database checks or edit module registration. The primary
retains registration, integration and serialized execution after handoff.
The helper handed back six ignored contract tests after file-only formatting
and whitespace checks; the primary now owns that file.

The helper's next exclusive file is
`backend/crates/crm-api/tests/db_today_source_filter_parity.rs`, covering
common-clock v1 filter parity between actual Today source reasons and People
filtering, including expected membership assertions and historical/age edges.
It may not register the module or execute database/cargo/SQLx commands.
The primary retains those integration and execution responsibilities.
The helper handed back five ignored parity tests, including paired fixed-clock
boundary checks using the primary's test-support query seams. The primary now
owns that file, its registration, metadata refresh and execution.

The helper next exclusively owned
`backend/crates/crm-api/tests/db_today_source_operator.rs` for source-enabled
HTTP/Operator consistency, ordering/absence/nullable output and name-safety
integration coverage. It handed back three ignored tests. It then authored
`backend/crates/crm-api/tests/db_today_source_races.rs` with four ignored tests
for duplicate/stale enable, opposing toggles, delete races and absence of
Organization realtime publication. Both files are now owned by the primary,
including registration, integration and serialized execution.

The helper then exclusively owned the new frozen original-query fixture
directory `backend/crates/crm-api/tests/fixtures/today_9d62e86/` and
`backend/crates/crm-api/tests/db_today_builtin_parity.rs`. It preserves the
original query, model and ranking from `9d62e86`, with only test-module/import
adaptations, and adds fixed-clock original/current serialized payload and
ordering comparisons. It may format these files but may not run Cargo,
database or SQLx commands, change registration, or edit production files.
It handed back two ignored original/current parity tests and the provenance
fixture after formatting and whitespace checks. The primary now owns these
files, their integration, metadata and execution.

The primary next assigned the helper only the new
`backend/crates/crm-api/tests/db_today_source_failures.rs` for no-hook AC8
cases: metadata outage/recovery and actual explicit-assignee validation lock
timeout/cancellation with an owned one-connection application pool. The helper
must not register or run the tests, execute database commands, or edit product
files. The primary retains fault hooks, registration, integration and every
database operation.
The helper handed back two ignored controlled-failure tests after formatting
and whitespace checks. The primary then transferred only
`db_today_builtin_parity.rs` back to the helper for the review-requested
reply/outcome precedence, multiple-viewer/tenant and exact 199/200/201 boundary
cases. The frozen fixture directory stays with the primary; the helper still
must not execute Cargo, SQLx or database commands or change registration.
The helper handed back that extension: four ignored parity tests now cover
the original mixed and overflow fixtures plus precedence/viewer/tenant cases
and exact 199/200/201 boundaries. The primary again owns the parity file.
The helper's next exclusive file is
`backend/crates/crm-api/tests/fixtures/today_http_perf_driver.rs`, a pure
authenticated HTTP workload driver. It must not launch a server, operate a
database, register a harness or change dependencies; the primary owns those
integration and execution steps. The coordinator owns the pre-execution
[HTTP protocol](../design/perf/slice-011c-http-2026-09-06/PROTOCOL.md) and report.
The driver was handed back to the primary for compilation and harness
integration. The helper then exclusively authored
`web/src/sessionLifecycle.concurrent.test.ts`: three focused tests using
independent retained coordinators for cookie/callback ordering, same-actor
replacement, interleaved probes and public auth transport during a pending
attempt. Its focused Vitest and whitespace checks passed; that file is now
handed back to the primary as well.
The helper next authored only `web/src/sessionLifecycle.recovery.test.ts`.
Its three focused tests passed for protected initial-route recovery, cleanup
of an aborted pre-dispatch attempt and actual logout transport when storage
is unavailable. It handed the file back without production changes. The
primary retains integration and the complete Web gate.
The primary subsequently assigned the helper only the new
`backend/crates/crm-api/tests/db_today_source_hooks.rs`, using the primary's
test-support-only, task-local Today checkpoints. It covers controlled snapshot
mutation, staged source SQL failure, backend termination and exhaustion of the
combined recovery budget. The helper owns no production hooks, registration,
Cargo/SQLx command or database execution.
It handed back four ignored tests after formatting and whitespace checks;
the primary now owns their registration, integration and execution.
The primary next assigned that helper only the new
`backend/crates/crm-api/tests/db_today_source_settings.rs`. It covers pooled
JIT, statement-timeout and transaction-setting restoration, plus cancellation
through a real Operator service and SQLx tool backend. Formatting and whitespace
checks are allowed; registration, integration, Cargo and all database execution
remain with the primary.
The helper handed back those two ignored settings/cancellation tests. It then
received only the new `backend/crates/crm-api/tests/db_today_source_deadlines.rs`
to pin the unchanged whole-source deadline and the shared recovery-to-commit
deadline, including a structurally invalid tail. It may use the existing
task-local checkpoints and format its file, but may not edit other files,
register or execute the tests, or operate the database.
The helper handed back both ignored deadline tests after formatting and
whitespace checks; the primary owns their integration and execution.
After the initial eight hook/settings/deadline tests passed, the primary
assigned `/root/slice_011c_implement/today_source_settings_deadlines` exclusive
ownership of `db_today_source_settings.rs` and `db_today_source_deadlines.rs`
for the review-requested stronger behavior assertions. The primary and other
writers leave both files untouched until handback. This helper may format
those files but may not run Cargo, SQLx, database or network operations, or
edit production hooks and registration. The primary retains the snapshot and
connection-loss fixture in `db_today_source_hooks.rs`.
This helper handed back both strengthened files after `rustfmt` and
`rustfmt --check`, without execution. All three files are again owned by the
primary for integration and database verification.

The coordinator then assigned `/root/slice_011c_http_harness`
(`gpt-5.6-terra`, `xhigh`) only the new
`backend/crates/crm-api/tests/db_today_http_perf.rs`. This writer integrates
the existing pure workload driver into an opt-in authenticated HTTP harness,
coordinating internal fixed-clock, router and telemetry seams with the primary.
It may format its file but must not read credentials, run Cargo/SQLx/database
commands, launch a server, make network requests, edit the driver or register
the module. The primary retains all production seams, fixture setup, runtime,
measurement execution and cleanup. The coordinator retains the protocol and
independent evidence recount.
The harness helper handed back its file after formatting and whitespace checks,
without Cargo/database/runtime execution. Its final orchestration review closed
failure retention, source-control setup, explicit sequential pool closure and
late telemetry reconciliation. The primary now owns integration, complete
source/binary provenance, measurement and cleanup.
The benchmark must stay outside the ordinary ignored database suite so
`scripts/check-db` cannot start a load measurement alongside other tests.
The primary owns its explicit registration and invocation. Independent
read-only review also compares the final fixture with the immutable Phase A
seed and actual seed summary before measurements are accepted.

The primary then assigned
`/root/slice_011c_implement/today_source_telemetry` only the new
`backend/crates/crm-api/tests/db_today_source_telemetry.rs` for captured
production-span privacy and safe-field assertions. The helper owns no
production tracing, fixture, registration or database execution; the primary
retains those integration steps. The coordinator's attempt to resume the
completed Astra reviewer during this helper run hit the agent tool's thread
limit, so that review resumes after the bounded helper hands back.

Expected owned implementation areas, narrowed to the approved specification:

- `backend/crates/crm-app/src/domain/today/` and the new typed source-setting
  command/query module; minimal ID/domain registration as needed.
- Saved-list visibility/evaluation helpers and deletion cleanup necessary for
  source lifecycle, without changing the unrelated 011b save/copy contract.
- One additive `today_work_source` migration, grants and SQLx metadata under
  the existing repository workflow. No unrelated query/cache changes.
- API Today/source adapters, errors and route wiring; existing Operator
  backend/explanations, output DTOs, static tool descriptions and prompt updates
  required by the declared output contract. Keep one shared Today read path.
- Web API types/query keys and invalidations, Today view, bounded source
  controls on existing list surfaces, and related tests. Preserve the Person
  inspector, existing filter/URL behavior and 011b drafts/retries.
- The bounded `AppShell.vue` / `OperatorPanel.vue` identity boundary and tests:
  discard private transcript, draft and proposals across actor/Organization or
  session replacement, and reject late turn callbacks and old history replay.
- Meaningful new/updated backend, route, Operator and Web tests and the
  consolidated test-module registration.

The coordinator owns shared documentation, contract amendment pointers,
acceptance evidence and integration. A reviewer is read-only and does not run
an independent database suite while the primary lane owns database execution.

## Execution and performance checkpoint

The authorized **Phase A feasibility checkpoint is complete**: a temporary SQL
harness, synthetic generated database, measurements and reviewed artifacts.
Terra alone performed database work and verified removal of its generated
database and sessions. Production API/schema wiring and the new public contracts
are now approved. SQL timings are not production HTTP measurements.

1. **Use the completed prototype evidence.** The
   [011c measurements](../design/perf/slice-011c-2026-09-06/README.md) record
   the original failure, bounded comparisons, exact payload/order parity and
   522 complete candidate requests across the history-rich 50k-Person matrix
   and raw burst repeat. The approved plan skips unused latest-source
   lookups, adds same-Organization predicates to exactly three built-in
   inquiry probes, and sets JIT off only inside the Today transaction.
   Preserve original-query fixtures and the earlier narrow pool-wait margin.
   Do not repeat Phase A merely to start implementation, infer production
   capacity, or broaden these corrections into general tuning.
2. With specification review and approval complete, inspect the final contract and
   actual tree; state a short implementation plan with
   ownership and required checks. The lane owns only the new/amended contracts
   explicitly declared by that specification.
3. Implement source persistence/commands and direct authorization tests. Prove
   the five-source cap under contention, idempotent state changes, deletion and
   enable races, invalid-source removal, rollback/tombstone quota recovery,
   actor privacy and tenant isolation.
4. Implement bounded query-time evaluation, built-in reservation, source
   membership evidence, deterministic merge/truncation, accurate nullable
   data and partial recovery. Preserve the built-in query as the comparison
   baseline; do not silently rewrite its semantics. Section 8 approves the
   three inquiry Organization predicates; retain executable original-query
   parity fixtures for complete row payloads and order. Verify that the approved
   transaction-local JIT setting is reset after success, failure, rollback and
   connection reuse. Source-clause-present/absent results must remain equivalent
   when the unused-source lookup guard is applied.
5. Validate the actual feed implementation against the accepted prototype
   thresholds, including repeated HTTP latency, pool behavior and failure
   rates; retain the distinction from the earlier SQL-only measurements.
   Follow the specification's whole-source budget/fairness and performance
   acceptance rules. The approved final HTTP p95 caps are 1,250/2,500/4,500 ms
   at concurrency 1/10/20; every normal sample must complete with no unexpected
   partial/unavailable result, source/pool timeout or HTTP 503. Whole-source
   p95 must stay below 450 ms. Include authentication, all pool acquisition,
   evaluation, merge and serialization. Keep the 250 ms enumeration, 500 ms
   whole-source, 2 s pool-acquisition and combined 100 ms recovery-grace limits.
   Paired zero-source p95 may regress by at most max(25 ms, 10%), with exact
   payload/order parity. Retain all raw samples and repeat the critical
   twenty-request bursts, reporting wait p95/max and timeout headroom.
   These targets were approved with the concrete specification on 2026-09-06.
   Do not obtain apparent speed by dropping sources,
   truncating People before ordering, silently returning partial results, or
   launching each source on another pool connection. If measured behavior
   fails the checkpoint, report the evidence and propose a bounded follow-up
   before expanding into read-model denormalization, materialization or pool
   architecture. A failed checkpoint leaves 011c incomplete.
6. Complete Web and Operator integration and tests. Include actor/session
   switches and late responses, actual window focus inside a fresh-cache
   interval, mutations and uncertain-write reconciliation, age cutoffs,
   refresh/reconnect, partial notices and safe rendering of list names.
7. Run `./scripts/sqlx-prepare`, then `./scripts/check`, then
   `./scripts/check-db` on the final implementation tree. Never overlap
   database-backed commands. Reuse applicable checks when only docs change;
   do not claim any unrun command passed.
8. Complete the specification's real-browser walkthrough against isolated
   synthetic accounts/data. Use two agents, admin and a second Organization;
   verify source controls, shared Me, other/unassigned matches, overlapping
   reasons, built-in preservation, no-inquiry/no-contact states, failure
   recovery, keyboard and narrow layout. Do not replace browser evidence
   with API-only assertions. Record and clean the exact owned runtime/data.
9. Obtain independent Astra / xhigh acceptance review against every numbered
   criterion; fix actionable findings and run the affected required checks.

## Handoff

Report exact branch/base/tree and every changed or generated file, including
SQLx metadata. Map criteria to actual tests and browser observations; attribute
each run to its runner. Record measured performance and its limits, review
findings/dispositions, temporary-service/database cleanup and remaining gates.
Do not estimate token usage or cost when unavailable.

The current phase implements and verifies the approved specification, following
[05-implement](../prompts/05-implement.md). The prior 011b commit/merge approval
does not authorize new commits, merges, pushes or deployment here.

## Takeover on 2026-09-06 (evening): Codex usage exhausted

The Codex lanes stopped with the tree uncommitted, every review finding
closed in code review, targeted tests passing, and three items open: the
Phase B authenticated HTTP run, the final-tree gates and the independent
acceptance review. The user asked Claude to finish the slice. Roles from
that point, recorded so the handoff is reproducible:

- **Coordinator, sole gate runner and acceptance reviewer:** Claude Fable 5.1
  (this session). It ran every repository/database gate itself, reviewed the
  backend, Operator and Web production code against §§2–8 of the
  specification, executed the Phase B benchmark as the sole database lane,
  and owns the documentation and evidence.
- **Lane A (Claude Sonnet 5, implementation):** the three clippy lints in
  helper-written test files, registration and repair of
  `tests/db_today_source_telemetry.rs` (two test-only defects; no production
  change), and the `sha256_hex` compile fix in `tests/db_today_http_perf.rs`.
- **Lane A2 (Claude Sonnet 5, implementation):** removal of unused API in
  `tests/fixtures/today_http_perf_driver.rs` and two style lints in the
  harness so `cargo clippy --features perf-harness -- -D warnings` is clean.
- **Lane W (Claude Sonnet 5, implementation):** the Web session-recovery
  corrections C011C-I17–I20 recorded in the verification record, with tests.
- **Lane P (Claude Sonnet 5, implementation):** the proposed fourth
  transaction-local planning change (`SET LOCAL enable_mergejoin = off`) in
  the Today read transaction, with its restoration assertions in
  `tests/db_today_source_settings.rs`, after the coordinator's Phase B root
  cause analysis; approved by the user the same evening.
- **Read-only reviewer (crm-reviewer subagent):** the independent Web
  session-privacy review whose findings F1–F5 became I17–I20 and residual R1.

Lane rules were unchanged: one writer per file set, no concurrent edits to
shared files, no lane ran `./scripts/check`, `./scripts/check-db` or any
database suite while another lane held the database, and no lane committed.
