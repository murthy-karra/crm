# 010e5 — Shared-development release

**DEPLOYED AND VERIFIED — 2026-09-15.** User authorized the pending push/deploy
and next recovery planning with “Ok go for it.” This release targets the existing
Mac-hosted shared development environment, not production Kubernetes.

## Published source and runtime

- `main` pushed at `df922ee`; implementation is `05abffa`.
- API, CLI and migrator built with locked dependencies from that exact source;
  copied into `/private/tmp/crm-010e5-release-20260915/bin/` so later verification
  builds cannot replace running artifacts.
- API on `127.0.0.1:3000`, Web production build on `127.0.0.1:5173`.
- API SHA-256: `0d988b5f78490c9539d4ca374c20810636995206a09d51cee318e07a17e43c21`.
- Both shared services were stopped at initial inspection. Separate native test
  service on port 3106 and installed native stores were preserved.
- Existing 14 migration worker tasks and one-second idle polling are unchanged.

## Database, backup and gate

- Mode-0600 custom-format backup saved before migration at
  `/private/tmp/crm-010e5-release-20260915/crm_dev-before.dump` (108 MiB).
  No restore exercise is claimed by this release.
- `20261006000001_fub_people_mapping_repair` applied to `crm_dev`.
- All 206 pre-existing public tables retained their row counts before API startup;
  11 new tables were added. No reset or restore occurred.
- Fresh launch and confirmation preflight passed with
  `mapping_repair_confirmation_ready=true`, all required current capabilities,
  and inventoried launch/admin/migrator paths. Reports remain outside Git.
- First preflight attempt rejected the inventory timestamp's `+00:00` form;
  inventory was regenerated using the required UTC `Z` representation and passed.
  No guard was changed. Release evidence is short-lived by design; regenerate
  current inventory/report for a later administrative confirmation.

## Smoke evidence

Health and internal readiness returned 200; both repair-mappings routes returned
401 without authentication. Web root returned 200 and the real browser displayed
the released Elysium sign-in form. This release smoke performed no customer
mutation or live FUB call. Full synthetic feature acceptance is in
[verification](SLICE_010e5_VERIFICATION.md), reused because source hashes match.

Build, migration, manifests, row counts, API/Web logs and smoke output are under
`/private/tmp/crm-010e5-release-20260915/` (build log also at
`/private/tmp/crm-010e5-release-build.log`). Database rollback after repair use
requires compatible binaries; this release does not authorize restoring a backup
over newer writes. Production, activation and live-source processing remain separate.
