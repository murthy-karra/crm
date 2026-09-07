# Slice 011d — Verification record

Status: VERIFIED (2026-09-07), awaiting the user's merge approval.
Coordinator: Claude Fable 5.1. Lanes: Claude Sonnet 5. This record holds
every actual result; nothing here is claimed without having been run.

Specification: [SLICE_011d.md](../specs/SLICE_011d.md) (approved 2026-09-07;
§8 amended by pointer to D-050). Brief: [SLICE_011d_IMPL.md](SLICE_011d_IMPL.md).
Base: `main` at `66b44ff`. Integration branch: `slice-011d-today-system-feeds`.

## Lane ledger

| Round | Lane | Commits | Content | Lane gates (actual) |
|---|---|---|---|---|
| 1 | W | `ab5f7b3`, `ef31a9b` | Steps 1–4: types/hooks, three chips + locked mode, `/manage/today-feeds`, Today Rules section and notices | lint, typecheck, Vitest 43 files / 560 tests, build: all green |
| 1 | B | `99e8d99`, `bd631c2`, `0280e1f` | Steps 1–3: vocabulary across eleven statements, migration `20260908000001`, seed, `Legacy \| Feeds` seam, `db_today_feed_equivalence.rs` | `check` 699 tests; `check-db` 471 of 471 |
| 2 | B | `5ffeb2e`, `a4dc96b`, `c0f9bd6` | Three corrections (no issue for a disabled feed; per-axis parity across count and sorts; call-feed connection recovery with three failure-injection tests); step 4 commands, preview, six routes, Operator field, telemetry, 15 command tests | `check` green; `check-db` 493 of 493 |
| 3 | B | `401c18e`, `549243a`, `145335a`, `97cdbec` | Call feed bound to the full filter matrix (coordinator decision); fallback and preview-timeout tests; Operator parity tests; step 5 performance evidence | `check` green; `check-db` 504 of 504 |
| merge | coordinator | `3fa7ca8`, `3496d71` | Both lanes merged into the integration branch, 113 files, no conflicts | not run at merge (final-tree gates run once at the end) |
| 4 | W | `00f35f6` | Step 5 browser walkthrough evidence | Web gate green on the lane tree |
| 5 | B | `30e1e5a`, `26984cc`, `febf080`, `46a87bd`, `cfba96a`, `a0413cd`, `8884815` | Review round 1 fixes F1–F5 and T4–T13, a 413 body-cap mapping bug found and fixed, D-050 call-statement EXPLAINs, step 6 (`Legacy` deletion, SQL frozen under `tests/fixtures/today_f51bff8/`) | `check` green; `check-db` 541 of 541 (lane run) |
| 5 | W | `02c2842` | Preview session-identity fence; explicit 409 reload notice with the editor kept open on refetch failure (a page-blanking bug on failed background refetch found and fixed); marker and typed-confirm pins | lint, typecheck, Vitest 45 files / 572 tests, build: all green |
| merge | coordinator | `3bc0d8c`, `72130ab`, `77a8963` | Final rounds merged (144 files against main); perf-archive consistency note (docs only, after the gates started) | see Final-tree gates |

Coordinator file-list audits: every lane round was diffed against the
integration branch; Lane B touched only `backend/` and the perf archive, Lane
W only `web/` and the QA archive. Lane B's shell once reset its working
directory to the user's main checkout and ran `git add -A backend` and `git
status` there; both were no-ops on a clean tree (verified by the coordinator).

Interruptions: Lane W's agent stalled twice for ten minutes with no lost work
(committed as a checkpoint and resumed); Lane B's agent was cut off once by
the machine sleeping, resumed without loss; Lane B's context was compacted
once mid-round (its report was re-verified against git).

## Production defects found before merge

1. Preview issued `SET TRANSACTION ... READ ONLY` before the membership
   `FOR SHARE` lock, which PostgreSQL rejects, so every non-trivial preview
   failed. Found and fixed by Lane B in round 2 (`c0f9bd6`); pinned by the
   preview tests.
2. The two call-feed statements bound no filter matrix, so any clause an
   admin added to that feed was ignored. Reported by Lane B, decided by the
   coordinator (extend, not restrict), fixed in round 3 (`401c18e`).
3. `person_state.sql` projected a nullable boolean into a non-null decode:
   a person-state feed customized with `assigned_to: [me, unassigned]` plus
   one unassigned Person with an inquiry and an unanswered reply would make
   Today a 503 for the whole Organization. Found by the reviewer in round 1
   (BLOCKING). Fixed in `30e1e5a`; pinned by two regression tests that
   reach the NULL path; closed by review round 2.
4. The two call-feed statements evaluated age clauses at `now()` rather than
   the bound evaluation clock, breaking the one-clock rule for that feed.
   Found by the tester and reviewer. Fixed in `30e1e5a` (bound clock in
   both statements and preview); closed by review round 2.
5. `UpdateTodaySystemFeed` validated references before the revision check,
   returning 422 where spec §6 requires 409. Fixed in `26984cc`; closed by
   review round 2.
6. Oversize request bodies on the feed routes mapped to 400 instead of the
   413 the rest of the API uses. Found by Lane B while writing the HTTP
   wire tests; fixed in `febf080`.
7. A failed background refetch blanked the whole Today rules page,
   including an open editor, although good data was still cached. Found by
   Lane W while testing the 409 flow; fixed in `02c2842`.

## Equivalence gate (spec §9.2)

Independent reviewer verdict on the merged tree (`3496d71`): **CONFIRMED**.
By SQL analysis the `Feeds` path reproduces the compiled-in statement at
canonical definitions: rule-7 inquiry constraint on all three feeds;
reply-wins precedence against feed B's edited definition; strict freshness
with `make_interval(hours)`; ordering inputs before `LIMIT 201`; hydration
over the prefix only; one call per Person by `ended_at DESC, id DESC`; the
call-only prefix only when person-state was not truncated with limit
`(200 − |P|) + 1`; B = P ∪ call-only then the unchanged 011c list path;
`TodayReason`, tiers and display order unchanged. `db_today_feed_equivalence.rs`
compares byte-identical `TodayList` JSON under both providers with and
without a list source; `db_today_builtin_parity.rs` (frozen `today_9d62e86`
SQL) runs against `Feeds`. Cases the reviewer found missing (fresh boundary
at exactly `now − window`, reply equal to the latest outbound, dual People
beyond 201, 2–3 call-only rows at the 199/200/201 boundary, caller ≠
assignee with an assigned callee) were pinned in `cfba96a` before the
deletion. `Legacy` deleted in `8884815`; the compiled-in statement is frozen
under `tests/fixtures/today_f51bff8/` (SQL byte-identical to `f51bff8`, with
import paths and a doc comment adjusted as the README discloses), and the
equivalence suite now compares `Feeds` against that fixture. Review round 2
confirmed no provider seam remains in production code.

## Performance (D-050 gates)

Archive: [`docs/design/perf/slice-011d-2026-09-07/`](../design/perf/slice-011d-2026-09-07/README.md),
one run, all attempts retained.

| Gate | Result |
|---|---|
| Paired regression, `Legacy` vs `Feeds`, concentrated book, serial p95 | 204 ms vs 177 ms (allowance 229 ms); payloads equal apart from `system_feed_issues`. PASS |
| Plan shape, `person_state.sql`, concentrated 30k book, vacuumed | Seq scan on person by org and assignee, per-row index probes, 100-row sort, `LIMIT 201`, hydration laterals at `loops=100`; identical with and without `enable_mergejoin = off`. PASS |

Trend data, not gated (D-050): the 011c matrix at 1/10/20 concurrency all
complete; five-source concurrency-20 request p95 2,514 and 2,764 ms with
pool-wait max 1,511 ms; new cases (feed A customized, feed A disabled, feed C
disabled) 101–158 ms serial; preview 125 ms at the command layer. Both
`SET LOCAL` settings are kept unchanged; the toggle question is closed by
D-050. Disclosures: the harness is smaller than 011c's; the EXPLAIN pair ran
with JIT on (production runs `jit = off`), so its absolute times overstate;
call-statement EXPLAINs (`call_only.sql`, `call_membership.sql`,
`filtered_summaries.sql` with the new clauses absent) were added in
`a0413cd`: index scan on `call_org_caller_ended_idx` with the caller in the
index condition and per-call indexed correction lookups, no table re-scan.
The archive's capture heads and the retained Part 1 row are annotated in
`77a8963`.

## Browser walkthrough (spec §9.12)

Archive: [`docs/design/qa/slice-011d-2026-09-07/`](../design/qa/slice-011d-2026-09-07/README.md),
24 of 24 scripted assertions, 16 screenshots. Scratch runtime on
`:31011`/`:51011` against a scratch database seeded through the HTTP API;
torn down and verified by the coordinator (ports free, database dropped, the
user's dev processes untouched). Covered: locked anchor and `me`; preview
with the member picker; save and the "Customized by" status; the agent's
Today and "Changed by your admin" marker updating; revert; typed
confirmation on turn-off and confirmation-free turn-on; the reply feed with
a 1-hour window; the three chips on People and in a saved list enabled as a
source; second-Organization isolation; member 403 and hidden nav. Not
reproducible through the application: the fallback state (no stage-deletion
path exists); skipped by instruction: the partial notice (failure injection
only). No defects found.

## Review rounds (D-050: at most two)

**Round 1** on `3496d71`. Reviewer: READY WITH FIXES (F1 blocking NULL
boolean; F2 call-feed clock; F3 precedence; F4 no HTTP wire tests; F5
missing equivalence cases; F6/F7 LATER: preview has no fixed-clock seam,
EXPLAIN ran with JIT on). Tester: the same clock and precedence defects, the
cap × call-only case, a mislabeled failure test, a weak revert-fact test, a
vacuous telemetry test, cheap boundary and idempotency tests, two Web items;
Operator parity had no gap; concurrency-20 items tagged BEYOND_ENVELOPE.
Disposition: all in-envelope items assigned (Lane B round 5, Lane W round 5).

**Round 2** on `72130ab`, limited to the fixes: **READY**, no fix
required. Every round-1 item was verified CLOSED with file references.
Recorded as LATER under the two-round cap: a person-state-statement 503
test (cheap via revoking a table grant; the feed-row 503 is covered), an
equal-timestamp correction pin and the preview exact-200 pin, the feeds
page's first-load error branch test, a spinner that stays after a discarded
preview settlement on a real session change (fail-closed), and a comment
wording nit in `person_state.sql`.

## Final-tree gates (once, coordinator)

Run by the coordinator in `../crm-worktrees/011d-integration` at `72130ab`
(the later `77a8963` is a README-only change):

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | complete; zero `.sqlx` drift files afterwards |
| `./scripts/check` | all checks passed, 95 s: 699 Rust tests, Web lint/typecheck/build, Vitest 45 files / 572 tests |
| `./scripts/check-db`, first run | FAILED at 35 of 541: `db_calls::a_second_correction_chains_onto_the_first_with_strictly_increasing_recorded_at` (history ordering assertion); nextest stopped fail-fast |
| Same test in isolation | integration tree: failed 2 of 3 runs; **main: failed 1 of 3 runs**. The file is untouched by 011d and the only production diff in that area is the additive fact type. Pre-existing timing flake, not a regression |
| `./scripts/check-db`, second run | all checks passed, 180 s: **541 tests run, 541 passed**, 699 skipped |

## Residuals (LATER)

- `db_calls::a_second_correction_chains_onto_the_first_with_strictly_increasing_recorded_at`
  is a pre-existing intermittent failure on `main` (1 of 3 isolated runs);
  it should get its own fix outside this slice.
- The three largest test files grew substantially this slice
  (`db_today_system_feed_commands.rs` about 2,950 lines); consider splitting
  in the next Today rung.

- Preview has no fixed-clock test seam; §9.6 parity is verified as id-set
  containment, not payload equality (reviewer F6).
- The person-state EXPLAIN pair ran with JIT on (reviewer F7).
- Beyond the D-050 envelope: concurrency-20 pool-wait margin and absolute
  caps are trend data only.
- The fallback state cannot be reached through the application today (no
  stage deletion path); it is pinned by database tests only.
