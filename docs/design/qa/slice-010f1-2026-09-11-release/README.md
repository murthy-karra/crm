# Slice 010f1 shared-development release evidence

**Deployed and verified, 2026-09-11.** Implementation `f37ddd1`, merge and deployed
source `e36ce360`, pushed to main. See [the release record](../../../tasks/SLICE_010f1_RELEASE.md)
for the integration, executed commands, compatibility, recovery and limitations.
The target is the existing Mac-hosted shared-development application at
[app.tarams.org](https://app.tarams.org/manage/migration), database `crm_dev`.

| Check | Result / evidence |
|---|---|
| Git and cleanup | [Merged/pushed; 45 pending documentation files preserved; branch/worktree removed](git-integration.json), [private QA cleanup](implementation-cleanup.json) |
| Source identity | All 975 entries match the [verified implementation manifest](../slice-010f1-2026-09-11/checks/verified-source-sha256.json); no release code change |
| Backend / Web build | Passed: [locked Cargo build](build-api-result.json), [production Web build](build-web-result.json) |
| Runtime identity | [Deployed source and PIDs](deployed.json), [three backend hashes](build-sha256.json), [72 Web hashes](web-build-sha256.json) |
| Backup | [109,990,155-byte archive; 804-entry catalog verified](backup-result.json); restoration not exercised |
| Migration | [Passed](migration-result.json): `20260919000001`, including [13 metadata tables](workspace-schema-check.json) |
| Compatibility | Actual-DB [launch](preflight-launch.json) and [confirm](preflight-confirm.json) passed, including `metadata_confirmation_ready: true`; [inventory](process-inventory.json), [observations](inventory-observation.json), [newly retired old artifacts](retired-artifacts.json) |
| HTTP/auth/assets | [43 checks passed](smoke-results.json); temporary sessions revoked |
| Data preservation | [Before](database-before.json), [after migration](database-after-migration.json), [after browser](database-after-browser.json): 45 business counts unchanged, all 43 migration tables empty, three Organizations operational at revision 1 |
| Public browser | [Eight workflow checks passed](browser-results.json), refresh/reload and member denial included; no page errors or unexpected business/source requests |
| Tunnel | [Passed](tunnel.log): app/API 200, realtime 101 |
| Bounded observation | [72 seconds; expected listeners/hashes; zero WARN/ERROR entries](observation.json) |
| Independent release audit | [Passed](release-audit.json): actual processes, artifact/source/backup hashes, preflight inputs and reported verification outcomes; no new tests or mutations |
| Evidence identity | [Private and published hashes](artifact-manifest.json); exact configured credential scan passed |

| Screen | Desktop | 390px Web |
|---|---|---|
| Migration page | [View](migration-desktop.png) | [View](migration-390px.png) |
| Tags/custom fields import | [View](metadata-import-empty-desktop.png) | [View](metadata-import-empty-390px.png) |
| People import | [View](people-import-empty-desktop.png) | [View](people-import-empty-390px.png) |
| Core snapshot | [View](snapshot-empty-desktop.png) | [View](snapshot-empty-390px.png) |

All eight screenshots were [visually inspected](visual-inspection.json).
Responsive Web has no horizontal overflow; these are not native SwiftUI or
Compose screens. The existing fixed Operator launcher remains visible in the
captures. Cloudflare telemetry was blocked by the helper and is outside this proof.

The shared-development release checks use the existing empty migration state;
they create no source connection, snapshot, import, activation or customer data.
The complete metadata workflow, real foreign-resource isolation, review hold,
reconciliation, cancellation and storage recovery retain their separate
[synthetic implementation proof](../slice-010f1-2026-09-11/README.md).
Live FUB validation remains deferred.

The report's **2026-09-12 00:30:47 UTC** expiry is intentional. Confirmation needs
a fresh successful report from the current actual workload inventory. No
automatic renewal or recurring monitor was established. Older artifacts retain
their bytes with executable permission removed; review bindings and schema must
be preserved during compatible recovery. The backup/configuration/private helpers
and runtime logs remain in protected local storage, not this evidence directory.
