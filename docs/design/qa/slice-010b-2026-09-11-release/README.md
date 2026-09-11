# Slice 010b shared-development release evidence

Deployed main `89471f0b7bccc3e91c5f6c94aacecf7e7bd2216d`, implementation
`b71a854`, on 2026-09-11. See the [release record](../../../tasks/SLICE_010b_RELEASE.md)
for commands, results, configuration, recovery and cleanup.

- `deployed.json`, `build-results.json`, `build-sha256.json`: revision, actual
  listeners, production build outcomes and exact executable/Web hashes.
- `smoke-results.json`: 29 HTTP/auth/asset checks, empty migration tables and
  unchanged business counts.
- `database-before.json`, `database-after.json`: counts only; no customer content.
- `browser-results.json`: public admin/member, refresh/reload and layout checks;
  temporary sessions revoked, no source/business mutations, zero page errors.
- Four PNGs show empty connection/core states at desktop and 390px. All were
  visually inspected; credential inputs are empty. Native clients are later work.
- `observation.json`: bounded post-start listeners/logs, tunnel result, 37 matching
  implementation hashes, equal merge tree and readable backup catalog.
- `cleanup.json`: merged worktree/branch and disposable QA cleanup, retained
  private rollback artifacts.
- `handoff-checks.json`: local documentation paths, JSON parsing, preserved
  implementation/evidence hashes, whitespace and bounded credential checks.
- `artifact-sha256.json`: integrity manifest for this bundle, excluding itself.

No authenticated FUB request, customer-data import or production-cluster release
was performed. The passing implementation gates are retained in the
[synthetic evidence bundle](../slice-010b-2026-09-11/README.md). Full logs, cookies,
private configuration and database archives are excluded from Git. Backup
restoration was not exercised. Live authorized FUB validation remains deferred.
