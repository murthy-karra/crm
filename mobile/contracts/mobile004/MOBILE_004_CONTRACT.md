# Mobile 004 frozen backend contract

Protocol remains `mobile-v1`. Mobile 004 is additive: an old client omits the
catalog flag and receives existing reconciliation semantics. A capable client
enables stage Save only when bootstrap advertises `change_person_stage`,
`stage_revisions`, and `stage_catalog`.

`person.stage_revision` and `organization.stage_catalog_revision` are positive
signed-64-bit values serialized as canonical decimal strings. Existing rows
start at `1`. The Person trigger advances stage revision exactly once for a real
`stage_id` transition and rejects caller resets. Stage insert/delete and stage
name/position updates advance the catalog through a schema-owned SECURITY
DEFINER trigger; `crm_app` has no ordinary Organization update privilege.

Mobile locking is: workspace shared/admission, context, operation advisory
lock, Person `FOR UPDATE`, membership/operational authority, target validation,
expected revision, typed command, receipt. A matching operation identity returns
its original receipt before fresh checks. Revision is checked before no-op
detection. Rollback persists no update, fact, or receipt; changed commits publish
one StageChanged invalidation, and no-op commits publish none.

Catalog snapshots read stage rows before `organization FOR SHARE`. Stage writers
own their stage row before the all-writer trigger advances the Organization
revision, so the snapshot never takes a stage-row share lock after the
Organization lock. Repeatable-read serialization rejects a mixed snapshot.

## Operation fixture

`POST /api/mobile/v1/operations` accepts exactly this new payload:

```json
{"context_id":"11111111-1111-1111-1111-111111111111","operation_id":"22222222-2222-2222-2222-222222222222","kind":"change_person_stage","device_recorded_at":"2026-09-13T12:00:00Z","payload":{"person_id":"33333333-3333-3333-3333-333333333333","stage_id":"44444444-4444-4444-4444-444444444444","expected_stage_revision":"7"}}
```

Canonical operation-v1 bytes/HMAC domain remain unchanged; the new payload is
included only for this new kind. An accepted changed receipt is:

```json
{"operation_id":"22222222-2222-2222-2222-222222222222","outcome":"accepted","resource_type":"person_stage","resource_id":"33333333-3333-3333-3333-333333333333","committed_revision":"8","person_revision":"19","changed":true,"accepted_at":"2026-09-13T12:02:00Z","replayed":false}
```

A same-revision same-target request returns this receipt shape with
`changed:false` and nonnull unchanged stage revision. A stale revision,
including A→B→A, is `409 revision_conflict`. Foreign/missing Person is `404
not_found`; foreign/deleted target is `422 invalid_stage`; changed canonical
bytes under the same operation identity is `409 operation_payload_mismatch`.

`GET /api/mobile/v1/people/{person_id}/stage` returns only an authorized,
context-bound conflict baseline and cannot renew access or seal a bundle:

```json
{"context_id":"11111111-1111-1111-1111-111111111111","person_id":"33333333-3333-3333-3333-333333333333","person_revision":"19","stage_revision":"8","stage":{"id":"44444444-4444-4444-4444-444444444444","name":"Custom pipeline"}}
```

## Reconciliation catalog fixture

The reconciliation body adds `include_stage_catalog`, default false. With true,
the response adds `{"stage_catalog":{"revision":"12","stages_url":"/api/mobile/v1/reconciliations/55555555-5555-5555-5555-555555555555/stages"}}`.
Person summaries add `stage_revision` as a decimal string.

`GET /api/mobile/v1/reconciliations/{id}/stages?cursor=...` has an endpoint-
bound signed cursor carrying the pinned catalog revision and `(position,id)`. It
returns at most 100 rows, sorts `(position,id)`, observes the 512-KiB bound, and
returns `generation_changed` after catalog mutation:

```json
{"generation_id":"55555555-5555-5555-5555-555555555555","revision":"12","items":[{"id":"44444444-4444-4444-4444-444444444444","name":"Custom pipeline","position":3}],"next_cursor":null,"complete":true}
```

Every opted-in seal revalidates the pinned revision. Cleanup cascades the
generation's catalog rows; no catalog history is retained.
