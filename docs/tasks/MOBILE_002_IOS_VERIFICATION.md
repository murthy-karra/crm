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
   generation `a90fb6c2-c6df-4a13-bc69-3b6e6bf4bf73` was explicitly sealed.
5. The QA app was installed and launched on iPhone 17 Simulator
   `F5BF9C74-37AC-4546-A576-F30587CB4F4C` with
   `--synthetic-keychain`. The inspected launch screen is
   `/private/tmp/crm-mobile002-native-evidence/ios-qa-launch-v2.png`; it displays
   the synthetic-only banner and sign-in UI.

## Failed/remaining native evidence

The first actual UI edit attempt failed because the fresh Person001 fixture did
not contain a downloadable note. Follow-up attempts fixed the navigation and
seeded a note online. The latest run reached the forced post-seed full
reconciliation, then timed out after 120 seconds before the isolated API
reported a complete workspace. Result:
`/private/tmp/crm-mobile002-ios-result-bundles/qa-ui-v14.xcresult`. This is
attributed as an isolated generation/capacity runtime issue and is not hidden as
a native edit success.
No QA store was wiped to turn that failure into a pass.

The installed-store proof using a separately built Mobile 001-compatible QA
fixture followed by an in-place Mobile 002 update, and native UI proof for
operation/receipt 404 ambiguity, permission loss, explicit conflict review and
resubmit remain incomplete. The protected store/model and real API checks cover
their underlying behavior, but are not substitutes for those native acceptance
scenarios. Physical device, cellular, signing and distribution are deferred.
