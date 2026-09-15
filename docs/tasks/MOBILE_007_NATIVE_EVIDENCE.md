# Mobile 007 native evidence

Date: 2026-09-15

## Scope

This record covers the iOS and Android native implementation and its focused
storage/UI checks. It contains no customer data, tokens, or identifiers beyond
the fixture names needed to identify the runners.

## Passing checks

- Android Kotlin compilation and test APK compilation passed with the installed
  Android Studio JBR. Log: `/tmp/mobile007-round2-android-reason-fix-build.log`.
- Android `Mobile007StorageTest` passed 3/3 after the sealed `active_reasons`
  persistence fix. Log: `/tmp/mobile007-round2-android-reason-storage.log`.
- Android UI projection tests passed 2/2 on the isolated API37 emulator.
  Log: `/tmp/mobile007-round2-android-ui.log`.
- Android populated Mobile006 upgrade fixture passed on `CRM_Field_API37_ARM64`:
  a synthetic schema-6 store with 100 cached People, an unsent draft, an
  accepted/covered outbox receipt, and the original envelope was migrated
  in place to schema 9 with all protected rows and bytes preserved. Log:
  `/tmp/mobile007-round2-android-populated-upgrade-test3.log`.
- Android real API/UI proof passed on the preserved `emulator-5564` install
  using the admitted synthetic installation context. The test exercised the
  two same-name Discovery Twin rows, exact phone and email searches, Remote
  Prospect search → Save offline, clear/relaunch recovery, resume to 101
  cached People with the pinned reason, and an offline note draft that was
  still present after activity recreation. Build/install and direct runner
  logs: `/tmp/mobile007-round2-android-build-final-proof2.log`,
  `/tmp/mobile007-round2-android-discovery-note-ui4.log`, and
  `/tmp/mobile007-round2-android-discovery-note-logcat4.log` (non-PII stage
  markers for the exact searches, 101-record seal, and note persistence).
- Android cold process restart proof passed after `adb shell am force-stop
  org.crm.field` without clearing the app. A fresh instrumentation process
  restored the paused 101-Person cache and the existing encrypted offline note
  draft. Logs: `/tmp/mobile007-round2-android-cold-start-ui.log` and
  `/tmp/mobile007-round2-android-cold-start-logcat.log`.
- Android installed Mobile006 upgrade proof passed on isolated `emulator-5580`.
  The old binary was `/private/tmp/crm-mobile006-010f4-thyhauvv/android006/outputs/apk/mobile006qa/debug/CrmField-mobile006qa-debug.apk`
  (HEAD Mobile006, schema 8). It seeded a dedicated protected store containing
  100 People, a queued operation, an accepted receipt, and a draft; the current
  APK from `android/build/outputs/apk/mobile006qa/debug/` was installed with
  `adb install -r`, migrated it in place to schema 9, and preserved the
  database key, envelope, receipt, draft, and People. Seed/verify logs:
  `/tmp/mobile007-android-schema8-seed2.log`,
  `/tmp/mobile007-android-schema8-current-install.log`, and
  `/tmp/mobile007-android-schema8-verify.log`.
- iOS focused storage tests passed 35/35 in the earlier fixture run.
- iOS pending intent, cleared search, relaunch, and cancellation UI flow passed
  on the owned `CRM-Mobile007-Round2` simulator. Log:
  `/tmp/mobile007-round2-ios-fixture-ui5.log`.

## Bounded runtime findings

An earlier Android real API attempt stopped at bootstrap with HTTP 429
`mobile_capacity` after session login succeeded. That attempt used a discarded
installation identifier and was not treated as product behavior. The final
run reused the admitted synthetic installation context without changing the
server capacity or resetting the populated store.

The first installed-upgrade probe used a stale schema-6 fixture label against
schema-8-shaped data and correctly failed with duplicate
`metadataRevisionsQualified`; that malformed fixture was not reused. The final
probe seeds the exact schema-8 old binary without changing `user_version`, then
installs the current APK in place on an isolated emulator.

The iOS extended run was retried on the owned `CRM-Mobile007-Round2` simulator
using its existing protected store. The build and launcher succeeded, but the
run was stopped when the parent performance hold began while the baseline cache
assertion was still polling. It is not reported as a pass. Log:
`/tmp/mobile007-round2-ios-797d-extended-ui2.log`.

The later iOS rerun reached the same baseline assertion after the existing
store already contained 101 People. Read-only simulator inspection showed
`Remote Prospect` both cached and still requested; the request's Cancel control
was only exposed after filtering the virtualized People list. The test was
stopped before claiming a result, and the setup now filters to that fixture
Person before cancelling it through the normal UI. Log:
`/tmp/mobile007-round2-ios-797d-final-ui3.log`.

## Current status

Both platforms are complete. [Final iOS evidence](MOBILE_007_IOS_EVIDENCE.md)
records the later 36/36 storage pass, interrupted-stage recovery correction,
complete UI12 pass and actual installed schema 9→10 upgrade. The earlier iOS
entries above remain historical results.

The Android final API/UI journey, cold-process reopen and installed upgrade are complete. The recovery helper remains
test-only and opt-in; production startup has no registry fallback. No store
reset, server-context mutation, or capacity change was performed.
The machine-readable status is maintained at
`/private/tmp/crm-mobile007-runtime-20260915/native-progress.json`.
