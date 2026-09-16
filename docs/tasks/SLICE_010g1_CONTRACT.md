# 010g1 — Concrete implementation contract

**Owned by accepted D-092.** Complements the accepted combined plan; no new product
policy. Primary writer owns all schema/domain/API/Web changes. Implementation is
in progress, not verified. Base `dd140b0`; branch `codex/010g1-family-refresh`.

## Exact unit identity and native write surface

All composite references include Organization. Bundle scope is one original
completed import and its frozen successful original/admission/recovery results.
The server resolves source IDs to these results and the global live identity.

| Unit | Target and mutable columns | Immutable checks |
|---|---|---|
| Person metadata | `person_tag` INSERT/DELETE; `person_custom_field_value` INSERT/UPDATE/DELETE of typed value, update actor/time/correlation/origin | Person, Org, field/type/catalog mapping; tag deletion means only the link; no tag/field definition DELETE |
| Note | INSERT; UPDATE `body,author_user_id,updated_at,correlation_id` | ID, Org, Person, origin, source/source_external_id, created_at, deletion state; database advances revision |
| Task | INSERT; UPDATE `title,kind,due_at,assignee_user_id,created_by_user_id,completed_at,completed_by_user_id,updated_at,correlation_id` | ID, Org, Person, origin, source/source_external_id, created_at, deletion state; no fabricated completer; database advances revision |
| History | New existing typed initial fact with exclusive refresh owner; correction INSERT into corresponding typed version table | Stable global identity and Person; immutable first owner/fact and predecessor chain; no fact UPDATE/DELETE |

Native Person `metadata_revision` fences the complete metadata unit. Note/task
`revision` fences the complete record, including fields not changed by refresh.
The only allowed native UPDATE differences are the listed columns and existing
server-maintained revision fields. Each permit binds bundle/family/plan/unit,
expected target, operation, before/after digest, current executor and lease epoch.
A write cannot borrow a sibling unit/family's permit. Permit proof is immutable
and only populated before plan sealing; no source text is stored in guard rows.

## Baseline and ownership evidence

A refresh baseline references a successful immutable original/admitted result or
successful previous refresh result. It includes canonical native after-state,
native revision, exact source binding and ownership flags in encrypted payload.
Applied updates advance that baseline atomically; held/excluded records do not.

Activity legacy bootstrap verifies the original/admitted global identity, exact
successful manifest/result, decrypted frozen native payload, complete native
row equality and initial native revision. A missing revision/provenance proof
holds `baseline_unproven`; mere value equality never repairs it. Mutable current
rows are observations, not historical proof. Newly generated first-family results
will retain after-state/revision explicitly for subsequent refreshes.

Metadata legacy bootstrap reconstructs only positively applied link/value
operations, with exact decrypted mapping/value provenance and all alias claims.
A link classified already-present is never acquired. Reconstructed complete
state and revision must be provable from immutable writes and the migration
review binding; pre-revision or incomplete evidence holds `baseline_unproven`.
Do not initialize a new head from whatever is currently in the Person table.
New first-family results retain exact metadata after-state/revision so future
refreshes do not need inference. Invalid legacy proof is a visible unsupported
baseline, not permission to weaken ABA protection.

History bootstrap uses existing immutable identity/fact/display provenance and
source semantic HMAC. First owner stays unchanged. Current heads must retain the
source boundary and correction version; identity equality alone cannot recreate
a deleted display or overwrite an erased flag. New head/version has a composite
predecessor reference plus compare-and-swap, so only one successor wins.

## Persistence ownership and byte inventory

`migration_family_refresh_bundle`: original parent, account, executor, immutable
selected source IDs, revision/digest, lifecycle; one active per parent.
`*_cohort`: successful original/admitted result identity; creation capture.
`*_plan`: family/revision/state, frozen input binding, counts, mapping digest,
source walk/classification/apply checkpoints, lease token/epoch/expiry.
`*_source`: authenticated source occurrence/capture/ordinal, semantic identity,
parser representation, encrypted lossless derived record. Every occurrence is
classified; no source ID/Person label chooses a privileged target.
`*_mapping`: versioned bounded explicit mapping and destination snapshot.
`*_manifest`: one immutable executable or held unit, encrypted B/C/S proof,
expected native/head revisions, prospective IDs and exact counted actions.
`*_result`: one immutable terminal outcome per unit, encrypted after-state and
provenance; references a successful baseline only when eligible to advance.
`*_head`: bounded current pointer per family unit, no native-target cascade.
`*_receipt`: actor/action/request ID + keyed input digest and encrypted response.
`*_reservation`: plan/token/epoch and payer ledger with separate control reserve.
`*_requirement`: append-only durable compatibility boundary per Org.
Three typed history-version tables reference the existing corresponding fact,
predecessor version, refresh result and encrypted display reference. A current
projection indexes Org/Person/family/source-created-time/identity and unknown-time
order; counts reflect identities and revisions reflect all changes.

Inventory includes every stored variable byte: identifiers represented as text,
family/state/reason/profile tokens, encoded counts/bindings, nonce/ciphertext,
semantic/key/digest bytes, source field fragments, proof columns, heads, receipts,
and discarded preparations. Fixed-width columns are treated consistently with
existing family ledgers. Shared controls choose one payer; source/canonical raw
capture bytes are not duplicated or recharged. Native content deltas and retained
derived bytes settle separately as existing ledgers require. Exact SQL measuring
functions and observed definitions become release inventory fixtures before
verification, not an estimate based on request body size.

## Lock, permit and compatibility order

Use workspace shared lock, active executor membership locks, Org retention lock,
bundle and family plan lock, source/identity/head lock, Person lock, then native
record/catalog locks in sorted UUID order. Concurrent first-family writers follow
the same shared Org/Person and catalog-claim ordering. Common workspace admission
checks `crm.family_refresh_reader=fub-family-refresh-v1` after durable confirmation;
keep every prior capability stamp as well. Existing worker transactions stamp
inside the actual bounded unit, not only at startup or before claim.

Confirmation takes the exclusive workspace barrier, validates every selected
ready plan/count/digest and installs the durable requirement atomically with one
receipt. Cancel/remainder/lease checks occur under the same family lock. Resume
requires fresh explicit authorization and release readiness. Expired or cancelled
preparation/execution dispatch cannot reactivate a plan. Crash rollback includes
native state, result, identity/head, counts, checkpoint and ledger.

## Read/HTTP envelope

Use the accepted routes/DTOs in plan §7, strict request deserialization and no-store
responses. Decimal counters/revisions are strings on the wire; closed family and
kind enums. 25/default, 50/max items, 512 KiB response ceiling, 4 KiB summaries and
16 KiB UTF-8 field fragments. Cursor AEAD binds actor/Org/workspace/bundle/plan/
revision/family/filter/endpoint/size/order. Uncertain command retries reuse the
same request ID and body; actor/Org changes clear all private state and receipts.

### Accounting implementation checkpoint

Shared evidence uses bundle `shared_measured_bytes` / `shared_retained_bytes` and
one `payer_plan_id`, assigned once before confirmation with a composite deferred
FK. Private evidence uses each plan's corresponding counters. Head
`storage_plan_id` remains its first payer even when the current result changes;
the permanent requirement records its installing bundle. Native content is
reported separately from retained evidence, as in the existing activity ledger.

Reservations count against the plan, Organization and selected source: the core
snapshot for core families or retained history capture run for history. Shared
bundle evidence follows its one frozen payer. History erasure refunds the original
capture as well as the owning plan and Organization; takeover reclamation refunds
all three reservation counters. Migration 009 backfills already retained history
refresh charges without recharging raw bytes or the Organization ledger. Each reservation includes 512 bytes for its own control-row overhead;
control reservations start at least 8 KiB. Settlement charges actual measured
deltas. Unit settlement refunds unused capacity; control settlement retains its
remaining cancellation capacity until explicit release. A control reservation
survives lease takeover, while unit settlement requires the original epoch and
current unexpired token. Deferred application-only checks require measured and
charged evidence to agree and reserved counters to equal owned reservation rows
at commit. Migrator fixture construction is outside that application check.

A lease takeover reclaims only an older-epoch unit reservation after proving that
all its evidence was already settled. Reclamation cannot touch the cancellation
reservation, and retrying it cannot refund again. The catalog-driven
`tests/fixtures/family_refresh_byte_inventory.sql` checks every variable-width
column independently, so a newly added nullable column cannot silently escape the
retained-byte inventory merely because current fixtures leave it empty.

### History storage implementation checkpoint

`migration_family_refresh_history_head` retains the first storage payer and the
immutable global identity/Person/original fact. Its mutable fields are fixed-width
current version, result, capture, semantic hash and source-created time. Bootstrap
copies exact original/admitted fact/display provenance; equality alone cannot
construct it. Three `fub_{event,call,text}_record_corrected` tables have scoped
original-fact, predecessor, display, manifest and result references. A successor
compares the current head, uses a strictly later source capture, and advances the
head/counts/review revision in the same transaction. Counts still count identities.

Each correction's `migration_family_refresh_history_display` contains at most
4 KiB of encrypted metadata plus the AEAD tag. Its ciphertext can only transition
to a tombstone after the global identity is erased. Erasure clears every version
and refunds exact nonce/ciphertext bytes to each version's original plan and the
Organization, including completed plans. Initial-display suppression decrements
the current date bucket. Immutable facts and first ownership remain intact.

This is a storage checkpoint: application roles have SELECT only on these five
new tables. Do not enable writes until version-aware readers, full execution
admission, capability inventory and new-identity refresh ownership are wired and
verified. The existing app deliberately does not advertise the new capability.
The retained-source adapter preserves the original `timeline-import-identity-v1`
and `timeline-import-canonical-v1` purposes. Body-only canonical changes alter the
semantic hash while leaving metadata unchanged; display AEAD has its own purpose,
4 KiB ceiling and exact version-row binding.

### Native delta planning checkpoint

The metadata planner compares complete baseline/current metadata and the native
revision before proposing any change. It retains explicit ownership separately
from value equality. Whole-Person proposals preserve local links, require all
supporting tag aliases to be absent before removing an owned link, retain unknown
source gaps, and distinguish qualified field clears from missing/null/empty data.
Duplicate field targets and conflicting alias targets hold the Person. Capacity
uses the final tag set and the same 20-tag constant as ordinary native commands.
A hold never returns a partially applicable change list or destructive counts.

The activity update planner starts from the complete unchanged native row and
replaces only declared note/task columns. It preserves identity, source binding,
creation time and any local-only fields; a changed source creation time is held.
Equivalent timestamp formats and a new correlation ID alone do not cause writes.
Historical author/creator membership and active-assignee membership remain
separate. Source completion/reopen has explicit counts and no invented completer.
The database must still verify the frozen proposal under locks and return the
actual post-write revision; these pure proposals do not authorize execution.

Both planners are internal preparation components. Cohort/source discovery,
immutable baseline adapters, HTTP commands, workers and Web integration remain
unfinished. No application path invokes these proposals yet.

### Cohort preparation checkpoint

`family_refresh::cohort::freeze_page` is an internal bounded transaction for the
existing dispatcher integration. The bundle's discovery timestamp is taken only
after acquiring the exclusive workspace lock, so earlier admission writers have
committed. Bundle creation must acquire that lock before taking other workload
locks; the trigger provides a final boundary check with a bounded lock timeout.
The payer plan scans original-parent People identities in source-ID order, at
most 50 per unit, retaining exact original or terminal admission/recovery result
references. Unproven, unfinished and erased identities receive explicit counts.
Newer admissions cannot join the frozen scope. Family first-coverage and baseline
qualification remain separate subsequent steps.

Each page rechecks the active admin, workspace, token, epoch and expiry, reserves
capacity, and commits cohort rows, cursor, counters and exact ledger settlement
together. Failed units roll back; expired older reservations use the existing
fenced reclaim operation. Completed cohort preparation is replay-safe. The
nullable initial counter avoids retroactive uncharged bytes on existing plans.
This does not yet expose an HTTP command or dispatch a production worker.

Database cohort guards additionally require the identity to predate the frozen
boundary, the native Person to remain present, and any admission to have reached
a terminal state by that boundary. Application cohort inserts and cursor/count
updates require the current payer-plan preparation token, an unexpired lease and
an active admin executor. Migrator maintenance access does not bypass the frozen
identity/terminal proof checks. These checks supplement the page transaction.

### Core evidence indexing and activity after-state checkpoint

The payer plan indexes each selected retained core capture once per bundle.
`migration_family_refresh_core_page` retains authenticated request/cursor evidence
for successful, empty, rejected and inaccessible pages. Its immutable scoped raw
reference supplies the original bytes; a page and its derived occurrences share
one transaction, checkpoint and exact settlement. Source rows bind that page by
composite FK. The database checks the live payer lease, source snapshot and frozen
report sequence boundary. The runner revalidates the report's authenticated input
tuple, source identity scope, raw AEAD/length, request HMAC, parser representation,
ordinals and every retained semantic HMAC. No capture or Person is fetched live.

The walk includes every occurrence before cohort filtering, including conflicting
Person associations. Metadata/activity conversion uses the existing lossless
parsers; raw canonical buffers are cleared before encrypting derived records.
Oversized derived units retain the raw reference and an explicit hold. A 404 note
detail is an inaccessible page, never a note-list fallback. Pagination resumes
from authenticated accepted-page cursors, not inferred source IDs. The new index
is an internal preparation runner; common mapping/classification and later plan
revision reuse are not yet integrated. History has a separate source walk pending.

New positively applied original/admitted note/task results now include optional
`after_state: {version: 1, native: ...}` inside their existing encrypted result.
The native JSON is read after INSERT under the same transaction and retains the
initial revision and complete persisted fields. Already-present/held results do
not acquire this proof. Existing ciphertext measuring/settlement includes these
bytes; existing result HTTP projections remain unchanged. Old payloads deserialize
with no after-state and are not backfilled from mutable native rows.

Activity baseline discovery uses the current worker/admin/workspace admission,
the frozen Person cohort, global identity, exact original/admitted manifest and
successful first-owner result. It requires terminal first coverage and verifies
the authenticated after-state's complete native binding/equality/revision. A local
edit-and-revert is held; a missing record is erased; missing legacy evidence is
unproven. Existing refresh heads take precedence (the first-result adapter returns
`stale_head` for those targets). Prior-refresh-result and provable legacy adapters,
metadata baselines and execution-time revalidation remain subsequent work.


### First-import metadata after-state checkpoint

Successful original/admitted metadata Person results now carry an optional,
versioned encrypted `after_state` with exact Organization/import/manifest/Person
binding, complete tag and typed-field values, native metadata revision and
insertion ownership. The format remains backward-readable; absent old evidence
holds `baseline_unproven`. Existing result HTTP projections are unchanged.

Only actual inserted link/value receipts grant ownership. Already-present tags
can contribute an additional source alias to a link positively inserted in that
same unit; they cannot grant ownership of a preexisting link. Held Person units,
including partial first-import outcomes, publish no usable baseline. All native
state and the proof settle in the existing transaction and ciphertext ledger.
Verification compares the complete state and revision, including unowned cells,
and requires the exact immutable result binding. Metadata baseline discovery,
legacy bootstrap and refresh execution remain unwired.


### Scoped discovery and shared source indexing

Metadata discovery uses the frozen cohort's exact original/admission result,
parent/account, successful terminal family result and authenticated after-state.
Results or terminal transitions after bundle creation cannot establish first
coverage. It compares current native state/revision without adopting equality;
existing refresh heads await their own adapter.

Core source selection includes every indexed occurrence before Person filtering,
compares variants within their representation, rejects open/completed task
contradictions and requires note detail. Immutable sources retain their original
plan's encryption scope and storage ownership when referenced by another family
or mapping revision. Manifest source FKs are now bundle/Organization scoped, with
kind and Person guards. Native guards reconcile source IDs instead of depending
on nullable preclassification identity hashes.

History indexing preserves authenticated page/cursor evidence separately from
raw captures, validates run/parent/account/access-user/profile and completed stream
totals, and requires capture ordering after the selected core or latest frozen
cohort creation capture. Every parsable occurrence remains visible before cohort
filtering; diagnostic pages are retained explicitly, never used as qualified
native evidence. Only metadata is copied into the derived index; raw bodies and
full canonical values remain in their existing encrypted captures. Page sources,
checkpoints and exact byte charges commit together behind the current lease.
This stage supplies preparation components, not native execution or HTTP admission.


### Prior-refresh native baseline discovery

Successful refresh results use the versioned, family-typed
`native_baseline::ResultData` after-state payload under the existing result AEAD
scope (Organization, bundle, plan, plan revision and result ID). The payload also
binds its manifest, source ID, Person, target and family. Metadata retains the
complete snapshot/revision, current result head and separate insertion ownership;
notes/tasks retain the complete native row/revision and positive ownership flag.
Unowned or absent after-state cannot become a mutation baseline.

Discovery requires the current head's successful result, exact manifest/cohort,
original parent/account, terminal confirmed predecessor family and bundle, and a
result/bundle completion boundary no later than the new bundle's freeze. The
selected capture must be strictly newer than that result's selected capture.
The existing target index bounds lookup; multiple source-key heads for one target
hold. A present but invalid head never falls back to an older first-import result.
AEAD failure fails closed; local edits, including edit/revert and task-local state,
hold without writing any new result. Application-role reads use the existing
current-admin/workspace/preparation-lease admission.

This defines the successful native-result payload and read adapter for the
forthcoming executor. Synthetic successful-result fixtures are migrator-owned;
no new native execution, HTTP admission or capability advertisement is enabled.


### Authenticated prior-history correction discovery

A current typed correction must join its exact successful result, manifest,
frozen Person cohort and immediate predecessor. The predecessor bundle/family
must be confirmed and terminal, with its bundle update, result commit and typed
version timestamp no later than the new bundle's frozen boundary. Its capture,
source account, original parent and exact original/admitted Person result remain
scoped; a new capture must still be strictly later than the accepted current one.

Discovery opens the deletable display in the correction's version/plan scope and
also opens its retained source occurrence in that occurrence's original plan
scope. Authenticated source identity, full canonical semantic HMAC, Person link,
source-created time and metadata must match the typed correction and display.
A validly encrypted but unrelated display is rejected; body-only canonical
changes remain distinguishable. Existing first fact/display verification and
erasure gates run before accepting a correction. Invalid or erased current
versions never fall back to the original fact.

The joins are bounded by the current identity/version keys; discovery does not
walk or decrypt the entire version chain. This is preparation evidence, not the
version-aware public timeline or execution admission. Application grants and
capability advertisement remain unchanged.


### Accepted scan boundaries remain separate from applied baselines

Fresh metadata, activity and history baseline discovery also checks prior
confirmed family plans for the exact frozen Person cohort, original parent and
account. A held unit or zero-write cancellation does not discard its accepted
capture boundary. The same capture, or a new interval starting before/on a prior
accepted interval's completion, holds `source_not_newer` even when no refresh
result/head was ever written. A later qualified capture may still use the older
unchanged applied baseline. Unconfirmed plans and other families/cohorts do not
establish this boundary.

The existing confirmed plans and frozen cohorts are authoritative; no duplicate
watermark or storage charge is introduced. The lookup uses the existing parent,
plan and cohort indexes, returns at most one violating boundary, and never
selects a winning source variant by timestamp. First-coverage and source-record
qualification remain separate. Exact remainders must copy their fixed manifests;
they do not obtain same-capture permission by rerunning fresh discovery.

### New identity prerequisite discovery

A separate read-only adapter resolves every retained source occurrence before
qualifying new note/task/event/call/text identities. It checks the exact frozen
Person and live original/admission identity, prior accepted refresh scans and
terminal first-family roots before the bundle boundary. A successful first-family
result must belong to that Person/cohort; held-only or missing coverage remains
`first_coverage_required`. Original and admitted/recovered owner shapes stay
separate. Every confirmed first-family capture must precede the new capture.
A fixed-size SQL aggregate checks the lineage without returning all roots.

Existing global identities, including tombstones, cannot become new. Activity
also rejects native collisions under both canonical account-scoped and legacy
source keys, regardless of content equality. This adapter creates no baseline,
head, identity or target. Its candidate is only a source/identity/coverage proof;
classification must still validate mappings/native limits and freeze stable IDs,
and execution must revalidate under its write locks and permit.

### Persisted qualified history proposals

The history unit preparer freezes authenticated source metadata/canonical HMAC,
exact prior fact/head/version proof, operation counts and stable identity/fact IDs
inside plan-scoped encrypted manifests. New identities use the first-coverage
adapter; consumed or unproven identities never obtain a fabricated baseline.
Original facts and native heads remain untouched during preparation.

The plan lock serializes duplicate preparation; an existing manifest is replayed
before observing mutable baseline state. An active admin/lease is required for
replay as well as insertion. Manifest insertion, position/count changes and exact
byte settlement commit together. Capacity failure or a failed checkpoint leaves
no partial manifest, count or charge. Source diagnostics, out-of-cohort exclusions
and held prerequisites still need dispatcher-owned settlement before plan sealing;
this entry point alone cannot mark the complete family ready.

Qualified new notes/tasks also have a pure initial-row proposal: it requires
unconsumed identity/first-coverage inputs, validates unchanged native content
limits and Organization role mappings, and preserves source timestamps and
completion without assigning a native completer. A completed source task is an
inserted initial state, not an additional native completion action. Revision zero
in the proposal means no existing row; the initial row has revision one. Future
write-proof construction must canonicalize proposed typed rows through PostgreSQL
before digesting them, including PostgreSQL's timestamp representation.

### Typed preparation admission

`PrepareFamilyRefresh` now creates the bundle and family plans from scoped,
completed retained evidence. The closed request accepts only parent/source IDs
and a unique family selection. Authority comes from the active admin command
context under the common workspace barrier; one active bundle serializes a
parent. Core report bindings are authenticated before admission, and history
profile/account/parent/interval bindings are frozen for full indexed validation.

Preparation seals source/profile/conversion/tzdb bindings, allocates immutable
family plan IDs, chooses a core payer when selected (otherwise history), records
an actor-scoped encrypted receipt and reserves measured evidence plus 8 KiB of
remaining cancellation capacity per plan. The whole command rolls back if any
family cannot fit. Authorized same-body receipt replay precedes fresh capacity
checks; a changed body with the same request ID conflicts. Preparation does not
install the durable native-refresh capability or confirm work. HTTP/release
admission and worker dispatch remain integration work.

History-only indexing uses the original parent's core anchor. Each Person's
history eligibility independently checks its frozen creation capture, so a later
admission/recovery can be held without preventing other cohorts' source indexing.
Combined history still must start after the selected core capture.

### Typed mapping revisions

`PlanFamilyRefresh` now owns at most 50 explicit mapping patches for one selected
family, with the accepted bundle revision and actor-scoped replay receipt. It
requires completed core mapping inventory, an unconfirmed current plan, no live
preparation claim, and authenticated frozen bundle/plan conversion bindings.
Success supersedes that family plan, clears the combined ready digest, and
reserves a successor's measured evidence plus cancellation capacity atomically.
Other selected families and the original shared-source payer remain bound.

Migration 014 retains encrypted, immutable patches referencing the immediately
preceding mapping. The bounded inventory worker applies those choices or inherits
untouched predecessor choices by exact source key. Proposed catalog IDs are
server-allocated and survive inheritance; this path creates no native catalog
rows. Existing choices capture scoped destination snapshots; classification,
confirmation and execution must independently revalidate targets and complete
source eligibility. Option choices bind their effective parent field, including
field patches in the same request. Intrinsic option validity does not depend on
whether an oversized source field could be created wholesale.

An omitted activity timezone inherits the previous choice; explicit null clears
it to hold. Historical authors/creators may reference inactive Organization
members, while assignees must be active. Read summaries authenticate encrypted
choices against relational disposition/target/source/parent/value bindings.
HTTP exposure, complete classification and native application remain pending.

### Complete metadata field-name qualification

Retained core indexing stores an exact `field-name` HMAC for every parsed field
occurrence, using the existing Organization/account namespace. Migration 017 adds
an indexed equality token and a fixed-width indexing marker on the source row;
the variable-width token participates in the common byte inventory and atomic
source-page settlement. Conflicting occurrences of one source ID are not reduced
to the first mapping's name. Qualification uses all occurrences before counting
at most two distinct source IDs, then verifies the selected encrypted definition
against its stored name token. Choice-label comparison additionally uses the
native database's `lower` semantics over one bounded source definition.

This is a read-only preparation prerequisite, not a native catalog grant. An old
immutable source index without the marker holds as source unavailable and needs
a newly prepared bundle; mapping revisions reuse that old index and cannot
silently upgrade it. No unchecked backfill or native catalog mutation occurs.

### Catalog destination inspection

The read-only catalog adapter authenticates the selected mapping and its parent,
rechecks native destination snapshots and existing original/admitted registry
claims, and preserves Plan's prospective IDs. It requires the completed shared
catalog handover; it never treats an inactive registry as empty or initiates that
handover from a reader. Refresh preparation still needs to wire the existing
handover into its typed admission path before exposing this workflow.

Distinct field/option source keys cannot claim one destination. Creation checks
use bounded prospective sets and native database folding, preserve the 200-tag,
50-field and 50-option limits, and require all captured options to be explicitly
created together for a new choice field. Colliding labels hold the affected
candidates rather than unrelated definitions. Migration 018 supports scoped
bounded target probes. Inspection produces evidence only; final catalog units,
refresh-owned claims and execution revalidation are separate remaining work.

### First retained mapping representatives

Migration 019 freezes a fixed-width mapping-order mode on each bundle. Newly
prepared bundles walk capture sequence, item ordinal and source ID, with bounded
element order inside each source. Equivalent embedded tag spellings therefore
use the first retained eligible occurrence instead of random source UUID order.
Separate metadata/activity indexes support that keyset traversal; the existing
live-lease/no-skip mapping fences use the same selector.

Existing bundles retain UUID traversal so an in-progress cursor cannot silently
skip records on upgrade. Their tag catalog choices remain source-unavailable and
require a freshly prepared bundle. Replanning over the same shared old index is
not a compatibility upgrade; existing immutable choices are not rewritten.

### Immutable catalog preparation units

Catalog manifests now carry a typed mapping FK. Migration 020 fences their
family, live lease, exact source/key/target, null Person/baseline fields and
parent-option prerequisite. Eligible units require compatible ready registry
state and qualified mapping intent; held units carry no native target. A catalog
unit cannot grant native write authority merely by being present.

The unit runner persists the encrypted outcome, exact counts, position and byte
settlement atomically. Replay returns the earlier outcome before mutable catalog
checks, preserving its prospective IDs and avoiding repeated charges. Options
wait for a parent catalog outcome; a held parent holds the dependent option.
Native execution must independently revalidate ready evidence. Full family
readiness/sealing must distinguish these prerequisites from useful Person work;
existing catalog mappings alone must not turn an otherwise all-held Person
family into actionable refresh work. Worker dispatch and Person preparation are
still separate integration work.
