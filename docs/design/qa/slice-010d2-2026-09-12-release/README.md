# Slice 010d2 — Shared-development release evidence

See the [release record](../../../tasks/SLICE_010d2_RELEASE.md) for actual results,
scope and qualifications. Runtime/merged source is `5924f097a7d7475e7e9742bb08e102d7c7ca98d2`;
the closeout commit contains documentation and evidence only.

| Evidence | Meaning |
|---|---|
| [Integration](git-integration.json), [source](release-source.json), [cleanup](implementation-cleanup.json) | Committed/pushed source, 1,036 source hashes and verified removal of the owned worktree/branch. |
| [API build](build-api-result.json), [Web build](build-web-result.json), [backend hashes](build-sha256.json), [Web hashes](web-build-sha256.json) | Actual locked backend and production Web artifacts. |
| [Backup](backup-result.json), [migration](migration-result.json) | Protected custom dump/catalog and successful additive migration; no restore exercise. |
| [Before](database-before.json), [after migration](database-after-migration.json), [after smoke](database-after-smoke.json) | Exact prior 108 tenant rowsets/45 business counts plus the independently verified 100,077-Person read-model backfill. |
| [Deployment](deployed.json), [inventory](process-inventory.json), [supplement](inventory-supplement.json), [preflight](preflight-confirm.json), [configuration](configuration-check.json), [observation](observation.json) | Actual runtime identity, independent timeline capability and bounded observation. |
| [HTTP](http-smoke-2026-09-12T22-27-24.652915+00-00/http-results.json), [tunnel](tunnel-result.json) | Final 65 HTTP assertions and app/API/realtime routing. |
| [Browser](browser-smoke-2026-09-12T22-27-48.665Z/browser-results.json), [visual inspection](browser-smoke-2026-09-12T22-27-48.665Z/visual-inspection.json) | Successful desktop/390px flow, four screenshots, 110 matching asset responses and original-cookie cleanup proof. |
| [TLS attempt](http-smoke-2026-09-12T22-25-11.972901+00-00/attempt-qualification.json), [Accept-header attempt](http-smoke-2026-09-12T22-26-43.209254+00-00/attempt-qualification.json) | Original failed harness attempts with exact helper hashes/source; neither is relabeled passing. |
| [Sanitization](sanitization.json) | Explicit publication set and bounded known-credential scan. |

Private backups, configuration, cookies, profiles, runtime binaries, Web build
directories and live logs are not published. The helpers record how the release
was executed; they are historical task evidence, not a new deployment service.
`SHA256SUMS` covers this published directory except itself. Browser/HTTP children
also retain their original checksum files.
