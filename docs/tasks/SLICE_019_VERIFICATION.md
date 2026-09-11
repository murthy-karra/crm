# Slice 019a — Verification record

Coordinator-owned evidence for [SLICE_019.md](../specs/SLICE_019.md) and
its [brief](SLICE_019_IMPL.md) under the D-050 budget. Branch
`slice-019-custom-fields` from `main` at `c19b257`, worktree
`../crm-worktrees/019`, one lane (Claude Sonnet 5), coordinated by Claude.
Implementation gate approved by the user on 2026-09-10; D-058.

| Commit | Part | Content |
|---|---|---|
| `54d1cac` | A1 | Migration `20260915000001_custom_field.sql`, spec §2 verbatim: `custom_field`, `custom_field_option`, `person_custom_field_value`; the three-column FK `(field_id, organization_id, field_type)` that makes a mistyped value unstorable and a type change impossible while values exist; the option FK; the Person cascade; archive-only grants. |
| `76f8eb1` | A2 | `crm-app/src/domain/custom_field/` (model, error, queries, seven commands), pure validators, redacting `Debug`; `PersonChange::CustomFieldChanged` and its token. |
| `8d6bca3` | A3 | `routes/custom_fields.rs` (six admin routes), two member value routes in `routes/people.rs`, `custom_fields` on the detail read, eight `ApiError` variants; numbers bound `CAST($n::text AS numeric)` and read `trim_scale(...)::text`. |
| `6a3cbb1` | A4 | Operator `PersonDetail.custom_fields` (label and value both untrusted text), the prompt parenthetical and its assertion. |
| `f1daea5`, `c77dadc` | A5 | `db_custom_fields.rs` (24 tests), `db_schema.rs` grant, index, CHECK and FK matrices, `db_operator.rs` shape and capture tests, `db_admin.rs` 401 enumeration; 26 `.sqlx` entries; fmt. |
| `dd91d3d`, `2174d73`, `99d3bb2`, `9669f96`, `9cba16b` | B | Web types, queries and the realtime token; `/manage/fields` and the nav entry; the Person page Details card; `FieldsView.vue`; 25 Vitest (two lane-caught bugs: a number save sent `{date}`, and Enter relied on a no-op `blur()`). |
| `837fc77`, `d8dd8dc`, `71651cc`, `7d17ab1` | fixes | Round-1 fixes (below). |

Coordinator file-list audits (Part A checkpoint, Part B, after the fix
round): every file inside spec §14 plus accepted mechanical
registrations: `ids.rs`, `domain/mod.rs`, `crm-api/src/lib.rs`,
`routes/mod.rs`, `crm-operator/src/lib.rs` (a re-export), the
`db_admin.rs` 401 enumeration, four one-line `custom_fields: []` fixture
edits and `events.test.ts`. No `crm-app/src/domain/person/`, filter
statement, `PersonSummary`, `tools.rs`, snapshot, `Cargo.*` or `docs/`
change. `GET /api/people` pinned byte-identical by test. The stash stack
empty throughout.

Lane measurement (spec §11, reported, not gated): the definitions list's
`person_count` statement at 25,000 People × 5 fields with every value set
(125,000 rows) runs in 33 ms as a sequential scan of the Organization's
value rows; cost is proportional to the Organization's own data.

## Lane gates (own tree)

Part A at `c77dadc`: targeted DB 126 of 126 (`db_admin`,
`db_custom_fields`, `db_operator`, `db_schema`, `db_tags`); `check` green
(821 Rust, 780 Vitest, 11 worker). Part B at `9cba16b`: `check` green
(821 Rust, 805 Vitest). After the fix round at `7d17ab1`: targeted DB
106 of 106 (backend fixes); `check` green (823 Rust, 822 Vitest, 21 s).

## Review round 1 (of two)

In two halves, the backend review overlapping the Web build. Backend at
`c77dadc`: reviewer READY WITH FIXES, tester no blocking finding. Web at
`9cba16b`: reviewer **NOT READY**, tester one blocking finding (the same
defect, reproduced). Verified by both halves: the migration is spec §2
verbatim and every `.sqlx` entry matches a source literal; the eight
routes, bodies, error codes and precedence match §4; authorization is the
admin extractor plus the command's `FOR SHARE` re-read for definitions
and any active member for values; no label or value reaches spans, logs,
envelopes, the ledger or realtime; the Web wire types match the backend
serialization exactly. Applied in one consolidated round (21 items):

- **backend (13):** value validation moved after the not-found, archived
  and type-mismatch checks so the declared 404 → 409 → 422 precedence
  holds; renaming an already-archived field or option no longer re-stamps
  `archived_at`; the 128 KiB body cap on the value route; control
  characters rejected in text values (a NUL was a 503) and U+2028/U+2029
  in labels (the database CHECK classes them as control under glibc);
  tests: cross-Organization clear and option update and the other
  Organization's empty list, a changing label sentinel and positive
  controls in the capture test, the field-level un-archive clash and
  archived-label reuse, a non-member author rejected by FK, caps counting
  live rows only, an option on a text field refused, set-to-same leaving
  `updated_at` and the author untouched, and a real-row clear on an
  archived field;
- **Web (8), one blocking:** **editor drafts were seeded once and never
  re-synced**, so with definitions cached before the Person loaded every
  editor rendered empty and a blur deleted the stored value; the
  canonical number never rendered back; realtime changes never reached
  the input. Now `{draft, server, dirty, error}` with server sync when
  not dirty, no save unless dirty, and Escape unable to re-send. Also: a
  422 `unknown_option` refetches; re-selecting a held archived option
  sends nothing; per-row pending state in a local set with
  `mutateAsync`, so two rows in flight keep their own errors; a
  half-typed date shows "Enter a complete date." instead of clearing;
  reorder, restore and option archive or restore failures surface with
  `role="alert"`; the two 50-cap copies; tests for each.

## Review round 2 (confirmation) on `7d17ab1`

Reviewer: all 21 items CONFIRMED, no test removed or renamed away, `.sqlx`
entries all additions with zero orphans, verdict READY. Two new items,
both recorded LATER: no Vitest pins the canonical number rendered back
(verified live in the walkthrough below), and an option-action error
message persists when another field's options editor opens.

## Walkthrough (coordinator, 2026-09-10, QA runtime)

Against the lane's API on `127.0.0.1:31019` and production Web build on
`51019` from the worktree, database `crm_slice019_qa` on the dev Postgres
(owned by `crm_migrator`, migrated with the worktree's `db-migrate`,
platform admin bootstrapped, seeded through the HTTP API), never
`crm_dev` or ports 3000/5173. Archive: `docs/design/qa/slice-019-2026-09-10/`.
As in Slice 018, the QA preview's websocket proxy answered 403 throughout;
realtime is not exercised live.

1. alice (admin) creates Referrer (text), Budget (number), Anniversary
   (date) and Lead temperature (choice: Cold, Warm, Hot) on Manage →
   Fields; the table shows four live fields in order with zero counts
   and the first row's Move up disabled (`019-01`).
2. carol (member) opens Grace Hopper: four editors in position order and
   no Fields link in her navigation (`019-02`). She sets all four: the
   Budget input shows the server's canonical `450000.5` for `450000.50`;
   the database holds all four with `origin = 'web_session'` and carol as
   the author (`019-03`). She clears Anniversary; the row is deleted.
3. carol's `POST /api/custom-fields` → 403 `forbidden`; her `GET` → 200
   with four fields; navigating to `/manage/fields` redirects her to
   Today (`019-04`).
4. bob (Best Realty) sees zero fields; his read of Grace and his value
   write are both 404 `not_found`.
5. alice archives Referrer: the dialog reads "1 person has a value on it.
   Values are kept and reappear if you restore it." (`019-05`); Grace's
   detail read drops Referrer while its value row survives and the count
   stays 1; the list shows it in the Archived section (`019-06`).
6. The Operator, asked "What is Grace Hopper's budget and lead
   temperature?", answers "Grace Hopper's budget is 450000.5 and her lead
   temperature is Warm." from one `get_person` call.
7. alice restores Referrer: Grace's value returns and the field is
   appended to the live order (`019-07`).

## Recorded LATER (D-050)

- Positions keep a gap after an archive (live 2, 3, 4 after archiving the
  first field; the restored one lands at 5). Ordering is unaffected and a
  reorder renumbers `1..n`; spec §3's "so the live order stays `1..n`" is
  inexact wording.
- The Web: no Vitest pins the canonical number rendered back (verified
  live above); the option-action error persists across options editors;
  date `min`/`max` and text `maxlength`; a client-side duplicate option
  check; a concurrent rename un-archiving a field another admin archived
  (full-replace default, one-tab envelope); no explicit unknown-token
  realtime test.
- The backend: the value read's `LEFT JOIN custom_field_option` could
  also bind `organization_id` and `field_id` (the composite FK already
  guarantees it); create checks options (422) before label and limit
  (409) in the two-fault corner; un-archive can raise live definitions
  above 50; the `person_custom_field.set` span omits `field_type`;
  chrono accepts lenient date spellings (`2026-2-3`) that the wire does
  not document, with canonical output.
- The pre-existing Slice 016a Vitest flake ("editing without touching the
  date re-sends the stored instant") failed once in three runs for the
  Web tester; not touched by this slice.
- Fifty 500-character values are a 25k-character worst case in the
  Operator's view (spec §6).

## Final-tree gates (coordinator, once, on `7d17ab1`, 2026-09-10)

Run by the coordinator from the worktree root under the shared gate lock,
in one sequence, each once:

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | clean, no diff |
| `./scripts/check` | all checks passed, 51 s: **823** Rust, 5 doc, **822** Vitest, 11 worker |
| `./scripts/check-db` | all checks passed, 289 s: **776 of 776** on the first run |

## Merge readiness

Source `slice-019-custom-fields` at `7d17ab1` plus this record and the QA
archive; destination `main` (docs-only commits ahead of the branch base,
so no code conflict is possible). Migration impact: one additive
migration with three new tables; `crm_dev` must be migrated with
`./scripts/db-migrate` after the merge, the dev API restarted by exact
PID (the domain module, routes and Operator view changed) and the
production web server (`dev-web-prod`) rebuilt and restarted. The
`crm_slice019_qa` database can be dropped. Unresolved risks: the LATER
list above. Push and deployment are not authorized by this record.
