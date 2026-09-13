# Mobile 004 / 010e4 — Implementation status

**COMPLETE — 2026-09-13, D-080.** Both accepted implementations, both allowed
independent review rounds, native/browser acceptance, performance and final gates
are complete. Changes are committed locally on the integration branch; publication
and shared-development release are separate.

## Implemented scope

Mobile004 adds durable encrypted offline stage proposals on iOS and Android,
server-owned catalog revisions, idempotent replay, explicit conflict/replacement
and follow-up drafts. Actual installed Mobile003 stores were upgraded without
losing identity, keys, cache, drafts, queued work or receipts. Simulator/emulator
real-API offline/restart/replay and conflict flows passed.

010e4 adds preview, confirmation, retry, cancellation and exact-report remainder
for later core refresh of a terminal admission cohort. It preserves original
admission evidence, validates source/mapping/identity/baseline proof at commit,
holds local changes, and keeps imported workspaces in administrator review.
Metadata, notes/tasks and history for admitted People remain later slices.

## Evidence and review

- [Backend and mobile performance](MOBILE_004_BACKEND_VERIFICATION.md): 18 mobile
  DB cases, three ordinary stage/tenant/realtime regressions and the paired Today
  gate with 25k People/50 members. Baseline/current p95: 145.48/122.37ms.
- [iOS](MOBILE_004_IOS_VERIFICATION.md): installed schema6→7, real offline/replay/
  conflict/replacement, 35 model/storage tests and the follow-up UI regression.
- [Android](MOBILE_004_ANDROID_VERIFICATION.md): installed schema4→5, real API
  replay/conflict/replacement, persisted baseline follow-up and dirty-composer guard.
- [Migration](SLICE_010e4_VERIFICATION.md): exact source/target and permit cases,
  readiness, lifecycle recovery, desktop/390px cancellation/remainder and provenance.
  Two business updates across cancelled/completed resources, local edits retained,
  108 table fingerprints reconciled, exact byte accounting and zero reservations.

Mobile review findings were corrected and tested on installed native QA apps.
The second/final migration review recorded code READY at `872e896`, contingent
on verification now completed below. All targeted fixes are integrated through
lane `8aa0e62`. No third
review was opened for either slice.

Final source `7930b83` passes `scripts/check` and `scripts/check-db`; SQLx cache
regeneration is included at `c7d2720`. Results: 48 preflight, 989 ordinary Rust,
five documentation, 1,238 Web, 11 email-worker and **1,034 database tests**, plus
lint/type/build and fresh-schema cache checks. Both D-050 gates pass. The migration
verification owns exact commands, timings, source attribution and failed attempts;
subsequent integration changes are documentation only.

## Integration and resources

Integration branch: `codex/mobile004-010e4-integration`, based on published
`44dcf52` plus accepted planning documents. Backend/migration/iOS/Android
implementation used Terra high; the coordinator serialized shared files and DB
gates. All three completed clean writer worktrees were removed after integration.
All schema changes are additive: Mobile004 `20260929000001`, migration
`20260930000001`–`00010`. Previously applied migrations were preserved.

Evidence and isolated build outputs: `/private/tmp/crm-mobile004-010e4/`.
Native API3102/database `crm_mobile_004` and migration API3103/Web5174/database
`crm_010e4_qa` use synthetic data. The migration API was stopped before performance
measurement; its completed Web preview was also stopped. Evidence and QA databases
are retained. Shared development API3000/Web5173, the private native demo3101
and prior installed stores are preserved.

No main merge, push, deployment, live FUB, customer activation or physical-phone
claim is part of this milestone. Calling remains after the agreed progression.
