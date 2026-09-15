# Mobile006 / 010f4 — Implementation status

**IN PROGRESS — 2026-09-14, D-084.** The user requested “implement them”.
Both independent planning reviews returned READY in round 1; compatible checkpoint
notes are recorded in [planning review](MOBILE_006_010f4_PLANNING_REVIEW.md).
[Coordinated plan](../plans/MOBILE_006_010f4_PARALLEL_LAUNCH.md) owns the acceptance
matrix, source references and future final gates. Focused backend and Web checks
have passed. Both independent implementation reviews returned NOT READY in round 1;
[findings and repairs](MOBILE_006_010f4_IMPLEMENTATION_REVIEW.md) are authoritative
for the remaining review work. Full acceptance and final gates remain open.

## Ownership and resources

Root integration: `codex/mobile006-010f4-integration`. Mobile backend and migration
start in separate owned worktrees; iOS and Android follow the frozen mobile
backend. Terra high remains assigned to substantive writers. Root owns integration,
original/admitted metadata-worker barrier patches and `.sqlx`. Root now owns migration registration, workspace readiness, API worker/router
registration, release preflight, shared Web migration navigation, and both
query-plan harnesses. Native writers retain their platform-only ownership.
Each lane alone creates
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
- Mobile backend: frozen at writer `82d081e` and integrated through root
  `9185cb3`. Typed metadata composition, derived tokens, bounded catalogs,
  opted-in generations, receipts and current authorization are implemented.
  Seven command/import compatibility tests and seven direct metadata tests pass
  across retained focused batches. Six mobile HTTP selectors pass across their
  recorded batches. The catalog row-lock privilege/snapshot failure is repaired
  by a narrow read-only SECURITY DEFINER function; its blocked-writer regression
  passes. Failed attempts remain retained, not counted as passes.
- Migration: root owns all integration fixes and the expanded adversarial tests.
  The expanded 12-selector matrix passed, including selected-source People
  qualification, excluded coverage, old-binary handover, authorization/tenant
  boundaries, rollback and exact remainder continuation. The corrected browser fixture is seeded; actual
  desktop/390px acceptance remains pending.
- iOS and Android: implementations and initial live journeys are integrated.
  Both proved actual populated Mobile005 in-place upgrades. Independent review
  found additional draft/catalog/conflict issues; repairs and focused regression
  checks are in progress as recorded in the review file.
- Migration realistic query plans passed at 25k People/50 members with zero
  plan failures; separate storage sizing remains pending. Mobile query plans,
  combined gates and the single paired Person/Today run remain pending.
  No slice is implementation READY.

### Retained evidence

All logs are under the private QA root, with command/timing sidecars where run
through `run-check.py`:

- `logs/integration/metadata-compatibility-db-1.log`: 7 passed at `ec2ccdd`.
- `logs/integration/metadata-extra-boundaries-db-1.log`: 3 passed, observer failed;
  `metadata-tag-delete-concurrency-db-2.log`: corrected observer test passed.
- `logs/mobile006-focused-2.log`: 4 passed at writer `b744e4c`.
- `logs/mobile006-http-boundaries-2.log`: bounds passed, receipt URL failed;
  `mobile006-http-boundaries-3.log`: corrected authority test passed at `82d081e`.
- `logs/integration/admitted-activity-remainder-db-2.log`: confirmed-cancel and
  successor execution passed (6.202 test seconds) after the pointer/FK order fix.
- Migration writer Web/API 28 tests and preflight 51 tests passed as development
  checkpoints; final-tree gates remain required.

### Isolated runtime resources

Native API3106 runs frozen writer source `82d081e` from an immutable source copy
and separate binary against owned `crm_mobile_006_qa` (100 synthetic People,
three tags/four custom fields). Provider integrations are disabled. Its runtime
and catalog manifests are under `inventory/`. Do not overwrite its Cargo output.

Owned iOS simulator: `32978562-0A51-4E84-B55A-179BC5B28738` (`CRM-Mobile006-QA`,
iOS26.5). Owned Android AVD: `CRM_Mobile006_QA`, API37 ARM64. Historical Mobile005
source `a5cb24d` is preserved under `mobile005-source/`. Its iOS build-for-testing
and Android app/test APK assembly passed with isolated outputs. These builds alone
are not installed-store upgrade evidence.

The controlled migration browser uses Web5177 and API3107 with owned
`crm_010f4_qa`; worker units are explicitly budgeted through a private control
file. Root serializes this fixture with native live-API work and all DB checks.
Protected original API3000/Web5173 artifacts remain separate.

The Mobile006 receipt example now includes inherited `accepted_at`, which the
existing backend always serializes. This corrects an incomplete example without
changing the production HTTP receipt contract or native validation.

Publication/deployment of this pair, physical-phone/cellular tests, distribution,
live FUB/customer work, activation and calling remain separate.

- `logs/integration/f4-review-matrix-db-2.log`: 12/12 passed on the integrated
  source/handover fixes (97.29s test execution).
- `logs/integration/web-source-reprepare-r1.log`: 54/54 focused Web tests passed;
  Web lint and typecheck also passed in their separate recorded runs.

- `logs/integration/activity-hotplans-db-1.log`: realistic migration plans passed
  (227.05s); `integration/activity-hotplans-1.json` retains exact SQL/source hashes,
  cardinalities, plans and zero failures. Bulk scale rows are explicitly inert
  ciphertext cardinality fixtures, not source-fidelity or capacity evidence.
- `logs/integration/final-local-check-1.log`: preflight 51 passed; Rust Clippy
  found five style errors and one stale readiness test initializer. Repairs are
  under verification in the second run; this failed attempt is retained.
