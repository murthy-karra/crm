# Mobile 006 Android verification

Status: **IN PROGRESS** — local encrypted-store checks, the populated
schema-6 installed-store upgrade, the serialized API3106 offline/restart
journey, and the retained catalog-conflict native replacement journey are
complete. The staged lost-response replay remains to be recorded in this
verifier.

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
| metadata catalog-reuse integrity, including recovery after incomplete cached rows | PASS, 10 direct emulator tests, 48.744s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-catalog-reuse-direct-pass.log` |
| historical populated schema-6 APK/test build | PASS, 41s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android005-upgradeproof-build.log` |
| current schema-8 APK/test build | PASS, 44s | `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-upgradeproof-build.log` |
| actual installed schema-6 seed | PASS, 5.681s | direct `adb shell am instrument` on `org.crm.field.mobile006upgradeproofqa.legacytest` |
| current APK installed over schema-6 store, inspector | PASS, 6.707s | direct `adb install -r` then `adb shell am instrument` on `org.crm.field.mobile006upgradeproofqa.test` |
| API3106 offline mixed metadata prepare | PASS, 1/1 | direct `adb shell am instrument`; `mobile006-stage.json` retained protected operation ID and SHA-256 envelope evidence |
| API3106 process relaunch + upload | PASS, 14.691s | direct second `adb shell am instrument`; verified same envelope SHA-256, `person_metadata` receipt and accepted/covered state |
| API3106 catalog-conflict native replacement | PASS, 1/1, 14.334s | direct `adb shell am instrument` continuation; original `389ca800-05f4-41be-aacf-1302e4ebe6cc` remained superseded, replacement `4317238d-aabf-4e1b-a89b-a35618364e8c` reached `accepted`; screenshots and device evidence in `/private/tmp/crm-mobile006-010f4-thyhauvv/android006-ui-final-artifacts/` |

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

The direct 10-test run used matched installed APKs (target
`7705977de274ca794b100bea63a6a62f2f3e6859d15e81d40bc30b93fab75268`, test
`94ae0ccde2a31f988991c383d1469648a75f543e01c112de78437122d70d64e5`) and
`adb install -r`; it did not clear the staged QA package or alter the separate
upgrade-proof package. Earlier UI attempts with a newly built test APK and a
stale target APK failed with `NoSuchMethodError`; those observations are a
harness mismatch, not evidence of catalog loss.

The scoped catalog-conflict diagnostic then proved the currently installed
catalog projection is intact: the exact metadata card existed with four DAO
metadata fields, four `FieldUi` fields and two metadata contexts while the
original operation remained `attention/catalog_revision_conflict`. Its complete
semantics dump places the metadata review action at y=3027–3133 while the
device viewport ends at y=2424. The initial Compose test therefore issued an
off-screen action and did not call the product callback. A scroll-aware action
subsequently opened the native `AlertDialog` and atomically superseded the
protected original into a replacement draft. The test then stopped in an
ambiguous multi-root diagnostic before editing or submitting. Evidence:
`/private/tmp/crm-mobile006-010f4-thyhauvv/android006-ui-diagnostic-artifacts/`,
`android006-ui-metadata-card-diagnostic.log`, and
`android006-ui-metadata-card-continuation.log`. No replacement request or
receipt was generated in those failed harness attempts.

The final continuation resumed that same staged state after the emulator reboot
through the ordinary same-account UI authorization screen; the screen reported
that one saved item remained protected, and the retained stage file remained
`catalog_conflict_ready_for_ui_review`. The proof used the existing
`saved-work-list` LazyColumn to compose the retained replacement card, asserted
that its tagged node exposed its own click action, opened the native editor,
replaced the text value, submitted it and observed `accepted`. The original
conflict stayed superseded; the final screenshot shows the accepted replacement
as “Synced · awaiting a current download.” The command was:

```sh
adb -s emulator-5554 shell am instrument -w \
  -e uiMobile006Conflict review \
  -e class org.crm.field.Mobile006UiProofTest#catalogConflictIsReviewedAndReplacedThroughNativeMetadataEditor \
  org.crm.field.mobile006qa.test/androidx.test.runner.AndroidJUnitRunner
```

It passed one test in 14.334 seconds using the frozen API source `82d081e` at
`http://10.0.2.2:3106`. Only the current test APK was installed with `adb install
-r`; the target APK stayed at SHA-256
`7705977de274ca794b100bea63a6a62f2f3e6859d15e81d40bc30b93fab75268`.

## Remaining required evidence

1. Record the staged lost-response replay in this verifier.

No physical device, distribution, production API, customer data or retained demo
package was used.
