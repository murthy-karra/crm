# Slice 010f1 — Implementation and verification

**IMPLEMENTED AND SYNTHETICALLY VERIFIED — 2026-09-11.** D-066 accepts the reviewed
[specification](../specs/SLICE_010f1.md), [brief](SLICE_010f1_IMPL.md), policies and
owned shared contracts. This record distinguishes implementation progress from
executed proof. All A1–A12 criteria pass within the synthetic scope. The worktree
is uncommitted and undeployed; shared development remains on 010c.

## Source, ownership and isolation

Baseline main `f01c2e3`; worktree `/Users/karrad/projects/crm-010f1`, branch
`codex/slice-010f1-metadata-import`. The 45 pending release/planning documentation
files were copied intact from main before approval annotations. Main's working
files and deployed 010c runtime are unchanged by this implementation.

- `snapshot_impl`: primary backend writer, sole additive migration owner, test
  registrations/SQLx metadata, narrow preflight changes and concrete contract.
- `root`: shared documentation, Web integration, the new HTTP contract test file
  and final verification. Web authoring begins after the frozen contract, in
  parallel with backend; integrated verification waits for backend review/fixes.
- Named support may own a new pure extractor or synthetic QA example; the primary
  owns shared registrations. Each support assignment is recorded at handoff.
- Independent backend checkpoint is round one; final integrated review is round
  two under D-050. Review progress and findings are recorded below.

Private verification directory: `/private/tmp/crm-010f1-qa-kzsnx80x` (0700).
Worktree `.env` is 0600 with fresh synthetic credentials, no provider keys.
Separate PostgreSQL 18.6 container `crm-010f1-postgres`, loopback port 55431,
contains `crm_010f1_tests` and `crm_010f1_qa`; roles use the existing provisioner.
Separate Centrifugo 6.9.2 container `crm-010f1-centrifugo` uses port 18081.
Future synthetic API/Web use 3016/5186. No Redis is required. The shared
`development-*` containers and `crm_dev` are outside this verification scope.

Initial PostgreSQL setup saw its temporary initialization server before database
creation. The helper then waited for an actual successful connection to the named
test database before provisioning; no application failure or shared DB operation.
Independent APFS copies of existing build/dependency caches avoid changing the
deployed artifacts. Rust 1.98.0, Node 24.16.0, pnpm 11.22.0 and sqlx-cli 0.8.6
were verified. Cargo/DB-backed runs are serialized; the backend writer owns the
initial execution slot. The dedicated Centrifugo health endpoint returned success.

## Acceptance evidence map

| Criteria | Required evidence | Current result |
|---|---|---|
| A1–A3 | Same completed parent/snapshot, raw qualification, exact fields and zero post-capture source calls | Passed: source/HTTP suites and actual browser/native API checks |
| A4–A5 | Explicit catalogs, native quotas, tombstones, separate People and absent/equal-only values | Passed: gate/source/acceptance suites; exact native API reconciliation |
| A6–A7 | Scoped revisions/cursors/receipts, exact replay and atomic crash/retry/cancel fences | Passed: HTTP/race/barrier/rollback suites and lost-response/partial-cancel browser cases |
| A8 | Private child permit, current admin/tenant proof and unchanged review hold | Passed: direct DB/HTTP guards and browser demotion/member hold |
| A9 | Exact variable-byte inventory, reservations, ceilings and exhausted cancellation | Passed: accounting suites and actual lower-ceiling/pause/explicit-resume walkthrough |
| A10 | Actual hot-query plans at 25k People/50 members and measured metadata fan-out | Passed: 90 plans / 59 SQL hashes / 246 checks; physical-size limits recorded |
| A11 | Web tests and real synthetic API/production Web walkthrough, desktop/390px | Passed: 1,054 Web tests, 51 browser checkpoints, 18 inspected screenshots |
| A12 | Sequential SQLx, repository and DB gates; compatible artifact preflight | Passed: final 900 Rust / 881 DB gates and actual synthetic DB capability preflight |

At initial setup no 010f1 checks had run. The chronological entries below retain
the status at each checkpoint, including initial failures and subsequent passes.
The earlier 010c release checks do not prove this implementation.
Record exact frozen source, commands, logs/counts and any corrections below as
work proceeds. Do not invoke live FUB or turn on operational imported workspaces.

### Development progress

The pure-extractor owner reports 15 passing focused lib tests; exact final-tree
gates remain pending. Root authored seven HTTP cases with real retained captures
and completed parent imports, including current authority, foreign scope, strict
bodies, revisions/cursors, exact confirmation replay and >2-MiB field segments.
The HTTP file passed standalone rustfmt but has not compiled or executed yet.
The scoped QA example is authored and formatted; no synthetic API is running.
Private setup-driver JavaScript syntax check passed; it has not made API calls.

The initial Web implementation and integration passed type checking and scoped
ESLint. Focused tests passed: 9 API transport, 11 real mapping-panel, 15 main-panel,
4 segmented-field and 5 Person-provenance cases. The main-panel cases cover exact
lost-response replay, dirty mapping confirmation guards, terminal cancellation,
current administrator access and progress polling. A regression batch found an
older Person-view test mock returning an unrelated shape for the new provenance
endpoint; the mock now returns the actual empty-page contract and the affected
suite passed without unhandled errors. Inspection of real backend source summaries
also found and fixed missing `tag`/`choice` labels in the Web caption helper;
focused caption regressions are being added before final gates.

The first coordinated DB run passed 19 of 20 cases and exposed a command accepting
an existing-field mapping for a source machine-key collision. Execution held the
data, but immediate command rejection was required; that check was corrected.
Preparation also now continues in at most 50 tag/option descriptors per unit. Both
subsequent combined runs passed all 21 cases, including a 121-tag/121-option case,
on the final backend checkpoint. The final run took 117.65 seconds after a
36.09-second build. Scoped Clippy passed without diagnostics. The independent
backend round-one reviewer is inspecting a frozen 21-path inventory; no production
edits are permitted during that read. The opt-in 25k-Person/50-member query collector
remains in progress.

A preliminary independent Web pass found three issues, now corrected with focused
regressions: same-plan preparation freshness, incompatible filters crossing read
modes, and narrow-layout wrapping for the full accepted source-ID length. See
[review dispositions](../design/qa/slice-010f1-2026-09-11/IMPLEMENTATION_REVIEW.md).
The Web type check passed again after these changes. The initial counts above
describe their particular runs; final repository-gate totals will be recorded
after the integrated review and all resulting changes.

The private real-browser driver is authored and syntax checked. It uses the actual
production Web build and a compile-fenced synthetic API, no live FUB. The QA
executable can grant a fixed number of actual `metadata_worker::run_once` units;
its private counter records only `Ok(true)` and includes phase/pause units. This
test-only schedule enables deterministic partial cancellation without modifying
the production worker or bypassing database permits. Browser execution is pending.

### First repository gates (before backend review dispositions)

`./scripts/sqlx-prepare` passed against a fresh throwaway database on isolated
port 55431, applying all migrations and preparing the offline cache; the 32.30-second
compile produced no `.sqlx` change. The following `./scripts/check` passed in
64 seconds: 900 Rust tests, 5 doctests, 1,053 Web tests, 19 compatibility-preflight
tests and 11 email-worker tests. Formatting, Clippy, production-shape compilation,
dependency fences, Web lint/typecheck and the production build passed in that run.
Private logs are `sqlx-prepare-1.log` and `check-1.log` in the QA directory.

These are development gates on the frozen backend checkpoint plus corrected Web.
The full DB gate, opt-in query plans and real-browser evidence remain pending;
review fixes will receive the relevant repeated checks before final acceptance.

### Review corrections and frozen integrated checkpoint

The five backend round-one findings are corrected. Existing metadata DB checks
passed 21/21 after the main corrections. The new seven-case regression suite first
passed five cases; two fixture assertions confused a qualified manifest with its
held execution result, and canonical numeric evidence `1e0` with raw spelling `1`.
Those expectations were corrected without changing production behavior, and the
full seven-case rerun passed in 23.33 seconds. It includes interrupted ancestor
choice inheritance, PostgreSQL-native Unicode collision handling, unresolved custom
properties, malformed/no-op tag collections, local counts and revoked executor
adoption. The 24 focused library cases passed again.

Round two corrected per-unit storage wording, metadata-exclusion wording and its
owned DTO field (`metadata_excluded_people`), and narrow tag-occurrence layout.
The final source-requalification DB regression passed after the DTO rename,
proving the renamed count includes child source holds but not definition-only cell
holds. Scoped Clippy with warnings denied passed in 18.58 seconds; formatting and
whitespace checks passed. The 30 Web mapping/main-panel tests passed after the
layout/copy changes; typecheck passed after the DTO rename. The final synthetic
example and optional collector compile successfully; 74 known dead-code warnings
come from existing optional Today/performance inclusions in the collector build.

The complete correction checkpoint is retained privately as
`backend-review-corrections-checkpoint.json`. Its 24-path source manifest hashes to
`4b9140ed1bd9d80a8e183fa0587608a9dd749acb27b4b0eaa51b12ed82ae4373`.
The integrated 44-path manifest `integrated-r2-source-sha256.json` hashes to
`89877064fea823930d69298484972bdf04317445c43ffa399ccdda62e47d3753`.
Neither is a commit: the baseline remains main `f01c2e3`. Final prescribed gates,
actual query-plan/size collection and browser evidence still follow this checkpoint.

### Prescribed gates on the reviewed production tree

After the independent integrated review returned READY, the required sequence
passed: `./scripts/sqlx-prepare` (26.92-second compilation, no `.sqlx` changes),
`./scripts/check` (62 seconds) and `./scripts/check-db` (451 seconds). The repository
gate passed 900 Rust tests, five doctests, 1,054 Web tests in 72 files, 19 preflight
tests and 11 email-worker tests, plus formatting, Clippy, production compilation,
boundary checks, lint, typecheck and the Web production build. The database gate
passed all 873 tests in 396.805 seconds, including all 28 current metadata cases.

Nextest marked one existing service-free capture authorization test as leaky
while still passing the repository command. A single justified targeted recheck,
`capture::dismiss_unmatched_without_cookie_returns_401`, passed in 0.018 seconds
without a leak classification. The database run reported one slow test and no
failures. Its production-only SQLx check emitted the existing potentially-unused
query-cache warning; the prescribed preparation produced no cache diff.

Logs are retained privately as `sqlx-prepare-final.log`, `check-final.log`,
`check-db-final.log` and `capture-leak-recheck.log`. The production Web build
manifest covers 72 files / 2,453,565 bytes and hashes to
`3013891ceb4ea97107b5297815be2212379ef24de1890976068a0d6b90df92dd`.

An acceptance-evidence audit identified indirect rather than direct proof for
plan expiry, running/cancelled parents, source-binding conflicts, target
tombstones, native business-row immutability and in-flight Person-unit/cancel
atomicity. Two test-only files add eight direct cases without changing reviewed
production behavior. All eight plus six adjacent tests passed in the focused
14/14 run (40.24 seconds after a 30.63-second build); scoped Clippy passed in
18.38 seconds. The checkpoint `backend-acceptance-checkpoint.json` hashes to
`5f1678708ee9a0d3c81fad0a29dc01a89f6f6b2cea5f4fde90b4caf3bcafcec2`.
The earlier 873-test gate does not include these eight new cases. The final
prescribed repeat remains required. See [the acceptance map](../design/qa/slice-010f1-2026-09-11/ACCEPTANCE_MAP.md).

### Actual query-plan collection

The first collector run populated 25,000 People / 50 members, emitted 90 actual
plans across 59 distinct SQL hashes (62 static call sites), and failed. PostgreSQL
18 encoded integral `Actual Rows` values as JSON floats, which the collector's
integer-only reader rejected. Four real scan-bound failures also exposed fixture
bias: monotonically grouped metadata UUIDs led the planner to select global
primary-key scans. Production assigns random UUIDs. The diagnostic correction
uses UUIDv4 fixture rows with rank-derived continuation anchors and accepts
integral JSON row counts. Every production SQL hash, cardinality and original
bound remains unchanged; no planner setting is forced. The failed raw log
`metadata-plans-1.log` is preserved. The corrected run passed all 246 checks across
90 plans / 59 SQL hashes in 88.85 seconds, including 87.851 seconds of fixture
preparation, after a 30.06-second compile. Each of the four formerly failing
pages now examines exactly 51 rows through its existing scoped index. No
production query, index or contract changed. See [actual plans and storage
observations](../design/qa/slice-010f1-2026-09-11/PERFORMANCE.md); physical relation
sizes and the tiny real child's logical ledger are reported separately.

### Final repeat with all acceptance tests

The final frozen tree passed the prescribed sequence again:
`./scripts/sqlx-prepare` (16.16-second compilation, no cache changes),
`./scripts/check` (54 seconds) and `./scripts/check-db` (388 seconds). The counts
are 900 Rust tests, five doctests, 1,054 Web tests in 72 files, 19 compatibility
preflight tests, 11 email-worker tests and **881 database tests**, including all
36 metadata cases. DB execution took 364.314 seconds; one test was slow and none
failed. No leak was reported in this final sequence. Formatting, Clippy, boundary
checks, production compilation, lint/typecheck and Web build all passed.
Logs use the `*-acceptance.log` suffix. The explicitly requested QA example build
also passed; its API serves only the isolated database on port 3016.

### Synthetic startup and browser verification

The guarded QA executable applied migrations only to `crm_010f1_qa`; actual
`/api/health` and `/internal/ready` checks passed. An initial probe mistakenly
used `/health` and returned 404 before the correct route was read. Production
Web preview is served on port 5186. Three initial review workspaces were created
through ordinary API commands; all setup sessions were revoked. A fourth fresh
workspace is used for the final complete browser rerun.

The actual release-preflight script passed against the isolated database's three
then-current review bindings. Its synthetic executable/in-process worker/startup
migrator inventory includes the actual binary hash and uncommitted source-base
identity. Removing only the metadata capability leaves ordinary 010c confirmation
ready but sets metadata readiness false with `metadata_capability_missing`.
The report was not installed into any runtime or used as release authorization.
The QA executable uses compile-fenced test readiness. See
[synthetic preflight evidence](../design/qa/slice-010f1-2026-09-11/checks/synthetic-preflight.json).

Initial real-browser execution completed metadata after source disconnect,
replayed identical confirmation bytes after a lost response, and preserved the
parent/review binding. A driver regex then missed the leading whitespace in a
visible 128-digit source-ID paragraph. A resumed check proved the exact ID and
administrator demotion, then used a nonexistent `main` element for a screenshot.
The private driver now handles the whitespace and captures the actual document
body. Visual inspection also found two screenshots captured during loading; it
now awaits the mapping rows and the next full-field segment. These corrections
change only verification tooling. The full rerun in the fresh workspace passed
all 30 checkpoints. Partial cancellation passed ten checkpoints. The first
budget-queue driver read state before its confirmation response returned; it now
awaits HTTP 202 and reuses the already confirmed child. Its corrected three-check
queue phase and both four-check pause/resume phases passed. Failed attempts remain
published separately from the five final passing reports.

All **51 final browser checkpoints** passed with zero browser page errors and
zero post-capture source-reader calls. Counters were 68→68 before API restarts and
0→0 afterward; each process resets the private counter. The child still made no
source request after source disconnect. Eighteen desktop/390px Web screenshots
were visually inspected, including loaded exact UTF-8 segments, readable dialogs,
128-digit identities, tag aliases, provenance, member/demotion holds and recovery.
These are responsive Web checks, not native Swift/Kotlin app work.

The final read-only native API audit checked both completed fixture imports:
25 created tags, five created fields, two created options, eight applied Person
tag links and eight applied values across five distinct People. Exact per-Person
tag sets and text/number/date/literal-choice values match the fixture. The
21-tag Person has no partial tag set; invalid data remains held; no Inquiry/task
was synthesized. Parent counts/plans and workspace revisions are unchanged.
Cancellation retains one settled Person and releases both reservations. All audit
sessions were revoked. See [native reconciliation](../design/qa/slice-010f1-2026-09-11/checks/native-api-reconciliation.json).

### Final evidence and cleanup

The [QA index](../design/qa/slice-010f1-2026-09-11/README.md) links all reports,
18 screenshots, measured plans, review dispositions and source/build/log hashes.
Twenty selected logs were checked against private environment values before
publication; none required redaction. The final source manifest covers 975
non-documentation repository files, and the Web artifact manifest covers 72 files.
Production source remained identical to the reviewed checkpoint; only the added
acceptance tests and diagnostic collector fixture changed after that review.

The isolated API and Web preview were stopped. Both dedicated containers and the
anonymous PostgreSQL volume were removed, and ports 3016/5186/55431/18081 have no
remaining listeners. Main remains `f01c2e3` with its earlier documentation-only
working changes. The 010f1 worktree, build outputs and private verification logs
are retained for the later integration handoff. No shared-development service or
customer data was modified. [Cleanup proof](../design/qa/slice-010f1-2026-09-11/checks/cleanup.json).

## Publication and remaining boundaries

Implementation approval does not authorize commit/merge/push or another deployment.
Sanitized evidence is prepared in the uncommitted worktree for the later
integration step. Notes/tasks, repair/delta imports, activation,
live FUB validation and customer-data readiness remain separate work.

Publication cleanup normalized trailing whitespace in six checked log copies.
Their original private hashes remain recorded alongside the published hashes;
no test result or source byte changed.
