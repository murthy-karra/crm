# Slice 010f1 — Shared-development release

**DEPLOYED AND VERIFIED, 2026-09-11.** D-066's follow-up authorized commit, merge,
push, cleanup and deployment. The existing Mac-hosted shared-development API and
production Web bundle at [app.tarams.org](https://app.tarams.org/manage/migration)
now serve source **`e36ce360a4956b76b6f8dc537a051e6cb8cbc7b7`** against `crm_dev`.
Implementation **`f37ddd1426189ff7c3c4a85c0f204b2b86605792`** is merged and pushed.
This replaces the prior 010c runtime from `f01c2e3`.
No live FUB connection/import, customer-data processing, activation or production
cluster operation occurred.

## Git integration and cleanup

All 45 pending main documentation files were preserved privately before merge:
36 were identical to the implementation worktree; nine had expected status and
implementation updates. The merged tree preserves all **975 verified source
files**. The source manifest hash is
`eb36727b7fc95221df4c36eb0ef1f2a04b9028dcda86a6c0ede3110e82e844ba`.
The push completed and the remote main ref matched `e36ce360`.

The clean merged `/Users/karrad/projects/crm-010f1` worktree and
`codex/slice-010f1-metadata-import` branch were removed, along with their ignored
build/dependency files. The temporary integration stash was dropped only after
preservation checks; main is the sole worktree. Ninety-six original private
verification logs/JSON/drivers were retained by hash in
`/private/tmp/crm-010f1-release-ozyr7lke/implementation-verification` before removing
the old private QA directory and its environment/control files. Dedicated QA
services and databases had already been removed during implementation cleanup.

Six published implementation logs had trailing whitespace/extra EOF blank lines
normalized before publication. Original and published hashes remain recorded;
no implementation byte or test result changed. Prior passing implementation
gates therefore remain applicable: 900 Rust tests, five doctests, 1,054 Web tests,
881 DB tests, 19 preflight and 11 email-worker tests, 90 query plans/246 checks,
51 browser checkpoints and exact native reconciliation. These gates were not
rerun during this release. See [implementation proof](SLICE_010f1_VERIFICATION.md).

## Executed deployment checks

1. Preserved old API/admin/migrator binaries, Web bundle and configuration in
   mode-0700 `/private/tmp/crm-010f1-release-ozyr7lke`. Built ordinary API,
   `crm-admin` and `migrate` artifacts from merged source using locked Cargo
   dependencies and the existing development profile (26.96s). Built staged
   production Web with pinned Node 24.16.0/pnpm 11.22.0 (1.20s). All three backend
   hashes and 72 Web file hashes are catalogued against the merged revision.
   Vite retains its existing large-chunk warning; the build completed successfully.
2. Recorded 45 business-table counts, empty migration state and three operational
   workspaces at revision 1. Took a custom-format `crm_dev` backup:
   **109,990,155 bytes**, SHA-256
   `35806f8323f187518988f1ec816e7af0e39f492db412d034899cbe198a855c62`.
   `pg_restore --list` verified the 804-entry catalog. **Restoration was not
   exercised.** `./scripts/db-migrate` passed in 0.35s, applying additive migration
   **`20260919000001`**, including all 13 metadata tables. Existing counts and
   workspace states remained unchanged.
3. Stopped only verified old API PID 60714 and Web PID 60731. Removed executable
   permission from three newly preserved old 010c binaries, retaining their bytes;
   earlier retired binaries remain non-executable. Inventoried actual processes,
   in-process workers, CLI/migrator launch paths, containers and launch files.
   No additional CRM worker, hashed/example runtime or scheduled launcher was
   found. The actual-DB release preflight passed for launch and confirmation,
   explicitly requiring **`metadata_confirmation_ready: true`** as well as
   ordinary readiness. API/worker artifacts declare `fub-metadata-import-v1`.
   A private adapter invokes the container's real `psql`; no database response
   was mocked, and credentials stayed in environment variables.
4. Atomically installed protected compatibility evidence and updated the existing
   `CRM_MIGRATION_RELEASE_REPORT` path. Installed staged Web and launched the
   existing `./scripts/dev-api` and pinned `pnpm exec vite preview` lifecycle.
   New API PID **35303**, Web PID **35323**; a fresh observed-runtime inventory
   and confirmation preflight passed after launch. The API includes the metadata
   worker; no separate worker deployment was introduced.
5. **43 HTTP/auth/asset checks passed:** local/public health/readiness, anonymous
   denial, same-Organization admin and other-Organization admin empty lists,
   member denial, unknown metadata-import denial, unchanged workspace session
   fields, existing CRM reads, exact served index and entry/Migration assets.
   Temporary sessions were revoked. Both admins' empty results are release
   smoke evidence; actual foreign-resource isolation retains its separate
   synthetic DB/HTTP proof.
6. **Eight public Chrome workflow checks passed**, including the metadata panel's
   absent completed parent, disabled preparation, explicit refresh, reload and
   member route/navigation denial. Desktop and 390px Web have no horizontal
   overflow. All eight screenshots were inspected, with zero page errors or
   unexpected source/business requests. Session cleanup succeeded. The browser
   helper blocked Cloudflare telemetry; telemetry delivery is outside this proof.
7. `./scripts/check-tunnel` passed app/API HTTP **200/200** and realtime WebSocket
   **101**. At **17:28:08 PDT**, a 72-second bounded observation confirmed both
   listeners, all three backend hashes, 72 Web hashes and 975 source hashes,
   with zero API/Web WARN/ERROR entries. No recurring monitor was created.
8. Final database reads after the browser confirmed all 45 business counts and
   three workspace states unchanged. All **43 migration tables**, operation
   admissions and imported-Person facts are empty. No seed, reset, import or
   binding change occurred. Registered FUB system configuration remains unset.

[Sanitized release evidence](../design/qa/slice-010f1-2026-09-11-release/README.md)
contains executed results, inventory, artifact hashes, counts and screenshots.
Private backup/configuration material, helpers and runtime logs remain outside
Git. Historical synthetic evidence is preserved as the implementation checkpoint;
the release evidence records the subsequent committed artifacts and actual fleet.

An independent read-only release audit at 17:29:27 PDT found no material gap. It
directly checked the listeners, working directories/executable mapping, all 975
source hashes, three backend/72 Web hashes, backup digest, preflight input hashes
and protected installed report. All three newly retired and 23 historical
artifacts retained exact bytes without executable permission. The broader
process scan found only the expected API. It inspected the recorded HTTP,
database, browser, tunnel and observation results without new DB queries, test
gates, implementation review, configuration reads or runtime mutations.

The installed compatibility report expires at **2026-09-12 00:30:47 UTC**
(2026-09-11 17:30:47 PDT). Import confirmation requires a fresh successful report
from a current workload inventory. Expiry leaves ordinary CRM access available
and confirmation unavailable; no automatic renewal or live import was performed.

## Compatibility and recovery

Follow [the existing runbook](SLICE_010c_RELEASE_PREPARATION.md) plus the
[metadata capability contract](../specs/SLICE_010f1_CONTRACT.md#compatibility-and-checks).
The migration is additive. An older 010c artifact cannot prepare or run metadata
children and cannot establish metadata-confirmation readiness. Preserve the new
schema and any retained/imported rows; prefer a verified metadata-capable
forward recovery. Never delete review bindings, reset workspace mode or restore
the whole database just to run old software. Any fallback requires an observed
compatible fleet with no overlapping workers and must leave metadata confirmation
unavailable unless every required artifact supports it.

The private backup and retired artifacts are recovery evidence, not an automatic
rollback procedure or production backup system. Live FUB validation remains
user-deferred. Notes/tasks, later source families, repair/deltas and activation
retain their own specifications and approval boundaries.
