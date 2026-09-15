# Slice 010d3 — Planning review

**INDEPENDENT ROUND 2 — READY, 2026-09-15.** H3-R1-01 is closed with no
remaining blocking or nonblocking planning finding in the reviewed migration
envelope. The revised specification, brief and concrete contract preserve
`crm.history_reader=fub-history-timeline-v1`, add the independent exact
`crm.admitted_history_reader=fub-admitted-history-v1`, and require each from its
own durable predicate. A workspace with both predicates requires both stamps.

The corrected contract also requires original/new workers and recovery to acquire
the shared barrier and stamp the actual unit transaction connection. Its durable
database write/owner fence rejects a unit claimed before first confirmation before
any affected manifest, identity, fact, display, result, revision or ledger charge
can commit. Required evidence covers the paused-after-claim race, simultaneous
original/admitted predicates, pooled connection reuse, failed transactions and
cancelled/erased/zero-write roots. This closes the exact failure scenario found in
round 1 without changing business authority, source fidelity or history meaning.

Round-2 SHA-256 values are:

- Specification: `6a9511a6134260b4ab72a2f7273f8440c0d95da06fd1c5766b86ef13d9f2d653`.
- Brief: `c605b3819501a76e05091c98ca4a54709cc7c53da66607c9bef400e519ff924b`.
- Contract: `464dfe1e7dd18a3232d9612ec91c178baf1543e79e4f0b659525b205098aad79`.
- Aggregate in that order using UTF-8 path, NUL, exact bytes, NUL:
  `01cb24ee0a492e1bf0ac40ee9416d65e038bd67d3e7eb9cd5f5f210e4658e853`.

Both D-050 planning review rounds are complete. No third planning review is
authorized or needed. Implementation review and required executed evidence remain
future gates; this documentation review ran no services, databases, builds or tests.

## Round 1 record

**ROUND 1 — REVISION REQUIRED before migration code work.**

Reviewed on 2026-09-15 against source revision `d8367a7`, D-015/D-050,
D-069–D-072, D-079–D-086, the admitted-family ladder and the complete 010d3
specification, implementation brief and coordinated plan. The review covers
retained 010d1 capture qualification, admitted-Person resolution, shared history
identity/fact/display ownership, byte accounting, bounded review, old-worker/read
fences, cancellation/remainder and erasure. It did not run services, databases,
builds or tests and did not edit product contracts or application code.

The reviewed specification/brief/plan aggregate fingerprint is
`2f219e2a6f0dfb3a854d39a6a871d0696c09fd6923e5a398d9efd2edb6931eef`.

## Blocking finding

### H3-R1-01 — The admitted-history capability can collide with the original reader and does not yet close the original worker's transaction gap

**Classification:** P1 / TRUST / blocking.

**Failure scenario and evidence:** The draft requires the common database guard
and existing original worker to use `fub-admitted-history-v1`, but it does not
name an independent transaction-local setting or define how it composes with the
existing history capability. Current `workspace::read_check` sets
`crm.history_reader = fub-history-timeline-v1`, and the database rejects any
different value whenever an original history anchor exists
([workspace.rs](../../backend/crates/crm-app/src/auth/workspace.rs),
[20260922000001_fub_history_timeline.sql](../../backend/crates/crm-api/migrations/20260922000001_fub_history_timeline.sql)).
A single scalar cannot prove both capabilities. Replacing the old value would
break valid original-anchor reads; treating the new value as an alias would not
prove that a process understands both owner shapes.

The original worker also holds the shared workspace barrier only while claiming
a unit, commits that claim, and then starts a separate unit transaction through
`history_import_store::begin(..., false)`, which does not reacquire the shared
barrier
([history_import_worker.rs](../../backend/crates/crm-app/src/domain/migration/history_import_worker.rs),
[history_import_store.rs](../../backend/crates/crm-app/src/domain/migration/history_import_store.rs)).
First admitted confirmation can therefore take the exclusive barrier after an old
process claims work but before that process enters its apply transaction. A fence
checked only at claim or confirmation cannot reject this already-claimed old unit.
That violates the draft's H3-08 zero-write old-worker requirement and can leave an
original-only process operating after the exclusive-owner schema becomes durable.

**Required revision:** Freeze a separate capability key, recommended
`crm.admitted_history_reader = fub-admitted-history-v1`, while preserving
`crm.history_reader = fub-history-timeline-v1` byte-for-byte. Require each setting
independently from its own durable predicate: the original anchor requires the
original setting, and any confirmed admitted-history root, including cancelled,
erased or zero-write roots, requires the admitted setting.

The contract must also close the claim-to-unit interval. Current original and new
workers must set the admitted capability on the actual unit connection and enter
the shared barrier in that unit transaction. The database write permit/owner
guards must consult the durable admitted predicate and reject an unstamped old
unit before any affected manifest, display, identity, fact, result, revision or
charge can commit. Preparation, fresh confirmation, Resume, recovery and release
preflight must check the same complete schema/capability. Add fixtures for an old
worker paused after claim, first confirmation under the exclusive barrier, then
the old unit attempt; both original and admitted predicates together; pooled
connection reuse; failed transactions; cancelled/erased/zero-write roots; and
forward recovery. This is a compatible implementation correction under D-086 and
does not change business authority or history semantics.

## Verified contract checkpoints

These points are adequately specified at the planning level and must be made
concrete in `SLICE_010d3_CONTRACT.md`; they are not additional findings.

- **Capture/cohort qualification:** one terminal completed/cancelled admission
  result set and one same-parent/account `completed_with_gaps` capture whose start
  is strictly after the admission's newer core snapshot completed. The contract
  should freeze the exact admission results, capture revision/final sequence,
  stream terminal reconciliation and every parser/profile/key-purpose input.
- **Person resolution:** reauthenticate each advancing raw occurrence and recompute
  its capture-local `person-reference` HMAC from the exact successful admission
  `source_id`. Require one valid source Person reference and exact agreement with
  the observation/person-link evidence; ambiguous, group, nested, mismatched,
  missing or erased relationships remain held. Existing capture linkage labels
  stay immutable and are not authority.
- **Global identity and provenance:** keep
  `timeline-import-identity-v1` over the existing account/family/representation/
  source-record tuple. Freeze complete mutually exclusive original/admitted owner
  tuples for the registry, all three immutable fact tables and display reference,
  including the original-first and admitted-first already-present cases. An
  already-present result points to the surviving fact/identity/display owner and
  does not create a second display, adopt ownership or recharge old bytes.
- **Accounting:** retain 010d2's independent history-root and Organization ledger
  semantics. Count every new variable byte on its admitted owner, dispatch shared
  identity/fact/display changes by the exclusive owner tuple, leave original-owner
  charges unchanged, and do not copy or recharge 010d1 raw evidence. The exact
  inventory must include discarded preparation, receipts, cancellation capacity,
  remainder lineage and suppression/erasure deltas.
- **Remainder and erasure:** one successor may copy only the predecessor chain's
  never-settled units under the identical capture/cohort/interpretation/target
  boundary. Applied, already-present and held results remain settled; occurrence
  and excluded-coverage reconciliation carries forward without becoming native
  units. Person/identity erasure must route revision, count, display deletion and
  tombstone work through either owner shape, remove admitted encrypted derivatives
  and prevent a prepared or remainder unit from reconstructing erased display
  content.
- **Bounded display:** update external history joins, fact guards, revision/count
  triggers, provenance reads and erasure paths together. Preserve the current
  `person_admitted` native family, existing page/response bounds and one consistent
  counts/revision/rows snapshot without an `OR` join that duplicates overlap.

After H3-R1-01 is frozen in the contract and referenced by the brief/spec as the
required compatibility shape, a focused round-2 review can decide READY. No
additional product, privacy, authority or migration-fidelity decision is required
for this correction.
