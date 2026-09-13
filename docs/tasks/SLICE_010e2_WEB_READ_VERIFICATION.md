# Slice 010e2 Web and protected-read checkpoint

This checkpoint covers the coordinator-owned Web implementation, protected
read queries, route fragment seam, and synthetic browser harness. It does not
claim the worker, ledger/recovery, browser acceptance or final release gates
complete. Source is on `codex/migration-010e2`; shared services were untouched.

The admin workflow selects a sealed report, prepares and reviews a frozen plan,
displays clear/removal counts, confirms exact immutable input, and supports
explicit re-preview, cancellation and exact-request retry after a lost response.
Items, contacts, complete retained name fragments and results have separate
bounded traversal. Authority/identity/plan transitions fence late data and
remove displayed private values. Prefixes are visibly marked and never become
executable input. The protected reads retain authorization/workspace locks
through bounded response assembly and authenticate all cursor context.

## Actual checks

- `npx vitest run src/api/peopleRefreshes.test.ts src/components/migration/PeopleRefreshPanel.test.ts src/views/MigrationView.test.ts`: **30/30**.
  `/private/tmp/crm-010e2-qa/web-focused-2.log`.
- Full `npm run test`: **1202/1202**, 88 files.
  `/private/tmp/crm-010e2-qa/web-all-1.log`.
- Targeted ESLint and `npm run typecheck`: passed;
  `/private/tmp/crm-010e2-qa/web-typecheck-2.log`.
- `npm run build`: passed; existing large LiveKit chunk warning remains.
  `/private/tmp/crm-010e2-qa/web-build-1.log`.
- Isolated SQLx `db_people_refresh_reads` tests: Unicode name reconstruction,
  complete contact pagination, immutable preview and fixed settlement traversal
  passed in `/private/tmp/crm-010e2-qa/db-reads-2.log`. Authorization, tenant scope,
  no-store, cursor context and sealed-plan test passed separately in
  `/private/tmp/crm-010e2-qa/db-reads-authority-3.log`. All use retained synthetic
  captures, a real completed 010c parent and command-created 010e1/010e2 plans.

Two initial assertions were corrected to match established behavior: an old
sealed plan remains inspectable before its replacement starts building, and a
session with an inactive membership is rejected with HTTP 401 (active non-admin
is 403). Neither correction weakened the read guard. The final test explicitly
rejects partial successor pages and reuse of the old cursor after sealing.

The `perf-harness`-only, explicitly ignored `serve_people_refresh_ui_fixture`
requires an exact synthetic opt-in and the isolated `crm_010e2_qa` database.
It binds API3103, creates only synthetic evidence, and has no live FUB reader.
Its earlier successful compile is harness preparation, not browser proof.
