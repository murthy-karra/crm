# Mobile005 / 010f3 — Shared-development release

**IN PROGRESS — 2026-09-14.** The user's explicit commit/push/merge/cleanup/deploy
request and D-082 release follow-up authorize this release to existing Mac-hosted
shared development at [app.tarams.org](https://app.tarams.org/manage/migration).
Candidate code is the verified `ab4a362`; subsequent documentation does not change
the [completed implementation checks](MOBILE_005_010f3_FINAL_VERIFICATION.md).

## Ordered release plan

1. Verify clean source, origin/main ancestry and publication content; merge to main
   and push. Remove only clean, merged milestone worktrees and branches, retaining
   QA databases, native stores, evidence and recovery artifacts.
2. Build locked API/admin/migrator binaries and production Web into isolated paths.
   Verify source and protected running artifacts remain unchanged during builds.
3. Back up `crm_dev` in PostgreSQL custom format and validate its catalog. Record
   applied migration checksums, workspace state and complete tenant row hashes.
4. Apply the 13 additive migrations through the verified normal migrator. Verify
   prior fields remain unchanged, new details revisions have their declared default,
   and new schema/checksums match. Preserve the workspace review hold and source data.
5. Replace only shared API3000/Web5173 using the existing local process lifecycle.
   Preserve receipt keys and other configuration; update only the server-owned
   `CRM_MIGRATION_RELEASE_REPORT` path. Inventory actual API/in-process workers,
   CLI/migrator launch paths, containers and scheduled paths; retire incompatible
   executables without deleting recovery material. Run launch/confirmation preflight.
6. Verify runtime/artifact identity, public/local health and Web assets, tenant/admin
   boundaries, mobile capabilities/current-profile reads, browser desktop/narrow
   behavior and realtime tunnel. Revoke created sessions and remove only guarded
   owned mobile smoke metadata. Recheck complete rowsets and observe for three minutes.

## Recovery and scope

Keep the pre-release configuration, custom backup, old artifacts and verified
forward binaries privately. Schema/claim ownership and workspace bindings require
compatible recovery; do not downgrade to an incompatible binary or reset bindings.
Prefer a verified forward correction. A full restore can replace post-backup writes
and needs a concrete recovery decision; validating the backup catalog is not restore
proof. On a failure, stop further rollout and retain evidence before recovery.

Native source publication is included; app distribution, installed-store replacement,
physical-phone/cellular testing, live source/customer operations, business mutations,
calling and activation remain outside this shared-development release.
