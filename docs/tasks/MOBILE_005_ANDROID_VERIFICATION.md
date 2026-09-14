# Mobile 005 Android verification

Implementation source: `codex/mobile-005-android` from backend base `bfac8b4`.
Runtime API source supplied by the coordinator: `382aadc`,
isolated `crm_mobile_005` on port 3103.  The native work used emulator
`CRM_Field_API37_ARM64`, Android SDK `/Users/karrad/Library/Android/sdk`, and the
Android Studio bundled JBR.  Gradle outputs and command logs are isolated under
`/private/tmp/crm-mobile005-010f3/android`.

## Independent implementation review round 2 — current corrections

This is the final round-2 fix cycle after sealed checkpoint `cf68808`, not a new
broad review. The earlier acceptance and its immutable artifacts remain below.
The Android HTTP/schema contracts and schema version are unchanged.

- Current-details traversal validates every broad revision as positive signed-i64,
  permits unrelated broad-revision changes, fences details revision and both names,
  and returns the final page's broad revision. Every page and the combined contact
  set use the strict contact validator.
- `HttpResult` retains the actual response bytes and decodes JSON lazily. The page
  API checks the current-details 524288-byte budget before decoding, so whitespace
  cannot hide an oversized response. Synthetic transports now explicitly serialize
  their response bytes through `testHttpResult`; an actual loopback HTTP server
  tests valid JSON padded to 524288, 524289 and 614400 bytes.
- Contact qualification and ordering share strict UUID/kind/nonempty-string value,
  parseable timestamp and explicit-null-or-integral-signed-int32 import-order
  validation. Strings, fractions, missing fields and out-of-range orders cannot
  qualify. Persisted qualification flags are revalidated for editor availability,
  draft save/submit, download skipping, promotion and receipt coverage. An invalid
  pre-fix cache is refetched at the same broad revision while preserving saved work.
- Receipt ordinals must be raw integral JSON numbers within the original operation
  array bounds. Both the exact add-index set and unique mapped contact UUIDs are
  required before accepting a receipt; malformed results preserve the queue,
  envelope, draft and comparison.

All Android round-2 checks completed on source-tree SHA-256
`d024aecd58e325a9091a9ec44b1164f824fae2a9ab0d9881ae3a42cb005811b3`. The final source is bound to its clean local commit
by `final-checkpoint-round2.json`; `android-round2-final.json` contains exact source
file hashes, command logs, per-test outcomes and retained failed attempts.

| Check | Result | Artifact |
|---|---|---|
| JVM unit, lint, demo compile, QA app/test assembly | pass; 2 JVM tests, lint 0 errors | `round2-final-platform-ready.log`, command exit 0 / 5.312 s |
| Full storage + repository + Compose regression command | pass; 42 tests / 278.655 s, no skips | `round2-all-regressions-42.log`, command exit 0 |
| Focused malformed-cache recovery | pass; 1 test / 9.321 s | `round2-cache-recovery-focused.log` |
| Whitespace/diff check | pass | `round2-diff-check.log` |

The 42-test command comprises Mobile005StorageTest **13**, historical StorageTest
**8**, Mobile004StorageTest **7**, RepositoryBoundaryTest **11**, and
Mobile005ProfileUiRegressionTest **3**. It includes the actual padded HTTP test,
all revision/name fences, same-broad-revision cache recovery preserving the exact
pending envelope/draft, malformed metadata/receipt preservation, old-wire reads,
lease/account boundaries, upgrade storage regressions and capability/card UI.

The platform command used the established JBR/SDK and isolated Gradle output:
`:testMobile005qaDebugUnitTest :lintMobile005qaDebug :compileDemoDebugKotlin
:assembleMobile005qaDebug :assembleMobile005qaDebugAndroidTest`.
QA APKs were installed with `adb install -r`; the full emulator command used
`adb shell am instrument -w -r -e class` with those five classes and runner
`org.crm.field.mobile005qa.test/androidx.test.runner.AndroidJUnitRunner`.
The native API lock serialized all native runs. Full commands and output remain
in the named artifacts under `/private/tmp/crm-mobile005-010f3/android`.

Retained failed attempts: `round2-platform.log` caught a PersonCard/PersonRow
projection type mismatch, corrected before the passing platform command.
`round2-storage-28.log` ran all 28 cases with one new-fixture failure: its seal
omitted required `sealed_at`. `round2-repository-11.log` ran all 11 cases with one
new-fixture failure: it asserted selection state before awaiting its asynchronous
refresh. Both setups were corrected and the final full 42-test command passed.
These failures are retained, not replaced by a passing narrow subset.

The immutable round-1 native offline/restart/replay/conflict/discard/use-current
and actual installed schema-5 upgrade evidence remains applicable: this cycle
changes no schema, keys, immutable envelopes, draft lifecycle or UI flow. Strict
wire/metadata behavior and cache requalification are covered by the new focused
regressions. No existing app store was cleared, downgraded or reseeded. No Android
check remains pending; coordinator combined integration gates remain separate.

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

## Auditable acceptance (implementation review round 1)

The authoritative artifact directory is
`/private/tmp/crm-mobile005-010f3/android`. Commands retain stdout/stderr, exit
status, source commit, working-diff hash and Android source-tree hash. Direct
instrumentation retains installed QA state; no connected-test uninstall, data
clear, or downgrade was used for retained-store proofs. D-050 review round 2 is
owned by the coordinator; this corrective verification does not reset the count.

| Check | Result | Artifact |
|---|---|---|
| Unit, lint, demo compile and QA assembly | pass | `final-platform-capture-corrected.log`; 2 JVM tests; lint 0 errors, 6 version warnings |
| Full storage suites | pass, 26 tests / 32.091 s | `final-storage-26-recovered.log`: Mobile005 11, historical Storage 8, Mobile004 7 |
| Repository boundaries | pass, 8 tests / 11.495 s | `final-repository-8.log`; includes failing retry persistence, capability refresh/withdrawal and bounded current-profile traversal |
| Compose regression suite | pass, 3 tests / 7.612 s | `final-compose-regressions-recovered.log`; per-kind primary warnings, capability-disabled editor and one typed profile card |
| Actual installed Mobile004 → Mobile005 upgrade | pass, 1 test / 51.199 s | `review-upgrade-installed-before.log`, `review-upgrade-installed-complete.log` |
| Real API lost-response replay and competing writer | pass, reused unchanged behavior | `mobile005-live-relaunch-final.log` (1 / 4.663 s), `mobile005-live-replay-conflict-final-source.log` (1 / 5.701 s) |

Round-1 finding coverage:

| Review ID | Correction and verification |
|---|---|
| 1 | Download and promotion require details qualification even at unchanged broad revision. The installed schema-5 upgrade test drives a modern repository traversal at revision 7 and proves qualification becomes true. Successful bootstrap now replaces the active binding, allowing newly advertised capabilities to take effect. |
| 2 | Old UUID/kind/value contact pages remain readable but unqualified. `oldCapabilityContactPagesPromoteReadableButNeverBecomeEditableDetailsBaseline` passes. |
| 7 | Primary warnings use the first server-ordered contact per kind, including the removed row when identifying the old primary. Compose verifies both successor addresses. |
| 8 | Current-profile reads reject 101 rows, 524289 bytes and repeated cursors before recording a comparison. The repository regression passes all three cases. |
| 9 | Canonical positive signed-i64 revision parsing rejects overflow and noncanonical strings. `LeaseTest` passes. |
| 10 | Pending and selection-removal accounting includes profile drafts/contexts. Storage removal and repository pending-count tests pass. |
| 11 | Editing requires current binding capabilities; refreshed capabilities replace the active store binding. Repository withdrawal preserves pending work, and Compose verifies the disabled editor and explanatory copy. |
| 12 | Profile operations render only in their typed card. Compose asserts one Profile details card and no Complete task card. |
| 13 | Explicit discard atomically removes linked draft/context and covers the original operation. Storage verifies the immutable operation remains and protected proposal records are absent. Native discard/use-current results are recorded below. |

## Installed schema-5 upgrade

The earlier `org.crm.field.mobile005upgradeqa` hash claim had no recoverable runner
output and is **not counted**. That app and store remain preserved. The replacement
proof uses the fresh, isolated identity `org.crm.field.mobile005reviewupgradeqa`
and namespace `vaultfield.mobile005reviewupgradeqa`.

`git archive fcc05b3 android mobile/contracts` supplied actual historical Mobile004
code in `review-upgrade-source`. The historical database, store and vault files were
byte-compared with that Git revision (`review-upgrade-source-integrity.json` and
`review-upgrade-source-manifest.json`). Only QA identity/build paths and the seed
fixture were added. The reproducible seed is
`android/qa/mobile005-upgrade/Mobile005InstalledSeedTest.kt`; it refuses to seed an
existing database. It created schema 5, an encrypted cache, queued envelope, old
accepted receipt, generic draft and contact draft, and captured their inventory.
Current code was then installed in place with `adb install -r`.

`Mobile005InstalledUpgradeTest` waits for the production application restore before
opening the database. It compares every protected value before synthetic
reauthorization, confirms schema 6 with unqualified details and empty new profile
tables, then drives the repository's full modern download at unchanged broad
revision **7**. The resulting inventory reports `details_qualified=true` and one
summary page fetched. The queue and receipt remain byte-identical after that sync.
The deterministic old fixture lease is explicitly reauthorized only after the
preservation comparison; it cannot be treated as a valid real-clock boot lease.

The before/after logs retain all inventory hashes. Selected identical hashes:

| Protected artifact | SHA-256 before and after |
|---|---|
| Unwrapped key hash | `87d186fe916f2f5d341f244f02208ac1020c75789c1b1307bb725866dad90113` |
| Wrapped key bytes | `5279ffddfed4e4765f59ed44debfd45c06cdb5b45bb3aa2892bc072d45b6cb46` |
| Exact queued envelope | `135a2cf3affa8ec2c86ca1188d335648e9527522e4690790546b91869398c2d2` |
| Old accepted receipt | `8d510e06620b7af9624b805e7f7bb7608214ab8cd6b6f13ab4fcf89f96e62604` |
| Generic draft | `af5e6b97fe27598b60dd4b1936a994371d8ce438ca6f11426f9d742a1b7c84d2` |
| Contact draft | `41a3e1cc46c0fbcbca6aa166850b5129046b071f49127d6ab8e5dd522c4e42e6` |
| Cached summary | `8e5a4e5698cc5e8f7bce24af2496dd3345cbde2e0bd4786a6b20c59ac3c6a5dc` |
| Cached contacts | `9dad0cc488e5db3d563ff782b5d629f3d4ea1081feb86a5372b19ad6c5448b80` |

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

The UI proofs use real Compose gestures for profile edits, submissions and conflict
choices against the isolated API. A second synthetic actor creates the competing
write through the shared typed API. Test screenshots temporarily clear
`FLAG_SECURE` in the test activity only; production flags and release dependencies
remain unchanged. The debug-only, already-pinned coroutine-test dependency supplies
the Compose exception service implementation.

All four native phases passed with full runner output. The initial combined run is
`final-ui-prepare-retained-history.log` (49.644 s, followed by actual force-stop)
and `final-ui-relaunch.log` (43.843 s). It recorded operation
`b2a8f722-da5e-4a84-b83e-b0ef4f65021d`, SHA-256
`ce6f32786e4f0244adce3266da34d90532219c4330a770a7e6d31a844964c77d`, unchanged
before/after process restart and replay, with a covered receipt. The same relaunch
run completed full current-profile conflict review and accepted manual replacement.

`final-ui-discard.log` passed (38.151 s), explicitly discarding rejected operation
`40f2bd24-ee1e-4270-90d6-6d41f3a9df81`. `final-ui-use-current.log` passed
(46.308 s), explicitly choosing current for conflicting operation
`30558a7d-a6ff-49c6-9e1a-78ddc4bc7ef2`. Both assert the original operation is
covered, the linked profile draft and comparison are absent, and the draft card is
absent from the Compose view. Existing unrelated superseded comparison history
remains preserved.

The first offline-dialog screenshot was black because its secure flag was inherited
before the QA helper cleared the parent window. That image is not counted as visual
evidence. The helper now clears the flag within the test before opening a dialog,
with no production flag change; the recaptured combined journey also asserts both
name fields and exactly email/phone additions in the stored envelope. Its final
capture passes are `final-ui-prepare-capture-corrected.log` and
`final-ui-relaunch-capture-corrected.log`. They retain operation
`0a154b74-6f1a-481e-a8a6-9e9f0f1cb08f`, unchanged envelope SHA-256
`845a33ccc5e6de23757063a5cfc44768ba115b335194f1d5028fe38cd5ea843f`, an actual
force-stop, covered receipt and completed manual replacement. The eight captured
images are in `final-native-screenshots-capture-corrected`; the offline editor,
queued/restarted state, conflict, replacement and discard/current images were
visually inspected. Unrelated retained comparison cards visible in the latter
images are not the operation explicitly discarded by that phase.

`android-acceptance-final.json` seals passing command artifacts, their source/diff
hashes, source-file hashes, screenshots and the inventory of retained logs. Round-1
Android source-tree SHA-256 is
`9e2b940b798edab1b5822c5e5662e4e458668042a9b8d95ae576f9782fb98735`.
The storage/repository/Compose/installed-upgrade test bodies and production code
were unchanged by subsequent UI harness/capture corrections, so their complete
passing evidence is reused. `final-checkpoint.json` binds the clean local commit
to the sealed manifest. At that round-1 checkpoint no Android acceptance check remained pending;
round-2 corrections and their current evidence are recorded above.

## Retained failures and recovery

No missing, empty, failed or interrupted artifact is counted as a pass:

- `mobile005-ui-relaunch-unique-final.log` failed at line 122. Its early replay
  assertions and the later standalone conflict pass did not prove the combined
  journey. `mobile005-ui-discard-2.log` is empty. Historical claimed Mobile005 8/8
  and Mobile004 7/7 runs without stdout were unverified.
- The original `final-round1-regressions.log` stopped during the retry-checkpoint
  repository test. The unchanged focused reproduction passed, as did a complete
  seven-test reproduction (`retry-checkpoint-diagnose.log`,
  `repository-boundaries-final.log`). The current eight-test full suite above
  includes that case and the new capability regression.
- `final-storage-26.log` reached test 12 after the 11 Mobile005 cases, then timed
  out; it is incomplete. `final-compose-regressions-3.log` was interrupted during
  the same guest degradation. The current complete 26/8/3 results above supersede
  those attempts without hiding them.
- Guest service/window/SQLCipher stalls were captured in the runtime diagnostics.
  An ordinary emulator stop/relaunch with its original AVD, userdata and flags
  restored progress (`emulator-cold-start-recovery.log`). No app data or Keystore
  was wiped. Upgrade-app backups succeeded; the normal QA tar capture timed out
  and is explicitly retained as a partial backup, not a verified backup.
- `review-upgrade-installed-after.log` exposed a test-inspector race with the
  application's schema migration. Awaiting application readiness corrected the
  harness. The retry showed the deterministic old lease correctly locked against
  the emulator clock; explicit fixture reauthorization after preservation checks
  corrected that setup. The final installed-upgrade pass used the same retained
  store, without reseeding or downgrading it.
- `final-ui-prepare.log` and `ui-fixture-diagnostic-prepare.log` failed a fixture
  assumption: retained superseded comparison history was mistaken for unresolved
  work. The harness now checks actual unresolved operation states and selects
  controls by the current proposal's immutable ID. The old history remains intact.

## Historical broad-storage disposition

The earlier `mobile005-storage-regressions-final.log` ran 15 tests and failed three
`StorageTest` cases. They were not treated as a passing narrower subset: missing old
notes/tasks item revisions were already made readable; this follow-up fixed null target keys
so independent immutable `add_note` actions no longer block each other; and rebuilt the
test's real schema-1 input before exercising migrations through schema 6. The first follow-up
run then exposed a stale test setup that began a second generation without staging its required
pages; it is repaired and the retained retry artifact reports `OK (8 tests)`.
