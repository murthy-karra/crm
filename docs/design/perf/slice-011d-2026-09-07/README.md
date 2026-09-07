# Slice 011d — Phase B performance evidence

Executed 2026-09-07 by Lane B (backend/database), in the isolated worktree
`crm-worktrees/011d-lane-b`, against a `#[sqlx::test]`-managed ephemeral
scratch database (never `crm_dev`), cleaned up by the harness itself
(`fixture.cleanup()`). One run, retained in full below. Host: the same
MacBook Pro (Apple M1 Max, 10 cores, 64 GB) used for the Slice 011c
archives, PostgreSQL 18 in the OrbStack development container. Full facts
in [environment.txt](environment.txt); source and binary hashes in
[source-sha256.txt](source-sha256.txt) (binary hash recorded in
`environment.txt`).

**Scope note, disclosed up front:** this archive reuses the Slice 011c
Phase B fixture (`today_http_perf_fixture.rs`, unmodified — same
50,000-Person history-rich database, same four viewer books, same five
source definitions) and driver primitives (`today_http_perf_driver.rs` —
`run_wave`/`run_serial`, nearest-rank percentiles, the exact
1,250/2,500/4,500 ms request caps and 450 ms whole-source cap). It does
**not** reproduce the 011c archive's own exhaustive telemetry-coverage,
sentinel-scanning and dual-arm evidence apparatus (`db_today_http_perf.rs`
itself, ~2,400 lines) — a new, smaller harness
(`db_today_feeds_http_perf.rs`, `perf-harness`-gated, outside
`tests/all.rs`, absent from every normal `check`/`check-db` run) was
written instead, because 011d's regression claim rests on `Legacy` still
being the exact same compiled-in statement it always was (already proven
byte-identical to `Feeds` by the DB-level equivalence suite in
`db_today_feed_equivalence.rs`), not on reconstructing a frozen historical
module the way 011c had to. This is a deliberate, disclosed scope
reduction, not a shortcut on the numbers themselves — every request in
this archive is a real loopback HTTP round trip against the real router,
real session auth, and the real 50k-Person fixture; nothing here is
estimated or fabricated.

## Part 1 — paired zero-source regression (Legacy vs Feeds)

Same fixture, same fixed clock (`2026-09-06T12:00:00Z`), same build, same
concentrated-book viewer, 8 serial samples each (the critical sample
shape's serial leg), immediately followed by matching 2×10/2×20 waves.
`Legacy` is served through a `test-support`-gated router selecting
`TodayProvider::Legacy` (new: `router_with_test_clock_and_provider`,
mirroring `router_with_test_clock`'s existing clock seam); `Feeds` through
the production default.

| | Legacy p95 | Feeds p95 | Allowed (max(25ms,10%)) | Within allowed | Payload equal (excl. `sources`) |
|---|---:|---:|---:|:---:|:---:|
| Concentrated, zero sources, serial | 204 ms | 177 ms | 229 ms | **yes** | **yes** |

Feeds is *faster* than Legacy here, not merely within the allowed
regression — consistent with `person_state.sql` being one statement
computing both person-state feeds at once versus `Legacy`'s equivalent
compiled-in query under the same plan shape. One representative full
payload was compared with `sources` stripped from both sides via `==` on
the parsed JSON `Value`; they were byte-identical.

## Part 2 — the 011c matrix at concurrency 1/10/20 (Feeds)

Same ten cases and the independent five-source concurrency-20 repeat, same
sample shapes (critical: 8 serial + 2×10 + 2×20; supplemental: 5 serial +
1×10 + 1×20), same six ten-request warm-up waves per case (discarded from
percentiles, retained in `run.json`/`harness-stdout.txt`). All 522
critical+supplemental+repeat measured requests returned `HTTP 200` with
`sources.status: "complete"` (`all_normal: true` in every row below).

| Case | c1 p95 / cap | c10 p95 / cap | c20 p95 / cap | Whole-source p95 / max | Feed-pool wait p95 / max |
|---|---:|---:|---:|---:|---:|
| Concentrated, zero sources | 221 / 1,250 | 370 / 2,500 | 664 / 4,500 | – | 394 / 398 |
| Concentrated, one dense source | 341 / 1,250 | 627 / 2,500 | 1,191 / 4,500 | 266 / 286 | 650 / 700 |
| Concentrated, one absence source | 271 / 1,250 | 507 / 2,500 | 1,116 / 4,500 | 181 / 238 | 627 / 772 |
| Concentrated, five overlapping sources | 750 / 1,250 | 1,315 / 2,500 | 2,514 / 4,500 | 270 / 325 | 1,276 / 1,364 |
| Concentrated five-source independent repeat (2×20) | – | – | 2,764 / 4,500 | 310 / 344 | 1,466 / 1,511 |
| Typical 2,000 book, zero sources | 21 / 1,250 | 45 / 2,500 | 89 / 4,500 | – | 46 / 46 |
| Typical 2,000 book, five sources | 35 / 1,250 | 74 / 2,500 | 144 / 4,500 | 9 / 12 | 80 / 90 |
| Partial built-ins, zero sources | 8 / 1,250 | 19 / 2,500 | 41 / 4,500 | – | 21 / 21 |
| Partial built-ins, five sources | 457 / 1,250 | 970 / 2,500 | 1,865 / 4,500 | 285 / 316 | 983 / 1,046 |
| Empty built-ins, zero sources | 4 / 1,250 | 9 / 2,500 | 19 / 4,500 | – | 9 / 9 |
| Empty built-ins, five sources | 470 / 1,250 | 973 / 2,500 | 1,867 / 4,500 | 274 / 387 | 1,010 / 1,043 |

All values in milliseconds, nearest-rank percentiles over every measured
attempt (there were no failures). Whole-source max never exceeded 387 ms,
comfortably under the 450 ms cap in every case. The independent
five-source repeat's pool-wait max was 1,511 ms, 489 ms below the
two-second acquisition timeout.

## Part 3 — new 011d cases (concentrated book, serial, cap 1,250 ms)

Each case is 8 serial samples against the concentrated zero-source
configuration, admin-customized via the real typed commands
(`update_today_system_feed`/`set_today_system_feed_enabled`) before
measurement, then reverted/re-enabled afterward (revisions tracked from
each command's own returned `outcome.feed.revision`, never hardcoded).

| Case | Serial p95 / cap (ms) |
|---|---:|
| Feed A (`unanswered_inquiry`) customized with an extra stage clause | 158 / 1,250 |
| Feed A disabled | 101 / 1,250 |
| Feed C (`call_outcome_needed`) disabled | 156 / 1,250 |
| Preview (`unanswered_inquiry`), 8 serial `preview_today_system_feed` calls | 125 / 1,250 |

The stage-clause case deliberately references the fixture's single
populated stage (every fixture Person shares it), so the candidate set is
unchanged — this measures the added predicate's evaluation **cost**, not a
narrower result set; correctness of narrowing itself is covered
exhaustively at the DB level in `db_today_system_feed_commands.rs`. Preview
was measured as a direct `commands::preview_today_system_feed` call
(real Postgres round trip, real read-only/timeout transaction) rather than
over HTTP, since the concentrated-book admin fixture here is not wired
through a full authenticated route dispatch in this smaller harness; this
is a disclosed simplification, not a different code path — preview's HTTP
handler (`routes/today_feeds.rs`) is a thin wrapper with no additional
per-request cost around the same command.

## Part 4 — `EXPLAIN (ANALYZE, BUFFERS)` of `person_state.sql`, vacuumed

`VACUUM (ANALYZE)` was run explicitly on `person`, `inquiry`,
`contact_attempted`, `correspondence_captured`, `contact_method` and `call`
immediately before both EXPLAIN runs (filling the visibility map
deterministically, rather than waiting on autovacuum timing as the 011c
archive's root-cause analysis had to). Both runs bind the SAME 51 real
parameters — organization id, both feeds' canonical `[assigned_to: me,
awaiting_response/client_replied_unanswered: true]` definitions, the
concentrated viewer as both `assigned_user_ids` and `viewer_id`, `now`,
both feeds enabled, both 24 h freshness windows — inside one transaction
via a savepoint, first without any planning override, then with `SET LOCAL
enable_mergejoin = off` (the transaction-local setting production already
applies unconditionally to every Today query, approved 2026-09-06 per
`docs/specs/SLICE_011c.md` §8).

| | Plan shape (`contact_attempted_corrects_once` guard) | Execution time |
|---|---|---:|
| Default (no override) | `Nested Loop Anti Join` | 533.8 ms |
| `enable_mergejoin = off` | `Nested Loop Anti Join` | 476.0 ms |

Full plans: [plans/person_state_default.txt](plans/person_state_default.txt),
[plans/person_state_enable_mergejoin_off.txt](plans/person_state_enable_mergejoin_off.txt).

**Verdict:** unlike the frozen `9d62e86` built-in query 011c diagnosed (a
knife-edge cost tie that flipped to a 2.2 s `Merge Anti Join` once the
visibility map filled), `person_state.sql` chose the fast `Nested Loop
Anti Join` for the correction-chain guard in **both** runs, on this same
concentrated fixture, under a freshly-vacuumed visibility map. The
`enable_mergejoin = off` setting made no plan-shape difference here (only
a ~58 ms/11% execution-time difference, well within run-to-run noise on a
loaded laptop); it does not need to independently "fix" `person_state.sql`
the way it fixed the old built-in statement. This is a reassuring
confirmation for the current statement and fixture, not a guarantee: 011c's
own finding was that this exact hazard is a cost tie sensitive to
`reltuples`/table shape, so a materially different data distribution could
still tip it — the existing unconditional `SET LOCAL enable_mergejoin =
off` (already applied to every Today query, this statement included) is
the correct standing protection regardless.

**Caveat (added for D-050):** neither of this Part's two runs set `SET
LOCAL jit = off` — both ran with **JIT ON** (the PostgreSQL session
default). Production applies `SET LOCAL jit = off` unconditionally to
every Today query (the pre-existing, approved planning change this Part
never touched). The 533.8 ms/476.0 ms absolute execution times above
therefore **overstate** what production actually spends on this
statement; only the plan-SHAPE comparison (both chose `Nested Loop Anti
Join`) is the load-bearing conclusion, per D-050's plan-shape gate.

## Part 5 — D-050 plan-shape evidence: `call_only.sql`, `call_membership.sql`, `filtered_summaries.sql`

Added per the coordinator's explicit review-round instruction ("no new
benchmark run"): a SEPARATE, smaller `#[sqlx::test]`
(`d050_plan_shape_evidence_call_statements_and_filtered_summaries`, same
file, same fixture builder) that seeds the concentrated book, builds the
fixture ONCE, but never re-runs the HTTP request matrix — only EXPLAIN
captures. The fixture's `call` table is normally empty (the Phase B
fixture never seeds calls), so 1,000 real qualifying calls were inserted
for the concentrated viewer as caller before `VACUUM (ANALYZE)` on
`person`, `inquiry`, `contact_attempted`, `call`, `correspondence_captured`
and `contact_method` — giving the plan real volume to reason about,
unlike an empty-table scan. Both `SET LOCAL jit = off` and `SET LOCAL
enable_mergejoin = off` were applied for all three captures (matching
production exactly, not a toggle comparison this time — D-050 dropped the
toggle question from the gate).

| Statement | Binding | Key plan facts | Execution time |
|---|---|---|---:|
| `call_membership.sql` | canonical (anchor only), 200 real retained ids | `Index Scan using call_org_caller_ended_idx on call` (Index Cond includes `caller_user_id`); the correction-chain guard is an `Index Scan using contact_attempted_org_causation_idx`, one indexed point-lookup per call (loops=200/1000), never a table-wide re-scan | 4.1 ms |
| `call_only.sql` | canonical, same 200 retained ids (so the other 800 called Persons are call-only candidates) | same `call_org_caller_ended_idx` index scan and per-call indexed correction lookup (loops=800/1000); hydration joins (`person_pkey`, `stage`, `app_user`, `contact_method`) all index scans | 11.5 ms |
| `filtered_summaries.sql` | the concentrated viewer's own book, the three NEW clauses (`awaiting_response`/`client_replied_unanswered`/`awaiting_call_outcome`) all **absent** (NULL) | `Index Scan Backward using person_organization_created_idx`, all LATERAL joins index scans (`inquiry_org_person_received_idx`, `contact_method_...`) — the same shape as before the three axes existed; their NULL-guarded predicates add no scan or join | 6.8 ms |

Full plans: [plans/call_membership.txt](plans/call_membership.txt),
[plans/call_only.txt](plans/call_only.txt),
[plans/filtered_summaries_no_new_clauses.txt](plans/filtered_summaries_no_new_clauses.txt).

**Verdict:** all three statements use real indexes for every hot path —
`call(organization_id, caller_user_id, ended_at)` for the call feed's own
membership probe (confirming no per-Person re-scan of `call` or
`contact_attempted`; each correction-chain check is one indexed lookup,
not a scan), and `filtered_summaries.sql`'s plan shape is unchanged by
the three new absent clauses. This satisfies D-050's plan-shape gate
("index use..., no super-linear growth with People") for every new or
changed hot statement.

## Record consistency (coordinator note, review round 2)

- Parts 1–4 were captured at lane head `145335a`; Part 5 at `a0413cd`. Two
  later changes post-date the Part 4 plans: the F1 fix (`30e1e5a`) wraps the
  `qualifying` CTE's matrices in `COALESCE(..., false)` (plan-shape neutral;
  no statement in the plans changed structure) and step 6 (`8884815`) deleted
  the `Legacy` provider and its test-clock router. `source-sha256.txt` was
  refreshed for `person_state.sql` after F1 but four of its entries
  (`db_today_feeds_http_perf.rs`, `today_http_perf_fixture.rs`,
  `today/mod.rs`, `routes/today.rs`) describe the pre-step-6 tree; the
  hashes are retained as captured, not regenerated, so they fingerprint the
  tree the runs actually executed on.
- Part 1 is a **retained artifact**: after step 6 the harness can no longer
  run `Legacy` live, and `db_today_feeds_http_perf.rs` carries the measured
  Part 1 row forward verbatim (`legacy_p95_ms: 204`, `feeds_p95_ms: 177`,
  allowance 229 ms, payload-equal) from `run.json` at `145335a`. A future
  run of the harness re-measures Parts 2–5 only; the Part 1 row it emits is
  this historical record, not a new measurement.

## Limits

- Single run, not the 011c archive's two-run failed/passed pair — no
  planner-hazard reproduction was attempted or needed here since the
  guard is already unconditionally applied in production.
- Fixture acceptance on one developer laptop with a 10-connection pool
  under desktop background load (load average ≈5-7 before the run per
  `environment.txt`), not a production capacity claim — same caveat 011c
  recorded.
- The three new-case measurements and preview were not repeated at
  concurrency 10/20; the coordinator's brief scoped them to serial only
  (cap 1,250 ms), consistent with `run.json`.
- Preview was measured at the command layer, not through
  `POST /api/organization/today-feeds/{feed_key}/preview` itself (see Part
  3's note).
- The EXPLAIN comparison used one representative parameter binding
  (canonical definitions, no admin customization); it does not explore the
  full predicate-matrix parameter space.
- Part 4's person_state EXPLAIN pair ran with JIT ON (see that Part's
  caveat) — its absolute times overstate production, which always runs
  with `jit = off`. Part 5's three captures ran with both `SET LOCAL`
  settings applied, matching production.
- D-050 supersedes the absolute performance caps this archive's Parts 2
  and 3 still report against (1,250/2,500/4,500 ms, 450 ms whole-source);
  those numbers are retained for trend-watching per D-050, not as pass/
  fail gates — the actual gate is Part 1's paired regression and Parts 4
  and 5's plan shape.

## Retained files

- [run.json](run.json) — the complete evidence JSON (fixture counts,
  regression, matrix, independent repeat, new cases, EXPLAIN plans).
- [harness-stdout.txt](harness-stdout.txt) — full harness stdout,
  including `test result: ok. 1 passed; 0 failed; ... finished in 113.75s`.
- [plans/](plans/) — five full `EXPLAIN (ANALYZE, BUFFERS)` plan texts:
  the `person_state.sql` default/`enable_mergejoin off` pair (Part 4) and
  `call_membership.sql`/`call_only.sql`/`filtered_summaries.sql` (Part 5,
  a second, much smaller `#[sqlx::test]` run — `test result: ok. 1
  passed; ... finished in 5.96s` — EXPLAIN-only, never a repeated HTTP
  benchmark).
- [environment.txt](environment.txt) — host, build, binary hash, git head.
- [source-sha256.txt](source-sha256.txt) — SHA-256 of every source file
  this harness and the measured statements depend on.

## Reproduce

```
cd backend
cargo test --release --locked -p crm-api --test db_today_feeds_http_perf \
  --features perf-harness --no-run
bin=target/release/deps/db_today_feeds_http_perf-<hash-suffix>
h=$(shasum -a 256 "$bin" | awk '{print $1}')
DATABASE_URL="$MIGRATION_DATABASE_URL" CRM_SLICE_011C_BUILD_HASH="$h" \
  "$bin" --ignored --exact slice_011d_authenticated_http_performance_harness \
  --nocapture --test-threads=1
```

`CRM_SLICE_011C_BUILD_HASH` is the fixture's inherited env var name from
011c (unmodified fixture file); it accepts any 64-char lowercase hex
string and is used only as a build-identity assertion, not a real 011c
comparison.

Part 5 (D-050 evidence) reuses the SAME built binary; only the test name
changes:

```
"$bin" --ignored --exact d050_plan_shape_evidence_call_statements_and_filtered_summaries \
  --nocapture --test-threads=1
```
