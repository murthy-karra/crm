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
Web realtime tests pass. Readiness now requires the additive per-unit settlement
schema (`75b40fd`; 51 preflight tests pass).

Migration worker checkpoint `4dd5e69` is integrated. Six focused database tests
pass for all four supported field types and tags, frozen local-value protection,
exact accounting/replay, catalog and Person rollback/retry, private permit
boundaries and missing-unit-schema rejection. `crm-app` all-target Clippy passes.
These results do not complete staged preparation, plan/recovery lifecycle or the
Web workflow; those remain in progress. After repeated Terra checkpoints did not
complete typed execution and accounting, the coordinator escalated that concrete
blocker to Astra high under `MODEL_ROUTING.md`. The migration writer retains
exclusive Rust/SQL ownership; a literal Web contract checkpoint precedes Web
handoff to the coordinator.

The mobile backend contract is frozen and integrated through `382aadc`. Eight
focused DB/API checks pass, including capture/profile lock overlap, complete
bounded traversal, tenant denial, atomic receipt/replay/no-op/conflict and
publish-failure recovery. Legacy mobile and capture regressions also pass.
The native API fixture is served by an immutable private copy of the `382aadc`
binary on3103; it includes the required empty `added_contact_ids` array for
name-only/edit/remove receipts, while old receipt kinds omit that field.
iOS checkpoint `125d044` is integrated as `a19fc88`: populated-store upgrade,
39 storage/model tests, real API replay, full multi-field offline/restart/sync
and conflict/replacement UI proofs pass. Coordinator fixes then passed42/42
iOS checks, including both actual native journeys. Android `9134a44` is integrated
as `43d8302`, with Mobile004/005 storage and lint/compile evidence. Its claimed
full native acceptance remains **unverified**: independent review found the cited
restart log ends in failure, the cited discard log is empty, and captured installed
upgrade probe evidence is missing. The separate conflict/replacement log passes;
it does not turn the incomplete restart journey into a pass. The writer is locating
valid existing artifacts or repeating the required proofs while preserving stores.
The large-contact fixture
exposed action placement problems, now fixed in iOS and checked in Android.

The literal010f3 Web contract is integrated, and Web implementation is checkpointed
at `b90c416`. Focused component/view checks, lint, typecheck and the isolated
production build pass. The real synthetic API3104/browser5174 journey has passed
login, completed-import/cohort selection and390px overflow/error checks; full
preparation/execution/remainder acceptance awaits the migration transport and
staged-preparation checkpoint. Original/admitted shared catalog compatibility is
integrated through `1d2d2b7`; new staged preparation remains in the migration lane.

Mobile005 independent implementation review round1 is active. Four actionable
backend/iOS findings cover current-profile concurrency, direct revision changes,
native display/primary fallback order and same-transaction add order. Coordinator
fixes are committed at `63ccae4`, with five focused DB regressions and final
iOS42-test verification passing. Android round1 then identified old-server
summary staging rejection, an unreachable primary-removal warning, omitted profile
drafts in pending/removal accounting, and missing current-capability UI gating.
Its writer owns those corrections plus an explicit audit of earlier historical-suite failures.
No implementation review has been declared READY. Each lane records concrete
schema/DTO/lock/guard contracts and runs isolated focused checks. Coordinator owns shared integration and sequential
final `scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db`; database gates do
not overlap. Independent implementation review, real native/browser acceptance,
populated-store upgrade and D-050 evidence remain required.

Private evidence is under the listed root: `integration/preflight-inventory-tests-fixed.log`,
`integration/web-realtime-tests.log`, `integration/mobile005-integrated-tests3.log`,
`integration/mobile005-legacy-and-capture-regression.log`, `mobile/db_mobile004.log`
and the migration lane's verification record. Xcode reports26.6 (17F113);
native QA uses the iOS26.5 simulator and Android37 emulator with isolated identities.
The four protected API/Web listeners remain at their original PIDs and ports.
Actual commands, failures and tested revisions will be linked as lanes complete.
No check is passed merely because its requirements were reviewed. Two implementation
review/fix rounds per slice maximum; no third review authorized.
