# Mobile 001 / 010e1 — Shared-development release

**DEPLOYED / VERIFIED — 2026-09-12.** The user's “yes release completed milestone
first” follow-up to D-075 authorized the completed milestone's main integration,
Git publication, owned cleanup and backend/Web release to the existing Mac-hosted
shared-development environment at [app.tarams.org](https://app.tarams.org/manage/migration).
Runtime source is `08cb42057013cb8766ae61acb23458b0cf38c416`; implementation
checkpoint `8819c6c` and release authorization `ed4c83f` are included. Closeout
commits change documentation/evidence only. Native source is published; app
signing/distribution and physical-device/cellular testing remain separate.

Implementation evidence was reused: [combined gates](MOBILE_MIGRATION_COMBINED_VERIFICATION.md),
[lifecycle correction](MOBILE_001_SEALED_GENERATION_VERIFICATION.md),
[010e1](SLICE_010e1_VERIFICATION.md), [iOS](MOBILE_001_IOS_VERIFICATION.md) and
[Android](MOBILE_001_ANDROID_VERIFICATION.md). No broad suite or implementation
review was repeated during release, and no application source fix was required.

## Executed release

| Step | Actual result |
|---|---|
| Git/preservation | Main was unchanged before integration. Scanned 196 changed publication files against known configured credentials with zero matches. Preserved old configuration, three exact released 010d2 binaries and 73 Web assets. Merged/pushed08cb420; removed the integration branch. Only main's worktree remains. |
| Builds | Explicit isolated Cargo target and Vite output. Locked API/admin/migrator build 41.229s; production Web 1.463s. All 1,062 source files and 77 existing runtime/Web artifacts remained unchanged during build. Three ARM64 executable hashes and 73 Web hashes captured. |
| Backup | Custom crm_dev backup 112,887,138 bytes; SHA256 `e7777032a206f864ec04a27d2a2587df0015552837c447e5b3126202e1487db5`. Catalog validated 1,359 entries; restore not exercised. |
| Migration | Verified migrator ran once, exit 0 in 0.292s. Applied `20260923000001`, `20260923000002`, `20260923000003` and `20260924000001`; all 41 applied SHA384 checksums match committed SQL. No reset or destructive rollback. |
| Preservation | All 123 prior tenant rowsets preserve every old column; Person.mobile_revision and task.revision are separately verified as 1. All 45 business counts, three operational workspace bindings and admissions are preserved. All 134 complete upgraded tenant rowsets match after smoke cleanup; all 11 new tables finish empty. Existing review-state 100,077 rows remain unchanged. |
| Configuration/runtime | Distinct persistent 32-byte mobile receipt HMAC key configured; only that ring and the operator report path changed. API PID 81656/port 3000, Web PID 81677/port 5173. Exact binaries launched directly. Native synthetic API PID 71121/port 3101 remains on its separately verified crm_mobile_001 database. Retired 26 obsolete shared executable launch paths without deleting recovery bytes. |
| Compatibility | Actual workload, launcher, container and scheduled-path inventory; ordinary workspace readiness plus all five feature capabilities, including fub-core-change-v1, passed. The private native API was explicitly excluded only after its actual process environment proved the different database target. |
| HTTP/tunnel | 94 HTTP checks passed on first deployed attempt; ordinary People/Today/source reads, migration admin/member/foreign-resource boundaries, closed no-store denials and asset identity. Tunnel app/API/realtime routing 200/200/101 passed. All 73 Web assets matched local served bytes; browser verified 110 public asset responses covering 29 unique assets. |
| Browser | 20 checks passed on first deployed attempt, desktop and 390px. Existing empty capture/report states, disabled unsafe controls, refresh/reload and Operator shell verified. Seven actual screenshots retained. No unexpected HTTP, console/runtime or request errors. No Operator turn or proposal was sent. |
| Mobile adapter | 38 public HTTP requests, including 29 no-store mobile responses. Same installation reused context; exact 604,800-second lease and actor/Organization/workspace binding verified. Four People downloaded through one manifest and 12 complete component pages, then sealed. Reads did not renew lease. Unknown receipt remained 404; anonymous, missing/random, other-actor and cross-Organization context denials passed. No operation POST or business write. |
| Cleanup/observation | All 8 temporary sessions revoked with original-cookie 401 proof. Removed only the owned mobile test context/admission after verifying zero receipts; scheduled cleanup had already reclaimed its sealed generation. Final full database preservation passed. Same listeners/artifacts/source across 422.643s; zero unexpected errors. Nine WARNs are attributed to deliberate mobile not-found/unauthenticated/unsupported-protocol checks. |

Backend SHA256:

| Artifact | SHA256 |
|---|---|
| crm-api | `36dca5961f2ad391d3119661594e631e57d142214cdc95af95b0d4b4dbddf0ef` |
| crm-admin | `fb3d7bdceeee72e534d2c428a865c1b1975be7afdc122511e32ccafd1ffd4e10` |
| migrate | `f0b3f508a6317593f2474d803bb45a75647cfdbe4f8ddfeb893664627bf65b8f` |

## Qualifications and recovery

- The first coordinator migration precheck expected a `status` backup-metadata
  field. It stopped before migration; the corrected check verified the actual
  `result`, validated catalog and dump hash. No application change or repeated
  migration was involved.
- Rust reported no build warnings. Vite retained the existing large-chunk advisory
  and warned about the intentionally external output directory. That new output
  directory was asserted empty. The actual pnpm runner was 11.19.0 for `pnpm exec`
  against installed dependencies; no dependency installation or lockfile change.
- The existing floating Operator launcher covers part of the bottom explanatory
  copy at the captured 390px report scroll position. No control is covered; this
  is recorded as a small layout follow-up, not an unreported passing layout claim.
- Shared browser proof covers an empty import workspace. Populated report/recovery,
  offline 100-action retries, device-storage failures and native UI proof remain
  attributed to implementation evidence. Sessions and SQLx's test registry are
  outside tenant-row preservation claims. Eight pure preservation-helper checks
  passed; actual database comparisons are separately captured.
- Fresh actual-inventory confirmation evidence expires at
  **2026-09-13T03:16:50Z** (2026-09-12 20:16:50 PDT). There is no automatic
  renewal. Ordinary CRM/retained reads remain available after expiry; future
  import confirmation needs a fresh operator preflight.
- Protected backup/configuration, verified forward-recovery binaries, private
  helpers and logs remain at
  `/var/folders/y9/r969pzyd1dx1fxgz8j7lw8280000gn/T/crm-mobile010e1-release-e9911g6r`.
  The isolated cloned Cargo cache was removed after preserving verified binaries,
  manifests and logs. The existing private native API and installed paused demos
  are retained.
  Never remove workspace bindings, receipts or evidence, or reset the database
  to make an older artifact run. Preserve the receipt key ring for retries.
- No live FUB/customer processing, delta application, activation, production
  cluster deployment or native app distribution occurred. Physical phones,
  passcode/device-key behavior and real poor cellular remain the next mobile gate.

[Sanitized release evidence](../design/qa/mobile001-010e1-2026-09-12-release/README.md)
contains executed results, exact source/artifact/schema hashes, preservation,
actual readiness, public/mobile checks, screenshots and cleanup attribution.
