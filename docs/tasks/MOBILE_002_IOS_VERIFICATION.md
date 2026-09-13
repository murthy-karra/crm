# Mobile 002 iOS verification

Source: `codex/mobile-002-ios` at the Mobile 002 backend integration base
`2fd9a9d`, with iOS-only changes in this lane. The QA API was the coordinator's
isolated process on `http://127.0.0.1:3102`; the existing 3101 demo, its bundle
ID and its store were not used.

## Implemented iOS scope

`FieldCRMMobile002QA` is a Debug-only Xcode scheme/configuration. It compiles
`MOBILE002_QA`, uses bundle ID `dev.crm.FieldCRM.mobile002qa`, permits only
`127.0.0.1:3102` for that configuration, and uses QA-specific protected key,
application-support and defaults namespaces. Normal Debug retains 3101 and
Release retains HTTPS-only behavior.

The encrypted SQLCipher store migrates schema 3 to 5 without replacing legacy
envelope bytes or readable bundles. Revision-less notes remain visible but fail
edit qualification until a versioned bundle/current-record response is present.
Drafts retain immutable baseline/proposal, target, expected record revision,
predecessor and local CAS revision. Typed `edit_note` and `update_task` payloads
are distinct from add/create operations. Receipt acceptance validates operation,
target, resource type, timestamp and positive revisions, while retaining the
legacy add-note null revision rule. Same-target unresolved work is saved as a
durable follow-up draft. The UI provides native edit controls, saved-draft,
pending and conflict/current/baseline/revised-edit views.

## Checks actually run

All commands set `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` and
used isolated derived-data/result paths.

1. `xcodebuild ... -scheme FieldCRMMobile002QA -configuration Mobile002QA ... build CODE_SIGN_IDENTITY=-`
   passed. The configuration produced the isolated QA bundle.
2. `xcodebuild ... -scheme FieldCRMMobile002QA ... -only-testing:FieldCRMTests/StorageTests ... test`
   passed: 17 tests, result bundle
   `/private/tmp/crm-mobile002-ios-result-bundles/qa-storage-v16.xcresult`.
   This includes SQLCipher/WAL/wrong-key, storage-full rollback, schema-3
   upgrade, old envelope preservation, legacy revision-less note qualification,
   draft CAS, follow-up preservation, receipt identity fences, and both
   ambiguous operation/receipt-404 protected-input handling.
3. `xcodebuild ... -scheme FieldCRMMobile002QA ... -only-testing:FieldCRMTests/LiveAPITests/testMobile002RealLostResponseReplayAndTwoActorConflictOnReservedPerson001 ... test`
   passed (also included in the earlier 17-test QA result bundle
   `/private/tmp/crm-mobile002-ios-result-bundles/qa-storage-v6.xcresult`). It
   used reserved Person001 against API3102, deliberately dropped an accepted
   response, replayed the identical edit envelope, and verified a second actor's
   intervening edit produces `revision_conflict` followed by a current-record
   fetch at revision 3.
4. `testMobile002AcceptedSeedAppearsInCurrentRoutePageAndQualifiedEncryptedBundle`
   passed, result bundle
   `/private/tmp/crm-mobile002-ios-result-bundles/qa-exact.xcresult`. It captured
   an accepted seed receipt's exact note ID, verified it in the authorized
   current-record response and reconciliation page, staged those pages through
   encrypted SQLite, and verified `editableRecord` qualification. Its server
   generation `a90fb6c2-c6df-4a13-bc69-3b6e6bf4bf73` was explicitly sealed. A
   subsequent exact seed/current/page/local-store check also passed with
   `bfb73b8f-4036-49dc-8cb0-a1963babee13`.
5. The QA app was installed and launched on iPhone 17 Simulator
   `F5BF9C74-37AC-4546-A576-F30587CB4F4C` with
   `--synthetic-keychain`. The inspected launch screen is
   `/private/tmp/crm-mobile002-native-evidence/ios-qa-launch-v2.png`; it displays
   the synthetic-only banner and sign-in UI.
6. `testMobile002NativeOfflineEditTerminateRelaunchAndSynchronizeReservedPerson001`
   passed in 51.8 seconds, result bundle
   `/private/tmp/crm-mobile002-ios-result-bundles/qa-ui-v23.xcresult`. It used
   the explicit QA loader to read the exact accepted note through the real
   current-record route, committed it into a complete encrypted Person001
   bundle, edited it while paused, observed one pending operation, terminated
   and relaunched the app with that operation still present, then reconnected
   and observed zero pending operations. The run includes the pending screenshot
   attachment `mobile002-offline-edit-pending`.
7. An isolated `git archive 9cbaf1a` Mobile001 build was compiled under only the
   Mobile002 QA bundle/key/store namespace and run on a second simulator. Its
   populated legacy UI fixture passed in
   `/private/tmp/crm-mobile002-ios-result-bundles/mobile001-populated-old-store-v2.xcresult`.
   It created a schema-3 SQLCipher database, revision-less note snapshots, and
   three old pending envelopes before the old app terminated. The app container
   was inspected and contained its SQLite, WAL and SHM files at the QA-only
   `SyntheticFieldCRMMobile002QA` path.
8. The installed-store upgrade was completed on the separate iPhone 17e
   simulator using the fixed legacy installation ID
   `76b45805-dbb7-4bf0-8ed6-7a6a30542a0f`. The old build version was 0 and the
   Mobile002 QA build version was 1. Before replacement, the old encrypted
   SQLite/WAL/SHM files were copied with SHA-256 records to
   `/private/tmp/crm-mobile002-upgrade-evidence/`. `simctl install` rotated the
   simulator data-container path but preserved byte-identical SQLite, WAL and
   SHM files before the new app launched. The new Mobile002 QA app then opened
   and upgraded that actual preserved encrypted store. The inspected native UI
   showed the synthetic banner, "Saved workspace is available on this device",
   the old cached Today timestamp and one locally saved task change while
   paused. This is the installed old-store upgrade proof; no demo simulator,
   3101 store or shared app identity was used.

## Failed/remaining native evidence

Earlier native attempts failed for a missing seed, lazy Settings form content,
and an unreachable Notes section after a long Task list. These were diagnosed
from accessibility logs and corrected before the successful v23 run. The v14
full-reconciliation capacity timeout remains retained as a failed attempt.

An initial upgrade attempt that relied only on a changed container path was not
accepted as evidence. The subsequent versioned run retained exact old-file
hashes and is the accepted proof above. Native UI proof for operation/receipt
404 ambiguity, permission loss, explicit conflict review and resubmit remain
incomplete. The protected store/model and real API checks cover their underlying
behavior, but are not substitutes for those native acceptance scenarios.
Physical device, cellular, signing and distribution are deferred.
