# Mobile006 / 010f4 — Shared-development release

**IN PROGRESS — 2026-09-15.** The user's explicit “commit, merge, push, cleanup
and deploy” request and D-084 release follow-up authorize publication and the
existing Mac-hosted shared-development release at
[app.tarams.org](https://app.tarams.org/manage/migration).

## Ordered release plan

1. Finish [implementation acceptance](MOBILE_006_010f4_FINAL_VERIFICATION.md),
   including the full DB gate, one completed paired measurement and final Android
   UI receipt. Both existing second-round independent reviews are now READY.
2. Commit the final reviewed tree, verify origin/main ancestry and publication
   contents, merge to main and push. Remove only completed clean milestone
   worktrees/branches after preserving any unique evidence or unmerged work.
3. Build locked API/admin/migrator and production Web into isolated outputs.
   Inventory the actual shared processes, schema and all executable launch paths;
   reconcile the older Mobile005/010f3 IN PROGRESS release against actual evidence.
4. Preserve configuration and old/forward artifacts; create a custom crm_dev backup
   and validate its catalog. Record migration checksums and complete prior tenant
   row projections. Apply only pending additive migrations with the normal migrator,
   then verify old-field preservation and declared new revision defaults.
5. Replace only shared API3000/Web5173 through the existing process lifecycle.
   Preserve receipt keys and all configuration except the server-owned release
   report path. Inventory API/in-process workers, CLI/migrator, containers and
   scheduled launch paths; pass fresh launch/confirmation compatibility preflight.
6. Verify exact running artifacts, local/public health/assets, tenant/admin/member
   boundaries, mobile metadata capabilities/current/catalog reads and real browser
   desktop/narrow behavior. Remove only guarded owned smoke metadata, revoke
   created sessions, reconcile rowsets and observe stability for three minutes.

## Recovery and scope

Preserve private backup/configuration, prior artifacts and verified forward binaries.
Do not remove workspace bindings or downgrade to an incompatible binary. Prefer
forward correction; a full restore can replace post-backup writes and requires a
concrete incident decision. Backup catalog validation is not restore proof.

Native source publication is included. App distribution, installed-store replacement,
physical-phone/cellular testing, live FUB/customer work, business import confirmation,
calling and activation remain separate. The deployment target is shared development,
not a production cluster.

## Preparation evidence

Private release/recovery root: `/private/tmp/crm-mobile006-010f4-release-z_di8bdf`.
The existing configuration and three binaries are protected in `recovery/`;
73 served Web hashes were recorded. Isolated backend build at `3c080b5` passed
in 66.995s and production Web in 2.033s. Exact artifact hashes are in
`build-sha256.json` and `web-build-sha256.json`. Running API3000 still matches its
original SHA256 `88f9b5fad385b5bd3834b2f81ceb48a6fd40fb1481769a30ddbdf9e261ad5b6a`.
These are staged artifacts; rollout has not started.

Implementation acceptance is complete at `4bf6053`; later edits are documentation.
The live schema inspection found 69 successful migrations through `20261002000010`:
Mobile005/010f3 schema is already present. This release will apply the five pending
Mobile006/010f4 migrations and establish a new complete release record, without
inventing completion evidence for the older IN PROGRESS runbook.
