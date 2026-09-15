# 010e5 — Concrete implementation contract

**Implementation checkpoint — D-088; planning review READY.** Implements the
[specification](../specs/SLICE_010e5.md) with one primary writer. Existing source
revision is `4a01fc0`. Original and admitted ownership remain distinct.

## 1. Integration shape

Use a narrowly scoped `people_mapping_repair` module shared by the two refresh
engines. An exhaustive Rust owner enum selects only the two fixed table families;
request input never supplies SQL identifiers. Keep each existing store's encryption,
receipts, reservations and baseline provenance. Reuse the two startup worker loops.

Additive migration: `20261006000001_fub_people_mapping_repair.sql`. Do not edit
previous migrations. Update DB guards, runtime capability evidence and SQLx cache
in this same implementation before making a repair confirmable.

## 2. Data and ownership

For each prefix `migration_people_refresh` and
`migration_admitted_people_refresh`, add equivalent owned structures:

- Root repair metadata: nullable source refresh/plan IDs and anchor kind,
  repair draft revision, frozen choices revision/digest, expected anchor lifecycle
  revision and candidate count. All-null means ordinary refresh. Composite FKs
  require the source plan to belong to the source refresh and same Organization;
  root validation enforces the same parent/account/cohort. Preserve existing engine
  identities and add explicit repair mode rather than pretending an original
  mapping became a repair mapping.
- Immutable choice rows: ID, refresh/Org, draft revision, kind, exact source-key
  HMAC, encrypted source-key/display/target snapshot, explicit target or unresolved
  disposition, approver, creation time. Unique per root/revision/kind/key.
  A bounded edit appends a new version; a sealed plan resolves the latest version
  at or before its frozen draft revision. Later choice edits cannot affect it.
- Immutable candidate rows: repair root/Org, anchored source refresh/plan/item,
  successful original/admission result and source Person identity. Composite FKs
  preserve the owner chain. Candidates derive from held items/results in a bounded
  worker pass; freeze count and digest before a ready plan can be confirmed.
- Item execution metadata: exact original/repair choice references for stage and
  assignment, frozen successful binding heads, source-key identity, existing
  baseline result/head version and native fingerprint. Enforce exclusive typed
  references: repair IDs never inhabit original-mapping FK columns.
- Immutable successful bindings: Person/source account/kind/key, owning
  root/plan/item/result, choice ID and prior binding. A per-Person/key mutable head
  points to the latest successful immutable binding, with a version for stale
  checks. Foreign keys and triggers enforce same Org, owner, Person and result.
  Held/unprocessed items cannot insert bindings or advance heads.
- New repair plan metadata includes mapping digest, selected candidate count and
  approval-only count. A genuine same-value approval stores a binding/result,
  advances approved provenance and emits no native transition fact.

All sensitive display/target snapshots are encrypted using the owning store and
purpose-specific authenticated context. Immutable mapping/candidate/binding rows
reject UPDATE/DELETE for the application role; designated draft/head/root columns
have narrow mutation guards. No history row contains display names or emails.

Old rows backfill to ordinary mode, without repair evidence. Change the original
same-boundary unique index to exclude repair roots; admitted refresh already
dropped that index in migration 20260930000002, which remains in force. Keep one active root per
existing cohort across BOTH modes. A successful repair resolves its anchored held
item; a later fresh mapping-held item remains eligible for explicit repair.

## 3. HTTP and lifecycle

Under each existing original/admitted refresh namespace:

- `POST /{source_id}/mapping-repairs`: strict request ID, report ID, expected
  source lifecycle revision and typed anchor (terminal results, or sealed preview
  plan ID/revision). Resolve parent/cohort/account server-side. Ready-preview
  replacement explicitly cancels the unconfirmed source root and creates the
  paused repair draft atomically; replay returns that same root. A cancellation
  failure rolls back replacement. Source evidence is never deleted.
- `GET /{id}/repair-mappings`: bounded authenticated cursor pages of source keys,
  retained labels, affected counts, current choices and exact qualified same-Org
  target options. Use full source evidence, not UI prefixes, to interpret a key.
- `POST /{id}/repair-mappings`: request ID, expected draft revision, up to 50
  typed choices. Body ≤128 KiB. A choice identifies a server-qualified source key
  and existing stage/member, explicit unassigned or leave-unresolved. Reject
  duplicates, unqualified keys, invalid targets and stale revisions. Append choices.
- Existing `POST /{id}/plans` seals repair choices then enters preparation. An
  unconfirmed ready repair may invalidate its plan and return to choice editing;
  preparing/confirmed roots reject edits. Ordinary re-preview keeps its old meaning.
- Repair confirmation binds exact ready plan/revision/digest, mapping digest,
  selected-candidate count, approval-only count, explicit-unassigned count (including
  unchanged assignments), and existing coverage/clear/removal
  acknowledgements. Regular confirmation must reject repair mode without those
  extra acknowledgements. Strict route DTOs keep ordinary request compatibility.
- Existing results/detail gain mode, source-anchor link, mapping approval summary
  and counts. Old results/held counts are immutable and link to subsequent repair
  attempts through new queries rather than being overwritten.

Unconfirmed repair roots begin with bounded candidate/key discovery preparation,
then enter `paused/awaiting_mapping_choices`; generic Retry
rejects this state in BOTH command paths. Confirmation may succeed with positive
approval-only count even when no native fields change. Zero updates and zero
approval-only outcomes remain an honest non-executable preview.

The discovery phase builds an immutable owner-specific `_repair_key` catalog
(root/Org/kind/key HMAC plus encrypted exact source-key display) and indexed
candidate-to-stage/assignee key references. Choice writes must reference catalog
entries; a client cannot invent a new source key. Discovery never applies CRM
updates or interprets half-entered choices. Include catalog envelopes and key
references in the owned storage ledger.

Initial implementation should use exact current source boundary or a qualifying
newer report under owner-specific rules. Same-boundary repair is explicit and
identified by anchor; normal retry must not reopen settled results. Cancel/remainder
preserves committed bindings and allows only exact unprocessed candidate replay.

## 4. Preparation and execution rules

Candidate selection is closed to the anchored mapping-held identities. Resolve
successful original/admitted identity through its existing chain and authenticate
the selected report/source observations. Process in existing bounded worker units;
never decrypt or load a whole book in one HTTP request or repeatedly scan the
original book for each candidate. A grouped choice affects only frozen candidates.

Source interpretation preserves raw omission/null distinctions and existing
stage-label/user/pond keys. Resolve an explicit candidate repair choice first,
then the Person's last successful binding for that exact key, then an original
qualified approval. A selected invalid binding holds instead of falling back.

All normal and repair settlements validate the exact typed binding and frozen
head/version, including already-current/approval-only results. Replace original
worker's any-mapping-for-target shortcut. Validate full current projection versus
baseline, exact contact row ownership and result/head version before a baseline
can advance, including no-op. Same-value local edits remain protected under the
existing ownership rules; a matching destination value alone proves no authority.

Use existing Organization workspace/serialization locking before owner roots,
plan/choice heads, baseline/Person/contact and target/membership/ledger rows. Freeze
the concrete lock order in code comments and use it in every command/worker path.
Require the latest source boundary on prepare/confirm/commit; original and admitted
normal paths participate in the same check. Detect concurrent confirmation or
binding/head changes rather than silently adopting another plan's result.

Business fields, native transition facts, approval bindings/head, baseline,
result/progress and reserved-byte settlement commit in one bounded transaction.
Recheck initiator, review binding, active target, exact lease and source/plan before
write settlement. No binding/fact duplication after lost-response replay.

## 5. Database fences and release evidence

Capability: `fub-people-mapping-repair-v1`. A durable Organization requirement is
installed no later than first repair confirmation, persists after cancellation
and is included in actual workload/readiness inventory. Require compatible
processes for draft operations as well; old HTTP clients may only use old shapes
on compatible normal resources. Reader stamps alone are not business write permits.

Enumerate and cover:

1. `crm_workspace_shared` and bounded raw/provenance readers, retaining all existing
   admitted-activity/history capability checks.
2. Root state/claim and plan-choice sealing, including normal workers on a repaired
   workspace; an old worker cannot pause/reclaim/reset a repair or its candidates.
3. Result/baseline/head settlement, including no-op paths with no business write.
4. Both private refresh business-write permit functions and new binding insertion.
5. Startup/runtime source capability manifest, `ReleaseReadiness`, release inventory,
   preflight/launch/confirm checks and compatibility tests. Evidence must cover
   actual API/worker/admin/migrator roles; no borrowed readiness from another family.

The additive migration makes normal legacy rows usable on compatible new builds.
An affected workspace cannot fall back to an old binary after confirmation.
Reject partial-schema/unsupported interpretation states before execution.

## 6. Storage and performance

Extend BOTH store `measured_bytes` paths for choices, encrypted candidate metadata,
binding evidence, digests, heads and repair receipts. Reuse owner reservations and
cancel allowance; transfer mutable head ownership without dropping immutable-row
charges. Measure byte changes before/after atomic units, including approval-only.
Record physical/deduplicated charge ownership and test rollback/retry/cancel.

Indexed access: root/cohort lifecycle, repair anchor item, root/revision/key choice,
plan candidate key, Person/account/kind/key binding head and result traversal.
Keep existing page/field bounds. D-050 requires realistic plans and paired affected
Person/Today reads; laptop absolute times are reports, not production capacity.

## 7. Verification checkpoint

Carry all five [planning review findings](SLICE_010e5_PLANNING_REVIEW.md) into
tests. In particular: all-held preview replacement, prior successful hold exclusion
versus fresh hold repair, wrong-source same-target mapping, no-op concurrent local
change, ordinary refresh retaining repair approval, generic retry choice bypass,
old claim/baseline write rejection and same-boundary completed-root repair.

Implementation evidence and exact schema/DTO adjustments belong here or the
verification record as code settles. The accepted spec governs if a concrete
detail would broaden authority or change customer-visible scope.

## Compatible implementation-review corrections

Cancelled confirmed repair Results may anchor a new draft for the exact unfinished
items of that frozen plan. The report and source boundary must match exactly.
Discovery copies only encountered source keys and the last choice at or before the
cancelled root's frozen revision into new immutable choice rows. Each copy records
its inherited choice/root, retains the original encrypted target snapshot, and is
charged to the new root. Settled items are excluded; the new draft still requires
preview and explicit confirmation. Subsequent edits append a new revision.

Sealed executable evidence also authenticates the retained source semantic HMAC.
Original execution rechecks the exact imported result, global source identity,
account, retained capture/ordinal and canonical source content before business or
no-op settlement. Database permit/result guards independently check source identity
and the retained capture descriptor. A lost identity holds the Person.

Existing mapping heads advance by conditional UPDATE against the frozen predecessor
and version; absent heads use INSERT. The old immutable binding remains attributed
to its original owner; only the mutable head's byte ownership transfers.
