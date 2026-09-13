# Mobile 001 — Native integration evidence

Only synthetic `crm_mobile_001` data were used. The native platform records map
actual source, device, commands and results:

- [iOS verification](../../../tasks/MOBILE_001_IOS_VERIFICATION.md) and
  [bounded review](../../../tasks/MOBILE_001_IOS_REVIEW.md).
- [Android verification](../../../tasks/MOBILE_001_ANDROID_VERIFICATION.md).
- [Backend lifecycle correction](../../../tasks/MOBILE_001_SEALED_GENERATION_VERIFICATION.md).

`api-lifecycle-upgrade.json` records the isolated API rebuild, its binary/source
hashes, two applied private-database migrations and unchanged pre-existing receipt
hash/counts. The two generation-retirement JSON files record precisely selected
completed/abandoned synthetic test rows. This was fixture maintenance after both
owners confirmed no active reads, not a product backfill or weakened admission
limit. Contexts, receipts and business rows were retained.

`shared-executable-restoration.json` records an incidental verification-output
collision: Cargo replaced the root launch-path binaries while the shared API
process kept its previously mapped executable inode. Exact released010d2 binaries
were found in retained dependency output, independently copied outside Cargo,
verified against the released SHA256 manifest, staged and atomically restored.
PID95513 and start time stayed unchanged; `/api/health` returned200. No shared
migration or process restart was performed. The earlier incidental Web output
replacement and exact asset restoration are recorded in
[the migration QA directory](../slice-010e1-2026-09-12/shared-preview-restoration.json).
Future verification must use explicit isolated Cargo and Web output paths.

The iOS worktree was closed after both commits were integrated. Full derived data
and original XCTest bundles now reside at `/private/tmp/crm-ios-native-build-final`;
committed summaries/screenshots remain in `ios/evidence/`. Physical-device,
poor-cellular, production signing/distribution and live customer checks were not
performed. These records establish the approved synthetic implementation scope.

Android commit `83379df` was integrated at `c238b24`; its clean worktree and branch
were removed. All build outputs, reports and APKs were moved to
`/private/tmp/crm-android-native-build-final`. Native committed screenshots were
visually inspected, including pending-work, reboot-lock and due-date restoration.
`integration-cleanup.json` records the exact local commits and preserved paths.
API3101 remains available for the installed synthetic demos; both apps are paused.
Only the primary integration checkout remains. Nothing was pushed or newly deployed.
