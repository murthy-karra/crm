# Mobile005 — independent implementation review round2

**CHANGES REQUIRED; final correction cycle in progress.** Sol high independently
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
combined check/SQLx passes are attributed to `a52a1e6`; correction verification and
replacement final gates remain required before a completion verdict. Targeted
assessment of these fixes remains within this second review/fix cycle.

Coordinator iOS corrections for1,4 and shared6 now pass47 storage/model tests;
[MOBILE_005_IOS_VERIFICATION.md](MOBILE_005_IOS_VERIFICATION.md) owns source hashes,
new rejection/recovery cases and the retained initial fixture assertion failure.
Android corrections and targeted independent assessment remain pending.
