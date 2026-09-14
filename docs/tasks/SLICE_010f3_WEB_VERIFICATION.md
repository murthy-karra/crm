# Slice 010f3 — Web and browser verification

**IN PROGRESS.** Coordinator-owned Web implementation uses the literal transport
checkpoint in `SLICE_010f3_CONTRACT.md`. Backend staged preparation and lifecycle
completion, real browser acceptance and final combined gates remain required.

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

Required next evidence: actual desktop and390px production-Web/API preview,
mappings, confirmation, partial cancellation, exact remainder, reconciliation,
source/field paging, reload and Person provenance; source/original/native/ledger
preservation. Deployment and release remain outside this implementation pass.
