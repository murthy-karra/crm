# Statement freeze: `b45b04f`

This directory freezes the pre-switch text of every statement Slice 012
(docs/specs/SLICE_012.md §4) will change to read the new trigger-maintained
`last_*_at` columns instead of computing them at query time: the People
matrix (`filtered_summaries` and its seven sorted copies), the saved-list
count projection (`count_filtered_matches`), the two Today-source
statements (`source_membership`, `source_candidates`), and the three
system-feed statements (`person_state`, `call_membership`, `call_only`) —
the fourteen statements named in SLICE_011e.md §4 and SLICE_012.md §4.

Named `b45b04f` (the commit that recorded D-052's "two parallel post-ladder
lanes" decision, immediately before the Slice 012/013 specs were written)
because the text is byte-identical at `b45b04f` and at this lane's own
branch point `61b08ac` — verified below, and re-verified by this directory's
own presence: nothing in Slice 012 steps 1–3 touches any of these
statements' source files. This assignment (steps 1–3) freezes the text and
proves the frozen and live paths agree; it makes **no** statement changes.
Step 4 (a later round) will switch the LIVE text one statement family at a
time and re-run the equivalence test in this directory's companion,
`tests/db_statement_equivalence.rs`, after each change — at that point the
frozen and live texts will diverge (by design) while their *results* must
stay identical.

## Why a copied Rust module, not bare `.sql` files

Mirrors `tests/fixtures/today_f51bff8`'s own reasoning: `person_state.sql`
alone binds 55 parameters, and its two person-state feeds (`unanswered_inquiry`,
`client_replied`) must bind **identically shaped** parameter sets on both
the frozen and the live side for a meaningful comparison. Freezing only the
`.sql` text and re-deriving the Rust binding call at the test site risks a
silently mismatched parameter order between the frozen and live calls,
which would make the "equivalence" comparison vacuous. Instead, each
statement's *calling* Rust code — the exact `query_file_as!`/`query_file!`/
`query!` invocation with its row struct and parameter list, copied
byte-for-byte apart from import-path and `.sql`-file-path adjustments (this
is now a `crm-api` test fixture, not `crm-app` library code) — is frozen
alongside its `.sql` text, in the `today_f51bff8` style. Frozen files run
through `query_file_as!`/`query!` keep their own `.sqlx` offline-cache
entries; since the text is byte-identical to the live statement today,
those entries are harmless duplicates of the live ones.

## Layout

- `sql/` — the twelve `.sql` files verbatim (`filtered_summaries.sql` and
  its seven sorted copies, `source_membership.sql`, `source_candidates.sql`,
  `person_state.sql`, `call_membership.sql`, `call_only.sql`).
  `count_filtered_matches` has no separate `.sql` file live either (it is
  an inline `query!` string in `person/queries.rs`), so its frozen copy is
  inline too.
- `person_sql.rs` — frozen `filtered_summaries`/`filtered_summaries_sorted`/
  `count_filtered_matches` (9 of the fourteen statements).
- `today_sql.rs` — frozen `source_membership`/`source_candidates` (2 of the
  fourteen).
- `system_feeds_sql.rs` — frozen `person_state_candidates`/`call_membership`/
  `call_only_candidates` (3 of the fourteen).

Row structs, parameter order, and the `PersonFilterParams` type itself
(`crm_app::domain::person::filter::PersonFilterParams` — unchanged by this
slice, imported live, not copied) are identical to the live callers; only
Rust import paths and `.sql` file paths were adjusted for this crate.

## Freeze provenance

| File | SHA-256 |
| --- | --- |
| `sql/filtered_summaries.sql` | `f31bbc0bf94307ce1800ac6a771f75f4a3d222df68e4bd9603079cf2517afbcd` |
| `sql/filtered_summaries_assignee_asc.sql` | `0552aedb205fd9ab4fd0aa05f46fa32f3450d379e481fb5b9b1037f4c009e904` |
| `sql/filtered_summaries_assignee_desc.sql` | `2b80cd3ed6e5c12a9754cf0a208f4c2ea5dc204745ba8863f57b70d91d0677b4` |
| `sql/filtered_summaries_created_asc.sql` | `6801b81e7bcb410b226c0e63f6abf82efed6cedc2a52621866f2c5299df255a7` |
| `sql/filtered_summaries_name_asc.sql` | `42a31bc9cf69a799d6b5d6f1e12d687c2ba568c5d04b977d37c6d666729dea44` |
| `sql/filtered_summaries_name_desc.sql` | `f263c4ffed9b5d701fcea279f24f9a80d891b4aebb6a618839db46227c934f47` |
| `sql/filtered_summaries_stage_asc.sql` | `0d383cb00987881c2fd00525c7f2df4936f1a5b9645406579f33d21bec797204` |
| `sql/filtered_summaries_stage_desc.sql` | `a0b5b1e4963a7551310245d5254d6e4465f3eae53763fbffa8373b7cf260e746` |
| `sql/source_candidates.sql` | `b38d6b77ad462d38cf06c02c9fdefc7e47edfcef10d660fd9f14831dd30c30b6` |
| `sql/source_membership.sql` | `31fae1b057e9ad96b2b576c369b4a05a20fca8dd38726366ee57677d9079917a` |
| `sql/call_membership.sql` | `a92b0cf092f16917cc91a255d1ee7e96420c5e6f2e79ba6d5e381d6540cf21dc` |
| `sql/call_only.sql` | `c203f7b990f28abb2ffad5ad16fb1dcec6b9b2901ee78495f7eae3f57c4d748e` |
| `sql/person_state.sql` | `794267ed57897de4178c149948666103b8dae78bf077ed427a4c0b6ecf95e319` |
| `crm-app/src/domain/person/queries.rs` (source of `count_filtered_matches`) | `fe95c95da69a31b1dcb7c1acc7b34d4edb9454941752eff71b312ffa223bf846` |

Each SHA-256 above is `sha256sum` of the live file at this lane's branch
point `61b08ac`, and was verified (2026-09-08, this checkout) to be
**identical** to `git show b45b04f:<path>` for every one of the fourteen
statements' source files — reproduce with:

```sh
git show b45b04f:backend/crates/crm-app/src/domain/person/sql/filtered_summaries.sql | sha256sum
git show 61b08ac:backend/crates/crm-app/src/domain/person/sql/filtered_summaries.sql | sha256sum
```

(substituting each path above), confirming nothing touched these
statements between `b45b04f` and the Slice 012 lane's own start.
