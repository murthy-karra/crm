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
launch preparation; databases `crm_mobile_004` and `crm_010e4_qa`. No service
started yet. Private evidence root `/private/tmp/crm-mobile004-010e4/`; per-lane
Cargo targets under this directory, Web/native outputs isolated from shared dev.

Preserved runtime listeners observed: shared API PID93752/3000, Web PID93773/5173,
private native demo API PID71121/3101. Existing Mobile003/010e3 fixture evidence
and installed stores remain untouched.

## Gates

Pending: independent planning findings, frozen contracts, implementation and
focused backend/migration/native acceptance, combined gates, bounded independent
implementation review, query/byte/preservation and actual browser/device-emulator
evidence. Physical phones/cellular, new family imports, calling, publication and
deployment remain separate.
