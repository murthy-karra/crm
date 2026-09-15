# Slice 010d3 — Implementation brief

**IMPLEMENTATION ACCEPTED — D-086, 2026-09-15.** The user accepted both plans
and declared contracts for implementation and isolated synthetic verification.
Independent planning review is in progress before code work. Compatible review
corrections are owned work; materially different policy and publication/deployment
remain separate. [Current implementation status](../tasks/MOBILE_007_010d3_IMPLEMENTATION_STATUS.md)
owns progress and evidence; planning-era wording below does not limit D-086.

**DRAFT — D-085 planning only.** [Specification](../specs/SLICE_010d3.md) and
[paired plan](../plans/MOBILE_007_010d3_PLAN.md) govern scope.

Proposed `codex/admitted-history-010d3`; one primary writer owns new
`domain/migration/admitted_history*`, narrow original `history_import*` /
`history_review*` compatibility changes, assigned additive migration, API/tests
and new Web migration components. `domain/` means `backend/crates/crm-app/src/domain/`.
Coordinator applies shared workspace guard/grant/router/worker/preflight/navigation
patches and integrates `.sqlx` serially. No mobile/client or source downloader edits.

## 1. Contract checkpoint

Create `SLICE_010d3_CONTRACT.md` before implementation. Freeze exact root/plan/
manifest/attempt/result/remainder/receipt tables and composite FKs, owner shape
extensions on global history identity/facts/displays, closed enums, HMAC purposes,
wire/cursor fixtures, selected capture qualification and complete counted bytes.

Inventory every original history identity writer, fact/display/provenance reader,
charge trigger, erasure/suppression writer, startup/recovery capability and private
permit. Confirm actual current history families including `person_admitted`.
Specify workspace/membership/Org/ledger/root/plan/identity/Person ordering against
existing original/admitted import locks. Freeze deterministic shared identity
serialization, reservation size/lease durations and bounded waits from current
code. No generic family framework or second identity registry.

Before new owner rows can exist, preparation/confirmation must require compatible
schema and original/new writer support. Durable first confirmation installs the
common read/worker barrier. Distinguish trusted old-artifact retirement from checks
old binaries cannot perform. Checkpoint completion is not release permission.

## 2. Prepare and classify

Build bounded retained-only source/cohort qualification and per-occurrence index.
Use authenticated raw evidence, not lossy old linkage labels, and resolve exact
successful admission results. Preserve all equal/conflicting/invalid/excluded/held
outcomes; no source I/O and no source actor mapping to the executor. Preview pins
all dependencies, interpretation and count/byte bounds. H3-01/02/09 are required.

## 3. Apply, recover and preserve

Extend existing identity/typed fact ownership additively with mutually exclusive
FK tuples; preserve old rows, stable HMAC bytes, append-only checks and owner charges.
Implement private permit + atomic fact/display/result/revision/settlement, exact
replay, fenced lease, cancel/resume/remainder and explicit executor adoption.
Test both orders of overlap with original history import, same-target equality,
variant/target mismatch and tombstone/missing-fact rejection. H3-03–06 required.

## 4. Readers, release fences and Web

Extend bounded existing Person timeline/provenance readers; maintain one consistent
read revision/count/row snapshot and no duplicate owner joins. Add capability,
actual-connection stamps, old complete-reader denial and readiness/recovery
inventory through coordinator-owned shared files. Test at confirmation with zero
facts and after cancellation/erasure. H3-07/08 required.

Add source-free admin root selection, qualified capture preview, explicit coverage
acknowledgement, confirm/progress/cancel/resume/exact remainder and paged results.
Use frozen DTOs and existing product styling, with no body/media exposure.
H3-09/10 require actual desktop/390px API-driven journeys.

## 5. Final evidence

Run focused contracts/DB/authority/fidelity/recovery/preservation/reader tests,
Web checks and preflight tests. Join the coordinator's serialized final DB/SQLx/
repository and paired performance gates rather than duplicating benchmarks.
Record H3 IDs, actual runner/source/commands/failures and evidence paths. Keep
original attempts visible. Required independent review remains pending; at most
two review/fix rounds under D-050. No runtime, live-source, activation or Git
publication task follows from planning.
