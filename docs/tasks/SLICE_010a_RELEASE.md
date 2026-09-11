# Slice 010a — Shared-development release

**DEPLOYED AND VERIFIED, 2026-09-11.** The user requested “Deploy 010a and
start 010b planning” and deferred authorized live FUB validation for a few days.
Target: the existing Mac-hosted API and production Web bundle at
[app.tarams.org](https://app.tarams.org/manage/migration), not the future
production cluster.

## Revision and procedure

Deployed main `0735015a583f6207c78d0141644a0753328f85f6`, containing implementation
`e4e0658` and merge `cd224fe`. All 25 code/config hashes still match the
[verified source](../design/qa/slice-010a-2026-09-10/final-code-tree.json).
The passing implementation gates remain applicable; no full test suite or
benchmark was repeated for deployment.

1. Verified the running 019b listeners: API 67757 on 3000, Web 68235 on 5173,
   with their expected repository working directories. Preserved the old API
   executable and Web bundle before building replacements.
2. Built `crm-api` and `migrate` with `cargo build --manifest-path
   backend/Cargo.toml -p crm-api --bin crm-api --bin migrate --locked`; built
   Web using Node 24.16.0 / pnpm 11.22.0, `pnpm run build --outDir` into a
   private staging directory. Both succeeded.
3. Backed up `crm_dev` using PostgreSQL 18.6 `pg_dump --format=custom --no-owner`
   as `crm_migrator`; the private archive is 109,806,826 bytes and its
   `pg_restore --list` catalog is readable. No restore exercise was performed.
4. Ran `./scripts/db-migrate`: applied `20260916000001`. The prior version was
   `20260915000001`. This adds five migration tables/indexes/grants; no business
   backfill, modification or deletion. Business counts before/after match.
5. Revalidated and stopped only the two former listener PIDs. Installed the
   staged Web bundle and launched `./scripts/dev-api` and `pnpm exec vite preview`
   in detached sessions using the existing configuration. No container reset,
   tunnel modification or unrelated service restart.

New API PID **9429**, Web listener PID **9455** (launcher 9443). Artifact hashes
and source revision are in [deployed.json](../design/qa/slice-010a-2026-09-11-release/deployed.json).

## Verification

- 21 HTTP/authentication/asset checks passed: local/public health and readiness,
  anonymous migration denial (401), admin empty migration summary (200), member
  denial (403), second-Organization admin empty summary (200), existing
  People/Today/custom-field/list reads, and exact served-index/asset comparisons.
  Temporary smoke sessions were revoked; no seed or business mutation was used.
- `./scripts/check-tunnel` passed app/API 200 and realtime WebSocket 101.
- Public production-build browser walkthrough passed: admin Migration navigation,
  empty key field, reload, desktop and 390px layouts, member redirect and hidden
  navigation. No browser runtime errors; screenshots were visually inspected.
- Applied schema verified; migration connections and assessments both remain
  zero. No FUB source request or real-account validation was made.
- Post-start observation confirmed the same expected listeners and zero
  WARN/ERROR entries in the API/Web logs. This is bounded release verification,
  not a recurring monitor.

[Sanitized evidence and screenshots](../design/qa/slice-010a-2026-09-11-release/README.md)
are retained with this record. An initial inspection command referenced a
nonexistent helper script; it made no changes. A preliminary presence check
used the wrong name `MIGRATOR_DATABASE_URL`; the actual
`MIGRATION_DATABASE_URL` was verified before the backup and migration.

## Configuration and deferred validation

`CRM_FUB_SYSTEM_NAME` and `CRM_FUB_SYSTEM_KEY` are both unset. This is a supported
disabled-source state: the API starts and the Migration page works, but the
reader fails closed before any network request. No configuration value or key
was invented. Configure both registered identification values and use an
authorized account API key when live validation resumes; record that result
separately in [the implementation verification](SLICE_010a_VERIFICATION.md).
The user's deferral remains in effect; no reminder or account operation was created.

## Recovery

Private release/rollback artifacts are retained at
`/private/tmp/crm-010a-release-_nhabszq`, including `crm-api-before`,
`web-dist-before`, `crm_dev-before.dump`, build/migration/runtime logs and exact
process records. The directory is mode 0700 and the database archive mode 0600;
these private files are not repository artifacts.

For application rollback, stop only the current release listeners, restore
the saved API executable and Web bundle, and launch them with the existing
root configuration. The five additive tables can remain unused by 019b;
do not drop captured data or restore the entire database as an application
rollback shortcut. No rollback was required. The backup catalog check is not
a claim that backup restoration has been tested.
