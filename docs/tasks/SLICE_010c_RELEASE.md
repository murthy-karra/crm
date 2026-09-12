# Slice 010c — Shared-development release

**DEPLOYED AND VERIFIED, 2026-09-11.** The user authorized deploying 010c and then
planning the next import slice (D-065 follow-up). No production-cluster or live
FUB operation was performed.

Target: the existing Mac-hosted API and production Web bundle at
[app.tarams.org](https://app.tarams.org/manage/migration), database `crm_dev`.
Deployed revision `f01c2e3b4d672ad388716916326e499c6eab4332`, implementation
`fcd2480`, merge `c3f6ca9`. All 236 entries in the final implementation manifest
match. Prior Rust/Web/DB/query-plan/browser verification remains applicable;
this release changes no implementation bytes.

## Deployment and verification

1. Verified old 010b API PID 49244 and Web PID 49270, working directories and
   artifact hashes. Preserved executable/Web and environment recovery copies
   in mode-0700 `/private/tmp/crm-010c-release-q50bgih3`.
2. Built locked `crm-api`, `crm-admin` and `migrate` binaries with
   `cargo build --manifest-path backend/Cargo.toml -p crm-api --bin crm-api
   --bin migrate --bin crm-admin --locked` (27.80s), plus Web using
   pinned Node 24.16.0/pnpm 11.22.0 into private staging. Catalogued exact hashes
   against the deployed Git revision, including in-process worker roles. Web
   `pnpm run build --outDir` passed in 1.30s; its existing chunk-size warning
   remains. No full application test/benchmark rerun was needed on unchanged code.
3. Recorded 45 business-table counts and empty migration/source state. Took a
   custom-format `crm_dev` backup as `crm_migrator`: 109,875,666 bytes.
   `pg_restore --list` read its catalog. **No restore exercise was performed.**
4. `./scripts/db-migrate` passed in 0.39s, applying `20260918000001`. All three
   existing Organizations remain operational at workspace revision 1. The
   migration adds import/provenance tables, contact order, workspace functions/
   triggers and scoped grants. It imports no source data.
5. Stopped only the old listeners and retired 23 old executable paths, including
   release-profile/hashed-dependency binaries and historical recovery copies.
   Their bytes remain intact; executable permission was removed. No standalone
   CRM worker/container or CRM launch-agent configuration was found. Catalogued
   the new API under both API and in-process worker roles, plus admin/migrator
   launch paths. Actual-DB compatibility preflight passed for launch and confirm.
   The host has no `psql`; a private adapter invokes the existing container's
   real `psql`, with credentials in environment variables, not arguments. It
   performs no mocked database reads.
6. Installed the staged Web and launched `./scripts/dev-api` and pinned
   `pnpm exec vite preview`. New API PID **60714**, Web PID **60731**; the runtime
   inventory was observed again and confirmation preflight passed. The only
   configuration addition is
   `CRM_MIGRATION_RELEASE_REPORT`, referring to protected operator evidence.
7. **38 HTTP/auth/asset checks passed:** local/public health/readiness, anonymous
   migration/import denial, admin and other-Organization admin empty reads,
   member denial, unknown import denial, workspace session fields, existing CRM
   reads and exact served-index/assets. All 45 business counts are unchanged;
   all 30 migration tables, operation admissions and imported-Person facts are
   empty. Temporary sessions were revoked.
8. **Seven public Chrome workflow checks passed**, including People-import empty
   state, disabled planning without retained evidence, explicit import refresh,
   retained assessment/snapshot behavior, reload and member navigation/route
   denial. Desktop and 390px Web widths have no document/body overflow. All six
   screenshots were visually inspected; no page errors or unexpected business/
   source mutation requests occurred. Cloudflare telemetry was blocked by the
   helper and is outside this browser proof.
9. `./scripts/check-tunnel` passed app/API HTTP 200 and realtime WebSocket 101.
   The first shell wrapper tried to assign zsh's read-only `status` variable;
   the unchanged check was rerun through Python to capture its successful exit
   status. At 13:51 PDT the expected listeners and artifact hashes still matched,
   with zero API/Web WARN/ERROR entries during the bounded observation. No
   recurring monitoring was established.

[Sanitized release evidence](../design/qa/slice-010c-2026-09-11-release/README.md)
contains checks, counts, artifact manifests, inventory, preflight results and
screenshots. The private release directory retains the backup, helper source,
configuration recovery copy and runtime logs; no credential values are published.
An independent read-only release audit found no material gap: all 236 source
entries, three backend artifacts and 72 Web files match; both listeners and all
23 non-executable retired artifacts match the evidence. It confirmed actual-DB
preflight, reported check/count results and no credential exposure in publication
candidates. This audit ran no additional tests or runtime mutations.

Import confirmation requires a fresh, successful compatibility report. The
successful release report records an expiry of 20:53:40 UTC on 2026-09-11. Report
expiry keeps confirmation unavailable until the operator re-inventories and
renews evidence; no automatic renewal or live import is part of this deployment.

## Recovery boundary

Follow [the compatibility runbook](SLICE_010c_RELEASE_PREPARATION.md). Once a
durable review binding exists, recover with a verified compatible artifact;
never clear a binding or restore the whole database to permit old software.
Before any binding, an application rollback still requires current compatibility
evidence and non-overlapping workers. Preserve additive schema and captured
data. Old artifacts are retained as non-executable historical recovery material,
not available launch targets for a database containing review bindings.

Live FUB validation, actual customer-data processing and activation remain
deferred. `CRM_FUB_SYSTEM_NAME` and `CRM_FUB_SYSTEM_KEY` remain unset; no source
connection, snapshot or import was created. Runtime checks did not manufacture
an import in the shared database.
The implementation's isolated synthetic review/hold checks remain separate
from release checks against existing operational Organizations.
