# Mobile 003 Android verification

**Branch:** `codex/mobile-003-android`  
**QA identity:** `org.crm.field.mobile003qa`, vault namespace `field.mobile003qa`, API `3102`.

## Completed focused checks

On the Android 17/API 37 emulator (`emulator-5554`), the encrypted Room focused
suite passed six cases in 48.309 seconds. Its result is at
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

## Historical-store attempt

The preserved `mobile002qa` package is separate from the demo package. An
archived old fixture could not be copied into this emulator because its encrypted
key wrapper was bound to a different app UID (`AEADBadTagException`); it was not
used as upgrade evidence. A clean historical schema-3 seed harness was then
prepared in `/private/tmp` for an in-place same-package 3→4 installation. That
manual emulator run remains to be completed alongside the live API 3102
force-stop/relaunch and receipt checks; no claim of those checks is made here.
