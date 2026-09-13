# Mobile 001 / 010e1 — Implementation coordination

**Implemented and verified — 2026-09-12.** D-074/D-075 authorize the completed
Mobile 001 backend, iOS, Android and 010e1 implementation with isolated synthetic
verification. This completes the approved first offline field workflow; physical
device/cellular validation, customer readiness and release remain separate.
Local integration: `codex/mobile-migration-integration`, approved planning
checkpoint `9eaeb0a`, combined backend/Web checkpoint `d6e7c74`, lifecycle correction `7f41907`,
iOS merge `152dcec` and Android merge `c238b24`. Main remains
`b301819fcf38947ee31f85b25588d9c5398795f0`; no Git publication or new release.

| Lane | Worktree / branch | Ownership and current state |
|---|---|---|
| iOS | Merged worktree/branch removed after `6e3f715` integration | Native app and complete synthetic verification; full derived data/XCTest bundles preserved at `/private/tmp/crm-ios-native-build-final` |
| Android | Merged worktree/branch removed after `83379df` integration | Native app and synthetic verification complete; full build/reports/APKs preserved at `/private/tmp/crm-android-native-build-final` |
| Migration | Merged worktree/branch removed after `fc1acc5` integration | Real-browser verification passed; owned API3102/Web5202 stopped, synthetic evidence retained |
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
`./scripts/check` and live SQLx preparation passed. All 966 DB cases have passing
evidence: 964 from the complete immutable archive run and two corrected legacy
fixture tests from a targeted archive. iOS passed 20 storage/model tests, actual
100-action lost-response proof, native terminate/relaunch UI, five repeated
refreshes and device Release compilation. Android passed encrypted-storage and
account-boundary instrumentation, actual 100-note lost-response proof, native
force-stop/relaunch with radios disabled, emulator reboot/reauthorization,
date-picker draft restoration and create/completion plus repeated refreshes.
Android final Debug/test builds, two JVM tests and lint passed (zero errors, five
advisories); seven focused instrumented cases passed, followed by the corrected
date-draft UI evidence capture. Exact commands,
counts and failures are attributed in the respective verification records.

Each lane has isolated credentials, databases/builds and explicit test ownership.
No FUB/AI/telephony credentials were supplied to native writers. SQLx preparation
is serialized. An initial root Web verification build incidentally replaced the
shared preview's build directory; every released asset was restored and verified
against the 010d2 hash manifest. [Restoration evidence](../design/qa/slice-010e1-2026-09-12/shared-preview-restoration.json).
Subsequent QA serves a separate output directory. Cargo verification also
replaced the three root executable pathnames; the running API continued mapping
its prior executable inode. All three exact released binaries were recovered
from retained dependency outputs, hash-verified and atomically restored without
restarting the shared API. Independent release copies are retained outside Cargo
outputs. [Executable restoration](../design/qa/mobile-001-native-2026-09-12/shared-executable-restoration.json).
No shared process reset or customer processing occurred. Future verification
must set isolated Cargo targets and Web output directories while this development
launcher points inside the checkout.

Native dependency pins are resolved, but resolution alone is not app evidence:
SQLCipher.swift/SQLCipher Android4.19.0, AGP9.3.1, Gradle9.5.0, Kotlin and Compose
compiler2.4.20, Compose BOM2026.09.00, Room2.8.5, KSP2.3.12, SQLite2.7.1.
Actual native build/runtime results belong to the platform verification records.

Both native apps exposed a retained-generation capacity defect. Correction
`7f41907` passed all ten affected mobile DB tests, formatting, Clippy and live
SQLx verification; both apps passed repeated downloads on the rebuilt private
API. Exact legacy synthetic test rows were explicitly retired with all receipts
and business rows preserved; no legacy rows were guessed to be successfully
sealed by the product migration. See [backend correction](MOBILE_001_SEALED_GENERATION_VERIFICATION.md)
and [private API/fixture evidence](../design/qa/mobile-001-native-2026-09-12/README.md).

Final platform records: [iOS](MOBILE_001_IOS_VERIFICATION.md),
[Android](MOBILE_001_ANDROID_VERIFICATION.md); retained screenshots live in
`ios/evidence/` and `android/qa/`. Worktree cleanup is recorded in
[integration evidence](../design/qa/mobile-001-native-2026-09-12/integration-cleanup.json).
The single loopback-only API3101 remains running for the installed synthetic
native demos; both apps are paused, with local saved data retained. Private runtime
configuration is `/private/tmp/crm-mobile001-qa/runtime.env` (outside Git).
No customer data, release signing or app distribution was used. The shared010d2
API/Web processes were not restarted; exact released artifacts are restored.

The next release scope can publish/integrate these local commits and refresh
shared development after its concrete release checks. Physical-device passcode,
restart and poor-cellular checks remain required before customer mobile use.
No request for repeated implementation approval is pending.
