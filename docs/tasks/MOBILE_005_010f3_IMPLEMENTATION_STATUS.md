# Mobile005 / 010f3 — Implementation status

**IN PROGRESS — D-082, 2026-09-14.** Implementation and isolated synthetic verification
accepted after both planning review rounds returned READY. No implementation
verification has completed yet. Source publication/deployment remains separate.

## Ownership and resources

- Coordinator: root checkout on `codex/mobile005-010f3-integration`; shared modules,
  guards, SQLx, Web realtime and final integrated verification.
- Mobile backend: `codex/mobile-005-backend`, worktree
  `/Users/karrad/projects/crm-mobile005-backend`; new migrations use `20261001` prefix.
- Migration: `codex/migration-010f3`, worktree
  `/Users/karrad/projects/crm-010f3`; new migrations use `20261002` prefix.
- Native lanes start only after the mobile contract and backend integrate and
  the backend worktree closes. At most three implementation worktrees.
- Private evidence/build root: `/private/tmp/crm-mobile005-010f3`, with separate
  `mobile`, `migration`, `integration`, `ios`, `android` directories. Free QA ports
  at launch: 3103/3104/5174/5175; assign before starting a service.
- Preserve observed API PIDs61780/71121/33892 on ports3000/3101/3102 and Web PID59404
  on5173. PostgreSQL/Centrifugo are shared supporting services. No shared artifact
  is rebuilt by verification; preserve native stores and prior release evidence.

## Current work and gates

Mobile backend and migration are starting from the same accepted source base.
Each freezes concrete owned schema/DTO/lock/guard contracts, implements its brief
and runs isolated focused checks. Coordinator owns shared integration and sequential
final `scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db`; database gates do
not overlap. Independent implementation review, real native/browser acceptance,
populated-store upgrade and D-050 evidence remain required.

Actual commands, failures and tested revisions will be linked as lanes complete.
No check is passed merely because its requirements were reviewed. Two implementation
review/fix rounds per slice maximum; no third review authorized.
