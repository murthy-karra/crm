# Mobile 004 / 010e4 — Implementation status

**IN PROGRESS — 2026-09-13, D-080.** User accepted both specifications, shared
contracts and isolated implementation. No completion or release claim.

## Ownership and resources

Coordinator: integration branch `codex/mobile004-010e4-integration` from `44dcf52`
plus approved planning/acceptance documents. Independent planning review runs
read-only before the writers start. Backend/migration/iOS/Android writers use
Terra high. Shared changes and database gates are serialized by the coordinator.

Reserved schema versions: Mobile004 `20260929000001`; 010e4 `20260930000001`.
Proposed isolated APIs: mobile3102 and migration3103/Web5174, verified free at
launch preparation; isolated empty databases `crm_mobile_004` and `crm_010e4_qa`
were created and migrated through the published baseline. Mobile backend
`6992aa7` integrated at `04b5f35`; 010e4 checkpoint `d52c02a` integrated at
`ac7fa5a` for combined verification. These checkpoints are not completion.
Schema corrections `20260930000002`/`00003` preserve the applied first migration.
Native QA API3102 PID33892 uses a copied isolated binary; health/readiness pass.
Its database has synthetic People001–100 and all current additive migrations.
Private evidence root `/private/tmp/crm-mobile004-010e4/`; per-lane
Cargo targets under this directory, Web/native outputs isolated from shared dev.

Preserved runtime listeners observed: shared API PID93752/3000, Web PID93773/5173,
private native demo API PID71121/3101. Existing Mobile003/010e3 fixture evidence
and installed stores remain untouched.

## Gates

Independent planning review: Sol high, read-only against `c581e1e`, READY.
Required owner checkpoints are narrow catalog-revision maintenance during review
imports and exact OLD→NEW field validation for the admitted-refresh permit.
Both remain required implementation/tests, not unresolved product decisions.

The mobile-backend worktree is closed. Active Terra high writers:
`codex/mobile-004-ios` and `codex/mobile-004-android` from `04b5f35`, plus
`codex/migration-010e4`, in matching directories under
`/Users/karrad/projects/crm-worktrees/`.

Combined focused evidence: 18 mobile DB tests passed, including new stage
rollback, concurrent duplicate publication, scope/deletion and catalog tests;
3 ordinary stage/tenant/realtime regressions passed. Logs are under the private
`integration/` evidence directory. Preflight tests passed 48/48. Native/API
acceptance, migration lifecycle tests, D-050 evidence and final gates remain.
One migration test began during the mobile DB gate after compilation continued
automatically; it used disposable databases, failed early, and is not performance
evidence. Subsequent DB gates require an explicit slot grant.

Pending: complete implementation and
focused backend/migration/native acceptance, combined gates, bounded independent
implementation review, query/byte/preservation and actual browser/device-emulator
evidence. Physical phones/cellular, new family imports, calling, publication and
deployment remain separate.
