# Slice 010c — Implementation verification

**IMPLEMENTED AND SYNTHETICALLY VERIFIED, 2026-09-11.** D-065 authorizes implementation and synthetic
verification of the [approved spec](../specs/SLICE_010c.md) and
[brief](SLICE_010c_IMPL.md). All required implementation checks passed.
Live FUB validation remains user-deferred. No release or real-data processing.

Final status: both implementation reviews and their fixes are closed;
the final backend gates, all 59 query plans/258 assertions and the single paired
reader benchmark passed. The latest repository check also passed, including
1,002 Web tests in 66 files. The production-build browser walkthrough passed all
50 recorded checkpoints, including six states at six viewport widths. The
[evidence summary](../design/qa/slice-010c-2026-09-11/README.md) is the concise
handoff. Sections below preserve development chronology; their earlier
pending statements describe those checkpoints, not the current status.

## Baseline and ownership

- Baseline main `c6c5930`, deployed 010b source `89471f0`.
- Branch `codex/slice-010c-people-import`, worktree
  `/Users/karrad/projects/crm-worktrees/010c`.
- Eight task-owned planning files were copied byte-for-byte from main and verified
  before only those original changes were restored/removed. Main was then clean.
  Approved planning and implementation remain together, uncommitted in the worktree.
- `snapshot_impl` is primary code/database owner; bounded `source_profile`
  extractor/display/preflight/source-test and `snapshot_harness`
  performance/example ownership are recorded in the brief. Root owns remaining
  docs, HTTP workspace/import tests, private browser QA and coordination.
  Review/check gates remain sequential and at most two implementation review rounds.
- Agent model/effort is inherited; effective settings and billed usage are not
  exposed by the returned evidence. No model/cost claim is inferred.

## Setup and preparation

- Read AGENTS, decisions, complete reviewed spec/brief, current-code inventory,
  architecture, implementation prompts and existing test/script precedents.
- Recorded full approval as D-065. Concrete backend DTO/schema/accounting
  details are frozen in [SLICE_010c_CONTRACT.md](../specs/SLICE_010c_CONTRACT.md).
- Isolated APFS clones of `backend/target` and `web/node_modules` completed.
  No shared-main artifact symlink. Pinned runtime is Node 24.16.0 / pnpm 11.22.0
  under `/Users/karrad/.nvm/versions/node/v24.16.0/bin`; ambient versions differ.
- Existing PostgreSQL/Centrifugo containers were observed healthy. Created the
  disposable `crm_slice010c_qa` database and applied only baseline migrations
  through `20260917000001`. Created empty `crm_slice010c_test_master` for tests.
  No migration or reset was applied to shared `crm_dev`.
- Private QA directory `/private/tmp/crm-010c-qa-0e4vdxyr` is mode 0700; its env
  is 0600, has fresh synthetic session/content/seed keys and disabled provider
  credentials. Browser API/Web ports are 3012/5182. The worktree's private `.env`
  uses the separate test-master URLs and provider-disabled settings.
- Read-only fixture consultation identified real accepted-capture helpers and
  negative cases; the old direct-seeded snapshot scale fixture cannot establish
  raw import fidelity. The performance consultation identified the existing
  authenticated paired harness, required c6c5930 freeze and missing contact rows.
- Verified Node 24.16.0 / pnpm 11.22.0 and local Chrome/Playwright availability.
  The private browser support script passed Node's syntax check. This does not
  count as a browser workflow pass; no QA API/Web process has started yet.

## Development evidence (chronological)

Status at this checkpoint: backend round 1 is closed READY and the integrated backend/Web
checkpoint is frozen for independent round 2. Focused checks below are historical
development evidence; the three final scripts, actual plans/performance and
synthetic browser execution remain pending.


The primary reports all nine original `import_source` unit tests passing after
the additive migration applied to the isolated test master. The explicit
`isTrash: true` regression and display helper additions are subsequent work and
must be included in the next focused run. No implementation review has completed.
Baseline migration application and environment setup above are setup evidence,
not verification of the new implementation. Required final gates remain
`sqlx-prepare`, `check`, `check-db`, the relevant paired performance/plan check,
and synthetic real-API/production-Web browser verification.

Bounded support evidence:

- Harness writer verified all fourteen frozen SQL files byte-for-byte against
  Git `c6c5930` and their manifest; owned Rust formatting passed. Compilation and
  the paired measurement have not run yet.
- The example augments the unchanged 010b fixture with source 104 (contactless,
  unfamiliar stage) and 105 (2,687,014-byte source field, primary-before-dedup
  contacts). The source page is 2,820,076 bytes. Expected post-choice results are
  four imported People/eight contacts, with source 103 held as Trash. These are
  fixture expectations, not observed runtime results.
- Root authored five HTTP workspace tests in `db_workspace_http.rs`; formatting
  passed. They exercise ordinary response/mutation surfaces and governance using
  synthetic negative review-mode fixtures, not production entry. Their compile
  and DB run remain pending the primary's exclusive build/database slot.
- Documentation whitespace validation (`git diff --check -- docs AGENTS.md`)
  passed during this checkpoint. Final-tree validation remains required.
- Root amended the historical 010b release rollback paragraph with the durable
  010c compatibility restriction. A bounded support writer owns a new standalone
  release preflight plus synthetic tests; the primary retains startup/confirmation
  integration. No release process was changed or executed.
- Private browser setup/API support scripts pass Node syntax checks. They can
  create synthetic Organizations/snapshots through ordinary local HTTP and inspect
  bounded record/field pages. No browser/runtime workflow has run yet.
- Rechecked original main: clean. Work remains isolated in the 010c worktree.
- The standalone release-preflight suite passed all fourteen synthetic Python
  tests. Its database reads are mocked; no runtime/process or release operation
  occurred. The full check script now includes that suite.
- The integrated backend passed `cargo check -p crm-api` in 14.97s with no
  warnings. This precedes integration corrections and is not the final gate.
- The primary subsequently reports all seventeen extractor/display unit tests
  passing, including the explicit Trash regression.
- Root authored five retained-capture HTTP import tests in `db_import_http.rs`.
  The source support writer authored eight DB source/import cases and a shared
  typed fixture. Both new modules passed formatting. Execution evidence is
  recorded separately as the primary finishes the focused suites.
- A [release compatibility runbook](SLICE_010c_RELEASE_PREPARATION.md) records
  the operator-owned artifact/inventory/preflight and compatible-only recovery
  procedure. It is preparation for a later authorized release; no deployment
  or real fleet certification was performed.
- Additional bounded proof ownership: `source_profile` writes
  `db_import_gate.rs` for actual confirmation barriers, direct guards, lease/
  executor changes, accounting and storage/cancel recovery. `snapshot_harness`
  writes `db_workspace_background.rs` for actual Operator task lifetime, signed
  ingress/extraction and terminal-call paths. The primary owns direct-reader
  ordering/query-plan tests. These assignments share no production or test files;
  primary registration/build/DB execution remains sequential.
- Those support files are now frozen and formatted: nine gate cases and four
  background cases. The background tests use actual provider/route/worker/
  settlement paths; forced held fixtures are explicitly negative states and do
  not claim legal entry with existing calls or business data. Their compilation
  and execution remain pending the primary's coordinated run.

Focused development run 1:

```sh
SQLX_OFFLINE=true cargo test -p crm-api --test all --features test-support --locked db_workspace_http:: -- --ignored --nocapture --test-threads=1
```

Used the isolated test-master migrator URL, not shared `crm_dev`. Compilation
completed in 1m15s; five cases ran in 14.09s: three passed and two failed (918
filtered). Passing cases cover Person/note/task/tag mutations, current admin/
member/tenant/session behavior, and Today/Operator/realtime with zero inference
or admission rows. The settings fixture sent `choice` instead of the existing
`option_id` key; corrected in the test. The Today-feed command correctly blocked
the update but its domain error was converted to 503 instead of the approved
409; the primary owns the mapping correction. A focused rerun remains pending.
Private log: `/private/tmp/crm-010c-qa-0e4vdxyr/http-workspace-focused.log`.

Focused import development run (primary runner):

```sh
SQLX_OFFLINE=true DATABASE_URL=$MIGRATION_DATABASE_URL cargo test --manifest-path backend/Cargo.toml -p crm-api --test all --features test-support --locked db_import_ -- --ignored --nocapture --test-threads=1
```

Private log: `/private/tmp/crm-010c-import-focused-2.log`. The run completed in
69.66s with thirteen passes and one fixture failure: all five root HTTP import
cases and all eight source integration cases passed. The new direct-reader case
passed list/search/by-ID/contact-order assertions before its synthetic task
INSERT omitted the required `origin`; the primary corrected that fixture. Earlier
iterations exposed strict unit-variant decoding and unnecessary stage-lock
privileges, both fixed before this passing import/source run. Cancellation receipt
reservation and refreshed readiness evidence were subsequent changes and require
the next focused run; this is not final-tree evidence.

The subsequent policy-threaded run, `/private/tmp/crm-010c-import-gate-2.log`,
finished twenty passes and three failures in 94.79s. HTTP/source cases and the
corrected direct-reader case passed. Two new capacity fixtures attempted to lower
a stored allowance below its immutable original allowance and were rejected by
the existing schema constraint. The ceiling case observed an intermediate running
phase where it expected admission to have paused already. These are under the
source test owner's investigation; the stored-allowance constraint is preserved.
Six other new gate cases passed, including the independent exact SQL byte
inventory. A clean focused rerun remains required.

The corrected run in `/private/tmp/crm-010c-import-gate-3.log` passed all
twenty-three import HTTP/source/gate/reader cases (927 filtered): compile 18.88s,
execution 95.11s. Root inspected the result. Stored allowance floors remain
unchanged; effective current ceilings drive exhaustion fixtures, and bounded
worker steps reach actual admission before asserting a pause. No atomicity,
accounting or cancellation assertion was removed. The strengthened reader
fixture has 25,000 People and 100,000 ordinary contacts with indexed lookups.
Comprehensive final plan/paired performance evidence and final full gates remain
separate pending requirements.

The primary's workspace/background run in
`/private/tmp/crm-010c-workspace-background.log` passed seven of nine cases.
Actual Operator lifetime/scheduling and inbound rejection/extraction cases passed.
The call fixture expected a top-level cancelled state instead of the existing
failed outcome with cancellation reason; its owner is correcting that assertion.
Address rotation returned generic 503 for a correctly blocked workspace write;
the primary is correcting its required 409 mapping. Both corrections require
focused reruns. The new `db_import_plans.rs` final-scale query-plan collector is
assigned to the harness owner, with production SQL/index changes and execution
remaining with the primary; no additional performance run is implied.

The corrected workspace run in
`/private/tmp/crm-010c-workspace-background-2.log` passed all nine cases in
23.89s (compile 22.14s). Together with the current import run, all thirty-two
focused backend integration cases pass. The primary also reran the fourteen
preflight Python tests successfully. Scoped all-targets Clippy and the formal
backend review are the remaining checkpoint gates; the three full final scripts,
Web, final plans/paired measurement and browser verification have not run yet.

## Backend review round 1 — complete

The fresh read-only `backend_review` agent began the first formal implementation
review after [the backend checkpoint](../design/qa/slice-010c-2026-09-11/BACKEND_CHECKPOINT.md)
was frozen. Root independently verified its SHA256
`4c45a11cce404ee01ecce496df9f75cdb63304f6cdd52a1ccb07e990a462c061`
and all 147/147 listed source hashes. The primary has stopped backend edits.
Reviewer preparation earlier was documentation-only, not a formal review or
verdict. The unregistered final-plan fixture remains outside this checkpoint.
The reviewer returned **fixes required**: four consolidated findings plus a fifth
mapping-field gap confirmed in a targeted continuation. All five P2 findings
are being corrected; none is deferred. Details are in
[the implementation review](../design/qa/slice-010c-2026-09-11/IMPLEMENTATION_REVIEW.md).
Web remains pending targeted backend correction confirmation. The checkpoint
manifest records the reviewed source, not the subsequent fix tree.

Root added a sixth HTTP import case using actual artifact/database-bound synthetic
release evidence: first confirmation with a current report, replay after forced
expiry, altered-input conflict, current member denial, genuinely new confirmation
denial and replay after file removal. The existing cancellation case now compares
response/replay accounting with subsequent GET and the committed ledger. Formatting
passed; execution remains pending the primary's coordinated focused run.

The correction run `/private/tmp/crm-010c-r1-import.log` now passes all
twenty-seven import cases: ten gate, six HTTP, ten source and one reader
(927 filtered); build 41.41s, execution 116.47s. Root and reviewer independently
read the completed result. The log SHA256 is
`0e756b165a4dc7d71c6be4bdc00d90c7b51ef471413824975f673e3998896d4b`.
This includes exact long mapping-field reconstruction/scoping, supporting NUL
handling, bounded metadata contention/resource release and both new HTTP
regressions. The reviewer found no remaining static code blocker; scoped lint
and final correction source hashes were then verified. Scoped Clippy passed in
18.96s, formatting and whitespace checks passed. Root and reviewer independently
verified all 147 combined source hashes, three correction log hashes and the
[correction checkpoint](../design/qa/slice-010c-2026-09-11/BACKEND_R1_CORRECTIONS.md)
SHA256. Targeted confirmation returned **READY for Web**, closing all five
findings within round 1. No finding was deferred. Web is now underway under the
separate new-component/shared-integration file ownership; final gates remain pending.

## Focused Web development checks

The coordinator owns `ImportFieldViewer.test.ts` and
`PersonImportProvenance.test.ts` after the support writer's acknowledged handoff;
the support writer retains their production components. Eight tests use the
real QueryClient and API mocks to prove escaped retained text, UTF-8 segment
positions/next/previous, explicit retry, resource-change abort and late response
discard, current-role loss/member denial, absent provenance and explicit field
inspection. The initial run passed seven and failed one overstrict assertion:
a disabled mounted query observer recreates an empty query shell. The corrected
assertion requires no cached private data, no additional fetch and idle status,
alongside the retained UI/late-response checks; it does not require the absence
of an empty observer entry. Synthetic segment lengths were also made consistent.

Final focused command (pinned Node 24.16.0 / pnpm 11.22.0, `web/`):

```sh
pnpm exec vitest run src/components/migration/ImportFieldViewer.test.ts src/components/migration/PersonImportProvenance.test.ts
pnpm exec eslint src/components/migration/ImportFieldViewer.test.ts src/components/migration/PersonImportProvenance.test.ts --max-warnings 0
```

Both pass: 8/8 tests in 635ms and zero lint findings. Logs:
`/private/tmp/crm-010c-web-fields-final.log` and
`/private/tmp/crm-010c-web-fields-lint.log`. Whitespace validation passed.
These are focused development results; integrated/final-tree Web and browser
checks remain pending. The private production-preview and browser layout,
confirmation-response-loss and late-role-change helpers pass Node syntax checks
only; no browser runtime workflow has run yet.

The expanded shared Web run passed 429 tests across 16 files in 5.37s, with no
unhandled errors (`/private/tmp/crm-010c-web-shared-4.log`, independently read by
root). It covers existing session recovery/concurrency and Today prefetch plus
workspace request/cache fencing, coalesced verification, mode/role routing,
admin read-only People/Person controls, member waiting/logout and active/pending
call teardown. Earlier fixture corrections were a broad Person mock returning
the wrong shape for the new provenance request and platform-only identities
attempting the tenant Today route; the corrected tests exercise their authorized
platform recovery route. No authorization rule was relaxed.

Support's final focused run passed all 34 tests across six import API/component
files in 1.05s, including the coordinator's eight cases. Owned-file ESLint and
whole-project `vue-tsc --noEmit -p tsconfig.app.json` passed. Logs are
`/private/tmp/crm-010c-web-support-vitest.log`,
`/private/tmp/crm-010c-web-support-eslint.log` and
`/private/tmp/crm-010c-web-support-typecheck.log`; support froze 34 owned files in
`/private/tmp/crm-010c-support-r2.sha256`. Root independently read the test result.
The registered opt-in plan/performance target initially found two SHA256
formatting helper compile errors, corrected in those helpers. The replacement
service-free compile passed in 33.38s. It retained 74 dead-code warnings (36
duplicates) from the existing shared Today performance driver; these are recorded
in the integrated checkpoint. Final helper edits were Rust 2021 formatting only.
No query-plan or performance measurement has run yet.

## Integrated review round 2 — complete

The [integrated checkpoint](../design/qa/slice-010c-2026-09-11/R2_CHECKPOINT.md)
freezes the approved backend, Web, registered opt-in plan/performance collectors
and concrete contract. Root independently verified all 207 source hashes and
eight log hashes in `R2_SOURCE_SHA256.json`, plus checkpoint SHA256
`7a35c6fd7c06a3443632a5c98342f4f7b31e7216b0c5cb342b46de7a1cf52b2a`
and manifest SHA256
`f35f3d5e6487debb107776898c59b4b1d801260d8a25f8cc9478d3be8b37773f`.
The final authority subset passes 123 tests across eight files after the
fail-closed-before-`/me` correction. Current Cargo formatting, Git whitespace,
project TypeScript and scoped Web lint checks pass. Counts overlap with the
focused suites above and must not be added as independent totals.

The independent reviewer has been assigned round 2 with all production sources
frozen. It returned three P2 BOUNDARY corrections: unapplied mapping drafts can
mislead confirmation, ordinary routes can exhaust retries before slow successful
workspace verification, and visible results can lag behind completed progress.
The support writer owns mapping/results corrections with real-child regressions;
the primary owns the shared route lifecycle correction. All findings are being
fixed. Targeted correction confirmation returned **READY for final validation**,
closing all three findings within this second/final bounded review. Root and
reviewer independently verified the 208 corrected source hashes, seven correction
logs and checkpoint/manifest hashes. All 143 backend entries are unchanged.
The current 69 authority/route tests and 39 import tests pass, plus TypeScript,
scoped lint and whitespace checks. See the [correction checkpoint](../design/qa/slice-010c-2026-09-11/R2_CORRECTIONS.md)
for exact commands and hashes. Final sequential gates are now underway.

## Sequential gates — passed before plan correction; final repeat pending

- `./scripts/sqlx-prepare` passed against its fresh disposable database; offline
  cache generation finished in 37.19s. All 208 reviewed source hashes remain
  unchanged afterward. Log: `/private/tmp/crm-010c-final-sqlx-prepare.log`.
- `./scripts/check` passed in 57s: 877 Rust tests (845 DB tests intentionally
  skipped for this service-free gate), five compile-fail doctests, 997 Web tests
  across 65 files, 14 Python preflight tests and 11 email-worker tests. Rust
  formatting, Clippy with warnings denied, production-shape compile, crate
  boundary checks, Web lint/TypeScript and production build all passed.
  Log: `/private/tmp/crm-010c-final-check.log`.
- `./scripts/check-db` passed in 382s: all 845 database-backed tests passed
  (329.116s test time, one slow test; 877 service-free tests skipped). The fresh
  schema/cache check and Centrifugo health precheck passed. Its production-only
  SQLx check emitted the non-failing unused-query warning for the cache generated
  with all targets; the all-targets preparation left reviewed cache hashes
  unchanged. Log: `/private/tmp/crm-010c-final-check-db.log`.
- The three scripts ran sequentially with no other database workloads. Actual
  comprehensive query plans, paired performance and synthetic browser checks
  follow these successful gates.

## Executed query plans — correction in progress

The first opt-in collector run compiled in 1m04s and failed after 114.42s at
its first plan. The claim query scanned 257 import metadata rows (16 shared
blocks, 0.083ms relation time; 0.172ms overall). No complete plan pass or paired
performance measurement is claimed. Log: `/private/tmp/crm-010c-final-plans.log`.

A read-only diagnostic reused that retained disposable fixture, confirmed its
25,000 People/257 imports, and ran both equivalent SQL forms as `crm_app`.
Stating the existing candidate state set directly made PostgreSQL use the
existing `migration_import_claim` partial index and plan primary key: one row
each, 0.054ms versus the original 0.144ms diagnostic. These isolated plan times
are diagnostic evidence, not the paired performance benchmark. Evidence is in
`/private/tmp/crm-010c-qa-0e4vdxyr/claim-diagnostic.json`.

The primary is applying that behavior-preserving predicate. Strict indexed and
bounded claim assertions remain. The collector owner is correcting its small
`app_user` count (50 Organization members plus one platform fixture user) and
reusing one synthetic password hash/pool for 48 users that are never logged in.
No supported cardinality is reduced. Rerun comprehensive plans, then repeat the
three final scripts after all resulting source corrections, before the one
paired measurement and browser run.

## Workspace seam matrix — final suite passed

The HTTP cases below use a deliberately constructed negative review state after
ordinary HTTP fixture creation. They test enforcement and unchanged contents;
they do not certify that a populated Organization may enter review. Import
confirmation and race tests must independently prove the real transition.

| Seam | Operational/control evidence | Review evidence | Status |
|---|---|---|---|
| People list/detail, stages, tags, fields, lists, tasks, source choices | Both fixture roles get 200 | Member denied; current admin reads; foreign Person stays 404 | Final database suite passed |
| Session login/me/logout and role changes | Mode operational, revision `1` | Mode/revision returned; promotion/demotion affects old cookie; logout revokes it | Final database suite passed |
| Today/read sources/feed view, Operator, realtime token | Today and token succeed | Both roles denied; scripted inference calls/admissions/publications unchanged | Final database suite passed |
| Assignment, stage, contact attempt | Person created via intake command | Admin mutations denied including no-op stage/unassignment | Final database suite passed |
| Notes/tasks | Real add/create fixture | Edit/delete/complete/reopen/snooze denied; full Person response/events unchanged | Final database suite passed |
| Tags and custom fields | Real definition, Person tag/value | Add/remove/rename/delete/archive/order/clear denied | Final database suite passed |
| Saved lists/Today controls | Real list and active Today source | Create/edit/delete/source enable-disable/feed update-revert-enable-preview denied | Final database suite passed |
| Existing unresolved intake | Real unresolved HTTP payload | Retry/discard denied; original retained response unchanged | Final database suite passed |
| Routing/address settings | Read original routing settings | Update/rotation and GET self-provisioning denied | Final database suite passed |
| Identity/membership governance | Ordinary identity helpers | Invitation acceptance provisions only new member; review mode persists; deactivation invalidates session | Final database suite passed |
| Exact empty target, concurrent confirmation/read/write, direct-domain guards | Nine gate cases and reader case | Real confirmation barriers, forged-permit denial, active/expired work distinction, executor revalidation | Final database suite passed; live release evidence remains separate |
| Inbound raw acceptance, call terminal cleanup, actual Operator admission/deadline | Four background cases | Held requests persist no raw data; extraction skips held jobs; terminal-only cleanup; admission retained until task termination | Final database suite passed |

The concrete HTTP test names are in
[`db_workspace_http.rs`](../../backend/crates/crm-api/tests/db_workspace_http.rs).

## Remaining work at the first final-gate checkpoint

The then-pending round 2, final sequential gates, comprehensive query plans and
single paired performance run subsequently passed as recorded below. Production
Web verification and final evidence reconciliation are the remaining work.
Live-source qualification, customer-data readiness, activation, commit/merge/push
and deployment remain outside this implementation authorization.

The second collector run compiled in 28.58s and ran for 98.71s. Its first
27 executed plans passed, including the corrected indexed claim, supporting
sources, dense overlap, immutable identities and first-page record filters.
The unfiltered result page then failed: 25,000 manifest and 22,500 result rows
were scanned to return 51 rows (1,469 shared blocks, 18.561ms). This is an
observed paging defect, not a small-table assertion issue. The primary owns its
index/query correction; the collector retains strict checks. Log:
`/private/tmp/crm-010c-final-plans-2.log`. No paired benchmark has run.

The fixture phase timings were 1,884ms base setup, 477ms additional members,
95,055ms bulk cardinality and 478ms final ANALYZE. A read-only activity check
identified the temporary-row/contact bulk join as the slow setup statement.
The collector owner will ANALYZE those newly populated fixture relations before
that bulk join; this changes setup statistics only and preserves all rows.
The reviewer accepted the exact claim-predicate and fixture-user corrections,
verified hashes/equivalence and whitespace, without edits or runtime work.

Rollback-only result-page diagnostics then established that a nonunique
`(manifest_id, organization_id)` result index alone fixes unfiltered pages but
still allows full-table hash joins for filtered pages. Keeping the per-manifest
lookup as a lateral subquery with constant `OFFSET 0` preserves all matches and
filter semantics while preventing its flattening. No planner settings were
changed and both diagnostic transactions rolled back their indexes.

With that query/index, first/deep pages inspect 51 manifest rows unfiltered,
64 for imported, 501 for held and 510 for pending in the mixed fixture; all
lookups are indexed, without spills, and observed times were 0.091–1.115ms.
The empty `already_imported` filter examines the remaining 25,000/1,000-row tail
(29.744/1.118ms). Thus the 600-row mixed-density assertion is not a universal
bound for empty or arbitrarily sparse filters. Evidence:
`/private/tmp/crm-010c-qa-0e4vdxyr/result-index-diagnostic.json` and
`/private/tmp/crm-010c-qa-0e4vdxyr/result-lateral-diagnostic.json`.

The primary is applying this measured index/query correction. The collector
will preserve its strict mixed-density checks, capture the separate empty-tail
case, and continue to extract actual production SQL. Constant zero OFFSET is a
lookup boundary, not offset pagination or skipping preceding result rows.

The third collector run compiled in 33.48s and failed after 97.04s on a
fixture assertion before reaching result pages. PostgreSQL chose the ordered
manifest uniqueness index for an eligible page and inspected 57 rows to return
51 (six held rows filtered, five shared blocks, 0.027ms). The strict 51-row
assertion incorrectly required the disposition index to be selected. The
collector owner is retaining indexed/no-spill checks while bounding this known
fixture by density: 51 unfiltered, 60 eligible, 510 held. Query-plan failures
will be collected through the whole run before failing, so a first failure does
not hide later queries. No production query changes follow this assertion issue.
Log: `/private/tmp/crm-010c-final-plans-3.log`.

The pre-bulk ANALYZE addition did not materially reduce setup time (93,746ms
bulk cardinality); no improvement is claimed. The targeted reviewer accepted
the result query/index correction, independently matched all four source hashes
and the diagnostic SQL hash, verified reversal against prior code/migration and
confirmed grants, constraints, tenant/filter/cursor/all-match behavior remain
unchanged. The collector and final gates still need successful execution.

The fourth collector run **passed all 59 executed plans and 258 checks**, with
zero failures. Compile 25.74s; execution 100.81s. Log:
`/private/tmp/crm-010c-final-plans-4.log`; parsed plan JSON with exact SQL hashes:
`/private/tmp/crm-010c-qa-0e4vdxyr/query-plans.json`. Populated result pages were
0.148–0.829ms in this run; empty result tails were 27.665/1.041ms. These are
EXPLAIN diagnostics, not the paired HTTP benchmark. All first/deep paging,
preparation/claim/mapping/source/identity/field/provenance and eight People sort
shapes passed their indexed/bounded/no-spill checks. The collector's local
assertion accumulator preserves failures and fails at the end; source, SQL and
fixture errors remain immediate failures. Root inspected that control flow.

The three sequential scripts are now being repeated on the corrected production
SQL/index and collector before the single paired benchmark and browser QA.

## Final corrected-source gates — passed

The validation snapshot records 234 changed implementation/contract paths,
including 18 obsolete SQLx cache entries deleted by preparation. It is an
uncommitted working-tree manifest, not a Git commit claim. Canonical source-map
SHA256: `84c4c8678530c7f00fd3c7042a34e917a1f718fc45684cb20ba6ee51b40631ad`;
file: [PRE_BROWSER_SOURCE_SHA256.json](../design/qa/slice-010c-2026-09-11/PRE_BROWSER_SOURCE_SHA256.json).
This historical snapshot precedes the browser corrections; the current final
manifest is separately named `VALIDATION_SOURCE_SHA256.json`.

- Corrected-source `./scripts/sqlx-prepare` passed (21.74s compilation), log
  `/private/tmp/crm-010c-final-sqlx-prepare-2.log`.
- Corrected-source `./scripts/check` passed in 72s: 877 Rust tests, five
  doctests, 997 Web tests/65 files, 14 preflight and 11 email-worker tests, plus
  formatting, lint, typechecking, production compile/build and crate fences.
  Log: `/private/tmp/crm-010c-final-check-2.log`.
- Corrected-source `./scripts/check-db` passed in 370s: all 845 DB tests
  passed (344.616s test time, one slow), with 877 service-free tests skipped.
  Log: `/private/tmp/crm-010c-final-check-db-2.log`.
- All three scripts ran sequentially. Successful logs and the 59 executed plans
  are retained under `docs/design/qa/slice-010c-2026-09-11/checks/`.
  The paired benchmark and synthetic browser runs follow these final gates.

## Paired reader performance — passed once

The executable frozen-SQL manifest test passed, then the single planned paired
HTTP run passed in 56.50s. No paired benchmark was repeated. The 25,000-Person,
50-member fixture has 100,000 contacts and dense shared-contact groups. Both
arms use the current workspace guard/task-only fallback, with the original
c6c5930 contact-reader SQL frozen in one arm. This checks that reader change;
it does not measure total workspace-guard overhead or establish larger-agent
capacity. Complete response hashes and statuses matched in all comparisons.

| Request | Baseline p95 ms | Current p95 ms | Allowed p95 ms |
|---|---:|---:|---:|
| People, created descending | 145.419 | 144.051 | 170.419 |
| Today, zero sources, serial | 61.238 | 63.143 | 86.238 |
| Today, five sources, serial | 102.981 | 99.226 | 127.981 |
| Today, zero sources, five concurrent | 67.653 | 73.351 | 92.653 |
| Today, five sources, five concurrent | 167.377 | 157.947 | 192.377 |

The acceptance bound is old p95 plus max(25ms, 10% of old p95). Protocol,
preflight, raw samples, cross-arm checks and comparisons are retained in
[checks/performance](../design/qa/slice-010c-2026-09-11/checks/performance/).
Logs are `/private/tmp/crm-010c-final-perf-baseline.log` and
`/private/tmp/crm-010c-final-performance.log`, also copied into checked evidence.
No QA browser/runtime or other task DB test ran concurrently with timing.

The final synthetic `import_qa` example build passed with test-support enabled
in 0.14s. It uses the actual API/domain layer with fake source reading and a
compile-time readiness seam. The production Web browser walkthrough is now
running; no live FUB qualification is implied.

## Synthetic browser verification — development chronology

The first attempt reached the ready plan and responsive checks, then failed a
private harness assumption: the mapping child resets to its Stage tab after a
new plan. The harness now explicitly selects Assignment mappings before its
dirty-draft check. This changed no product behavior. It also records bounded
viewport screenshots alongside full-page evidence so long migration pages can
be visually inspected at native width.

The second attempt passed partial mapping inheritance, all six responsive widths,
and dirty-draft blocking/discard. It reached the visible confirmation dialog,
but the exact accessible-name locator failed: the shared dialog's custom header
has no accessible title wiring. The screenshot also exposed the stale page
claim that no records are imported there. The primary is correcting those two
Web-only issues. Backend, query-plan and paired performance proof remain
unchanged; full Web/repository check and production build will be refreshed.
The owned API/Web/browser processes were stopped after both attempts. Private
attempt logs are retained separately under the QA directory; no live source ran.

The dialog now connects its real PrimeVue root's `aria-labelledby` to the
visible heading through a unique Vue ID. Three tests mount the real dialog and
verify import, budget and cancellation titles. Migration page copy now describes
the explicit People import and administrator review before later activation.
The refreshed `./scripts/check` passed in 32s: 877 Rust tests, five doctests,
1,000 Web tests/66 files, 14 preflight tests and 11 email-worker tests, plus all
formatting/lint/type/production-build checks. The final log is
[checks/check.log](../design/qa/slice-010c-2026-09-11/checks/check.log).
These changes are Web-only; the successful backend DB/plans/performance bytes
remain unchanged.

Attempt 3 completed the actual import and exact uncertain-confirmation replay,
then verified four separate People/eight contacts/one held Person, all result
pages and the complete >2MiB source field's SHA256 through both plan and Person
provenance endpoints. It then exposed a contract defect: a held member's import
GET returned the generic workspace 409 before the route's required admin-only
403. No data escaped. The browser assertion remains 403.

The correction invokes the existing `OrgAdminContext` for server-matched
migration and the two provenance read routes before generic member review denial.
Allowed administrators still acquire the complete shared response permit and
current database role/mode checks; ordinary held member People reads remain 409.
The existing real-import HTTP test now checks eight migration/provenance 403
responses alongside admin 200 and ordinary member 409. No SQL, schema or reader
query changed. The three final scripts and synthetic example build are being
repeated sequentially. The one paired reader benchmark will not be repeated:
its permitted People/Today paths, query bytes and measured behavior are unchanged.

Targeted independent confirmation accepted the guard/test and dialog/copy fixes,
with all five supplied hashes verified. It found no introduced defect; this was
bounded correction confirmation, not a third full review. The subsequent final
sequential run passed:

- `./scripts/sqlx-prepare`: 14.83s compilation, no new query/schema changes.
- `./scripts/check`: 64s, 877 Rust tests, five doctests, 1,000 Web tests/66 files,
  14 preflight and 11 email-worker tests; all format/lint/type/build/fence checks.
- `./scripts/check-db`: 356s, all 845 DB tests passed in 333.079s (one slow),
  including the new eight-route held-member error-precedence assertions.
- `SQLX_OFFLINE=true cargo build -p crm-api --example import_qa --features test-support --locked`:
  passed in 0.17s. Backend working directory; the browser uses this actual binary.

Current successful logs replace `checks/sqlx-prepare.log`, `checks/check.log`,
`checks/check-db.log` and `checks/browser-build.log`. The previous successful
logs remain privately retained. Runtime binary/production Web assets are recorded
in [BROWSER_ARTIFACTS_SHA256.json](../design/qa/slice-010c-2026-09-11/BROWSER_ARTIFACTS_SHA256.json).

Attempt 4 then passed completed import/provenance checks, the corrected member
403, member waiting and actual browser logout, completed-results reload and
six-width layouts. The role-demotion test timed out. The initial hypothesis was
that Playwright's default document-load wait was coupled to the deliberately held
Person response. Its failure screenshot showed the member waiting screen, but
that screenshot was taken after cleanup released the response; it did not prove
a timely transition. The private harness changed to an actual SPA pathname and
required-heading wait. Attempt 5 still failed before the response was released,
disproving the load-only hypothesis. No failing acceptance assertion was removed.

Visual inspection also caught a screenshot taken inside the existing 150ms
Operator-launcher leave transition. Its handlers already reject review-mode use.
The harness now waits for actual DOM removal before capturing the waiting screen.
Additional viewport screenshots focus directly on the long-field viewer; earlier
provenance summary shots alone did not show that viewer. Storage-recovery proof
also asserts zero source-reader calls before each runtime restart, because the
example resets its counters at startup. These strengthen evidence without changing
runtime behavior or reinterpreting a failing product assertion.

A bounded standalone run against the already-imported synthetic Organization
then isolated a product defect. The document was fully loaded and the server
reported the demoted member, but a window-focus event caused no new `/me` read
for six seconds. Releasing the old Person response caused a provenance 403 and
only then a session refresh/redirect. The existing cookie-session coordinator
verifies once per session generation; it does not detect a role change within
that generation, and TanStack's installed focus manager listens to visibility
changes rather than the window-focus event. The earlier explanation was incorrect.
The safe trace is retained as
[checks/authority-before-fix.json](../design/qa/slice-010c-2026-09-11/checks/authority-before-fix.json).

The primary now owns a narrow workspace-authority refresh on app return through
the existing synchronous request/cache fence and coalesced `/me` operation.
Session lifetime and uncertain import confirmation identity must remain intact.
Focused regression, final Web/repository check and the saved-scenario browser
authority test precede another complete browser pass. Backend/schema/query-plan
and paired performance bytes remain unchanged.

The Web correction is frozen in `AppShell.vue` and `App.test.ts`. Capturing
focus/visibility/pageshow handlers call the existing workspace refresh only for
a visible, authenticated private app with settled session authority. Descendant
input focus is excluded; listeners are removed on unmount. Coalescing preserves
one authority read and the same cookie generation/auth-session lifetime, while
the synchronous fence aborts old Person reads and clears private caches.

The representative new regression failed before the correction (one failed,
four passed). After the fix, the combined six-file suite passed all 53 cases,
including existing Operator open/focus and uncertain import-confirmation behavior.
Combined testing caught and corrected an initial listener that also reacted to
input focus; the final target/currentTarget filter excludes it. A test-runtime
Window proxy also exposed an overly literal global-Window comparison, corrected
without weakening assertions. Final owned lint and full Web typecheck passed
(5.15s typecheck). Red/green logs are retained in `checks/focus-regression-*.log`.
The full repository check and production build now follow on these exact files.

The refreshed `./scripts/check` passed in 31s: 877 Rust tests, five doctests,
1,002 Web tests in 66 files, 14 preflight tests and 11 email-worker tests, with
all formatting/lint/type/build/fence checks. `checks/check.log` is this final run.
The standalone production-build browser authority check then passed against the
saved synthetic import: focus caused the member redirect before the intercepted
Person response was released, and the old response could not restore Person data.
Role restoration also recovered administrator navigation. Evidence:
[browser-authority.json](../design/qa/slice-010c-2026-09-11/checks/browser-authority.json)
and [log](../design/qa/slice-010c-2026-09-11/checks/browser-authority.log).
The complete import and storage-recovery browser pass follows this focused proof.

## Final browser pass and implementation completion

The final complete walkthrough passed against the recorded example binary and
production Web build. It recorded 50 successful checkpoints, including all six
states (ready plan, completed import, member waiting, Person review, preparation
paused for storage, and partially imported/cancelled) at 390, 640, 768, 1024,
1280 and 1536 pixels. No document/body horizontal overflow or uncaught browser
page error occurred. Long tables scroll inside their containers. Root and a
bounded independent visual check found no actionable visual defect; the member
launcher transition and missing field-viewer captures are resolved.

The actual workflow proved partial mapping inheritance, explicit matching stage
creation/member/unassigned choices, required acknowledgments, dirty-draft
blocking/discard, dropped-response canonical confirmation replay, completion and
reload, separate People with contact overlaps, one held Trash record, inert
source strings, current-role/cross-Organization denial and member logout.
The 2,687,014-byte source value matched SHA256
`7fbc68410025d26a6c852cdc662fd9b345f314bfa1b1e366a60dc886074c4b35` through
both plan and Person provenance; its complete JSON representation required
45 bounded reads totaling 2,818,088 UTF-8 bytes.

Storage recovery proved that raising approved allowances does not resume work.
Explicit preparation resume completed the plan. A lower current ceiling paused
execution after three People/four contacts, before the large fourth Person.
Cancellation preserved those same three Person IDs/results, released both
reservations to zero, and retained review mode after process restart. Source
reader counters stayed zero through preparation/execution/recovery/cancellation;
checks occur before counter-resetting restarts as well as afterward.

Evidence is retained in [browser-happy.json](../design/qa/slice-010c-2026-09-11/checks/browser-happy.json),
[browser-recovery.json](../design/qa/slice-010c-2026-09-11/checks/browser-recovery.json),
[browser.log](../design/qa/slice-010c-2026-09-11/checks/browser.log) and
[32 screenshots](../design/qa/slice-010c-2026-09-11/screenshots/).
The recovery JSON includes the preceding happy-path checkpoints; do not add
their row counts together. The private runner was
`node /private/tmp/crm-010c-qa-0e4vdxyr/run-browser.mjs`, using the protected
synthetic environment, private API 3012 and production Web preview 5182.

The final [source manifest](../design/qa/slice-010c-2026-09-11/VALIDATION_SOURCE_SHA256.json)
contains 236 changed implementation/contract paths, including 18 explicit
obsolete SQLx cache deletions. Canonical map SHA256:
`45f1c940403f29e56479dd17714ca033b0a9e99a7d1b4575adb086909ee787ad`.
The final Web return-event fix changes no backend, schema, query-plan or benchmark
bytes. Historical review/source checkpoints remain available separately.

Implementation is complete and remains uncommitted on the isolated 010c branch.
Live FUB qualification, customer-data readiness, deployed-fleet compatibility
evidence, Git publication, deployment and later activation remain separate.

Final handoff checks matched all 236 source/deletion entries and 73 runtime
artifact hashes. All 137 local document links across 12 current documents exist;
Git whitespace validation passed. The owned browser/API/Web processes exited,
QA ports 3012/5182 are closed, and the private environment's original fingerprint
and 0600/0700 permissions are restored. Original `main` remains clean at `c6c5930`.
The dedicated `crm_slice010c_qa` database and private fixtures remain available
for local synthetic inspection; no shared-development process or database was reset.
See [final-integrity.json](../design/qa/slice-010c-2026-09-11/checks/final-integrity.json).
