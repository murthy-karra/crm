# Migration idle readiness — shared-development release

**DEPLOYED AND VERIFIED — 2026-09-18.** User authorized
commit, deployment and a repeat database activity sample in this task.

## Source and runtime

Implementation `8255ad9c3a2c988e18626b2f8409a620f10d6229` was committed on
`codex/migration-idle-readiness`, fast-forwarded into `main` and published.
The shared-development API/admin/migrator artifacts use that source, locked
dependencies and default features (no test-support), built in the isolated
`/private/tmp/crm-projection-target` and copied to
`/private/tmp/crm-idle-release-20260918/bin/`.

The unchanged Web remains the verified `8fac1dc` production build on port 5173.
New API listens on 3000; native-test API3106 is preserved. Configuration names
and existing secrets are unchanged except the API's `CRM_MIGRATION_RELEASE_REPORT`
now points into the new private release directory. The previous API was stopped
by its verified PID/path; no broad process termination or data reset occurred.

## Database, verification and recovery

No schema migration is needed: all 112 successful migration checksums match
source. The pre-restart backup is 128,379,426 bytes with a validated 2,732-entry
restore catalog; no restore exercise is claimed. After startup, all 244
application tables retained their exact counts and data hashes. No columns,
prior migration checksums or tables changed.

Launch/confirmation preflight and post-startup live inventory passed with all
required capabilities. Health/readiness and Web returned 200; unauthenticated
Today/reconciliation returned 401. Served Web index matches its preserved build.
Public app/API returned 200; realtime WebSocket upgraded with 101.

Reused code verification: 99 selected database cases, all 16 browser migration
steps (run `a518666f517d`, first attempt, cleanup verified), production compilation,
formatting and strict all-target Clippy. Source code is unchanged since those gates.
[Implementation verification](../reviews/migration-idle-readiness-2026-09-17.md)
records the controlled browser harness's scope and the direct scheduler tests.

Private release scripts, source/artifact hashes, observed processes, backup,
schema/data comparisons, smoke results and activity samples are under
`/private/tmp/crm-idle-release-20260918/`. Three previous launch artifacts were
made non-executable and retained. The previous projection-capable release is
schema-compatible recovery material; any rollback requires fresh process inventory
and release preflight. No rollback or database restore was needed.

## Activity comparison

Matched approximately 30-second samples: before 6,341 transactions / 30.17s
(**210.15/s**), after 514 / 30.15s (**17.05/s**): **91.89% lower**.
People-refresh scans fell from 255 to 30, admission from 255 to 29, and history
capture from 238 to 15. Workspace and history-anchor scans disappeared from the
positive-delta list. The remaining activity includes normal job-presence polling
and other existing background workers. The 180-second stability observation passed all seven health/readiness samples
with unchanged API/Web process IDs and no restarts. Final runtime-inventory
confirmation and HTTP smoke passed. These are whole-database observations with empty queues, not
HTTP request rates, exact SQL statement counts or a capacity benchmark. No
statistics reset or PostgreSQL restart was performed.

Release evidence expires; regenerate observed confirmation evidence for later
administrative work. Deployment does not activate workspaces or authorize live
FUB/customer actions. No ongoing monitor is installed.
