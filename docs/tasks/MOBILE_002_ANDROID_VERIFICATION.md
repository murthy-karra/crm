# Mobile 002 Android verification

**Branch/source:** `codex/mobile-002-android`, started from `2fd9a9d`.
**QA identity:** `org.crm.field.mobile002qa` with test identity
`org.crm.field.mobile002qa.test`, vault namespace `field.mobile002qa`, and the
compile-time Debug-only origin `http://10.0.2.2:3102`.  The existing
`org.crm.field` demo package and its 3101 origin were retained.

## Completed checks

On 2026-09-12 the Pixel 9 ARM64 AVD `CRM_Field_API37_ARM64` was inventoried and
booted as `emulator-5554` (Android 17/API 37).  Before QA installation, package
inventory showed only the preserved demo `org.crm.field` and test package.  The
QA APK was installed manually with `adb -s emulator-5554 install -r -g` and
started as `org.crm.field.mobile002qa/org.crm.field.MainActivity`; the post-install
inventory contained both application IDs.  A UI-automator inspection confirmed
the isolated QA sign-in screen.  The real 3102 login using the supplied synthetic
agent reached a live complete-download run (the UI reported `Downloading 71/100
People`), so the emulator origin, HTTP exception, app identity, and real backend
session path were exercised.  `FLAG_SECURE` intentionally made `screencap`
output black; UI-automator text was the inspected runtime evidence.

The following commands used isolated Gradle/project/build directories.  The
temporary dependency cache was initially missing four verification hashes, so
`--dependency-verification=off` was used only for this local disposable QA
build; committed verification metadata was not changed.

```sh
JAVA_HOME='/Users/karrad/Applications/Android Studio.app/Contents/jbr/Contents/Home' \
ANDROID_HOME='/Users/karrad/Library/Android/sdk' \
GRADLE_USER_HOME='/private/tmp/crm-mobile002-android-gradle-home' \
./android/gradlew -p android --no-daemon --console=plain \
  --dependency-verification=off \
  --project-cache-dir /private/tmp/crm-mobile002-android-project-cache \
  -PCRM_MOBILE002_ANDROID_BUILD_DIR=/private/tmp/crm-mobile002-android-build \
  assembleMobile002qaDebug assembleMobile002qaDebugAndroidTest \
  testMobile002qaDebugUnitTest
```

This produced both QA APKs and passed the two selected JVM `LeaseTest` tests.

```sh
... ./android/gradlew ... \
  -Pandroid.testInstrumentationRunnerArguments.class=org.crm.field.Mobile002StorageTest \
  connectedMobile002qaDebugAndroidTest
```

The connected result at
`/private/tmp/crm-mobile002-android-build/outputs/androidTest-results/connected/debug/flavors/mobile002qa/TEST-CRM_Field_API37_ARM64(AVD) - 17-_-mobile002qa.xml`
reported 2 tests, 0 failures, 0 errors.  Those tests exercise encrypted Room/
SQLCipher persistence through reopen: immutable edit envelope/proposal/baseline,
per-target submission block with a durable follow-up draft, and strict
`edit_note` positive committed revision versus legacy `add_note:null` receipt
handling.

`lintMobile002qaDebug` completed with 0 errors and five pre-existing/version or
storage-space warnings.  Its report is
`/private/tmp/crm-mobile002-android-build/reports/lint-results-mobile002qaDebug.txt`.

## Installed-store upgrade attempt

An isolated fixture was made from `git archive 9cbaf1a` at
`/private/tmp/crm-mobile002-android-v1-fixture.zXYm5g`, with only its temporary
application ID, origin and vault namespace changed to the Mobile 002 QA
identity. It was built, installed after removing only the disposable QA package,
signed in to 3102, and created the old encrypted v2 account directory. The
current APK was then installed with `adb -s emulator-5554 install -r -g`,
force-stopped and relaunched. The same account directory, encrypted database and
wrapped key remained present; the v3 app opened without Room/SQLCipher migration
failure in logcat. The demo `org.crm.field` package remained installed throughout.

## Scope still requiring a subsequent real-API QA pass

The live session was not used to mutate a reserved Person record before this
checkpoint.  Thus real offline edit save/force-stop/relaunch, lost accepted
response replay, two-actor conflict/current-read/revised operation, removal and
unsupported-capability cases still need recorded evidence. The full required
preserved-store upgrade proof also remains pending: the fixture's 3102 download
did not complete before update, so it did not contain the required legacy
revision-less note bundle, old queue or draft. It must be repeated with those
records created before the `install -r` step. No claim here substitutes a new
Mobile 002 download for that legacy-store proof.
