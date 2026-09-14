# Mobile005 — Independent implementation review round1

**CHANGES REQUIRED.** Sol high, read-only independent review. Reviewed published
base `e36c9b30bb7de6fcd73d20f8e32a7cd6fd42be18` through backend/iOS `a19fc88b`
and Android `9134a447`. Coordinator correction commits were excluded. No files
were edited and no DB-backed checks were run by the reviewer. This consumes
implementation review round1 of D-050's maximum two; planning reviews are separate.

## Actionable findings

| ID | Priority/tag | Finding at reviewed source | Correction owner |
|---|---|---|---|
| 1 | P1 BOUNDARY | Android download skip omits details qualification, so unchanged-broad-revision schema5 upgrades skip required pages and fail promotion | Android |
| 2 | P1 CONTRACT | Android summary staging rejects old-server UUID/kind/value-only contact items instead of preserving read-only compatibility | Android |
| 3 | P2 BOUNDARY | Backend current-details final SELECT cannot see a writer committed after the repeatable-read snapshot | Coordinator |
| 4 | P2 BOUNDARY | Direct Person UPDATE can manufacture details revision+1 without an identity change | Coordinator |
| 5 | P2 BOUNDARY | Same-transaction native additions tie on now(), producing random UUID display order | Coordinator |
| 6 | P2 BOUNDARY | iOS uses UUID page order for display/editor and detects only the overall first contact for removal | Coordinator |
| 7 | P2 BOUNDARY | Android primary-removal warning excludes the removed row before checking primary, making the warning unreachable | Android |
| 8 | P2 BOUNDARY | Both native current-profile traversals omit per-page100-row/512-KiB bounds and repeated-cursor checks | Coordinator/iOS and Android |
| 9 | P2 CONTRACT | Android accepts positive revision strings above signed-i64 maximum | Android |
| 10 | P2 BOUNDARY | Android pending/removal accounting omits protected profile drafts and contexts | Android |
| 11 | P2 CONTRACT | Android Edit UI uses cached qualification without current binding capabilities, then mislabels rejection as storage trouble | Android |
| 12 | P2 BOUNDARY | Android renders profile operations again in the generic list as Complete task | Android |
| 13 | P2 BOUNDARY | Android explicit discard/use-current covers the operation but leaves a linked inaccessible profile draft | Android |

Primary reviewed locations: `FieldRepository.kt:772,702,835,419`,
`FieldStore.kt:787,363,890,960,697`, `MainActivity.kt:1325,584,1307,863,890`,
`Protocol.kt:24`, `LeaseTest.kt:20`, iOS `LocalStore.swift:417`,
`FieldCRMApp.swift:295,427`, `FieldModel.swift:431,621`, Rust `mobile/mod.rs:480,557`,
`update_person_details.rs:154,283`, and the first Mobile005 migration:10.
These are locations in the immutable reviewed trees, not moving final line numbers.

## Acceptance and evidence assessment

Backend focused tests and iOS native/installed-upgrade evidence were independently
checked and match their records. The native API executable hash matches the
recorded immutable `382aadc` artifact. Realtime fixture/invalidation/publication
evidence passes. The remaining Mobile005 acceptance map is not ready while the
findings and these Android evidence gaps remain:

- `mobile005-ui-relaunch-unique-final.log` ends in a failed test. Earlier
  replay/cover assertions and a separate later passing conflict test are partial
  evidence, not a passing combined journey.
- `mobile005-ui-discard-2.log` is empty.
- No retained result artifact proves the claimed final Mobile0058/8 or
  Mobile0047/7 storage runs. The available mixed storage log has3 failures of15.
- Installed schema5→6 before/after hashes appear only in prose; the available
  build log proves assembly, not the installed probe.
- D-050 paired performance/hot-query plans and sequential combined
  `scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db` remain pending.

Evidence lives under `/private/tmp/crm-mobile005-010f3`. Corrections and actual
subsequent checks belong in the backend/iOS/Android verification records. Round2
must assess the complete corrected source and required evidence; this record
does not pre-approve those corrections or claim release readiness.
