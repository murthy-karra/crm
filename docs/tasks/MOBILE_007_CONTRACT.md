# Mobile007 — Search and download handoff contract

D-086 accepts implementation. Checkpoint based on `d8367a7`; planning round1 is
in progress. This file freezes compatible wire detail within the accepted
[specification](../specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md), not new product policy.

## Discovery request

`POST /api/mobile/v1/people/search`, normal session cookie and `x-mobile-context`.
No query parameters. Strict JSON `{"term":"Ada"}`; 4-KiB body maximum. Structural
JSON/unknown-field/query rejection uses existing 400 `malformed_request`; valid
JSON with blank or overlong term is 422 `invalid_input`. Trim outer Unicode
whitespace; 1–200 Unicode scalar values and <=800 UTF-8 bytes. No term in URLs,
logs, metrics, event payloads or durable search-history storage.

Current membership, operational workspace and context/actor/Org/expiry checks
precede the typed query. Existing 401 `unauthenticated`, 403 `forbidden` /
`workspace_in_migration_review`, 503 `unavailable` / dependency errors apply.
No context refresh/access-window renewal, mutation receipt or realtime event.
All responses including outer middleware denial carry `Cache-Control: no-store`.

## Response

```json
{
  "context_id": "10000000-0000-0000-0000-000000000001",
  "items": [{
    "person_id": "20000000-0000-0000-0000-000000000001",
    "display_name": "Ada Sample",
    "stage": {"id":"30000000-0000-0000-0000-000000000001","name":"New"},
    "assigned_user": null,
    "primary_email": "ada@example.test",
    "primary_phone": null
  }],
  "has_more": false
}
```

`person_id` is a UUID, not the `id` key of complete Person DTOs. `stage` follows
the existing required-stage invariant; clients may tolerate null as unavailable
preview metadata but must not invent a stage. Nullable assignment/contacts remain
explicit nulls. `display_name` uses existing name → primary email → primary phone
fallback, never UUID as identity-matching text. Results use the existing last-name,
first-name, UUID ordering and primary contact order (import_order nulls last,
created_at, ID). Match literal case-insensitive name substring or exact normalized
email/phone as the established Person query does. Fetch <=26, return <=25 and exact
has_more; show refine-search guidance without an exhaustive-count claim. Per-row
encoded JSON <=4096 bytes, response <=131072; overflow is503 `unavailable`.

Capability `people_search` is added to bootstrap's existing capabilities array;
no protocol/representation/receipt version changes. Old servers without it leave
Organization search unavailable but preserve local search/work.

## Client state and selection

Use explicit local versus Organization scopes. A submitted term creates a new
search epoch; cancel old work and reject late responses by active server/actor/Org,
context and query epoch. Clear transient term/results on signout/expiry/revocation
or context switch. Search rows never populate `Bundle`/complete cache tables.

Save offline persists the existing encrypted pin before acknowledgment. Reuse the
same pin identity, existing sync coordinator and complete metadata-capable sealed
generation. A pin revision change while downloading schedules a fresh generation;
only successful local promotion of a generation actually containing the Person
allows Available offline. Existing complete bundles remain usable with freshness
labels during replacement; unsynced work/accepted overlays remain protected.

Expose requested pins independently of search results, including after restart.
Without complete cached metadata, show a content-free requested record entry;
display only already-authorized transient/current cached name labels. Pending intent
must stay discoverable for explicit retry/cancel after a generic invalid-pin error.
Unpin removes only request intent. It never deletes drafts/outbox/receipts or removes
Today/assigned selection. No automatic full pin-set discard after one invalid ID.

Native schemas add protected pin intents/revisions and retained manifest selection
reasons as specified in Mobile007 §4a. Persist desired UUID, intent revision and
generic failure only; seed old pins without discarding caches/queues/keys. Freeze
the exact pin-set revision with staging before network admission, coalesce a dirty
newer revision into one follow-up after completion/failure/restart, and honor
existing backoff/auth/pause limits. An unchanged permanently invalid set waits for
explicit user correction. Missing legacy reasons are unknown, not inferred.

Latest sealed reasons are a closed subset of `today`, `assigned`, `pinned`. Promote
them with the active generation; cancellations use those labeled last-synced
reasons to explain why a record may remain selected. The all-pins404 identifies no
specific invalid ID: show the account's pending intent list with cached/transient
labels where available and short local IDs otherwise, then let the user cancel a
chosen request and retry. Names/contact details are never persisted from search.

## Fixtures and required checks

`mobile/contracts/mobile007/search-response.json`, `search-empty.json` and
`search-truncated.json` are synthetic contract fixtures. Server JSON and both
native parsers use the same fields. M7 acceptance and current protocol/receipt/
sealed-download behavior remain required. Backend freeze is complete only after
review findings are disposed and focused real DB/router checks pass; client work
starts from the verified integrated checkpoint.
