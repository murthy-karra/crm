# Mobile 004 Android verification

**Branch:** `codex/mobile-004-android`  
**QA identity:** `org.crm.field.mobile004qa`, vault `field.mobile004qa`, API `3102`.

## Completed checks

The Android 17/API 37 emulator (`emulator-5554`) passed all five focused encrypted-storage
checks. The XML result is
`/private/tmp/crm-mobile004-010e4/android/build/outputs/androidTest-results/connected/debug/flavors/mobile004qa/TEST-CRM_Field_API37_ARM64(AVD) - 17-_-mobile004qa.xml`.

```sh
ANDROID_HOME="$HOME/Library/Android/sdk" ANDROID_SERIAL=emulator-5554 \
JAVA_HOME="$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
./gradlew connectedMobile004qaDebugAndroidTest --no-daemon \
  -PCRM_MOBILE004_ANDROID_BUILD_DIR=/private/tmp/crm-mobile004-010e4/android/build \
  -Pandroid.testInstrumentationRunnerArguments.class=org.crm.field.Mobile004StorageTest
```

It verifies typed stage-draft CAS, the atomic draft/outbox/context transition,
one unresolved submitted stage change with a separately retained follow-up draft,
strict `person_stage` receipt parsing, A→B→A conflict comparison and explicit
new-operation reproposal, and a populated v4→v5 SQLCipher database migration.
The upgrade test preserves an existing legacy envelope byte-for-byte and does
not replace its key, receipt, or draft.

The final source compilation also passed with `--rerun-tasks`; its log is
`/private/tmp/crm-mobile004-010e4/android/compile-mobile004.log`.

## Real API status

API 3102 was available and the fixture confirmed Person `13e8d47a-5648-4d88-9358-fe5ed400078d`
is admitted for the Android actor. The first real prepare attempt used Gradle's
connected-test runner and failed before an offline operation was created because
the test observed selection immediately after asynchronous UI refresh:
`Android fixture Person must be fully downloaded`. The test now fences that
selection with an explicit wait.

The corrected real-API offline prepare stage then passed (`1` test, `0`
failures, 18.452 seconds; timestamp `2026-09-13T16:17:40`). It logged in to
the production repository/API route, completed opt-in stage catalog and Person
download, paused sync, committed a typed stage proposal and immutable operation
to the encrypted store, and recorded its SHA-256 for a later same-store replay.

Gradle connected tests uninstall the target package, so they cannot establish an
installed-store force-stop proof. Current APKs were built and manually installed
under the isolated Mobile004 identity for a direct `adb shell am instrument`
prepare/force-stop/relaunch run. The prepare stage created
`mobile004-force-stop-stage.json` inside the target package; after
`adb shell am force-stop org.crm.field.mobile004qa`, the relaunch stage reopened
the same database/Keystore identity without login, preserved the operation's
SHA-256, uploaded it, and validated a `person_stage` receipt with a nonzero
committed revision. It passed `OK (1 test)` in 21.359 seconds.

The coordinator's direct Android replay/conflict/reproposal run also passed on the
separate `mobile004qa` identity: `OK (1 test)` in 71.474 seconds. Its source and
evidence remain with the integration lane at
`/private/tmp/crm-mobile004-010e4/integration/android-live-build2.log` and
`/private/tmp/crm-mobile004-010e4/integration/android-live-replay.log`.

## Historical installed-store upgrade

The historical Android source supplied at `c581e1e` was built as its preserved
`mobile003qa` flavor, whose package and vault are deliberately the isolated
`org.crm.field.mobile004upgradeqa` / `field.mobile004upgradeqa` identity. The
current source was built as `mobile004upgradeqa` under the same identity. The
demo, Mobile002 QA, Mobile003 QA, and `mobile004qa` live-API stores were not
uninstalled or cleared.

```sh
# Historical source: /private/tmp/crm-mobile004-010e4/mobile003-native-source/android
ANDROID_HOME="$HOME/Library/Android/sdk" \
JAVA_HOME="$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
./gradlew assembleMobile003qaDebug --no-daemon \
  -PCRM_MOBILE003_ANDROID_BUILD_DIR=/private/tmp/crm-mobile004-010e4/android/historical-mobile003-build

# Current Android worktree
ANDROID_HOME="$HOME/Library/Android/sdk" \
JAVA_HOME="$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
./gradlew assembleMobile004upgradeqaDebug --no-daemon \
  -PCRM_MOBILE004_ANDROID_BUILD_DIR=/private/tmp/crm-mobile004-010e4/android/current-mobile004-build

adb -s emulator-5554 shell pm clear org.crm.field.mobile004upgradeqa
adb -s emulator-5554 install -r CrmField-mobile003qa-debug.apk
adb -s emulator-5554 shell am start -n \
  org.crm.field.mobile004upgradeqa/org.crm.field.LegacySeedActivity
# Wait for V4_SEED before replacing the APK.
adb -s emulator-5554 install -r CrmField-mobile004upgradeqa-debug.apk
adb -s emulator-5554 shell am start -n \
  org.crm.field.mobile004upgradeqa/org.crm.field.Mobile004UpgradeProbeActivity
```

The first attempt intentionally remains recorded as a failure in
`/private/tmp/crm-mobile004-010e4/android/mobile004-upgrade-final.log`: the
current APK was installed before the old seed worker completed, and the probe
reported `V5_PROBE_FAILED NoSuchElementException`. It wrote no stage mutation
and did not touch any other package. The retry waited for the historical
`V4_SEED` line before `install -r` and passed. The key fingerprint and queued
operation ID/bytes were identical across the boundary:

```text
V4_SEED key=29a7c97e4cf165d262da07bb72d71aa8ff5124072c60599054a67bd09486d57c
        op=d9cbe5e5-a045-4a15-9495-7813018e940c
        sha=0e04e121dfae6d19de3eee95a663f3f85810bff977f408b217fad6b62fb9e5be
V5_PROBE key=29a7c97e4cf165d262da07bb72d71aa8ff5124072c60599054a67bd09486d57c
         queued_id=d9cbe5e5-a045-4a15-9495-7813018e940c
         queued_sha=0e04e121dfae6d19de3eee95a663f3f85810bff977f408b217fad6b62fb9e5be
         receipt_id=6f1cec5c-72c6-46c2-8c17-ce70abfc8426
         receipt_sha=bd996f06beb272fd61735038c993623234f559a76ab9d1de169e06230c312dfe
         draft_id=legacy-draft
         draft_sha=e8034a1057f9cdec09f419e9d19885a0b32a34ff42dafafd0251e404c6eeecc4
         cache_id=10000000-0000-4000-8000-000000000051
         cache_sha=796a86e19903e30d1d5fe7aa814af7e1e95bcc8c3ed34d36f15b9f51870e1852
         stage_qualified=false
```

`stage_qualified=false` is the required v4 cache fence. The probe also asserts
the expected accepted receipt is present and no v5 stage draft was invented.
The build and retained log paths are
`/private/tmp/crm-mobile004-010e4/android/historical-mobile003-build.log`,
`/private/tmp/crm-mobile004-010e4/android/current-mobile004-build.log`, and
`/private/tmp/crm-mobile004-010e4/android/mobile004-upgrade-final.log`.

## Review-round fixes

The catalog parser now accepts every signed `SMALLINT` position returned by the
server; it retains the existing 100-row and 512-KiB transport bounds and adds no
client-side stage quota. A submitted stage change now leaves the stage action
available to create a separate editable follow-up draft. That draft shows its
waiting state and does not create an outbox row or submit until the preceding
proposal is covered or explicitly reviewed. It retains its original stage
baseline; submitting later does not silently rebase it.

The direct tests used the separate upgrade test package and did not uninstall
the seeded target package:

```sh
./gradlew assembleMobile004upgradeqaDebugAndroidTest --no-daemon \
  -PCRM_MOBILE004_ANDROID_BUILD_DIR=/private/tmp/crm-mobile004-010e4/android/current-mobile004-build
adb -s emulator-5554 install -r CrmField-mobile004upgradeqa-debug-androidTest.apk
adb -s emulator-5554 shell am instrument -w -r -e class \
  org.crm.field.Mobile004StorageTest#stageCatalogAcceptsTheFullServerSmallintPositionRange \
  org.crm.field.mobile004upgradeqa.test/androidx.test.runner.AndroidJUnitRunner
adb -s emulator-5554 shell am instrument -w -r -e class \
  org.crm.field.Mobile004StorageTest#unresolvedStageOperationKeepsMutableFollowupDraftWaitingWithItsOriginalBaseline \
  org.crm.field.mobile004upgradeqa.test/androidx.test.runner.AndroidJUnitRunner
adb -s emulator-5554 shell am instrument -w -r -e class \
  org.crm.field.Mobile004StageFollowupUiTest \
  org.crm.field.mobile004upgradeqa.test/androidx.test.runner.AndroidJUnitRunner
```

All three passed: catalog position `OK (1 test)` in 4.135 seconds, preserved
follow-up baseline/outbox `OK (1 test)` in 8.58 seconds, and the person-screen
follow-up affordance `OK (1 test)` in 16.041 seconds. Logs are retained at
`/private/tmp/crm-mobile004-010e4/android/mobile004-negative-catalog.log`,
`/private/tmp/crm-mobile004-010e4/android/mobile004-followup-storage.log`, and
`/private/tmp/crm-mobile004-010e4/android/mobile004-followup-ui.log`.
