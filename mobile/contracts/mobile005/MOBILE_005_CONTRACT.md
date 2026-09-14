# Mobile 005 frozen backend contract

`mobile-v1` remains the protocol.  The bootstrap capability array advertises
`update_person_details` and `details_revisions` only when this backend/schema is
present.  Existing clients ignore those extra capabilities and the additive
summary fields.

`person.details_revision` is a positive signed 64-bit revision serialized as a
canonical decimal string.  Existing and newly inserted People start at `1`.
The schema owns increments for a real name or contact identity/value/order
change; an exact no-op preserves it.  `mobile_revision` remains the broad
generation invalidation revision.

## Operation and receipt

`POST /api/mobile/v1/operations` accepts this new strict payload (all omitted
name fields are unchanged; an explicit JSON `null` clears that name):

```json
{"context_id":"11111111-1111-1111-1111-111111111111","operation_id":"22222222-2222-2222-2222-222222222222","kind":"update_person_details","device_recorded_at":"2026-09-14T12:00:00Z","payload":{"person_id":"33333333-3333-3333-3333-333333333333","expected_details_revision":"7","first_name":"Ada","contact_operations":[{"op":"add","kind":"email","value":"ada@example.test"},{"op":"edit","id":"44444444-4444-4444-4444-444444444444","value":"(555) 555-0100"},{"op":"remove","id":"55555555-5555-5555-5555-555555555555"}]}}
```

The only contact operations are `{op:"add",kind:"email"|"phone",value}`,
`{op:"edit",id,value}`, and `{op:"remove",id}`.  There are at most 50.  Names
trim, allow null, contain at most 200 Unicode scalar values and no controls;
contact display values trim, are nonempty, at most 1,024 UTF-8 bytes and contain
no controls.  The server applies the existing email/phone normalizers and
rejects duplicate `(kind, normalized_value)` values on the Person.  Empty
patches, repeated edit/remove IDs, invalid/foreign IDs, a missing expected
revision, and a resulting Person without a name or contact are invalid.

Adds receive server UUIDs in `added_contact_ids`, keyed by their zero-based
index in the original `contact_operations` array (not their position among
adds); edits preserve ID/kind/created_at/import_order. A receipt is:

```json
{"operation_id":"22222222-2222-2222-2222-222222222222","outcome":"accepted","resource_type":"person_details","resource_id":"33333333-3333-3333-3333-333333333333","committed_revision":"8","person_revision":"19","changed":true,"accepted_at":"2026-09-14T12:02:00Z","replayed":false,"added_contact_ids":[{"ordinal":0,"id":"66666666-6666-6666-6666-666666666666"}]}
```

Exact receipt replay returns the original receipt including this extension
before fresh baseline checks; modified bytes under its identity return
`operation_payload_mismatch`. A matching-revision no-op has `changed:false`, a
nonnull unchanged details revision, and an empty add mapping. A stale revision
returns `revision_conflict` before no-op detection. New-kind canonical bytes are
under the existing operation-v1 HMAC domain; old kinds and receipt JSON remain
unchanged.

## Profile read and generation fields

`GET /api/mobile/v1/people/{person_id}/details` returns a context-bound current
profile read: `context_id`, `person_id`, `person_revision`, `details_revision`,
`first_name`, `last_name`, `items`, `next_cursor`, and `complete`. Contact items
are `{id,kind,value,import_order,created_at}` ordered
`(import_order NULLS LAST,created_at,id)`. A cursor binds endpoint/context/Person
and pinned details revision; changed state returns `revision_conflict`. It is a
bounded transient conflict read and never seals or renews a download.

Summary component contact items add `import_order` and `created_at`, its summary
adds `details_revision`, and retains UUID paging for old clients. New editing
requires a complete sealed same-revision summary traversal.

## Lock/writer/realtime inventory

The new operation obtains workspace/context/operation admission and the
`intake:<organization>` advisory lock **before** the adapter's Person `FOR
UPDATE`; it then follows Person/contact validation and receipt commit. Intake
writers already take this lock before Person/contact work. Capture
`link_unmatched(add_contact_method)` currently takes Person first, so it must
never take the intake lock afterward; the revision trigger covers its insert.
The trigger also covers original import, original refresh, admission, admitted
refresh, normal intake, and deletion cascades without granting any workspace
permit.

After a changed commit only, publish the existing v1 IDs-only envelope
`{"type":"person.changed","data":{"person_id":"…","change":"details_changed"}}`.
No PII, contact ID, revision, or receipt content travels on realtime. No-op,
replay, and rollback publish nothing; post-commit publication failure preserves
the accepted command and receipt.
