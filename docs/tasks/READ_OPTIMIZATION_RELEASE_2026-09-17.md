# Read optimization and migration follow-up — shared-development release

**DEPLOYED AND VERIFIED — 2026-09-17.** User explicitly
authorized merge, branch/worktree cleanup and deployment in this task.
Target: existing Mac-hosted shared development, not production.

## Source and artifacts

Published `main` and API/Web source: `8fac1dc1e878483ed44387086e5be80d83fc4693`.
Includes Person-detail projection, Today filter reference batching, migration
coverage/reconciliation, and the repeated-refresh head correction. Later release
record commits change documentation only.

Private artifacts and evidence: `/private/tmp/crm-read-optimization-release-20260918/`.
The directory uses the UTC date; local release date is September 17.
API/admin/migrator: locked default-feature development build, test-support disabled,
with isolated `CARGO_TARGET_DIR=/private/tmp/crm-projection-target`. Web: fresh Vite
production build in the release directory. Both builds passed; Web retained its
nonblocking chunk-size warning. Binary hashes and source manifest are recorded.

Exact commands and scoped process lifecycle are preserved in private
`deploy_ops.py`, `runtime_release.py`, `database_release.py` and logs. API uses
port 3000, Web 5173; the separate native-test API3106 was preserved.
`CRM_MIGRATION_RELEASE_REPORT` points to this release's report. Existing `.env`
configuration remains unchanged; no secrets are copied into this record.

## Database and recovery

The previous shared API was stopped before backup and migration. The custom-format
backup is `recovery/crm_dev-before.dump` (114,033,199 bytes); its restore catalog
validated. A restore exercise was not performed.

Applied only `20261008000035_person_detail_projection.sql`. All 112 migration
records succeeded and their SHA-384 checksums match source. The backfill produced
100,077 projection rows for 100,077 People, with zero missing rows. All 243
pre-existing application tables retained exact row counts and data hashes over
their original columns; no prior columns or migration checksums changed. Only
`person_detail_projection` was added. Post-smoke comparison also preserved all
244 application tables, including the projection.

Previous release artifacts and backup are retained. Three old launch binaries
were made non-executable and recorded. Do not restart the old API casually: old
write paths do not maintain the new projection. Recovery requires compatible
code and fresh capability preflight; any return to old code needs projection
rebuild before returning to this reader. Restoring a backup over newer writes
is a separate recovery decision, not an automatic rollback. No recovery was needed.

## Verification

- Launch and confirmation preflights passed, including a fresh live inventory
  after startup and all confirmation capabilities. An initial inventory attempt
  during the running migrator correctly rejected that active process; the gate
  passed after migration exited. No check was bypassed.
- Health/readiness and Web root returned 200. Today and reconciliation reads
  returned 401 without authentication. Served Web index matches the built artifact.
- Desktop and 390px sign-in browser checks passed with zero page errors or
  horizontal overflow; both screenshots were visually inspected.
- External app/API checks returned 200 and realtime WebSocket upgraded with 101.
- Full existing implementation evidence is reused: all ten E2E families/103
  steps passed against the deployed code in `.e2e/runs/0478945326d9`, plus the
  relevant DB, lint and performance gates in the linked verification records.
  No new authenticated customer-data smoke or live FUB action was performed.
- The 180-second observation passed seven health/readiness samples with stable
  API/Web process IDs and no restarts. Every served JavaScript artifact matched
  its release hash. Final live-inventory confirmation and HTTP smoke passed.

Reports expire: regenerate fresh observed inventory and confirmation evidence
before later administrative confirmation. This release does not permanently
approve migration confirmation, activate workspaces, distribute native builds,
or promise ongoing monitoring.

Implementation evidence: [Today](../reviews/today-read-path-2026-09-17.md),
[migration follow-up](../reviews/migration-completion-2026-09-17.md),
[Person detail](../reviews/person-detail-projection-2026-09-17.md).
