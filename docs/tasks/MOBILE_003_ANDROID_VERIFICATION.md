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
passed against API 3102 in 22.646 seconds using the production encrypted store,
repository, operation route, and reserved Android Person 051. It queued the
historical-offset contact while sync was paused, sealed the immutable envelope,
locked/reopened the protected account store, then synced it to an accepted or
covered contact receipt. It asserts exact envelope preservation,
`resource_type=contact_attempt`, `committed_revision:null`, `changed:true`, and
a fresh sealed Today record. The connected result is at
`/private/tmp/crm-mobile003-android-build/outputs/androidTest-results/connected/debug/flavors/mobile003qa/TEST-CRM_Field_API37_ARM64(AVD) - 17-_-mobile003qa.xml`.

## Historical-store attempt

The preserved `mobile002qa` package is separate from the demo package. An
archived old fixture could not be copied into this emulator because its encrypted
key wrapper was bound to a different app UID (`AEADBadTagException`); it was not
used as upgrade evidence. A clean historical schema-3 seed harness was then
prepared in `/private/tmp` for an in-place same-package 3→4 installation. That
manual emulator run remained unsuitable as evidence, because archive encryption
is deliberately UID-bound. The clean same-package seed above is the upgrade
evidence. Physical-device process termination is deferred; emulator validation
used the repository's protected lock/reopen boundary.
