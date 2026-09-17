# 010g1 — Implementation status

**COMPLETE LOCALLY — D-092, 2026-09-17.** The accepted combined plan and contracts
are implemented and verified on `codex/010g1-family-refresh`. The accepted Lavish
session was not reopened. Merge, push and deployment remain deferred.

## Delivered

- Retained-source preparation, frozen original/admitted/recovered cohorts,
  explicit mappings, bounded reviews, exact source and storage accounting.
- Whole-Person tag/custom-field deltas; note/task additions and updates;
  immutable event/call/text corrections with current and retained-version readers.
- Independently proven legacy baselines. Missing proof, local changes, ABA,
  ambiguous source and unsupported data stay held rather than being overwritten.
- Atomic typed execution, Confirm/Cancel/Resume, revocation pauses and exact
  Remainder across interrupted or mixed-family attempts. Settled catalog
  prerequisites are referenced without re-execution or double counting.
- Scoped APIs and the common Web workflow: selection, mapping, preview, explicit
  confirmation, lost-response replay, progress/results, cancellation, remainder,
  history versions and private-state clearing.
- Bounded scheduler integration, compatibility barriers, full release/preflight
  schema inventories and indexed review queries through migration 034.

## Acceptance

- Repository gate: 1,037 Rust tests, 1,327 Web tests, 56 preflight checks, production
  build/shape, doctests and supporting checks. Final Clippy and formatting pass.
- SQLx fresh-schema/offline-cache validation passes. All 1,193 DB/API cases are
  verified: 1,190 in the broad serial run, then three corrected independent
  accounting-audit reruns. The original failures remain recorded.
- Real synthetic production-Web acceptance passes at desktop/390px, including
  committed lost-response recovery, partial cancellation, exact remainder,
  reload, current/prior history and logout. Mobile dialogs were visually checked.
- 83 query-plan shapes inspected at 25,000 People/50 members. One measured paired
  Person/Today run passes response equality and D-050 relative p95 limits.
- Both independent implementation-review rounds are closed. Subsequent
  verification fixes and the source-integrity preflight correction are documented;
  no third review or second measured benchmark was used.

[Final verification and evidence](SLICE_010g1_VERIFICATION.md) is the authoritative
record of commands, results, retained failures, provenance and limitations.
[Project history](../plans/PROJECT_HISTORY.md) retains earlier milestones.

## Preserved boundaries

No merge, push, deployment, live FUB/customer processing or activation. The shared
runtime, unrelated `docs/prompts/MODEL_ROUTING.md` and `.lavish/` are preserved.
Isolated API/preview processes were stopped. Build outputs and evidence remain
under `/private/tmp`. Full synchronization, cutover, unsupported/private data and
O-012/O-013 remain outside this accepted slice.
