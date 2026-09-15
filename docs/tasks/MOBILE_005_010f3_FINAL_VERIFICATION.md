# Mobile005 / 010f3 — combined final verification

**PASS; both independent reviews READY.** Final tested source:
`ab4a362a224183dd1d51acd51196b71315826fd6`, coordinator checkout
`/Users/karrad/projects/crm`. Subsequent completion changes are documentation only.
All evidence paths below are relative to
`/private/tmp/crm-mobile005-010f3/integration` unless stated otherwise.

## Final sequential gates

`run-final-gates3.py` runs the repository's three gates in order using the private
environment and isolated Cargo/Web/SQLx outputs through `run-db-check.py`.
`final-gates-correction-run3.json` records the exact commands, source and results.
The private pinned pnpm11.22.0 path is recorded in `pnpm-gate-toolchain.json`.

| Gate | Result | Evidence |
|---|---|---|
| `scripts/check` | PASS,67.59s:992 Rust tests,5 doctests,1,264 Web tests,51 preflight and11 worker tests; formatting, lint, production compile, type/build and boundary checks pass | `final-check5.log` |
| `scripts/sqlx-prepare` | PASS,34.25s; `backend/.sqlx` unchanged | `final-sqlx-prepare3.log` |
| `scripts/check-db` | PASS,861.74s total; all1,076 database tests pass in810.170s, including two reported slow tests | `final-check-db2.log` |

The DB gate also passes Centrifugo health and a fresh-schema SQLx cache check.
Its992 skipped ordinary tests were already run by `check`; the first gate's1,076
skipped ignored tests all run in `check-db`. No required test is omitted.

## Acceptance, review and preservation

- [iOS verification](MOBILE_005_IOS_VERIFICATION.md):47 storage/model tests;
  actual native journeys and installed-store upgrade evidence remain applicable.
- [Android verification](MOBILE_005_ANDROID_VERIFICATION.md):42 storage/repository/
  Compose tests,2 JVM tests, lint/build and sealed native/installed-upgrade proof.
- [Migration verification](SLICE_010f3_VERIFICATION.md):24 final functional cases,
  historical reservation-owner repair, oversized-baseline preservation and scoped
  query-plan evidence, with original failures and correction results retained.
- [Readiness](SLICE_010f3_READINESS_VERIFICATION.md):184 rollback-only incomplete
  schema variants and51 preflight tests pass, including established-index reuse.
- [Browser verification](SLICE_010f3_WEB_VERIFICATION.md):full real Web/API workflow
  and retained010 upgrade/reconciliation pass. All15 protected table sets, both
  original cells and complete import/snapshot/storage count+SHA pairs are unchanged.
- [Performance](MOBILE_005_010f3_PERFORMANCE.md):the single Today pair and11 mobile
  hot-query plans remain valid. `final-source-evidence-reuse.json` verifies all four
  mobile hot files and19 Today files unchanged from the measured source, native
  sources unchanged from their reviewed pin, and production API/Web unchanged
  from the retained010 runtime. No unchanged benchmark is repeated.
- [Mobile round2](MOBILE_005_IMPLEMENTATION_REVIEW_2.md) and
  [migration round2](SLICE_010f3_IMPLEMENTATION_REVIEW_2.md) both conclude **READY**
  at the final tested source. All findings are closed; no third broad review ran.

`protected-resources-final-gates.json` confirms all74 protected artifacts, including
root `.env` and shared Web output, remain byte-identical and all four shared listener
PIDs are preserved. Native store/key preservation is established by the platform
records; `native-installed-inventory-corrected.json` adds presence evidence only.
Its first probe was inconclusive because the iOS simulator was shut down and
Android stores use `no_backup`; no inconclusive probe is counted as a pass.
`ios-round2-simulator-restored.json` confirms the simulator returned to shutdown.
Only inactive private compiler caches were removed when space was needed; products,
result bundles, evidence, installed stores and keys remain. Cleanup records include
`ios-module-cache-cleanup-final.json` and `sqlx-incremental-cleanup-round2.json`.

## Retained earlier attempts

- At `a52a1e6`, `final-check2.log` and `final-sqlx-prepare1.log` pass. These precede
  substantive review corrections and are superseded by the final gates above.
  `final-check1.log` fails Clippy on a needless test borrow, corrected by839155a.
- At `bc4af33`, `final-check3.log` fails with exit127 after992 Rust tests/five
  doctests because pnpm was absent from the runner PATH. Exposing the existing
  pinned version through a private shim fixes the runner without a global install.
  `final-check4.log` then passes, as does `final-sqlx-prepare2.log` with no cache diff.
- `final-check-db1.log` fails the historical activity upgrade fixture:28 pass,
  one fails,1,047 are not run. Its only old/new-row difference is the added
  `person.details_revision = 1`. The fixture now explicitly verifies that default
  alongside other derived revisions before comparing every pre-existing field;
  complete upgraded-row equality after startup/API reads remains. The reviewer
  accepted the two-line test-only correction. Its focused run passes in
  `historical-upgrade-details-revision1.log`,40.01s total /4.13s test. All three
  final gates were then rerun on the corrected source and pass.

No failed, incomplete or superseded run is used as final acceptance. Publication,
shared-development deployment, native distribution, live FUB/customer work,
workspace activation and physical-phone/calling work were not performed.

The final completion update changes eight Markdown files only. All 180 local
Markdown link targets exist and `git diff --check` passes. Application/native/DB
tests and benchmarks were not repeated for these documentation-only changes.
