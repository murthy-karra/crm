# Mobile 001 / 010e1 — Shared-development release evidence

Runtime source `08cb42057013cb8766ae61acb23458b0cf38c416`; released under D-075's
explicit follow-up. The [release record](../../../tasks/MOBILE_001_010e1_RELEASE.md)
explains results and limits. `deployed.json` captures launch-time state;
`release-final.json` and `closeout-preflight-confirm.json` capture closeout and
renewed, time-limited confirmation evidence. `build-summary.json` is pre-deploy
build evidence; its deployed=false field refers to that earlier build phase.

The allowlisted evidence contains build hashes, source hashes, metadata-only
rowset digests/counts, backup metadata, migration checksums, actual inventory,
preflight results, HTTP/browser/mobile checks and actual browser screenshots. Full
database payloads, backup catalog/dump, .env/configuration values and runtime
secrets are excluded. Exact helper scripts, logs and recovery artifacts remain
in the private recovery directory recorded in `release-final.json`.

All123prior tenant rowsets preserve every old column. The two initial revision
columns are verified separately. All134complete upgraded rowsets match after
smoke cleanup. A single owned mobile context/admission row was removed only
after all three mobile test sessions were revoked and zero receipts verified;
the running worker had already reclaimed its sealed generation. No business
rows or operation receipts were deleted. All11new tables finish empty.

HTTP94 checks and browser20 checks passed on their first deployed attempt.
Mobile38requests (29mobile responses, all no-store) verified the seven-day lease,
context isolation and four-Person complete reconciliation/seal. All8temporary
sessions were revoked with original-cookie401 proof. Browser covers the existing
empty import workspace; populated migration and offline action durability keep
their separately attributed implementation evidence. Physical devices, poor
cellular and app distribution remain pending.

The first coordinator migration precheck used an incorrect backup JSON field and
stopped before migration; the corrected assertion checked result, catalog and
actual dump hash. Browser evidence notes the existing floating Operator launcher
covers some explanatory text at the captured390px scroll position, without
covering a control. No application source change was introduced during release.
