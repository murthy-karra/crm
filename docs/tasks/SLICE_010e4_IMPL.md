# Slice 010e4 — Execution brief

**APPROVED FOR IMPLEMENTATION — D-080.** The [specification](../specs/SLICE_010e4.md)
and admitted-People core-refresh contracts are accepted for isolated implementation.
[Coordination](../plans/MOBILE_004_010e4_PARALLEL_LAUNCH.md) and
[planning review](MOBILE_004_010e4_PLANNING_REVIEW.md) define prerequisites.

## Ownership and implementation order

Proposed branch `codex/migration-010e4`; one Terra high writer owns backend and
Web in one worktree. Own new `domain/migration/admitted_people_refresh*` modules,
small explicitly reviewed shared interpreter/mapping helper extractions, additive
feature schema, focused migration tests and new Web API/panel/tests. No mobile,
contact command or native edits. Supply shared guard/router/worker/release and
`.sqlx` patches to the coordinator for sequential integration.

1. Freeze `SLICE_010e4_CONTRACT.md`: terminal cohort, exact origin FKs and target
   identity, baseline decrypt inputs/contact ownership, closed outcomes/errors,
   all API fixtures/cursors/byte bounds, source watermark/remainder semantics,
   locks, release capability, private permit and retained-byte accounting.
   Inventory every table/reader affected; preserve 010e2 original-only constraints.
2. Implement source requalification and bounded cohort manifest/preview. Begin
   with successful admission results, including committed cancelled-run results;
   resolve report evidence regardless of its `new` labels. Never reconstruct B
   from current Person data. Freeze every mapping and complete contact collection.
3. Implement new admitted-refresh stores/typed execution and item permit. Reuse
   lossless parse/overlay/mapping rules without widening old original/admission
   permits or duplicating a privileged command path. Validate trigger interaction
   with Mobile 004's stage revision when both changes are integrated.
4. Implement explicit preview/confirm/retry/cancel/remainder, immutable settlements,
   lease fencing, per-cohort watermark, charge/transfer-once baseline ownership
   and actor/Org/action-bound request replay. Verify failure and race cases first.
5. Implement bounded Web origin/source/cohort selection and before/current/proposed
   values, clear/removal acknowledgements, held/excluded coverage and recovery.
   Keep old routes/buttons and source records intact. No dependent-family actions.
6. Run isolated retained-synthetic production-Web/API desktop and 390px acceptance;
   compare exact original/admission/sibling/native rowsets and byte-ledger units.

## Required checks and exit

Use every applicable criterion in spec §7. Include a second Org and admin, both
completed/cancelled admission cohorts, multiple successive source boundaries,
original People, same-contact distinct People and preexisting local edits.
Negative DB tests must exercise all old/new permit types, wrong lease/item/Org,
source tombstones, rollback and migration-review denial for ordinary mobile writes.
Reconcile cancellation/remainder with no duplicate facts or unreleased reservations.

Run changed backend/Web format/lint/type/unit/DB/API tests and actual browser
acceptance. Use one D-050 25k-Person plan pass for cohort lookup, sparse preview/
result pages and worker claim plus the relevant paired Person read. Coordinator
owns final sequential `scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db`
on the integrated source. Use scripts' actual current setup, isolated Cargo/Web
outputs and no simultaneous DB gates; repeat only affected checks after fixes.

Only this lane creates 010e4 migrations; unique versions are assigned at launch.
Do not edit applied migrations, add infrastructure/dependencies, touch shared dev
or invoke live FUB. Retained fixtures use separate review-mode API/database/Web
resources. No future QA port/database or migration timestamp is preclaimed here.

Record commands/source/runtime, exact acceptance counts, failure attempts,
query/byte checks and preservation evidence. At most two review/fix rounds.
Completion is admitted-core refresh only; metadata/activity/history, customer
readiness, physical-phone tests, activation and deployment remain separate.
