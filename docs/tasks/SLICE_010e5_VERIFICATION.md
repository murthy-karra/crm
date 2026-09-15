# 010e5 — Verification

**PASSED — local implementation, 2026-09-15.** Local branch `codex/010e5-mapping-repair`, base
`4a01fc080d93cee959263e10f0d74fc4d64614f1`. No commit, release, live FUB call,
shared-development deployment or production change belongs to this verification.

## Runners and isolation

- Rust: local Rust 1.98 / cargo-nextest; `SQLX_OFFLINE=true` and
  `CARGO_TARGET_DIR=/private/tmp/crm-010e5-target-20260915`.
- Database: SQLx-created isolated synthetic databases under the migrator role;
  commands and workers use the application role. Database suites are serial.
- Web: installed pnpm/Vite/Vitest; production output
  `/private/tmp/crm-010e5-web-dist-20260915`. Shared `web/dist` is preserved.
- Browser fixture: loopback API 3105 and production Web preview 5185, synthetic
  account/source records and recording publisher. The fixture's test-only readiness
  is not a production release report.

## Evidence

| Check | Current result / evidence |
| --- | --- |
| Independent implementation review | READY, final round 2; [review record](SLICE_010e5_IMPLEMENTATION_REVIEW.md) |
| Final-tree service-free gates | `./scripts/check`: passed in 163 seconds, 995 Rust tests, 1,310 Web tests, 54 preflight tests, formatting, Clippy, production check, crate graph fences, doctests, Web lint/typecheck/build and 11 email-worker tests; `/private/tmp/crm-010e5-final-tree-check.log` |
| Final Clippy after remainder correction | Passed; `/private/tmp/crm-010e5-clippy-final.log` |
| Focused final recovery regressions | 2 passed: mixed settled-hold/unfinished remainder in both cohorts; unchanged-unassigned counted acknowledgement; `/private/tmp/crm-010e5-final-regressions.log` |
| Broader database gate | 43/43 passed, serial, 355 seconds; `/private/tmp/crm-010e5-db-verified.log` |
| Bounded source-key regression | 1 passed; oversized source stage becomes an explicit hold before encrypted catalog limits; `/private/tmp/crm-010e5-key-bound-test.log` |
| Expanded 25,000-record hot plans | 12/12 probes passed for both owners; `/private/tmp/crm-010e5-plans-final.json`, `/private/tmp/crm-010e5-plans-final.log` |
| Paired Person/Today gate | Passed, one measured run; `/private/tmp/crm-010e5-paired-final.log`, `/private/tmp/crm-010e5-person-today/` |
| Real browser desktop walkthrough | Passed: held preview replacement, saved choices, full field comparison, exact confirmation, one update plus one approval-only outcome, reload recovery; `/private/tmp/crm-010e5-browser.log` |
| Browser 390px/cancellation walkthrough | Passed: saved choices, full preview, cancel, reload with retained choices/preview and zero settlements, logout clearing; `/private/tmp/crm-010e5-browser-narrow.log` |

Existing and new database cases cover original/admitted ownership, scope and tenant
checks, target changes, local-edit holds, same-request replay, full core updates,
no-op source revalidation, exact baseline/result changes, encrypted byte accounting,
transaction rollback/fault injection, normal-refresh follow-through, second mapping
repair, partial cancellation, immutable predecessor bindings and legacy capability
rejection. The HTTP case verifies member/foreign-Organization rejection, strict
input, no-store responses and foreign target rejection through real routes.

## Failures found and corrected

Earlier iterations failed on an outdated compatibility hash, row-lock permissions,
a PL/pgSQL variable ambiguity, an incorrect Retry error, older byte-ledger formulas,
and tests injecting corruption through a now-immutable evidence path. Corrections
retain the guards and update explicit corruption setup/byte expectations. Two new
test assumptions were corrected: initial baselines exist before execution, and
admitted confirmation needs its `acknowledged_eligible_count` field. The paired benchmark first stopped before measurement because its shared-helper manifest needed the intentional D-088 workspace capability stamp; the declared shared-helper hashes were updated while retaining the three frozen baseline query bodies. Browser review found a generic awaiting-choices pause message, now corrected in both panels. The final label edit initially failed ESLint formatting and was corrected. No failed run
is counted as passing evidence. Earlier logs are retained under the same
`/private/tmp/crm-010e5-` prefix.

## Scope and residuals

Only existing People are repaired. Never-imported People recovery, workspace
activation, retention/erasure policy, PgBouncer/supervisor architecture and
production deployment remain separate work. The two existing People workers are
extended; `crm-api/src/lib.rs` has no startup-loop changes. No native client code,
installed native store, shared API/Web binary or runtime database is replaced.
The build reports existing large-Web-chunk and large-test-binary unwind warnings;
they are not test failures or resolved performance claims.

## Query-plan evidence

The expanded fixture has 25,000 inert descriptors, candidates, keys and choices
per owner. It never executes a worker against those opaque volume rows. Production
SQL text is extracted into EXPLAIN ANALYZE; temporary fixture setup bypass is
restored before measurement. Discovery reaches one held item through indexes;
candidate preparation reads at most 50, key paging at most 51, and the frozen
choice lookup at most one. Affected-count and retained-byte aggregates remain
linear in the repair population.

| Statement | Original / admitted execution time (ms) |
| --- | --- |
| Held discovery | 0.058 / 0.049 |
| Candidate page | 0.083 / 0.042 |
| Key page | 0.022 / 0.020 |
| Frozen choice | 0.016 / 0.018 |
| Affected count | 2.534 / 4.272 |
| Retained bytes | 12.011 / 12.996 |

These are local synthetic plan observations, not production latency or maximum
capacity claims. The 43-case functional gate preceded the final accounting indexes,
query root predicates, oversized-key guard and UI label correction. The expanded
plan run applied the final migration; the final service-free gate and focused
oversized-key test cover those source changes.

Tested implementation source inventory: `/private/tmp/crm-010e5-tested-source.json`
(37 changed/new backend, Web and script files; SHA-256
`dd7d12acdc71494bc958fc285feaf0f98c75a0396da685615686021591a637cb`).
Documentation-only final status edits follow this inventory.

## Paired reader regression

Forty measured samples per arm with alternating order, identical responses and
unchanged fixture counts. Person detail p95: baseline 16.763 ms, current 19.796 ms
(limit 41.763 ms). Today p95: baseline 60.535 ms, current 58.548 ms
(limit 85.535 ms). Both passed the existing baseline + max(25 ms, 10%) gate.
The frozen Person query bodies remain pinned; both arms share the current D-088
authorization/capability helpers, as declared in the source manifest. This tests
query-path regression, not a historical authentication-stack comparison.

## Browser and cleanup

Desktop used the production Web build and real typed HTTP routes with the synthetic
fixture: replaced an all-held preview, mapped the unknown stage to Lead, saved
choices, prepared the full preview, inspected baseline/current/proposed name and
contact identities, acknowledged exact counts, confirmed, and observed one native
update plus one approval-only settlement. Both preview and final results recovered
after reload. The results traversal required its explicit “Load latest” action
to include the later settlement, as designed.

At 390 × 844, mapping controls and actions fit the viewport. The corrected pause
message directed the next action. A separately prepared preview was cancelled;
reload preserved the choices and preview with zero settlements and no confirmation
action. Logout returned to the sign-in view without retained workspace content.
Tenant/member denial is covered through real HTTP tests; actor/Organization cache
clearing and uncertain-response recovery are covered by Web component tests.
The browser fixture bypasses release inventory only through its existing test
readiness configuration; production schema/readiness guards are exercised by the
database/preflight suites. No live FUB or customer-source request was made.

Both loopback fixture servers and the Web preview were stopped; ports 3105 and
5185 have no listener. Temporary browser tabs were closed and the viewport override
reset. Shared services and native stores were preserved. The startup source is
unchanged: 14 migration worker tasks remain, including the two extended People
refresh workers; zero new startup tasks, executables or pods are introduced.

## Local merge — 2026-09-15

User authorized commit, merge and cleanup after verification. All 37 tested source
hashes still matched before commit. Implementation commit `05abffa` was merged
into local `main` with `--ff-only`; no conflict or implementation edit occurred.
The feature branch was deleted. Remote push and deployment were not performed.
Local Lavish artifacts and unrelated worktrees were preserved.
