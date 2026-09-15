# Slice 010e3 — Admit People newly observed after the original import

**APPROVED FOR IMPLEMENTATION — D-078, 2026-09-13.** The user accepted this
new admission contract after D-077 planning. 010e2 is released and complete;
its existing-People refresh contract remains unchanged. Read [AGENTS](../../AGENTS.md),
D-050/D-059/D-064/D-065/D-075–078, [010c](SLICE_010c.md),
[010e1](SLICE_010e1.md), [010e2](SLICE_010e2.md), its frozen
[contract](../tasks/SLICE_010e2_CONTRACT.md) and the current
[architecture baseline](../architecture/ARCHITECTURE_BASELINE.md).
The [brief](../tasks/SLICE_010e3_IMPL.md),
[planning review](../tasks/MOBILE_003_010e3_PLANNING_REVIEW.md) and
[parallel plan](../plans/MOBILE_003_010e3_PARALLEL_LAUNCH.md) own execution readiness.

## 1. Outcome and deliberate boundary

A current Organization admin chooses a sealed completed 010e1 report for its
completed original 010c import. The app previews new People it can safely add
from that report's newer retained capture. Explicit confirmation admits eligible
People once; other rows remain visibly excluded or held. The original workspace
binding and `migration_review` hold remain in force.

“New” means newly observed relative to the original imported source boundary,
not proof of when FUB created the record. Never use source created/updated dates
as identity or treat a report's representative metadata as executable evidence.
New People contain names, all qualified emails/phones and primary order, mapped
stage/assignee, source identity and retained provenance. The original confirmed
mapping choices are reused and revalidated; unknown/new mapping keys are held.

This rung does not repair original held records, import new stage/member choices,
merge matching contacts, refresh an already admitted Person, or modify any older
Person. It does not extend 010f1 tags/fields, 010f2 notes/tasks, 010d history capture/
import or 010e2 refresh to these new People. Those family extensions are a following
specification, with an explicit visible coverage gap here. Do not call these
People fully migrated or eligible for activation. No live FUB reader, customer
operation, Inquiry, communication, operational access or mobile access is added.

## 2. Declared shared changes

| Current contract | Proposed contract and reason | Compatibility / ownership |
|---|---|---|
| 010c only creates People in the initial empty workspace; 010e2 only updates successful original results | A separate retained-evidence admission command/worker in the bound review workspace | New domain/routes/state; no reopening original imports or changing terminal results |
| Shared source identity PK points only at original plan/manifest | Extend the same identity registry with an explicit admission origin, preserving its existing global key and tombstones | Add nullable admission references with exact new-row constraints; no independent duplicate identity namespace or rewrite of old identity rows |
| Original provenance/fact points to 010c result/plan | Separate admission provenance and an IDs-only admission fact/history kind | Add bounded admin reads and corresponding Person history/preview rendering; old original-provenance DTO/meaning stays unchanged |
| Existing review permit has no admission write scope | New private permit for one confirmed admission item and its new Person | Guard table/operation allowlist, all read/mutation fences and release preflight explicitly extended; old import/refresh permits unchanged |
| No admission capability in workload inventory | Add independent `fub-people-admission-v1` engine/read/write capability | API/worker and admin/migrator schema/launch inventory checked before later rollout/confirmation |

Implementation approval would own these declared changes. The exact schema/DTO
checkpoint is written before dependent Web work. It may choose simple names and
encodings within these bounds, but cannot broaden identity, mapping, coverage,
privacy or admission policy without an amendment.

## 3. Source qualification and exact absence

Bind trusted Organization, original parent/confirmed plan, workspace revision,
FUB account, original snapshot and final sequence, report ID/output revision,
newer snapshot/final sequence/capture interval, profile/parser versions, original
mapping choices and admission engine into immutable encrypted inputs and a digest.
The parent must remain completed and bound to this workspace. The report and
captures must still be supported, retained and readable with current keys.

Require exhausted People streams at both boundaries, and exhausted qualified
users/stages at the newer boundary. Preserve 010c's complete exact raw observation
qualification: correct Org/account/capture/representation/ordinal, successful
accepted nontruncated bytes, lossless duplicate-key-rejecting parser, exact positive
source ID (up to 128 decimal digits), and matching semantic HMACs. Identical repeats
collapse only within the same exact source identity; conflicting, rejected,
restricted, Trash, unrepresentable or unsupported evidence remains held.

The report's `newly_observed` label is a candidate index, not proof. Preparation
must independently requalify complete newer People observations and prove no
occurrence of that exact source ID in the original retained People boundary.
Build/reuse a qualified bounded identity index and walk source descriptors once;
never scan the original book for each candidate. Unknown/incomplete baseline
identity coverage cannot prove absence. If malformed or inaccessible baseline
evidence prevents that proof, pause/hold qualification with an explicit reason;
do not manufacture a new Person from uncertainty. Source timestamps, contact
matches, the report's representative evidence array and display prefixes are
never substitutes for this proof. Frozen warnings disclose unrelated-family gaps.

Every relevant report People group must be accounted for in the summary, including
unchanged, changed, absent, unresolved and previously held records. All raw input
traversal uses bounded pages/checkpoints, including pages whose records are all
excluded. Reaching zero eligible rows is a valid ready preview, with confirmation
disabled rather than a fabricated completed write run.

## 4. Eligibility, identity and materialization

| Observed condition | Result |
|---|---|
| Qualified newer Person, proven absent at original boundary, no registered source identity, inherited mappings valid | Eligible new admission |
| Present at original boundary, including an original hold or an omitted original result | Excluded original record; repair is a separate scope |
| Successful existing original import identity | Already imported; no native mutation |
| Successful identity from a prior admission | Already admitted; no refresh, new fact or baseline advance |
| Registered identity whose target/evidence is missing, erased or inconsistent | Held identity; never recreate or adopt a lookalike |
| Missing/uncertain baseline proof, conflicting newer evidence, unqualified mapping or unsupported native value | Whole-Person hold |
| Source disappeared or is Trash/restricted | Held/excluded with retained evidence; no deletion inference |

Use the existing `(Organization, source account, family, exact source ID)` primary
key in `migration_import_identity` as the single cross-run admission authority.
Add explicit nullable admission-run/item references for the new origin. Original
rows retain every value. For an admission row, family is People, import_id/plan_id
identify the original workspace parent, original manifest/mapping references are
null, and composite foreign keys bind admission/item/Org/parent/plan/target. The
origin must be distinguishable without guessing from missing old provenance.
No target FK/cascade may remove the durable identity marker on erasure.

An existing identity is successful only with its own matching immutable original
or admission result and existing same-Org target. The PK arbitrates concurrent
attempts at commit. A uniqueness failure rolls back all native writes before
classification from the actual current identity; a raw SQL conflict is never
reported as successful admission. Do not copy a newly admitted Person into a fake
010c result merely to satisfy a child reader. Old identity-reader code must be
inventoried for null-manifest assumptions and remain correct for original rows.

Allocate a prospective target UUID inside the server's immutable plan item. A
new Person and its contacts/facts/provenance/result/identity commit together. If
that UUID is unexpectedly occupied, hold the item; never update the occupant.
Two distinct source People with equal email/phone values remain separate People
under D-064. Normal intake matching is not invoked.

Names, nullable blanks, contactless named People, all-contact qualification,
normalization/deduplication within one Person, primary flags and deterministic
order exactly reuse 010c's lossless rules and its 2,048-byte indexed-value bound.
An unsupported nonempty contact holds the whole Person; preserve the raw value.
The newer source's names/contacts are initial values, not a clear operation on
an older Person. No contacts belonging to another Person may be inserted into
this item's ownership record or updated/deleted.

Resolve source stage/assignee through the original confirmed mappings only,
including their explicitly approved null/unassigned choices. A stage created by
010c resolves to that actual existing target; this does not authorize another
stage creation. Freeze/recheck exact stage identity/name and active member
identity/email at preview and commit. New source users/stages, ambiguous IDs,
invalidated targets or missing choices hold the Person. There is no fallback by
name/email, no new invitation and no mapping editor in 010e3.

Preserve source label/URL, creation/update values, communication flags and contact
metadata in encrypted erasable admission provenance with exact raw references,
plan/engine and both source boundaries. Initial native created_at is actual local
admission time; activity maxima remain null and there are zero Inquiries. Write
one IDs-only `person_admitted` fact, and existing initial stage/assignment facts
with migration reason, System actor, Migration origin, server commit time and
current authorized executor as on-behalf-of actor. No source text goes into facts.

Add `GET /api/people/{id}/admission-provenance` and its bounded field/contact pages
for current admins under the held-workspace read gate. Add a separate Person
profile card and typed history/preview rendering for the new fact. The original
`import-provenance` route retains its old response and 404 semantics; an authorized
not-found fallback may check the new endpoint, while 401/403 immediately clears
content. No new member or Operator raw-value access, outbound fetch or source
filter/Inquiry semantics is introduced.

## 5. Preview, confirmation, serialization and recovery

Proposed lifecycle: preparing → ready → queued/running → completed, with bounded
paused/retry and terminal cancellation. Plans are immutable after sealing; explicit
re-preview creates a successor. A ready plan expires after ten minutes. Frozen
summary includes total considered, eligible, already imported/admitted, excluded
original and held by reason, intended contact count and deferred-family coverage.

Confirmation supplies request ID, exact run/plan/revision/digest, eligible count,
and explicit acknowledgments for coverage gaps, original mappings, distinct-Person
creation despite shared contacts, and continued review hold. The server compares
the exact frozen values, current authority/workspace and fresh operator-owned
release report. Replays return the same durable receipt; changed request bodies
conflict. Re-preview is unavailable after confirmation. A no-eligible plan remains
read-only. No request may select a subset from a clipped or partially built plan.

Allow one active admission run per parent, including preparation/ready/paused.
Maintain an admission-owned nondecreasing confirmed source boundary. A genuinely
older or overlapping source interval is rejected after a newer admission boundary
has been confirmed; do not borrow or modify 010e2's refresh boundary. An exact same
report/boundary may be explicitly re-previewed after terminal cancellation to admit
only still-unclaimed People; prior settlements remain already admitted. This is a
new confirmed remainder plan, not reopening the cancelled run. Report identity,
output revision and capture interval/sequence must all match for this exception.
A failed unconfirmed preview cannot advance the confirmed boundary.

Use the established workspace → parent → admission lease/item → identity/target
→ membership/ledger lock order, consistently across confirm/cancel/retry/worker.
One new Person is the native transaction unit. Recheck active admin, workspace and
parent, immutable plan/source/mapping identity, item byte bound and fencing lease
immediately before commit. Scoped DB permits independently verify those bindings
and the exact target UUID. Cancellation or a successor lease fences stale writes.
010e2 can continue against original People; any shared parent lock is held only
for the bounded unit, not an entire run. No original/sibling state is rewritten.

The new permit admits INSERT only for the current item's Person/contacts/initial
facts/source identity and its owned records. Any derived revision updates are
limited to the existing schema-owned trigger mechanism for the newly created
Person. It is not an app-role UPDATE/DELETE permit for Person/contact or an ordinary
review-mode bypass. Original-import tokens remain contact INSERT-only; refresh
UPDATE/DELETE remains bound to 010e2's exact existing item. The coordinator owns
combined guard tests, including attempts to borrow any other lane's token/lease.

Preparation visits at most 50 raw descriptors per checkpoint and one qualified
raw capture at a time, with at most 16 MiB raw input per unit. One admitted
transaction is bounded by the existing 64 MiB
retained-unit ceiling. Oversized candidates are held before confirmation, never
truncated. Reserve owned capacity before preparation/commit; charge exact encrypted
plan/provenance/result/contact/receipt/checkpoint bytes once to the newer snapshot
and Org ledger, including non-executable evidence and the defined retained
identity/fact overhead. Freeze the existing ledger's unit definitions in the
contract; do not double-charge shared raw captures. Keep an O(1) prepared-byte
counter and bounded cancel reserve. Do not take/release a parent or sibling's
reservation; lowered current policy clamps admission before any native write.

Cancellation retains settlements, identities, provenance and review hold. Retry
requires the original initiator still be a current admin plus fresh release,
source/key and capacity checks; any current admin may inspect/cancel. Executor
adoption after initiator departure remains a later policy. Storage, lease,
authority, key/integrity or release failures roll back the unit, then record an
owned fenced pause. Crash after commit must recover from identity/result/receipt;
crash before commit may replay the unit. No unbounded retry or duplicate fact.

## 6. Bounded HTTP and review UI

Proposed namespace `/api/migrations/fub/people-admissions`. All routes use current
admin/session/CSRF/Org boundaries and `Cache-Control: no-store`, including denials.
Shared-contract implementation details freeze before dependent work.

| Route | Proposed behavior |
|---|---|
| POST / | `{request_id,report_id}` creates preparation; request replay is exact |
| GET / | Required parent_import_id, opaque cursor, at most 20 runs / 128 KiB |
| GET /{id} | State, current/sealed plan metadata, counts, warnings/actions; 128 KiB |
| GET /{id}/items | Closed disposition filter, opaque cursor, at most 50 / 256 KiB |
| GET /{id}/items/{item} | Bounded scalar projection/provenance and explicit truncation metadata; 128 KiB |
| GET /{id}/items/{item}/contacts | Complete traversal, at most 50 / 256 KiB |
| GET /{id}/items/{item}/fields/{field} | Allowlisted full text in UTF-8-safe fragments ≤16,384 bytes; response ≤128 KiB |
| POST /{id}/plans | request_id + expected plan revision; explicit re-preview |
| POST /{id}/confirm | Exact plan inputs/count/acknowledgments; queues once |
| POST /{id}/retry or /cancel | request_id + expected lifecycle revision |
| GET /{id}/results | Immutable outcomes/Person links, fixed upper traversal key; 50 / 256 KiB |

Use string UUIDs and decimal-string source IDs/counts/revisions; no JS number for
source identities. Cursors authenticate actor/Org/parent/run/endpoint/filter/page
limit and frozen plan/output identity. Seal preview pages atomically; building
pages cannot be read as complete. Old sealed previews remain inspectable without
being confirmable after supersession. Result traversal fixes an upper settlement
key; refreshing starts a new traversal. SQL ciphertext admission is bounded before
decryption, and the protected read barrier lasts through response assembly.

Manage → Migration gains “Add newly observed People.” Show new-to-import wording,
full intended values, inherited mapping targets, all exclusions, deferred child
families, stable progress/results and explicit confirmation. Visible prefixes
are never resubmitted as source. No optional source connection is required.
Never label admission results as fully reconciled or active. Identity/role/workspace
transitions abort requests, clear decrypted values and fence late responses.
Use existing bounded polling/backoff and stop at terminal states.

## 7. Required verification

1. Qualified new candidates from real retained captures/report/parent: full names,
   ordered contacts, inherited mappings, provenance/facts and null activity clocks.
   Shared contacts across distinct source IDs create distinct People; repeat IDs do not.
2. Report labels are not trusted: original-present held records, incomplete baseline,
   rejected/conflicting raw variants, huge exact IDs, wrong capture/ordinal/HMAC,
   unsupported shapes and invalidated mapping targets cannot create a Person.
3. Original and prior-admission identities, occupied prospective targets, erased
   targets/tombstones, two runs racing a key, replay and payload mismatch; never
   recreate or treat a foreign/unsupported identity as successful.
4. Exact-plan confirmation, expiry/re-preview races, monotonic boundary, cancelled
   partial run/remainder plan and already-admitted rows; original parent/report/
   refresh/child results and all existing native People remain byte-for-byte intact.
5. Atomic native+identity+fact+result+receipt+ledger writes, injected failures before/
   after commit, process takeover, cancellation, actor demotion, stale lease,
   missing keys/release report, lowered storage ceiling and exact charge-once proof.
6. Scoped permits: wrong Org/actor/parent/item, old import/refresh token reuse,
   direct app-role writes and attempts to update/delete older People/contacts fail.
   Operational Web/mobile/Operator and migration review barriers remain unchanged.
7. Complete paged Unicode/source/contact review and provenance, response byte caps,
   opaque cursor fencing, held/old plan reads and fixed settlement traversal. Actual
   production Web/API preview → confirm → reload → partial recovery at desktop/390px.
8. New admission history/provenance works without fabricated original result;
   old provenance/history remain equivalent. Metadata/activity/history/refresh
   families explicitly exclude these new People and display their coverage gap.
9. One 25k-People/50-member D-050 plan pass, including sparse eligible pages and
   dense shared contacts, without per-Person full scans. One relevant paired
   Person/read regression, final repository/live SQLx/DB and Web checks. At most
   two bounded review/fix rounds. No production capacity claim from laptop data.

Use synthetic isolated resources only after implementation approval. No code,
schema, source processing, runtime deployment or acceptance success is claimed here.

## D-090 recovery amendment

The accepted [010e6 recovery specification](../plans/SLICE_010e6_NEVER_IMPORTED_RECOVERY.md)
extends admission and follow-on qualification for explicitly anchored, successfully
created recovery People. Normal modes retain this specification's rules. Original
holds are immutable; recovery is a distinct admission mode with its own initial
approvals/provenance. Initial approval readers recognize recovery-owned evidence
before original fallback, with subsequent exact-key 010e5 repair precedence.
No fabricated original results or automatic family cascade is permitted.
