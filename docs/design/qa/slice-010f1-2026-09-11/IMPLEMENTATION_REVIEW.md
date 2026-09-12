# Slice 010f1 implementation review

**READY after two bounded review/fix rounds.** D-066 approved implementation.
Formal backend round one returned CHANGES_REQUIRED; all five findings are closed.
Integrated round two found and verified three further corrections, with no
remaining actionable static finding. The final gates and measured browser/query evidence subsequently passed as
separate acceptance work. The reviewer had no production
file ownership and ran no builds, tests, services or database operations.

## Preliminary independent Web checkpoint

`backend_review` inspected the Web implementation, additive transport and tests
without editing or running commands against services. Three actionable findings:

| ID | Severity / evidence | Finding | Resolution / proof |
|---|---|---|---|
| W1 | P2 / SEEN_HERE | An initially empty or partial mapping/planned-record page could stay stale when the same plan became ready. | Progress now participates in scoped query keys for mappings, aliases and records, including latest-plan progress. Valid filters/cursors and mapping drafts survive refresh. Real main-panel readiness, mapping continuation and visible-page tests pass. |
| W2 | P2 / BOUNDARY | A result-only disposition could be sent to the planned-record endpoint after switching view, or vice versa. | Mode switches reset disposition/kind and cursor. Bidirectional request assertions pass. |
| W3 | P2 / BOUNDARY | An accepted 128-digit source ID could overflow the narrow Person provenance view. | The source-ID paragraph wraps. QA source bytes now include an exact positive 128-digit numeric ID checked by the lossless parser; the exact 128-digit ID passed the actual 390px browser check. |

Root additionally bounded long field-inspection buttons within their container,
and added readable source captions to catalog result rows. The reviewer confirmed
the button correction. Field labels remain escaped, with full segmented source
inspection. No actionable authority or exact-replay defect was found in the
inspected Web scope; that is a scoped review conclusion, not a release guarantee.

## Backend round one

**Initial verdict: CHANGES_REQUIRED; corrections verified in round two.** The independent reviewer verified all 21 source hashes and
eight checkpoint log hashes. Supplied logs substantiate 21 DB passes, 24 focused
library passes and scoped Clippy success. No extra blocker was found in the
private-write guard, tenant authority or compatibility gate. Five P2 findings:

| ID | Evidence | Finding | Required correction and proof |
|---|---|---|---|
| B1 | BOUNDARY | Replacing a building plan can lose approved choices not yet copied into materialized rows. | Preserve complete effective predecessor choices through bounded inheritance; prove an approved v2 survives immediate empty v3. |
| B2 | BOUNDARY | Rust and PostgreSQL can disagree on case-equivalent source choices, leaving a new field executable before its options become held. | Use the native equivalence rule before readiness, hold the source field and dependent values, and test the real PostgreSQL collision. |
| B3 | CONTRACT | An undeclared custom-looking People property remains retained but has no unresolved reconciliation. | Add a closed unresolved source issue and bounded evidence without inventing a field. |
| B4 | CONTRACT | Person result counters report held link/value outcomes with local planned and held_count still zero. | Derive consistent per-result counters and assert them in held-value fixtures. |
| B5 | CONTRACT | A malformed tag collection with no applicable operations is reported already_present. | Preserve the collection hold in the aggregate result while keeping independent valid operations and the existing catalog/link/value definition of held_count. |

Root verified the B2 database fact read-only in isolated PostgreSQL 18.6,
`en_US.utf8`: `lower('İ') = lower('i')` is true and both yield UTF-8 hex `69`.
Rust's `İ` lowering includes a combining dot. No customer database was involved.
The first settings query used an unavailable PostgreSQL setting name; the corrected
catalog query succeeded. The author owns the end-to-end collision regression.

The reviewer also requested focused executor-revocation-between-confirmation-and-
execution proof followed by authorized retry adoption. That regression passes,
alongside six new cases covering the R1 corrections. Development gate
evidence is in [the verification chronology](../../../tasks/SLICE_010f1_VERIFICATION.md).

## Integrated round two

**Final verdict: READY.** The independent reviewer read the complete Web
implementation, synthetic example, query collector and corrected backend. It
verified the integrated manifest `89877064…` (44/44 paths), backend manifest
`4b9140ed…` (24/24 paths) and all 15 checkpoint log hashes and byte lengths. The
three findings below are closed on that frozen source.

| ID | Severity / evidence | Finding | Verified correction |
|---|---|---|---|
| R2-W1 | P2 / CONTRACT | Storage copy describes the maximum atomic-unit bound as a total-plan bound. | Copy identifies the largest atomic unit. |
| R2-W2 | P2 / CONTRACT | Excluded People copy and a parent-specific wire name attribute child source requalification holds to the parent. | Copy and owned DTO/contract use metadata exclusions. The final source-requalification regression proves the count and absence of the old key. |
| R2-W3 | P2 / BOUNDARY | A tag-occurrence inspector's nowrap button can overflow on a 128-digit source ID; its heading does not wrap long source labels. | Button text is bounded with its accessible name intact; heading wraps. The source fixture now supplies real long-ID/long-label aliases. Actual 390px alias inspection passed after the final gates. |

## Additional development verification

The Web now distinguishes qualified source data from matching-creation eligibility.
The owned DTO exposes `create_matching_available`, derived by the server from
intrinsic source/native constraints. A valid source field whose machine key cannot
be created may still map to a compatible existing field. A real target-picker
regression proves this distinction; the 30 mapping/main-panel tests, typecheck and
scoped lint passed after the change. Capacity remains checked by the engine.

A separate read-only check of the private browser driver caught five evidence or
automation defects before execution: password-role selection, cold mapping-page
waits, premature long-ID layout measurement, accepting an unrelated pause reason,
and insufficient post-cancellation assertions. The driver now uses the actual
password label, waits for each mapping kind, checks the exact rendered 128-digit
ID, requires `storage_limit` and a lower current policy ceiling, and verifies both
released reservations plus preserved committed results. A synthetic second admin
also demotes the active reviewer through the ordinary membership command; the
walkthrough must observe the revoked reads and cleared administrator interface.
These driver checks are preparation, not evidence that browser execution passed.

## Executed verification after the review checkpoint

All final gates passed (900 Rust, 1,054 Web, 881 DB), as did 90 measured plans /
246 checks and 51 final browser checkpoints. The 18 final screenshots were
inspected. The two added acceptance files and collector's UUID/count-parser
corrections were test-only; reviewed production code remained unchanged.
Private browser tooling also corrected whitespace/DOM locators, loading-state
capture waits and awaiting a normal confirmation response. Those failed attempts
and final passes are retained in the [verification chronology](../../../tasks/SLICE_010f1_VERIFICATION.md).
No third implementation review or additional production fix was required.
