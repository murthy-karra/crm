# Mobile006 / 010f4 — Implementation status

**IN PROGRESS — 2026-09-14, D-084.** The user requested “implement them”.
Both independent planning reviews returned READY in round 1; compatible checkpoint
notes are recorded in [planning review](MOBILE_006_010f4_PLANNING_REVIEW.md).
[Coordinated plan](../plans/MOBILE_006_010f4_PARALLEL_LAUNCH.md) owns the acceptance
matrix, source references and future final gates. No implementation acceptance
checks have passed yet.

## Ownership and resources

Root integration: `codex/mobile006-010f4-integration`. Mobile backend and migration
start in separate owned worktrees; iOS and Android follow the frozen mobile
backend. Terra high remains assigned to substantive writers. Root owns shared
guard/router/preflight/registration integration and `.sqlx`. Each lane alone creates
its feature migrations: Mobile006 prefix `202610030000`, 010f4 `202610040000`.

Private QA root: `/private/tmp/crm-mobile006-010f4-thyhauvv`. Protected running
API3000/Web5173 artifacts are hashed under its `inventory/`; PostgreSQL/Centrifugo
are available. No simulator was booted at initial inspection. Native retained
stores, prior QA data and backup/recovery artifacts must be preserved. Isolated
Cargo/Web/SQLx/Xcode/Gradle paths are mandatory. Serialize all DB-backed tests, SQLx
preparation and performance through the coordinator. Never print `.env` contents.

The prior [Mobile005 release](MOBILE_005_010f3_RELEASE.md) is still recorded IN
PROGRESS; this implementation does not claim or repeat that rollout.

## Current checkpoints

- Planning: independently READY, zero blocking findings per slice.
- Mobile backend contract/code/verification: pending.
- Migration contract/backend/Web/verification: pending.
- iOS and Android: wait for integrated mobile contract.
- Combined gates, installed upgrades, browser/native acceptance and independent
  implementation review: pending.

Publication/deployment of this pair, physical-phone/cellular tests, distribution,
live FUB/customer work, activation and calling remain separate.
