# Mobile 007 planning review

**ROUND 1 — REVISION REQUIRED before writer launch.**

Reviewed against source revision `d8367a7` on 2026-09-15. This review covers
Mobile 007's mobile discovery query, context/capability handling, the boundary
between a transient hit and a sealed bundle, and pin/download/cancel/restart/
upgrade recovery. It did not run services, databases, builds, or tests, and it
does not change an accepted contract.

The proposed read-only search is compatible with the existing mobile context and
no-store router. The draft needs the three decisions below frozen in the contract
and native briefs before implementation. They are necessary to meet M7-04 through
M7-07 without weakening the Mobile001 sealed-generation boundary.

## Blocking findings

### R1-01 — A pin written during an active generation is lost as a scheduling event

**Why this blocks:** M7-05 requires a pin added during sync to reach a later
generation. The current clients persist the pin but do not record that the active
generation used an older pin set or arrange a follow-up run. Android returns from
`requestSync` while `syncJob` is active, then can successfully promote that old
generation ([FieldRepository.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldRepository.kt:522),
[FieldRepository.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldRepository.kt:532),
[FieldRepository.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldRepository.kt:802)).
iOS likewise returns when `syncing` is true; `pin` only starts a new `Task` and
the request captures `store.pins()` only when `generation == nil`
([FieldModel.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldModel.swift:694),
[FieldModel.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldModel.swift:699),
[FieldModel.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldModel.swift:821)).
The server correctly snapshots the submitted pin set into the generation
([generations.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/generations.rs:239),
[generations.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/generations.rs:278)); it cannot repair a client that treats an older seal as satisfying a newer request.

**Required revision:** Freeze one durable, account/epoch-fenced native
reconciliation intent rule: atomically mark the pin-set revision dirty on every
pin add/cancel; record the pin-set revision admitted into staging; and, after
either terminal completion or a recoverable failure, start exactly one later
generation when they differ. The old generation may still seal and update existing
data, but must not mark a newer pin **Available offline**. Cover add/cancel while
manifest/component/seal work is in flight and after restart. This is native
implementation and storage work; it does not require a new server mutation.

### R1-02 — Pin intent and selection reasons are insufficient for explicit, safe recovery

**Why this blocks:** Mobile007 requires a user to cancel the invalid requested pin
without removing drafts/outbox/receipts and to explain when the Person remains
selected for Today or assignment. The current Android `pins` table contains only
the UUID and `ManifestRow` drops the server's `reasons`
([FieldDatabase.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldDatabase.kt:214),
[FieldDatabase.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldDatabase.kt:226),
[FieldStore.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldStore.kt:995)).
iOS persists only `[String]` under the `pins` metadata key and its `members`
table has only generation, Person and revision
([LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:42),
[LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:498),
[LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:852)).
The backend does return manifest reasons, but invalidates any submitted pin that
is absent from the Organization-scoped selection with a content-free `404
not_found` ([generations.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/generations.rs:175),
[generations.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/generations.rs:199),
[mobile/mod.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/mod.rs:103)).
That avoids foreign-Person disclosure, but the existing stores cannot retain an
individual request's durable lifecycle or explain any remaining server selection
reason after cancellation.

**Required revision:** Freeze a minimal encrypted local pin-intent model for both
clients: Person ID, local intent revision/state, the generic recoverable failure
class, and the latest sealed selection reasons. It must not persist a search
response as a cache or search history. The model needs a transactional cancel that
changes only explicit intent, a schema migration for Android Room and an
additive/iOS store migration, plus populated-Mobile006 upgrade evidence. Define
the generic wording and multi-pin behavior: a failed all-pins admission identifies
no foreign/deleted Person, while the app offers cancellation only of user-owned
pin intents and retries the remaining set. The UI may use transient result labels
while they exist; after restart it must remain honest without retaining result
body data.

### R1-03 — The malformed-body status rule needs an explicit compatible split

**Why this blocks:** The draft currently groups malformed input under 422, while
the current mobile adapter maps every `JsonRejection` (including invalid JSON and
unknown fields on a `deny_unknown_fields` request) to `400 malformed_request`
([mobile.rs](/Users/karrad/projects/crm/backend/crates/crm-api/src/routes/mobile.rs:122),
[generations.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/generations.rs:5)).
Changing that global interpretation for a new route would silently diverge from
the established mobile error precedence.

**Required revision:** Freeze the route-specific contract as: invalid JSON,
wrong JSON shape, and unknown fields are `400 malformed_request`; a structurally
valid `{ "term": ... }` whose trimmed value is empty or exceeds the scalar/byte
bounds is `422 invalid_input`. Keep existing context/authority precedence and
`no-store` for every path. The router already applies `no-store` to the entire
mobile router ([mobile.rs](/Users/karrad/projects/crm/backend/crates/crm-api/src/routes/mobile.rs:21),
[mobile.rs](/Users/karrad/projects/crm/backend/crates/crm-api/src/routes/mobile.rs:69)).

## Verified non-blocking foundations

- The existing search query already has the intended literal escaped-name and
  exact normalized-contact semantics, deterministic ordering, and `limit + 1`
  behavior ([queries.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/person/queries.rs:859),
  [queries.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/person/queries.rs:881)).
  Mobile007 should use a narrow projection because this one also computes inquiry
  aggregates ([queries.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/person/queries.rs:893)).
- Existing reconciliation already binds a generation to the authenticated
  context/installation and seals only a complete selected snapshot
  ([generations.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/generations.rs:247),
  [generations.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/generations.rs:617)).
  Search can use that same context boundary, but a hit must remain transient until
  this mechanism seals and the local transaction promotes it.
- Metadata-capable reconciliation is already selected by both native clients
  ([FieldRepository.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldRepository.kt:823),
  [FieldModel.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldModel.swift:826)).
  The new state model must preserve that representation rather than add a
  search-specific cache path.

## Required contract/brief additions before implementation

1. Add the R1-01 pin-set revision and one-follow-up-generation algorithm to both
   native briefs and M7-05 fixtures.
2. Add the R1-02 encrypted pin-intent/reason fields, upgrade migration inventory,
   generic invalid-admission recovery UI, and no-search-history boundary to the
   native contract and M7-06/M7-07 fixtures.
3. Add the R1-03 `400` versus `422` fixture matrix to `MOBILE_007_CONTRACT.md`
   before clients consume the route.
4. Keep the planned narrow query and D-050 paired query/Person/Today evidence.
   The review found no reason to change the current matching semantics or to add
   a server-side pin table.

After those revisions are accepted, the Mobile007 search route and native lanes
can proceed within the existing D-050 envelope.

## ROUND 2 — READY for the declared implementation checkpoint

Reviewed the Round 1 corrections against the same `d8367a7` native and backend
baseline. The corrections close R1-01 through R1-03 without changing the accepted
product boundary or adding a server-side pin mutation.

- **R1-01 closed.** Mobile007 §4a now requires an encrypted account pin-set
  revision, a staging/admitted revision, snapshotting before admission, and one
  coalesced latest-set follow-up after completion, recoverable failure, or restart
  ([MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:131),
  [MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:139)).
  It explicitly preserves the existing pause, authorization, backoff, attempt
  limits, and old sealed cache while preventing an old seal from fulfilling a
  newer intent. That addresses the current Android active-`syncJob` and iOS
  active-`syncing` gaps identified in Round 1.
- **R1-02 closed.** The new contract makes pin intent and latest sealed manifest
  reasons encrypted, durable native state, with additive populated-Mobile006
  upgrades, unknown legacy reasons, generic all-pins `404` handling, and an
  explicit cancel path scoped only to this account's own intents
  ([MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:149),
  [MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:155),
  [MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:162)).
  This fits the current content-free all-pins `not_found` response and avoids
  retaining a search-result cache or disclosing which requested UUID is foreign
  or deleted.
- **R1-03 closed.** The specification and frozen handoff contract now retain the
  existing `400 malformed_request` treatment for JSON/shape/unknown-field errors
  and reserve `422 invalid_input` for a structurally valid invalid term
  ([MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:83),
  [MOBILE_007_CONTRACT.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_CONTRACT.md:9)).
  This is consistent with the current router's `JsonRejection` mapping
  ([mobile.rs](/Users/karrad/projects/crm/backend/crates/crm-api/src/routes/mobile.rs:122)).

The native briefs now explicitly assign the encrypted intent/revision/reason
schema, follow-up coalescing, and populated-store upgrade evidence
([MOBILE_007_IMPL.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_IMPL.md:65)).
The remaining work is implementation and required verification, not a planning
blocker. No code, service, database, build, or runtime check was run during this
Round 2 review.
