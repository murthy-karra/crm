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

The following first-run commands used isolated Gradle/project/build directories.
The disposable cache initially exposed missing entries for the Maven Central
artifacts `guava-parent-33.4.0-jre.pom`, `junit-bom-5.10.2.module`,
`junit-bom-5.11.0-M2.module`, and `kotlinx-coroutines-bom-1.8.0.pom`.
`--dependency-verification=off` was used only to bootstrap that disposable
cache; it was not used for final verification. The SHA-256 values read from
those downloaded artifacts are now committed in
`android/gradle/verification-metadata.xml`.

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

After recording the four hashes, the following final build completed without a
dependency-verification bypass:

```sh
JAVA_HOME='/Users/karrad/Applications/Android Studio.app/Contents/jbr/Contents/Home' \
ANDROID_HOME='/Users/karrad/Library/Android/sdk' \
GRADLE_USER_HOME='/private/tmp/crm-mobile002-android-gradle-home' \
./android/gradlew -p android --no-daemon --console=plain \
  --project-cache-dir /private/tmp/crm-mobile002-android-project-cache \
  -PCRM_MOBILE002_ANDROID_BUILD_DIR=/private/tmp/crm-mobile002-android-build \
  assembleMobile002qaDebug
```

## Installed-store upgrade attempt

An isolated fixture was made from `git archive 9cbaf1a` at
`/private/tmp/crm-mobile002-android-v1-fixture.zXYm5g`, with only its temporary
application ID, debug origin, and vault namespace changed to the Mobile 002 QA
identity. Its
temporary, QA-only seed activity wrote an old encrypted schema-v2 account store
using the installed package's real vault keyspace. It seeded one revision-less
note bundle, one old queue item, and one draft without relying on a new 3102
download:

| fixture item | recorded value |
| --- | --- |
| Person / old revision | `10000000-0000-4000-8000-000000000051` / `1` |
| revision-less note | `10000000-0000-4000-8000-000000000052`, with no `revision` key |
| old queued operation | `e1780f92-8d7d-48c5-9905-60e737fd2f62` (`add_note`) |
| old operation envelope SHA-256 | `ddb1c87f632bd0fa08f9d30f54d2854a470de2a64ff8cfbd783a78d4cca35acb` |
| retained draft | `legacy-draft`: `legacy retained draft` |

Only the disposable `org.crm.field.mobile002qa` package was uninstalled to
install that old fixture. The current QA APK was then installed in place with:

```sh
adb -s emulator-5554 install -r -g \
  /private/tmp/crm-mobile002-android-build/outputs/apk/mobile002qa/debug/CrmField-mobile002qa-debug.apk
```

After force-stop/relaunch, the QA-only read-only probe logged the same Person,
original operation UUID and envelope digest, and retained draft. It also showed
`qualified=false` and no note revision, which is the required safe initial state
for a revision-less Mobile 001 note bundle:

```text
UPGRADE_PROBE person=...0051 qualified=false note_revision=<none> \
operation=e1780f92-8d7d-48c5-9905-60e737fd2f62 \
digest=ddb1c87f632bd0fa08f9d30f54d2854a470de2a64ff8cfbd783a78d4cca35acb \
draft={"person_id":"...0051","body":"legacy retained draft"}
```

The current QA build contains a QA-only deterministic refetch equivalent because
the synthetic legacy fixture has no matching authenticated API actor. It replaces
the fully staged note component for that same Person revision in one Room
transaction, adding the note revision and setting `noteRevisionsQualified=true`.
The post-transaction log confirms the exact old queue UUID and draft remain:

```text
UPGRADE_QUALIFIED person=...0051 person_revision=1 qualified=true note_revision=1 \
operation=e1780f92-8d7d-48c5-9905-60e737fd2f62 draft=true
UPGRADE_PROBE person=...0051 qualified=true note_revision=1 \
operation=e1780f92-8d7d-48c5-9905-60e737fd2f62 \
digest=ddb1c87f632bd0fa08f9d30f54d2854a470de2a64ff8cfbd783a78d4cca35acb \
draft={"person_id":"...0051","body":"legacy retained draft"}
```

Finally, starting the real current QA `MainActivity` rendered its protected
sign-in screen with UI-automator text `2 saved items remain protected on this
device.` This is the expected two persisted queue/draft records in the locked
fixture store. `FLAG_SECURE` makes pixel screencaps black; the corresponding
runtime hierarchy was inspected rather than treating that black image as visual
evidence. The original demo `org.crm.field` package remained installed throughout.

## Historical scope before the later real-API QA pass

The live session was not used to mutate a reserved Person record before this
checkpoint. Thus real offline edit save/force-stop/relaunch, lost accepted
response replay, two-actor conflict/current-read/revised operation, removal and
unsupported-capability cases still need recorded evidence. The populated legacy
store and in-place upgrade evidence above is intentionally isolated from API3102;
it demonstrates preserved legacy bytes and atomic cache qualification without
claiming that a fresh Mobile 002 download is legacy-cache proof.

## Follow-up live run

After the populated-fixture archive was preserved at
`/private/tmp/crm-mobile002-upgrade-fixture-artifact/field.mobile002qa.v3.tar`
(SHA-256 `80fe01438f3f52a342f6e69769de2297957b6de104f6d43c454666318440007b`),
only the disposable QA package was reset. The current QA APK then authenticated
to API3102 and completed a single 100-Person reconciliation; the inspected
hierarchy reported `Up to date. Complete cache available offline.` and
`Complete: 100 People`.

On reserved `Mobile Person 099`, sync was explicitly paused before editing a
note. The note composer reported `Draft saved on this device`; after an app
force-stop/relaunch the inspected Saved work hierarchy still contained the
persisted draft and the control was `Resume sync`. This is evidence for durable
local draft retention across process death. A Saved work card incorrectly called
an `edit_note` draft a task draft; that presentation bug is corrected in the
current source.

The emulator killed the QA process while the draft was being reopened (logcat
shows process signal 9), before it could be explicitly submitted and replayed.
Consequently, this run records no accepted operation receipt and makes no claim
for offline replay, task field preservation, or two-actor conflict/current-read
behavior. Those live mutation cases remain required.

## Completed Mobile 002 real-API acceptance

The subsequent acceptance run used the same Android 17/API 37 AVD, installed
the already-built QA and test APKs manually, and invoked the exact test methods
with `adb shell am instrument`. This preserves the QA package's encrypted store
between the intentional force-stop and relaunch; Gradle's connected-test task
uninstalls the target package after a run and was therefore not used for that
process-death proof. The dedicated synthetic `android@mobile.test` actor had no
assigned People, so the production `FieldRepository.pin` path pinned only the
reserved records 096, 097, and 098 before each normal authenticated
reconciliation. No demo package, API3101, or unreserved Person was changed.

The live test source is
`android/src/androidTest/java/org/crm/field/Mobile002AcceptanceTest.kt` and the
instrumentation transcripts are retained at:

- `/private/tmp/crm-mobile002-note-prepare5.log`
- `/private/tmp/crm-mobile002-note-relaunch2.log`
- `/private/tmp/crm-mobile002-task2.log`
- `/private/tmp/crm-mobile002-conflict2.log`
- `/private/tmp/crm-mobile002-conflict-receipt.log`

Each transcript reports `OK (1 test)`.

1. **Existing note, offline queue, force-stop and exact replay.** The test
   edited P096's existing note while sync was paused, submitted immutable
   operation `5e279131-89f0-4fc6-8dbc-3097398709a8`, force-stopped
   `org.crm.field.mobile002qa`, and relaunched it. The same stored envelope
   (SHA-256 `248f5466197918f23401487fdd085c988823033ae56895cde638f89dfb88c224`)
   received an `accepted` receipt with committed revision `2` and person
   revision `23`. The local status can become `covered` in the same sync only
   after that accepted receipt; the test asserts the receipt outcome and
   byte-for-byte envelope equality.
2. **Existing completed task update.** The fixture begins with open tasks, so
   the test first created then completed P097 task
   `d2c7bd7a-5fb7-4cf0-8db1-87a44b279f85` through distinct ordinary
   `create_task` and `complete_task` setup operations
   `8c12bef8-b600-48ff-9a30-fdcb48ce9547` and
   `e23a5075-43a4-4d92-85ff-2b4e000c207a`. It refetched the resulting completed
   record before issuing Mobile002 `update_task`
   `bd4f0a87-5799-4566-98c2-d8405fdeb92a`, which changed the title and kind to
   `other` and cleared `due_at`. The accepted receipt has committed revision
   `3`; the test verifies unchanged `completed_at`, assignee, and creator.
3. **Second-actor revision conflict and revised operation.** P098's first
   primary proposal `e3774872-8062-43e3-b39f-ccbdc3d9f314` conflicted after
   second actor operation `2d2b021c-ff9f-40d1-9278-c49955283a6f` was accepted.
   The native Saved work UI asserted `Your saved edit`, `Version you started
   from`, `Current version`, and `Review and revise`, using the protected
   baseline, immutable proposal, and authoritative current note. Calling the
   normal `supersedeConflict` path created fresh revised operation
   `2bffbe9a-c336-4985-8300-1583a48b5460` at current revision `3`; the old
   operation was asserted `superseded`. A separate receipt-drain invocation
   then accepted/covered that exact revised operation with receipt and
   authoritative note revision `4`, verified its body remains `Mobile002
   revised primary proposal 098`, and re-verified the old operation remains
   `superseded`.

Root independently verified the four newly recorded Gradle dependency hashes
against their official Maven Central artifact URLs; all passed in
`/private/tmp/crm-mobile002-qa/android-metadata-hash-audit.log`.

The emulator had intermittent SQLCipher/Room connection contention during the
two-actor run (the longest recorded wait was about 11 seconds), but every final
instrumentation case completed successfully. Physical-phone validation remains
deferred.
