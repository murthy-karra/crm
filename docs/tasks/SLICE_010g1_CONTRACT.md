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

Reservations count against the plan, Organization and (for core families) selected
snapshot. Each reservation includes 512 bytes for its own control-row overhead;
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
