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
| Mobile backend | Verified and integrated at `2fd9a9d`; worktree/branch closed | Terra high / former `codex/mobile-002-backend` |
| Migration backend then Web | Implementing retained People refresh and recovery | Terra high / `codex/migration-010e2` |
| iOS | Implementing protected offline editing and native QA from `2fd9a9d` | Terra high / `codex/mobile-002-ios` |
| Android | Implementing equivalent behavior concurrently from `2fd9a9d` | Terra high / `codex/mobile-002-android` |

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

## Backend foundation checkpoint

The [bounded source review](MOBILE_002_BACKEND_REVIEW.md) is READY at `2fc753a`.
Directly inspected logs show final Mobile 002 focused tests 2/2, complete mobile
DB tests 12/12 and notes tests 19/19. Mobile tests overlap those focused cases.
Root live `scripts/sqlx-prepare` passed in an isolated target/throwaway database
(72-second compilation); `.sqlx` remains clean. Task regressions and repository
checks run serially after an earlier cache-regeneration collision prevented the
task test attempt from executing. Native integration remains pending.

The migration preflight has 39 passing Python tests, including partial-schema
fail-closed behavior. Its baseline test has reported a first passing real
PostgreSQL parent/capture/report/preview flow. Migration execution, recovery,
Web and complete acceptance evidence remain in progress; no scaffolding or
conservative placeholder outcome is treated as completed refresh behavior.

Foundation coordinator gates now have passing evidence on unchanged source:

- `scripts/check` Rust formatting, Clippy, production compilation and dependency
  fences passed, with 968 Rust tests and five compile-fail doctests. Web lint and
  typecheck passed. Its first Web run passed 1,180/1,181; the remaining test
  exposed the coordinator's isolated config `VITE_API_BASE_URL=''`, which produced
  `/session` instead of `/api/session`. No application source was changed.
- After correcting both private QA configs to `/api`, all 1,181 Web tests passed,
  the Web production build passed in the disposable worktree output, and all
  11 email-worker tests passed. Unchanged Rust gates were not repeated.
- Remaining `db_tasks::` regression passed 34/34 in 27.13 seconds under the
  explicit isolated migrator test URL. No test was counted from its earlier
  cache-regeneration collision.

Root logs are retained under `/private/tmp/crm-mobile002-qa/`: `check-1.log`,
`sqlx-prepare-1.log`, `web-test-2.log`, `web-build-1.log`, `email-worker-1.log`
and `db-tasks-2.log`. The actual private API and native fixture are being prepared
on a separate Cargo target; the final combined migration DB gate remains later.

## Native launch

The verified backend foundation is integrated locally at
`2fd9a9d8c81bc0bfe40873b589412ab2ccac3b91`. Its worktree and merged branch were
closed before creating `mobile-002-ios` and `mobile-002-android` under the same
worktree parent. Both Terra high native writers are running alongside the one
migration writer, with exactly three implementation worktrees. Coordinator
executor-QA authoring transferred its new test file to the migration writer to
free the native slot; those executor tests had not yet run at that handoff.

The real synthetic API is `/private/tmp/crm-mobile002-qa/runtime/crm-api`, SHA-256
`1235cfd33dfb34f7150e79486be574b04c81e73dd4c800f7c575f6cf03a75042`.
The preparation agent exercised bootstrap, current-note read and an accepted edit.
Its original PID19981 exited after handoff; root observed connection refusal and
restarted the same binary with detached process/file logging. **Current PID26687**
listens on loopback3102; readiness and a separate post-launch health/listener
check passed. The runtime working directory is outside the closed worktree.

Fixture inventory is 100 People, 1,000 notes and 1,000 tasks. Native reservations
are in `/private/tmp/crm-mobile002-qa/runtime/native-reservations.json`: iOS001–049,
Android051–099, shared050 by coordination, and100 held for coordinator checks.
The primary synthetic actor is a member and the second is a fixture admin for
authorized conflicts. The demo API3101 and demonstrated app/store remain intact.

The existing same-build `mobile_today_perf` example passed with unchanged Today
DTOs, 40 samples per side, old p95 17.120666ms and new p95 17.379083ms against a
25ms permitted increase. Root inspected the actual JSON log; this is isolated
synthetic regression evidence, not a production capacity or native success claim.
