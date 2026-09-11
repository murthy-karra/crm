# Slice 019b — retained performance evidence

**Passed: all 13 paired request-p95 budgets, complete response parity, and
all 18 inspected plans.** Exactly one benchmark invocation ran on the final
uncommitted source tree. No successful benchmark was repeated.

## Provenance and command

- Specification: [SLICE_019b §10](../../../specs/SLICE_019b.md#10-performance-and-delivery-budget).
- Worktree: `/Users/karrad/projects/crm-worktrees/019b`.
- Branch: `codex/slice-019b-custom-field-filters`; base/current commit:
  `6bad52a32376001d4082ac21aad29699462e3ceb`.
- Final code-tree SHA-256:
  `a75ee70b9b3b51021ca00853109e71ff8c222ca46ad1f91c0937976fabf88e0a`;
  [843 per-file hashes](../../qa/slice-019b-2026-09-10/code-tree.json).
- The fourteen frozen SQL texts match the base commit byte for byte;
  [baseline SQL manifest](baseline-sql-manifest.json). Live statement hashes
  and bind counts are retained with the plans.
- Original output: `/private/tmp/crm-019b-perf.zEs2UH`. Raw JSON files below
  are byte-for-byte copies, including the harness's UUID-redacted plans.
- [Harness output](harness-stdout.txt): one test passed, zero failed,
  894 filtered out; test 78.34 seconds, compile 55.56 seconds, total wall
  134 seconds. The opt-in feature exposes 37 unused legacy-fixture warnings;
  the ordinary required workspace Clippy gate passed without warnings.

Executed from `backend`, with private development environment loaded,
`SQLX_OFFLINE=true` and `DATABASE_URL` set to the migration test connection.
The shared DB gate lock was held; SQLx created an isolated scratch database.
Credentials were not serialized. The command was:

```sh
CRM_019B_PERF_OUTPUT="$(mktemp -d /private/tmp/crm-019b-perf.XXXXXX)" \
cargo test -p crm-api --test all --features perf-harness --locked \
  db_custom_field_filter_perf::slice_019b_authenticated_request_performance \
  -- --ignored --exact --nocapture --test-threads=1
```

## Request results

[Protocol](protocol.json): 25,000 People, 50 members, 50 definitions,
five values per Person, 100 built-in candidates, 10 call-only candidates,
100 source candidates, and a second Organization. Fixture shape SHA-256:
`4c977de366d2532cae45c534a76ca0fd9e5e8d18a044a283d4db9716d3096622`.
The two arms share fixture, fixed clock and build, with equal-sized pools and
alternating arm order. Fixed server-side test dispatch selects frozen/live
query wrappers; the production request/auth/session path consumes the entire
response body. No production arm switch is available.

Each case/arm has five serial warmups and forty measured serial requests.
Both Today cases also have two warmup and eight measured waves of five
concurrent requests per arm. The archive contains **1,040 measured requests
and 150 separate warmup requests**; every request is an exact timing with a
200, complete response. No timeout, transport failure or partial response
was removed. Raw ordered body hashes match between arms for every measured
series. Untimed preflight reached all fourteen statement families in each
arm; [measured cross-arm guards](measured-cross-arm.json) passed all 22 checks.

[Raw warmups and timings](raw-timings.json) retain nanosecond precision;
[comparisons](comparisons.json) retain paired p95, permitted maximum and hash
parity. Nearest-rank p95 below is in milliseconds. Each maximum is
`old p95 + max(25ms, old p95 × 10%)`, with serial/concurrent samples kept separate.

| Case | Frozen p95 ms | Live p95 ms | Permitted live maximum ms | Result |
|---|---:|---:|---:|---|
| `people_created_desc` (serial) | 41.527 | 40.545 | 66.527 | Pass |
| `people_created_asc` (serial) | 43.577 | 43.215 | 68.577 | Pass |
| `people_name_asc` (serial) | 42.528 | 42.635 | 67.528 | Pass |
| `people_name_desc` (serial) | 47.373 | 45.688 | 72.373 | Pass |
| `people_stage_asc` (serial) | 44.242 | 43.143 | 69.242 | Pass |
| `people_stage_desc` (serial) | 43.532 | 42.752 | 68.532 | Pass |
| `people_assignee_asc` (serial) | 43.523 | 43.461 | 68.523 | Pass |
| `people_assignee_desc` (serial) | 44.173 | 42.554 | 69.173 | Pass |
| `saved_list_count` (serial) | 5.843 | 5.640 | 30.843 | Pass |
| `today_zero_sources` (serial) | 21.658 | 41.006 | 46.658 | Pass |
| `today_five_sources` (serial) | 80.178 | 65.431 | 105.178 | Pass |
| `today_zero_sources` (concurrency 5) | 66.619 | 71.197 | 91.619 | Pass |
| `today_five_sources` (concurrency 5) | 172.446 | 157.846 | 197.446 | Pass |

Zero-source Today serial p95 increased **19.348 ms** (21.658 → 41.006 ms),
within the accepted 25 ms allowance. Passing the paired budget does not mean
every path became faster or establish an absolute latency SLO.

## Custom-predicate plan inspection

[All eighteen full EXPLAIN ANALYZE/BUFFERS plans](plans.json) and the
[derived node/loop/buffer summary](plan-summary.json) are retained. Five
populated custom slots include positive/negative text, number/date ranges
and choices. The person-state statement populates its two matrices with
independent criteria, both retaining 90 candidate IDs. These new predicates
use parity tests and plans; there is no old custom-filter request comparator.

All fourteen hot plans use field-scoped bitmap index/heap access via
`person_custom_field_value_field_idx`. Every executed value scan has
`loops=1`, with 25,000 index entries for the selected field; no plan contains
a sequential scan of the value table or rescans its full contents per Person.
Thirteen statements have five value heap scans; person-state has **40 fixed
value heap scans**, each executed once across its independent matrix subplans.
That fixed scan count is visible in the evidence and is not per-Person growth.
The four metadata reads are Organization-scoped and ID-bounded; each uses two
shared-buffer hits and does not access the value table.

The raw diagnostic `repeated_value_table_scan` is true for all fourteen hot
plans: that helper counts table-name occurrences and any bitmap heap scan,
without examining actual loops. Those flags are preserved. Manual inspection
of the full nodes, index conditions, loops and buffers establishes that they
do not indicate the prohibited per-Person full-table rescan. The diagnostic
is explicitly not the plan gate.

SQL execution times below are plan-capture observations, separate from the
request p95 gate. These captured plans do not claim forced-generic-plan
coverage, production load capacity, or behavior beyond the accepted envelope.

| Statement | Binds | Value heap scans | Max value-scan loops | SQL execution ms | Root shared hits |
|---|---:|---:|---:|---:|---:|
| `filtered_summaries_created_desc` | 72 | 5 | 1 | 51.854 | 21,145 |
| `filtered_summaries_created_asc` | 72 | 5 | 1 | 49.814 | 21,145 |
| `filtered_summaries_name_asc` | 72 | 5 | 1 | 48.846 | 21,049 |
| `filtered_summaries_name_desc` | 72 | 5 | 1 | 48.542 | 21,063 |
| `filtered_summaries_stage_asc` | 72 | 5 | 1 | 48.237 | 21,139 |
| `filtered_summaries_stage_desc` | 72 | 5 | 1 | 48.632 | 21,139 |
| `filtered_summaries_assignee_asc` | 72 | 5 | 1 | 49.578 | 21,136 |
| `filtered_summaries_assignee_desc` | 72 | 5 | 1 | 48.695 | 21,136 |
| `count_filtered_matches` | 71 | 5 | 1 | 34.119 | 3,951 |
| `source_membership` | 73 | 5 | 1 | 34.273 | 2,826 |
| `source_candidates` | 75 | 5 | 1 | 42.930 | 5,640 |
| `person_state` | 145 | 40 | 1 | 265.578 | 21,161 |
| `call_membership` | 73 | 5 | 1 | 62.656 | 5,229 |
| `call_only` | 74 | 5 | 1 | 93.461 | 8,205 |
| `custom_field_live_type` | 2 | 0 | 0 | 0.032 | 2 |
| `custom_field_option_belongs` | 3 | 0 | 0 | 0.030 | 2 |
| `custom_field_labels` | 2 | 0 | 0 | 0.013 | 2 |
| `custom_field_option_labels` | 2 | 0 | 0 | 0.011 | 2 |

[Artifact hashes](artifact-sha256.json) cover every file in this directory
except the hash manifest itself. Final code, database, browser and release
notes are in the [verification record](../../../tasks/SLICE_019b_VERIFICATION.md).
