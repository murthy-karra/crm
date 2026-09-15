# Mobile007 / 010d3 — Implementation status

Updated 2026-09-15. **IMPLEMENTED AND VERIFIED — D-086.** The accepted Mobile007
and 010d3 contracts are implemented and their required gates pass.
[Integrated verification](MOBILE_007_010d3_FINAL_VERIFICATION.md) owns the current
acceptance matrix, command results, retained failures and evidence identity.

## Scope and ownership

[Accepted coordinated plan](../plans/MOBILE_007_010d3_PLAN.md),
[Mobile007](../specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md),
[010d3](../specs/SLICE_010d3.md). Root owns integration/shared files and isolated
verification setup. Existing reviewers own only their separate planning records.
Writer worktrees and exact API/schema/device allocations are assigned at handoff.

## Retained resources

- Base main `d8367a7`; integration branch `codex/mobile007-history010d3`.
- Private QA root `/private/tmp/crm-mobile007-010d3-yyoxjx50`; private check environment
  and ownership manifest prepared. Build outputs remain isolated from running services.
- Observed existing listeners: API3000 PID78285, Web5173 PID79921, native API3106
  PID52273, PostgreSQL5432/Centrifugo8000. No service was restarted or replaced.
- `.lavish/` contains local planning artifacts; excluded from code checkpoints.

## Implemented surface

- Mobile API: bounded, no-store Organization People search with tenant and
  migration-review authority checks, deterministic ordering, exact email/phone
  matching, and 25-result paging metadata.
- iOS and Android: explicit online Organization search, capability gating,
  transient epoch-fenced results, result metadata, and save/cancel offline pin
  actions using the existing encrypted bundle/reconciliation path.
- Native recovery: durable requested-pin lists, whole-set retry/cancel, retained
  sealed selection reasons, account/query fencing and in-place encrypted upgrades.
- 010d3 backend: additive admitted-history schema and owner extensions, source
  qualification/prepare, confirm/resume/cancel/budget/remainder lifecycle,
  bounded manifest/result readers, and admin no-store routes. Retained-only workers
  use shared global history identities, immutable facts, exact reservations,
  durable remainder cursors and independent release/reader/worker fences.
- Web: bounded preview/counts, source interval and coverage acknowledgement,
  confirmation, progress, cancellation, explicit recovery and exact remainder.

## Verification records

- [Shared checks and acceptance mapping](MOBILE_007_010d3_FINAL_VERIFICATION.md).
- [Actual desktop/390px Web journey](SLICE_010d3_WEB_EVIDENCE.md).
- [D-050 harnesses and results](MOBILE_007_010d3_PERFORMANCE.md).
- [Mobile review rounds](MOBILE_007_IMPLEMENTATION_REVIEW.md) and
  [migration source review](SLICE_010d3_IMPLEMENTATION_REVIEW.md).
- Native progress is retained in
  `/private/tmp/crm-mobile007-runtime-20260915/native-progress.json`. Both complete
  native discovery/download/editor journeys pass. [Final iOS evidence](MOBILE_007_IOS_EVIDENCE.md)
  also covers process restart, interrupted catalog recovery and an actual installed upgrade.

## Release boundary

No implementation acceptance gate remains. Both real API/native journeys,
process-restart checks and actual installed Mobile006 upgrades pass, alongside
the backend, database, Web and D-050 checks. The integrated record retains the
initial failed attempts and the passing corrections.

Publication/deployment, live-source/customer work, activation and native distribution
remain outside D-086 implementation authorization.
