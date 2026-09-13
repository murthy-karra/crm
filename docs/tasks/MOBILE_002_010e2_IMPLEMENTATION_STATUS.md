# Mobile 002 / 010e2 — Implementation status

**IN PROGRESS — D-076, 2026-09-12.** The user approved both reviewed contracts
and confirmed Terra for implementation. No repeat implementation approval is
pending. [The launch plan](../plans/MOBILE_002_010e2_PARALLEL_LAUNCH.md) defines
scope, dependencies and ownership. Physical-phone/cellular work and broad mobile
design cleanup remain deferred.

Local approval checkpoint: `9cbaf1a`, integration branch
`codex/mobile002-010e2-integration`. Main and shared development remain on the
previous completed milestone; these local checkpoints are not publication or
deployment.

| Lane | Actual state | Primary writer / branch |
|---|---|---|
| Mobile backend | Implementing additive revision/operation/read contracts and tests | Terra high / `codex/mobile-002-backend` |
| Migration backend then Web | Implementing retained People refresh and recovery | Terra high / `codex/migration-010e2` |
| iOS | Native QA preparation complete; implementation waits for verified mobile foundation | Terra high planned / `codex/mobile-002-ios` |
| Android | Same dependency and equivalent acceptance scope | Terra high planned / `codex/mobile-002-android` |

Coordinator applies shared registration/authorization changes serially. A bounded
Terra coordinator subtask owns only release-preflight script/tests in the migration
worktree; it does not become a fourth feature worktree or edit migration/native
domain files. Fixed SQLx preparation and full DB gates require coordinator
scheduling. Focused per-test ephemeral DB runs are admitted for both backend lanes.

## Owned resources

- Two initial worktrees under `/Users/karrad/projects/crm-worktrees/`:
  `mobile-002-backend` and `migration-010e2`; both branch from `9cbaf1a`.
- Empty isolated PostgreSQL databases created successfully: `crm_mobile_002`
  and `crm_010e2_qa`. Creation alone is not migration/fixture/test success.
- Private mode-0600 runtime configs:
  `/private/tmp/crm-mobile002-qa/runtime.env` and
  `/private/tmp/crm-010e2-qa/runtime.env`; no credential values in this record.
- Mobile QA reserves API3102; migration reserves API3103/Web5174. No listener
  is claimed launched at this checkpoint. Shared API3000/Web5173 and demo3101
  are preserved. Isolated Cargo/Vite/Xcode/Gradle outputs are mandatory.
- Additive schema ownership: Mobile `20260925000001`, migration `20260926000001`.
  The mobile foundation integrates before native branches open.

## Evidence boundary

Planning reviews and documentation checks passed before D-076; they do not prove
implementation. Native preparation inspected existing tooling/test paths and
recorded an isolated QA app/store approach. It did not install, erase, launch,
build or test either app. Required implementation, schema, failure/retry,
authorization, real-API/native/Web and combined regression evidence remains open.
Record each actual checkpoint and limitations here as work completes.
