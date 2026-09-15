# Mobile005 — independent implementation review round2

**READY at `ab4a362a224183dd1d51acd51196b71315826fd6`.** All six findings and
final verification requirements are closed. Sol high independently
reviewed published base `e36c9b3` through pinned `a52a1e6`. This is round2 of D-050's
maximum two; no third broad review is authorized or planned. The reviewer inspected
source and retained evidence read-only, without rerunning native/DB/performance work.
All13 round1 findings were substantively corrected in the reviewed tree.

| ID | Priority/tag | Finding at pinned source | Correction owner |
|---|---|---|---|
| 1 | P1 CONTRACT | iOS summary qualification omits strict import-order presence/type/range and revalidation of already-qualified malformed cache rows | Coordinator/iOS |
| 2 | P2 CONTRACT/BOUNDARY | Android current-profile traversal wrongly pins broad Person revision and omits cross-page name consistency | Android |
| 3 | P2 CONTRACT | Android applies the512KiB limit to reserialized JSON rather than actual wire bytes | Android |
| 4 | P2 CONTRACT | iOS accepts unchanged profile receipts containing add mappings | Coordinator/iOS |
| 5 | P2 CONTRACT | Android summary qualification/sorting coercively accepts fractional or string import-order values | Android |
| 6 | P2 CONTRACT | Android receipt ordinals accept coercible fractional/string values; both clients must reject duplicate mapped server UUIDs | Android and Coordinator/iOS |

Pinned locations: iOS `LocalStore.swift:419,500,334`, `Protocol.swift:134`;
Android `FieldRepository.kt:730,726`, `FieldApi.kt:67`, `FieldStore.kt:378,393,623`.
These refer to immutable `a52a1e6`, not moving final line numbers. The correction
requires one strict contact representation across qualification/current/display
paths, integer receipt ordinals and unique add IDs, and actual response byte bounds.
Protected cache/drafts/operations must survive rejection; old malformed qualification
must permit same-broad-revision replacement. Broad revision changes from unrelated
writes remain valid when details revision and names are stable across current pages.

The reviewer found backend command/authorization, locking, replay/receipts,
all-writer revision, realtime, overflow/ABA and installed schema boundaries otherwise
consistent with scope. Android sealed artifacts, iOS43-test/two-journey result bundles,
and the D-050 pair/hot-query evidence were independently checked. The original
combined check/SQLx passes are attributed to `a52a1e6`; at that checkpoint,
correction verification and replacement final gates remained required before a
completion verdict. Targeted assessment stayed within this second review/fix cycle.

Coordinator iOS corrections for1,4 and shared6 now pass47 storage/model tests;
[MOBILE_005_IOS_VERIFICATION.md](MOBILE_005_IOS_VERIFICATION.md) owns source hashes,
new rejection/recovery cases and the retained initial fixture assertion failure.
Android corrections are integrated at `80b6c48`; all 42 tests pass.

## Targeted correction assessment

Sol high assessed only the six existing findings at pinned integrated source
`f70a6e4c891982b579d76cc9fa1e0843ecb9f966`. **All six are closed; no remaining
actionable defect was found in these corrections.** This is the same second
review/fix cycle, not a third broad review. iOS 47/47 and Android 42/42 evidence
source hashes match that tree. The Android sealed manifest SHA-256 is
`ab896b7f7ebc7755bc1cf9c7d89d89dfa2a752cbf2fd77ef8dd84f7fbda9fcb5`.

Reuse of prior actual native journeys and installed-store upgrades is justified:
the corrections do not change schema, keys, persisted envelopes or UI flow.
At this correction checkpoint, the reviewer withheld READY until migration
readiness and replacement combined repository gates passed.

## Final evidence acceptance

Sol high independently accepted the final evidence at `ab4a362` and returned
**READY**, with no remaining actionable findings. The
[combined final record](MOBILE_005_010f3_FINAL_VERIFICATION.md) owns passing
`check`, `sqlx-prepare` and all 1,076 database tests, migration readiness, and
preservation of all 74 protected artifacts and four shared listeners. The reviewer
accepted the historical-upgrade fixture's explicit `details_revision = 1` check;
the failed earlier gate remains retained alongside its correction and passing run.

The 47 iOS and 42 Android tests still match unchanged native source. Actual native
journey/installed-upgrade and D-050 performance evidence reuse remains valid.
This closes the existing second review/fix cycle; no third broad review ran.
