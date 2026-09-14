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
  metadata. Same broad revision old caches are refetched and upgraded. Current
  profile conflict reads are context/Person/revision fenced editor material only.
- Compose exposes names and explicit email/phone add/edit/remove operations,
  warns about server ordering after removal, and provides retained conflict/current/
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
| Native Compose editor offline/restart/conflict proof | in progress | `Mobile005UiProofTest`; test-only screenshots |

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

## Native UI harness note

The initial Compose-rule package carried the coroutine exception-handler service entry
without its `ExceptionCollectorAsService` implementation, so the rule failed before any
gesture. A debug-only direct dependency on the already lockfile-pinned
`kotlinx-coroutines-test:1.9.0` now packages both provider and service entry in the QA
debug APK. The next execution reached the actual editor and exercised add-email,
add-phone, and CAS autosave behavior. The remaining run waits for the UI's final
encrypted autosave to re-enable Submit before tapping it. Production `FLAG_SECURE` and
release dependencies are unchanged.
