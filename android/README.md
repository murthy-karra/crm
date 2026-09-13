# CRM Field — Android Mobile 001

A native Kotlin/Compose development app for the approved offline field-work slice.
This is a synthetic local build, not a distributed application or a device-support
commitment. Release server identity, signing and distribution remain unconfigured.

## Build

Use an Android SDK containing platform 37 and Build Tools 37.0.0, and a compatible
JDK. The verified host uses Android Studio's JBR 25. Set `JAVA_HOME` and
`ANDROID_HOME` per command or configure Android Studio for this project; do not
change global Java settings. `local.properties` is ignored.

```sh
JAVA_HOME='/path/to/android-studio/jbr/Contents/Home' \
ANDROID_HOME='/path/to/Android/sdk' \
./gradlew --no-daemon assembleDebug testDebugUnitTest lintDebug
```

Pins: Gradle 9.5.0 (wrapper SHA-256 enforced), AGP 9.3.1, built-in Kotlin with
KGP/Compose compiler 2.4.20, KSP 2.3.12, Compose BOM 2026.09.00, Room 2.8.5,
AndroidX SQLite 2.7.1 and SQLCipher Android 4.19.0. AGP's built-in Kotlin means
`org.jetbrains.kotlin.android` must not be applied again. Java bytecode targets17;
minimum SDK28 is a development implementation choice, not an accepted release
support matrix.

The debug API origin is `http://10.0.2.2:3101`. Cleartext is allowed only for explicit
emulator/loopback names in the debug network configuration. Android17 requires
local-network permission for that host alias, so only the debug manifest declares
it. The native debug activity requests the platform permission; instrumented tests
grant it only to the synthetic package. Release requires HTTPS and has no such
permission or cleartext exception.

## Storage and recovery

Each server/actor/Organization has a separate random-key SQLCipher database under
`noBackupFilesDir`. AndroidKeyStore wraps its key with AES-GCM and an unlocked-device
requirement. Cloud backup and device transfer exclude every storage domain. No
plaintext database, WAL, payload, key or draft fallback exists. SQLCipher version,
WAL mode, synchronous FULL and memory temporary storage are checked at each open.
Schema version2 upgrades version1 with an additive migration; no destructive
migration method is configured.

Draft compare-and-swap writes and immutable outbox insertion/deletion are
transactions. The composer distinguishes uncommitted text from committed drafts,
prevents closing after failed autosave and offers an explicit discard of unsaved
edits. Submitted operation IDs and bytes remain unchanged across retry. Accepted
receipts remain as overlays until a complete Person revision covers them.

Pages and cursors are committed together. The active cache changes only after all
selected components and the generation seal pass validation. Missing selected
People leave the active cache; their original queued work and drafts remain
protected, with content-free unavailable entries until that Person is available
again. A removed cached record is never presented as current merely because local
work exists.

The access lease uses elapsed realtime (including sleep), boot count and a wall-clock
rollback check. Reboot, uncertain boundaries, seven-day expiry and current authority
denial hide cached content and require online sign-in. A preallocated, synced lock
marker is independent of the encrypted registry/database, so failure updating either
does not authorize a fresh process. Sign-out is local and works without a network.

Today and Person sections display complete cached content with evaluation time and
coverage. People name search covers the local selection; a known UUID can request
additional offline availability. There is no unbounded online name-search adapter.
Date and time entry uses Android's native pickers.

## Verification

```sh
./gradlew testDebugUnitTest connectedDebugAndroidTest
```

Ordinary instrumentation covers encrypted real files/WAL, wrong keys, crash-style
reopen,100 immutable actions, rejected writes, migration, receipt/seal order,
clock/context/actor boundaries, sign-out persistence failure, late login and native
task-conflict UI. Tests create synthetic stores only. Opt-in API/UI proof requires
the parent-owned synthetic service and is described in
`../docs/tasks/MOBILE_001_ANDROID_VERIFICATION.md`.

For process durability proof, install with `adb install -r` and invoke instrumentation
directly. Gradle's connected-test task removes its installed app at the end, so it
cannot establish cross-invocation app-data survival. Never use package-data clearing
or uninstall between a prepared queue and its relaunch/reboot checks.

Sensitive content is excluded from logs and production screenshots/recents use
FLAG_SECURE. The test APK alone clears that window flag while capturing labelled
synthetic UI evidence. It is not a production capture feature.
