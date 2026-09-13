# Mobile 001 Android verification — 2026-09-12

Android implementation is confined to `android/` and this verification note, on
`codex/mobile-001-android` from reviewed foundation base `8b48008`. The frozen
Mobile001 HTTP DTOs were consumed without changes. Native iOS and Android use the
same Rust command backend. No production deployment, signing or distribution was
performed by this lane.

## Environment and reproducibility

Actual native build: Kotlin/Jetpack Compose, Room2.8.5 with SQLCipher Android4.19.0,
API37 ARM64 emulator `CRM_Field_API37_ARM64`. AGP9.3.1 and Gradle9.5.0 ran on the
existing Android Studio JBR25, producing Java17 bytecode. Compile/target SDK37;
minSdk28 is a synthetic development floor, not an approved release support matrix.
The SDK and JBR were selected per command; no global Java/SDK settings changed.

The Gradle wrapper checks its distribution SHA-256. Dependency versions and
artifact SHA-256 values are committed in `android/gradle.lockfile` and
`android/gradle/verification-metadata.xml`. SQLCipher's actual AAR SHA-256 is
`3f2aeebb584157baf145805dd8e2118a093632fbfa3dcb72a39e43a2c56b41fe`.
Its bundled ARM64 library has16384-byte load alignment. The app verifies
`cipher_version=4.19.0`, encrypted WAL, FULL synchronous mode and memory temporary
storage whenever a database opens; no plaintext fallback exists.

Official references used for the toolchain and compatibility fixes:
[AGP9.3](https://developer.android.com/build/releases/agp-9-3-0-release-notes),
[AndroidX Test release notes](https://developer.android.com/jetpack/androidx/releases/test),
[Android17 local-network permission](https://developer.android.com/privacy-and-security/local-network-permission),
[SQLCipher Android](https://www.zetetic.net/sqlcipher/sqlcipher-for-android-community/).
The emulator host alias needs Android17 local-network permission; it exists only
in the debug manifest. Espresso3.7.0 is explicitly pinned to avoid an older
transitive test driver's removed InputManager reflection. UIAutomator2.4.0 is used
for native date/time-picker actions.

## Checks actually executed

The following build command passed with dependency verification enabled. The
verified debug APK SHA-256 is
`274f49b0f05a335973f82670541f8d46816a4566619f45c21bcb80cd984f41bf`.

```sh
JAVA_HOME='/Users/karrad/Applications/Android Studio.app/Contents/jbr/Contents/Home' \
ANDROID_HOME='/Users/karrad/Library/Android/sdk' \
./android/gradlew -p android --no-daemon --console=plain \
  assembleDebug assembleDebugAndroidTest testDebugUnitTest lintDebug
```

Kotlin sources were formatted with ktfmt0.63, Kotlin style; formatter jar SHA-256
`a015521ddb1c7a80c41edb56b91b4a231439592ffd2e85ac866ff8134c37c112`.
The final formatting check passed. Git whitespace validation passed with CRLF
recognized for the generated Windows wrapper. Lint has no errors. Its remaining advisories concern deliberately pinned versions
and the conservative16MiB free-space check; that check does not preallocate space
by reclaiming other applications' caches.

`LeaseTest` (2 JVM tests) passed: exact seven-day expiry, reboot, monotonic rollback,
wall-clock rollback and large revision ordering. `StorageTest` (8 actual encrypted
file tests) passed: database/WAL contents and wrong-key failure,100 immutable queued
actions and process-style reopen, rejected outbox write retaining its draft,
nondestructive1→2 schema upgrade, pending work retained across lease/context denial,
atomic staging/promotion failure, accepted overlay retained across an older seal,
stale draft compare-and-swap, selection removal retaining protected work, and
KeyStore-wrapped actor/Organization/origin boundaries.

The bounded independent review's six findings were corrected: dirty composer
protection and revision checks; independent durable sign-out marker; current
authority403 locking; explicit known-Person pins in place of unbounded online
search; active-cache removal with protected unavailable work; and indexed
manifest lookup for each downloaded page. The native task-conflict UI test passed:
the old revision cannot be submitted again, while a refreshed revision enables an
explicit completion. The final focused instrumented run passed all 7 tests in 10.147s: six repository
boundary tests plus the partial-page/promotion durability test. It covered failed
lock persistence followed by fresh repository construction, late login after
sign-out, authority denial, reclaimed generation404, Retry-After and failed
retry-checkpoint persistence.

## Actual API and native UI proof

The app ran against the parent-owned synthetic Axum service at host3101 via
emulator `10.0.2.2:3101`. Android wrote UUID-labelled synthetic data only on Person020;
iOS used Person080. Both native runtimes exercised the same actor concurrently.
No customer data, live FUB credentials or shared development service were used.

The100-note test passed in14.165s:100 selected People,1,199 downloaded notes and
1,004 downloaded tasks;100 committed local note operations;101 actual HTTP POSTs
because the first response was deliberately discarded **after server acceptance**;
100 unique operation IDs, identical replay bytes and exactly100 resulting notes.
Counts are preserved in [the API evidence](../../android/qa/live-notes-evidence.json).
This is an emulator/synthetic-fixture measurement, not a production capacity claim.

That proof exposed a backend lifecycle defect: completed generations still
consumed per-context/actor capacity until expiry. The parent-owned fix added a
sealed-generation marker and bounded reclamation. Legacy prepatch test generations
were explicitly retired by the parent; contexts, receipts and canonical records
were preserved. The DTOs did not change. The patched service binary SHA-256 was
`f4b8bedbeaef723c47d56a3215e298737dd9ada303da2a98b3fdd071441d5fc7`.
Android treats reconciliation404 as reclaimed/expired staging; operation404 retains
its separate needs-attention behavior and original operation ID.

After the patched service was ready, the separate API finish stage passed
in7.474s: create-task dependency followed by completion, two task synchronization
cycles and three additional fresh downloads, then second-account isolation and
reopening the first account's protected draft. The100 notes were **not repeated**.
See [finish evidence](../../android/qa/live-api-evidence.json).

The actual Compose walkthrough passed: online sign-in, cached Today/People/Person,
local note and task creation, completion of the still-local task, and SQLite-trigger
failure during autosave. Close and back dismissal remained blocked until explicit
discard of unsaved edits. It left three immutable queued actions with sync paused.
The UI preparation took162.398s, largely waiting through the legacy capacity window.

Wi-Fi and mobile data were then disabled. The actual package was force-stopped and
relaunched without uninstall or data clearing; the native test passed in7.219s with
100 People and all three pending actions still available. Observed process IDs and
radio state are in [process evidence](../../android/qa/process-relaunch.json).

The emulator was actually rebooted: boot count2→3. The app showed a protected queue
and no cached content before online sign-in. Reauthorization reopened the same
three actions; Resume synchronized them successfully. This native test passed
in14.141s; [boot evidence](../../android/qa/reboot.json) records the boundary. Emulator
Wi-Fi and mobile data were restored. DatePickerDialog and TimePickerDialog entry,
committed due-date storage, closing and reopening the same draft also passed
in15.906s. The final recapture run passed in 14.793s after its wait was corrected
to observe completed autosave when reopening an already dated draft. No protocol
timestamp was typed into the product UI.

Screenshots are native renders under [android/qa](../../android/qa/): Today, Person,
failed autosave, pending queue, relaunched queue, reboot lock, synchronized queue
and reopened due-date draft. Production keeps FLAG_SECURE; the test APK alone
clears it while capturing labelled synthetic proof. The date-dialog capture had
to clear the parent flag before dialog creation because existing dialogs retain
the inherited secure flag. No production screenshot exception was introduced.

## Reproduce staged proof without losing app data

Use the configured SDK's `adb` and the generated debug/test APKs. `adb install -r -g`
preserves the installed data and grants only declared debug permissions. Gradle's
connected-device runner removes its target app at the end, so it was used for
isolated file tests, **not** as evidence of cross-invocation queue survival.

```sh
adb -s emulator-5554 install -r -g android/build/outputs/apk/debug/CrmField-debug.apk
adb -s emulator-5554 install -r -g android/build/outputs/apk/androidTest/debug/CrmField-debug-androidTest.apk
adb -s emulator-5554 shell am instrument -w -r \
  -e class org.crm.field.LiveApiTest -e runLive true -e apiStage notes \
  org.crm.field.test/androidx.test.runner.AndroidJUnitRunner
```

The API's `apiStage=finish` reuses the same installed `live-api-proof` namespace and
skips the100-note stage. UI methods in `NativeUiProofTest` are selected individually
with `Class#method` and `uiStage=prepare`, `relaunch`, `reboot`, or `date`. Preparation
expects a clean queue for this synthetic UI scenario. Preserve its data between
stages; do not uninstall or clear it. Current synthetic drafts remain encrypted on
the emulator for review. Ordinary tests do not opt into API mutations.

## Limits and initial failures

No physical Android device, real cellular connection, OEM background scheduling,
minimum-supported-version matrix, hardware-backed-key attestation, device transfer
or cloud restore was exercised. Backup/transfer exclusions and device-bound keys
were configured and inspected, not advertised as a tested recovery service.
SQL-triggered failures exercise real SQLite rollback paths; they do not simulate
all physical flash failures or sudden power loss. Schema1 is a synthetic prior
shape for nondestructive upgrade proof, not a previously distributed version.

Initial harness failures were corrected before pass claims: a PRAGMA needed a
query cursor rather than execSQL; expression-bodied tests needed Unit return;
an old Espresso transitive dependency used removed reflection; Android17's debug
local-network permission was missing. The first native UI wait was stopped while
the backend generation quota was exhausted, preserving app data. Final proof used
the corrected toolchain and parent-patched backend. Broader server authorization,
concurrent receipt, migration-review, retention and Web/Operator regression claims
belong to the separately verified shared foundation; they are not inferred from
these Android screenshots.
