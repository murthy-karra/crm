# Mobile 003 / 010e3 — Implementation status

**In progress — D-078, 2026-09-13.** Both contracts and Mobile 003 occurrence time
are accepted. Terra high owns implementation; the coordinator owns integration,
shared seams and final verification. No completion claim is made yet.

## Allocation

Integration: `codex/mobile003-010e3-integration`, from main closeout `619c1b3`
plus approved planning/acceptance documents. Follow the
[coordinated plan](../plans/MOBILE_003_010e3_PARALLEL_LAUNCH.md).

| Lane | Worktree / ownership | Assigned resources | State |
|---|---|---|---|
| Mobile backend | `crm-worktrees/mobile-003-backend`; mobile/contact core, fixtures, tests | Additive migration `20260927000001`; API3102 / `crm_mobile_003` | Integrated `c1e0999`, `c3d5b47`, typed SQLx follow-up `fef40d8`; worktree closed |
| Migration backend/Web | `crm-worktrees/migration-010e3`; admission modules/schema, new Web components | Additive migration `20260928000001`; API3103 / Web5174 / `crm_010e3_qa` | Implementation checkpoint; new lifecycle acceptance and runtime query repairs in progress |
| iOS | `crm-worktrees/mobile-003-ios`; `ios/` and platform evidence | Isolated QA app/store against API3102, People001–049 | Terra high implementation and verification in progress |
| Android | `crm-worktrees/mobile-003-android`; `android/` and platform evidence | Isolated QA app/store against API3102, People051–099 | Terra high implementation and verification in progress |

The 2026-09-13 listener inventory found API3000 PID49067, Web5173 PID49082,
demoAPI3101 PID71121 and PostgreSQL5432. Ports3102/3103/5174 were free. Recheck
before binding. Preserve those shared/demo services and installed native stores.
Each lane uses its own Cargo/Web/native output directories; no root release
artifact is replaced. Coordinator serializes fixed-name SQLx/DB gates and shared
registration/guard/preflight/history/profile edits. At most three implementation
worktrees; close the mobile backend before opening both native worktrees.

## Evidence

Planning doc/link checks passed before acceptance. Mobile foundation evidence is
in [the backend record](MOBILE_003_BACKEND_VERIFICATION.md). Coordinator checks:

- Independent admission readiness/preflight: 44 Python tests and 6 Rust workspace
  readiness tests passed on the isolated target. The schema inventory additionally
  checks all three nullable identity-origin UUID columns.
- `scripts/sqlx-prepare`, with isolated `CARGO_TARGET_DIR`, passed against a fresh
  throwaway schema. Its first attempt exposed a missing SQL alias closing quote
  in the typed fact projection; corrected before the successful run and committed
  with generated metadata in `fef40d8`.
- `cargo build -p crm-api --bin crm-api --bin migrate --locked` passed on
  `/private/tmp/crm-mobile003-integrated-target`; it never overwrote shared artifacts.
- API3102 runs a private copy of that binary (PID80802 at launch). Its isolated
  `crm_mobile_003` clone has 44 migrations. Real HTTP health, login, bootstrap
  capability, contact acceptance and same-fact replay passed. IDs/receipts and
  binary SHA are in private `/private/tmp/crm-mobile003-qa/` evidence.
- Three focused `mobile003_` real SQLx/router tests passed together: occurrence
  normalization/replay; older-contact/newer-Inquiry chronology and exact sealed
  Today parity with no Person revision bump; receipt-trigger failure rollback,
  current workspace/membership/tenant authority, and retained consumed IDs after
  Person removal. Exact command is added to the backend record.

QA setup cloned the retained synthetic Mobile002 database; the original is intact.
Only four empty bootstrap contexts with no receipt or reconciliation references
were removed from the clone to leave capacity for isolated native installations.
All receipt-bearing legacy contexts were retained. Native writers must reuse
stable QA installation IDs and preserve installed-store upgrade evidence.

The admission schema installs on the isolated database and 10 existing refresh
regression tests pass; new admission behavior is **not yet verified**. Its copied
runtime queries are being repaired through direct admission integration tests.
Native runtime, new Web workflows, query measurements and combined gates remain
pending; no completion claim follows from compilation or schema installation.
Physical devices/cellular, broad mobile redesign, publication/deployment,
distribution, live FUB/customer work and activation remain later scopes.
