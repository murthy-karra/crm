# 010g1 combined family refresh — verification

Local acceptance under D-092 and the accepted combined-family plan. No merge,
push, deployment, live FUB access or customer processing is authorized by this
record. Final SQLx/database and paired-performance results are pending.

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

## Remaining final gates

Fresh-schema SQLx/offline-cache check and serial DB/API/compatibility suite:
`/private/tmp/010g1-final-db-gates.log` (running). The single paired Person/Today
run remains pending and must set `CRM_MOBILE006_PAIRED_TODAY=1`.

## Scope limits

Retained qualified evidence only. This does not establish full FUB synchronization,
cutover readiness, source deletion semantics, unsupported/private audiences,
catalog structural repair, email/media, native workspace activation, customer
erasure/restore readiness, or O-012/O-013 resolution. Deployment remains deferred.
Unrelated `docs/prompts/MODEL_ROUTING.md`, `.lavish/` and the shared runtime are
preserved. Rust and Web build outputs are isolated under `/private/tmp`.
