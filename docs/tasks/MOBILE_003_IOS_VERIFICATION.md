# Mobile 003 iOS verification

**Status: accepted native simulator evidence recorded on 2026-09-13.**

The iOS implementation adds the Mobile 003 `log_contact_attempt` operation to
the existing protected Mobile 001/002 store. It uses the frozen Mobile 003
contract: channel/outcome/explicit reported occurrence instant, no contact free
text, `contact_attempt` receipts with null `committed_revision`, and a fresh
server-sealed Today snapshot after acceptance even if the Person revision does
not change.

## Source and isolated runtime

- Source branch: `codex/mobile-003-ios`, based on `90b6ae2` / `c3d5b47`.
- QA bundle and protected container: `dev.crm.FieldCRM.mobile003qa` / 
  `SyntheticFieldCRMMobile003QA`; this is distinct from the installed demo and
  Mobile 002 QA app/container.
- Simulator: iPhone 17e (iOS 26.5). No physical device, cellular, or release
  distribution test was performed.
- API: isolated `http://127.0.0.1:3102`, database `crm_mobile_003`, verified
  by the coordinator before native execution. The shared demo API/store was not
  used.
- Native fixture target: reserved iOS Person001,
  `f40f5132-9822-4bdf-ba34-76affb181195`. No Android or shared Person050
  fixture was mutated by these tests.

## Checks and observed results

1. Focused protected-storage/model suite passed **29 tests, 0 failures**:

   ```sh
   DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
     -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile003QA \
     -destination 'platform=iOS Simulator,name=iPhone 17e' \
     -derivedDataPath /private/tmp/crm-mobile003-ios-build test \
     -only-testing:FieldCRMTests/StorageTests \
     -only-testing:FieldCRMTests/ModelTests CODE_SIGN_IDENTITY=-
   ```

   Result bundle: `/tmp/crm-mobile003-ios-build/Logs/Test/Test-FieldCRMMobile003QA-2026.09.13_02-28-30--0700.xcresult`.
   This covers schema 5→6 non-destructive migration, byte-identical old
   operation preservation, contact CAS/independent drafts, strict receipt
   validation, full/lock behavior, ambiguous 404 retention, permission locking,
   future-time repair with a new identity, and successful/failed same-revision
   Today refresh paths.

2. Focused real API operation test passed **1 test, 0 failures**:

   ```sh
   DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
     -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile003QA \
     -destination 'platform=iOS Simulator,name=iPhone 17e' \
     -derivedDataPath /private/tmp/crm-mobile003-ios-build test \
     -only-testing:FieldCRMTests/LiveAPITests/testMobile003RealContactReplayTwoIndependentLogsAndFutureRetention \
     CODE_SIGN_IDENTITY=-
   ```

   Result bundle: `/tmp/crm-mobile003-ios-build/Logs/Test/Test-FieldCRMMobile003QA-2026.09.13_02-22-32--0700.xcresult`.
   The test used a stable QA installation ID, submitted a contact to Person001,
   deliberately dropped an accepted response, replayed the exact saved bytes to
   the same contact fact, accepted a second independent contact, and retained
   the future-time-rejected original bytes in attention state.

3. Installed populated-store upgrade and native UI test both passed.

   A private historical Mobile 002 source archive at `90b6ae2` was built only
   with the isolated Mobile003QA bundle/key namespace. Its existing native flow
   populated the encrypted cache and accepted note/task receipts on Person001.
   The current Mobile 003 app was installed over that same app identifier and
   protected store; it opened the store at schema 6. The prior-app evidence is
   `/private/tmp/crm-mobile003-qa/m002-populate-upgrade.log` and
   `/tmp/crm-mobile003-m002-build/Logs/Test/Test-FieldCRMMobile003QA-2026.09.13_02-24-52--0700.xcresult`.

   The current native flow then ran offline `Log contact` → pending contact badge
   without changing downloaded Today membership → termination/relaunch → online
   receipt → sealed refresh. The QA probe asserted `schema=6` after the upgrade.
   Result bundle:
   `/tmp/crm-mobile003-ios-build/Logs/Test/Test-FieldCRMMobile003QA-2026.09.13_02-26-12--0700.xcresult`.
   The full UI command/log are preserved at
   `/private/tmp/crm-mobile003-qa/mobile003-native-contact.log`.

## Limits

The simulator validates the isolated synthetic runtime, encrypted SQLCipher
store, relaunch behavior, and real API operation path. Physical-device passcode,
reboot, cellular interruption, APNs/background execution, and production
release signing remain deferred.

## Review round 1 — 2026-09-13

Focused QA build and storage/model run after review fixes:

```text
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild test \
  -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile003QA \
  -destination 'platform=iOS Simulator,id=A5A6F06E-9E4D-4D55-9882-940ED696D226' \
  -only-testing:FieldCRMTests/StorageTests -only-testing:FieldCRMTests/ModelTests \
  -derivedDataPath /tmp/crm-mobile003-review-build
```

Passed: 32 tests, zero failures. Result bundle:
`/tmp/crm-mobile003-review-build/Logs/Test/Test-FieldCRMMobile003QA-2026.09.13_02-36-38--0700.xcresult`.

This run proves UTC-only contact timestamp selection survives a simulated Los Angeles DST gap, both explicit offsets in the repeated fall wall hour, and reopen under Tokyo; invalid `2026-02-30` is rejected without normalization. It also proves a contact receipt discards pre-receipt staging, retains protected accepted work through a failed seal/relaunch, creates a new generation, and does not re-upload. Contact receipt validation rejects `changed=false`, device-recorded time is frozen when the immutable operation is saved, Mobile 002's `forbidden` unavailable-state semantics remain intact, and full-store contact draft persistence preserves the existing mixed queue.

The actual isolated iPhone 17e UI acceptance was rerun after the review fix and passed: offline manual-contact Save, terminate/relaunch, reconnect, accepted receipt and sealed refresh. Result bundle: `/tmp/crm-mobile003-review-build/Logs/Test/Test-FieldCRMMobile003QA-2026.09.13_02-41-39--0700.xcresult` (1 passed, 0 failures/skips).

### Historical installed-store upgrade inventory

On isolated iPhone 17 Pro `007BB316-AB17-4D10-979D-C1F6D50AA285`, a private historical Mobile002QA host test used the actual `dev.crm.FieldCRM.mobile003qa` container and real API3102 authorization to populate schema 5: one accepted note receipt, one pending task operation, one retained draft, and a sealed active cache. It wrote only structural inventory to the container:

```text
schema=5 active=fa8aa24b-5642-40b4-b1f9-8174e47d4472
ops=1158aa90-c4d2-426b-a416-baab9cbc3644:93b4101d274db669:1,
    a8e3cea7-5e44-45c6-afb5-0fc06f779a08:baa08976b580a87:0
drafts=F6EA7D46-AE2D-4FA4-990E-A0A347E1BDD8:5d2a70d4d658b822
```

Without uninstalling that QA bundle, Mobile003 opened the same store, migrated it to schema 6, and compared every prior operation ID/digest/receipt-presence marker, the retained draft ID/digest, and active-cache presence. Passed result: `/tmp/crm-mobile003-upgrade-build/Logs/Test/Test-FieldCRMMobile003QA-2026.09.13_03-20-08--0700.xcresult` (1 passed, zero failures).

Historical seed result: `/tmp/crm-mobile003-historical-hosted-build/Logs/Test/Test-FieldCRMMobile002QA-2026.09.13_03-18-23--0700.xcresult` (1 passed, zero failures).

The installed comparison is explicitly opt-in. The normal/default test compilation succeeded with `build-for-testing` at `/tmp/crm-mobile003-default-test-build`. A private copy of the Mobile003 QA `.xctestrun` added `CRM_MOBILE003_INSTALLED_UPGRADE=approved-synthetic` to both the hosted test target environments; `test-without-building` then executed the actual installed comparison, passing at `/Users/karrad/Library/Developer/Xcode/DerivedData/FieldCRM-cyeniptgsuqbdacrwoyqexuhigmc/Logs/Test/Test-FieldCRMMobile003QA-2026.09.13_03-34-38--0700.xcresult` (1 passed, 0 skips/failures).
