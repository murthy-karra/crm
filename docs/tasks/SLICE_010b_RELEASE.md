# Slice 010b — Shared-development release

**DEPLOYED AND VERIFIED, 2026-09-11.** The user requested “commit, merge, deploy
and cleanup” after the verified implementation handoff (D-063 follow-up).
Target: the existing Mac-hosted API and production Web build at
[app.tarams.org](https://app.tarams.org/manage/migration). Live authorized FUB
validation remains deferred; no production-cluster or source-account operation
was performed.

## Source and deployment

Implementation `b71a854` was committed, merged without conflicts as main
`89471f0b7bccc3e91c5f6c94aacecf7e7bd2216d`, and pushed to origin/main. The merge
tree equals the implementation tree. All 37 implementation/configuration/contract
hashes still match the [verified source](../design/qa/slice-010b-2026-09-11/source-sha256.json).
The prior full gates remain applicable: SQLx preparation, 860 Rust tests,
5 doctests, 933 Web tests, 11 email-worker tests and 809 DB tests. No full test
or benchmark rerun was needed for this release. Subsequent commits contain
release evidence and current-state documentation only.

1. Inspected the existing 010a API PID 9429 and Web listener PID 9455 (launcher
   9443), including working directories and the prior executable hash. Preserved
   the current executable and Web bundle before building replacements.
2. Built the production-shaped `crm-api` and `migrate` binaries with
   `cargo build --manifest-path backend/Cargo.toml -p crm-api --bin crm-api
   --bin migrate --locked` (24.91s). Built Web with pinned Node 24.16.0 /
   pnpm 11.22.0 using `pnpm run build --outDir` into private staging (1.25s).
   Both passed; the existing Vite large-chunk warning remains.
3. Backed up `crm_dev` using `pg_dump --format=custom --no-owner` as
   `crm_migrator`. The archive is 109,828,221 bytes; `pg_restore --list` read its
   catalog. **No restore exercise was performed.**
4. Ran `./scripts/db-migrate`, advancing `20260916000001` to `20260917000001`.
   The migration adds 12 tables, 15 indexes and grants. It does not alter
   existing tables or backfill, modify or delete business records.
5. Revalidated and stopped only the old API/Web listeners, installed the staged
   Web bundle, and launched `./scripts/dev-api` and `pnpm exec vite preview`
   using the existing root configuration. Old 010a workers were replaced before
   any 010b work could be enabled. No service-container or tunnel reset occurred.

New API PID **49244**, Web listener **49270** (launcher **49245**), started at
07:43 PDT. Exact source/artifact hashes are in
[deployed.json](../design/qa/slice-010b-2026-09-11-release/deployed.json) and
[the build inventory](../design/qa/slice-010b-2026-09-11-release/build-sha256.json).

## Release verification

- **29 HTTP/auth/asset checks passed:** local/public health and readiness,
  anonymous assessment/snapshot denial, admin and second-Organization admin
  empty reads with `no-store`, member denial, unknown-resource denial, existing
  People/Today/custom-field/list reads, and exact served-index/asset comparisons.
  Temporary sessions were revoked.
- `./scripts/check-tunnel` passed app/API HTTP 200 and realtime WebSocket 101.
- Public Chrome verified admin navigation, empty credential/snapshot states,
  disabled preparation without a connection, explicit snapshot refresh, retained
  010a assessment UI, reload, desktop/390px layout and member navigation/route
  denial. Zero page errors or source/business mutation requests were observed.
  All four screenshots were visually inspected; document/body widths matched
  1360px and 390px viewports. Narrow screenshots exercise Web, not native clients.
- All **45 business-table counts remained unchanged**. All migration tables
  remained empty, including the 12 newly installed snapshot tables. No seed,
  reset, FUB connection, assessment, snapshot proposal or capture was created.
- A bounded observation at 07:47 PDT confirmed the same expected listeners and
  zero API/Web `WARN`/`ERROR` entries. This does not establish recurring monitoring.

Two smoke-helper corrections were necessary. The system Python TLS stack could
not negotiate the public endpoint; the HTTP helper used the existing verified
curl transport instead, without weakening TLS verification. The browser helper
initially classified blocked Cloudflare telemetry and the existing read-only
realtime-token POST as business mutations. It now distinguishes telemetry and
allows that exact read-only endpoint; the corrected walkthrough passed. No
application change resulted from either helper issue. The browser evidence
therefore excludes telemetry delivery validation.

## Configuration and deferred source validation

`CRM_FUB_SYSTEM_NAME` and `CRM_FUB_SYSTEM_KEY` remain unset. The production reader
fails closed before source network requests. Existing shared-development defaults
apply: `CRM_FUB_SNAPSHOT_RUN_CEILING_BYTES` is 2 GiB and
`CRM_FUB_SNAPSHOT_ORG_CEILING_BYTES` is 4 GiB. These are logical retained-payload
allowances, not allocated PostgreSQL disk or production customer quotas. No
credential or configuration value was invented or changed.

When the user resumes authorized FUB validation, configure registered system
identification and use the agreed authorized dataset. Record live results
separately. D-015/O-012/O-013 customer-data prerequisites and the qualification
gaps in [the source profile](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md) remain.
Business import, further capture families and cutover still need their own specs.

## Cleanup and recovery

Removed the merged `codex/slice-010b-core-snapshot` branch and its worktree,
including ignored build/dependency copies and copied `.env`. Confirmed temporary
QA ports 3011/5181 were closed, dropped only the task-owned `crm_slice010b_qa`
database, and removed its private runtime directory and synthetic keys.
Shared-development data and other services were preserved.

Release/rollback artifacts remain in mode-0700 directory
`/private/tmp/crm-010b-release-g33z5awg`: `crm-api-before`, `web-dist-before`,
`web-dist-retired`, mode-0600 `crm_dev-before.dump`, build/migration/runtime logs
and process records. Implementation gate logs and synthetic helper source were
also copied into its `implementation-private` directory. These are private local
artifacts, not a production backup or retention policy.

**010c compatibility amendment (D-065):** the following historical rollback
procedure applies only while no durable migration review workspace exists.
Once an Organization has a `migration_workspace` binding, pre-010c API, worker
and CLI artifacts cannot enforce its hold and must not run against that database.
Recovery requires a verified compatible artifact and the 010c release preflight;
do not delete a binding, reset its mode or restore the whole database to permit
an older application. Before the first binding, retire every pre-gate process.
See [the approved compatibility boundary](../specs/SLICE_010c_CONTRACT.md#compatibility-checkpoint).

For application rollback within the historical 010b boundary, inspect current listener identity, stop only the
current release API/Web processes, restore the saved executable/Web bundle, and
launch them with the existing root configuration. Leave the additive 010b tables
intact; do not drop captured evidence or restore the entire database as an
application rollback shortcut. Returning to old 010a workers disables 010b and
must not overlap with active 010b workers. No rollback was required.

[Sanitized release evidence and screenshots](../design/qa/slice-010b-2026-09-11-release/README.md)
are committed alongside this record.
