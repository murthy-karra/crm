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
