# Slice 010d1 — Shared-development release

**IN PROGRESS — 2026-09-12.** D-070's follow-up authorizes commit, merge, push,
cleanup and deployment of the verified historical capture/coverage slice to the
existing Mac-hosted shared-development API/Web. No live FUB/customer processing,
native timeline import, activation or production-cluster deployment is included.

The release starts from main `028d6133e1b7c3f81642275e030f63e98b2cca49` and the
verified `codex/slice-010d1-history-capture` worktree. All 43 pending main audit/
planning files were preserved privately. The previous three backend binaries,
73 Web files and configuration were preserved before building any replacement.
Recovery and release helpers/evidence are protected in
`/private/tmp/crm-010d1-release-6kg5mx3s`; credentials and backups stay outside Git.

[Implementation verification](SLICE_010d1_VERIFICATION.md) records the passing
SQLx/full/DB gates, both reviews, 75,000-observation collector and synthetic
production-Web walkthrough. Those test results retain their original source
manifests. Release steps will verify the merged source and separately record
actual builds, database preservation, capability preflight, HTTP/browser checks,
worktree cleanup and pushed revisions. An earlier test result is not relabeled
as a release-time rerun.

Follow the [compatibility runbook](SLICE_010c_RELEASE_PREPARATION.md) and
[history contract](SLICE_010d1_CONTRACT.md). Recovery must preserve schema,
identities, encrypted evidence and workspace/People-parent bindings. Prefer a
verified history-capable forward recovery; the backup and retired binaries do
not authorize a whole-database rollback or erasing bindings to run old code.
