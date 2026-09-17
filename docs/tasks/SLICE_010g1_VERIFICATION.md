# 010g1 combined family refresh — verification

Local acceptance under D-092 and the accepted combined-family plan. No merge,
push, deployment, live FUB access or customer processing is authorized by this
record. **Local implementation acceptance passed on 2026-09-17.**

## Implementation and review

The delivered package includes whole-Person metadata preparation, native
note/task deltas, immutable history corrections, execution and lifecycle,
retained-source APIs, and the common Web workflow. Exact Remainder and narrow
legacy-proof reconstruction landed in `12586c3`. Real Web acceptance exposed a
mixed-family terminal-state bug; `ebe05b6` preserves confirmed cancellation when
a sibling completes, and its regression verifies the successor and byte audit.

One independent implementation reviewer used both D-050 rounds. The exclusive
Confirm barrier and Web ready-subset findings were fixed; round 2 closed without
actionable findings at `12586c3`. Subsequent changes are verification fixes,
covered by the final checks, not a third review round. See the
[review record](SLICE_010g1_IMPLEMENTATION_REVIEW.md).

## Functional and Web evidence

- Repository gate: `/private/tmp/010g1-final-check.log` — 1,037 Rust tests,
  1,327 Web tests across 103 files, 56 preflight checks, formatting, Clippy,
  production compilation/build, crate fences, doctests and 11 email-worker tests.
  This preceded the small mixed-family lifecycle and final query-plan fixes.
- Final-tree Clippy: `/private/tmp/010g1-final-clippy.log` — workspace/all targets,
  `test-support`, warnings denied. Final formatting and diff checks pass.
- Focused DB: `/private/tmp/010g1-legacy-remainder-final-db.log` — 40 passed;
  `/private/tmp/010g1-partial-completion-fix.log` — 1 passed. Earlier full family
  evidence: `/private/tmp/010g1-remainder-verified-family.log` — 54 passed.
- Real synthetic production-Web: `/private/tmp/010g1-final-browser`, with the
  isolated API test passing in `/private/tmp/010g1-final-browser-api-2.log`.
  Desktop/390px workflows exercised retained selection, mapping choices,
  destructive counts, local-change holds, committed lost-response Confirm and
  byte-identical retry, partial cancellation, exact successor completion, reload,
  current/prior history versions and logout clearing. No page errors or horizontal
  overflow. The final mobile dialog bounds and scrolled versions were inspected
  in screenshots 14/15; the early confirmation screenshot caught an animation
  frame and is not the visual acceptance evidence.

The Web sequence used real commands and workers. Its only response interception
lost a real committed confirmation response. Selector/login-timing failures are
retained in the original logs and were corrected in follow-up browser checks.
The pre-lifecycle-fix failed run is retained separately at
`/private/tmp/010g1-final-browser-before-lifecycle-fix`.

Fresh-schema SQLx/offline-cache validation passed. The serial broad run at the
application code in `dc26ccb` ran 1,193 tests in 4,805.147 seconds: 1,190 passed
and three older People-refresh accounting audits failed
(`/private/tmp/010g1-final-db-gates.log`). Those audits omitted D-084 retained
mapping evidence, fingerprints, digests, repair rows and mapping-head ownership.
The independent physical byte inventory now includes them, retaining exact
equality with the runtime ledger. All three corrected tests passed in 18.283
seconds (`/private/tmp/010g1-final-audit-rerun.log`). Runtime accounting did not
change. Thus all 1,193 functional cases are verified across the broad run and
focused correction; the initial failures remain visible in the evidence.

The serial gate excluded four explicit manual fixtures/older plan harnesses and
did not enable `perf-harness`. Isolated API/preview processes were stopped after
Web acceptance. The plan runner also tolerates its indexes already being present
on a newly migrated fixture; this does not change the production migration.

## D-050 query plans

Final evidence: `/private/tmp/010g1-final-query-plans-corrected/plans.json`, exact
SQL/bindings/hashes and full `ANALYZE, BUFFERS` trees; `inspection.json` summarizes
83 shapes. The book has 25,000 People and 50 memberships, approximately 25k rows
per family and 25k history identities/corrections split across events/calls/texts,
including dense per-Person history and unknown dates. Relation clones are inert:
setup suppresses row/FK triggers inside a transaction, restores normal trigger
behavior before EXPLAIN, and rolls back. This is cardinality/index evidence;
normal guarded database tests establish fidelity and authorization.

The first evidence run exposed three avoidable scans. Migration 034 adds scoped
Person-preview and result-outcome indexes; the remainder summary now fetches the
first eligible position through the existing ordered partial index. Explicit
assertions verify all three families' indexes and nonempty current/prior history
reader results. Source reconciliation and walkers use scoped indexes. Dense
history may choose one linear original-fact scan plus indexed head, identity and
correction joins. Empty remainder tails may exhaust their scoped candidates.
Inspection found no repeated large inner sequential scan or disk spill. Laptop
latencies are descriptive, not capacity limits.

Earlier plan evidence and setup failures remain in
`/private/tmp/010g1-final-query-plans*`. One setup exceeded its 60-second insert
timeout; a later slow setup was explicitly cancelled before changing inert clone
setup. Neither is counted as a passing query-plan run. Source-payer bindings and
settled/unfinished fixture positions were corrected before final collection.

Reproduction instructions and runners are in
[family_refresh_acceptance.md](../../backend/crates/crm-api/tests/fixtures/family_refresh_acceptance.md).

## D-050 paired Person/Today gate — passed

One measured paired run, with `CRM_MOBILE006_PAIRED_TODAY=1`, passed in 103.827
seconds (`/private/tmp/010g1-final-paired-measurement.log`). Evidence is in
`/private/tmp/crm-010g1-person-today/person-detail-paired.json` and
`mobile006-today-paired.json`. Both arms share a build, fixture and clock; each has
40 measured requests at concurrency one, alternating AB/BA. The fixture has
25,000 People, 50 members, 75,018 notes and 100,017 tasks. Counts stayed unchanged.

| Reader | Baseline p95 | Current p95 | D-050 current limit | Equality |
| --- | ---: | ---: | ---: | --- |
| Person detail | 19.23 ms | 21.60 ms | 44.23 ms | Complete JSON and bytes equal; frozen entry points verified |
| Today | 62.33 ms | 63.26 ms | 87.33 ms | All DTOs equal; all samples complete |

The initial invocation stopped in source-integrity preflight before any request
measurement (`/private/tmp/010g1-final-paired.log`). Comparison with `2a4c207`
proved that two shared authorization helpers changed only by adding the
transaction-local family-reader stamp to their existing SELECT. Their manifest
hashes/provenance were refreshed; the three frozen reader bodies remain
byte-exact. As before, the pair uses current shared authorization and is not a
full historical authentication-stack or production-capacity comparison. The
failed preflight is retained, and no second measured benchmark was run.

Post-audit workspace/all-target Clippy with `test-support` and warnings denied
passed (`/private/tmp/010g1-final-audit-clippy.log`). Final formatting, browser-script
syntax, plan-script syntax and diff checks pass. No acceptance gate remains open.

## Scope limits

Retained qualified evidence only. This does not establish full FUB synchronization,
cutover readiness, source deletion semantics, unsupported/private audiences,
catalog structural repair, email/media, native workspace activation, customer
erasure/restore readiness, or O-012/O-013 resolution. Deployment remains deferred.
Unrelated `docs/prompts/MODEL_ROUTING.md`, `.lavish/` and the shared runtime are
preserved. Rust and Web build outputs are isolated under `/private/tmp`.
