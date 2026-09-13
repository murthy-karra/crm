# Mobile 004 iOS verification

Runner: Xcode 26.5, iPhone 17 Pro Simulator
`007BB316-AB17-4D10-979D-C1F6D50AA285`, API `http://127.0.0.1:3102`, database
`crm_mobile_004`, QA bundle `dev.crm.FieldCRM.mobile004qa`.

The focused real-API test uses the reserved synthetic iOS installation
`6a3fdca6-981a-4f20-a1fe-5bcf0d115b77` and Person001
`13e8d47a-5648-4d88-9358-fe5ed400078d`. It downloads the opted-in catalog,
persists a stage proposal, closes/reopens the encrypted store, deliberately
drops the accepted response, replays the exact bytes, checks the durable
`person_stage` receipt/current-stage read, and proves a second actor produces a
fresh-operation revision conflict.

## Executed

- `xcodebuild ... -configuration Mobile004QA build-for-testing
  CODE_SIGN_IDENTITY=-` — passed.
- `xcodebuild ... -only-testing:FieldCRMTests/StorageTests ...` — passed:
  schema-6 to schema-7 operation-byte preservation, missing-old-cache stage
  qualification, same-broad-revision representation replacement, catalog page
  completeness/atomic promotion, disk-full rollback, locked/key and identity
  fences.
- `xcodebuild ... -only-testing:FieldCRMTests/LiveAPITests/testMobile004RealStageLostResponseReplayConflictAndCurrentRead ...`
  — passed. Result bundle:
  `/tmp/crm-mobile004-010e4/ios/Logs/Test/Test-FieldCRM-2026.09.13_09-02-55--0700.xcresult`.

An earlier run of the legacy non-reserved live test returned `429
mobile_capacity`; it used ephemeral contexts. It was replaced for Mobile004
verification by the persistent reserved installation above. The coordinator
removed only two abandoned unsealed generations for that same context; no
receipts, drafts, People, contexts or other lane resources were removed.

## Remaining evidence

The archived Mobile003-app installed-store upgrade and the visible UI
offline-save/terminate/relaunch flow still require the coordinator's prepared
historical-app fixture. They are not represented as a passing check here.
