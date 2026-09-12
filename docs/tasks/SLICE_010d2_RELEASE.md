# Slice 010d2 — Shared-development release

**DEPLOYED / VERIFIED — 2026-09-12.** The user's explicit “commit, merge, push,
cleanup and deploy” follow-up to D-072 authorized this release to the existing
Mac-hosted shared-development API/Web at [app.tarams.org](https://app.tarams.org/manage/migration).
Runtime source is `5924f097a7d7475e7e9742bb08e102d7c7ca98d2`; implementation
`f6f74262e0cd019cb8fd49a114529aafea4f2aca` was merged and pushed to main.
The release closeout changes documentation/evidence only.

[Implementation verification](SLICE_010d2_VERIFICATION.md) retains the original
963 Rust, 953 DB and 1,164 Web results, supporting suites, bounded reviews,
populated synthetic walkthrough and query measurements. This release verified
the committed source and actual artifacts without repeating those full gates.
No live FUB/customer processing, readable body/media access, workspace activation
or production-cluster deployment was performed.

## Executed release

| Step | Actual result |
|---|---|
| Preservation and Git | Preserved all seven pending main planning files, prior configuration, three backend binaries and 73 Web files before replacement. Verified all 47 changed implementation/test hashes against the tested manifest. Committed, merged without conflicts and pushed `5924f09`. Removed the merged implementation branch/worktree and its ignored caches; main is the sole worktree. |
| Builds | Locked API/admin/migrator build passed in 38.685s; production Web build passed in 1.525s. All 1,036 source hashes, three backend hashes and 73 Web hashes matched after deployment. The existing LiveKit chunk-size advisory remains a build warning. |
| Backup | Custom `crm_dev` backup: 110,180,841 bytes, SHA-256 `6aa33380ff89a78853f80dbc2f4cdefd3359e6e4ea69226742e3e91e126d0675`. Validated the 1,135-entry catalog; restore was not exercised. |
| Migration | `./scripts/db-migrate` passed in 5.021s, applying `20260922000001` additively. No reset or destructive rollback. |
| Data | Before/after-migration/after-smoke repeatable-read snapshots matched all 108 prior tenant rowsets, 45 business counts, three operational workspace revisions and admissions. All 14 new import/fact tables remained empty. The new derived review state exactly matched the independently computed native backfill for 100,077 People; it is intentionally not empty. Existing 65 migration tables remained empty. |
| Runtime and readiness | API PID 95513/port 3000, Web PID 95531/port 5173. Actual workload/launch-path inventory and DB preflight passed ordinary, metadata, activity, capture and independent timeline capability gates. No unrecognized CRM runtime or obsolete executable launch path remained in the scoped inventory. |
| HTTP/tunnel | Final 65 health/auth/asset assertions passed, including closed no-store errors for anonymous/member/foreign-resource requests and empty authorized history-import reads. `./scripts/check-tunnel` passed app/API/realtime routing with 200/200/101. |
| Browser | First browser attempt passed admin/member flow, empty-parent history-import state, refresh/reload and desktop/390px views. All 110 observed asset responses matched. Four actual screenshots were inspected with no actionable layout finding. No unexpected HTTP, console, runtime or blocked activity remained. |
| Cleanup/observation | All eight confirmed temporary sessions across HTTP/browser attempts were revoked with original-cookie 401 proof; browser profiles were removed. Listeners, source and artifacts remained stable during 288.656s of bounded observation, with zero API/Web WARN/ERROR lines. Final database preservation passed after session cleanup. |

Actual backend SHA-256:

| Artifact | SHA-256 |
|---|---|
| `crm-api` | `ffeee8cb408e0df716892bc7cf3a1981b7e09a280f36566fcab9e59d6989690a` |
| `crm-admin` | `64e85ba5c2eb33709a5f0fe491aeffd11ce31ca4eeb1d333a81c03d12b8b69e1` |
| `migrate` | `cdfc28d856326833782b763358f17c7431abd32dbbbc9bbb99187fa42bca687b` |

## Failures, qualifications and recovery

- A wrapper initially expected `exit_code` in the backup metadata; that assertion
  failed before any migration ran. It was corrected to check the actual passed
  result, validated catalog and dump hash. The migration then passed once.
- HTTP attempt one passed 12 local checks but failed host Python TLS negotiation
  before any public response or session. Curl transport fixed the harness without
  disabling certificate verification. Attempt two passed 57 authorization/read
  checks and revoked all three sessions, then failed because it requested the
  Web root with `Accept: application/json`. The corrected Web Accept header
  produced the final 65-check pass. Both failed attempts and exact helper versions
  remain evidence; no application change was needed.
- The zero-session first HTTP attempt records `all_created_sessions_revoked: false`;
  its session inventory is empty. All eight sessions actually created in later
  attempts have explicit cleanup proof. Browser logout's two `ERR_ABORTED`
  annotations are qualified by the same request's 204, completed UI logout and
  independent original-cookie 401 proof; they are not silently discarded.
- Public browser proof covers the empty shared-development migration workspace.
  Populated import/cancel/resume/timeline behavior retains its separate synthetic
  implementation evidence. Session history and SQLx's internal test registry are
  outside the tenant-rowset preservation claim.
- Protected backup, old runtime, configuration and live logs remain in
  `/private/tmp/crm-010d2-release-q9q7lzwf`. Only the report-path configuration key
  changed; registered FUB system settings remain unset. Known-value scanning found
  no credential matches in the explicit publication set. This is not a general
  secret-discovery or backup-restore qualification.
- Fresh actual inventory/DB checks renewed the operator-owned report, valid until
  **2026-09-12T22:41:23Z**. There is no automatic renewal or unattended monitor.
  Expiry leaves ordinary CRM/retained reads available and confirmation unavailable.
- Follow the [compatibility/recovery runbook](SLICE_010c_RELEASE_PREPARATION.md)
  and [010d2 contract](SLICE_010d2_CONTRACT.md). Preserve schema, immutable anchors,
  identities and encrypted evidence; prefer a verified timeline-capable forward
  recovery. The backup does not authorize whole-database rollback or removing a
  binding to run an older artifact. Live qualification, erasure readiness,
  deltas/repair and activation remain separate work.

[Sanitized evidence](../design/qa/slice-010d2-2026-09-12-release/README.md) records
executed commands, artifact identities, database hashes, failed/successful smoke
attempts, screenshots and owned cleanup.
