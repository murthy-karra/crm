# Slice 013 — Verification record

Coordinator-owned evidence for [SLICE_013.md](../specs/SLICE_013.md) §8 under
the D-050 budget (two review rounds at most).

Branch `slice-013-operator-filter` from `main` at `61b08ac`, worktree
`../crm-worktrees/013`, one lane (Claude Sonnet 5), coordinated by Claude
Fable 5.1, run in parallel with the Slice 012 lane and rebased onto `main`
after Slice 012 merged (a clean rebase; the three new `tests/all.rs` registrations from both slices sat in non-adjacent positions as planned). Pre-rebase hashes are given for the lane commits; after the rebase the branch head is `3d68a7c`. Commits in order:

| Commit | Content | Lane gates (own tree) |
|---|---|---|
| `ab1657f` | Contracts: `PeopleFilterSpec`, `SavedListSelector`, two `ToolBackend` methods, `FilterOutcome`/`FilterResult`/`SavedListRef`, both tool schemas and parsing (`limit` default 10), regenerated snapshot, `kind_label` mirror test in crm-api, fake backends | `check` 734 Rust / 614 Vitest |
| `b0511e3` | Pure name resolver `crm-api/src/operator/filter.rs` (exact then case-insensitive; collisions fail closed; active members; tokens win; echoes clipped) | — |
| `2a8279a` | `SqlxToolBackend` adapter (resolve → validate → describe → params → `filtered_summaries[_sorted]`; saved lists via the request's `AuthContext` threaded through the Operator route; `ListInvalid`; counts and `limit`) | — |
| `b815e04` | Dispatch, `ledger_name` arms, tool-aware not-found, `MAX_REFERENCES` 25 with the reference-cap fixture at 26 ids, span fields declared and recorded, prompt rules and the "eight tools" pin | — |
| `163b876` | `db_operator_filter.rs`, thirteen database tests (§8.4–8.13) | — |
| `7da1b21` | check-db caught a real bug: duplicate-named saved lists resolved to the first exact match instead of a clarification; fixed with a regression test, plus two fixture fixes | `sqlx-prepare` no new entries; `check` 751 / 614; `check-db` 600/600 |
| `e20bd5f` → `fe3321b` after rebase | Round-1 fixes (below) | `check` 756 / 614; `check-db` 601/601 |
| `fcea707` → `3d68a7c` after rebase | The round's own gate run caught two "byte-identical" assertions comparing `duration_ms`; compared by shape instead | same run |

Coordinator file-list audit: sixteen files, all under `crm-operator/**`,
`crm-api/src/operator/**`, the granted `routes/operator.rs` one-line change
and `tests/all.rs` registration, the new test file, plus two test files
outside the boundary (`db_today_source_operator.rs`,
`db_today_source_settings.rs`) that received inert placeholder
`AuthContext` fixtures because the backend constructor signature changed;
accepted. No edits to `person/filter.rs`, `person/queries.rs`,
`saved_list/**`, migrations, `.sqlx` or `web/`.

### Criterion mapping (§8.1–8.16)

| § | Proof |
|---|---|
| 8.1 | snapshot updated deliberately; schemas forbid additional properties; `list_id` the only id, commented beside `person_id`/`contact_method_id` |
| 8.2 | parser caps (> 50, wrong types, `created.never`, days bounds, empty spec) → `invalid_arguments`; `limit` clamped, absent → 10 |
| 8.3 | every `Clause::kind_label()` has a field (walks the live enum from crm-api); fake backends' `seen` cover both methods |
| 8.4 | composed filter with stage, tag and member names → cards, description, count; ledger `person_ids` = card ids and `tool_name` never `unknown` |
| 8.5 | unknown tag → `needs_clarification` with wrapped `available_tags`; two in a row do not end the turn |
| 8.6 | ambiguous member → candidates; `me` differs per actor |
| 8.7 | `last_contact not_within_days 30` includes the never-contacted |
| 8.8 | 30 matches with limit 10 → 10 cards, count 30; 501 → count 500, `more_than_500` |
| 8.9 | D-046 matrix: own personal, shared for member and admin, another member's personal → not_found incl. admin, duplicate names → candidates → `list_id`, `invalid_tag` list → `list_invalid`, stored sort honoured |
| 8.10 | foreign `list_id` and foreign names byte-identical to nonexistent; no foreign name in any prompt |
| 8.11 | full scripted turn 200 with `references.people`, no `operator_proposal` row |
| 8.12 | crate fences in `check` |
| 8.13 | span capture contains `filter_kinds` and no names, ids or day counts; reference-cap fixture ≥ 25 ids |
| 8.14 | prompt-rule test pins the rules and "eight tools" |
| 8.15 | Web untouched; Vitest unchanged and green |
| 8.16 | final-tree gates below |

### Review round 1 (of two) on `7da1b21`

Reviewer READY WITH FIXES, tester no blocking finding (isolation verified on every read path; the thirteen database tests pass under the migrator URL as `check-db` runs them). Verified: no trusted id
reaches the model except `list_id`, re-validated through the D-046 predicate
by name, by id and through candidates; `AuthContext` used only for the two
saved-list reads; no crm-app type in crm-operator; resolver rules incl. the
exact-match collision fix; adapter order without `validate_references`;
`Ok` clarifications reset the malformed counter; span fields declared;
description, list names and tag names wrapped as untrusted text; the only
file shared with Slice 012 is `tests/all.rs` with non-adjacent hunks.
Applied in `e20bd5f`/`fcea707` (rebased `fe3321b`/`3d68a7c`):

- the bridging default bodies for the two trait methods, reported removed
  in step 3 but still present on the tree (coordinator-confirmed), deleted
  so a backend that forgets a method fails to compile;
- the stale "placeholder dispatch" comment removed;
- resolver deduplicates resolved ids order-preservingly before `validate()`,
  so aliases such as `["Lead", "lead"]` are one value rather than a strike
  (coordinator decision revised on the tester's finding); empty items after
  clipping are dropped and an effectively empty spec is the one strike;
- a fail-closed context-mismatch guard at the top of `run_saved_list`
  (the constructor carries two sources of identity; unreachable in
  production);
- tests: same-name vocabulary in two Organizations with People on both
  sides; an admin passing another member's personal `list_id` directly;
  a service unit proving a clarification resets the malformed counter
  between two strikes; the span capture asserting exact field values and
  the absence of list, Person and tag uuids; two active members sharing a
  display name → `ambiguous_assignees`.

### Recorded LATER (D-050)

- `maxLength` is advertised only on `stage_names` items though the parser
  clips all four name arrays (snapshot re-pin when next touched).
- The `sources` pattern is enforced by `validate()`, not the parser
  (bounded, never echoed).
- The not-found detail says "by that name" on the `list_id` path too.
- Members sharing a display name cannot be selected by name (spec LATER).
- `sources` are not clipped and a `validate()` failure reports a generic
  "the filter is invalid" without naming the field.
- Exactly 500 matches (`count 500`, `more_than_500 false`) is untested; 501
  is (RESTATES 011b).
- `list_saved_lists` caps at 250 rows, exactly D-046's 200 + 50; raising
  either limit would make some lists unresolvable by name.
- Case-only list-name collisions are covered at string level, not through
  the adapter.
- Resolver uses ASCII case folding where Postgres uses `lower()`; a
  non-ASCII case pair fails in the safe direction (clarification).
- `members` and `assigned_user_display_name` are user-authored but
  unwrapped, following the `PersonCard` precedent.
- Turn-deadline cancellation inside `filtered_summaries` follows the
  `search_people` pattern, not Today's owned-connection path (pre-existing,
  BEYOND_ENVELOPE).

### Final-tree gates (coordinator, once, on `3d68a7c` after the rebase onto `26ddab7`, 2026-09-08)

Run by the coordinator from the worktree root under the shared gate lock, in
one sequence, each once:

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | clean; no new metadata (no new SQL statement in this slice) |
| `./scripts/check` | all checks passed, 73 s: fmt, clippy, cargo check, crate fences, **757** Rust tests, doc tests, Web lint/typecheck/**614** Vitest/build, email-worker tests |
| `./scripts/check-db` | all checks passed, 197 s: **617 of 617** DB-backed tests on the first run (Slice 012's tests included after the rebase); the pre-existing `db_calls` timing flake did not occur |

Push and deployment are not performed or authorized by this record. No
migration; the shared development runtime needs a binary rebuild and restart
by exact PID only.

### Merge readiness

Source `slice-013-operator-filter` at `3d68a7c` plus this record; destination
`main` at `686dc26` (the branch is rebased onto the Slice 012 merge, so the
merge is a fast-forward-able no-conflict merge). Migration impact: none.
Contract changes as declared in spec §7: two new tools and the regenerated
snapshot, the `ToolBackend` trait methods, `MAX_REFERENCES` 25 (with the
accepted `get_today` drawer side effect), the prompt asset. Unresolved
risks: the LATER list above.
