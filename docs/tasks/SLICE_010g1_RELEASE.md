# 010g1 / 010e6 — Shared-development release

**DEPLOYED AND VERIFIED — 2026-09-17.** Following merge and cleanup, the user
explicitly authorized shared-development deployment with “go ahead.” This targets
the existing Mac-hosted environment, not production or customer activation.

## Published source and runtime

- Source `74e63a8c36d7519815fc83228c12db79d0cc5f83` on published `main` includes
  010g1, its previously unpublished 010e6 prerequisite and the separate planning
  artifacts. PRs #1 and #2 are merged. Merged branches and the old worktree were
  removed; the main checkout is retained.
- API, admin CLI and migrator were built with locked dependencies, default
  features and the existing development profile; test-support is disabled.
  This is the established shared-development runtime profile, not a production
  Kubernetes build. Web uses a fresh Vite production build.
- Immutable release copies live under `/private/tmp/crm-010g1-release-20260917/`:
  binaries in `bin/`, Web in `web/`. API listens on `127.0.0.1:3000` and Web on
  `127.0.0.1:5173`. The owned native-test API3106 and its stores were preserved.
- API SHA-256: `6fa952aff2abd24c04c62801e623def7d3638f2d8c368624bb3a5cd08fc661ed`.
  Full binary hashes, source identity, process inventory and logs are in that
  private release directory. Ten older shared launch artifacts were made
  non-executable and recorded; recovery files were retained.

## Database and compatibility

- Before migration, the old shared API was stopped. A mode-0600 custom-format
  backup was created at `recovery/crm_dev-before.dump` (113,579,992 bytes).
  Its 2,219-entry catalog validated; no restore exercise is claimed.
- Applied 35 migrations: recovery `20261007000001` and family refresh
  `20261008000001` through `20261008000034`. All 111 migration records succeeded.
- All 217 pre-existing application tables retained exact row counts and data
  hashes over their original columns. No prior columns or migrations changed;
  26 tables were added. Post-smoke comparison also preserved all table data.
- The first new preflight correctly rejected the old schema with
  `admitted_metadata_schema_incomplete`; retained as `preflight-before-migration.json`.
  After migration, launch and confirmation preflight passed with all capabilities,
  including `people_recovery_confirmation_ready` and
  `family_refresh_confirmation_ready`. The live-process inventory passed again
  after startup. No gate was bypassed.
- Release reports intentionally expire. Regenerate a fresh observed inventory and
  report before later administrative confirmation; this deployment does not grant
  permanent confirmation authority. Use `runtime_release.py confirm` in the
  private release directory for the recorded environment, rechecking its inventory
  assumptions if processes or launch paths change.

## Verification and boundaries

- HTTP health/readiness and Web root returned 200. Family-refresh routes returned
  401 without authentication. Desktop/390px sign-in rendered with no page errors
  or horizontal overflow; both screenshots were visually inspected.
- Existing tunnel checks passed: app and API health 200, realtime WebSocket 101.
- Full synthetic acceptance, DB/API coverage, review and paired performance
  evidence are reused from [verification](SLICE_010g1_VERIFICATION.md); application
  sources are unchanged since those gates. No repeated measured benchmark.
- No live FUB requests, customer mutations, workspace activation, native-store
  changes, database reset or restore were performed. Production remains separate.
- Backup restore over newer writes is not authorized. Old binaries must not be
  restarted against newly confirmed family work; rollback needs compatible code
  and current retained-capability preflight.

Private evidence: `/private/tmp/crm-010g1-release-20260917/`, including build logs,
backup, schema/data fingerprints, manifests, preflight reports, runtime logs,
HTTP/browser/tunnel smoke and desktop/mobile screenshots. Keep recovery evidence
outside Git. Subsequent documentation commits do not change the deployed source.
