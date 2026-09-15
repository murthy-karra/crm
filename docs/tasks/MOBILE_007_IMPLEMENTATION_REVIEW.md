# Mobile007 implementation review

## Round 1 — changes requested

Reviewed the current uncommitted Mobile007 search implementation against
[Mobile007](../specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md),
[the frozen contract](MOBILE_007_CONTRACT.md), D-086, and the existing Mobile001–006
sealed-generation behavior. This was source review only; no build, database,
runtime, simulator, or emulator command was run.

The new server route uses the existing no-store mobile router, server-owned
authorization/context transaction, Organization visibility, bounded narrow
projection, and transient native search state. Those foundations are appropriate.
The following blocking gaps must be resolved before this implementation can meet
M7-05 through M7-07.

### P0 — The accepted pin-intent/reconciliation model is not implemented on either client

Mobile007 §4a requires encrypted pin intents with per-intent and account pin-set
revisions, the staging/admitted revision, a frozen desired set before generation
creation, coalesced follow-up work after active generation completion/failure or
restart, and no busy loop for an unchanged invalid set
([MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:131),
[MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:139)).

The submitted code still stores only raw UUID pins: Android uses `PinRow(person)`
and reads `dao.pins()` directly into each reconciliation request
([FieldDatabase.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldDatabase.kt:226),
[FieldRepository.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldRepository.kt:526),
[FieldRepository.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldRepository.kt:820)).
Its `requestSync` drops a pin-triggered request while a job is active
([FieldRepository.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldRepository.kt:538)).
iOS continues to store `[String]` in metadata and captures it only when no
generation is staged; the new `unpin` simply removes the UUID
([LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:852),
[FieldModel.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldModel.swift:712),
[FieldModel.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldModel.swift:719)).

As a result, a pin added or cancelled during a generation can be absent from the
sealed generation and receive no follow-up. The UI can call that older cache
available while the newer intent is silently left pending. There is no persisted
state to recover the intent after restart, distinguish an admitted set from the
current desired set, or suppress automatic retries of an unchanged invalid set.

**Required fix:** implement the §4a model before treating the UI search flow as
functional: additive encrypted pin-intent/revision schema and DAO/store
transactions, staging/admitted revision checkpoint, exact desired-set snapshot,
and an epoch-fenced coalesced scheduler that runs once after terminal sync work
when the revisions differ. Add populated Mobile006 store migrations and tests for
add/cancel during manifest/components/seal, completion/failure, and restart.

### P0 — Invalid-pin recovery and retained selection reasons are absent

The contract requires a requested-intent list independent of transient results,
generic all-pins `404` review without identifying the missing/foreign Person, and
last-sealed `today|assigned|pinned` reasons to explain a post-cancel retained
selection ([MOBILE_007_CONTRACT.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_CONTRACT.md:68),
[MOBILE_007_CONTRACT.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_CONTRACT.md:75),
[MOBILE_007_CONTRACT.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_CONTRACT.md:83)).

Neither native store persists the manifest's `reasons`: Android's `ManifestRow`
has only generation, Person, and revisions, and `appendManifest` discards the
wire field ([FieldDatabase.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldDatabase.kt:214),
[FieldStore.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldStore.kt:995)).
iOS's `members` table and manifest insert retain the same limited shape
([LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:42),
[LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:498)).
The new Android and iOS UIs only expose Cancel inside a live search-result row
([MainActivity.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/MainActivity.kt:466),
[FieldCRMApp.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldCRMApp.swift:194)).
After restart or clearing the transient response, a raw pin has no requested-entry
surface, generic failure state, retry/cancel route, or reason explanation.

The server continues correctly to return content-free `not_found` for an invalid
member of an all-pins request ([generations.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/generations.rs:199)).
The clients must not infer which UUID failed from that response.

**Required fix:** persist the generic failure class and latest sealed closed
reason set with §4a's intent model; expose account-owned requested intents even
without a transient result or cached label; mark the requested set needing review
on the generic `404`; and ensure cancel atomically removes only explicit intent,
then uses last-sealed reasons (or explicitly unknown legacy reasons) in the UI.
Add tests that cancellation preserves drafts, envelopes, receipts, and
Today/assignment selection.

### P1 — Structurally valid invalid terms return the wrong backend status

The contract requires `422 invalid_input` for blank or overlong terms, reserving
`400 malformed_request` for JSON/shape/unknown-field/query errors
([MOBILE_007_CONTRACT.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_CONTRACT.md:9)).
`search::term` currently calls `invalid()` for those valid-body violations
([search.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/search.rs:24)),
but `invalid()` is globally mapped to `400 malformed_request`
([mobile/mod.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/mod.rs:103)).
The new fixture expects 422 and will fail when executed
([mobile007.rs](/Users/karrad/projects/crm/backend/crates/crm-api/tests/fixtures/mobile007.rs:105)).

**Required fix:** return `code(422, "invalid_input")` from the typed term
validator while retaining router `JsonRejection` and query parsing as 400.

### P1 — The held-workspace fixture contradicts the frozen error contract

The contract declares held-workspace rejection as `403 workspace_in_migration_review`
([MOBILE_007_CONTRACT.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_CONTRACT.md:16)).
The mobile error adapter maps the review SQL condition to 403
([mobile/mod.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/mod.rs:60)),
but the new router fixture expects `409 Conflict`
([mobile007.rs](/Users/karrad/projects/crm/backend/crates/crm-api/tests/fixtures/mobile007.rs:198)).

**Required fix:** correct the fixture to the frozen 403 result and assert its
error code. Do not alter the shared mobile workspace error mapping for this route.

## Follow-up review scope

Re-review after the native §4a implementation and the two server/fixture error
corrections land. Passing builds or focused checks will still need separate
evidence; none is asserted by this source review.

## Round 2 — changes requested

Reviewed the repaired uncommitted implementation source against Mobile007 §4a,
the frozen contract, D-086, and the Round 1 findings. This remains a source-only
review: I did not run a build, database, backend runtime, simulator, or emulator.

The Round 1 server findings are closed. The scoped search validator now returns
`422 invalid_input` for structurally valid blank/overlong terms
([search.rs](/Users/karrad/projects/crm/backend/crates/crm-app/src/domain/mobile/search.rs:24)),
and the fixture asserts that result
([mobile007.rs](/Users/karrad/projects/crm/backend/crates/crm-api/tests/fixtures/mobile007.rs:110)).
It also now asserts the frozen `403 workspace_in_migration_review` result
([mobile007.rs](/Users/karrad/projects/crm/backend/crates/crm-api/tests/fixtures/mobile007.rs:203)).
Both native stores now add encrypted intent/revision state, stage the frozen
pin set, and promote its staged revision with the seal
([LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:106),
[LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:811),
[FieldDatabase.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldDatabase.kt:554),
[FieldStore.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldStore.kt:1182)).

### P0 — Persisted requested intents cannot be reviewed or cancelled after results disappear

The specification and contract require an account-owned requested-pin list that
survives restart and a generic all-pins `not_found`; it must use short local IDs
when an authorized transient/current label is unavailable and allow cancelling
each selected intent ([MOBILE_007_FIND_AND_SAVE_PEOPLE.md](/Users/karrad/projects/crm/docs/specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md:155),
[MOBILE_007_CONTRACT.md](/Users/karrad/projects/crm/docs/tasks/MOBILE_007_CONTRACT.md:68)).

The durable rows are present, but the native presentation models expose only a
set of UUIDs. Android's `FieldUi` has `pinnedPeople` but no requested-intent
entries or failure/reason data
([FieldRepository.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldRepository.kt:47));
its screen renders a generic retry control and renders **Cancel** only for a
currently retained search result
([MainActivity.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/MainActivity.kt:471)).
iOS likewise exposes `pinNeedsReview` only by a Person ID and puts its Cancel
and Retry controls exclusively inside `organizationSearchResults`
([FieldModel.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldModel.swift:717),
[FieldCRMApp.swift](/Users/karrad/projects/crm/ios/FieldCRM/FieldCRMApp.swift:194)).
Clearing a search or restarting therefore leaves a generic `not_found` set with
no discoverable per-intent entry and no way to cancel the selected request. The
separate known-UUID add field does not satisfy the required account-owned list.

The persisted manifest reasons are also never projected to either UI: Android
stores the raw array
([FieldStore.kt](/Users/karrad/projects/crm/android/src/main/java/org/crm/field/FieldStore.kt:1025))
and iOS stores it in `members`
([LocalStore.swift](/Users/karrad/projects/crm/ios/FieldCRM/LocalStore.swift:524)),
but neither client renders the latest sealed, labeled reason set after a
cancellation. That leaves the required retained-selection explanation absent.

**Required fix:** add a protected requested-intent projection to each native
state/UI. It must list only this account's desired UUIDs, show an existing
authorized cached/transient label or a short ID, show generic review state
without attributing the invalid ID, allow cancelling any listed intent, and
offer explicit retry for the remaining set. Project latest-sealed closed reasons
for an affected downloaded record as “last synced”; show unknown for legacy
stores. Do not persist search-result names or contact details to implement it.

### Evidence status

The supplied focused native evidence reports iOS `StorageTests` 35/35 passing
at [/tmp/mobile007-ios-storage-all.log](/tmp/mobile007-ios-storage-all.log) and
Android `Mobile007StorageTest` 3/3 passing at
[/tmp/mobile007-android-runtime3.log](/tmp/mobile007-android-runtime3.log).
Those checks cover migration, staging, promotion, and restart persistence; they
do not exercise the missing post-restart requested-intent/reason UI. Root's
separate real mobile database and interactive-journey gates remain outside this
review.

## Authored Round-2 correction closure — 2026-09-15

The requested-intent list is now an account-owned native projection on both
platforms. It survives cleared search and restart, uses an authorized current
label or short local ID, supports per-request cancellation and whole-set retry,
and presents the generic review state without inferring which UUID failed.
Both UIs project the last sealed selection reasons with unknown legacy fallback.
Android promotion retains those reasons before clearing staging.

The actual API/UI journeys pass on iOS (179.813s) and Android (36.181s), including
requested intent recovery, the 101-Person complete download and pinned reason.
iOS verifies a submitted offline note after process termination/relaunch;
Android's additional cold-process test passes (6.458s) with the retained offline
note draft. Storage and UI projection tests cover revision/failure/legacy states.
See [integrated evidence](MOBILE_007_010d3_FINAL_VERIFICATION.md),
[native evidence](MOBILE_007_NATIVE_EVIDENCE.md) and
[final iOS evidence](MOBILE_007_IOS_EVIDENCE.md).

This records the author's correction and executed evidence for Round 2; it is
not a third independent review. The separate Android installed-upgrade acceptance
gate now also passes with the actual schema-8 old binary followed by schema 9,
as recorded in the integrated evidence.
