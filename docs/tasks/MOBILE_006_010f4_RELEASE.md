# Mobile006 / 010f4 — Shared-development release

**DEPLOYED-AND-VERIFIED — 2026-09-15.** D-084's release follow-up and the user's
explicit commit/merge/push/cleanup/deploy request are complete for existing
Mac-hosted shared development at [app.tarams.org](https://app.tarams.org/manage/migration).
This is the current operational release record.

## Publication and runtime

- Main merge `d11fccf` was pushed; the final responsive Web correction `a96b771`
  was also pushed. Subsequent release/status commits are documentation only.
- API/in-process workers: PID `78285`, API3000; backend built at `3c080b5`,
  verified byte-for-byte source-equivalent to final production Rust. Later Rust
  changes were test/benchmark-manifest corrections. Web5173: PID `79921`,
  production build at `a96b771`; all **73 public assets** match staged SHA256s.
- Exact backend SHA256s:
  - API/worker: `6cb2702af164711d1510468d294b892a33f6d6ba95eb996068796847819be4c7`.
  - Admin CLI: `30b4cc88f1231220833748c7410f98ec9565ff6d6aef49caa6b35753d0fce75b`.
  - Migrator: `1fa4c59d8696b80146b9fd113ce5b600fa44148718eb7df20c9e48e7193cf85b`.
- Private evidence/recovery root:
  `/private/tmp/crm-mobile006-010f4-release-z_di8bdf`.
  `final-release-status.json`, build/source manifests and runtime inventories own
  exact artifact identity. Only `CRM_MIGRATION_RELEASE_REPORT` changed in `.env`;
  all other configuration values, including receipt keys, were preserved.

## Database and recovery

The actual database already had 69 successful migrations through
`20261002000010`, including Mobile005/010f3. This release applied exactly five
pending Mobile006/010f4 migrations with the verified normal migrator. All **74
applied checksums** match source. This closes the current operational state;
it does not invent missing historical evidence for the earlier release attempt.

The custom `crm_dev` backup is **113,237,922 bytes**, SHA256
`a2516c2dc060e8e18f93aa2e41a78da9391e580e650f52b4d1a6d4ecd582a9f6`.
Its catalog validates with 1,742 entries. Configuration, three old binaries,
all 73 old Web assets and verified forward artifacts are preserved in `recovery/`
and staged directories. Catalog validation and row extraction are **not restore
proof**; temporary artifacts are not a durable backup policy.

Before migration, all 180 existing data-table projections were preserved exactly.
After smoke cleanup, **194 of 196** data-table projections match the post-migration
baseline exactly. The two explicitly reconciled changes are five new revoked
sessions and one existing mobile admission counter advanced by exactly two.
All 381 prior sessions remain unchanged. The owned smoke context/generation was
guarded by exact Org/actor/installation IDs and zero receipts, then removed.
All 100,077 existing Person rows also match the backup's older fields directly;
new metadata revisions are exactly 1 and the catalog backfill matches every Org.
There were no business mutations or import confirmations.

Evidence: `backup.json`, `database-before.json`, `database-after-migration.json`,
`migration-verification.json`, `database-final-corrected.json.comparison.json`,
`person-differences-summary.json`, `smoke-runtime-reconciliation.json` and
`smoke-mobile-cleanup.json`.

Keep workspace bindings and compatible binaries intact. Prefer verified forward
correction. Restoring a full backup can replace post-backup writes and requires
a concrete incident decision; do not downgrade to an incompatible old binary.

## Executed verification

- [Implementation acceptance](MOBILE_006_010f4_FINAL_VERIFICATION.md): both final
  D-050 implementation reviews READY; all 1,103 DB and 993 ordinary Rust tests,
  SQLx, native installed-store/UI/replay proofs, exact migration browser
  reconciliation, 18 Mobile and 26 migration plans, paired Person/Today gates pass.
- Locked isolated API/admin/migrator build: **PASS 66.995s**. Initial production
  Web build: **PASS 2.033s**. Final Web lint/typecheck/**1,298 tests**/isolated build:
  **PASS 28.009s**, `logs/release/web-wrap-check.log` under the implementation QA root.
- Fresh launch and confirmation preflights: **PASS**. Actual API/workers,
  CLI/migrator paths, Docker services and scheduled launch paths inventoried;
  no incompatible executable launch path remains. Final report expires at
  `2026-09-15T08:01:58Z`; later import confirmations need fresh inventory/preflight.
- **75 HTTP checks PASS:** anonymous/member/foreign-Org boundaries, no-store,
  ordinary People/Today, mobile metadata capabilities/current reads/catalog and
  complete sealed generation. Three HTTP sessions and two real browser sessions
  were explicitly logged out; exact database reconciliation verifies all five.
- Real deployed desktop and 390px UI: **PASS**. Release verification found a
  clipped preparation label; `a96b771` lets it wrap and grow at narrow widths.
  Final screenshot/DOM evidence shows the entire label within a 237px-wide,
  60px-high control, with no horizontal overflow. Viewport reset and test tab closed.
  This was a bounded release UI correction, not another implementation review round.
- Five local/public health endpoints and **73 exact public asset hashes PASS**.
  `scripts/check-tunnel` passes app/API routing and Centrifugo WebSocket **101**.
- Final **180.21-second** observation passes seven samples of readiness/public
  API/Web health. API log has zero errors and one expected unauthenticated mobile
  warning from the negative authorization smoke check.

Retained verification failures were resolved without weakening checks: the host's
older Python TLS client failed before public probes, so the helper uses normal
certificate-validating system curl; a transient asset probe succeeded on focused
recheck and the final complete hash sweep passed with bounded transport retries.
The first final-row comparison accidentally mixed old-column and full-column
hash projections. The corrected helper inherits the original projection, and the
independent backup comparison plus new-default checks verify actual preservation.
Failed artifacts and the corrected results remain traceable.

## Cleanup and limits

Three completed writer worktrees and five milestone branches were removed.
`branch-archive/completed-lanes.bundle` was verified before deletion; it preserves
all four writer histories and the exact uncommitted migration-lane patch/file.
QA databases, native installed stores, captures, logs, screenshots, frozen binaries
and recovery material remain. The native QA API3106 at `82d081e` was resumed after
performance verification and retains PID `52273`. Browser QA processes are stopped;
their databases, outputs and acceptance evidence are retained.

Native source publication is included. Native distribution, physical-phone/cellular
proof, live FUB/customer processing, workspace activation, calling and production
cluster deployment remain separate. No unattended monitoring was configured.
