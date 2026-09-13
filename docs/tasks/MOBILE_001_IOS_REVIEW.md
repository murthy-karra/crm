# Mobile 001 iOS — Targeted review-fix verification

**Targeted fixes verified — 2026-09-12.** This is the bounded recheck of the six iOS findings
and due-date/Today corrections in
[the implementation review](MOBILE_MIGRATION_IMPLEMENTATION_REVIEW.md), against
[the approved native specification](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md).
No third broad review, source edits, competing build, simulator mutation or
additional live API upload run was performed by this reviewer.

## Source attribution

Reviewed `/Users/karrad/projects/crm-worktrees/mobile-001-ios`, main native
checkpoint `92abb916c92e6a797caaa081d38c72fce1550817` (base `feab066`).
The first provisional snapshot was captured at `2026-09-13T02:05:39Z` in
`/private/tmp/crm-ios-review-source.json`. Only the subsequent controlled-response
seams/tests, staging recovery and named findings were rechecked after that
snapshot. Final checkpoint SHA-256 values:

| File under `ios/FieldCRM/` | SHA-256 |
|---|---|
| `SecureStorage.swift` | `996737990f021572f1ea2cf443ba3299f4cced72a5049b6fb504388057d56745` |
| `FieldModel.swift` | `754e82b85576dfae1fa5eeaac4af86638ae447fdbf3fa304632ab5c7fe0cb258` |
| `LocalStore.swift` | `a10a65dcc67e1f5ca19416c472fff290a1164a1e29868ef1e55953411e30c697` |
| `FieldCRMApp.swift` | `0fdf5cf27104abc0156ae0531f2ff3737034253f189d1c15c7ebc789f83c6501` |
| `API.swift` | `fece7336ae1b2f088b99a3667d4ca2d969722e3daabc7a2a25179005a35c601a` |

## Finding-by-finding evidence

| Finding | Targeted source conclusion | Completed execution evidence and limits |
|---|---|---|
| Sleep-inclusive access/generation clock | `ClockSample.current` and generation expiry use `mach_continuous_time` converted with the Mach timebase. Lease validation preserves boot identity, elapsed-time monotonicity, expiry and wall-rollback checks. | Lease test covers encoded reopen, seven-day boundary, reboot, rollback and monotonic regression. The installed iPhoneSimulator SDK `usr/include/mach/mach_time.h:58–62` explicitly documents sleep advancement. No physical-device sleep/reboot experiment was run. |
| Durable sign-out lock | `lock` removes in-memory access immediately, writes and fsyncs an independent protected marker, and falls back to deleting only the credential if marker persistence fails. `restore` checks the marker before reading the old credential. It clears only after successful online authorization and opening that identity-bound store; another account opens a separate store. | Marker test forces credential-write failure and verifies a new `SecureStorage` sees the lock. The signed simulator Keychain test separately writes/reopens a this-device-only key. The final controlled model test additionally opens an authorized account with pending work, forces credential replacement to fail during the actual lock path, and verifies reopening remains locked with identical envelope bytes. |
| Authoritative 403 and task-specific denial | Reconciliation/read 401/403 locks. An operation 403 calls `/api/me`, checks actor/Organization and operational workspace, then records a task-specific failure only if current authority succeeds; inability to verify authority locks. | Final controlled model tests drive actual sync: reconciliation 403 hides cache and remains locked on reopen; task-operation 403 plus valid `/api/me` becomes attention while cache remains readable; revoked authority locks and preserves exact envelope bytes. Earlier live API proof separately covers cross-actor 404 and context 401. |
| Refreshed completion conflict | The original immutable envelope stays in the queue. After reconciliation supplies a different task revision with current manage permission, the UI offers a separate, explicitly chosen completion using that revision. Completed tasks do not offer the action. | Live API test proves a new stale completion conflicts; immutable-envelope/dependency tests prove preservation. The actual refreshed-action UI path has source evidence, not a completed interaction assertion in the reviewed logs. |
| Obsolete cache reclamation and pace | Cache-only batches preserve active/staged generation references, drafts and operations. Schema v3 adds the Person/revision member index. The model drains backlog with an actor/lease check and `Task.yield` between batches before reconciliation and after promotion. | Thirteen-test signed storage run includes active/staged/draft/outbox preservation and 2,501 obsolete members, 401 pages and 151 bundles—each above its batch limit. Repeated cleanup drains all obsolete rows and keeps the active bundle. The test exercises actual SQLite cleanup; placement/yield in orchestration is source-verified. |
| Update-required versus user pause | `updateRequired` is separate from `paused`, persisted in SQLite, restored on open, checked at sync admission, and shown independently in the UI. Toggling offline mode cannot override it. | The final controlled model test delivers `protocol_unsupported`, toggles pause off/on and reopens the model, then verifies update-required remains set and no further requests are issued. |
| Saved due date and Today labels | Composer initializes its picker/toggle from the saved due date. Today displays the server evaluation timestamp and local task-overlay count without inventing ranking. | Source-verified. Existing UI test does not assert restoration of a dated draft or these specific Today labels. |

## Test-log attribution

The reviewer read the actual test bodies and completed logs; execution belongs to
the platform implementer. Both logs target iPhone 17 simulator
`F5BF9C74-37AC-4546-A576-F30587CB4F4C`, iOS 26.5. Runtime identity was confirmed
with read-only `simctl list` using the Xcode developer directory.

- `/private/tmp/crm-ios-tests3.log`: **12 passed**, zero failures. Eleven storage
  tests plus one real-API test. The latter saves 100 immutable operations, closes
  and reopens SQLite, discards one actual successful response, retries the same
  bytes, verifies replay/resource identity, uploads the queue, checks task-create
  dependency, stale conflict, changed-payload denial and actor/context isolation.
  It calls `API` and `LocalStore` directly; it is not an app-orchestration/UI test.
- `/private/tmp/crm-ios-storage4-signed.log`: **13 storage tests passed**, zero
  failures, 1.955 seconds. Includes the larger-than-batch cleanup and actual
  simulator Keychain reopen proofs. Command uses `CODE_SIGN_IDENTITY=-`.
  These storage cases overlap the earlier eleven; they are not 25 distinct tests.
- The implementer reported that the earlier unsigned simulator UI launch could
  not use Keychain. The completed signed storage run verifies the corrected
  simulator signing path. Production key accessibility remains device/passcode
  bound. The subsequent SwiftUI test passed, as attributed below.

The first command selected `FieldCRMTests` with `CODE_SIGNING_ALLOWED=NO`; the
second selected `FieldCRMTests/StorageTests` with ad-hoc signing. Both used
`xcodebuild -project ios/FieldCRM.xcodeproj -scheme FieldCRM -configuration Debug
-destination id=F5BF9C74-37AC-4546-A576-F30587CB4F4C -derivedDataPath ios/.build test`.
Their `.xcresult` locations are recorded at each log's end.
After integration, the coordinator removed the clean iOS worktree and preserved
its complete derived data at `/private/tmp/crm-ios-native-build-final` (formerly
`ios/.build`). Retained XCTest result bundles are under `Logs/Test/` there;
historical log paths still name the original worktree location.

## Closing evidence for the requested fixes

`/private/tmp/crm-ios-final-focused.log`: **20 passed**, zero failures, 2.793
seconds, at the final native checkpoint: fourteen storage cases and six
controlled-response model cases. The model tests use DEBUG-only API response
injection with actual status/envelope decoding, actual `FieldModel.sync`,
identity-isolated Keychain namespaces and file-backed SQLite. Release uses
URLSession directly and contains no response-injection branch. These tests close
the provisional access-denial, protocol-stop and model-lock proof gaps without
repeating the 100-operation live API test.

The sixth model test covers a retired-generation 404: only staging is discarded,
the active bundle/draft stay intact, a new generation seals successfully, and no
component reads occur when the complete bundle revision matches. This directly
supports the client recovery required by the later
[sealed-generation backend correction](MOBILE_001_SEALED_GENERATION_VERIFICATION.md).

`/private/tmp/crm-ios-ui4.log`: the actual simulator SwiftUI test **passed** in
60.438 seconds. The reviewer read the test and completed log and viewed both
committed screenshots: the same note/create/completion operation IDs change from
three pending to zero pending. The test terminates/relaunches the app between
local submission and reconnect, then verifies synchronization and sign-out.
Earlier selector/hittability failures are retained in the platform verification
record; they are not presented as successful UI runs. This UI test does not
exercise the separately reviewed refreshed-conflict button or saved due-date
picker, so those specific UI findings retain source-level attribution.

`/private/tmp/crm-ios-release2.log`: **BUILD SUCCEEDED** for Release,
`-sdk iphoneos -destination generic/platform=iOS`, `CODE_SIGNING_ALLOWED=NO`.
This is an ARM64 device compilation, not signing, distribution or a device run.
The durable committed evidence copies and build commands are in the platform's
`ios/evidence/` and [verification record](MOBILE_001_IOS_VERIFICATION.md).

No additional concrete source defect was found in the assigned paths. The six
requested source corrections and their bounded available proof are verified.
Physical-device cellular/sleep/power-loss trials, release signing, app distribution
and customer readiness remain outside this synthetic verification.

## Repeated sync against the patched backend

The reviewer read the new `testRepeatedReadOnlyRefreshBeyondGenerationCapacity`
body and `/private/tmp/crm-ios-refresh5b.log`: **PASS**, 26.301 seconds. Five manual
refreshes each changed the persisted last-complete-sync label and returned the
complete-workspace success state. The same app showed 100 offline People and
at least 1,000 notes plus at least 1,000 tasks. It finished paused. The test performs
CRM reads/reconciliation only; it does not repeat the 100-operation upload proof.
The only additional application-source edit was to put operation UUIDs inside a
collapsed “Sync details” disclosure; this exact one-line change was reviewed.
The follow-up is committed as
`6e3f715a14938e17971cfb9da911761a36cb4334`; the reviewer confirmed the clean
platform worktree and read its retained `ios/evidence/repeated-refresh-tests.txt`.
The coordinator then integrated that follow-up in merge `152dcec` before removing
the worktree and preserving its build data at the location above.
The reviewer also viewed `five-refresh-coverage.png`, showing exactly 100 People,
1,299 notes and 1,004 tasks in the complete cache.

The API used the verified generation source from backend commit `7f41907`, with
artifact SHA-256
`f4b8bedbeaef723c47d56a3215e298737dd9ada303da2a98b3fdd071441d5fc7`:
[isolated runtime upgrade evidence](../design/qa/mobile-001-native-2026-09-12/api-lifecycle-upgrade.json).
An initial post-patch attempt encountered four completed synthetic generations
created before the sealed marker existed. Their owners confirmed those specific
runs were finished; the coordinator retired those four at
`2026-09-13T02:25:23Z`, preserving contexts, receipts and canonical rows:
[legacy fixture retirement evidence](../design/qa/mobile-001-native-2026-09-12/legacy-test-generation-retirement.json).
Product migration does not infer that old `complete=true` rows were sealed.
The recorded five-refresh pass occurred after this explicit fixture adjustment.
