# Slice 010d1 — Shared-development release evidence

The release is deployed and verified; see the [release record](../../../tasks/SLICE_010d1_RELEASE.md)
for scope, runtime hashes, executed commands and qualifications. Implementation
`45260403` was merged as `77287a01`; the final API includes the denial-header
correction `eb1aa353`. Web assets remain from the original merge. The closing
Git commit contains documents/evidence only.

## Evidence map

| Evidence | Meaning |
|---|---|
| [Git integration](git-integration.json), [source delta](release-source-delta.json), [source manifest](release-source.json) | Original verified source and the exact two-file release correction; final manifest hashes 1,037 source files. |
| [Final build](build-api-result.json), [backend hashes](build-sha256.json), [Web build](build-web-result.json), [Web hashes](web-build-sha256.json) | Actual locked builds and deployed artifacts; final Web retains all 73 original files. |
| [Regression summary](history-no-store-fix-summary.json) | Failing full-app reproduction, 28 passing focused tests, 48 response assertions, scoped formatting/lint and owned SQLx cleanup. Earlier wrapper metadata failure is qualified. |
| [Backup](backup-result.json), [migration](migration-result.json) | Backup byte/hash/catalog evidence and actual additive migration. No restore exercise is claimed. |
| [Before](database-before.json), [after migration](database-after-migration.json), [after HTTP](database-after-http.json), [after browser](database-after-browser.json) | Complete prior 99 tenant-rowset hashes and 45 business counts, not samples. Session rows and SQLx master test registry are outside this invariant. |
| [Launch report](preflight-launch.json), [post-launch report](post-launch-preflight-confirm.json), [closeout report](preflight-confirm.json), [inventory](process-inventory.json), [supplement](inventory-supplement.json) | Fresh actual DB/workload checks for ordinary and metadata/activity/history capabilities. The final report expires 2026-09-12T19:14:35Z. |
| [Deployment](deployed.json), [configuration](configuration-check.json), [observation](observation.json) | Actual process/artifact identity, protected configuration boundary, 354.42s bounded observation, stable data and zero API/Web warnings/errors. |
| [HTTP](smoke-results.json), [tunnel](tunnel-result.json) | 58 successful HTTP/auth/asset checks and public app/API/realtime 200/200/101. Three temporary HTTP sessions revoked. |
| [Final browser](browser-smoke-2026-09-12T19-07-57.531Z/browser-results.json), [visual inspection](visual-inspection-final.json) | Passing final public browser flow, 110 observed matching asset responses, desktop/390px views, both sessions revoked and profiles removed. All four screenshots exactly match the independently inspected first-attempt images. |
| [Cleanup](implementation-cleanup.json), [retired executable paths](retired-artifacts.json), [sanitization](sanitization.json) | Owned worktree/branch/cache removal, preserved audit/recovery evidence and explicit publication/credential checks. |

## Failed attempts are retained

The [initial HTTP attempt](initial-deployment/smoke-results.json) found the real
missing history no-store header on an outer member-denial response. The
[initial browser attempt](browser-smoke-2026-09-12T18-45-58.150Z/browser-results.json)
completed its functional flow but failed the logout-abort assertion. The
[second browser attempt](browser-smoke-2026-09-12T19-03-12.357Z/browser-results.json)
failed when a reload waiter accepted a pre-reload request. Neither is relabeled
passing. [Diagnosis](browser-logout-diagnosis.json) preserves the original helper
and result bytes/hashes, request evidence and the narrowly corrected checks.

The final helper records request IDs and document generations. A logout abort
is accepted only for the same request's 204 response, successful UI logout and
end-of-attempt cookie revocation proof. Explicit cleanup DELETE was followed by
GET/me with the exact original cookie, returning 401. This proves revocation at
cleanup completion, not which of the two DELETEs first revoked it. Every session
actually created in all three attempts was revoked; the incomplete second
attempt created only one and does not claim both actors completed.

## Scope and integrity

All source data shown here is metadata or the empty shared-development migration
workspace. The public browser did not create a connection, proposal, capture or
import. Live FUB qualification/customer processing, native timeline interpretation,
activation and production-cluster deployment remain outside this release.

The protected backup/configuration/cookies and live process logs remain outside
Git. Runtime logs are represented by hashes and diagnostic counts in observation.
The explicit publication allowlist was scanned against 16 known credential values
and 32 raw/encoded forms, with zero matches. This is a bounded check, not a general
secret-discovery claim. Historical raw whitespace is preserved unchanged.

`SHA256SUMS` covers every published file except itself. Browser subdirectories
also retain their original checksums. The initial deployment JSON checksum map
covers the preserved original metadata files; original runtime binaries remain
non-executable in protected recovery storage and are not published.
