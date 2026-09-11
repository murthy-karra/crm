# Slice 010a — Implementation verification

**IMPLEMENTED, SYNTHETICALLY VERIFIED, MERGED AND PUSHED. Live FUB validation
is explicitly user-deferred; 010a is not deployed.** Implementation `e4e0658`,
merge `cd224fe`, base `b2fb368`. Source published on 2026-09-11; the original
`codex/slice-010a-fub-assessment` branch and
`/Users/karrad/projects/crm-worktrees/010a` checkout were removed after merge.
The shared development runtime remains on released 019b.

## Delivered scope

The approved [specification](../specs/SLICE_010a.md) and
[brief](SLICE_010a_IMPL.md) are implemented: admin Manage → Migration,
encrypted saved credentials, same-account replacement and disconnect, six
bounded GET checks, encrypted evidence, persistent reports, retry/cancel and
restart recovery. Thirteen additional families remain visibly not checked.
Counts are decimal strings; overlapping People queries are never summed.
There is no import, full enumeration, mapping, source mutation, Operator tool
or cutover capability in this slice.

Changes are confined to the new migration domain and API route, necessary
config/state/startup/error registration, names-only `.env.example` additions,
one additive migration, new DB/HTTP tests, Web API/view and minimal admin
navigation/query/test integration. [Final code hashes](../design/qa/slice-010a-2026-09-10/final-code-tree.json)
list every changed code/config file. New runtime SQL uses bound dynamic query
calls; `sqlx-prepare` produced no offline-cache changes. No dependency was added.

## Acceptance evidence

| Specification acceptance | Result |
|---|---|
| §7.1 admin authorization and tenant isolation | Passed direct-domain, HTTP, DB and browser checks |
| §7.2 closed GET boundary and no business writes | Passed local adapter allowlist/hostile-response tests and unchanged business-table counts |
| §7.3 honest count/coverage/error report | Passed parser, DB, real-HTTP and Web/browser checks; all 19 rows visible |
| §7.4 encryption, raw evidence and redaction | Passed purpose/tenant/row/tamper checks, malformed capture, captured tracing and browser secret-retention checks |
| §7.5 receipts, leases, retry, cancellation, commit failure | Passed concurrent requests, expired lease, failed evidence commit, retry budget, replacement/disconnect and in-flight fencing |
| §7.6 browser walkthrough and session boundaries | Passed production Web build against real API with injected synthetic reader; member denial, Organization change, cleared key and narrow layout |
| §7.7 authorized live FUB test | **Not run; explicitly deferred by the user because no test account is available** |

[Public source qualification](../research/SLICE_010a_FUB_SOURCE_CONTRACT.md)
pins official published identity/collection shapes. It does not establish
live endpoint access, actual account scope or full connector readiness.

## Commands and results

- `cargo test -p crm-app migration --lib`: **8 passed** (parser, crypto and local
  HTTP adapter tests).
- Targeted `nextest db_migration:: --run-ignored only`: **11 passed**, including
  actual tracing capture and four query-plan assertions.
- `./scripts/sqlx-prepare`: **passed**, consolidated migration applied cleanly.
- `./scripts/check-db`: **796 passed**, 839 service-free tests skipped by design;
  **313 seconds**, including the SQLx schema check. Ran once on the frozen backend.
  [Result](../design/qa/slice-010a-2026-09-10/check-db-result.json).
- Final `./scripts/check`: **839 Rust tests, 5 doctests, 889 Web tests and
  11 email-worker tests passed**, **15 seconds**. Includes formatting, Clippy,
  production-shape compilation, dependency fences, Web lint/typecheck/build.
  [Result](../design/qa/slice-010a-2026-09-10/check-result.json).
- Private real-API walkthrough: **passed**. Covers immutable receipts, scoped
  locators, source-account binding, 19-row counts, prior report, pause/retry,
  in-flight cancellation, repeated disconnect and no imported People.
  [Result](../design/qa/slice-010a-2026-09-10/real-api-result.json).
- Private Playwright walkthrough against the production build: **passed**,
  with no browser runtime errors. [Result](../design/qa/slice-010a-2026-09-10/browser-result.json).
  Screenshots: [empty](../design/qa/slice-010a-2026-09-10/01-empty.png),
  [completed](../design/qa/slice-010a-2026-09-10/02-completed-report.png),
  [new work with prior report](../design/qa/slice-010a-2026-09-10/03-previous-report-during-assessment.png),
  [paused](../design/qa/slice-010a-2026-09-10/04-paused.png),
  [narrow](../design/qa/slice-010a-2026-09-10/05-narrow-report.png).
- Four `EXPLAIN (ANALYZE, BUFFERS)` cases over 2,000 synthetic historical
  assessments: indexed top-1 latest/active/completed reads and indexed due claims
  with a bounded sort. **No 019b benchmark was rerun.**
- All 17 [frozen backend hashes](../design/qa/slice-010a-2026-09-10/backend-checkpoint.json)
  still matched after the full DB gate and final Web changes.

The existing Vite large-chunk warning remains. SQLx noted potentially unused
cached test-query metadata while its check passed. An initial private QA
`VITE_API_BASE_URL` mistake caused a wrong-path test failure and prevented
browser login; corrected to `/api`. A subsequent full Web run exposed one
transient existing PersonDetail task-edit test failure; its focused rerun and
two subsequent full gates passed without changing that unrelated test/code.
Development-server dependency optimization also interrupted early browser runs;
the final passing walkthrough used the production build, without HMR reloads.
No failed attempt is represented as a pass.

## Review, isolation and remaining limits

Two bounded review/fix rounds completed: [R1–R9 backend and W1–W5 Web](SLICE_010a_REVIEW.md).
Terra began implementation. Repeated incomplete idempotency repairs triggered
the repository MODEL_ROUTING escalation to an Astra high backend writer; Terra
then completed Web. The coordinator owned documentation and independent QA.
No concurrent writers edited the same source files. Actual billed cost/token
usage is unavailable and is not inferred.

Only disposable `crm_slice010a_qa` was recreated for the consolidated unreleased
migration. Synthetic Organizations/users were seeded using the sanctioned
platform bootstrap and existing real-HTTP seed flow. API 3010 injected the
fixture reader; Web preview 5180 served its tested production build. Both
temporary QA servers were stopped after verification. No live FUB
requests or real-customer records were used. Private `.env` stays mode 0600;
no credentials appear in Git, screenshots or the report. Node 24.16.0 / pnpm
11.22.0 uses an isolated APFS dependency clone; frozen offline install changed
no package manifest, lockfile or workspace configuration. Unrelated main
`notes.txt` was untouched.

Source serialization/cooldown is process-local for the current single modular
workload; per-check retry deadlines are durable. Invalid timing pauses its check
without automatic retry and imposes a recoverable 60-second shared cooldown.
An already in-flight GET may finish after cancellation, but the fenced result
cannot update the cancelled report. Scope remains unknown without sufficient
source evidence. Broader deployment topology, full inventory, mapping, import,
retention/erasure and cutover remain later authorized work. Live FUB validation
requires the deferred test account and registered integration identification;
fixture success is not live-source qualification.

Final documentation check passed: 81 local links, balanced fences, eight synced
documents, unchanged historical ladder survey and all 25 final code hashes.
`git diff --check` passed in both checkouts.

## Source integration and cleanup — 2026-09-11

The user authorized “do any cleanup and commit, merge, push” after reviewing
the synthetic-only verification and deferred live-source status (D-060
follow-up). No application behavior or shared contract changed during integration.

- Committed the 25 code/config files and 21 documentation/QA files as
  `e4e0658842dab019c41e4180bf125da078d1cd42`.
- Merged with `git merge --no-ff` as
  `cd224fea353d88f4cd18c4398bdd2c75f677bf7f`; no conflict and no tree difference
  from the implementation commit.
- Pushed main from `6bad52a` through `cd224fe`, including 019b's three local
  commits. `git ls-remote origin refs/heads/main` matched the merge hash.
  Documentation closure follows in the integration-record commit.
- All 25 final-code-tree hashes and all 17 backend-checkpoint hashes matched.
  The changed code/config set exactly matched the manifest. Existing test
  evidence therefore applies; no full test suite or benchmark was rerun.
- `git diff --cached --check` and `git diff --check` passed; relative links in
  the changed documentation resolved (82 checked before the implementation
  commit). Scoped source/artifact scans and five screenshot inspections found
  no accidental secrets, customer data or generated/private files to publish.
- Backed up duplicate main documentation/evidence before consolidating it into
  the branch. Preserved the worktree's `.env` in a private directory with mode
  0600, then removed the clean, fully merged worktree and its generated target,
  node_modules and dist folders. Verified no process had that checkout as cwd.
  Deleted the merged branch with `git branch -d`; only main remains.
- Reconciled current status, approval, next-action and verification sections;
  labelled older checkpoints as historical. The synthetic QA database and
  unrelated empty `notes.txt` were retained. Runtime processes, shared database,
  configuration and services were not changed.

The private backup location is recorded locally in
`/private/tmp/crm-010a-integration-backup-path`; private contents are not in Git.
Live FUB validation, 010a deployment and later migration rungs remain pending.
