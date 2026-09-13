# Mobile 002 backend contract

Frozen for the approved D-076 implementation. This supplements, and does not
rewrite, the `mobile-v1` contract frozen by Mobile 001.

## Capability and representations

`bootstrap.capabilities` retains the four Mobile 001 values and adds
`edit_note`, `update_task`, and `note_revisions` together. A capable client
requires all three; an older client ignores their additive names.

Mobile reconciliation note items now add `revision`, a positive canonical
decimal string. Existing fields and page ordering are unchanged. Existing
public Web and Operator note DTOs do not expose that token. A note's baseline
is `1` after migration, including a tombstone. `mobile_note_record_revision`
increments it before every persisted note change (body, author, attribution,
provenance, timestamps or tombstone) and retains it on a byte-identical update.
The pre-existing component trigger continues to advance Person revision.

## Operations and receipts

The immutable envelope and `crm-mobile-operation-v1\0` canonical bytes/key
domain remain unchanged. The additional closed payloads are:

```json
{"person_id":"uuid","note_id":"uuid","expected_revision":"positive-decimal","body":"normalized NoteBody"}
```

for `edit_note`, and

```json
{"person_id":"uuid","task_id":"uuid","expected_revision":"positive-decimal","title":"normalized TaskTitle","kind":"follow_up","due_at":null}
```

for `update_task`. Unknown fields fail `malformed_request`. `due_at` is
required and nullable. No assignee field is accepted.

Replay is looked up by `(organization, actor, operation_id)` before a fresh
authority/version check and remains context- and digest-bound. Replay still
checks current visibility. New execution locks the Person then target row,
rechecks membership/authority, compares the expected target revision, then
does the no-op comparison. A stale equal proposal is `409 revision_conflict`;
conflict bodies never include current content. `edit_note` receipt
`committed_revision` is the resulting positive note revision, including a
successful no-op's expected revision. `update_task` carries task revision.
Historical and new `add_note` receipts remain `committed_revision:null`.
All receipt and business writes commit in one transaction.

`update_task` uses the current locked assignee without writing or validating it:
it preserves NULL and inactive historical values, and does not alter completion
state. The ordinary full-replace task wrapper keeps its required-assignee
contract. Ordinary note/task wrappers validate their original input before
opening their workspace transaction, preserving existing error precedence.

## Current-record reads

Both routes are under `/api/mobile/v1`, require the existing session and a
valid `X-Mobile-Context`, have `Cache-Control: no-store`, reject query fields,
and return generic `404 not_found` for another Person, Organization, tombstone,
or missing resource:

* `GET /people/{person_id}/notes/{note_id}` returns
  `{context_id,person_id,person_revision,note}` where note has existing mobile
  fields plus decimal `revision`.
* `GET /people/{person_id}/tasks/{task_id}` returns
  `{context_id,person_id,person_revision,task}` where task has existing mobile
  fields including decimal `revision`.

They recheck operational workspace, active membership and current
organization-wide visibility. They run a short consistent read and do not
advance a reconciliation cursor, seal a bundle, or renew the access lease.

## Locking and publication

The mobile operation transaction keeps Mobile 001's two-second lock and
five-second statement settings, operation advisory lock, workspace guard and
after-commit IDs-only Person invalidation. It acquires the target through the
typed cores under the Person then note/task lock order. Receipts are content
free and store final Person revision atomically.
