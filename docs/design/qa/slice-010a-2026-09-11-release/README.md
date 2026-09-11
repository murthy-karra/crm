# Slice 010a shared-development release evidence

Deployment from `0735015`, 2026-09-11. See the
[release record](../../../tasks/SLICE_010a_RELEASE.md) for commands, recovery and limits.

- `deployed.json`: tested source revision, new listeners and executable/index hashes.
- `smoke-results.json`: 21 HTTP/authentication/bundle checks, schema version,
  unchanged business counts and no source connection/assessment.
- `browser-results.json`: public desktop/narrow navigation, reload and member denial.
- `observation.json`: listener continuity and bounded runtime log check.
- `migration-desktop.png`, `migration-mobile.png`: empty Migration page with
  synthetic seeded administrator; no credential input or FUB data.
- `artifact-sha256.json`: file hashes for the sanitized evidence above.

Private logs, database backup and rollback binaries remain outside Git.
Live FUB validation is user-deferred and is not represented as passed.
