# Slice 010f2 — Shared-development release

**AUTHORIZED; EXECUTION IN PROGRESS — 2026-09-11.** D-068's follow-up authorizes
commit, merge, cleanup, push and deployment of the verified notes/tasks import
to the existing Mac-hosted shared-development API and production Web bundle.
Live FUB/customer processing, activation and production-cluster work remain
outside this release.

Implementation starts from `cd3b0107392d4b40230bdd8996c12ec41be3d588` in
`codex/slice-010f2-activity-import`. The [verification record](SLICE_010f2_VERIFICATION.md)
retains both closed implementation reviews, passing final scripts, actual query
plans, the one paired Person comparison and all 18 browser phases with exact
data/storage reconciliation. No product source changes are planned for release.

Private recovery/evidence directory: `/private/tmp/crm-010f2-release-guor74mb`
(0700). The eight pending main planning files were preserved byte-for-byte;
four are identical to the implementation copies, and four have implementation
and status amendments. Existing 010f1 API/admin/migrator and Web hashes were
verified before preservation. Configuration remains private.

The release will use the normal additive migrator, actual database and executable
hash preflight, an observed inventory including in-process workers and retired
launch paths, and explicit `activity_confirmation_ready` in addition to metadata
and ordinary readiness. Fresh confirmation evidence is short-lived; ordinary
CRM access remains available after expiry. No import or automatic renewal is
part of the release. The SQLx cancellation concern remains an open bounded
[production-readiness follow-up](../plans/PRODUCTION_READINESS.md#observed-transaction-cancellation-notices).

Executed Git, artifact, backup, migration, runtime, smoke and cleanup results will
replace this in-progress record after verification.
