# Slice 010f1 — Shared-development release

**PREPARING, 2026-09-11.** The user authorized commit, merge, push, cleanup and
deployment under D-066's follow-up. Target is the existing Mac-hosted API and
production Web at [app.tarams.org](https://app.tarams.org/manage/migration),
database `crm_dev`. Shared development currently serves 010c source `f01c2e3`.
No live FUB connection/import, customer-data processing or activation is included.

## Ordered release plan

1. Preserve the 45 pending documentation files on main; 36 match the 010f1
   worktree exactly and nine have the expected implementation/status updates.
   Commit the verified implementation and evidence, merge it into current main,
   verify all 975 source hashes, and push. Remove only the merged branch/worktree
   and its ignored synthetic build/control artifacts after preservation checks.
2. Preserve current API/admin/migrator/Web and configuration recovery material in
   protected `/private/tmp/crm-010f1-release-ozyr7lke`. Build locked production
   `crm-api`, `crm-admin`, `migrate` binaries and staged Web from the merged source.
   Prior full gates remain applicable only while implementation bytes match.
3. Record existing business/migration counts and workspace modes, take a
   custom-format `crm_dev` backup, and verify its archive catalog. Run
   `./scripts/db-migrate` to apply additive migration `20260919000001`. Preserve
   all existing records; no reset, seed, import or binding change is intended.
4. Stop only the verified old API PID 60714 and Web PID 60731, retire incompatible
   executable launch paths while preserving their bytes, and inventory actual
   processes, in-process workers, admin/migrator paths, containers and launch
   configuration. Require `crm-workspace-v1` and `fub-metadata-import-v1` on all
   API/worker candidates and inventory entries. Run actual-DB launch/confirmation
   preflight and atomically install protected evidence at
   `CRM_MIGRATION_RELEASE_REPORT`; its five-minute expiry remains enforced.
5. Install staged Web; use the existing `./scripts/dev-api` and pinned
   `pnpm exec vite preview` lifecycle. Observe the new listeners and artifact
   hashes and rerun preflight from that current inventory.
6. Verify local/public health/readiness, admin/member/foreign-scope metadata
   access, unchanged existing CRM access, exact served Web assets, empty-source
   UI/reload/refresh at desktop and 390px, unchanged business counts and empty
   migration state. Run `./scripts/check-tunnel`; inspect screenshots and logs
   during a bounded observation. Revoke temporary sessions and publish sanitized
   results. No unattended monitoring is scheduled.

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

A backup catalog check is not a restore exercise; record whether restoration was
actually tested. Build, migration, preflight or smoke failure stops further rollout
while evidence and known recovery artifacts are preserved. Actual Git/source,
backup, runtime identities and executed results will replace this preparation
status after verification.
