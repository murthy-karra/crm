# Mobile 004 iOS verification

Runner: Xcode 26.5, iPhone 17 Pro Simulator
`007BB316-AB17-4D10-979D-C1F6D50AA285`; isolated API
`http://127.0.0.1:3102`; database `crm_mobile_004`.

The normal Mobile004 QA app uses `dev.crm.FieldCRM.mobile004qa`.  The upgrade
proof uses the separately created `dev.crm.FieldCRM.mobile004upgradeqa`; it
does not overwrite the normal, Mobile003, Mobile002, or default app stores.

## Installed Mobile003 to Mobile004 upgrade

The starting application was the supplied archived native Mobile003 source at
`/private/tmp/crm-mobile004-010e4/mobile003-native-source/ios`, revision
`c581e1e`.  Its upgrade-only build created a real schema-6 encrypted store in
the same upgrade-only bundle/container after an API-3102 sign-in and complete
download.  It then inserted a cached Person001 bundle, one accepted note and
receipt, two queued old operations (task and contact attempt), and a retained
draft.  Before upgrade it recorded the exact operation identifiers and envelope
digests, receipt presence, draft digests, active cache identifier, and the
SQLCipher-key fingerprint.

```
cd /private/tmp/crm-mobile004-010e4/mobile003-native-source/ios
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -project FieldCRM.xcodeproj -scheme FieldCRMMobile004UpgradeQA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile004-010e4/ios-historical-key test \
  -only-testing:FieldCRMTests/ModelTests/testPrepareMobile004InstalledSchemaSixInventory \
  CODE_SIGN_IDENTITY=-
```

Passed. Result bundle:
`/private/tmp/crm-mobile004-010e4/ios-historical-key/Logs/Test/Test-FieldCRMMobile004UpgradeQA-2026.09.13_09-13-17--0700.xcresult`.

Without deleting that app identity or its keychain items, current Mobile004
opened the installed schema-6 store and performed its additive schema-7
migration.  It compared each recorded protected operation ID and original-byte
digest, receipt bit, retained draft ID/digest, active Person cache, and the
existing SQLCipher-key fingerprint.

```
cd /Users/karrad/projects/crm-worktrees/mobile-004-ios
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile004UpgradeQA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile004-010e4/ios-upgrade-key test \
  -only-testing:FieldCRMTests/ModelTests/testMobile004OpensActualInstalledMobile003StoreWithoutChangingProtectedRows \
  CODE_SIGN_IDENTITY=-
```

Passed. Result bundle:
`/private/tmp/crm-mobile004-010e4/ios-upgrade-key/Logs/Test/Test-FieldCRMMobile004UpgradeQA-2026.09.13_09-13-51--0700.xcresult`.

## Native offline stage flow

The current Mobile004 app then used that upgraded installed store.  It synced
the real stage catalog and Person001, turned offline mode on, visibly saved a
stage proposal, showed one pending item, terminated, relaunched, showed the
same pending item, reconnected to API 3102, and reached zero pending.  The
result contains screenshots named `mobile004-stage-pending-after-offline-save`
and `mobile004-stage-accepted-after-relaunch`.

```
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile004UpgradeQA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile004-010e4/ios-upgrade-key test \
  -only-testing:FieldCRMUITests/FieldFlowTests/testMobile004NativeOfflineStageTerminatesRelaunchesAndSynchronizes \
  CODE_SIGN_IDENTITY=-
```

Passed. Result bundle:
`/private/tmp/crm-mobile004-010e4/ios-upgrade-key/Logs/Test/Test-FieldCRMMobile004UpgradeQA-2026.09.13_09-14-18--0700.xcresult`.

The visible picker retains the server's selected stage for this test, so the UI
exercise is an accepted no-op stage receipt.  The separate real-API test below
changes to a different stage and proves replay and conflict behavior.

## Stage protocol and encrypted-cache checks

`FieldCRMTests/StorageTests` and `FieldCRMTests/ModelTests` passed under the
Mobile004 QA scheme: schema-6 to schema-7 no-reserialization preservation,
stage-revision qualification when old cache data lacks it, catalog completeness
and atomic promotion, catalog-only label refresh, mixed queues, lock, disk-full,
lease, and identity fences.

The focused real-API test used reserved installation
`6a3fdca6-981a-4f20-a1fe-5bcf0d115b77` and Person001
`13e8d47a-5648-4d88-9358-fe5ed400078d`.

```
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile004QA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile004-010e4/ios test \
  -only-testing:FieldCRMTests/LiveAPITests/testMobile004RealStageLostResponseReplayConflictAndCurrentRead \
  CODE_SIGN_IDENTITY=-
```

Passed. It downloaded the opted-in catalog, made a different-target stage
transition, closed and reopened the encrypted store, deliberately lost the
accepted response, replayed the exact immutable envelope, checked the durable
`person_stage` receipt and current-stage endpoint, and used the second actor to
obtain a revision conflict. Result bundle:
`/private/tmp/crm-mobile004-010e4/ios/Logs/Test/Test-FieldCRM-2026.09.13_09-02-55--0700.xcresult`.

One earlier legacy live test returned `429 mobile_capacity` because it minted
ephemeral reconciliation generations. Mobile004 real-API verification uses the
reserved persistent installation above; that run passed. A first UI attempt
looked for a PickerWheel that SwiftUI did not expose. The test was corrected to
save the displayed server stage and the rerun above passed.

All evidence is simulator-only; no physical-device, cellular, or distribution
claim is made.

## Round-one follow-up and conflict review correction

The independent review identified that conflict replacement checked the supplied
conflicted predecessor as an unresolved operation before it could be
superseded, and that a second stage selection was not retained as a draft. The
corrected storage test proves that only the explicitly supplied conflicting
predecessor is excluded, its immutable bytes remain unchanged, the replacement
has a new operation ID, and a second selection remains a typed follow-up draft
until the agent explicitly submits it after the predecessor receipt. The test
also promotes a valid 2 KiB stage label, under the server component budget.

```
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile004QA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile004-010e4/ios-round1 test \
  -only-testing:FieldCRMTests/StorageTests -only-testing:FieldCRMTests/ModelTests \
  CODE_SIGN_IDENTITY=-
```

Passed: 35 tests. Result bundle:
`/private/tmp/crm-mobile004-010e4/ios-round1-final/Logs/Test/Test-FieldCRMMobile004QA-2026.09.13_09-33-04--0700.xcresult`.

The native upgrade-QA simulator flow now visibly creates a follow-up while its
predecessor is pending, persists it through termination/relaunch, accepts the
first operation, then requires an explicit follow-up Save before a second
outbox operation is created and synced.

```
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -project ios/FieldCRM.xcodeproj -scheme FieldCRMMobile004UpgradeQA \
  -destination 'platform=iOS Simulator,id=007BB316-AB17-4D10-979D-C1F6D50AA285' \
  -derivedDataPath /private/tmp/crm-mobile004-010e4/ios-round1-ui test \
  -only-testing:FieldCRMUITests/FieldFlowTests/testMobile004NativeOfflineStageTerminatesRelaunchesAndSynchronizes \
  CODE_SIGN_IDENTITY=-
```

Passed. Result bundle:
`/private/tmp/crm-mobile004-010e4/ios-round1-ui/Logs/Test/Test-FieldCRMMobile004UpgradeQA-2026.09.13_09-30-48--0700.xcresult`.
