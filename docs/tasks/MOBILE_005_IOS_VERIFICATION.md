# Mobile005 iOS verification

Verified on 2026-09-14 from iOS branch `codex/mobile-005-ios`.  The native
checks used Xcode 26.6 (17F113) and the iOS 26.5 iPhone 17 Pro simulator
`007BB316-AB17-4D10-979D-C1F6D50AA285`.  Every build product and test result
is under `/private/tmp/crm-mobile005-010f3/ios`; no shared service build
artifact was replaced.

The isolated native service was `http://127.0.0.1:3103`, database
`crm_mobile_005`, source `382aadc10d635fa72443fa0cf482577ce83fcd48`, binary
SHA-256 `da3065f283c0b295933f6e82eb446fd261ca5f9d203e03fb553d0b43a9fbc529`.
The test fixture was the isolated Person001 UUID
`07cb08d0-56c3-43c1-a538-68573e5a24f0`; no shared API on 3000/3101/3102 was
used.

## Installed schema-7 to schema-8 upgrade

The archived actual Mobile004 source at
`/private/tmp/crm-mobile005-010f3/integration/mobile004-native-source/ios`
(the task-provided `fcc05b3` source) was adapted only in that private archive
to give it the isolated `dev.crm.FieldCRM.mobile005upgradeqa` bundle.  Its
schema-7 application was installed first and created a protected SQLCipher
store containing an active cache, one pending immutable note envelope, one
accepted note receipt, and an unsent protected draft.  It recorded the actual
SQLCipher key fingerprint plus base64 of every encrypted-row envelope and draft
in the app container inventory.

```text
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild -quiet \
  -project FieldCRM.xcodeproj -scheme FieldCRMMobile005UpgradeQA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile005-010f3/ios/historical005 test \
  -only-testing:FieldCRMTests/ModelTests/testMobile005CreatesActualInstalledSchemaSevenInventory CODE_SIGN_IDENTITY=-
```

Result: passed (1 test),
`/private/tmp/crm-mobile005-010f3/ios/historical005/Logs/Test/Test-FieldCRMMobile005UpgradeQA-2026.09.14_11-31-11--0700.xcresult`.

Without uninstalling or resetting that bundle or replacing its key, the current
Mobile005 application was then installed over it.  The schema-8 test opened the
same protected file and proved user version 8, the identical key fingerprint,
exact envelope bytes, receipt presence, exact protected draft bytes, retained
cache, and no invented details qualification for the old cache.

```text
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild -quiet \
  -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile005UpgradeQA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile005-010f3/ios/upgrade005 test \
  -only-testing:FieldCRMTests/ModelTests/testMobile005OpensActualInstalledSchemaSevenStoreWithoutChangingProtectedRows CODE_SIGN_IDENTITY=-
```

Result: passed (1 test),
`/private/tmp/crm-mobile005-010f3/ios/upgrade005/Logs/Test/Test-FieldCRMMobile005UpgradeQA-2026.09.14_11-46-31--0700.xcresult`.

## Storage and model coverage

### Independent review round1 correction

Coordinator changes derive contact display/editor order from the complete
`import_order NULLS LAST,created_at,id` projection while preserving stored page
order and legacy readability. Primary-removal previews are calculated per kind
and displayed before submission; new local methods remain a distinct section.
A deliberately UUID-scrambled fixture covers null order, timestamp and UUID ties,
mixed phone/email primary fallback, and unchanged raw page data.

`xcodebuild` with the same scheme/destination below, coordinator root checkout,
`-derivedDataPath /private/tmp/crm-mobile005-010f3/ios/review005`, and both
StorageTests/ModelTests filters passed40/40, zero failures/skips. Evidence:
`ios/mobile005-review-order-tests1.log`. Xcode later rotated that first40-test
bundle; the complete42-test superseding result below retains the same regression.
The subsequent fresh-add button adjustment appends entries within the distinct
New section to preserve user insertion order. Final correction verification at
root `43d8302` passed42/42: both storage/model suites plus the actual multi-field
offline/restart/sync and conflict/current/replacement UI journeys. Runner:
private `integration/run-ios-review-check.py`, serialized native API lock,
133.89s total. Evidence: `ios/mobile005-review-final-native1.log` and
`ios/review-native-42.xcresult` (copied outside Xcode's rotating log directory).
The native API remains the immutable `382aadc` fixture recorded below. The later
server ordering/locking corrections have separate backend DB evidence. This is
round1 correction evidence, not review approval.

Round1 finding8 subsequently adds actual-wire512-KiB and100-row bounds, complete
page shape and metadata validation in the current-details API; a shared complete
traversal rejects cursor cycles, empty continuations, duplicate contact IDs and
changed profile bindings. Both explicit requalification and automatic conflict
review use it. Unrelated broad-revision changes remain allowed while details
revision/names stay pinned. The undocumented10,000-total-row rejection is removed;
the accepted contract bounds pages without inventing a new collection limit.
Invalid responses preserve exact queued bytes, baseline and proposal.

Storage/model verification passed43/43, zero failures/skips, in
`ios/mobile005-current-pages-tests1.log` and
`ios/review-current-pages-43.xcresult`. Three new tests cover oversized rows/UTF-8
bytes/cyclic cursors, valid multi-page reads with unrelated broad changes, and
automatic conflict fallback retaining the immutable envelope and proposal.

The final API guard also enforces the existing backend2048-byte UTF-8 cursor
limit on input and output. At root `be15cb0` plus this cursor guard/test, the
storage/model rerun passed43/43, zero failures/skips:
`ios/mobile005-current-pages-tests2.log` and explicit result bundle
`ios/current-pages-final-43.xcresult`. Both native UI journeys passed2/2 at
`8b60354`, using valid cursors within that limit: serialized runner
`integration/run-ios-current-pages-ui.py`,124.68s,
`ios/mobile005-current-pages-native1.log`,
`ios/current-pages-native-2.xcresult`. These explicit result bundles are outside
Xcode's rotating Logs/Test directory. The later cursor-only guard is covered by
the43-test rerun; it does not invalidate the successful valid-cursor UI journeys.

```text
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild -quiet \
  -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile005QA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile005-010f3/ios/modelstorage005 test \
  -only-testing:FieldCRMTests/ModelTests -only-testing:FieldCRMTests/StorageTests CODE_SIGN_IDENTITY=-
```

Result: passed (39 tests),
`/private/tmp/crm-mobile005-010f3/ios/modelstorage005c/Logs/Test/Test-FieldCRMMobile005QA-2026.09.14_12-43-41--0700.xcresult`.

This covers encrypted storage, CAS/identity fencing, follow-up and conflict
preservation, schema upgrades, details-only qualification, same-broad-revision
replacement, and atomic detail receipts.  A dedicated 51-contact imported
profile test verifies a one-name edit preserves the large cached profile and
serializes only `first_name` plus the required empty `contact_operations`
array.  It also rejects a Mobile005 details receipt which omits
`added_contact_ids`, while preserving the original immutable queue row for
exact replay.  Legacy receipt shapes still decode for their legacy operation
kinds.

The focused editor projection coverage also reopens a sparse saved proposal
over its baseline, retains existing contact IDs and deterministic local IDs for
unaccepted adds, preserves the serialized operation order, and proves an
untouched whitespace-bearing contact and an over-limit imported name are not
normalized or submitted when a different field changes.

## Serialized native API and UI acceptance

All API-mutating checks were invoked via `run-native-check.py --lane ios`.

```text
python3 /private/tmp/crm-mobile005-010f3/run-native-check.py --lane ios \
  --log mobile005-live-final.log -- env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  xcodebuild -quiet -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile005QA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile005-010f3/ios/live005 test \
  -only-testing:FieldCRMTests/LiveAPITests/testMobile005RealDetailsLostResponseReplayConflictAndCurrentTraversal CODE_SIGN_IDENTITY=-
```

Result: passed (1 test), log
`/private/tmp/crm-mobile005-010f3/ios/mobile005-live-final.log`, result
`/private/tmp/crm-mobile005-010f3/ios/live005/Logs/Test/Test-FieldCRMMobile005QA-2026.09.14_11-35-01--0700.xcresult`.
This uses a deliberate client-side lost response, exact immutable replay,
validates original operation-array ordinals for two added contacts, obtains
current details, and proves a second actor causes the primary stale revision
to conflict.

```text
python3 /private/tmp/crm-mobile005-010f3/run-native-check.py --lane ios \
  --log mobile005-ui-final3.log -- env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  xcodebuild -quiet -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile005QA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile005-010f3/ios/ui005 test \
  -only-testing:FieldCRMUITests/FieldFlowTests/testMobile005NativeOfflineProfileTerminateRelaunchAndSynchronize CODE_SIGN_IDENTITY=-
```

Result: passed (1 test), log
`/private/tmp/crm-mobile005-010f3/ios/mobile005-ui-final3.log`, result
`/private/tmp/crm-mobile005-010f3/ios/ui005/Logs/Test/Test-FieldCRMMobile005QA-2026.09.14_11-38-07--0700.xcresult`.
It syncs the real fixture, takes the profile editor offline, saves a name
change, terminates/relaunches, and synchronizes the exact queued action.
Screenshots are XCTest synthetic-simulator attachments only.

The final full multi-field execution used the same serialized runner with log
`/private/tmp/crm-mobile005-010f3/ios/mobile005-ui-multifield-final13.log`
and result
`/private/tmp/crm-mobile005-010f3/ios/ui005/Logs/Test/Test-FieldCRMMobile005QA-2026.09.14_12-41-40--0700.xcresult`:

```text
python3 /private/tmp/crm-mobile005-010f3/run-native-check.py --lane ios \
  --log /private/tmp/crm-mobile005-010f3/ios/mobile005-ui-multifield-final13.log -- \
  env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  xcodebuild -quiet -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile005QA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile005-010f3/ios/ui005 test \
  -only-testing:FieldCRMUITests/FieldFlowTests/testMobile005NativeOfflineProfileTerminateRelaunchAndSynchronize CODE_SIGN_IDENTITY=-
```

Result: passed (1 test).  It edits the name and an existing email, adds a new
email through the native controls, saves while offline, terminates/relaunches,
checks the exact saved envelope fingerprint, then syncs and verifies this
operation's accepted receipt, the edited and added values, and the add-ID map
by the original operation ordinal.  The `mobile005-profile-offline-pending`
and `mobile005-profile-synced` XCTest attachments in that result are synthetic
simulator screenshots.

```text
python3 /private/tmp/crm-mobile005-010f3/run-native-check.py --lane ios \
  --log mobile005-conflict-ui-final5.log -- env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  xcodebuild -quiet -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile005QA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile005-010f3/ios/conflictui005 test \
  -only-testing:FieldCRMUITests/FieldFlowTests/testMobile005NativeProfileConflictReviewCurrentAndManualReplacement CODE_SIGN_IDENTITY=-
```

Result: passed (1 test), log
`/private/tmp/crm-mobile005-010f3/ios/mobile005-conflict-ui-final5.log`, result
`/private/tmp/crm-mobile005-010f3/ios/conflictui005/Logs/Test/Test-FieldCRMMobile005QA-2026.09.14_12-47-23--0700.xcresult`.
The real native UI creates the primary immutable profile operation, has the
second authenticated actor advance the profile revision, displays the complete
current profile with the saved proposal, creates an explicit replacement, and
accepts that replacement.  The two retained screenshot attachments are
synthetic-simulator evidence.

## Failed attempts retained for diagnosis

Early direct API attempts exposed the simulator's loopback `Secure`-cookie
rejection and an invalid QA phone value; the client now uses the QA-only
loopback transport handling and valid fixture values.  A subsequent strict
receipt check exposed the isolated API omission of `added_contact_ids` for a
no-add profile receipt; the service was updated to return `[]`, and the client
continues to leave malformed receipts pending for exact replay.

The first multi-field UI attempts are retained as
`mobile005-ui-multifield-final*.log`.  They found successive real layout and
harness issues: actions below the 151 imported contacts, an inserted contact
outside the materialized viewport, an incorrect pending-count-only assertion,
and a swipe in the wrong direction after focusing an existing contact.  The
final implementation places Person actions, add controls, new-contact fields,
status, and Save before the imported-contact traversal; it uses stable QA
identifiers and verifies the specific accepted receipt instead of treating
zero pending as success.  The dedicated conflict/current iterations remain in
`mobile005-conflict-ui-final*.log` and passed at the result cited above.

No physical-device run was performed; simulator keychain availability is
explicitly synthetic and the tests do not represent hardware passcode behavior.

## Independent round2 strict representation and receipt corrections

Coordinator corrections for round2 findings1,4 and the shared portion of6 use one
strict contact validator across summary qualification, editable profiles, current
API pages and display-order fallback. Modern contacts require an explicit nullable
int32 import order, parseable timestamp, UUID, supported kind and nonempty value.
Complete qualified contacts also have unique IDs. Stored qualification is rechecked
against the actual cached body, so a pre-fix malformed cache stays readable but
can be redownloaded at the same broad revision. Saved drafts and queued bytes remain.
Profile receipts reject unchanged responses with add mappings and duplicate mapped
server UUIDs; exact ordinals/counts and other existing receipt checks remain.

The complete storage/model suite passes **47/47**, zero failures/skips,33.79s total:
`ios/round2-strict-tests2.log`, `ios/round2-strict-final-47.xcresult`, and
`ios/round2-strict-summary2.json`. `ios/round2-strict-test-run2.json` records the
exact xcodebuild command and hashes of every tested iOS source file. It uses the
same named iOS26.5 simulator and isolated `modelstorage005c` build directory,
with both StorageTests/ModelTests and parallel testing disabled. Source is root
`3755623` plus the iOS correction committed with this record.

Four added tests cover missing/fractional/string/out-of-range contact order and
other malformed fields, pre-fix qualified-cache replacement at unchanged revision,
malformed current-page draft preservation, and impossible no-op/duplicate-ID
receipts retaining the exact queue. The initial run passed46 but failed one new
assertion because a submitted generic note draft is intentionally removed after
becoming an immutable operation. The corrected test preserves and checks a separate
unsent draft alongside that operation. `round2-strict-tests1.log`,
`round2-strict-47.xcresult` and summary1 remain retained; no production correction
was needed for that fixture assertion.

The actual native journeys and installed schema7→8 proof above are reused with
their source attribution. Valid wire payloads/receipts and schema/UI/envelope
behavior are unchanged; the stricter rejection/requalification cases are covered
by the complete47-test run. Targeted independent correction assessment and combined
final gates remain required; this record alone is not a round2 READY verdict.
