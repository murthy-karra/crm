# Slice 010f2 — Execution brief

**APPROVED FOR IMPLEMENTATION — D-068, 2026-09-11.** User request:
“Ok go ahead and implement 010f2.” The complete reviewed spec, policies, shared
contracts and isolated synthetic verification are authorized. D-067 records the
earlier HTML/date-only choices. Source access, commit/merge/push and deployment
remain outside scope. Baseline main `cd3b010`; deployed source `e36ce36`.

## Outcome and authoritative inputs

Implement [SLICE_010f2](../specs/SLICE_010f2.md), after full approval: one retained
notes/tasks child of completed 010c People, explicit mappings and source-only/held
acknowledgement, atomic insert/equality results, no local overwrite/resurrection,
and paged admin review under the unchanged durable workspace hold.

Read AGENTS, DECISION_LOG D-012/015/027/050/053/054/063–067, architecture baseline,
complete 010b/010c/010f1 specs and concrete contracts, 015/016 native models,
[code/source evidence](../research/SLICE_010f2_CODE_CONTRACTS.md), and prompt-library
05/06. Source is retained raw evidence, never clipped previews. Preserve D-067
and the reviewed defaults; do not resolve a failing fixture with a new lossy rule.

No live FUB/customer data, source writes, native mobile or Operator tools, native
task descriptions/recurrence, reply flattening, activation, ordinary-reader
pagination redesign, deployment, new service or storage provider. Both activity
families must exhaust the declared streams. Do not require a completed metadata
child or copy its People/custom-field stream assumptions.

## Ownership and order

After approval create one short-lived `codex/slice-010f2-activity-import` worktree
from then-current main. One primary implementation writer is the **sole database
migration owner** and owns shared module/test registrations. The coordinator owns
spec/status/decision pointers, evidence, integration and final-tree checks. Respect
the three-worktree maximum and do not overlap DB-backed runs in one checkout.

Use the `design` profile for this authorization/fidelity/reader boundary; preserve
the actual assigned model and record it. MODEL_ROUTING is advisory, not a model
switch or a reason to invent providers. Independent read-only review uses a
frozen tree. Bounded support can own a new pure extraction/conversion test file
or new Web component after its seam is frozen, never shared files concurrently.

| Step | Owned deliverable / dependency | Acceptance checkpoint |
|---|---|---|
| 1. Concrete contract | Freeze schema/DTOs/errors, source qualification and exact HTML/tzdb profiles, state/receipts/cursors, retained-byte inventory and account-qualified identities | A1–A5/A8/A12; all unresolved semantics returned to spec, not guessed |
| 2. Backend child | One additive migration, bounded raw extraction/conversion, immutable choices/plans/manifests, role/type/time mapping, typed note/task INSERT permit, worker/identity/results/accounting | A1–A9; no ordinary-command defaults or parent/sibling rewrite |
| 3. Bounded review and compatibility | Native paged admin queries, review-core representation, direct/HTTP legacy fail-closed guard, exclusive confirmation barrier, startup/preflight activity capability | A9/A10/A12; actual old-reader/confirm race, source-free review, cancelled boundary retained |
| 4. Backend review/fix | Focused unit/DB/HTTP checks and first independent bounded implementation review | Freeze complete client contract after accepted corrections |
| 5. Web integration | Activity panel/mappings/timezone/confirmation/results, paged review Person notes/tasks/full content; shared session/workspace fencing | A3–A5/A7/A10/A11; new route rather than incomplete legacy arrays |
| 6. Final verification | Second bounded integrated review/fix, final sequential gates, query plans + one paired reader check, production-Web synthetic walkthrough and exact native reconciliation | A1–A12; retained evidence, actual file/source hashes and cleanup proof |

No third implementation review round without the explicit D-050 exception.
Report remaining blockers instead of claiming completion. Planning review of this
draft is a separate handoff and does not prove implementation correctness.

## File and contract boundary

- New activity modules under `crm-app/src/domain/migration/` for extraction,
  conversions, plans/commands/store/worker/queries; narrow reuse of existing
  lossless parser, AEAD/HMAC, source boundaries, receipts and ledgers. Do not
  create a generic import framework or rewrite 010b/010c/010f1.
- Native note/task modules: private validated import operations, scoped lookup/
  equality checks and bounded native review queries. Ordinary command bodies,
  authorization/defaults and mutation semantics remain unchanged.
- `auth/workspace.rs`, parent begin/locking and Person history/task complete-read
  paths: only the new activity read boundary, exclusive first confirmation and
  compatible runtime/DB guard checks. New core-history path must not call an
  unbounded note/task query. Typed query failures map to the declared 409.
- Exactly one new versioned migration under `crm-api/migrations/`; composite
  tenant/source keys, noncascading identities, manifest INSERT permit, typed
  confirmation boundary, indexes/grants/triggers and startup-bound-state query.
  Do not edit applied SQL. `backend/.sqlx/` generated metadata is sole-owner work.
- New API activity routes and paged Person review routes; state/startup worker,
  current preflight/readiness types and `scripts/migration-release-preflight`
  only for the distinct activity capability. Preserve old readiness fields.
- Web new activity API/types/query keys/components plus MigrationView and
  PersonDetailView selection of the bounded review representation. Reuse UI_STYLE
  and applicable styling skill at implementation; no operator/native write path.
- New focused pure/source/DB/HTTP/guard/plan-collector tests; primary owns `all.rs`
  registrations. Reuse guarded synthetic QA server patterns, never shared data.
- Owned amendments/pointers: 002 Person reads, 015 §2/body read sites, 016 §2/reads,
  010b/010c/010f1 sibling accounting/compatibility, plus new concrete contract,
  verification record, erasure inventory and current status. Never relabel old
  JSON/checkpoint evidence as proof of the new implementation.

A bounded HTML parser and IANA timezone database are concrete requirements of
D-067. Inspect existing dependencies first; introduce only necessary parsing/tzdb
dependencies, pin them and freeze profile/version behavior. No browser execution,
network fetch or LLM conversion. Parser/output bounds must precede large allocation.

## Required checks

During implementation, run focused pure Rust/Web tests and scoped DB/HTTP suites
for A1–A10/A12. Verify actual tenant resources against random IDs; direct-domain
and DB permit negatives with positive controls; foreign/stale cursors and late
responses; raw/list/detail/gap identity; HTML/subject/Unicode output; explicit
timezone/DST; completion actor/time independence; account-qualified and legacy
source keys; local edits/tombstones; zero-eligible confirmation rejection and
pre-confirm replanning; explicit retry executor adoption; worker/lease/cancel/receipt races; concurrent
sibling admission; native-versus-retained byte measurements. Include log sentinels
in successful/rejected source and command bodies with positive controls.

After the two bounded reviews, run on the frozen final tree, sequentially:

```sh
./scripts/sqlx-prepare
./scripts/check
./scripts/check-db
```

Use repo-pinned Rust/Node/pnpm and documented isolated PostgreSQL/Centrifugo setup.
`check` is service-free; DB scripts need the authorized disposable fixture and
exclusive DB slot. Never reset/seed shared `crm_dev`, print environment secrets,
call FUB or claim a skipped command passed. Keep logs, exact counts and hashes.

Register a new opt-in collector (proposed `db_activity_import_plans`) using actual
new/changed application SQL, then retain successful EXPLAIN output explicitly:

```sh
cd backend
SQLX_OFFLINE=true cargo test -p crm-api --test all --features perf-harness --locked db_activity_import_plans:: -- --ignored --nocapture --test-threads=1
```

Use D-050's 25k-People/50-member book, report realistic activity density plus a
concentrated ≥500-full-size-note Person, rare/empty filters and page boundaries.
Prove ≤50/512-KiB pages and full traversal. Run **one paired operational Person
detail baseline/final check** for the added shared guard: same build/fixture/clock,
equal payloads, relative p95 bound max(25 ms,10%). If another shared reader changes,
document why and include it in that same run; do not add absolute laptop gates,
planner-toggle matrices or an unrelated capacity benchmark.

Finally run a synthetic real API with production Web, not mocks alone: prepare,
mapped/unmapped roles, plain/HTML/subject and source-only reply evidence, task kind/
date/DST previews, dirty choices, held subset confirmation, lost response replay,
native pages/full note/provenance, reload, cursor revision change, partial cancel,
storage pause/increase/explicit retry, source disconnect, ordinary-member hold and
admin demotion. Inspect desktop and 390px screenshots and named dialogs. Audit
exact native rows/times/actors and parent/sibling identities/results, source-call
counter zero, no unwanted Today/notification/communication work. Clean only
owned sessions/processes/containers/volumes after evidence preservation.

## Planning handoff

Planning is complete when this brief, complete specification and evidence have
independent review, findings are resolved/recorded and the user can approve the
concrete policies/contracts. This handoff is complete: independent review is READY,
and D-068 accepts the full specification and implementation under AGENTS §11/§16.
Commit/merge/push, deployment, live validation and activation remain separate scope decisions.
