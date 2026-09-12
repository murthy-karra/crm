# Slice 010d1 — Shared-development release

**DEPLOYED / VERIFIED — 2026-09-12.** D-070's follow-up authorized commit,
merge, push, cleanup and deployment of historical capture/coverage to the
existing Mac-hosted shared-development API/Web. The final runtime source is
`eb1aa35307852e3746f7140318bab1790d680a64`; the implementation integration was `77287a0`. No live
FUB/customer processing, native timeline import, activation or production-cluster
deployment was performed.

The release starts from main `028d6133e1b7c3f81642275e030f63e98b2cca49` and the
verified `codex/slice-010d1-history-capture` worktree. All 43 pending main audit/
planning files were preserved privately. The previous three backend binaries,
73 Web files and configuration were preserved before building any replacement.
Recovery and release helpers/evidence are protected in
`/private/tmp/crm-010d1-release-6kg5mx3s`; credentials and backups stay outside Git.

[Implementation verification](SLICE_010d1_VERIFICATION.md) records the passing
SQLx/full/DB gates, both reviews, 75,000-observation collector and synthetic
production-Web walkthrough. Those test results retain their original source
manifests. This release separately verified the final source and actual builds,
database preservation, capability preflight, HTTP/browser behavior, worktree
cleanup and pushed revisions. Initial smoke exposed a real missing `no-store`
policy on a history member-denial response emitted by the outer workspace guard.
The release includes the forward correction and its focused regression check;
the original implementation gates are not relabeled as runs against that later
source. Initial failed smoke/browser attempts remain evidence alongside the
successful final checks. The implementation worktree and merged branch were
removed; main is the sole current worktree.

Follow the [compatibility runbook](SLICE_010c_RELEASE_PREPARATION.md) and
[history contract](SLICE_010d1_CONTRACT.md). Recovery must preserve schema,
identities, encrypted evidence and workspace/People-parent bindings. Prefer a
verified history-capable forward recovery; the backup and retired binaries do
not authorize a whole-database rollback or erasing bindings to run old code.


## Actual release results

| Step | Recorded result |
|---|---|
| Git integration | Implementation `45260403caf8cd76fb94e1bd940e3cfdeb42f093`; merge `77287a01599097b79cb9e5d99237820a852a8c81`; header correction/runtime `eb1aa35307852e3746f7140318bab1790d680a64`. Main was pushed after integration and correction. The release closeout changes only documents/evidence. |
| Final source | 1,037 source hashes verified. Exact release manifest SHA-256 `9c6673e9925530520bed13eea4a4df48e0414c6008bf28074b024984d5d3d08a`. Its only two changes from the original gate are the workspace response guard and its regression test; the original compact source fingerprint remains `2af03113b4e654ba4490c2b0bf40c46b1d3e883e005fb26e1b31048911a8f590`. |
| Builds | Locked API/admin/migrator build from final commit passed in 3.786s; initial merged build passed in 33.209s. Production Web build passed in 1.187s and remains from merge `77287a0`: all 73 file hashes matched after API replacement. The existing 531.04-kB LiveKit chunk advisory remains a build warning. |
| Correction checks | New full-app test first reproduced missing `no-store` on member 403. After the guard correction, 28 focused history/activity tests passed (90.325s execution, 112.556s wrapper); 48 response cases cover both workspace modes and 200/400/401/403/404. Scoped rustfmt and Clippy with warnings denied passed. No persistence contract or schema change was needed for the fix. |
| Backup/migration | Custom `crm_dev` backup: 110,141,898 bytes, SHA-256 `ec721dedcca14f9938ab93077b75f7963a0fdce9c12747429ca8cc9a7c91bc4e`; 1,070-entry catalog validated, restore not exercised. `./scripts/db-migrate` passed (0.495s), applying `20260921000001` additively. |
| Data preservation | Read-only repeatable-read snapshots before migration, after migration, after HTTP and after browser checks compared all 99 pre-existing tenant-owned rowsets, 45 business counts, three operational workspace revisions and admissions. All matched. The nine new history tables are empty; all 65 migration tables and admissions are empty. Session rows are excluded from tenant-rowset hashes and are checked through explicit session cleanup. |
| Launch/readiness | Final API PID 27920/port 3000; Web PID 24129/port 5173. Fresh actual workload inventory and DB preflight passed ordinary, metadata, activity and history capability gates before/after launch and during closeout. Three ordinary backend hashes are recorded below; no unrecognized executable launch path remained in the scoped inventory. |
| Public HTTP | 58 health/auth/asset checks passed, including anonymous 401, member 403, admin/other-Org empty history reads, unknown 404 and the corrected `no-store` error policy. Three HTTP sessions revoked. Public app/API/realtime routing passed 200/200/101 via `./scripts/check-tunnel` (3.221s). |
| Public browser | Final run `browser-smoke-2026-09-12T19-07-57.531Z` passed on final API source. Admin empty-parent state, disabled Prepare, refresh/reload and member navigation/route/API denial passed. Desktop 1360 and 390px checks passed; four screenshots retained. All 110 observed asset-response hashes matched. Both sessions were revoked with original-cookie 401 proof and both owned profiles removed. No unexpected HTTP, console, request or runtime errors remained. |
| Bounded observation | 354.42s UTC observation verified stable listeners, 1,037 source/three backend/73 Web hashes and zero API/Web WARN/ERROR entries. Final reconciliation completed at 2026-09-12T19:09:00.385743+00:00. This is bounded release evidence, not an uptime monitor. |
| Cleanup | Implementation worktree/branch and ignored caches removed; main is the sole worktree. All 43 prior pending main documents were preserved before integration; source and original QA checksums were verified before removal. The owned failed SQLx regression DB/registry row and all browser profiles are gone. Three obsolete test-build executable paths were disabled during the final replacement. Prior release recovery directories and protected backups remain. |

The actual final backend SHA-256 values are:

| Artifact | SHA-256 |
|---|---|
| `crm-api` | `7890b6d978c1025cfa7243a1332a5e3718c9ba15a1edb4ea461f8ea664f49b62` |
| `crm-admin` | `2a95fb4a62e170b7e60cb14a686d2e100d767f6294926779675aa9216f2dcbf6` |
| `migrate` | `eab83a4b2e12c179a1d959b322b591bce09b3d226961a4b0a955b438a7b15998` |

[Sanitized evidence](../design/qa/slice-010d1-2026-09-12-release/README.md)
contains actual command/result records, source deltas, inventories, DB rowset
hashes, screenshots and all failed/successful browser attempts. Live logs,
configuration, credentials, cookies and recovery dumps stay outside Git.

## Qualifications and continuing boundaries

- The first HTTP attempt failed on the real header defect and is retained.
  The first browser attempt completed its functional flow but failed its final
  assertion on logout abort annotations. The corrected helper accepts those only
  with the same-request 204, completed UI logout and original-cookie revocation
  proof; it preserves the abort events.
- The second browser attempt failed when a reload waiter accepted an import
  request started before reload. Retained request IDs prove that boundary error;
  the exact discarded response-body exception was not captured. The final helper
  fences request-start IDs and document generations and preserves original error
  phase/class/hash. Earlier attempts remain failed. Every session actually
  created across all three runs was revoked; the incomplete second run created
  only Alice's session and does not claim both actors completed.
- The red regression's private wrapper hit a metadata-writing NameError after
  nextest finished. Its unaltered failing nextest output and explicit qualification
  are preserved. SQLx's approved test harness creates isolated databases but also
  maintains its test registry/sequence in the master `_sqlx_test` schema. The
  regression's owned failed database/registry entry was removed; unrelated prior
  test state was preserved. The unchanged-data claim above covers public tenant
  rowsets/business counts, not that internal test sequence or session history.
- Original full gates (949 Rust, 1,131 Web, 934 DB and supporting suites), reviews
  and the 75,000-observation collector remain evidence for their original source.
  The release correction has the separate focused checks listed above. No full
  suite rerun against the correction is claimed. Original captured whitespace
  was retained byte-for-byte; the non-artifact source/document whitespace check
  passed. The full raw-artifact whitespace check has the retained findings.
- Protected configuration changed only the compatibility-report path to
  `/private/tmp/crm-010d1-release-6kg5mx3s/release-report.json`. Registered FUB system
  settings remain unset; no live source/customer operation, source connection,
  history proposal, import or activation was created. Current report evidence
  expires **2026-09-12T19:14:35Z**. Closeout renewal used a fresh actual
  inventory and DB preflight; no automatic renewal or recurring monitor exists.
  Expiry leaves ordinary CRM/retained reads available and confirmation unavailable.
- Public text pagination remains provisional and live-unqualified. Backup restore,
  customer-data readiness, D-015 erasure handling, email/media, native timeline
  semantics/010d2, deltas/cutover and production-cluster deployment remain outside
  this release. Existing bounded SQLx cancellation, audited call-history ordering
  and inherited core-worker handoff follow-ups retain their qualifications.
