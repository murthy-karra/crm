# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-16.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Working constraints

- Branch `codex/010g1-family-refresh`, base `dd140b0`; latest preceding history reader milestone
  is **`94f4b88`**. One primary writer; the previously authorized single
  implementation reviewer remains reserved for the final review. Planning review
  READY, round 1. Preserve D-050's two review/fix rounds and final performance gate.
- Do not merge, push or deploy. Preserve unrelated `docs/prompts/MODEL_ROUTING.md`
  and `.lavish/`, the shared 010e5 runtime, local verified 010e6 and native stores.
- Common refresh HTTP commands/readers and history version reads are installed; the Web workflow remains pending. No refresh native
  executor is wired. This slice is not complete.

## Implemented foundation and preparation

- Typed retained-only Prepare and immutable mapping Plan revisions; authorized
  replay receipts; explicit scoped target snapshots; stable prospective IDs;
  bounded bundle/family/item/mapping summaries with authenticated scoped cursors.
- Frozen original/admitted/recovered cohorts, a shared metered core index and a
  separate history index. Complete source reconciliation precedes Person filtering;
  metadata/activity stream completeness is independently qualified. Shared source
  encryption and accounting retain the original payer across mapping revisions.
- Atomic capacity reservation/settlement, source-run/Organization ledgers, metered
  control capacity, guarded leases/checkpoints, immutable evidence and native
  proof foundations. Preparation uses the existing scheduler's bounded turns.
- Pure atomic Person metadata and note/task delta policies, positive ownership,
  local-state/revision protection, separate destructive-action counts and history
  correction policies. Original/admitted successful metadata/activity results now
  retain exact encrypted after-state. Read adapters authenticate first and prior
  refresh baselines, accepted scan boundaries and first-coverage prerequisites.
- History: retained full-source selection, original/admitted correction baselines,
  stable addition/current/correction proposals, diagnostics and observed/missing
  ownership walks. Typed history corrections now execute through guarded application writes.
- Activity: explicit retained role/kind/timezone conversion, persisted insert/
  update/current/held proposals, source traversal and original/admitted missing-
  ownership traversal. Source moves outside the cohort remain ownership holds.
  Native rows, global identities and baseline heads do not change in preparation.
- Metadata catalog: bounded mapping inventory; full occurrence field-name checks;
  database-folded choice collisions; exact registry and destination inspection;
  stable prospective IDs; native capacity/label and duplicate-target checks;
  all-option requirements for new choice fields. Typed admission now establishes
  compatible shared registry readiness; bounded worker catalog outcomes precede
  Person proposals.
- New bundles choose mapping representatives in capture/item/element order. Old
  bundles retain their original UUID cursor semantics and cannot qualify old tag
  choices; pre-name-index sources also require fresh bundles. Replanning does not
  rewrite an old shared source index.

## Current verified milestone

All three families now execute through the common bounded queue. History supports
exclusive refresh-owned initial facts and immutable correction versions, with
source/baseline reauthentication inside the final transaction. Result, typed fact,
current head, maintained counts, accounting and cursor settle atomically. Completed
and cancelled bundles release remaining control reservations. Scheduling remains
disconnected pending complete readiness inventory and final verification.

The history Web review shows current corrections and bounded prior-version pages
and metadata details. Authority changes clear private state; late replies are
ignored. Message bodies remain outside the history display contract.

Verified: 49 family-refresh database tests pass, including native history creation,
correction, later-bundle ownership, injected rollback/retry, erasure and exact byte
accounting (`/private/tmp/010g1-history-execution-family-db.log`). Preflight tests
pass 55/55 (`/private/tmp/010g1-history-preflight-tests.log`). Clippy with warnings
denied, formatting and diff checks pass. Web tests pass 18/18, typecheck and focused
lint pass (`/private/tmp/010g1-history-versions-ui-{tests,typecheck,lint}.log`).
Browser checks at 320, 640, 768, 1024, 1280 and 1536 pixels pass without overflow or
page errors (`/private/tmp/010g1-history-browser/checks.json`); mobile and desktop
screenshots were visually inspected. Temporary synthetic browser fixtures removed.

## Remaining implementation

1. Bootstrap legacy baselines only where independently provable; complete activity
   refresh provenance in native review readers.
2. Implement exact Remainder. Preserve frozen unfinished
   units, sources/mappings/targets and baseline heads; settled holds are not
   unfinished work. Complete cancellation/resume crash and capacity coverage.
3. Complete activity and metadata execution edge coverage,
   including source/native/head/member/catalog revalidation, atomic results/
   accounting/heads and after-state production. Preserve compatible reader capability inventories.
4. Finish bounded results, field fragments, proof and history-version readers;
   wire HTTP routes and the common Web review/confirm/progress workflow. Preserve
   existing mobile/Operator/read-path compatibility and review-workspace gates.
5. Run the authorized independent implementation review, required database/API/
   browser/final-tree checks and the single realistic paired-relative + EXPLAIN
   performance gate. Review source-only reporting and all-held/no-useful-work
   confirmation semantics across Person and catalog prerequisites.

## Evidence and isolation

Earlier detailed preparation and verification records are archived in
[Project history](../plans/PROJECT_HISTORY.md#010g1-preparation-checkpoint-details-through-fad4761--2026-09-16),
with links there to the foundation, source-walk and mapping checkpoints.

- Rust target `/private/tmp/crm-010g1-target-20260915`; intended Web output
  `/private/tmp/crm-010g1-web-dist-20260915`; synthetic DB
  `crm_010g1_schema_20260915`. Database tests run serially.
- No live FUB/customer processing. The linker emits the known large `__eh_frame`
  warning. Required final gates have not yet been completed; do not report 010g1 done.
