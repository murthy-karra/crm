# Mobile004 / 010e4 — Shared-development release

**PREPARING — 2026-09-13.** The user's “ok commit and push, cleanup and deploy”
and D-080 release follow-up authorize main integration/publication, merged-branch
cleanup and the existing Mac-hosted shared-development API/Web release.

Candidate: completed implementation `3fa5bc8`, application/test source `7930b83`,
plus release authorization documentation. Reuse the [implementation evidence](MOBILE_004_010e4_IMPLEMENTATION_STATUS.md);
no application code change or repeated broad test/review round is planned.

Order: fetch and verify main ancestry; scan publication files; commit and publish;
build locked API/admin/migrator and production Web in isolated outputs; retain
old artifacts/configuration and a catalog-validated custom database backup; verify
existing migration checksums and exact tenant rowsets; apply additive migrations
`20260929000001` and `20260930000001`–`00010`; check new revision defaults and
unchanged old fields; refresh only API3000/Web5173 with actual workload inventory
and compatibility preflight; verify public/local HTTP, deployed assets, desktop/
390px browser and mobile read/catalog compatibility; clean only owned smoke
metadata; reconcile complete rowsets and observe stable service/artifact state.

Only `CRM_MIGRATION_RELEASE_REPORT` is expected to change. Preserve all existing
mobile receipt keys and native demo3101, native QA3102 and installed stores.
No source/import/refresh confirmation, business mutation, live FUB, external call,
customer activation or native distribution is part of release smoke checks.

Recovery uses the retained verified forward artifacts, original configuration,
additive schema and database backup. Do not drop schema, reset bindings or restore
an older binary merely to bypass compatibility. A backup catalog check is not a
restore test; restoring crm_dev would need a concrete incident/recovery decision
because it can replace post-backup writes. On failure stop rollout, retain evidence
and use compatible forward recovery. Deployment is complete only after runtime,
preservation and release checks pass.
