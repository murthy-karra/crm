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

Lost-response replay, second-actor conflict, and the historical Mobile003
source-install→Mobile004 same-store upgrade remain pending; no failed attempt
modified the demo, Mobile002 QA, or Mobile003 QA package/store.
