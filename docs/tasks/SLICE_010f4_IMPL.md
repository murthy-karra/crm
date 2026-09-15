# Slice 010f4 — Execution brief

**ACCEPTED — D-084, 2026-09-14.** The user approved implementation and isolated
synthetic verification. Both independent planning reviews returned READY. Current
implementation and acceptance evidence are recorded in
[implementation status](../tasks/MOBILE_006_010f4_IMPLEMENTATION_STATUS.md).
Publication/deployment and materially different policy remain separate.

[Accepted specification](../specs/SLICE_010f4.md),
[paired plan](../plans/MOBILE_006_010f4_PARALLEL_LAUNCH.md). Branch
`codex/migration-010f4` has one Terra high writer for backend then Web; the
coordinator owns integration and final gates.

## Ownership and execution

Own new `domain/migration/admitted_activity*` modules, assigned narrow original
activity identity/read/helper changes, feature/identity-owner migrations, focused
DB/API tests, separate API route module and Web API/panel/tests. No mobile metadata,
tag/field catalog or native client edits. Coordinator integrates shared workspace
guards, API/worker registration, preflight, grants, `.sqlx` and common navigation.
Only this lane creates 010f4 migrations, with unique versions assigned at launch.

1. Freeze `SLICE_010f4_CONTRACT.md`: exact source/report/cohort resolver and raw
   limits, original/admitted exclusive identity-owner constraints/FKs, native
   source-key equivalence, closed lifetime/receipt/error shapes, permit allowlist,
   counted retained columns, locks, paged routes/cursors and compatibility predicates.
   Include the existing `crm_activity_measure_row` owner lookup in the additive
   identity measurement/ledger adaptation, preserving original charges.
   Inventory every original identity reader/writer and bounded review join; the
   existing registry is already global, so no second claim registry/backfill.
2. Reuse qualified 010f2 source interpretation, HTML/time conversion and native
   equality helpers without changing their accepted policies. Add separate admission
   resolution and a bounded snapshot-wide index for all source variants. Freeze
   exact destinations, mappings, text output, source-only counts and target IDs.
3. Extend registry ownership additively, preserving original rows/FKs and source
   keys. Update original/admitted code to understand exclusive ownership under
   existing Org/native serialization. Verify original-owner reuse, conflicting
   variants/targets and retained deletion tombstones. Avoid weakening old rows'
   integrity merely to permit a nullable new-owner shape.
4. Implement typed INSERT-only permit, confirmation/replay, per-unit leases and
   atomic native/identity/result/byte settlement; explicit pause/retry/cancel and
   exact unprocessed successor. No original-child reopening or imported-row UPDATE.
5. Extend durable activity read-boundary detection, combined original/admitted
   activity revision and exact bounded provenance joins. Prove an admitted insert
   between pages invalidates the old cursor. Add actual schema/reader/writer capability checks, including already-running
   original unit attempts. Freeze compatible recovery semantics and prove zero-write
   confirmed cancellations still fence legacy readers.
6. Freeze Web fixtures before UI work. Implement cohort/source selection, role/
   kind/timezone mapping, exact note preview, acknowledged subsets, bounded results/
   source fields, retry/cancel/remainder and admitted-Person provenance.
7. Exercise real desktop/390px browser + owned synthetic API flow. Reconcile source,
   original/admission/metadata/native rows and every ledger owner before/after.
   Separate allowed native inserts/derived revisions from unintended changes.

## Required evidence

Map every F4 acceptance ID to fixtures/commands and outcomes. Include at least two
Orgs, current/demoted admins, a member, completed/cancelled admissions, an admission
remainder, same-admission/later/incomplete sources, equal/changed/deleted native
records and overlapping global source identities. Notes/detail and both task
streams must be represented by retained synthetic raw capture evidence, not
hand-built executable manifests that bypass source qualification.

Run focused Rust/DB/HTTP/contract/preflight and Web checks, real browser acceptance,
one D-050 paired baseline and new/changed hot SQL EXPLAINs. The paired plan provides
commands and owns final sequential integrated gates, isolated resources and native
compatibility checks. At most two review/fix rounds; blocking trust gaps are never
deferred as a pass. Write `SLICE_010f4_VERIFICATION.md` only with actual evidence.

Live FUB qualification remains deferred; source-free tests do not prove live API
coverage. Customer readiness, first activity lifetime, later refresh/repair/history,
activation and runtime deployment remain separate from this implementation brief.
