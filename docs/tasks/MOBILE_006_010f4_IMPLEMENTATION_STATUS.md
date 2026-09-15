# Mobile006 / 010f4 — Implementation status

**IN PROGRESS — 2026-09-14, D-084.** The user requested “implement them”.
Both independent planning reviews returned READY in round 1; compatible checkpoint
notes are recorded in [planning review](MOBILE_006_010f4_PLANNING_REVIEW.md).
[Coordinated plan](../plans/MOBILE_006_010f4_PARALLEL_LAUNCH.md) owns the acceptance
matrix, source references and future final gates. Focused backend and Web checks
have passed; full acceptance and independent implementation review remain open.

## Ownership and resources

Root integration: `codex/mobile006-010f4-integration`. Mobile backend and migration
start in separate owned worktrees; iOS and Android follow the frozen mobile
backend. Terra high remains assigned to substantive writers. Root owns integration,
original/admitted metadata-worker barrier patches and `.sqlx`. The migration writer
has exclusive reassigned ownership of migration registration, workspace readiness,
API worker/router registration, release preflight and shared Web migration navigation.
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
- Mobile backend: integrated typed command composition, revision schema, receipts,
  opted-in generations and bounded metadata/catalog reads. Library checks pass;
  direct command/import compatibility passes seven focused database tests.
  Mobile HTTP event/no-op/replay passes; opted-in generation tests exposed a
  catalog read-lock privilege failure, whose concurrency-safe repair is pending.
- Migration: backend/Web integrated. The retained preparation/execution fixture
  passes, including four mapping roles and exclusive admitted activity identities.
  Cancellation, remainder, readiness and browser acceptance remain in progress.
- iOS and Android: wait for integrated mobile contract.
- Combined gates, installed upgrades, browser/native acceptance and independent
  implementation review: pending.

Root checkpoint `67086d6` composes ordinary typed tag/value helpers into the atomic
metadata command; the earlier original/admitted metadata-worker barrier is now
integrated. Isolated `cargo check -p crm-app --locked` passed in
`logs/integration/typed-metadata-check-1.log`. Direct command tests cover capacity,
all value types, atomic validation, no-ops, ABA, unrelated notes and current
actor/tenant/workspace checks. These and four changed original/admitted import
compatibility tests passed at `ec2ccdd` (seven tests, 11.568 seconds after compilation;
total 106.43 seconds), `logs/integration/metadata-compatibility-db-1.log`.
The first all-test build found two new mobile-test UUID comparisons; their fixed
rebuild passed, `logs/integration/typed-metadata-test-compile-2.log`. Failed evidence
is retained. Mobile writer `9a4cd7c` passed its content-free event/no-op/replay test
and failed two opted-in generation cases with HTTP503; that batch is retained at
`logs/mobile006-focused-1.log` and is not an acceptance pass.

Migration checkpoint `7ff7619` passed the retained source→mapping→confirmation→
native identity test (one test, 6.827 seconds after compilation), recorded in
`logs/activity/db-admitted-activity-execution-r1.log`. Its earlier focused Web/API
suite passed 28 tests and release-preflight suite passed 51 tests. These are
development checkpoints on the writer revision, not final-tree acceptance.

Native QA now has a newly owned iOS simulator `32978562-0A51-4E84-B55A-179BC5B28738`
(`CRM-Mobile006-QA`, iOS26.5) and Android AVD `CRM_Mobile006_QA` (API37 ARM64).
Neither has been used for acceptance yet. Historical Mobile005 native source at
`a5cb24d` is archived under the private QA root's `mobile005-source/` for the actual
installed-store upgrade. Native API port3106 is planned and must be checked again
before binding. Resource manifests are under `inventory/`.

Historical Mobile005 iOS `build-for-testing` passed (17.924 seconds) and Android
upgrade QA app/test APK assembly passed (48.925 seconds), recorded in
`logs/ios-prep/historical-mobile005-build.log` and
`logs/android-prep/historical-mobile005-build.log`. Builds use isolated outputs and
dependency caches; neither app has been installed for this proof yet. The owned
`crm_mobile_006_qa` database has been created but remains empty pending stable
schema integration. No API3106 service is running yet.

Publication/deployment of this pair, physical-phone/cellular tests, distribution,
live FUB/customer work, activation and calling remain separate.
