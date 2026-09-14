# Slice 010f3 — admitted metadata concrete contract

Implementation contract under D-082 and `SLICE_010f3.md`. It freezes the
admitted-child seam before its Web client is written. It does not alter the
original metadata child or authorize a source request, customer data, release,
activation, or an operational workspace mutation.

## Identity, authority and lifecycle

The engine is `fub-admitted-metadata-v1`. An admitted metadata root is bound to
one Organization, original completed import, one terminal admission run and its
confirmed admission plan. A root has one selected completed/sealed core-change
report and source snapshot. The report must belong to the same Organization,
account and original parent and must either be the report used by admission or
start after that admission capture completed. The selected snapshot has completed
`people` and `custom_fields` streams, qualified retained representations and the
same final capture sequence frozen in the root. Completion of a report alone is
not qualification.

The cohort manifest contains only successful settled items from a completed or
cancelled terminal admission. It records every successful admission result/item, global Person
identity and current same-Organization Person. Uncommitted, held and identity-
mismatched items are excluded. A confirmed cancelled predecessor has one
transactionally unique successor whose manifest contains exactly its never-settled
units; held and settled units never re-enter a plan. A completed root is terminal.

Root states are `proposed`, `queued`, `running`, `paused`, `completed`,
`cancelled`, `expired`; phases are `preparation`, `catalog`, `people`, `complete`.
Plan states are `building`, `ready`, `paused`, `failed`, `superseded`. A ready
plan expires in ten minutes. Replays resolve the actor-bound receipt before
expiry/readiness checks. Current active same-Organization admins alone can read,
prepare, replan, confirm, retry and cancel. Foreign IDs are 404, members 403,
anonymous callers 401 and review-workspace ordinary mutation attempts 409,
all with `Cache-Control: no-store`.

## Schema, provenance and accounting

Migration `20261002000001_fub_admitted_metadata.sql` adds tenant-keyed tables:
`migration_admitted_metadata_import`, `plan`, `source`, `mapping`, `manifest`,
`operation`, `result`, `receipt`, `reservation`, `issue`, and
`migration_metadata_catalog_claim`. Every child relation has composite
Organization and owner foreign keys. The root records original parent,
admission run/plan, source snapshot/report/account/capture interval,
workspace revision, executor, state/phase/lease and retained/reserved counters.
Original `migration_metadata_*` tables, their original-only FKs and one-child
lifetime remain unchanged.

`migration_metadata_catalog_claim` is the authoritative global identity after
readiness: `(organization_id, source_account_id, kind, source_key)` uniquely
maps to a live target and immutable provenance. `source_key` is the existing
32-byte versioned tenant/account/purpose HMAC; encrypted exact label/definition
evidence is checked on equality. Original identity rows remain immutable evidence
and retain their existing owner FKs. An admitted claim has admitted provenance;
it cannot impersonate an original mapping. Target disagreement, tombstone,
archive, key/type/choice disagreement or evidence inequality holds the mapping.

The selected source snapshot and its Organization ledger own new child bytes.
Existing original/admission evidence is referenced without recharging it. Count
encrypted nonce/ciphertext, source descriptors/keys, HMACs, claim provenance,
receipts, checkpoints and cursor material; fixed UUID/enums/timestamps/native
row bytes are excluded. Each executable unit is bounded at or below 64 MiB;
confirmation reserves work plus cancellation control capacity and every commit
settles native rows, claim/operation provenance, result, checkpoint and ledger
atomically. Failed handover and failed units retain neither partial claims nor
owner charges. Cancellation releases only uncommitted reservations.

## Readiness handover and locks

The first mutating prepare serializes under the existing workspace/Organization
barrier, then the metadata namespace advisory lock. It first checks current admin,
review workspace, account, source and the compatible release inventory. It fences
original metadata writers at their write boundary, drains or rejects old leased
units, enumerates original identity evidence, performs final catch-up and inserts
claims/equality checks, admits exact original-owner backfill bytes, persists
readiness and activates reader/writer/recovery capability
`fub-admitted-metadata-v1` in the same transaction. No admitted preview treats a
missing claim as free until committed readiness is read.

The lock order is workspace barrier, membership, Organization, snapshot/Org
ledgers, admitted root/plan/attempt, metadata namespace/readiness, deterministic
claim/target locks, then deterministic Person locks. All waits have the existing
two-second local timeout; leases use fresh 60-second fences. No network or model
call occurs in a transaction. Once readiness commits, original and admitted
catalog writers consult the claim inside this namespace transaction; old writer
permits fail at the write boundary even when acquired before handover. A failed
handover rolls back claims/accounting/readiness; a successful handover remains
ready after later preview cancellation and requires compatible recovery.

The admitted private permit validates review workspace, original/admission/root,
confirmed plan/attempt/executor/lease and exact manifest unit. Its allowlist is
only planned `tag`, `person_tag`, `custom_field`, `custom_field_option` and
`person_custom_field_value` inserts. It grants no Person core update, delete,
ordinary metadata update, native/mobile command or original/admission/refresh
token capability.

## DTO and routes

Base route: `/api/migrations/fub/admitted-metadata-imports`. Bodies are at most
64 KiB, reject unknown fields, mappings/pages are at most 50, displays at most
512 KiB and field segments are 4–65,536 UTF-8 bytes. UUIDs are strings and every
count, revision and source ID is a decimal string. Cursors bind Organization,
root/plan/attempt/revision, endpoint/filter/field and cannot cross a cohort.

| Method/path | Request | Response |
|---|---|---|
| `POST /` | `{request_id,admission_id,source_report_id}` | `201 {import:Detail}` |
| `GET /` | admission/cohort filter, cursor, limit | `200 {imports,next_cursor}` |
| `GET /{id}` | — | `200 {import:Detail}` |
| `POST /{id}/plans` | original mapping patch envelope (at most 50) | `202 {import:Detail}` |
| `POST /{id}/confirm` | original actor-bound confirmation envelope | `202 {import:Detail}` |
| `POST /{id}/retry`, `/cancel` | `{request_id}` | `202 {import:Detail}` |
| `GET /{id}/plans/{plan}/mappings|targets|records|issues` | bounded cursor/filters | bounded page |
| `GET /{id}/results` | bounded cursor/kind/disposition | bounded page |
| `GET /{id}/remainder` | — | exact never-settled cohort summary |
| `POST /{id}/remainder` | `{request_id}` | `202 {import:Detail}` exact immutable successor |
| admitted Person provenance | existing person provenance shape, scoped by result | bounded page/segment |

`Detail`, `Plan`, `Mapping`, `Target`, `RecordSummary`, `ResultSummary` and
`Segment` preserve the 010f1 shapes and add `admission_id`, cohort result counts,
`admission_plan_id`, `source_report_id`, `source_capture_interval`,
`cohort_counts`, `shared_claims_ready`, and `remainder` where applicable. Choices
remain exactly `hold`, `create_matching`, and `map_existing {target_id}`. Missing
or foreign targets never fall back to a different target. Normalized source text,
choices and baseline link/value state are frozen in the encrypted plan.

## Execution rules

010f1 parsing and representation bounds are reused. Definitions match exact source
ID and machine name; tags come only from embedded People arrays. The source
boundary never mixes People with definitions from another capture. Each catalog
and Person operation checks its frozen claim and destination fingerprint. An
incoming cell is inserted only when its preview baseline remains absent; an equal
preexisting cell is `already_present` without ownership adoption; different or
changed/removed baseline is held. Missing/null never clears native data. Tag cap
holds the full new tag set while independent fields may proceed. Every result has
exact `applied`, `already_present`, `held`, `not_supplied` or `source_null`
outcomes; no unprocessed remainder is approximated.

## Compatibility inventory

The coordinator registers routes, worker startup/recovery/preflight inventory,
workspace HTTP guard and `.sqlx` cache. The original metadata catalog writer is
amended to consult readiness/claims at its native commit boundary. No existing
route DTO, original reader, identity FK, child semantics or Web realtime file is
changed by this contract.

## Frozen Web transport checkpoint — staged preparation and lifecycle

This section replaces the earlier shorthand “preserve the 010f1 shapes” and the
initial scaffold's `{import_id,state}` mutation responses. It is owned concrete
implementation detail under D-082. The worker checkpoint is implemented; these
remaining transport/preparation routes are the next implementation checkpoint.
The Web writer can build against these shapes while the backend completes them.
No source operation is added.

Reuse the exact `MetadataPlan`, `MetadataCounts`, `MetadataMapping`,
`MetadataTarget`, `MetadataAlias`, `MetadataRecord`, `MetadataResult`,
`MetadataIssue`, `MetadataSummary`, `MetadataField`, `MetadataSegment`,
`MetadataChoice`, `MetadataPatch`, `MetadataConfirm`, and `MetadataPage<T>` shapes
in `web/src/api/metadataImports.ts`. A page is always `{items,next_cursor}`;
`next_cursor` is an opaque authenticated string, never a UUID pagination token.
Field segments use the same UTF-8 boundary behavior and `{text,full_utf8_bytes,
offset_bytes,next_cursor,complete}` envelope as 010f1. Ordinary paths never
return complete encrypted/raw source envelopes.

The admitted detail is `MetadataImport` plus these exact fields (its `actions`
adds `remainder`, and `engine_version` is `fub-admitted-metadata-v1`):

```typescript
interface AdmittedMetadataDetail extends MetadataImport {
  admission_id: string
  admission_plan_id: string
  source_report_id: string
  source_output_revision: string
  source_capture_interval: CoreChangeBoundary
  shared_claims_ready: boolean
  cohort_counts: {
    settled_people: string
    eligible_people: string
    excluded_people: string
    settled_metadata_people: string
    remaining_people: string
  }
  progress: {
    phase: 'fields' | 'cohort' | 'baselines' | 'seal' | 'catalog' | 'people' | 'complete'
    fields_processed: string
    people_processed: string
  }
  remainder: {
    available: boolean
    predecessor_import_id: string | null
    successor_import_id: string | null
    remaining_catalog: string
    remaining_people: string
    excluded_settled_catalog: string
    excluded_settled_people: string
    excluded_held_people: string
  }
  actions: MetadataImport['actions'] & { remainder: boolean }
}
```

`CoreChangeBoundary` is exactly the existing type from
`web/src/api/coreChangeReports.ts`; the selected report's **newer capture** is
shown, including its actual `started_at`/`completed_at`, sequence and stream
coverage. Existing `fetchImports`, `fetchPeopleAdmissions` and
`fetchCoreChangeReports` provide the labeled selectors. Filter to a completed
original import, a completed/cancelled admission with successful results, and
same-parent completed reports; the server independently qualifies the capture
and cohort. The report used by admission is allowed, as is a later capture whose
`started_at` is strictly after admission's source `completed_at`.

All detail/mutation fields not listed above retain `MetadataImport` semantics.
`latest_plan.phase` and `progress.phase` disclose preparation progress.
Preparation is active when `state='proposed'` and `latest_plan.state='building'`.
Poll only that condition or queued/running execution. A paused plan/root requires
explicit Retry; completed/cancelled/expired/paused polling stops. Counts on a
building plan describe only processed evidence and cannot authorize confirmation.
Ready plans expose a non-null digest, complete counted subset and ten-minute
expiry. `actions.confirm` requires a ready unconfirmed plan and at least one
executable operation; dirty client choices must be applied or discarded first.

| Method and suffix under `/api/migrations/fub/admitted-metadata-imports` | Exact input | Exact output |
|---|---|---|
| `GET /` | `admission_id?`, `cursor?`, `limit?` (1–50) | `{imports: AdmittedMetadataDetail[],next_cursor}` |
| `POST /` | `{request_id,admission_id,source_report_id}` | `201 {import: AdmittedMetadataDetail}` |
| `GET /{id}` | none | `AdmittedMetadataDetail` (unwrapped) |
| `POST /{id}/plans` | `MetadataReplan` plus optional `source_report_id` | `202 {import: AdmittedMetadataDetail}` |
| `POST /{id}/confirm` | exact `MetadataConfirm` | `202 {import: AdmittedMetadataDetail}` |
| `POST /{id}/retry` | `{request_id}` | `202 {import: AdmittedMetadataDetail}` |
| `POST /{id}/cancel` | `{request_id}` | `202 {import: AdmittedMetadataDetail}` |
| `GET /{id}/plans/{plan}/mappings` | `kind?`, `cursor?`, `limit?` | `MetadataPage<MetadataMapping>` |
| `GET /{id}/plans/{plan}/targets` | `kind`, `field_id?`, `cursor?`, `limit?` | `MetadataPage<MetadataTarget>` |
| `GET /{id}/plans/{plan}/mappings/{mapping}/aliases` | `cursor?`, `limit?` | `MetadataPage<MetadataAlias>` |
| `GET /{id}/plans/{plan}/records` | `disposition?`, `cursor?`, `limit?` | `MetadataPage<MetadataRecord>` |
| `GET /{id}/plans/{plan}/issues` | `cursor?`, `limit?` | `MetadataPage<MetadataIssue>` |
| `GET /{id}/results` | `kind?`, `disposition?`, `cursor?`, `limit?` | `MetadataPage<MetadataResult>` |
| `GET /{id}/remainder` | none | the exact `remainder` object above |
| `POST /{id}/remainder` | `{request_id}` | `202 {import: AdmittedMetadataDetail}` for the unique exact successor |

`MetadataReplan` and `MetadataConfirm` are literal envelopes, for example:

```json
{
  "request_id": "c4a2386b-a37b-4ba6-a9bf-4f2a5014d4a8",
  "expected_plan_revision": "1",
  "mappings": [
    {"mapping_id":"af5774b6-e231-40c7-9ddf-3b514f02560f","choice":{"kind":"create_matching"}},
    {"mapping_id":"a306cf73-a59b-4c78-a913-8b4080d05db0","choice":{"kind":"map_existing","target_id":"02dde962-cd44-416f-87d4-d844ceaa8dc8"}},
    {"mapping_id":"c87cf220-af27-4663-95cb-05bb69ab27c8","choice":{"kind":"hold"}}
  ]
}
```

```json
{
  "request_id": "75a843e0-c3f1-4f7c-87a5-8ad495409a9d",
  "plan_id": "e29fe399-d7f5-46c3-8566-fcc6fcd7033b",
  "plan_revision": "2",
  "confirmation_digest": "server-returned-opaque-digest",
  "workspace_revision": "1",
  "acknowledgments": {"held_count":"3","review_only":true,"remaining_data":true}
}
```

A replacement plan is immutable once ready. Same-boundary replacement inherits
previous explicit choices in bounded worker steps, applies at most 50 patches,
refreshes the frozen native comparison, and exposes a new revision/digest.
Changing `source_report_id` creates a fresh preparation revision, invalidates old
cursors/dependent choices, and requires a new ready-plan confirmation. Historical
plan/receipt encryption and retention remain bound to their original snapshots.
Confirmed roots reject replacement source or choices.

Each mutation persists its actor-bound receipt atomically and returns that exact
receipt on uncertain-response replay. Reuse the same request ID and body until a
definite result; do not create a new request ID after a timeout. A cancelled
confirmed root may start one successor automatically queued for exactly the
never-settled units of its already-confirmed source/plan; successful and held
terminal units are excluded. Catalog dependencies reference the original committed
results. No new source/mapping prompt or destructive undo is implied by remainder.
Cancelled **unconfirmed** preparation can be prepared again because it never
accepted a source/mapping execution boundary.

Field-segment paths mirror 010f1, scoped to this admitted route family:

- `/{id}/plans/{plan}/mappings/{mapping}/fields/{field_key}`
- `/{id}/plans/{plan}/records/{record}/fields/{field_key}`
- `/{id}/results/{result}/fields/{field_key}`

Admitted Person provenance is separate from original-import provenance:
`GET /api/people/{person}/admitted-metadata-import-provenance` returns
`MetadataPage<MetadataResult>` with the same bounded filtering/cursor conventions.
`GET /api/people/{person}/admitted-metadata-import-provenance/{result}/fields/{field_key}`
returns `MetadataSegment`. Every provenance result is checked against the exact
same-Org Person/admission/root/result before decryption, including segment replay.
These paths require coordinator workspace/no-store registration; no original
provenance route changes meaning.

The new Web panel remains `AdmittedMetadataImportPanel.vue`, with no required
props: it reads trusted actor/Organization state and uses the existing parent,
admission and report lists for selection. It can be mounted below the existing
migration family panels once these transports integrate. A separate
`PersonAdmittedMetadataProvenance.vue` accepts the same `personId` prop convention
as `PersonMetadataProvenance.vue`. Reused evidence widgets must receive explicit
admitted route adapters so a cached original field request cannot cross families.
