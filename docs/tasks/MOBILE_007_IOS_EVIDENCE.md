# Mobile007 iOS final evidence

## Current result

PASS: storage/recovery, the installed Mobile006 upgrade and the complete real
API UI journey. UI12 passes in 179.813s, including exact matching, requested intent
after clear/restart, the complete 101-Person pinned selection, offline note
submission and saved work after a further process restart.

## Interrupted catalog recovery

A real repeated UI run exposed `invalidProtocol` after a completed stage catalog
was requested again while resuming an unfinished generation. The stage cursor
now distinguishes completed catalogs from an initial empty cursor. Reconciliation
skips the completed catalog and continues Person components.

- Regression: `testMobile007ResumeSkipsACompletedStageCatalog` fails before the
  correction in `/tmp/mobile007-ios-stage-resume-red.log`.
- All 36 storage tests pass after the correction in
  `/tmp/mobile007-ios-stage-resume-green.log` (5.812s).
- The subsequent real API journey completes the previously interrupted download.

This is an authored recovery correction within the accepted slice and its second
review/fix round, not a third source review.

## Installed Mobile006 → Mobile007 upgrade

The old production source is the tracked iOS tree at
`edac8c3975bd457c436171ce47075689cfcc14ef`, copied to
`/private/tmp/crm-mobile007-ios-legacy-source`. Only the dedicated bundle identifier
and opt-in test fixture were added. The current production snapshot is at
`/private/tmp/crm-mobile007-ios-current-source`; its `production-manifest.json`
records the exact Swift hashes. Both use bundle
`dev.crm.FieldCRM.mobile007upgradeproof` on simulator
`797D7976-06D1-4D17-BF5D-8309A24AC9CF`.

The old binary creates a real encrypted schema-9 store in its installed app data
container with 100 complete cached People, an unsent draft, a pending immutable
command, an accepted command/receipt, legacy pins and the update-required flag.
The current binary installs over that bundle and opens the same database at
schema 10. It compares all original bundle, draft, operation, receipt, overlay and
selected metadata bytes against the old binary's saved inventory and checks the
original Keychain key. The legacy pin is initialized as a desired intent.

- Seed PASS: `/tmp/mobile007-ios-installed-upgrade-seed5.log`.
- Upgrade PASS: `/tmp/mobile007-ios-installed-upgrade-verify.log`.
- Structural installation evidence:
  `/private/tmp/crm-mobile007-010d3-yyoxjx50/native-diagnostic/ios-installed-upgrade.json`.
- Inventory SHA-256:
  `b5a43fa891be72c0e858cb5d8274b5cda9307b08f7e56306ba085777c68bab1d`.

No uninstall, database replacement or key reset occurred. iOS remapped the data
container UUID during installation; the physical database inode stayed unchanged.
The first structural check incorrectly required an unchanged container UUID; the
record preserves both paths and correctly distinguishes a moved container from a
replaced database.

Earlier fixture attempts are retained: seed build 1 required two missing Swift
`try` annotations; build 2 lacked the archived shared contract resources; attempt
3 could not launch on a new simulator (`CoreSimulator server died`) and was
interrupted. Attempt 4 correctly rejected an invalid synthetic add-note receipt
with a non-null committed revision. Attempt 5 resumes that retained partial seed
with the valid receipt shape and passes. No customer or shared demo data was reset.

## UI fixture and bounded earlier attempts

The real API is the owned synthetic service on port 3101 with database
`crm_mobile_007_current_20260915`. The original 100 assigned People remain the
base selection. `Remote Prospect` is unassigned and downloaded only by explicit
pin. Two further unassigned `Discovery Twin` records have different synthetic
email/phone values; the fixture script and log are
`/private/tmp/crm-mobile007-010d3-yyoxjx50/native-diagnostic/discovery-pair.sql`
and `.log`.

The extended UI11 run verifies exact name/email/phone searches, same-name
selection, requested-intent persistence after clear/restart, complete 101-Person
selection, last-synced pinned reason, and use of the offline note editor. It fails
its final inline note assertion on the Person list without scrolling to the note;
it is not a passing full journey. UI12 moves that assertion to Saved
work and repeats it after process termination/relaunch; it passes. Log:
`/tmp/mobile007-ios-complete-ui12.log`. Its two screenshot attachments are retained
in `/private/tmp/crm-mobile007-010d3-yyoxjx50/native-diagnostic/ios-final-attachments/`
with the export manifest.

Earlier interrupted or failed journeys remain in the native evidence trail;
none is relabeled as a complete UI pass. This synthetic simulator proof does not
claim physical-device distribution or calling acceptance.
