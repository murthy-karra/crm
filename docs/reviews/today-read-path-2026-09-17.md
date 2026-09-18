# Today read-path audit and reference batching

Implementation base: `11f5d85` on former `codex/migration-completion`.
Merged and published at `8fac1dc`; shared-development deployment is tracked in
[the release record](../tasks/READ_OPTIMIZATION_RELEASE_2026-09-17.md).
Person detail is already one current-state projection lookup plus one history
union. This work concerns Today, not another Person-detail rewrite.

## Findings

The existing Today path uses one repeatable-read snapshot and evaluation clock.
With enabled default feeds, nonempty retained sets and no truncation, its domain
reads are feed configuration, Person-state candidates, call membership and
call-only candidates, task membership and task-only candidates, and saved-source
metadata: seven statements before saved-source validation/evaluation (customized
system feeds can also add reference-validation reads). Empty sets,
disabled feeds and truncation skip applicable statements. Each valid saved source
adds at most two candidate/membership statements plus reference validation.
Authentication, workspace authorization, savepoints, timeout settings and commit
are separate from those domain reads.

Baseline real browser journeys (`.e2e/runs/d851f353f28e`) recorded:

- Lists: 12 Today requests, 32–50 SQLx executions, median 42.
- Tasks: 25 Today requests, 1–34 SQLx executions, median 32; 22 requests
  recorded 32 or 34 executions, and three short paths recorded 1, 2 and 11.
  The SQL profile does not record HTTP status, so it does not establish whether
  those short paths were rejected or cancelled.

These are QueryLogger executions, not wire round trips or data-query counts.
Small journey fixtures establish exercised paths, not production capacity.
Both journeys passed with verified cleanup. Their `sql-profile.json` files retain
per-request categories and their `sql-profile.md` files summarize endpoints.

## Implemented change

Filter reference validation previously issued one existence query per selected
stage, explicit assignee or tag. Saved-source validation also set the remaining
statement timeout before each identifier. The shared validator now checks each
clause's identifiers in one static, Organization-scoped `NOT EXISTS` query over
a bounded UUID array (the existing shape limit is 50).

A 50-tag saved-source clause therefore goes from 50 reference reads and 50
timeout settings to one of each. This is a code-path count; ordinary default
feeds with no explicit identifiers receive no reduction. Configuration and
People-filter callers share the ordinary validator and receive the same batching.

Clause-order error precedence, foreign/missing-reference errors, duplicate
semantics at the reference layer, and inactive-membership reference validity are
preserved. `me`/`unassigned` do not cause a reference query. Custom-field checks
are unchanged. The timeout still uses the original absolute source deadline;
savepoint recovery and all-or-nothing source contributions are unchanged.

No schema, HTTP, realtime, Operator, ranking, visibility or persistence contract
changes. No migration or new dependency. Authority: D-015, D-019, D-027, D-043,
D-047, D-050, D-052 and D-054; existing 011c/011d/012/016 specifications.

## Verification

Final verification passed for this change:

- `cargo fmt --all --check` and `git diff --check`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` with normal
  features (`/private/tmp/crm-today-reference-clippy-standard.log`).
- `cargo check --workspace --locked`, production feature shape
  (`/private/tmp/crm-today-reference-production-check.log`).
- Final `cargo nextest run -p crm-api --test all --features perf-harness
  --run-ignored only` with selector
  `(test(db_today) or test(db_people_filter) or test(db_custom_field_filters::)
  or test(db_saved_list) or test(db_tag)) and not test(today_reference_batch_http_perf)`:
  **278 passed**, 152.823 seconds. Covers tenant isolation, filter error
  precedence, inactive references, saved-list privacy, source timeouts/recovery,
  feed equivalence, task-axis failures and the new reference-group regression.
  Log: `/private/tmp/crm-today-reference-final-db-retry.log`.
- One completed paired benchmark and three query plans, detailed below.
- `./scripts/e2e --family lists --family tasks --concurrency 1 --sql-profile`:
  both journeys passed before and after the implementation, with verified
  cleanup. The final change after browser verification corrected only the
  opt-in benchmark's synthetic membership fixture.

Rust outputs used `/private/tmp/crm-projection-target`; E2E used owned Docker
images, databases and Web assets. Shared development was not rebuilt or deployed.
The earlier focused database run also passed 138 tests; the final run supersedes
it as the implementation-tree evidence.

An attempted all-target lint with the optional `perf-harness` feature failed on
34 dead-code errors in the pre-existing `db_today_feeds_http_perf` target and its
frozen fixtures. That target is outside the normal lint gate; no unrelated
fixture changes or warning suppression were made.

The first paired-benchmark attempt stopped before measurement because its
synthetic membership INSERT omitted the required role/status columns. The fixture
was corrected to explicit `member`/`active`; no application code changed for it.

The first broader final-test selector also matched the old opt-in
`db_custom_field_filter_perf::slice_019b_authenticated_request_performance` under
`perf-harness`. That test failed on its missing `CRM_019B_PERF_OUTPUT` setting;
90 ordinary tests passed before fail-fast stopped the run. The selector was
narrowed to the intended `db_custom_field_filters::` module, preserving all
ordinary Today/filter/list/tag coverage. This was a runner selection mistake,
not a passed check or an application regression.

The completed opt-in paired benchmark freezes only the old per-identifier probes from
`11f5d85`; both arms share the rest of the authenticated HTTP route, snapshot,
clock, fixture, process and database. It runs an in-process HTTP router, not a
network capacity test. Its synthetic book has 25,000 People and 50 members, with
one source selecting nine stages, 50 explicit members and 50 excluded tags.
The source exercises the changed validation path and returns 200 identical items.
It is not a history-heavy benchmark or a claim about five-source concurrency.
Ordinary test runs do not compile the performance test; production builds do not
include the baseline override.

Reproduce the measured test with the normal isolated SQLx test credentials,
an explicit absolute `CRM_TODAY_REFERENCE_PERF_OUTPUT`, and
`cargo test -p crm-api --test all --features perf-harness
today_reference_batch_http_perf -- --ignored --test-threads=1`.

[Paired results and plans](../design/perf/today-reference-batching-2026-09-17/run.json)
record 40 measured requests per arm after five warmups each, with alternating
order. Full response equality passed. Request p95 fell from **68.722 ms to
29.085 ms** (57.7% lower); the D-050 relative regression gate passed. The fixture's
109 identifier probes and 109 timeout settings become three of each: 212 fewer
executions by code-path accounting, not an additional profiler measurement.
Measured on Darwin arm64, Rust 1.98.0 (`88d9e12ae`), unoptimized test build.
The adjacent `source-sha256.json` pins the three measured source files; those
hashes were checked against the final implementation tree.

The three `EXPLAIN (ANALYZE, BUFFERS)` plans took 0.022–0.035 ms and touched
only the relevant nine-stage or 50-row member/tag catalogs. PostgreSQL chose
hash anti joins and sequential scans of these tiny catalogs, not index scans.
None scans People or history. This demonstrates bounded catalog work for this
fixture, not index use or production multi-tenant capacity. Existing catalog
keys remain available; no speculative indexes were added.

Updated-code browser evidence: `.e2e/runs/2486b60f693a`, lists and tasks both
passed with verified cleanup. The list journey retains the same 32–50 execution
range because its configured filters contain only a single tag or symbolic `me`.
The reduction is exercised by the separate reference-rich fixture above.

User-requested full E2E follow-up: `./scripts/e2e --all --concurrency 4
--timeout 900` passed **all 10 registered families and all 103 journey steps**
on the current working tree, first attempt for every family. Cleanup was
`verified_empty` for every environment. Evidence:
[full-run summary](../../.e2e/runs/0478945326d9/summary.json).
This includes workspace, leads, tasks, relationships, lists, routing,
correspondence, Operator, calls and migration. No implementation fixes were
needed during this run.

## Projection follow-up

Today already reads the four trigger-maintained activity dates from Person.
Person-state, call-only, task-only and saved-source candidates still assemble
primary contact values, inquiry counts/latest inquiry and effective contact
attempts. That repeated work is inside bounded candidate SQL, not an N+1 series
of application queries. A compact, indexed activity/summary projection remains
a candidate if statement-level measurements justify its maintenance cost.

Do not cache whole Today responses: membership, ages, caller-owned call outcomes,
task ownership/due times and saved-source definitions are viewer/time dependent.
Any future projection design must retain same-snapshot freshness, correction
semantics, tenant boundaries, erasure/rebuild behavior and source failure isolation.
It needs its own concrete persistence contract and verification before adoption.
