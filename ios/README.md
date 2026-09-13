# Field CRM — iOS Mobile 001

Native SwiftUI synthetic field workflow using the frozen
[`mobile-v1` contract](../docs/tasks/MOBILE_001_CONTRACT.md). The same Rust
commands accept Web, Operator and native writes. This app is an authorized
synthetic development build; distribution and customer OS/support choices remain
separate release decisions.

## Build and run

Use Xcode 26.6 with the installed iOS 26.5 iPhone 17 simulator. The temporary iOS
17 deployment floor is a technical build setting, not a customer support promise.
Do not change global `xcode-select`. From the repository root:

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
xcodebuild -project ios/FieldCRM.xcodeproj -scheme FieldCRM \
  -configuration Debug -destination 'id=F5BF9C74-37AC-4546-A576-F30587CB4F4C' \
  -derivedDataPath ios/.build build CODE_SIGN_IDENTITY=-
xcrun simctl install F5BF9C74-37AC-4546-A576-F30587CB4F4C ios/.build/Build/Products/Debug-iphonesimulator/FieldCRM.app
xcrun simctl launch F5BF9C74-37AC-4546-A576-F30587CB4F4C dev.crm.FieldCRM --synthetic-keychain
```

Simulator builds use local ad-hoc signing; no signing team, certificate, profile,
license change or distribution is required. Do **not** use
`CODE_SIGNING_ALLOWED=NO` for runtime tests: an unsigned simulator app cannot
reliably use Keychain.

The root coordinator owns the synthetic API at `http://127.0.0.1:3101`. Its
retained fixture is seeded with the backend `mobile_fixture` example documented
in [`MOBILE_001_BACKEND_VERIFICATION`](../docs/tasks/MOBILE_001_BACKEND_VERIFICATION.md).
The synthetic development login is `agent@mobile.test` /
`Mobile-demo-only-123!`; the second same-Organization actor is
`second@mobile.test` with the same explicitly synthetic password. No password is
stored by the app. Do not reset shared services or use real customer data.

The normal key policy is `WhenPasscodeSetThisDeviceOnly`, non-synchronizable
Keychain items. Simulator cannot prove physical passcode enforcement: the normal
mode fails closed there. Only Debug + Simulator + explicit
`--synthetic-keychain` enables the visible synthetic banner and separate
`WhenUnlockedThisDeviceOnly` key namespace. This exception is absent on devices
and in Release. A Release app also requires an approved HTTPS environment before
sign-in; it is not configured for independent distribution.

## Storage and recovery

SQLCipher Community 4.19.0 is pinned to official SPM commit
`39f212458aeb88e33bdac2200a793a3f0d55d32b`. The published XCFramework checksum is
`39f02d2f04f0de2ba1facf215550bfc6e6e2c9971d5d8ebb0cdd604874781bd7`.
`Package.resolved` is checked in, and the importer sets `SQLITE_HAS_CODEC=1`.
The upstream [license](SQLCipher-LICENSE.md) is retained. `generate_project.py`
regenerates the checked-in small Xcode project using Python's standard library;
it is only needed after adding/removing source files.

Each actor/Organization has a separate encrypted file and device-bound key.
WAL, FULL synchronization, memory-only temporary storage, complete file
protection and backup exclusion apply. Schema versions 1→2→3 migrate
transactionally. Missing/wrong keys and unknown future schemas never recreate a
database. Real SQLite-full failures retain earlier drafts/outbox transactions.

Draft autosaves report committed revision numbers. Submission consumes that
revision and writes an immutable operation envelope in one transaction. Local
task completion references the create operation ID. Accepted receipts persist,
and their visible overlay stays until a complete active Person bundle covers its
causal revision. Staging pages and opaque cursor checkpoints survive process
restart; only a fully downloaded, sealed selection promotes. A retired generation
404 discards staging and refreshes; an operation 404 retains its original ID and
needs-attention input. Cache-only cleanup runs in bounded batches with yields and
drains before admitting another generation.

The seven-day lease uses the server's authorization limit plus
`mach_continuous_time` (including sleep) and a persisted boot-session UUID.
Reboot, expired monotonic duration or detected wall-clock rollback locks access;
ordinary process restart during the same boot preserves it. Sign-out/revocation
writes a durable independent lock marker before replacing credentials. If that
replacement fails, an older credential cannot reopen the store. Pending work
and keys remain protected; reopening requires the same authorized identity.

Foreground sync resumes on opening/connectivity and at a bounded minute timer;
manual Sync is available. The offline toggle persists across app termination.
One upload and one sequential component download run at a time. Retry-After30
is preserved for capacity responses; transient upload backoff retains exact
bytes. Three conflicting generations stop automatic reconciliation until a
manual retry. Unsupported protocol persists an update-required stop independent
of the user's offline toggle. Background execution is not promised.

## Verification

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
xcodebuild -project ios/FieldCRM.xcodeproj -scheme FieldCRM -configuration Debug \
  -destination 'id=F5BF9C74-37AC-4546-A576-F30587CB4F4C' -derivedDataPath ios/.build \
  test -only-testing:FieldCRMTests/StorageTests -only-testing:FieldCRMTests/ModelTests CODE_SIGN_IDENTITY=-
```

`LiveAPITests` is a separate **opt-in** real API run: replace the two selectors
with `-only-testing:FieldCRMTests/LiveAPITests`. It commits 98 uniquely labelled
notes plus one task and its completion on synthetic Person080. Coordinate the
shared fixture first; do not repeatedly run it during another lane's generation
proof. It drops a real successful HTTP response, retries the identical durable
envelope, and checks the server's replay receipt.

For the actual app flow use `-only-testing:FieldCRMUITests`. It signs in, checks
downloaded coverage, pauses sync, composes a note/task/completion, terminates and
relaunches the app with pending actions, reconnects, and checks receipts and
local sign-out. XCTest attachments capture the native screens. See the precise
[verification record](../docs/tasks/MOBILE_001_IOS_VERIFICATION.md) for observed
results and limitations; physical-device reboot/passcode and real cellular
checks are pending unless explicitly recorded there.
