# 010g1 — Combined migration family refresh

**ACCEPTED FOR IMPLEMENTATION — D-092, 2026-09-15.** User explicitly approved
P1–P4, all declared shared contracts, staged implementation and the single
reviewer through Lavish Send & End. Deployment remains deferred. 010g1 assignment
is accepted. Independent planning review is READY, round 1:
[review](../tasks/SLICE_010g1_PLANNING_REVIEW.md). Proposed wording below is retained
as design history; D-092 authorizes compatible concrete implementation detail.

Base: local main `dd140b0`, including verified 010e6 `2a4c207`. Shared development
remains 010e5. No source fetch, customer processing, activation or deployment.

## 1. Outcome

An Organization admin uses one **Refresh migrated records** workflow to select
retained source evidence, inspect additions/changes/conflicts across three family
groups, confirm a counted subset, and track independent durable progress.

A family is one group of related records: metadata (tags/custom fields), activity
(notes/tasks), or history (external event/call/text metadata). A capture is saved
source evidence collected over an interval. A baseline is the exact last verified
value the migration wrote or recognized, with its ownership and native revision.

The workflow covers successful original People, later admissions and 010e6
recoveries belonging to one completed original import. Each successful terminal
admission, including partial cancelled admissions, remains an exact cohort;
freeze successful result IDs when preparing. No new Person creation or identity
repair occurs. Newly admitted People cannot silently join an approved bundle.

Combine the user experience and delivery, while retaining family-specific source
qualification, checkpoints, permissions and byte accounting. One confirmation
queues approved family plans atomically; native writes then commit in small
independent units. It is not a whole-migration rollback transaction. Successful
work remains if another family pauses or the admin cancels.

## 2. Inspected current behavior and amendments

- `metadata_worker.rs` / `admitted_metadata_worker.rs` implement first coverage,
  explicit catalog mapping and conservative value checks. 010f1/010f3 have fixed
  child/root lifetimes; they do not maintain subsequent source-value baselines.
- `activity_worker.rs` / `admitted_activity_worker.rs` preserve globally qualified
  note/task identities. Different native content, completion/reopen or assignment
  is held; the import permit authorizes INSERT, not general UPDATE (010f2 §5).
- `history_import_worker.rs` / `admitted_history_worker.rs` share the stable
  `timeline-import-identity-v1` registry. A changed canonical record is held;
  the existing three typed facts and their original ownership are immutable.
- `people_refresh.rs` / admitted core refresh provide precedents for frozen
  previews, local-change holds, receipts, explicit removals and exact remainders.
  They do not authorize family updates.
- `history_review.rs`, `activity_review.rs` and Person readers provide bounded
  review with revisions. `MigrationView.vue` currently presents family panels.

This package explicitly amends 010f1/010f3 first-only restrictions through new
refresh ownership; 010f2/010f4 no-UPDATE restrictions through exact private refresh
permits; and 010d2/010d3 changed-variant handling through append-only correction
versions. Old roots/plans/results/identity owners are never reopened or rewritten.
D-015 fix-forward history, D-053 notes, D-058 field kinds, D-064 review-only use,
D-067 HTML/timezone rules and D-050 verification limits remain in force.

## 3. Proposed product policies — approval required

### P1. Protect local work; no force-overwrite button

Use three-way comparison: last migration baseline B, current CRM C, newer
qualified source transformed through approved mappings S. Require authenticated
baseline provenance and exact live source identity first.

- If C equals B and native revision/ownership is unchanged, apply qualified S.
- If C equals S with no unaccounted native mutation, classify already current and
  advance the verified baseline with a result. Equality alone never acquires
  ownership of a local record or erases evidence of an intervening local change.
- Otherwise hold that atomic unit as a local conflict; retain current values.
- Missing baseline, missing target, tombstone, foreign ownership, changed Person
  linkage or unavailable source evidence is held. Never reconstruct deleted data.
- Recheck under lock at commit. A stale preview becomes a counted hold, not an
  overwrite. A held item does not advance its last-applied baseline.

Metadata uses one bounded Person metadata unit; one unsafe change holds that
Person's proposed metadata mutations together. Notes and tasks each use one
record. History uses one stable external identity. Other safe units can proceed.
There is no cross-family force-merge policy or interactive conflict override here.

### P2. Explicit, owned clears/removals only

An acknowledged, qualified complete embedded tag list may remove a proven
migration-owned Person/tag link if all source aliases supporting that target are
absent and the link has not been changed locally. Keep local/unowned links.
This removes a Person link, never the shared tag definition. Do not infer a
complete standalone FUB tag catalog from embedded arrays.

A qualified explicit null may clear an owned field value if the source profile
states that it means clear. Missing properties, absent People, inaccessible
records and source 404s are unknown, not deletion. Empty text follows existing
lossless field rules; do not silently equate empty, null and missing. Preview and
confirm exact tag-removal and field-clear counts separately.

Do not delete notes, tasks, People, field definitions or history because they
vanish from a capture. Report not-observed/inaccessible separately. A qualified
source task may complete or reopen an unchanged imported task; count each action
explicitly. Never create native contact credit or invent a completion actor.

### P3. History corrections preserve earlier versions

A source record with the same global identity may have a different complete
canonical value in a strictly later qualified capture. That is a proposed
correction, not a second event. Append a typed correction version referencing the
previous head. Never update/delete old fact contents or replace their owner.

The normal timeline shows one current entry per external identity, labeled with
source record-created time and correction status. An administrator can inspect
prior metadata versions and capture provenance through bounded pages. Counts
count identities, not all versions; correction updates revision/order atomically.
Unknown source-created time stays in the separate unknown-date group. Conflicting
variants within one capture, Person reassociation, type/representation changes,
erasure or invalid prior provenance stay held. A captured updated timestamp alone
does not prove which variant wins.

Message bodies, subjects, phone endpoints, recordings and attachments remain
outside the history display contract. Exact raw evidence stays retained and
privacy-qualified. A body-only canonical change still gets a recorded version,
with “source changed; displayed metadata unchanged,” not a false unchanged result.

### P4. One review and confirmation; honest partial progress

The admin can select any ready family groups and explicitly exclude blocked ones.
The confirmation lists exact immutable family plan IDs, counts and digests,
including held/excluded/unsupported units, clears/removals, task reopens/completions
and history corrections. No default checkbox authorizes destructive changes.
After confirmation, the UI shows each family's applied/already-current/held/
unprocessed counts and source interval. Never label the whole migration complete
merely because queued units settled.

## 4. Sources, initial coverage and ordering

Select one completed retained 010e1 report for metadata/activity and one separately
completed 010d1 history capture for history. Both must belong to the same trusted
Org/account/original import. The UI displays two intervals; they are not an atomic
snapshot. Selecting evidence makes no FUB call. Missing evidence links to the
existing separately confirmed capture workflow without starting it.

For each selected cohort/family, require terminal first-coverage work with valid
results (010f1/010f2/010d2 or 010f3/010f4/010d3). A cancelled root may contribute
its proven successful units; unresolved first coverage remains a visible gap.
A cohort with no qualified first coverage is blocked for that family and can use
its existing first-import workflow. Do not fabricate baselines or silently run
those workflows as part of a refresh.

The newer core capture must start strictly after the previous accepted family
capture completed for the selected cohort; the history capture must additionally
start after the selected core capture completes when both are selected, and after
the cohort's creation capture. History-only refresh uses its existing core anchor.
Freeze all capture/report IDs, final sequences, intervals, access/account evidence,
parser/conversion/tzdb/key versions and cohort IDs. Overlapping/older evidence is
held as a prerequisite, not silently preferred by wall-clock timestamps.

Independently verify exhausted People/custom-field streams for metadata, and
People/users/notes/detail/open+completed tasks for activity. History requires all
three existing streams exhausted/reconciled. Authenticate accepted raw bytes,
ordinals, AEAD/HMAC and lossless records using existing profiles. Never combine
fields from separate records/captures or treat report completion as full fidelity.
Qualify duplicate occurrences across the whole capture before cohort filtering;
conflicting Person linkage cannot be hidden by excluding the other Person.

Index each source capture once per bundle in bounded pages. Reuse retained raw
references; no full-book decrypt/query per Person. Iterate all selected source
identities, not just report changed/new labels. New notes/tasks/history records
on eligible existing People can be inserted with global dedupe. Unowned existing
rows cannot be adopted by name/content equality. Prior held records may be
reconsidered only from positively qualified newer evidence and explicit mapping,
without rewriting the previous held outcome or pretending it was a baseline.

## 5. Family-specific execution

### Metadata

Use the shared original/admitted catalog registry and existing four field kinds,
exact source keys, option choices, alias handling and native limits. Explicit
create-matching or map-existing choices are frozen; reusing approved exact claims
is a visible suggestion until accepted for this plan. No global catalog renaming,
type coercion, definition deletion or implicit option creation. Definition/type
changes incompatible with the existing target are held and require later repair.
New valid definitions/options may be explicitly mapped/created under existing
rules. Catalog prerequisite units settle before affected Person units.

Baseline records distinguish links actually inserted by migration from links
merely observed already present. Only positive ownership authorizes removal.
For legacy provenance, bootstrap from immutable operation/result data plus exact
current target/revision evidence; if proof is insufficient, hold. Never infer
ownership from `origin=migration` alone. Person metadata revision is conservatively
fenced across the unit; all its edits/revisions/results settle in one transaction.

### Activity

Reuse existing source-key purposes, record validators, readable-note conversion,
source-only reply/attachment reporting, author/assignee/completer role separation,
explicit task-kind mapping and confirmed IANA timezone. Preserve native character
limits; no truncation, guessed author or browser/server timezone.

Note refresh may update the imported body's qualified representation and approved
source attributes. Task refresh may update title/kind/assignee/due/source completion
state. Freeze the complete native baseline and revision, including snooze, local
completion/reopen and attribution. A local mutation holds the whole record. Source
Person association cannot move an existing record to a different Person.
Private typed refresh commands authorize only that record's declared columns;
ordinary Update/CompleteTask commands do not bypass the workspace hold. Source
completion/correction provenance remains separately labeled from native actions.

### History

Maintain the existing global HMAC identity and original/admitted first-owner FK.
New identities get an exclusive refresh-owner shape with full tenant FKs; old
identities retain their first owner. Add three typed append-only version tables
for external event, call and text corrections (not a generic event store), plus
an indexed current-head projection keyed by Org/global identity. Initial heads
are deterministically backed by existing fact/display provenance.

Version rows contain IDs, predecessor IDs and encrypted deletable display
references/content hashes; no bodies or PII in immutable facts. The first version
for a new refresh identity uses the existing corresponding typed fact with the
new exclusive owner. Correction insertion CAS-checks the prior head, source
boundary and live Person. All version/head/count/revision/provenance/ledger writes
commit together. Erasure/suppression covers every version and prevents re-display;
old consumers cannot fall back to a superseded initial display.

## 6. Concrete shared-contract declaration

| Existing contract | Proposed change and reason | Affected components / compatibility |
|---|---|---|
| Separate first-import roots, no combined refresh | Typed bundle lifecycle and three frozen family plans, without changing old route meanings | Rust/API/Web, additive persistence; old roots remain terminal |
| Import-only native activity permits | Exact note/task UPDATE permit with baseline revision and declared columns | DB guards, typed commands, native revision/read models; no ordinary bypass |
| No subsequent metadata ownership baseline | Per-Person proven ownership plus immutable successful refresh results/head | Metadata readers/workers, catalog registry, receipts and byte inventory |
| History identities have initial original/admitted owners; changed variant held | Add exclusive refresh initial owner and typed correction chains/current heads | Three fact guards, global registry, bounded timeline/provenance readers |
| Readers see initial history projection only | Version-aware current-entry projection and paged prior-version endpoint | Admin history DTO/cursor revision; old readers fail closed after activation |
| Family-specific compatibility requirements | Durable `fub-family-refresh-v1` stamp in addition to all existing stamps | API/workers/recovery/startup inventories and release preflight |
| No consolidated review | `/api/migrations/fub/family-refreshes` | Strict scoped DTOs; Web uses same typed commands; native/Operator access unchanged |

Old requests keep their formats and semantics in untouched workspaces. In a
refresh-enabled workspace, compatible old-family paths revalidate current heads
and can never overwrite/refill data based on stale initial baselines. First
confirmation installs a durable compatibility requirement under the exclusive
workspace barrier before new writes. Every original/admitted/recovery/refresh
unit and read obtains the common barrier/stamp in its actual transaction. Test
claim-before-confirm/write-after-confirm races. Zero-write cancellation never
removes the requirement. Rolling back to a binary without support is disallowed;
forward recovery preserves all state.

## 7. HTTP and durable ownership proposal

Commands reject unknown fields; actor/Org come only from server context. UUIDs
are server-validated scoped references, numeric revisions/counts serialized as
decimal strings, digests as 64 lowercase hex characters.

- `POST /family-refreshes` PrepareFamilyRefresh:
  `{request_id,parent_import_id,core_report_id?,history_capture_id?,families}`;
  families is a unique nonempty subset of metadata/activity/history. Freeze all
  eligible successful cohorts for that parent, and separately count exclusions.
- `POST /{id}/plans` PlanFamilyRefresh:
  `{request_id,expected_revision,family,patches,source_timezone?}`; <=50 mapping
  patches per command. A new source selection requires a new preparation.
  Every change invalidates the combined ready digest until rebuilt.
- `GET /`, `/{id}`, `/{id}/families`, `/{id}/mappings`, `/{id}/items`,
  `/{id}/results`, `/{id}/items/{item}/fields/{field}`: bounded scoped pages;
  explicit family/cohort/outcome filters and missing-prerequisite counts.
- `POST /{id}/confirm` ConfirmFamilyRefresh:
  `{request_id,expected_revision,bundle_digest,families:[{family,plan_id,
  plan_revision,plan_digest,expected_counts}],acknowledged_exclusions}`.
  Expected counts include every action class in §3; compare exact server counts.
  Each selected family must have an eligible or proven already-current unit.
- `POST /{id}/resume`, `/cancel`:
  `{request_id,expected_revision,families}`. Resume explicitly adopts current
  admin and checks readiness/capacity; cancel stops future selected-family units.
- `POST /{id}/remainder`:
  `{request_id,expected_revision,families}` creates one successor for exact
  unfinished eligible units, fixed source/mapping/targets and baseline heads.
  Settled holds are not unfinished work. A newer accepted head makes an old
  remainder stale; it cannot reverse newer changes.
- `GET /people/{person}/history/{kind}/{identity}/versions` plus version detail:
  same admin review gate, metadata-only and bounded. Timeline optional version
  summary is additive; capability-aware readers are required after confirmation.

Reads default 25/max 50 rows, <=512KiB response, <=4KiB summaries, <=16KiB field
fragments; authenticated cursors bind actor/Org/workspace/bundle/family/plan/
revision/endpoint/filter/size/order. Truncation carries total byte length and a
fragment cursor; every retained value stays reviewable. No full plaintext state
in URLs, logs, client persistence, telemetry or Operator context.

Persistence uses `migration_family_refresh_*` bundle/cohort/plan/mapping/source/
manifest/result/receipt/reservation tables, with composite Org FKs and explicit
family checks; family payloads and permits remain typed. One active bundle per
original parent serializes overlapping preparations/execution. Manifest positions
and source walks have indexed keyset checkpoints. Native target IDs for inserts
are allocated in preparation and remain stable across retries/remainders.

Separate per-family/unit current-head records reference immutable successful
results; every head update compares expected predecessor. First-import adapters
supply proven baselines, never mutable present-day guesses. A completed scan
watermark is distinct from last applied per-unit state; held/missing units remain
visible gaps across later bundles. Reusing the same capture may resume exact
unfinished work only; new changes require a strictly newer qualified capture.

## 8. Transactions, leases, accounting and workload

States: preparing → ready → queued → running → completed, with paused/cancelled
and per-family terminal states. Preparation discovers all counts before ready;
ready expiry is ten minutes before fresh confirmation. Authorized receipt replay
precedes fresh expiry validation; same request ID with different body conflicts.

One bounded unit atomically writes native changes or typed history versions,
identity/provenance/result, baseline head, read revisions and byte settlement.
Lock order follows existing workspace → current memberships → Org → bundle/
family plan → Person → record; deterministic ordering for shared catalog/identity
locks. Verify current executor, source/target/head, plan digest, workspace,
compatibility and unexpired lease token/epoch inside that transaction.

Use existing one-second migration scheduling and bounded fairness. Extend the
app's worker dispatch, without three additional uncoordinated polling loops,
new pods, a broker, PostgreSQL LISTEN/NOTIFY supervisor or PgBouncer changes.
A worker claim lasts 60 seconds, renews only while owned and cannot authorize a
late commit after expiry/cancellation. Crash-before-commit rolls back; crash-after-
commit finds the same immutable result. Other family units can continue while one
is paused, within the single active bundle and shared Org budget serialization.

Core-family derived bytes charge their selected snapshot; history-derived bytes
charge the existing history run/Org ledger. Bundle/control shared bytes have one
explicit payer (core snapshot if selected, otherwise history run); never charge
the same raw reference twice. Reserve cancellation/control capacity at admission.
Enumerate nonce/ciphertext, keys, references, mappings, heads, versions, receipts,
results and discarded preparations in an exact measured SQL inventory before
implementation. Retain existing limits; oversized units hold visibly, capacity
pauses safely and monotonic allowance changes require separate explicit resume.

## 9. Delivery and acceptance

One primary writer, sequential stages on one short-lived branch. The previously
authorized single reviewer completed independent planning review
(read-only, READY, round 1). Reuse that one reviewer for implementation review
within the accepted package. At most two planning
review/fix rounds and two implementation review/fix rounds under D-050.

1. Accept P1–P4 and this shared-contract declaration; freeze exact SQL owner,
   trigger/grant/byte-inventory and DTO fixtures before application implementation.
2. Build bundle/source/cohort/baseline foundation and versioned compatibility fences.
3. Implement metadata deltas, then note/task deltas, then typed history correction
   projection. Each stage supplies its own replay/conflict/cancel evidence.
4. Build the shared Web preview/confirmation/progress UI, with per-family drilldown,
   explicit exclusions and reuse of capture/first-coverage prerequisite workflows.
5. Run integration/review and final gates once; deliver the combined verified
   package with 010e6 retained on local main. Deployment remains deferred.

Required evidence: all original/admitted/recovery cohorts; legacy baseline
bootstrap with insufficient proof held; native/local edits including ABA changes;
source alias removals and explicit clears; tag/option limits; new/missing/erased
records; task due timezone/DST, completion/reopen/snooze; HTML/source-only data;
canonical repeats/variants/body-only history changes; current/prior history counts,
time grouping and no native contact credit. Tenant/member/CSRF/foreign targets,
receipt lost-response replay, stale cursors/demotions, concurrent old writers,
lease takeover and stale claims in preparation/execution, atomic fault injection,
partial-family cancellation/exact remainder, budget/byte audit and compatibility
inventory all require concrete assertions. Complete source occurrence accounting.

Real synthetic production-Web workflow at desktop and 390px: all three groups,
conflicts/holds, explicit destructive counts, reload and uncertain confirmation,
partial failure, cancellation, history revisions and private cache clearing.
`scripts/check`, SQLx prepare/schema/offline checks and serial synthetic DB tests;
25k realistic EXPLAIN for every changed hot query and one correctly configured
paired Person/Today run under D-050. Isolate build outputs from running services.

## 10. Limits and approval boundary

This closes later family additions/updates/corrections for qualified retained
evidence; it does not claim full FUB synchronization or cutover readiness. Source
deletions, arbitrary unsupported data, changed Person identity, catalog structural
repair, new private audience/consent rules, email/media, native activation, live
source/customer work and O-012/O-013 readiness remain separate.

**Approval sought:** accept P1–P4, the new family-refresh contracts and phased
implementation as one package. The reason for this gate is AGENTS §11 (shared
contracts) and §16 (customer-visible behavior/migration fidelity), plus the
existing specs' explicit no-update/immutable-history boundaries. It is not another
deployment approval; deployment has been explicitly deferred by the user.
