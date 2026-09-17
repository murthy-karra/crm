# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-16.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Working constraints

- Branch `codex/010g1-family-refresh`; preceding common Web milestone `af19664`.
- One primary writer and one authorized independent implementation reviewer.
  Round 1 found two issues, both fixed with regressions. Round 2 closed without
  actionable findings at `12586c3`; the review budget is exhausted. Preserve
  D-050's one final paired gate.
- Do not merge, push or deploy. Preserve unrelated `docs/prompts/MODEL_ROUTING.md`
  and `.lavish/`, the shared runtime and native stores.

## Implemented

- Retained-only Prepare, immutable Plan revisions, scoped bounded summaries,
  mapping inventory/targets, exact field fragments, source accounting and frozen
  original/admitted/recovered cohorts. Shared core and separate history indexes
  remain encrypted, metered and resumable.
- Whole-Person metadata deltas, note/task updates and additions, immutable history
  corrections/current projections. Typed native permits revalidate authority,
  source/head/revision, mappings and complete state atomically with results and
  accounting. New first imports preserve explicit after-state.
- Legacy original/admitted activity reconstructs initial native state only from
  immutable global identity, successful manifest/result and decrypted native
  payload. Metadata reconstructs positive link/value writes and aliases, including
  admitted choice mappings and native decimal scale. Revision-install ledger
  evidence and immutable Person creation facts reject pre-revision/incomplete
  proof; complete state/revision comparison rejects local changes and ABA.
- Exact Remainder copies only frozen unfinished eligible units. It carries settled
  catalog prerequisites as non-executing, non-counted references, including held
  prerequisites, without retrying settled work. Interrupted copies and mixed-family
  source attempts remain in the same immutable lineage. Capacity, cursor, lease,
  replay and rollback guards cover bounded copy and resealing.
- Confirm/Cancel/Resume, durable revocation pauses and HTTP/Web Remainder. Confirm
  takes the exclusive compatibility barrier before membership/storage locks.
  Web permits an exact ready subset with a paused/preparing sibling; uncertain
  responses replay the same request ID/body and authority changes clear state.
- Preparation/execution alternate within the existing scheduler's bounded turns.
  Release admission verifies full family schema, functions, grants, triggers,
  indexes and native revision guards; preflight includes retained requirements.

## Current verification

- 40 focused family DB tests pass, including legacy paths, exact Remainder,
  mixed attempts, inherited held catalog dependencies, capacity/rollback/replay,
  barrier concurrency, tenant authority and release tamper checks:
  `/private/tmp/010g1-legacy-remainder-final-db.log` (332.48 s).
- Earlier full family run: 54 passed
  (`/private/tmp/010g1-remainder-verified-family.log`).
- Five common Web workflow tests pass, including ready-subset confirmation:
  `/private/tmp/010g1-review-fixes-web.log`.
- Release preflight tests: 56 passed. Earlier Remainder Clippy/typecheck/lint and
  seven-viewport inspection passed. Final-tree checks are running; no final pass
  is claimed yet. Full earlier evidence is in Project history.

## Remaining acceptance work

1. Finish the final 25k query-plan gate. The first run exposed full scans for
   Person-filtered previews, sparse result filters and remainder availability;
   migration 034 and the scalar summary query address those findings.
2. Complete SQLx schema/offline checks and the serial DB/API/compatibility suite,
   then the correctly configured single paired Person/Today gate.

The repository gate passed before the small lifecycle verification fix
(`ebe05b6`): 1,037 Rust and 1,327 Web tests, Clippy, production build and supporting
checks (`/private/tmp/010g1-final-check.log`). That fix has its own passing DB
regression. Production-Web acceptance passed at desktop/390px with real typed
API/worker persistence: mappings, destructive counts, local holds, uncertain
confirmation with byte-identical retry, partial cancellation, exact remainder,
reload, current/prior history versions and logout clearing. Evidence lives in
`/private/tmp/010g1-final-browser`; selector/fixture failures are retained rather
than silently discarded. No final package pass is claimed yet.

## Isolation

Rust target `/private/tmp/crm-010g1-target-20260915`; Web output
`/private/tmp/crm-010g1-web-dist-20260915`; synthetic schema database
`crm_010g1_schema_20260915`. DB tests run serially. No live source/customer work.
The linker reports its existing large `__eh_frame` warning; Web reports its
existing large-chunk warning. 010g1 is not yet complete.
