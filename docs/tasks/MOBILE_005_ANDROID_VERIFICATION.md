# Mobile 005 Android verification

Implementation source: `codex/mobile-005-android` from backend base `bfac8b4`.
Runtime API source supplied by the coordinator: `382aadc`,
isolated `crm_mobile_005` on port 3103.  The native work used emulator
`CRM_Field_API37_ARM64`, Android SDK `/Users/karrad/Library/Android/sdk`, and the
Android Studio bundled JBR.  Gradle outputs and command logs are isolated under
`/private/tmp/crm-mobile005-010f3/android`.

## Implemented Android behavior

- Schema 6 adds only `detailsRevisionsQualified`, encrypted `profile_drafts`, and
  encrypted `profile_context`; schema-5 rows, key material, old drafts, envelopes,
  and receipts are not rewritten.
- A profile draft has a frozen complete names/contact baseline, local CAS revision,
  immutable outbox transition, details-specific expected revision, one unresolved
  operation per Person, and a separate follow-up draft.
- Receipts validate `person_details`, details/person revisions, changed/no-op
  semantics, and server add UUIDs against the original contact-operation index.
  Accepted proposals remain local overlays until a causally covering sealed refresh.
- Full summary contact traversal qualifies editing only when the summary exposes
  `details_revision` and every contact has UUID/kind/value/import-order/creation
  metadata. Once that traversal is complete, the display/editor projection orders contacts by
  `import_order NULLS LAST`, `created_at`, then UUID without rewriting raw page bytes; old
  unqualified caches retain their legacy readable order. Same broad revision old caches are
  refetched and upgraded. Current
  profile conflict reads are context/Person/revision fenced editor material only.
- Compose exposes names and explicit email/phone add/edit/remove operations,
  keeps new additions in insertion order in a separate server-appended section, identifies a
  removed primary per contact kind, and provides retained conflict/current/
  manual replacement controls. It never changes an outbound destination before
  acceptance.

## Completed checks

| Check | Result | Evidence |
|---|---|---|
| `:compileDemoDebugKotlin` | passed | `compile-third.log` |
| `:compileMobile005qaDebugKotlin :compileMobile005qaDebugAndroidTestKotlin` | passed | `final-compile.log` |
| Emulator `Mobile005StorageTest` (7 tests) | passed | `mobile005-storage-final.log`; schema-5 upgrade, CAS/immutable outbox, exact no-add receipt array, stale same-revision qualification, legacy over-limit preservation, incomplete-current rejection, and replacement flow |
| Actual installed Mobile004 → Mobile005 upgrade | passed | schema-5 seed and schema-6 probe below |
| Native API offline save → force stop/relaunch → exact replay | passed | `mobile005-live-relaunch-final.log` |
| Native API accepted replay, competing writer conflict, and manual replacement draft | passed | `mobile005-live-replay-conflict-final-source.log` |
| Native Compose invalid proposal retained then explicitly discarded | passed | `mobile005-ui-discard-2.log` (19.321 s); screenshot `mobile005-ui-discarded-rejected-profile.png` |
| Native Compose offline multi-field profile → force-stop/restart → exact replay/cover | passed | `mobile005-ui-prepare-unique.log` (93.519 s), staged `5794b1dc-e110-4dda-9a7e-5d7e5aba008b` SHA-256 `f0553c751e9ed128f8442d9059567d86e81062ce8cdf154279817b5983a583c9`, then `mobile005-ui-relaunch-unique-final.log`; screenshots offline, queued, restarted, and synced |
| Native Compose retained conflict/current/replacement and accepted follow-up | passed | `mobile005-ui-conflict-replacement-complete4.log` (60.768 s); conflict and replacement screenshots |
| `Mobile005StorageTest` (8 tests, including scrambled UUID contact order) | passed | direct `adb am instrument`, 40.531 s on final source |
| `Mobile004StorageTest` regression (7 tests) | passed | direct `adb am instrument`, 34.902 s on final source |
| `:lintMobile005qaDebug :compileDemoDebugKotlin :compileMobile005qaDebugAndroidTestKotlin` | passed | `mobile005-final-platform-final.log` |

The installed upgrade used the historical Mobile004 source archive at the requested
revision, built under `org.crm.field.mobile005upgradeqa` with the same
`vaultfield.mobile005upgradeqa` namespace. No uninstall, data clear, or key
replacement occurred between seed and `adb install -r` of the current app.

| Artifact | SHA-256 before | SHA-256 after |
|---|---|---|
| Wrapped database key | `15c63e97bb5becc5bc1ffddffd7b762690a63c765e1eb264ccd7d3f993d9d1d7` | same |
| Queued envelope (`…0051`) | `43d80d9b0f7dfb5c84584dde3c1dc34391c66c3c19141e64326eaaa6b9dd7a04` | same |
| Old accepted receipt (`…0052`) | `616585d782748fd611477d7e61a649a9232b3cb0c16db2426c93033b41fa4f39` | same |
| Legacy draft payload | `83e1b6fd2be7d45749e25c8d135d9a2eba3388cc98039684aad8936f7dc804ce` | same |

The post-upgrade probe also confirmed `details_qualified=false` and empty new
profile tables, requiring a complete modern traversal before editing.

## Repaired receipt replay

Before the coordinator's receipt serializer repair, the real API accepted the queued
`update_person_details` operation (HTTP 200) but omitted `added_contact_ids` for an
empty mapping. Android correctly retained that malformed receipt as queued. API 3103
was then replaced under the native lock with source `382aadc` (binary SHA-256
`da3065f283c0b295933f6e82eb446fd261ca5f9d203e03fb553d0b43a9fbc529`), which emits
`added_contact_ids: []` for every person-details receipt while retaining the legacy
receipt shapes. The already-persisted envelope replayed exactly after force-stop and
relaunch, and the subsequent competing-writer test retained the conflict for review
and created its explicit replacement against the completed current traversal.

## Native UI harness and acceptance

The initial Compose-rule package carried the coroutine exception-handler service entry
without its `ExceptionCollectorAsService` implementation, so the rule failed before any
gesture. A debug-only direct dependency on the already lockfile-pinned
`kotlinx-coroutines-test:1.9.0` now packages both provider and service entry in the QA
debug APK. The final proof drove the real editor through add-email/add-phone, encrypted
CAS autosave, explicit submit, force-stop/relaunch, exact immutable replay, server-created
revision conflict, visible completed-current comparison, and manual replacement. Test-only
screenshots are under
`/private/tmp/crm-mobile005-010f3/android/mobile005-ui-screenshots-accepted/files`.

The first staged UI phone (`555-555-0105`) was already present after normalization on the
isolated fixture, so its stored operation `0b59f227-3ab3-456a-88e1-abfcb26190e9` correctly
remained without a server receipt. The proof read that durable condition and used the visible
Discard proposal control; the successful retry selects a number absent from the sealed baseline.
Two earlier harness-only failures are retained as `mobile005-ui-relaunch-unique.log`
(API-37 startup exceeded the previous 20-second bound) and
`mobile005-ui-relaunch-unique-retry.log` (a second sync tap raced the first posted busy state).
The harness now reports restore diagnostics, uses the established 60-second native startup
bound, and waits for durable sync transitions and visible controls. Production `FLAG_SECURE`
and release dependencies are unchanged.
