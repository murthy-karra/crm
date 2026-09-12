# Slice 010f1 — Concrete backend contract

Implementation detail under D-066 and the approved [spec](SLICE_010f1.md),
2026-09-11. This freezes the child backend/Web seam before code. It does not
authorize source requests, real customer processing or deployment.

## Authority and lifecycle

Engine `fub-metadata-import-v1`; original workspace gate remains
`crm-workspace-v1`. The child is unique per Organization/parent import for its
entire lifetime, including cancellation. Parent state must be `completed`, its
confirmed plan must match `migration_workspace`, and the Organization must remain
`migration_review`. Snapshot/account/preview/final capture sequence are copied
from that parent and revalidated. All four People/users/stages/custom_fields
streams must be completed; unrelated snapshot gaps remain explicit coverage.

Run states: `proposed`, `queued`, `running`, `paused`, `completed`, `cancelled`,
`expired`. Run phases: `preparation`, `catalog`, `people`, `complete`. Plan states:
`building`, `ready`, `paused`, `failed`, `superseded`. Preparation phases:
`copying_choices`, `captures`, `catalog`, `mappings`, `people`, `ready`.
A ready plan expires after ten minutes. An unconfirmed root may prepare a new
revision, including after expiry. A confirmed/cancelled/completed root cannot
replan. Confirmation permanently selects one plan and never alters the parent.

All reads/actions require a current active same-Org admin. Acquire workspace
shared barrier, current membership, Organization row, snapshot/Org ledgers,
child/plan, then metadata namespace/Person/target locks. Set a transaction-local
two-second row-lock timeout before membership/Org locks, including initial claim.
Claim/reclaim and every unit revalidate executor, parent/binding and lease.
Leases are 60 seconds with fresh UUID fences. No network work inside transactions.

The private child permit uses its own transaction-local token, independently
validated against the running child/confirmed plan/executor/lease and original
completed parent. Only tag, person_tag, custom_field, custom_field_option and
person_custom_field_value INSERTs belonging to the planned unit are authorized.
No metadata DELETE/ordinary UPDATE, Person write or old People-import token grants
this capability. Equal existing values/links do not issue an UPDATE. Ordinary
commands/guards, operational restrictions and original parent permits stay intact.

## Source and pure extractor

Raw qualification matches 010c: original capture scope/profile/sequence,
accepted successful nontruncated 2xx representation, item ordinal/exact source ID
and semantic HMAC. Every relevant observation participates. A rejected identical
observation is not a winner; an accepted identical observation may qualify it;
any disagreeing or invalid observation holds the candidate. Preview JSON is never
an input. Exact source IDs are positive decimal strings, at most 128 digits.

Pure module `metadata_source.rs` owns these crate-private serializable types:

```text
Record { source_id: Option<String>, canonical: Vec<u8>, entity: Entity,
         reasons: Vec<String>, transformations: Vec<String>,
         provenance: BTreeMap<String,String> }
Entity = Person(PersonInput) | Field(FieldInput) | Invalid
PersonInput { tags_state: String, tags: Vec<TagInput> }
TagInput { ordinal: u32, raw: Option<String>, label: Option<String>,
           reasons: Vec<String> }
FieldInput { name: Option<String>, label: Option<String>,
             field_type: Option<String>, choices: Vec<ChoiceInput>,
             reasons: Vec<String>, creation_reasons: Vec<String> }
ChoiceInput { ordinal: u32, raw: Option<String>, label: Option<String>,
              reasons: Vec<String> }
ValueInput { disposition: String, value: Option<NativeValue>,
             reasons: Vec<String>, transformations: Vec<String> }
NativeValue = Text(String) | Number(String) | Date(String) | Choice(String)
extract_page(Stream, &[u8]) -> Result<Vec<Record>, ParseError>
extract_value(&Record, &FieldInput) -> ValueInput
```

Entity and NativeValue serialize using snake_case `kind`/`value` tagged enums.
`provenance` values are exact canonical JSON text, never parsed through f64;
canonical bytes are used for HMAC and cleared before storing the derived Record.
Only People/CustomFields streams are accepted, ≤4 MiB raw and ≤100 items using
the existing bounded duplicate-key-rejecting parser. Capture qualification and
parent identity resolution belong to the caller, not this pure module.

`tags_state`: `not_supplied`, `source_null`, `empty`, `present`, `held`.
Tag raw spelling/ordinal survives; label is existing normalized native name when
valid. Invalid elements have no invented identity. Field reasons hold all its
dependent values; creation_reasons only prevent matching creation, preserving
explicit existing mapping for unrepresentable labels/keys. Recurrence true or
nonboolean, unsupported kind, malformed/colliding choices hold a field. Missing
recurrence is disclosed, not claimed imported. Native limits remain unchanged.

`extract_value` distinguishes missing/null from empty strings, validates the four
native types, and returns `eligible`, `not_supplied`, `source_null` or `held`.
Number input must be a JSON number: derive exact bounded decimal text from the
lossless coefficient/exponent, without allocation proportional to a huge exponent.
No numeric-string conversion, rounding, timestamp truncation or null-like string
inference. Choice value remains the exact source label until approved mapping.

## Persistence and identity

One additive migration introduces separate `migration_metadata_*` tables:

| Table suffix | Owned state / keys |
|---|---|
| import | Org, parent import/plan, snapshot/account/boundary/workspace revision, executor, run state/phase, latest/confirmed child plan, lease and work/control byte counters; unique parent/Org |
| plan | Immutable revision inputs and encrypted choice patch/destination catalog, phase checkpoints including an ancestor-plan inheritance cursor, ready digest/expiry, closed counts; unique child/Org/revision |
| choice | Frozen per-revision kind/source-key choice, encrypted; unique plan/Org/kind/key |
| source | Qualified People/custom-field evidence referencing exact retained record/capture; canonical HMAC, conflict/observation counts and encrypted Record; unique plan/Org/family/source ID |
| mapping | Tag/field/option source mapping with source/dependency references, frozen choice/target/reasons/suggestions, native group/name HMACs and byte bound; unique plan/Org/kind/key |
| alias | Bounded exact-tag alias key, group mapping and source-row/element reference; no raw plaintext labels; indexed by mapping and source |
| manifest | One planned Person metadata unit, parent result/identity target, exact encrypted operations/holds/evidence references and byte bound; unique plan/Org/source Person ID |
| operation | Bounded per-Person planned tag/value descriptors (mapping/target IDs and disposition only); scoped manifest FK, indexed native target; lets the DB permit verify native INSERT target without decrypting source content |
| identity | Org/account/kind/derived source key → native target plus immutable child/mapping provenance; no target FK/cascade, never raw label tombstones |
| result | Catalog or Person terminal result, native IDs, closed per-operation outcomes and encrypted provenance; unique child/Org/unit key; indexed plan/unit and Person |
| receipt | Actor/action/request-ID input digest and encrypted committed response; append-only, scoped child/Org |
| reservation | Child/plan/lease/snapshot/Org owner, work or cancel purpose, byte count and fence |
| issue | Plan/Org/closed issue-code nonexclusive counts, updated with the corresponding bounded unit |

All child references have composite tenant/parent keys. Do not change existing
parent identity/source/workspace tables or 010b record payloads. Catalog native IDs
are deterministic frozen UUIDs for approved creation; an existing-target mapping
preserves the native row. New fields set `source=fub` and the exact machine name;
existing source pairs remain unchanged. A conflicting pair/archived target holds.

Derived keys are 32-byte tenant/account/purpose-scoped retained HMACs. Tag group
keys use verified native case equivalence; each distinct exact source alias has
its own HMAC and encrypted original reference. Choice keys additionally bind the
qualified source field ID and exact label. Compare exact decrypted source/native
group text on digest collisions; mismatch holds, never merges. Field identity
binds the exact source ID. Plaintext labels/values never become permanent keys.

Native destination freeze contains ≤200 tags, ≤50 live fields and ≤50 live options
per field; selected source-key/archived conflicts are separately recorded. It is
encrypted, bounded and never treated as session authority. New entries append in
frozen order, new field choices use captured array order, existing order remains.

## Mapping and atomic work

Kinds `tag`, `field`, `option`; choices `hold`, `create_matching`,
`map_existing {target_id}`. Patches refer to current-plan mapping UUIDs; the server
resolves stable kind/key for inheritance. No client label/type/value is accepted.
A new revision materializes its own at-most-50 choices atomically with its patch.
The worker then walks predecessor choices newest first, at most 50 descriptors or
one exhausted ancestor per unit. Existing choices in the new revision win; an
interrupted predecessor continues through its immutable parent lineage. A rapid
replacement cannot lose accepted edits which had not yet been copied. Choice
ciphertext/source keys and variable cursor bytes follow the same exact ledger.

Suggestions are optional IDs, not implicit choices. Mapping a field never approves
new options: a new choice field requires all its source options explicitly chosen
for matching creation; existing fields permit explicitly mapped/created options.
Duplicate/case-colliding source choices and many-source-field/option target
collisions hold. Source choice labels are also compared with PostgreSQL `lower`
before option decisions; native-equivalent labels hold the field and dependent
options/values even when Rust Unicode folding does not equate them. All captured
definitions remain inspectable, including unused.

Catalog execution units are one tag, one new field with its ≤50 approved options,
or one new option under an existing field. Each settles identity/results/native
rows atomically. Person units add ≤20 total native tag links and ≤50 eligible
native values; >20 planned-plus-existing tags holds that Person's entire new-link
set. Equal data is already_present, differing values are held; no blind upsert.
Independent held cells do not prevent other approved operations. Parent-excluded
or tombstoned People cannot receive metadata. Units with no writes still reconcile.
Manifest eligibility describes parent/source eligibility; operation-level holds do
not turn a qualified Person into a parent-excluded record.

An undeclared top-level `custom*` property
produces one held value operation with `source_definition_unavailable`, exact
source-field name and original source evidence, without a guessed mapping/type.
A malformed tag collection holds the Person aggregate with
`tags_shape_unqualified` while independent valid operations can still apply; it
does not invent a held tag-link unit. Result counters count actual catalog/link/
value operations (`planned`, settled disposition, zero `pending`), with
`held_count` equal to held operations only. Person aggregate precedence is held,
applied, actual already_present, source_null, then not_supplied. Empty/no-applicable
units cannot claim already_present; exact null/empty distinctions remain in source.

Each unit atomically commits native data, identity/provenance/result, counters,
checkpoint and bytes. Cancellation retains committed catalog/Person work and
releases only this child's unused capacity. Retry resumes the frozen phase and
adopts the current authorized admin without altering original request receipts.
Terminal completion has no activation implication or operational publication.

## Byte and crypto accounting

Current SnapshotPolicy clamps both stored run and Org allowances on every new
admission. Child charges roll into snapshot/Org totals, never the parent import's
retained/reserved counters. Separate child reservations prevent source/preview/
parent cancellation from releasing them. Proposal reserves a 64-KiB cancellation
receipt allowance; terminal completion releases it; cancel settles it even when
no additional capacity remains. Receipt replay does not charge again.

Maximum unit is 64 MiB, preparation reads one raw capture or selected payload at
a time and discovery batches contain ≤50 descriptors. Freeze each executable
unit's bound before ready: exact serialized encrypted input/provenance lengths,
worst-case new result/identity keys and JSON escaping, nonce/tag overhead and
bounded per-operation outcomes. Source/derived payload excess fails before storing
that unit; intrinsic executable excess is held before confirmation. >2-MiB
evidence remains supported. Do not duplicate a full source Person per value.

Count persisted nonce+ciphertext, semantic/input/name/identity HMAC bytes, source
IDs, variable cursor strings, representation/field keys and derived identity data.
Fixed UUIDs/timestamps/closed enums/native integer counters are excluded; closed
counts encoded as JSON contain no dynamic source keys. Native business row bytes,
indexes/TOAST/WAL/replicas are separately reported, not hidden within logical
migration admission. Net replacement deltas and receipt fixed-point sizing are
settled once. Capture exact counted-column inventory in migration/tests.

AEAD uses existing snapshot/receipt crypto with `metadata-v1:{plan}:{purpose}`
separation and Org/snapshot/row scope. Purposes distinguish source, destination,
patch, choice, mapping, manifest, result and cursor; receipts additionally bind
actor/action/request. Cursor context binds endpoint, child, plan/revision, filter,
selected record/mapping/Person and field. HMAC purposes never reuse source semantic
or parent import keys. All source content is untrusted text, not executable UI.

## HTTP and Web wire contract

Base `/api/migrations/fub/metadata-imports`; UUIDs as strings and counts/revisions/
source IDs as decimal strings. Successful responses use `Cache-Control: no-store`.
All bodies reject unknown fields and exceed 64 KiB with 413. Empty bodies are not
implicit confirmation. Existing parent routes/envelopes are unchanged.

| Method/path | Request | Response |
|---|---|---|
| POST base | `{request_id,parent_import_id}` | 201 `{import:Detail}` |
| GET base | optional `parent_import_id`, cursor, limit | 200 `{imports:Detail[],next_cursor}` |
| GET /{id} | — | 200 `Detail` |
| POST /{id}/plans | `{request_id,expected_plan_revision,mappings:[{mapping_id,choice}]}` (≤50) | 202 `{import:Detail}` |
| POST /{id}/confirm | `{request_id,plan_id,plan_revision,confirmation_digest,workspace_revision,acknowledgments:{held_count,review_only:true,remaining_data:true}}` | 202 `{import:Detail}` (durable receipt replay) |
| POST /{id}/retry or /cancel | `{request_id}` | 202 `{import:Detail}` |
| GET /{id}/plans/{plan}/mappings | cursor, limit, optional kind | 200 `{items:Mapping[],next_cursor}` |
| GET /{id}/plans/{plan}/targets | cursor, limit, kind, optional field_id | 200 `{items:Target[],next_cursor}` |
| GET /{id}/plans/{plan}/records | cursor, limit, optional disposition | 200 `{items:RecordSummary[],next_cursor}` |
| GET /{id}/results | cursor, limit, optional kind/disposition | 200 `{items:ResultSummary[],next_cursor}` |
| GET /{id}/plans/{plan}/issues | cursor, limit | 200 `{items:[{code,count}],next_cursor}` |
| GET /{id}/plans/{plan}/mappings/{mapping}/aliases | cursor, limit | 200 `{items:Alias[],next_cursor}` |
| GET /{id}/plans/{plan}/records/{record}/fields/{field} | cursor, limit | 200 `Segment` |
| GET /{id}/plans/{plan}/mappings/{mapping}/fields/{field} | cursor, limit | 200 `Segment` |
| GET /api/people/{id}/metadata-import-provenance | cursor, limit | 200 `{items:ResultSummary[],next_cursor}` |
| GET /api/people/{id}/metadata-import-provenance/{result}/fields/{field} | cursor, limit | 200 `Segment` |

`choice` is exactly `{kind:"hold"}`, `{kind:"create_matching"}` or
`{kind:"map_existing",target_id}`. Invalid choice/type/eligible-source failures
are 422 with existing closed `invalid_import_choice`/`source_not_eligible` codes.
Missing/foreign IDs are 404; non-admin 403 precedes held business 409. Malformed
query/body/cursor is 400; stale revision/body reuse/confirmed-plan conflict 409
`import_conflict`; expired plan 409 `import_expired`; admission storage_limit 409;
unavailable key/storage/release evidence 503. Resolve authorized committed confirm
receipt before checking current readiness/expiry; new confirmation still requires
fresh server-owned release evidence. Never require a credential/FUB call.

```text
Detail { id,parent_import_id,parent_plan_id,snapshot_id,source_account_id,
  capture_sequence,workspace_revision,engine_version,state,phase,pause_reason,
  created_at,updated_at,confirmed_at,confirmed_plan_id,completed_at,retained_bytes,reserved_bytes,
  cancellation_reserved_bytes,release_ready,policy,coverage,counts,
  latest_plan: Plan|null, actions:{replan,confirm,retry,cancel} }
Plan { id,revision,state,phase,pause_reason,expires_at,confirmation_digest,
       counts,max_added_byte_bound }
counts { people:{source,eligible,excluded,settled},
  tags: Dispositions, fields: Dispositions, options: Dispositions,
  tag_links: Dispositions, values: Dispositions,
  held_count,invalid_source_ids,issues:[{code,count}] }
Dispositions { planned,eligible,created,applied,already_present,held,
               not_supplied,source_null,pending }
policy { run_byte_limit,org_byte_limit,run_ceiling_bytes,org_ceiling_bytes,
         run_retained_bytes,run_reserved_bytes,org_retained_bytes,
         org_reserved_bytes,unit_byte_limit,policy_revision }
coverage { embedded_tags_only:true,custom_fields_complete:true,
           metadata_excluded_people,remaining_data:[String],source_gaps:[String] }
Mapping { id,kind,parent_mapping_id,source_id,field_id,disposition,qualified,create_matching_available,choice,
  target_id,target:Target|null,reasons,suggestions:[Target],dependent_count,source:Summary,
  added_byte_bound,alias_count }
Target { id,kind,field_id,label,field_type,source_bound,archived:false }
RecordSummary { id,source_id,person_id,disposition,reasons,counts,
                added_byte_bound,source:Summary,operations:Summary }
ResultSummary { id,kind,source_id,person_id,mapping_id,record_id,disposition,
                committed_at,counts,reasons,source:Summary,operations:Summary }
Alias { source_id,record_id:UUID|null,ordinal,source:Summary }
Summary { fields:[{key,label,text,full_utf8_bytes,abbreviated}],
          total_fields,abbreviated,field_key:"source.all"|"operations.all" }
Segment { text,full_utf8_bytes,offset_bytes,next_cursor,complete }
```

Nullable fields are present as null, including source_id for derived catalogs.
Coverage.metadata_excluded_people is the current child plan's excluded source
Person count. It includes unavailable/excluded parent People and child-level source
requalification holds; it is not a count attributable only to the parent import.
Operation-level tag/value holds on an otherwise qualified Person are not included.

Mapping.create_matching_available is a server-derived boolean shared with typed
create-choice validation: source qualification, available validated label, field
creation restrictions and known source collisions must permit creation. It is not
a quota or future-execution promise. A valid oversized source machine key can
therefore allow explicit existing-field mapping while disallowing matching create.

Option Mapping.field_id exposes the resolved existing parent destination field;
it is null until that field choice is applied/replanned, or for new-field options.
Alias.record_id points to its same-plan Person manifest when available so the
existing record source.all endpoint exposes its exact original tags/ordinal.
`confirmed_plan_id` is null before confirmation and authoritative for selecting
preparation versus execution views; confirmed_at is its timestamp. Summary field
keys are `source.<sha256>` or `operations.<sha256>` within the enclosing item;
`source.all`/`operations.all` expose that exact section. Endpoint field `all`
exposes an envelope containing both full source and operations, never an ambiguous
section. Mapping source keys use the same `source.` prefix. Prefixes are
≤1024 UTF-8 and escaped-JSON bytes, ≤20 fields,
individual summary ≤128 KiB and page ≤512 KiB; pages shrink before exceeding the
ceiling. Ordinary pages default/max 50, minimum 1. Segments use 4–65,536 UTF-8
bytes, default 16,384, with scalar-boundary progress and non-reusable scoped cursor.
Issue totals are closed/nonexclusive and held_count is the explicit sum of held
catalog/link/value dispositions, not a claim of unique affected People.

Pause reasons: `authority_changed`, `source_evidence_unavailable`,
`source_integrity`, `mapping_target_changed`, `storage_limit`,
`retained_key_unavailable`, `storage_unavailable`, `target_unavailable`.
Ready actions are current server advice; mutation revalidates. Local confirmation
receipts do not change session authority. Terminal/paused polling stops; active
detail polling may refresh records/results while preserving filters/cursors.

## Compatibility and checks

Known artifacts declare `fub-metadata-import-v1` alongside the existing workspace
gate in the operator-owned preflight manifest/report. Child confirmation requires
that capability for the running artifact and the complete selected API/worker
fleet; old reports remain sufficient for 010c only. No tenant capability flag.
Existing report-path refresh/expiry and synthetic test-only readiness stay intact.
New child tables are not claimable by old workers; recover child work only with
compatible artifacts, preserving all review bindings and parent records.

Focused source/DB/HTTP tests cover all accepted policies, authority, exact receipts,
lease/cancel fencing, native limits and exact byte inventory. Register opt-in real
query plans for new hot statements at D-050 scale; no CRM reader SQL change is
planned and no unrelated full-reader benchmark is added. Root coordinates final
sequential gates after the two review/fix rounds and integrated Web verification.

Committed result fields also use `GET /api/migrations/fub/metadata-imports/{id}/results/{result}/fields/{field}`. It returns the same bounded Segment for every result kind, decrypts the committed result (not a current plan), and binds cursors to Org, child, result, field and segment limit. `all`, `source.all`, `operations.all` and the section-specific hashed field keys have identical semantics to record inspection.

Preparation catalog discovery persists a local element ordinal while re-reading
its one retained source row, emitting at most 50 tag/option descriptors in a unit
(the first field unit includes the field descriptor). This is continuation within
an encrypted source collection, not offset pagination of a database table.
Mapping first-pass candidate decisions/targets remain immutable through the
finalization pass. Collision and native-capacity decisions consider that complete
candidate set, so row order cannot choose a partial winning subset. Threshold
queries stop at two collisions or the existing native cap plus one. Native table
permit predicates compare UUIDs directly so per-unit target checks remain indexed.
