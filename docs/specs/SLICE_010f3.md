# Slice 010f3 — Tags and custom fields for admitted People

**APPROVED FOR IMPLEMENTATION — D-082, 2026-09-14.** Reviewed contracts and 010f3
assignment are accepted for the next metadata step in the
[admitted-People ladder](../plans/SLICE_010_ADMITTED_PEOPLE_LADDER.md).
[Brief](../tasks/SLICE_010f3_IMPL.md),
[paired plan](../plans/MOBILE_005_010f3_PARALLEL_LAUNCH.md),
[source review](../tasks/MOBILE_005_010f3_PLANNING_REVIEW.md).

Read AGENTS.md, D-015/D-050/D-051/D-058–066/D-073–081 and O-012/O-013/O-015;
010f1 owns unchanged value/normalization/capacity rules, 010e3 owns admission
identity and 010e4 establishes terminal-cohort/exact-remainder precedents.

## 1. Outcome and proposed boundary

An Organization admin selects one terminal admission cohort, qualifies its tags
and custom-field evidence, approves catalog mappings and confirms a bounded import
into those exact People. Results identify applied, already-present, held and
unsupported data. Original People and all source/provenance remain intact; the
workspace remains under administrator review.

One metadata family root per admission run. Successful settled results from a
terminal completed or cancelled admission are eligible; uncommitted items are
not. An admission remainder is its own cohort. Do not fabricate original-import
results, require admitted People in the original snapshot, or reopen 010f1's
one-child lifetime. Completed 010e4 refresh is not an eligibility prerequisite.

This is first metadata coverage for the cohort, not ongoing family refresh.
Confirmed source/mappings cannot later be replaced. Exact unprocessed remainder
after cancellation is allowed under §5; held/settled items are not repair candidates.
Notes/tasks, history, new People, core edits, later metadata updates, source deletion,
mapping repair, activation and live FUB/customer work remain following scopes.

## 2. Shared-contract declaration

| Current | Proposed contract and reason | Components / compatibility |
|---|---|---|
| 010f1 belongs to original completed import and original snapshot | Separate admission-metadata root/plan/attempt/results anchored to terminal admission identities | Additive Rust/DB/HTTP/Web; original child/results/provenance unchanged |
| Original metadata lookup joins original People results | Cohort resolver uses admission result + global People identity + live target | No contact/name matching, synthetic original rows or broadening of existing endpoint meanings |
| Original catalog identity registry references original import/plan/mapping rows | Add shared catalog claims spanning original and admitted imports, preserving original evidence | Additive registry/backfill and narrow original-writer claim integration; exact compatibility checkpoint required (§4) |
| Private metadata permit recognizes original child only | Separate admission/plan/attempt/lease/item scoped permit | Narrow guard/grant extension; no ordinary mobile/admin bypass and no Person/core writes |
| No admitted metadata routes or recovery capability | Separate `/api/migrations/fub/admitted-metadata-imports` family and `fub-admitted-metadata-v1`, activated atomically with shared-claim readiness before admitted preview | API/worker/admin/migrator launch inventory/preflight, original per-unit writer fence, bounded Web/provenance readers; incompatible launch and already-running paths fail closed |
| Snapshot ledger has existing source/import/refresh owners | Dedicated child reservations/results charge its selected snapshot and Org | Existing allowances and exact settlement retained; source references do not transfer ledger ownership |

Implementation approval would own these changes and the explicit original catalog
writer amendment. Exact schema/serialization/lock fixtures precede Web work.
Avoid a generic family framework or weakening existing original-only foreign keys.

## 3. Source and target qualification

Choose one completed/sealed 010e1 report for the same trusted Org/account/original
parent. Its newer core snapshot is the only source boundary for metadata. It may
be the report used by the admission, or a later report whose source capture begins
strictly after the admission's source capture completed. Do not join People from
one snapshot with definitions from another. Freeze source snapshot/final sequence,
capture interval, report revision and parser/engine versions before preview.

Independently require exhausted People and custom-field streams and qualified
retained representations; report completion alone does not prove field coverage.
Use all observations of every successful cohort source ID, regardless of the
report's new/changed/unchanged label. Missing People, incomplete evidence,
unsupported representations and inaccessible families remain counted gaps.
Unrelated capture gaps are disclosed, not converted to complete migration status.

Reuse 010f1's lossless parser, duplicate-key/variant handling, raw reference and
semantic-HMAC rules. Match definitions by qualified source ID and exact machine
name, never label/casefold/prefix heuristics. Tags are embedded People arrays,
not a full standalone source tag catalog. Preserve missing/null/empty distinctions,
unknown properties, raw ordinals and exact long values. Corrupt/unavailable keys
or source integrity failures pause; unsupported valid data is held.

Freeze a bounded manifest of terminal settled admission results. Revalidate Org,
account, original workspace binding, admission/result/item, global People identity
and live same-Org Person at preparation and commit. Tombstones, target loss or
disagreement hold the item and never adopt another target. New admissions cannot
expand a confirmed manifest. Unused qualified source definitions may be selected
explicitly as in 010f1; tags seen only on excluded People remain coverage evidence.

## 4. Catalog ownership and value safety

Reuse 010f1 §4 in full: explicit `create_matching`/`map_existing`/`hold`, approved
normalization, frozen destination targets, tag aliases, no arbitrary renaming,
no coercion/truncation/rounding, immutable field types, native catalog/Person limits,
source machine-key bounds and exact option mapping. D-051 tag deletion and D-058
archive rules remain unchanged. Mapping a field does not approve its options.

Propose one shared catalog claim by Org/source account/kind/source key, with the
same source-key version/normalization/HMAC purpose as 010f1. Existing metadata
identity rows remain immutable evidence with their original FKs. Backfill claims
from those rows transactionally, rejecting disagreement rather than choosing a
winner. An admitted claim references its own provenance; it cannot masquerade as
an original child. Scoped exact encrypted label/definition evidence must verify
hash equality; a digest match alone cannot merge identities.

Backfill references existing evidence rather than copying customer content. Its
new retained key/reference bytes must be admitted and charged once to the original
metadata owner's snapshot/Org ledger without rewriting historical results. Preflight
calculates that bound. No silent quota increase or transfer of old source charges
to a new cohort. Shared claims become authoritative only through this handover:

1. The first mutating admitted preparation request checks current admin/workspace,
   source account and compatible release readiness. Read-only list/provenance/GET
   requests never initiate backfill or retire writers. Exact replay reports the
   same completed readiness transition without another charge.
2. Fence incompatible original writers before enumerating/backfilling. Drain or
   reject already-claimed old units at the existing Org/metadata namespace barrier;
   a release inventory alone is insufficient. A unit that committed before the
   barrier is included; an old in-flight unit may not commit after readiness.
3. Serialize bounded enumeration, final catch-up against original identity rows,
   claim insertion/equality checks, exact owner accounting, durable readiness and
   `fub-admitted-metadata-v1` activation in one transaction under that same barrier.
   No original catalog commit can occur between final catch-up and activation.
   No admitted preview may treat missing claims as unclaimed until it reads the
   committed ready state. If the bounded handover cannot complete, fail closed.
4. After activation, every original/admitted catalog commit atomically consults/
   writes claims; incompatible original permits fail at the write boundary even
   if their worker started before handover. Capability/reader/recovery requirements
   are durable from this point, not postponed until the first admitted confirmation.

Insufficient allowance, incompatible fleet, collision, timeout or any failure leaves
readiness inactive, no authoritative partial registry and no partial backfill/owner
charge. Preserve original evidence and completed results. A successful readiness
transition remains active if the later preview fails or its unconfirmed root is
cancelled; disclose that compatible recovery is now required. Cancellation must
not downgrade the registry or reenable old writers. Freeze the guard/lock/accounting
implementation and idempotent rerun proof in the owned concrete contract.

Both original and admitted catalog writers must acquire/check the shared claim
inside the namespace transaction before committing native data. Existing claims
may only be reused for their same live compatible target. Reuse still requires
explicit approval in the new plan; inherited source identity is not blanket mapping
approval. Changed field type/key/choice semantics, conflicting approved target,
archived/deleted target or tombstone hold affected mappings and dependents. No
repoint, resurrection, option deletion or overwrite of existing source attribution.
Existing extra destination options and unrelated catalog entries remain unchanged.

Freeze each plan's global-claim and destination fingerprint. Recheck at confirmation
and each unit. A concurrent claim by another import can be recognized as already
present only if exact identity/evidence/approved target agree; otherwise pause for
replan before confirmation, or hold the affected mapping/dependents during execution.
Never bind an approved creation to a different target as a fallback.

For each Person's cells, retain preview baseline of link presence/value state and
the selected live target definition/options. At commit, a value absent in preview
may be set only if still absent. An equal preexisting value is `already_present`
and is never adopted as migration-owned content; a differing value is held.
Any baseline change after preview is held, including equal incoming values that
appeared meanwhile. A link removed since preview is held, not recreated. An
already-present link never gains new ownership. A matching new link/value written
by this exact operation is recovered by its receipt, not inferred from current data.

Missing/null source values never clear native data. If selected new tags plus
existing links exceed the per-Person cap, hold that Person's new tag set; independent
field values can proceed. No arbitrary first-N clipping. Native edits cannot run
in a review workspace, but these checks also protect concurrent import/catalog
operations and later compatible writers. Proposal freshness is baseline equality;
this does not introduce a historical metadata edit/repair protocol.

## 5. Plans, confirmation, execution and remainder

Before confirmation the root can replace ready plans with at most 50 mapping
patches per request, preserving bounded inherited choices. One building plan at
a time; ready plans expire after ten minutes. Changing selected source boundary
creates a fresh preparation revision and invalidates dependent choices/cursors.

Confirmation binds actor/request ID, root/plan/revision/digest, cohort manifest,
source boundary, workspace binding/revision and explicit counted-subset acceptance.
Require at least one executable catalog/link/value operation. Recheck authority,
dependencies, budget and release readiness under locks; persist queue and receipt
atomically. Resolve exact authorized receipt replay before new expiry/readiness
checks. Altered requests conflict; receipts never grant another actor access.

Use existing 60-second fenced lease pattern and current-admin checks at claim,
reclaim and every commit. Catalog dependencies settle first, one tag/field with
bounded options/option per unit, then one Person with independent eligible cells.
Commit native mutations, global claim, owned operation identity, encrypted result,
provenance, checkpoint and exact byte settlement together. No result without data
or data without result. Per-cell identity includes the successful admission Person
identity and source field/tag identity; never derive it from display values.

Retry explicitly resumes the same immutable attempt/plan. Key/integrity/authority/
capacity failures pause with closed reasons; restoring capacity never auto-resumes.
Cancel fences uncommitted work and preserves all committed catalogs/People/results.
No destructive undo or deleting shared catalog entries to simulate rollback.

A confirmed cancelled attempt may start one successor for the exact same source,
plan/mappings/cohort and only never-settled work. Previous successful units and
held terminal items are excluded. Dependencies reuse original committed catalog
results; lost/deleted targets hold dependents. One active attempt and one successor
per predecessor, enforced transactionally. Cancelled unconfirmed roots can be
reprepared because no source/mapping execution boundary was accepted. Completion
closes first coverage; it does not permit another source, repair or family refresh.

## 6. Authority, compatibility and bounded retention

Current same-Org admins alone read/prepare/confirm/retry/cancel. Anonymous 401,
member 403, foreign ID 404 and operational mutation review-hold 409 remain closed
and no-store, including outer middleware denial paths. Keep complete typed read
permits. A private child permit checks workspace/parent, admission, confirmed
plan/attempt/executor/lease and exact unit; only planned metadata mutations pass.
Old original/admission/refresh tokens gain no new powers. No operational publication,
Person-core mutation, Today/contact credit or customer-value history fact.

Use the existing workspace/membership/Org/snapshot-ledger/child ordering, then the
shared metadata namespace/claim and deterministic target/Person locks. Freeze one
concrete graph across original and admitted workers; bounded waits and no network
or inference in transactions. Mobile's profile lock must not introduce an inverse
path through metadata triggers. Coordinator owns shared guards/registration.

Add `fub-admitted-metadata-v1` to the established durable reader/writer/recovery
requirement during the atomic readiness handover in §4, before the first admitted
preview. Inventory API, in-process workers, admin,
migrator, container and scheduled launch/recovery paths, including original
metadata writers affected by shared claims. Backfill plus reader guards must
prevent incompatible original writers from bypassing claims during handover or
after capability activation, including workers already running before activation.
Prove that rejection, not just advertise a capability string.
Document compatible recovery; never reset workspace/child state to run an old binary.

Use existing snapshot/Org allowances and policy ceilings. Charge the selected
source snapshot for new work; retain dependencies on admission/original evidence
without charging their already retained bytes again. Source references pin evidence
needed for retry under current retention rules, without inventing a retention term.
Each owner settles/releases only its reservations. Reserve cancellation control
capacity and account for encrypted JSON overhead, claims, provenance, receipts,
source descriptors and checkpoints. Freeze counted-column inventory and prove
every unit's added retained bound ≤64 MiB before confirmation. Bound raw reads by
the source profile's qualified capture limit; 010f1's 4-MiB metadata reader must not
silently accept a larger page used by another feature. Oversize evidence is a
visible hold unless a separately reviewed bounded-reader change is included.

## 7. HTTP, Web and acceptance

Separate admitted-metadata routes offer root/list/detail, prepare, bounded mapping/
record/result/issue reads, confirm/retry/cancel and exact remainder. Bodies ≤64 KiB,
mapping patches/pages ≤50, display pages ≤512 KiB and summaries ≤128 KiB; full
fields use 010f1's 4–65,536 UTF-8-byte segments. Counters/source IDs are decimal
strings; reject unknown fields. Cursors bind Org/root/plan/attempt/revision/endpoint/
filter/field; no cursor or cached result can cross identity or source boundaries.

Web labels the cohort, admission origin, exact capture interval, source coverage,
mapping changes, normalization, subset counts, completed versus remaining work
and later-family gaps. Dirty choices must be applied/discarded before confirmation.
Uncertain responses replay the same request; retry is explicit; cancel explains
retained data; terminal/paused polling stops. Preserve bounded admin Person
provenance navigation and actor/Org/role fencing at desktop and 390px width.

Required synthetic evidence:

- Completed/cancelled admissions, remainder cohorts, uncommitted exclusions,
  foreign/erased/identity-mismatched People; original child/results unchanged.
- Same-admission and later qualified source; incomplete fields, missing People,
  all raw variants, malformed/unknown fields, large text and field-segment access.
- Original/admitted/multiple-cohort catalog reuse and conflicting races, shared
  claim backfill, stale targets, tag deletion/field archival, exact options and
  capacity; no identity repoint or resurrection.
- Old original worker attempts a catalog/identity commit between backfill and
  first admitted confirmation: the barrier includes a prior commit in catch-up or
  rejects the late commit. Test failed handover rollback, exact rerun accounting,
  nonauthoritative pre-ready absence and durable readiness after preview cancellation.
- Native-equal/different/absent values, changed preview baselines, removed links,
  null/missing distinctions, decimal/date/type/recurrence and full-tag-set limits.
- Lost-response replay, duplicate workers, lease expiry/cancel commit fences,
  retry and exact remainder after partial catalog and Person execution; accounting
  including exhausted budget, key failure and rollback with zero partial units.
- HTTP and direct typed guard negatives, source-free reads after disconnect,
  no-store outer denials, old writer/reader/recovery rejection and metadata-only
  permit boundaries. No live source operation is required or authorized.
- Real browser preview → mappings → confirm → cancel → remainder → reconciliation,
  reload/filter/provenance at desktop/390px; exact source/original/native rowset
  preservation except declared applied cells and legitimate derived revisions.
- D-050 realistic 25,000-Person metadata book: bounded indexed new hot queries,
  one paired Person/Today regression, byte-identical unaffected payloads; report
  storage fan-out estimates separately from logical quotas and physical disk.

Run brief/final gates; maximum two review/fix rounds. Completion means accounted
first metadata coverage, not cutover readiness. Policies/contracts in this draft
need review and acceptance before implementation.
