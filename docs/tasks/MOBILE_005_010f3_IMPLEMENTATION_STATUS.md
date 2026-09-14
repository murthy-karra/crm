# Mobile005 / 010f3 — Implementation status

**IN PROGRESS — D-082, 2026-09-14.** Implementation and isolated synthetic verification
accepted after both planning review rounds returned READY. Focused backend and
shared-contract checks have begun; integrated/native acceptance remains pending.
Source publication/deployment remains separate.

## Ownership and resources

- Coordinator: root checkout on `codex/mobile005-010f3-integration`; shared modules,
  guards, SQLx, Web realtime and final integrated verification.
- Mobile backend: `codex/mobile-005-backend`, worktree
  integrated as `bfac8b4`; its clean worktree is closed and its isolated target
  moved to `integration/target` for serial reuse. New migrations use `20261001` prefix.
- Migration: `codex/migration-010f3`, worktree
  `/Users/karrad/projects/crm-010f3`; new migrations use `20261002` prefix.
- Native lanes now own `crm-mobile005-ios` / `codex/mobile-005-ios` and
  `crm-mobile005-android` / `codex/mobile-005-android`, both from `bfac8b4`.
  Exactly three implementation worktrees including migration.
- Private evidence/build root: `/private/tmp/crm-mobile005-010f3`, with separate
  `mobile`, `migration`, `integration`, `ios`, `android` directories. Free QA ports
  at launch: 3103/3104/5174/5175; assign before starting a service.
- Preserve observed API PIDs61780/71121/33892 on ports3000/3101/3102 and Web PID59404
  on5173. PostgreSQL/Centrifugo are shared supporting services. No shared artifact
  is rebuilt by verification; preserve native stores and prior release evidence.

## Current work and gates

Both implementation lanes began at accepted checkpoint `fcc05b3`. Shared readiness
and realtime integration is checkpointed at `a565145`: 51 preflight tests and 20
Web realtime tests pass. Migration checkpoint `deb444b` reports six original
metadata gate tests and eight workspace-readiness unit tests passing; the earlier
four guard failures remain recorded in its evidence. These focused passes do
not verify the full admitted workflow.

The mobile backend contract is frozen, compilation/parser checks pass, and its
Mobile004 stage regression passes. Contact-trigger corrections, new details
DB/API acceptance and coordinator capture-overlap tests remain in progress.
The `bfac8b4` backend handoff is integrated and both native lanes are active. Each lane records concrete
schema/DTO/lock/guard contracts and runs isolated focused checks. Coordinator owns shared integration and sequential
final `scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db`; database gates do
not overlap. Independent implementation review, real native/browser acceptance,
populated-store upgrade and D-050 evidence remain required.

Private evidence is under the listed root: `integration/preflight-inventory-tests-fixed.log`,
`integration/web-realtime-tests.log`, `mobile/db_mobile004.log` and the migration
lane's verification record. Xcode currently reports 26.6 (17F113); the iOS26.5
simulators and Android37 emulator exist but were shut down at the current inventory.
The four protected API/Web listeners remain at their original PIDs and ports.
Actual commands, failures and tested revisions will be linked as lanes complete.
No check is passed merely because its requirements were reviewed. Two implementation
review/fix rounds per slice maximum; no third review authorized.
