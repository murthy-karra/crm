# Mobile 002 / 010e2 — Shared-development release

**DEPLOYED / VERIFIED — 2026-09-13.** The user's “commit, merge to main, push,
cleanup and deploy” follow-up to D-076 authorized this release to the existing
Mac-hosted shared-development environment at
[app.tarams.org](https://app.tarams.org/manage/migration).
Runtime source is `9d04755e405848cb06a8e4c135793d9994176648`, including verified
implementation `c883a3c`. Closeout commits change documentation/evidence only.
Both native source trees are published; signing/distribution and physical-phone/
cellular testing remain separate. The existing native-demo API and stores remain intact.

The [implementation record](MOBILE_002_010e2_IMPLEMENTATION_STATUS.md) owns the
975 Rust, five doctest, 1,203 Web, 11 email-worker and 990 DB case coverage,
SQLx/Clippy, native upgrade/offline/conflict and populated migration proof.
Release reused that evidence; no broad suite or implementation review was repeated.
No application-source correction was needed during release.

## Executed release

| Step | Actual result |
|---|---|
| Git | Fetched origin, scanned 111 changed publication files against configured credentials with zero matches, committed release authorization, fast-forwarded main and pushed `9d04755`. Removed the integration branch; only main's worktree remains. |
| Builds | Locked API/admin/migrator build passed in 48.47s; production Web in 1.899s. Explicit isolated Cargo/Vite outputs. All 1,083 source files and 77 existing runtime/Web artifacts remained unchanged during build. Three binary hashes and 73 Web hashes retained. |
| Backup | Custom crm_dev backup, 112,945,044 bytes; SHA256 `d8f02162c1d0fb1a47f185c705dda5a05375b4daf9f003b8a6aa27a57dd54c84`. Catalog validated 1,466 entries; restore not exercised. |
| Schema | Verified migrator applied `20260925000001` and `20260926000001` once. All 43 applied SHA384 checksums match committed SQL. No reset or destructive rollback. |
| Preservation | All 134 prior tenant rowsets preserve every old column; note.revision is separately verified as 1. All 45 business counts, workspace bindings/admissions and 100,077 history-review rows are unchanged. After smoke cleanup, all 142 complete upgraded rowsets match; all eight new refresh tables remain empty. |
| Runtime/configuration | API PID49067/port3000, Web PID49082/port5173. Exact verified binaries launched directly. Only the release-report path changed; the entire mobile receipt key ring is unchanged. Native demo API PID71121/port3101 remains on crm_mobile_001. |
| Compatibility | Actual running API/in-process workers, admin/migrator launchers, containers and scheduled paths inventoried. Workspace readiness and all six migration feature capabilities, including fub-people-refresh-v1, passed. Private demo exclusion is backed by its actual process environment/database target. |
| HTTP/assets/tunnel | 124 HTTP checks passed, including ordinary reads, refresh/history/report admin/member/unknown-resource boundaries and closed no-store denials. Public entry/Migration assets match. All 73 locally served Web files match the build manifest. `scripts/check-tunnel` passed app/API/realtime 200/200/101. |
| Browser | Actual desktop and 390×844 screenshots inspected, new refresh panel and guarded empty state, reload and ordinary Today verified. Narrow refresh panel clientWidth=scrollWidth=277. No console warnings/errors. Existing authenticated session retained; temporary viewport reset and created tab closed. No Operator action submitted. |
| Mobile | 43 public HTTP requests. New edit_note/update_task/note_revisions capabilities, exact 604,800-second lease, same installation/context, current-task fields/revision and actor/Org/context denials passed. Four People downloaded through one manifest and 12 component pages, then sealed; reads did not renew authorization. No operation POST or business write. |
| Cleanup/observation | All six temporary sessions revoked with original-cookie 401 proof. Only the owned mobile context/admission removed after zero receipts and identity checks; the worker had already reclaimed the sealed generation. Runtime/artifacts/source stable across 263.434s, zero API errors; 12 WARNs match deliberate mobile denial checks. Isolated build cache removed; protected backup, configuration, evidence and recovery binaries retained. |

## Limits and recovery

The deployed browser check covers an empty import workspace; populated refresh
execution, conflicts, recovery, accounting and the 25k query-plan evidence remain
attributed to implementation. The selected mobile workspace has no notes, so this
release directly verified current-task reads; positive current-note and note-edit
proof remains in the backend/native implementation evidence. Native apps were not
redistributed or installed over the user's demo stores.

Rust build emitted no warnings. Vite retained its existing large-chunk advisory
and external-output-directory warning. SQLx's existing potentially-unused-query
warning belongs to implementation validation, with no metadata mismatch. Source,
configuration, deployment, browser and data-preservation checks passed without a
release-source fix. Sessions and SQLx's registry are outside tenant-rowset claims.

Actual-inventory confirmation evidence expires at **2026-09-13T07:50:46Z**.
There is no automatic renewal. Ordinary CRM and retained reads continue after
expiry; a later import/refresh confirmation requires a fresh operator preflight.
No live FUB/customer processing, workspace activation or production-cluster
release occurred.

Protected backup, old configuration/artifacts, exact new forward-recovery
binaries, private helpers and logs are retained at
`/private/tmp/crm-mobile010e2-release-enuz4h9k`. Preserve the receipt key ring and
migration evidence for retries; do not reset the database or remove workspace
bindings to run an older artifact.

[Sanitized release evidence](../design/qa/mobile002-010e2-2026-09-13-release/README.md)
contains exact binary/schema/source hashes, preservation and executed check results.
