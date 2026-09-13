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

## Storage audit checkpoint

The three coordinator-owned `db_people_refresh_accounting` tests passed in
`/private/tmp/crm-010e2-qa/db-accounting-2.log` (14.56 seconds). They audit exact
encrypted payload and owned reservation sums, snapshot/Organization deltas,
prepare/confirm/re-preview/cancel replay, no writes under a lowered policy
ceiling, and baseline ownership transfer across two completed refreshes. The
first run failed with uncharged receipt differences of 110–113 bytes; the
receipt charge and prior-owner baseline debit fixes were included in the passing
run. The full-scan auditor is test-only; production work must still use bounded
unit accounting. Final preparation/recovery changes require their focused checks.

## Browser review and correction

Actual CUA review used the production Web on 5174 and synthetic-only API3103,
DB `crm_010e2_qa`; it created a frozen preview from a real sealed report. The
three comparison columns, explicit name/assignment clears, contact addition,
removal and preserved contact identities were inspected at desktop and 390×844.
At the narrow viewport the panel had clientWidth=scrollWidth=277 (no horizontal
content overflow); screenshots were actually inspected. The normal viewport was
restored. Cancellation through its dialog produced `Cancelled · 0 settled` and
no results. Browser error/warning inventory was empty at that checkpoint.

This caught a real backend tally issue: local conflict entries were counted as
already current. It also exposed absent-record and empty-explanation removal
labels. The worker fixes and fresh preview/confirmation remain to be verified.
The Web now labels held comparisons as non-executable and suppresses clear and
contact-change badges for those entries. Focused Web tests pass **31/31** in
`/private/tmp/crm-010e2-qa/web-focused-3.log`, including this regression. The
worker's actual closed pause reasons now have readable Web labels.
