# Slice 010f3 — Web and browser verification

**BROWSER ACCEPTANCE PASSES.** Coordinator-owned Web implementation uses the
literal transport checkpoint in `SLICE_010f3_CONTRACT.md`. Final migration fidelity/
size-bound checks, combined repository gates and independent review remain required.

## Implemented Web scope

The admitted panel selects a completed original import, terminal admission cohort
and retained source report through bounded existing lists. It shows the selected
and frozen capture boundaries, preparation progress, source coverage and cohort
counts; reuses mapping/target/alias/full-field and reconciliation widgets through
an explicit admitted reader adapter; and supports counted confirmation, immutable
uncertain-request replay, explicit retry/cancel and exact remainder review.
Source changes and unsaved mapping choices prevent confirmation. Ready-plan expiry
is visible. The new Person provenance reader has its own route and cache scope.
Migration and Person mounts and outer no-store/admin-read registration are owned
coordinator changes. Original metadata readers keep their default adapter.

## Executed focused checks

Runner: coordinator in `/Users/karrad/projects/crm`; private evidence root
`/private/tmp/crm-mobile005-010f3/integration`.

- `cargo check -p crm-api --test all --features perf-harness --locked` through
  the serial private gate helper passed for new browser harness `6f4e6a3`.
  `metadata-ui-harness-check.log`, 98.97 seconds. This compiles the harness;
  it is not evidence of a completed browser journey.
- `pnpm --dir web run typecheck` passed in `admitted-web-typecheck1.log` and,
  after the complete panel/source/remainder tests, `admitted-web-typecheck2.log`.
- Focused new admitted panel/provenance plus existing metadata widget tests:
  64 passed in `admitted-web-tests1.log` (7 matched files, 2.16 seconds).
- Migration/Person view regression initially completed 129 assertions but had
  one unhandled rejection: the Person test mock returned the unrelated Person
  response for the newly added provenance route. The mock now supplies that
  route's exact empty page. This initial gate is **failed**, retained in
  `admitted-web-views.log`.
- The corrected Person view plus admitted panel (including source replacement,
  expired plan and exact uncertain remainder tests): 139 passed, zero unhandled
  errors, in `admitted-web-tests2.log` (4.93 seconds).
- `pnpm --dir web run lint` passed in `admitted-web-lint2.log` after formatting
  the new panels with the existing lint configuration. No dependency changed.
- `git diff --check` passed. The workspace HTTP formatting check first displayed
  the newly added multiline path expression; applying normal Rust formatting
  resolves that formatting-only difference.

## Browser harness and remaining acceptance

`db_admitted_metadata_ui_fixture.rs` is compiled only with the opt-in
`perf-harness` feature and an explicit ignored test name. It binds API3104 before
migrating the guarded loopback `crm_010f3_qa` database. Synthetic seeding requires
an empty database and uses genuine retained capture, original import and admission
commands. The fixture has 56 admitted People, all four field types, more than one
mapping/record page, long raw evidence and explicit equal/different native cells.
Only worker scheduling is controlled; real typed workers and commands apply data.
Private worker-unit grants allow a partial cancellation and exact remainder to be
observed through the production Web/API. It does not use a live source reader.

At Web/API source `b90c416`, the private production build passed in
`metadata-ui/web-build1.log`. The seeded immutable test binary serves API3104;
private `runtime.json` records its PID, source and SHA-256. The first launcher
checked the incorrect `/health` path and stopped only its own process; it resumed
the existing seeded database using `/internal/ready` without reseeding.

The first browser script waited for an incorrect login route and failed before
the journey assertion. Correcting it to the actual `/api/session` produced a pass:
`metadata-ui/browser-start2.log` verifies genuine login, completed parent and
available admission cohort, zero page errors, and document/viewport width390px.
Desktop and phone selector screenshots are retained and visually inspected.
This is initial navigation evidence; it does not assert a completed import.

## Completed retained-fixture browser journey

The same preserved fixture continued without reseeding. Immutable API source
`5cc63a60656a` handled preparation, mappings, confirmation and partial cancellation;
`d590e6d82703` handled the exact successor and reconciliation. Their binaries,
SHA-256 and runtime PIDs are retained in `metadata-ui/build-*.json` and
`runtime-*.json`. The production Web source is `66b6ba7`; its131 asset hashes are
in `metadata-ui/web-artifacts-66b6ba7.json`. Builds passed in
`web-build-option-label.log` and `web-build-review-provenance.log`.

Runner: `node /private/tmp/crm-mobile005-010f3/integration/metadata-ui/browser-journey.cjs
PHASE PREFIX`. Every row below has a passing `PREFIX-PHASE.log` and matching
`PREFIX-PHASE-evidence.json` under `metadata-ui`, with no browser page errors.

| Phase | Prefix | Verified behavior |
|---|---|---|
| `preview` | `transport5` | Real retained report/cohort, field and alias pages50+6, record pages38+18 within512KiB, all56 distinct records, full long Japanese source inspection, desktop/390px layouts |
| `confirm` | `transport5` | Three acknowledgments, actual committed response deliberately dropped, exact same request/receipt replay |
| `partial` | `transport7` | Catalog work completed, exactly4 People settled, controlled worker scheduling |
| `cancel` | `transport7` | Actual cancellation retains settled work and offers52 remaining People |
| `remainder` | `transport7` | One automatically confirmed successor on the unchanged source boundary |
| `reconcile` | `transport8` | Remaining52 complete, result filtering/reload, admin Person provenance and source fields, desktop/390px layouts |

Mappings were applied through real controls: existing Text/Number fields, matching
tag/Date/Choice creation, and explicit North/South options. The ready revision3
replaces the initial immutable plan. Confirmation request
`764633e5-9a2d-41b3-84a1-bd4c9c8c389d` retains request SHA-256
`fe0ac3dda3340278c7e6da999ca921f08f7889303de23d5677286f06d25691a2`.
The private worker records61 steps before cancellation and384 after completion;
both executable stages record zero source-reader calls. The persisted step count
survives the API restart, so restarting cannot grant the earlier budget again.

## Browser-discovered corrections and retained failures

- `transport1-preview` failed on a harness locator scoped twice to the mappings
  container. Correcting the relative row locator fixed that test navigation.
- `transport2-preview` exposed indistinguishable option labels: retained option
  source included both its parent label and exact choice. Web `b962b44` prioritizes
  the choice.34 focused mapping/admitted-panel tests and lint pass in
  `admitted-option-label-tests.log` / `admitted-option-label-lint.log`.
- `transport3-preview` exposed a harness rendering race; `transport4-preview`
  assumed50 records per page despite the512KiB limit. The runner waits for the
  current page and traverses actual cursors, validating all56 unique records.
- `transport5-partial` / `transport6-partial` tried to reselect the source after
  confirmation locked it. Subsequent phases retain that source and completed state.
- `transport7-reconcile` reached completion/reload but exposed a missing mount in
  the actual admin review screen. Web `66b6ba7` mounts admitted provenance in
  `PersonActivityReview`;17 component and120 Person-view tests plus lint pass in
  `admitted-review-provenance-tests.log`, `admitted-review-person-view-tests.log`
  and `admitted-review-provenance-lint.log`.

Failed logs and screenshots remain retained and are not counted as passes. The
successful phases reuse the same persisted plans, receipts and native data.
Desktop/phone preview, completion and provenance screenshots were visually checked;
the document width remains390px and large evidence stays inside its scroll area.

## Preservation and reconciliation

`preservation.py before|after` compares canonical complete rowsets from15 tables.
`preservation-before.json` and `preservation-after.json` match exactly for retained
captures/records/streams, original imports/plans/results, original metadata,
admission/results, People, contacts, notes and tasks. Both existing native cells
are byte-for-byte unchanged; total native values are222 after permitted additions.

Read-only SQL reconciliation in `reconciliation-ledgers.json` verifies two roots:
one cancelled and one completed;56 Person results for56 distinct People; seven
catalog claims; zero successor catalog results; one source boundary; and no
remaining reservation rows or reserved bytes. Final native counts are56 tag links,
four custom fields and two options. The roots retain1,985,232 and1,102,111 logical
bytes respectively; these amounts are not physical disk measurements.

Final source-fidelity/size-bound integration, shared gates and independent review
remain separate. Deployment and release remain outside this implementation pass.
