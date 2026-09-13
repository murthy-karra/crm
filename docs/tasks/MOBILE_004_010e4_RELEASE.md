# Mobile004 / 010e4 — Shared-development release

**DEPLOYED / VERIFIED — 2026-09-13.** The user's “ok commit and push, cleanup
and deploy” and D-080 release follow-up authorized main integration/publication,
merged-branch cleanup and the existing Mac-hosted shared-development release at
[app.tarams.org](https://app.tarams.org/manage/migration).

Runtime backend source: `ffbc9fd69871bb6a3dece88dfa008228dc30cb47`. Production Web
was built at `26657c8923f6e5c91f811050dced0b8c3a9adb24`; its source and all 73
artifacts are unchanged by the backend-only follow-up. Later commits contain
closeout documentation/evidence. Native sources are published; installed demo
and QA stores/APIs were preserved. Native distribution remains separate.

The [implementation record](MOBILE_004_010e4_IMPLEMENTATION_STATUS.md) owns the
989 ordinary Rust, 1,034 database, 1,238 Web, native acceptance and D-050 evidence.
Release reused those gates and ran the affected checks for the correction below;
no broad suite or independent review round was repeated.

## Execution and evidence

Private evidence/recovery root: `/private/tmp/crm-mobile010e4-release-8lygj_mr`.
[Sanitized release summary](../design/qa/mobile004-010e4-2026-09-13-release/README.md)
contains selected results and inspected browser screenshots.

| Step | Result |
|---|---|
| Publication/cleanup | Fetched origin, verified ancestry, scanned 104 publication files with zero configured-credential matches, fast-forwarded/pushed main. Removed all six merged milestone branches; only main and its worktree remain. |
| Builds | Isolated locked API/admin/migrator build passed in 46.721s; production Web passed. All 78 protected runtime files and 1,129 source files were unchanged during build. Forward header-fix build passed in 5.424s, preserving running artifacts. Isolated release build cache removed after checks; recovery binaries retained. |
| Backup | Custom crm_dev backup: 113,064,720 bytes; SHA256 `79361ba6918224ceaccba8e2a1a00787e843e89080e9cdc59a6cd28b0ddff744`. Catalog validated 1,656 entries. Restore was not exercised. |
| Schema | Normal verified migrator applied `20260929000001` and `20260930000001`–`00010`. All 45 old checksums matched before migration; all 56 applied SHA384 checksums match source afterward. No reset or destructive rollback. |
| Preservation | All 151 prior tenant rowsets preserve every old field, with separately verified stage/catalog revision defaults and null legacy generation catalog. All 45 business counts, workspace state/admissions and 100,077 history-review rows remain unchanged. After smoke cleanup, all 161 complete upgraded tenant rowsets match, including the ten new empty tables. |
| Runtime/configuration | API61780/port3000 and Web59404/port5173 serve the verified artifacts. Only `CRM_MIGRATION_RELEASE_REPORT` changed; all receipt keys and other configuration are preserved. Demo71121/3101 and native QA33892/3102 remain on their verified separate databases. |
| Compatibility | Actual running API/in-process workers, CLI/migrator launchers, containers and scheduled paths were inventoried. Workspace gate and all migration feature capabilities, including `fub-admitted-people-refresh-v1`, pass. Excluded native APIs have verified process environment/database targets. Old release executables were retired without deleting recovery material. |
| HTTP/assets/tunnel | Corrected smoke: 64 checks / 73 requests pass. Anonymous/member/admin/foreign-admin reads return closed no-store 401/403/404 responses on refresh list, availability, detail, item, contact, field and result routes. Ordinary People/Today reads and public entry/Migration assets pass. All 73 locally served Web files match hashes; `scripts/check-tunnel` passes 200/200/101. |
| Browser | Actual login, empty-cohort preview disabled state, refresh reload and Today navigation pass. Desktop1280 and 390×844 screenshots inspected: no page/console errors; narrow panel clientWidth=scrollWidth=292. Created browser session revoked and private browser closed. No migration or Operator action submitted. |
| Mobile | 63 public HTTP requests pass. Bootstrap advertises stage changes/revisions/catalog alongside prior capabilities, preserving the exact seven-day lease and installation/context binding. Opt-in generation returns nine stages at revision1, four People through 12 component pages, matching current stage IDs/names/revisions, actor/Org denials and a valid seal. No business operation POST. All owned metadata cleaned after identity/receipt guards. |
| Observation | Artifacts, source, configuration, listeners and served assets remained stable across 188.310s. Zero API errors; 22 WARNs match deliberate mobile denials (3 not_found, 18 unauthenticated, 1 protocol_unsupported). All seven created HTTP/mobile/browser sessions were revoked. |

## Release correction

The first HTTP smoke correctly rejected a member with `403`, but the outer
workspace authorization guard omitted `Cache-Control: no-store` before the new
refresh route middleware ran. The initial rollout is retained under
`first-rollout/`; its failed smoke log/results are preserved separately.
`ffbc9fd` adds the new route prefix to the outer no-store guard. Formatting,
workspace/all-target Clippy and all nine focused refresh/workspace SQLx tests
passed, including `member_denials_before_refresh_routes_are_not_cacheable`
(`header-fix-check.log`, 10.478s test execution). The corrected API was rebuilt,
pushed and deployed; full public HTTP and browser/mobile release smoke then passed.
No schema, business behavior, authorization policy or Web source changed for this
fix. The unchanged implementation/performance evidence retains its original
revision attribution rather than claiming a second full-suite run.

## Limits and recovery

The deployed migration browser check covers an empty cohort. Populated refresh,
cancellation/remainder, source/provenance and local-change handling belong to the
isolated retained-synthetic implementation evidence. Mobile release checks cover
capabilities and reads; offline mutations/conflicts and installed-store upgrades
remain attributed to simulator/emulator implementation checks. No physical-phone,
app distribution, live FUB/customer processing, source confirmation, business
mutation, external call, activation or production-cluster deployment occurred.

Fresh actual-inventory evidence expires at **2026-09-13T19:45:18Z**. It is not
renewed automatically. Ordinary CRM/retained reads continue after expiry;
future migration confirmation needs fresh operator preflight.

Preserve the private backup, original configuration, retired artifacts and exact
verified forward binaries. Only the release-report path changed; receipt keys
must remain available for old operations. Recovery must preserve additive schema
and workspace bindings. Do not reset bindings or restore an old binary to bypass
compatibility. A full database restore can replace post-backup writes and needs
a concrete incident/recovery decision; backup catalog validation is not restore
proof. This release completed without restoring or rolling back data.
