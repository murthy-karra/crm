# Mobile 001 / 010e1 — Implementation coordination

**In progress — 2026-09-12.** D-074/D-075 authorize implementation and isolated
synthetic checks. Native verification is still in progress; neither this record
nor backend integration claims the entire Mobile 001 slice complete.
Local integration: `codex/mobile-migration-integration`, approved planning
checkpoint `9eaeb0a`, combined backend/Web checkpoint `d6e7c74`. Main remains
`b301819fcf38947ee31f85b25588d9c5398795f0`; no Git publication or new release.

| Lane | Worktree / branch | Ownership and current state |
|---|---|---|
| iOS | `/Users/karrad/projects/crm-worktrees/mobile-001-ios`, `codex/mobile-001-ios` | `ios/` and its verification doc; native app build/runtime tests, isolated API3101, reserved Person080 |
| Android | `/Users/karrad/projects/crm-worktrees/mobile-001-android`, `codex/mobile-001-android` | `android/` and its verification doc; native app build/emulator tests, isolated API3101, reserved People020–025 |
| Migration | `/Users/karrad/projects/crm-worktrees/migration-010e1`, `codex/migration-010e1` | Implementation locally integrated; retained private API3102/DB `crm_migration_010e1` for real-browser verification |
| Coordinator / verification writer | Existing checkout | Coordinator owns shared code/docs and browser QA; verification writer owns the combined evidence doc and two specifically assigned legacy test-fixture corrections |

Mobile backend `cc3cb6b` and evidence `b06f086` are locally integrated. Its
worktree and branch were closed before the iOS worktree opened, preserving the
three-worktree maximum. Migration backend `6eac323` and shared/Web wiring
`fc1acc5` were integrated at `d6e7c74`. Each shared seam retains both additions.
Native writers use the frozen [mobile contract](MOBILE_001_CONTRACT.md) and the
same private synthetic API. The former backend build output is isolated at
`/private/tmp/crm-mobile001-target`; private runtime configuration is outside Git.

Focused backend DB/API tests, D-050 paired/query-plan checks, migration process
handoff and real-API desktop/390px Web walkthrough have passed. Combined
`./scripts/check` and live SQLx preparation passed; the full DB regression gate
uses an immutable archive to prevent concurrent Cargo cleanup from removing its
runner. Native build/persistence/failure/UI proof remains active. Exact commands,
counts and failures are attributed in the respective verification records.

Each lane has isolated credentials, databases/builds and explicit test ownership.
No FUB/AI/telephony credentials were supplied to native writers. SQLx preparation
is serialized. An initial root Web verification build incidentally replaced the
shared preview's build directory; every released asset was restored and verified
against the 010d2 hash manifest. [Restoration evidence](../design/qa/slice-010e1-2026-09-12/shared-preview-restoration.json).
Subsequent QA serves a separate output directory; no shared process reset or
customer processing occurred.

Native dependency pins are resolved, but resolution alone is not app evidence:
SQLCipher.swift/SQLCipher Android4.19.0, AGP9.3.1, Gradle9.5.0, Kotlin and Compose
compiler2.4.20, Compose BOM2026.09.00, Room2.8.5, KSP2.3.12, SQLite2.7.1.
Actual native build/runtime results belong to the platform verification records.
