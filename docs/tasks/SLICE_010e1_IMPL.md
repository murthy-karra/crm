# Slice 010e1 — Execution brief

**APPROVED FOR IMPLEMENTATION — D-075.** Mobile 001 is already approved under
D-074. Implement the reviewed [specification](../specs/SLICE_010e1.md) and required
isolated synthetic verification within the coordinated launch. No worktree or
implementation completion is claimed by this brief.
The [coordinated launch plan](../plans/MOBILE_MIGRATION_PARALLEL_LAUNCH.md)
assigns shared-file ownership and the three-worktree schedule.

## Objective and ownership

Deliver a durable admin report comparing the original imported core snapshot
with a newer retained core capture. It supplies evidence for future delta/repair;
it changes no native CRM data and does not activate the review workspace.

One primary writer owns branch `codex/migration-010e1` and one
short-lived worktree. The migration lane may start alongside the mobile backend
foundation under D-075. Once that foundation integrates,
iOS and Android join this migration lane within the three-worktree cap.

Owned files: new `backend/crates/crm-app/src/domain/migration/core_change_*`
modules; new `crm-api` report routes and `db_core_change_*` tests; the additive
010e1 schema migration; `web/src/api/coreChangeReports.ts`, a new migration report
component and its tests; assigned verification evidence. Only this lane creates
010e1 database migrations. Allocate its unique migration timestamp with the
coordinator before writing; Mobile 001's separate migration remains backend-owned.

Coordinator alone edits shared module/router/worker registration, AppState,
workspace response headers, preflight/inventory wiring, `.sqlx`, root scripts,
navigation integration and shared status/decision/spec documents during parallel
work. Submit precise patches for those seams; the coordinator integrates them
sequentially. This lane does not edit native folders, mobile DTOs, note/task
commands, revision triggers or mobile reconciliation. A demonstrated core worker
handoff fix is migration-owned after its exact file boundary is announced.

## Execution order

1. Read AGENTS, decisions, approved spec, 010b/f1/f2 source contracts and current
   report/collector seams. Freeze concrete tables/DTOs/errors, canonical engine,
   counting precedence, bounded aggregation, cursor encoding, byte ledger,
   reservation/cancellation ownership, lease lock order and compatibility inventory.
   Preserve policy; do not invent a deletion or source-field filter contract.
2. Build a synthetic original/newer pair with original completed 010c binding.
   Exercise existing full recapture, including one independent-process handoff
   reproduction. Avoid replacing a missing source contract with guessed fixtures.
   Synthetic records are labelled synthetic and include explicit coverage gaps.
3. Implement additive report persistence, typed lifecycle commands, raw evidence
   qualification and resumable comparator. Freeze source observations and never
   consume clipped destination-preview data or current credentials.
4. Implement bounded HTTP and Web review with closed change categories, honest
   absence/scope reporting, role/context fences and no new body/value exposure.
   Integrate shared registrations only through the coordinator.
5. Verify important failures and tenant boundaries using actual isolated PostgreSQL
   and application routes. Assert immutable parent/business/workspace state before
   and after report completion, cancellation and recovery.
6. Perform one independent bounded review/fix round, required final gates and one
   D-050 representative plan pass. A second round is only for concrete fixes;
   do not launch repeated broad audits after those checks pass.

## Required checks and completion evidence

- Pure interpretation: exact/unknown-field equality, invalid IDs, missing/null,
  all-observation conflicts, note representation separation and task partition moves.
- DB/API: request replay/body binding, current admin/Org/account/workspace guards,
  frozen boundaries, corrupt evidence, scope change, incomplete enumeration,
  cancellation/resume, lease takeover/stale response and exact storage accounting.
  Prove that partial rows stay unpublished and completed pages/counts share one
  immutable output revision while workers progress, cancel or retry.
- Performance: keyset pages and bounded comparator plans at 25k People plus a
  representative realistic notes/tasks distribution; prove no repeated whole-book
  scans or unbounded source-ID grouping hidden behind a small response page.
- Web: eligible snapshot selection, counted results/filter pages, inaccessible/
  not-seen/unresolved explanation, cancellation/recovery and late-response fencing;
  one real API walkthrough on desktop and a narrow viewport.
- Final: formatting/static analysis, relevant unit/contract tests, required Rust/
  SQLx/DB and Web gates, changed preflight tests and independent review findings.

Use separate test databases, ports, target directories and evidence paths. Worktrees
do not isolate services. Serialize `check-db` on a shared PostgreSQL server and
coordinator-owned `.sqlx` generation. Never run shared `dev-bootstrap`/volume resets
or modify the user's development stack to obtain a passing test.

Exit only with specification-to-evidence mapping, reconciled report counts and
zero business writes, actual commands/results, resolved review findings and
explicit source/physical/runtime limits. Commit/merge/push, cleanup and deployment
require their own authorized concrete integration step; no live FUB call or
customer-data processing is authorized by this implementation brief.
