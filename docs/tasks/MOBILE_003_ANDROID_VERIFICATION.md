# Mobile 003 Android verification

**Branch:** `codex/mobile-003-android`  
**QA identity:** `org.crm.field.mobile003qa`, vault namespace `field.mobile003qa`, API `3102`.

## Completed focused checks

On the Android 17/API 37 emulator (`emulator-5554`), the encrypted Room focused
suite passed six cases in 33.262 seconds after the final reconciliation-staging
cleanup. Its result is at
`/private/tmp/crm-mobile003-android-build/outputs/androidTest-results/connected/debug/flavors/mobile003qa/TEST-CRM_Field_API37_ARM64(AVD) - 17-_-mobile003qa.xml`.

```sh
JAVA_HOME='/Users/karrad/Applications/Android Studio.app/Contents/jbr/Contents/Home' \
ANDROID_HOME='/Users/karrad/Library/Android/sdk' \
./gradlew connectedMobile003qaDebugAndroidTest \
  -PCRM_MOBILE003_ANDROID_BUILD_DIR=/private/tmp/crm-mobile003-android-build \
  -Pandroid.testInstrumentationRunnerArguments.class=org.crm.field.Mobile003StorageTest
```

The test covers encrypted persistence and reopen, the additive 3→4 migration
with exact legacy operation-envelope and draft-payload retention, duplicate-save
CAS convergence, separate same-Person contacts, strict `contact_attempt` receipt
validation (`changed:true`, `committed_revision:null`), future-time repair that
does not modify the uncertain operation, DST overlap/gap resolution, and an
outbox write failure that leaves the draft without an operation. The contact
receipt test also proves a staged pre-receipt reconciliation is discarded, so a
fresh generation and Today seal are required even when Person revision is equal.

The final post-change compilation passed:

```sh
./gradlew compileMobile003qaDebugAndroidTestKotlin \
  -PCRM_MOBILE003_ANDROID_BUILD_DIR=/private/tmp/crm-mobile003-android-build
```

## Populated historical install upgrade

The same isolated `org.crm.field.mobile002qa` package and its real Android
Keystore identity were used for an in-place manual install. A temporary,
historical Mobile 002 build at base `90b6ae2` seeded schema 3 with a cached
Person, an accepted legacy note receipt, a queued legacy task operation, and a
retained note draft. It recorded:

```text
V3_SEEDED person=...0051
note_operation=98e7dcb5-68ce-49da-91eb-687f11dc1df1
task_operation=8a0c24c5-04cf-4524-ac6c-03b6593f6286
note_sha256=8377c4948e6e57227e081e22546d47d688161492636df152a1e45a3bcad87324
```

Installing the current Mobile 003 `mobile002qa` APK with `adb install -r -g`
opened schema 4 without a data rewrite. The read-only QA probe recorded the
same Person revision and both exact operation IDs; the note envelope digest
remained `8377c4948e6e57227e081e22546d47d688161492636df152a1e45a3bcad87324`,
the task remained queued, the old accepted receipt remained present, and the
draft body remained `legacy retained draft`. There were zero contact drafts.
The probe log is available from emulator logcat under `Mobile003Upgrade`.

## Live API 3102

`Mobile003LiveApiTest.offlineContactSurvivesRepositoryRelaunchThenReceivesFreshSeal`
passed against API 3102 using the production encrypted store, repository,
operation route, and reserved Android Person 051. The final emulator acceptance
used manually installed `mobile003qa` and instrumentation APKs because Gradle's
connected-test task uninstalls its package after a run. The test did the
following in two direct instrumentation stages:

1. Logged in and completed a normal reconciliation; sealed an old note receipt;
   then paused sync and saved a legacy task plus a Mobile 003 contact attempt.
   The protected store retained the note receipt and queued task/contact IDs and
   their original envelope SHA-256 values:

   ```text
   note=b725bda6-8b7f-403b-a733-f51f688bc74c b9547c3d3c1530aa7564dbf5dba85250c1d0043a9dc9ddfe76f212bbed915301
   task=95dc3842-59ee-4b18-8979-fdc0db4e5989 d07e7a28a3224dfa124fcceddb8b7a586a5924038f6527316ce2bd95184a6154
   contact=b6a9253f-6c34-4b2d-90e8-940cbb6997b9 55a586fb86974964a32e7321480a7a069605098076e7cb0a21d4ed58bf981d60
   ```

2. Ran `adb shell am force-stop org.crm.field.mobile003qa`, confirmed no target
   PID, then launched the second instrumentation stage against the same package,
   UID, Android Keystore key, and database. It reopened without a new login,
   replayed the queued task and contact, retained all original bytes, and finished
   `OK (1 test)` in 13.272 seconds. The contact receipt was strict
   (`resource_type=contact_attempt`, `committed_revision:null`, `changed:true`)
   and the resulting fresh Today seal had no `generation`, `manifest_cursor`, or
   `manifest_complete` staging metadata.

The direct commands were:

```sh
adb install -r -g /private/tmp/crm-mobile003-android-build/outputs/apk/mobile003qa/debug/CrmField-mobile003qa-debug.apk
adb install -r /private/tmp/crm-mobile003-android-build/outputs/apk/androidTest/mobile003qa/debug/CrmField-mobile003qa-debug-androidTest.apk
adb shell am instrument -w -r -e class org.crm.field.Mobile003LiveApiTest \
  -e runMobile003Live true -e mobile003Stage prepare \
  org.crm.field.mobile003qa.test/androidx.test.runner.AndroidJUnitRunner
adb shell am force-stop org.crm.field.mobile003qa
adb shell am instrument -w -r -e class org.crm.field.Mobile003LiveApiTest \
  -e runMobile003Live true -e mobile003Stage relaunch \
  org.crm.field.mobile003qa.test/androidx.test.runner.AndroidJUnitRunner
```

## Historical-store attempt

The preserved `mobile002qa` package is separate from the demo package. An
archived old fixture could not be copied into this emulator because its encrypted
key wrapper was bound to a different app UID (`AEADBadTagException`); it was not
used as upgrade evidence. The clean historical schema-3 seed prepared in
`/private/tmp` and installed in-place under the same package is the upgrade
evidence. Physical-device validation is deferred; the emulator acceptance did
perform an actual target-package process termination and relaunch.
