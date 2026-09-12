# Slice 010f1 — Tags and custom fields in the migration review workspace

**Approved sibling amendment — 010f2 (D-068, 2026-09-11):**
[010f2](SLICE_010f2.md) adds an independent retained notes/tasks child, without
requiring metadata-child completion or rewriting this child's state/results.
Both use the same Organization/snapshot retention admission locks, with separately
owned reservations and cancellation. First activity confirmation also establishes
a durable bounded-reader requirement; future launch/recovery must preserve it
and verify `fub-activity-import-v1` in addition to existing capabilities.

**APPROVED FOR IMPLEMENTATION — D-066, 2026-09-11.**
Implementation and synthetic verification are complete;
[executed evidence](../tasks/SLICE_010f1_VERIFICATION.md). The D-066 follow-up
authorized integration and shared-development deployment: implementation
`f37ddd1`, merged/pushed and [deployed as `e36ce360`](../tasks/SLICE_010f1_RELEASE.md).
The user approved the reviewed specification and brief with “ok proceed with
010f1.” The proposal wording below records the reviewed contract; its policies
are now accepted within this slice. Live FUB validation and activation remain deferred.

Historical planning context:
2026-09-11, inspected main `f01c2e3` after the verified 010c deployment.
D-065's next-import follow-up authorizes this draft, not its proposed defaults,
shared contracts, customer-data processing or implementation. Review this spec
and [brief](../tasks/SLICE_010f1_IMPL.md) before requesting full approval.
[Code evidence](../research/SLICE_010f1_CODE_CONTRACTS.md) distinguishes existing
contracts from proposals. D-050, D-051, D-058 and D-063–065 remain authoritative.

## 1. Outcome, scope and proposed policy

An Organization admin can preview and explicitly confirm tags and custom-field
definitions/options/values from the retained snapshot that produced its completed
010c People import. The child import attaches metadata to those exact People,
reports every held or unsupported item, and leaves the Organization under review.

The proposed first step permits one child import for the completed parent, with
immutable preparation revisions and retry-safe execution of one confirmed plan.
The original workspace binding, parent plan/results/provenance and snapshot
boundary are immutable inputs. Neither completion nor cancellation activates the
workspace. A cancelled child retains partial work and cannot start another child
in this slice. A cancelled, running or paused 010c parent is ineligible.

Proposed defaults requiring approval with this spec: preserve all destination
limits; require explicit matching creation or existing-target mapping; hold
unsupported/ambiguous data; never overwrite a differing local value. Independent
valid metadata may proceed while other fields/tags/People are held, but the admin
must explicitly acknowledge the counted subset. No arbitrary renaming, type
conversion, truncation, rounding, default filling or many-field value merging.

Only embedded People tags are available: this is not a complete standalone tag
catalog import. Captured custom-field definitions, including unused definitions,
are inspectable and individually selectable. Values and tag links apply only to
live People proved by committed parent results and identity maps; never identify
by name, email or phone. Parent-held/unimported People remain explicit exclusions.

Excluded: new FUB requests, credential validation, delta/repair imports, another
snapshot, parent mutation, new People/contacts/stages/assignments/Inquiries,
notes/tasks, mail/media, privacy-policy inference, activation/cutover, destructive
Undo, native apps, operational notifications and Operator execution. Live FUB
validation and customer-data readiness remain deferred; synthetic proof does not
close them. Notes and tasks are the next separately specified import work.

## 2. Current → proposed shared contracts

The future 010f1 implementation owner owns only these changes after approval.
Exact serialization/table names may be frozen in an owned concrete contract
before coding; doing so cannot change the policies in this specification.

| Current contract | Proposed change and reason | Affected scope / compatibility |
|---|---|---|
| 010c workspace binds one People import/confirmed plan | Separate metadata child references that unchanged binding, completed parent and same snapshot/account/final sequence | Additive child tables; no migration/reopening of parent plans or mode transition (§3–6) |
| Import source/identity families cover People, stages and supporting users | Child-qualified People metadata and custom-field definitions; child identities for tags/fields/options plus per-Person operation results | New scoped rows and crypto purposes; preserve existing tombstones and 010b/010c responses (§3–5) |
| Review write permit validates a running workspace-bound 010c import and a small table allowlist | Separate private child permit validates its lease, confirmed plan, executor and original binding | Extend application and DB guards only for child-owned metadata operations; no general admin/Migration-Origin bypass (§6) |
| Ordinary tag/custom-field commands require operational workspace; value upsert can replace values | Typed import operations reuse validators/locking, create or map approved catalog entries and add links/set absent values only | Ordinary command/API behavior and destination limits unchanged; IDs-only migration results, no value history facts (§4–6) |
| New custom-field CRUD leaves reserved source/external_key unset; tags/options have no source key | New imported fields populate `source=fub` and exact qualified machine key; separate child identities retain all associations | Never rewrite an existing field's provenance; source-key conflicts/archived targets held (§4–5) |
| Snapshot accounting has source, preview and 010c owners | Child-owned work/control reservations and exact variable-byte ledger charge the same snapshot/Org allowances | Preserve parent counters; current policy ceilings clamp every admission; no extra customer quota (§7) |
| Migration UI offers People import; parent Person provenance covers original Person | Add admin child plan/mapping/progress and scoped metadata provenance reads | Additive routes/UI; `/me` mode/revision and member waiting remain unchanged; existing clients fail closed (§8) |

Implementation must amend owning migration summary/contract pointers through the
coordinator, preserve historical 010c contracts, and document compatible deployment
and recovery. No release action is authorized by approving implementation alone.

## 3. Source qualification and frozen inputs

Require the exact `fub-core-v1` snapshot/preview accepted by the parent, its final
capture boundary, completed parent, unchanged confirmed parent plan and current
`migration_review` binding. In addition to 010c's exhausted People/users/stages,
the custom-fields stream must be exhausted. A snapshot completed with unrelated
gaps is allowed with those gaps prominently retained; a missing/incomplete
custom-fields stream is ineligible, not an empty schema. No new source reader is
injected into preparation or execution; disconnect/credential rotation is irrelevant.

Qualify People and custom-field definitions from decrypted original captures,
not display projections, preview suggestions or only the two example variants.
Check Org/snapshot/sequence, profile, stream/representation, accepted successful
2xx nontruncated capture, item ordinal, exact positive source ID and semantic HMAC.
Use the existing bounded lossless parser: duplicate decoded keys, invalid encoding
and excessive structure fail closed. All observations of each relevant ID take
part; rejected disagreeing or unqualified evidence cannot be promoted to a winner.
Same canonical semantics may collapse; accepted unknown-field-only variants still
hold the source item. Supporting definitions obey the same rules as People.

Source IDs remain decimal strings, up to the existing 128-digit bound. A field's
`id` is its source identity; `name` is the exact People property key, and `label`
is its human label. Resolve by qualified exact `name`, never by label, `custom*`
prefix alone or case folding. Conflicting IDs with the same key hold both field
mappings and dependent values. An undeclared custom-looking property is preserved
and reported unresolved; it cannot create a guessed field. Definition absence
does not prove the corresponding People property was never present.

Tags are string elements of a qualified People `tags` array. Retain every spelling,
ordinal and duplicate before native normalization. Missing/null array is reported
as not supplied; an empty array is explicitly empty. A non-array is held as a
collection; invalid elements are counted with ordinals and never fabricated IDs.
Other valid elements can be planned. Tags seen only on parent-excluded People
remain coverage evidence and are not automatically created.

Custom-field choices are captured strings, not source option IDs. Identity for a
choice is scoped to its qualified source field and exact captured label; retain
original order and duplicate ordinals. Duplicate or case-colliding source choices
are ambiguous and hold that choice field and its dependent values in v1. Do not
invent vendor option IDs or discard options to fit the destination.

Retain canonical exact fields plus raw capture references, unknown properties,
source names/labels/values, flags, missing-versus-null and all transformations.
Neither privacy flags nor successful capture grant communication permission.
Parent eligibility/holds cannot be reversed by this child. Corrupt/missing keys or
lost evidence pause affected work before a write and remain distinct from source
data that is valid but unsupported by the destination.

## 4. Mapping and native value rules

Freeze current destination definitions/options/tags, archival state and selected
targets in each plan. Suggestions are advisory. Choices are `create_matching`,
`map_existing` or `hold`, addressed by scoped server mapping IDs. Creating uses
the captured label/options after the destination's documented normalization;
no client-supplied arbitrary replacement text is executable. Each choice and its
dependent effect is inspectable before confirmation. Unchosen mappings stay held.

- Tags retain the existing 200-per-Org and 20-per-Person caps, the 1–40
  Unicode-code-point name rule and
  case-insensitive create-or-get semantics. Group native-equivalent spellings
  with explicit aliases; a new group's matching label is its first retained
  occurrence in capture/item/element order, disclosed for approval. Reuse keeps
  the existing tag spelling. Resolve actual database collation collisions under
  the namespace lock; Rust lowercase alone cannot authorize uniqueness.
  If approved new unique links plus existing links exceed 20 for a Person, hold
  that Person's new tag-link set, not an arbitrary first 20; independent field
  values may proceed. Explicitly holding catalog mappings can reduce that set.
- Fields retain 50 live definitions, immutable text/number/date/single-choice
  types and 1–60-code-point label rules. `dropdown` maps to single choice; other
  unknown kinds are held. Map only a live same-Org compatible-type field. Distinct
  source fields cannot map to the same destination field in this first step.
  New fields store the exact machine key with `source=fub`. Existing mapped
  fields keep their source pair; a conflicting existing source binding is held.
- Choice fields retain 1–50 live options, native label rules and case-insensitive
  uniqueness. For a newly created field, all captured choices must be representable
  together; no first-50 subset. For an existing field, approve each source choice's
  live target option or matching new option within capacity. Distinct choices
  cannot map to one option. Extra destination options remain unchanged. Mapping
  a field alone never silently approves new options.
- Archived definitions/options are never revived; deleted tag targets and identity
  tombstones never cause automatic recreation. A source label unfit for creation
  can still map to a valid existing target if its source identity/type is qualified.
  NUL/oversized native text is not sent to PostgreSQL suggestion/lookup bindings.
  New source machine keys use a conservative 2,048-UTF-8-byte index-input bound;
  retain exact larger keys encrypted and hold creation rather than hash or truncate
  the native `external_key`. Prove the composite index bound in implementation.

Append new catalog entries in the plan's frozen deterministic order; newly created
field options preserve captured array order. Existing positions stay unchanged.
Retain `orderWeight`, `hideIfEmpty` and other unsupported source settings as
explicit evidence/coverage limitations, not invented native behavior or defaults.

| Source value under its qualified field | Planned destination behavior |
|---|---|
| Missing property / explicit null | Separate `not_supplied` / `source_null` results; no set or clear, even if local value exists |
| Text JSON string | Existing ASCII whitespace trim and 1–500-code-point/control validation, with any trimming disclosed and original retained; empty/whitespace-only string held, not null |
| Number JSON number | Exact lossless decimal rendering only if the unchanged numeric value satisfies native 15 integer/4 fractional digit bounds; no floating point, rounding or overflow. Numeric strings/booleans are held, not coerced |
| Date string | Exact valid `YYYY-MM-DD` within 1900-01-01…2200-12-31; timestamps are not truncated. `isRecurring:true` or nonboolean recurring flag holds the field; absent/false means no recurrence behavior is imported and omission remains disclosed |
| Choice string | Exact captured choice-label lookup followed by its approved mapping; unknown/unmapped strings, arrays and multiple selections are held. Declared literals such as `None`, `N/A` or `null` remain ordinary choices, never inferred nulls |
| Object/array/boolean or incompatible shape | Held with a closed type/shape reason; exact value remains inspectable |

For every eligible value, compare typed destination state under lock. Absent may
be set; natively equal is `already_present`; differing is `held:local_value_conflict`.
Never clear or overwrite. Existing identical tag links likewise report already
present. New catalog rows attribute the local action to the current executing
admin; values use Migration origin and child correlation. Do not invent a FUB
creator or backdate local writes. Preserve actual source attribution separately.

## 5. Preparation, confirmation and atomic execution

One child root per parent/Org has revisioned plans and an immutable confirmed-plan
pointer. Proposed run states follow 010c: proposed, queued, running, paused,
completed, cancelled, expired; phases distinguish preparation/catalog/People/done.
Before confirmation, a replacement plan inherits choices in bounded pages then
applies at most 50 supplied patches. Only one plan builds; replacement fences the
old plan/cursors. A ready plan expires after ten minutes; the same unconfirmed
root may prepare a replacement. Cancelled roots are terminal, not repair handles.

Preparation reads one ≤4-MiB raw capture or one selected payload at a time, with
keyset discovery batches ≤50. It freezes qualified source references, parent
identity/result links, current targets, choices, dependency holds, proposed native
values, exact evidence, counts and an added-byte bound for every execution unit.
It does not rewrite parent source/manifest/provenance or use business tables as a
scratch plan. At least one eligible catalog/link/value operation is required to
confirm; unchosen/held/unsupported counts require explicit subset acknowledgement.

Confirmation binds request ID, actor, child, ready plan/revision/digest, expected
workspace binding/revision and subset acknowledgement. Recheck completed parent,
current admin, targets/capacity and server release readiness under locks; queue
atomically with the receipt. No workspace transition or second emptiness check.
Resolve and authorize an existing committed receipt before readiness/expiry checks:
an expired operator report or lost response cannot invalidate exact replay.
Altered request content conflicts; another actor never receives that receipt.

Execution uses 60-second fenced leases and current-admin revalidation on claim,
reclaim and each unit. Catalog dependencies settle first: one approved tag, one
field with its ≤50 options, or one approved option on an existing field per unit.
Then one Person unit atomically adds its approved links and absent eligible values,
preserving independent held cells. Each unit commits native rows, child identity,
encrypted provenance/result, checkpoint and exact byte settlement together. A
failed unit commits none of those; committed catalog entries may remain if later
Person work is cancelled. No operational publication or custom-value history fact.

Parent identity and committed result must agree on account/source ID/Person/Org;
missing or tombstoned Person is held, never recreated. New child identities are
unique by Org/account/source family/key and survive missing native targets without
cascading deletion. Per-Person operation identity additionally includes parent
Person source ID and field/tag source key; retries never select another Person.
Unqualified IDs have explicit counts, never fabricated source identity rows.
ID-less tag/choice keys are bounded application-owned tenant/account-scoped HMAC
identities, not vendor IDs or permanent plaintext-label tombstones. Choice keys
also bind qualified source field identity. Preserve exact labels/aliases encrypted
and verify equality on key collisions before assigning a target; never merge
different source evidence because a digest matched. Freeze purposes/serialization
in the concrete contract and charge those keys/evidence to the child ledger.

Retry is explicit and resumes the same frozen phase/plan. Authority, mapping target,
storage, key or integrity failures pause with closed distinct reasons. Restored
capacity/keys/readiness never auto-resume. Cancel fences queued/claimed work, waits
only for bounded transactional contention, retains committed metadata/evidence and
releases its own unused reservations. Completion means all planned items have
terminal reconciliation, not complete FUB coverage or activation readiness.

## 6. Authority, concurrency and compatibility

Current active Organization admins alone prepare/read/confirm/retry/cancel. Missing
session is 401, non-admin migration/provenance access 403, foreign IDs 404, and
ordinary held-workspace business requests remain 409. Keep complete authoritative
read permits and all 010c member/Today/Operator/realtime/intake/call restrictions.
No platform-admin implicit tenant access; no per-Person visibility exception.

Use the existing workspace shared barrier, bounded row-lock waits and lock order:
workspace → current membership → Organization → snapshot/Org ledgers → child/plan
→ metadata namespace/Person/target rows. Define one deterministic order for both
tag and custom-field locks. No external call or inference occurs within a DB
transaction. Unique child binding serializes competing preparations/confirms;
lease fences and source/result uniqueness resolve duplicate workers and late commits.

A private child write permit must independently validate the original workspace
binding, completed parent, child confirmed plan, active executor and live lease.
Its DB enforcement authorizes only planned tag/person_tag/custom_field/option/value
operations, in the same transaction as the corresponding result/identity. It cannot
create People or alter parent assignments/stages; an old 010c token cannot perform
child metadata writes. Do not broaden the existing permit's Origin/admin test or
disable triggers. Ordinary commands remain operational-only, including direct
domain calls and alternate edit/archive/clear paths.

Use an additive migration and minimal grants; preserve `crm-workspace-v1` and
its existing mode/binding semantics. Deploy known 010f1 API/worker artifacts before
enabling child confirmation through the existing server-owned preflight. Old
010c workers cannot claim separate child rows; mixed-version recovery must stop
child execution and use compatible 010f1 artifacts, never reset the hold or parent.
Document the narrow child capability check in the existing release path, without
a general release platform. Preserve 010c's pre-gate rollback prohibition. No
tenant checkbox asserts fleet retirement. Test with synthetic manifests/state.

## 7. Capacity, fidelity and reconciliation

Child reservations charge the existing snapshot and Org ledger with current
SnapshotPolicy clamps. They have separate ownership from source/preview/parent
work, retain historical revisions, settle exact variable bytes once, and release
only their own capacity. Reserve bounded cancellation-receipt capacity at proposal;
cancel remains possible at exhausted allowance. Receipts report committed counters
including their own storage; replay does not charge twice. No new higher quotas.

Before readiness, prove each unit's additional retained bound ≤the existing 64-MiB
unit ceiling, including escaped JSON, encryption overhead, source keys/identities,
all fan-out results and duplicated metadata. Intrinsic excess is held before
confirmation; insufficient allowance pauses before any native write. Freeze a
counted-column inventory; distinguish retained migration bytes from uncharged
native rows, indexes/TOAST/WAL and three database copies. Never present the logical
allowance as a PostgreSQL disk-size guarantee. Avoid copying an entire raw page
per value; reference retained captures and use erasable scoped per-item evidence.

At existing caps, 25,000 People can imply up to 1.25M values and 500,000 tag links, plus
2,500 live options. These are fan-out arithmetic, not raised quotas, promised
capacity or a required all-maxima benchmark. Report measured per-unit/row byte
sizes, aggregate estimates and index/physical overhead separately; use a documented
worst realistic metadata book plus dense/shared and small-limit adversarial cases.

Report source People eligible/excluded and separate catalogs, tag links and values:
planned, created/applied, already_present, held, not_supplied/source_null, pending.
Keep nonexclusive closed issue counts and dependency counts separate from mutually
exclusive dispositions. Per-item reconciliation links exact source evidence,
approved target/normalization, committed native ID, parent identity and child plan.
All original fields remain accessible through bounded scoped UTF-8 segments,
including long mapping keys/labels and full collections; abbreviated summaries
must advertise clipping/counts. No source strings/values in facts, logs, metrics or
operational realtime. Migration provenance follows retained-content encryption and
erasability; child identities retain only the minimum tombstone, never value history.

## 8. HTTP and Web proposal

Use additive `/api/migrations/fub/metadata-imports`: create with parent_import_id
and request_id; scoped list/detail; plan/replan with expected revision and choices;
confirm/retry/cancel actions. Add keyset records/mappings/results/issue pages and
record/mapping field-segment reads; expose Person metadata provenance to admins.
Strict bodies reject unknown fields, use decimal string counters/source IDs and
≤64-KiB mutation bodies. Mapping patches total ≤50; discovery/page sizes ≤50,
display responses ≤512 KiB and individual summaries ≤128 KiB. Field segments use
4–65,536 UTF-8 bytes with guaranteed scalar-boundary progress. Cursor AEAD binds
Org/child/plan/revision/endpoint/filter/selected record or mapping/field separately.
Current admins retain source-free access after disconnect. Responses are no-store.

Web offers “Import tags and custom fields” only for the eligible completed parent.
Show retained coverage/limits, source-to-target choices, full-field inspection,
unsaved mapping changes, counted omissions and a named explicit confirmation
dialog. Preparing/selecting a suggestion never confirms or creates native data.
Dirty choices require apply/discard before confirmation. Progress updates refresh
real result rows/links without losing filters/cursors; paused/terminal polling stops.
Budget approval and Retry are separate actions; cancellation explains retained work.

Reuse authoritative workspace refresh/focus fencing and same-identity uncertain
request replay. Reject late responses on Org/actor/role changes; no receipt changes
session authority. Show admins read-only Person metadata/provenance; members remain
on the waiting screen. Native/Operator contracts gain no execution tools. Walk
through desktop and 390px responsive Web, including named dialogs and long content.

## 9. Acceptance and verification

| ID | Observable acceptance | Required proof |
|---|---|---|
| A1 | Only completed bound parent/same frozen snapshot is eligible; custom-fields exhaustion required; no FUB calls | Typed/HTTP eligibility and reader-counter tests; foreign/cancelled/running parent cases |
| A2 | Exact raw People/definition qualification and all variants; corrupt/unsupported evidence never becomes a native value | Lossless numeric/duplicate-key/ordinal/HMAC/capture/key tests, including altered display projections |
| A3 | Embedded-only tag coverage, machine-key/choice collisions, null/empty and unsupported types remain honest | Fixtures for every §3–4 distinction; exact provenance/segment reassembly, large fields and NUL |
| A4 | Explicit catalog choices respect existing quotas, labels, archive, source bindings and no many-field merge | DB validator/capacity/collision/active-target tests; 20-tags-per-Person overflow holds the set, no first-20 subset; unique keys and option dependencies |
| A5 | Only committed parent People receive links/absent values; equal is idempotent, different held | Separate overlapping People, parent-held/tombstoned targets, direct native-state assertions; no Person/Inquiry/history writes |
| A6 | Plan revisions/confirm receipts/expiry and exact lost-response replay are fenced and authorized | HTTP concurrency, stale/foreign cursors, changed actor/body, expired readiness and replay tests |
| A7 | One atomic catalog or Person unit; crash/retry/cancel never duplicates or permits late commit | Deterministic transaction barriers; identity/result/native/cursor/ledger checks before/after commit |
| A8 | Review hold and narrow child permission survive direct calls, role revocation and old tokens | Domain/DB/HTTP tests for child vs parent permits and ordinary writes; member/admin/tenant matrix |
| A9 | Exact accounting includes fan-out/control receipts and current ceilings; exhausted cancel works | Small-limit fixtures, >2-MiB evidence, per-column reconciliation, policy lowering/explicit retry |
| A10 | New hot queries stay indexed/bounded at D-050 scale, including rare/empty filters | Actual EXPLAIN(ANALYZE,BUFFERS) for claim/source/dependency/manifest/result/field paths; no all-book per-Person scans |
| A11 | Review UI handles dirty choices, uncertainty, progress, authority changes and long evidence | Focused Web tests plus synthetic production-build desktop/390px browser walkthrough |
| A12 | Existing 010a/b/c/native CRUD responses and workspace isolation remain compatible | Final repository gates and synthetic startup/preflight compatibility proof; owned contract/verification records |

Use at most two implementation review/fix rounds under D-050. Collect plans for
new/changed hot statements, including sorted/filter variants that change shape;
document bounded tail scans for empty filters rather than promise 50 probes for
every selectivity. Do not force planner settings. No full CRM-reader benchmark
is mandated if shared reader SQL/behavior is unchanged; if changed, run one paired
baseline/final regression under D-050, including the relevant five-concurrent Today
case. Exceeding logical capacity must fail closed, not require speculative scaling.

## 10. Decisions to approve, not assumptions already accepted

Full review/spec approval must accept or amend: completed-parent-only/one-child
lifetime (including terminal cancellation); independent held-cell subset execution;
explicit matching creation/mapping with unchanged quotas; exact no-overwrite policy;
and conservative type/recurrence/collision rules in §3–4. Alternatives are later
repair/delta support, larger quotas, renaming/coercion, value replacement or richer
field kinds; each changes fidelity/customer behavior and is outside this draft.
No source-account access, new research, production architecture or activation
decision is needed to review this bounded proposal. Stop at the reviewed draft
until the user approves its specification, brief and shared contracts.

## D-070 amendment — independent historical capture

D-070's [010d1](SLICE_010d1.md) adds an independent history source capture bound
to the completed People parent. It does not change this child's lifetime,
confirmed source boundary, mappings or results. Shared Org byte accounting and
reservation ownership include history; metadata cancellation cannot release
history reservations.
