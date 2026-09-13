# Mobile 003 contract — backend foundation

This contract freezes the additive Mobile 003 wire behavior accepted under
D-078. It extends `mobile-v1`; it does not change an existing operation,
receipt, business clock, call settlement, or Web/Operator manual contact
command.

## Bootstrap capability

`POST /api/mobile/v1/bootstrap` adds the exact capability string
`log_contact_attempt`. Existing capability strings and all other bootstrap
fields are unchanged.

## Operation

The existing `POST /api/mobile/v1/operations` accepts this additional `kind`:

```json
{
  "context_id":"11111111-1111-4111-8111-111111111111",
  "operation_id":"22222222-2222-4222-8222-222222222222",
  "kind":"log_contact_attempt",
  "device_recorded_at":"2026-09-13T12:00:00Z",
  "payload":{
    "person_id":"33333333-3333-4333-8333-333333333333",
    "channel":"other",
    "outcome":"reached",
    "occurred_at":"2026-09-12T20:00:00Z"
  }
}
```

The outer operation is still strict (`context_id`, `operation_id`, `kind`,
`device_recorded_at`, `payload` only). The contact payload is strict and has
exactly `person_id`, `channel`, `outcome`, and `occurred_at`; it accepts no
actor, Organization, call/correction ID, Person revision, note, or free text.
`channel` is one of `call`, `text`, `email`, `other`; `outcome` is one of
`reached`, `no_answer`, `left_message`, `sent`, `busy`, `wrong_number`.

`occurred_at` is a required RFC3339 instant with an explicit offset. The
interoperable accepted range is years `0001` through `9999`, inclusive. The
server converts every accepted offset to UTC then truncates only this new
field's fractional seconds to PostgreSQL microsecond precision before
canonicalization and storage. Thus `2026-09-12T13:00:00.123456789-07:00`
canonicalizes to `2026-09-12T20:00:00.123456Z`. There is no past-age cutoff.
Existing `device_recorded_at` remains untrusted operation metadata and keeps its
pre-existing parser/canonicalization behavior; Mobile 003 does not newly
truncate it or promote it to a business clock.

The HMAC input remains the exact existing JSON object
`{"version":1,"protocol":"mobile-v1","context_id", "kind",
"device_recorded_at", "payload"}` serialized after existing per-kind
normalization, under `crm-mobile-operation-v1\0`. Existing operation kinds use
their exact previous normalization and bytes. A new operation ID is a new
manual log; only identical normalized content, context, and ID can replay.

## Execution, facts, and errors

Fresh execution retains the current workspace → context → operation advisory
lock → Organization-scoped Person lock → live membership order. It then takes
the server recording clock. A reported instant later than that clock returns
`422 {"error":"contact_time_in_future"}` and commits neither fact nor receipt.
Malformed payloads, enum values, timestamp syntax, or out-of-range years return
the existing `400 {"error":"malformed_request"}`.

The typed `log_contact_attempt_in_transaction` core accepts validated contact
data and a server-owned `CommandContext`, locks the Person through the active
Organization, and inserts exactly one append-only `contact_attempted` fact
without committing or publishing. Mobile sets `origin=mobile_session`, trusted
actor and correlation, `causation_id=NULL`, `corrects_id=NULL`, reported
`occurred_at`, and server `recorded_at`. The ordinary wrapper still chooses its
existing server occurrence/default recording clocks and retains repeated-submit
behavior. Automatic calls and call corrections keep their established clocks
and fact semantics.

## Receipt, replay, and invalidation

A new accepted contact operation returns the unchanged receipt shape with:

```json
{
  "outcome":"accepted",
  "resource_type":"contact_attempt",
  "committed_revision":null,
  "changed":true,
  "person_revision":"<current positive revision>"
}
```

`resource_id` is the fact ID. A contact fact intentionally does not fabricate a
Person revision. Fact, trigger-maintained activity values, and durable receipt
commit atomically. A receipt insert failure rolls all of them back.

Before returning an accepted replay or lookup, the server rechecks current
context, workspace, membership, and visibility. Contact receipts prove the
exact fact joins the stored Person and active Organization; a missing/erased
Person or fact returns the existing opaque `404 {"error":"not_found"}` and
never recreates the fact. Accepted replay skips new occurrence-time validation.
Changed normalized content or context returns `409
{"error":"operation_payload_mismatch"}`.

Only a fresh commit publishes the existing IDs-only
`person.changed{contact_attempted}` event after commit. Replays and rejections
publish nothing. This causes the normal client reconciliation path; it does not
add a Person revision or replicate contact history into a mobile projection.
