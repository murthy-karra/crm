# Mobile 006 Android verification

Status: **IN PROGRESS** — local encrypted-store checks, the populated
schema-6 installed-store upgrade, and the serialized API3106 offline/restart
journey are complete. Native UI boundary walk-through remains.

## Environment

- Source: `9185cb3` base; Android lane worktree `codex/mobile-006-android`.
- Frozen mobile API source: `82d081e`; API origin for this verifier is
  `http://10.0.2.2:3106` through the `mobile006qaDebug` variant.
- Device: owned `CRM_Mobile006_QA`, API 37 ARM64, serial `emulator-5554`.
- SDK: `/Users/karrad/Library/Android/sdk`; JDK: Android Studio JBR 25.0.3.
- Isolated outputs: `/private/tmp/crm-mobile006-010f4-thyhauvv/android006`;
  Gradle cache: `/private/tmp/crm-mobile006-010f4-thyhauvv/gradle-home`.
- Database: Room schema 8, SQLCipher 4.19.0, WAL and `synchronous=FULL` asserted
  at open. The additive 6→7→8 migration preserves prior encrypted database,
  outbox and receipt rows. Mobile006 adds metadata draft/context/catalog tables
  and pins a manifest metadata revision separately from broad Person revision.

## Completed local evidence

| Check | Result | Evidence |
|---|---|---|
| `:assembleMobile006qaDebug :testMobile006qaDebugUnitTest` | PASS | prior isolated build, 23 s |
| `:assembleMobile006qaDebugAndroidTest` | PASS | `android006-assemble-test.log` |
| `:connectedMobile006qaDebugAndroidTest` filtered to `Mobile006StorageTest` | PASS, 4 tests | `android006-storage.log` |
| `:testMobile006qaDebugUnitTest :connectedMobile006qaDebugAndroidTest :lintMobile006qaDebug` | PASS, 2m02s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-final-local.log` |
| corrected metadata parser + unit tests + `Mobile006StorageTest` | PASS, 5 emulator tests, 1m23s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-storage-parser.log` |
| catalog-only revision requalification regression | PASS, 6 emulator tests, 1m06s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-r1-storage.log` |
| archived/deleted catalog, key-loss, access-expiry and late-identity metadata boundaries | PASS, 9 emulator tests, 1m24s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-storage-boundary-final.log` |
| replacement waits for a matching sealed catalog/metadata baseline, then CAS recovery | PASS, 9 emulator tests, 1m20s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-r2-catalog-replacement-retry.log` |
| historical populated schema-6 APK/test build | PASS, 41s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android005-upgradeproof-build.log` |
| current schema-8 APK/test build | PASS, 44s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-upgradeproof-build.log` |
| actual installed schema-6 seed | PASS, 5.681s | direct `adb shell am instrument` on `org.crm.field.mobile006upgradeproofqa.legacytest` |
| current APK installed over schema-6 store, inspector | PASS, 6.707s | direct `adb install -r` then `adb shell am instrument` on `org.crm.field.mobile006upgradeproofqa.test` |
| API3106 offline mixed metadata prepare | PASS, 1/1 | direct `adb shell am instrument`; `mobile006-stage.json` retained protected operation ID and SHA-256 envelope evidence |
| API3106 process relaunch + upload | PASS, 14.691s | direct second `adb shell am instrument`; verified same envelope SHA-256, `person_metadata` receipt and accepted/covered state |

`Mobile006StorageTest` covers mixed immutable tag/typed-value envelopes,
exact decimal/date string retention, explicit clear, choice option identity,
duplicate/invalid rejection before outbox creation, receipt shape, and a
catalog-conflict current-review/replacement CAS path. The fifth test verifies
the frozen `metadata-v1` generation descriptor and a metadata component that
contains tags/values rather than generic `items`.

The installed upgrade used a second owned verifier package to preserve the
earlier `mobile006qa` package/store after its first diagnostic attempt. The
historical Mobile005 schema-6 build wrote a populated protected store (Person,
generic draft and immutable queued envelope); the current Mobile006 APK was
then applied with `adb install -r`, with no uninstall, data clear or key
replacement. The inspector proved schema 8, unchanged key length, exact legacy
envelope and draft bytes, and an unqualified pre-Mobile006 Person.

The current metadata boundary test proves that archived fields and options cannot
be selected again, an archived value can still be explicitly cleared, and a
deleted tag cannot enter a new proposal. It proves exact keystore-wrapped key
loss leaves only ciphertext, an elapsed offline lease persists the locked marker
even when the rejected write's transaction rolls back, and a late bootstrap with
a different actor identity cannot bind to the protected account.

Catalog-conflict recovery also requires a causally later sealed catalog whose
revision matches the authorized current-metadata response and a Person component
qualified against that exact metadata revision. A conflict read alone preserves
the immutable proposal and cannot expose stale labels/options for replacement.

## Remaining required evidence

1. Serialized API3106 login, opted-in reconciliation, complete catalog/component
   traversal, seal, offline mixed edit, forced process relaunch and one upload
   are PASS. Lost-response exact replay remains to be observed separately.
2. Two-client metadata and catalog conflict walk-through, including unrelated
   profile/stage/note/task edit boundaries, archived clear and deleted target.
3. UI screenshots/inventory plus storage/key failure, access-expiry and late
   identity/account boundary checks.

No physical device, distribution, production API, customer data or retained demo
package was used.
