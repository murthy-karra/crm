# Mobile 006 — Offline tags and custom-field values

**ACCEPTED — D-084, 2026-09-14.** The user approved implementation and isolated
synthetic verification. Both independent planning reviews returned READY. Current
implementation and acceptance evidence are recorded in
[implementation status](../tasks/MOBILE_006_010f4_IMPLEMENTATION_STATUS.md).
Publication/deployment and materially different policy remain separate.

The user selected offline tags and custom fields as the next mobile outcome.
D-083 records the initial planning authority; D-084 accepts these contracts. [Brief](../tasks/MOBILE_006_IMPL.md),
[paired plan](../plans/MOBILE_006_010f4_PARALLEL_LAUNCH.md),
[author findings](../tasks/MOBILE_006_010f4_PLANNING_REVIEW.md).

Read AGENTS.md, D-015/023/050–053/058/073–083 and O-012/O-013;
[architecture](../architecture/ARCHITECTURE_BASELINE.md),
[tag rules](SLICE_011e.md), [field rules](SLICE_019.md), and Mobile 001–005.
Those accepted contracts remain authoritative except for the proposed extensions
declared here. This document makes no source request or native-release claim.

## 1. Outcome and bounded product rules

An active member can apply/remove existing tags and set/clear existing custom-field
values on a downloaded Person in either native app while offline. A committed
local proposal survives ordinary termination/relaunch and uploads once. Conflicts
retain the proposal for explicit review. Assignment does not restrict visibility
or these member actions; a migration-review workspace denies ordinary mobile
access even to an admin.

Proposed first scope:

- Existing catalog IDs only. No tag creation/rename/delete, field/option creation,
  archival/reorder, new People, reassignment, bulk editor, new filter UI, Operator
  tool, calls, messaging, push, broad redesign or app distribution.
- Reuse the four field kinds and their existing validators: text, exact decimal
  number, calendar date, single choice. No floating-point conversion, inferred
  timezone for a date, coercion, truncation or label-based identity matching.
  Text retains the existing 500-code-point validator; numbers retain the exact
  NUMERIC(19,4) input grammar; dates retain 1900-01-01 through 2200-12-31.
- Keep 20 tags per Person, 200 tags per Organization, and existing field/option
  limits. Archived definitions/selected options remain readable. Setting requires
  a live definition and, for choice, a live option belonging to it. Explicit clear
  remains allowed for an archived field, matching the current command. Empty text
  is invalid; clear is a distinct action. An absent value is not numeric zero.
- One submitted unresolved metadata operation per Person. Later edits are a
  separate follow-up draft, never a mutation of the in-flight envelope. One
  operation contains at most 50 explicit tag/field actions, all applied atomically.
  Larger edits require separately reviewed submissions; no replace-all collection.
- A single Person metadata revision covers tag membership and all field values.
  Any intervening metadata write, including A→B→A, conflicts with the old proposal.
  Different fields edited by two devices therefore conflict conservatively.
  Names, contacts, stage, assignment, notes/tasks and contact logging alone do not.
- A separate Organization metadata-catalog revision covers definition/option/tag
  creation, labels, order, archive state and deletion. Any intervening catalog
  change requires an explicit refreshed review, including an unrelated rename.
  This is a deliberate conservative v1 tradeoff, not automatic merge/rebase.

These conflict and atomic-edit rules are proposals requiring scope/contract
acceptance after independent review. D-051/D-058 permissions are unchanged.

## 2. Declared shared-contract changes

| Current contract | Proposed contract and reason | Owner, consumers and compatibility |
|---|---|---|
| Tag/value commands each open their own transaction; no expected metadata token | Transaction-compatible implementations of the same typed commands, composed by typed `UpdatePersonMetadata` with mandatory expected tokens | Mobile backend owns narrow tag/custom-field command refactoring and composer; existing Web/Operator bodies, permissions, validation precedence and last-write behavior stay unchanged |
| Broad `mobile_revision` does not describe a complete editable metadata baseline | Positive `person.metadata_revision`, advanced for actual link/value insert/update/delete, plus broad Person invalidation | Additive migration and all-writer read-model triggers; existing records start at 1; no changes to stage/details revision semantics |
| Native generations have summary/contacts, notes/tasks and a stage catalog | Add opt-in metadata representation with bounded Person tag/value sections and bounded shared tag/field/option catalog pages | Backend generation/seal/cleanup, native protocols/stores and fixtures; old generation requests and seal requirements stay unchanged |
| No versioned native metadata catalog | Dedicated Org-scoped metadata-catalog revision read model and generation snapshots, using Mobile004's catalog discipline | Mobile backend owns catalog-writer inventory and bounded paging; catalog changes do not fan out value rewrites to every Person |
| Mobile receipts enumerate earlier operations/resources | Add `update_person_metadata` / `person_metadata`; committed revision identifies the resulting Person metadata state | Backend and both native receipt dispatchers; exact old kinds, digest input and stored receipt bytes remain valid |
| Only current note/task/stage/details conflict reads exist | Context-bound current-metadata and catalog traversal, with explicit final token validation | Additive no-store mobile routes; transient editor reads never renew access or replace sealed Today/cache state |
| Existing `tags_changed` and `custom_field_changed` events | Reuse those exact variants after changed commit, one per changed category | Backend publication dispatch and Web compatibility fixtures; no new realtime vocabulary, catalog-content payload or Operator tool |

Amendment ownership: this spec proposes narrow extensions to Slices 011e/019 and
Mobile 001/004/005. Their historical DTOs and policies are not silently rewritten.
The backend checkpoint freezes routes, JSON, revision/error encodings, SQL/FKs,
permit/grant changes and the writer lock graph before native work begins. Simple
encoding details are owned implementation work after approval; different conflict,
privacy, permission or catalog-management policy returns to this specification.

## 3. Baselines, catalogs and revision coverage

Keep `mobile-v1`. Advertise `update_person_metadata`, `metadata_revisions` and
`metadata_catalog` only with complete server support. A new client explicitly
requests the metadata representation; older requests still need only their old
components. Add an immutable representation discriminator to generation/cache
identity, so a same-`mobile_revision` old bundle can be upgraded deliberately.
Never infer an editable empty tag/value set from omitted old fields.

An editable baseline requires the sealed Person metadata sections and the matching
complete catalog. Catalog pages include stable tag/field/option IDs, names/labels,
types, order and archive state; Person sections include all stored tag links and
typed values, including archived values. Definitions and options are distinct
paged records, not an unbounded options array. Preserve exact decimals as strings
and date strings independently of device locale. Unknown representation/type,
missing referenced catalog records or an oversized row disables submission with
an explicit error; it must not erase existing cached data or pretend completeness.

Use existing 100-row/512-KiB component pages, generation lifetime, download slots
and cleanup. Snapshot construction, paging and local caching must stay bounded
even if archived definitions accumulate beyond live-catalog limits. A partial
catalog is visibly incomplete; continue or fail closed, never clip at live limits.
Freeze a measured temporary-byte bound for the new generation snapshots at the
backend checkpoint; do not turn that internal bound into a customer quota.
No whole-book metadata duplication into the per-Person summary.

Pin every page to context, representation, generation or editor traversal, Org,
Person where applicable, section and both relevant revisions. Seal validates
the current Person/catalog versions and all required components. A changed
catalog alone invalidates metadata reuse even if Person revisions are equal.
Current-editor reads perform final validation after complete traversal; a token
change discards only that transient traversal and preserves the draft. Old delayed
responses cannot cross actor/Org/context/editor epochs.

Person revision coverage includes ordinary Web tag/value commands, catalog tag
deletion cascades, original metadata import, admitted metadata import, fixtures
and erasure cascades. Actual value or membership changes advance both metadata
and broad mobile revision in the same transaction; no-op does not. Catalog-only
label/order/archive changes advance the catalog revision without changing stored
Person values. Initial tokens are 1; tokens are positive signed-64-bit decimal
strings, derived from previous state with overflow rejection and no caller reset.
Triggers maintain revisions only, never business facts or authorization.

Freeze the concrete lock graph before implementation. Current value/tag commands
lock Person before catalog rows, while catalog deletion can cascade to Person:
do not just add a revision trigger and assume those paths compose safely. Proposed
ordering is existing workspace/membership and any required migration Org/ledger
admission, then a shared metadata-catalog barrier for link/value work (exclusive
for catalog writes), then existing domain namespace locks, Person and catalog/value
rows in deterministic order. All affected writers enter that barrier before any
Person/catalog row they can later need. The mobile adapter must enter it before
its common Person lock. Catalog read-model rows are separate from `organization`
to avoid introducing a new Organization-row lock into ordinary metadata writes.
The checkpoint must verify this ordering against original/admitted import and
all affected catalog commands, preserving bounded waits and existing grants.
No unreviewed nested transaction or silent broader lock rewrite is permitted.

## 4. Operation, UI, privacy and recovery

Proposed payload:

```text
{person_id, expected_metadata_revision, expected_catalog_revision,
 actions:[
   {kind:"add_tag"|"remove_tag", tag_id},
   {kind:"set_field", field_id, value:<existing typed value shape>},
   {kind:"clear_field", field_id}
 ]}
```

Reject empty patches, repeated tag/field targets, conflicting operations, unknown
fields, malformed IDs/tokens and bodies over the existing 128-KiB operation limit.
IDs must resolve within the trusted Org and selected Person/catalog. Do not accept
client actor/Org/origin, source identity or permissions. Match expected revisions
before considering a fresh no-op. A deleted tag does not become a successful
remove/add merely because a same-name tag exists. Retain its proposal for review.

Exact authorized receipt lookup/replay precedes fresh revision/catalog validation.
An accepted retry remains accepted after later field archival/deletion; visibility
checks concern the Person, not continuing existence of each affected tag. Altered
input under an existing operation ID conflicts. Unknown outcomes use lookup/exact
retry before any replacement. Validate all actions and final tag capacity first;
compose the normal commands and store their single receipt in one transaction.
Any validation, receipt or database failure rolls back the entire edit. Publish
only changed categories after commit; no-op/replay publish nothing. Publication
failure does not change a successful command outcome.

Both apps show stored values and a clearly labeled pending proposal. Save succeeds
only after encrypted SQLite commits baseline, CAS draft revision, immutable
envelope and overlay together. On conflict show baseline/proposed/current values
and changed catalog labels/status; let the user keep the draft, use current data,
or explicitly create a new proposal against the newly qualified baseline. Never
offer force-overwrite or automatically retarget a deleted/archived option.

Accepted overlays persist until a causally covering sealed metadata representation
arrives. A failed refresh is stale display, not permission to send a new mutation.
Pending tags/values do not locally rerank authoritative Today or claim server
filter membership. Seven-day access expiry, logout/account changes, revocation,
locked keys and protected-work retention follow Mobile001–005 unchanged.
All proposed/catalog/value content stays out of logs, metrics, realtime and receipt
audit metadata. Existing encrypted on-device storage/backup exclusions cover new
tables; custom-field plaintext server CRUD remains under D-058/O-013. No retention
or erasure policy is invented here.

## 5. Acceptance and evidence

| ID | Required observable result | Verification |
|---|---|---|
| M6-01 | Both platforms save mixed tag/field actions offline, restart, sync once; accepted no-op/lost-response retry has one receipt | Native store tests and real simulator/emulator + isolated API journey |
| M6-02 | Metadata ABA and competing edits conflict; unrelated profile/stage/note/task changes do not; any changed catalog requires review | DB all-writer tests, two-client native conflict walkthrough |
| M6-03 | Four types, exact decimal/date rules, archived clear, deleted tags, foreign options, duplicate actions and final tag cap behave as §1/4; invalid action rolls back siblings | Rust validator/DB/HTTP fixtures and both native form tests |
| M6-04 | Partial/old/oversized metadata is never editable; all pages, catalog-only changes, final seal and same-revision representation upgrades are correct | Backend pagination/contract tests; native malformed/late-response tests |
| M6-05 | Workspace/tenant/actor/lease boundaries hold in HTTP and direct typed commands, including demotion during commit and receipt visibility | Negative DB/API/receipt tests with two Orgs and a review workspace |
| M6-06 | All real writers and deletion cascades advance the correct revisions; no-op, overflow, erasure and guards remain correct | Trigger/command/import compatibility and bounded concurrency tests |
| M6-07 | Existing Web/Operator command shapes and both event variants remain compatible; no payload content, no publication on rollback/replay | Exact wire fixtures and Web invalidation tests |
| M6-08 | Populated installed Mobile005 apps upgrade without reinstall/key replacement, retaining mixed old/new drafts, outbox bytes and receipts | Real installed simulator/emulator upgrade inventories; storage/key-failure tests |
| M6-09 | New paging/query plans are bounded/indexed at D-050's realistic book; unaffected Person/Today reads meet paired regression and payload requirements | One paired run and new/changed hot-statement EXPLAINs |

The [paired plan](../plans/MOBILE_006_010f4_PARALLEL_LAUNCH.md) owns exact checks,
ownership and independent-review gate. Physical phones/cellular and distribution
remain deferred. Acceptance entries are required future evidence, not test results.
