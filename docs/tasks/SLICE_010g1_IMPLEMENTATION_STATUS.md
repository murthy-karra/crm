# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-16.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Working constraints

- Branch `codex/010g1-family-refresh`, base `dd140b0`; latest preceding history reader milestone
  is **`9b03868`**. One primary writer; the previously authorized single
  implementation reviewer remains reserved for the final review. Planning review
  READY, round 1. Preserve D-050's two review/fix rounds and final performance gate.
- Do not merge, push or deploy. Preserve unrelated `docs/prompts/MODEL_ROUTING.md`
  and `.lavish/`, the shared 010e5 runtime, local verified 010e6 and native stores.
- Common refresh HTTP commands/readers, history version reads and the Web review workflow are installed; exact Remainder remains pending. No refresh native
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

Bounded field inventory and UTF-8 fragment routes now review frozen source,
before/after values, changes and mapping choices without exposing proof envelopes.
Source numeric values retain lossless canonical JSON (including integers beyond
JavaScript's safe range). Fragments are at most 16 KiB, summaries at most 4 KiB;
cursors bind the actor, workspace, bundle, plan, revision, item, endpoint and size.
Current scoped mapping-target suggestions remain subject to typed Plan validation.
Family summaries now include settled counts and retained/reserved/limit bytes.

Native activity review exposes separate refresh provenance for first refresh
owners and later updates. Its page revision includes refresh execution positions,
so new committed work invalidates old page series. Source-only coverage survives
late native holds. The bounded field Web viewer opens from native activity review
and clears private state on authority/revision changes.

Verified: all 49 family-refresh database tests pass
(`/private/tmp/010g1-review-apis-final-db.log`), including field/API/tenant/cursor
checks, exact long Unicode reconstruction, large canonical numbers, mapping
suggestions, activity provenance and source-only reporting. Family unit tests pass
42/42 (`/private/tmp/010g1-fields-unit-final.log`); an existing test-only readiness
initializer missing the family flag was fixed. Clippy with warnings denied passes
(`/private/tmp/010g1-review-apis-clippy.log`). Field/native activity Web tests pass
20/20; focused lint/typecheck pass. Six viewport checks and mobile/desktop visual
inspection pass (`/private/tmp/010g1-fields-browser/checks.json`). The common Web
panel now selects retained sources and families, edits scoped mappings, reviews
exact counts and fields, confirms a frozen request, tracks results, cancels and
resumes individual families. Uncertain responses replay the same request ID/body;
authority changes clear private state. Remainder, scheduler admission and final
gates remain.

Web verification: all 1,325 tests pass across 103 files
(`/private/tmp/010g1-workflow-all-web.log`); the final mapping/progress adjustment
passes all five focused workflow tests. Typecheck, focused lint and isolated
production build pass (`/private/tmp/010g1-workflow-{final-typecheck,final-lint,build}.log`).
The build reports the existing large-chunk warning (MigrationView is now 505 kB).
Combined preview and scrollable confirmation pass six viewport checks with no
page errors (`/private/tmp/010g1-workflow-browser/checks.json`); mobile confirmation
and desktop preview were visually inspected. Synthetic fixture files removed.

## Remaining implementation

1. Bootstrap legacy baselines only where independently provable; preserve all
   current first-owner and corrected-version reader compatibility.
2. Implement exact Remainder. Preserve frozen unfinished
   units, sources/mappings/targets and baseline heads; settled holds are not
   unfinished work. Complete cancellation/resume crash and capacity coverage.
3. Complete activity and metadata execution edge coverage,
   including source/native/head/member/catalog revalidation, atomic results/
   accounting/heads and after-state production. Preserve compatible reader capability inventories.
4. Add exact Remainder to the common Web workflow and complete scheduler/readiness
   admission. Preserve
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
