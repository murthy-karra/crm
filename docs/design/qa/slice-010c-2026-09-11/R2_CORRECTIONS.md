# Slice 010c — Round 2 targeted corrections

This addendum closes the three consolidated Round 2 findings for targeted reviewer confirmation within the same second review round. The original R2_CHECKPOINT.md and R2_SOURCE_SHA256.json, their original source hashes and verification logs remain historical evidence. No database, Cargo, runtime, live source/provider or Git mutation was performed for these Web corrections.

## Corrected findings

1. **I010-R2-1: unapplied mapping choices.** The real mapping child reports dirty drafts to the parent. An unapplied draft disables confirmation and invalidates an already open confirmation dialog. The administrator must apply it to produce a new frozen plan or explicitly discard it before confirming. Tests exercise the real parent/child components.
2. **I010-R2-2: ordinary route recovery.** App.vue now leaves ordinary private RouterViews unmounted while workspace verification is pending, including failed verification. They mount and start a fresh authorized read only after successful `/me`. The migration route keeps its stable mounted, hidden instance so reviewed confirmation recovery remains intact. The new App-level tests use the real shell, workspace coordinator, API client and People query hook with a five-second delayed authority response, beyond the existing single-retry window. Both ordinary recovery cases failed against the frozen code before the fix; the corrected cases render fresh data without a lingering error. A third test proves the actual App retains migration-local request state across the same verification.
3. **I010-R2-3: stale visible results.** Existing parent import polling now refreshes the visible results page when committed progress or terminal state changes. It preserves the active filter/cursor and adds no independent polling loop. Integrated tests follow Pending to the committed Person link and completion, then prove polling stops and filter/cursor remain selected.

## Targeted verification

All commands used the isolated worktree and pinned Node 24.16.0/pnpm 11.22.0. Test suites overlap earlier evidence; counts are not cumulative unique totals.

- Before fix: `pnpm exec vitest run src/App.test.ts` — **2 expected regression failures** against the frozen App. Log `/private/tmp/crm-010c-r2-route-before.log`.
- Primary final: `pnpm exec vitest run src/App.test.ts src/components/AppShell.test.ts src/workspaceLifecycle.test.ts src/api/workspaceClient.test.ts src/router.test.ts` — **69 passed, 5 files, 2.78s**, no unhandled errors. Log `/private/tmp/crm-010c-r2-route-corrections-final.log`.
- Support final: the six import API/component test files — **39 passed, 6 files, 1.31s**, no unhandled errors. Log `/private/tmp/crm-010c-web-r2-fixes-vitest.log`.
- Primary changed-file ESLint — **PASS**, no diagnostics; `/private/tmp/crm-010c-r2-route-eslint.log`. Support changed-file ESLint — **PASS**, no diagnostics; `/private/tmp/crm-010c-web-r2-fixes-eslint.log`.
- Current project vue-tsc, executed by support after the primary fix — **PASS**; `/private/tmp/crm-010c-web-r2-fixes-typecheck.log`. Git whitespace check — **PASS**; `/private/tmp/crm-010c-r2-corrections-diff.log`.

## Frozen correction scope

R2_CORRECTIONS_SOURCE_SHA256.json contains **208 current source hashes** and seven correction-log hashes. Exactly six original source files changed, one new App test was added, and **201 original source hashes remain unchanged**. All 143 backend source entries are unchanged from the original R2 snapshot. All eight original verification-log hashes still match.

Changed sources:

- `web/src/App.vue`
- `web/src/components/migration/ImportMappingPanel.vue`
- `web/src/components/migration/ImportRecordPanel.test.ts`
- `web/src/components/migration/ImportRecordPanel.vue`
- `web/src/components/migration/PeopleImportPanel.test.ts`
- `web/src/components/migration/PeopleImportPanel.vue`
- `web/src/App.test.ts`

Manifest SHA256: `27fb2a03b4f5225791ef118171f863430296a7a5dcbf787d62f0c6632a4d1108`. The current source/hash snapshot is frozen for targeted confirmation. Final full gates, actual D-050 query plans, the paired reader run and private browser QA remain required after that confirmation.
