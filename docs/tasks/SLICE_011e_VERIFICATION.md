# Slice 011e — Verification record

Coordinator-owned evidence for the 011e rungs against
[SLICE_011e.md](../specs/SLICE_011e.md) §9, following the D-050 budget (two
review rounds at most; performance gated on paired regression and plan shape
only). Rung e2 will append its own section when it lands.

## Rung e1 — tag model, commands, routes, Person page, Tags page

Branch `slice-011e-tags` from `main` at `ad1c33b`, worktree
`../crm-worktrees/011e-e1`, one lane (Claude Sonnet 5), coordinated by Claude
Fable 5.1. Commits in order:

| Commit | Content | Lane gates (own tree) |
|---|---|---|
| `502f418` | Migration `20260909000001_tag.sql`, `TagId`, `domain/tag` module (five commands, queries), `routes/tags.rs` + two person-tag routes, `PersonChange::TagsChanged`, Operator `PersonDetail.tags`, 19 `.sqlx` entries, `db_tags.rs` (21 tests) + schema/admin/operator extensions | `check` 704 tests; `check-db` 565/565 |
| `e7530d2` | Web: types, tags query and mutations, Person page chips and Add-tag popover, preview chips, member `/manage/tags` `TagsView`, `tags_changed` token, Vitest | `check` green, 596 Vitest |
| `a563cce` | §9.10 live browser walkthrough archive | n/a (docs) |
| `8d7c748` | Round-1 fixes (below) | `check` green, 598 Vitest; `check-db` 566/566 |

Coordinator file-list audit per commit against `git status` and
`git diff --name-only`: 41 files all under `backend/`; 14 all under `web/`;
21 all under `docs/design/qa/slice-011e-e1-2026-09-07/`; 9 as reported. No
undisclosed files. `.env` never edited; nothing touched in the main checkout.

### Criterion mapping (§9.1–9.10)

| § | Proof |
|---|---|
| 9.1 Schema | `db_schema.rs`: grants exactly §2; `lower(name)` uniqueness and cross-Organization FK rejection; `tag_and_person_tag_indexes_exist` (`tag_org_lower_name_key`, `person_tag_org_tag_person_idx`) |
| 9.2 Create | `db_tags.rs`: create-or-get by case-insensitive name (first spelling kept); trim/length/control-character 400; 201st tag 409 `tag_limit_reached` and create-or-get of an existing name still succeeds at the cap; two concurrent identical creates → one row, two successes |
| 9.3 Apply/remove | idempotent `changed` true/false; 21st tag 409 while re-applying at 20 stays 200; foreign **real** Person/tag pairs across two Organizations and nonexistent ids → identical 404s, zero rows in both Organizations |
| 9.4 Rename | admin any tag; creator while unused, 403 once in use; non-creator 403; same-name unchanged, case-only changed, collision 409 `tag_name_taken`; demoted and deactivated admin 403 with no write; other Organization 404; no realtime event |
| 9.5 Delete | admin removes and counts rows; creator 200 while unused, 403 once in use; repeat 404; other Organization untouched; add racing an admin delete never 503; no realtime event |
| 9.6 Reads/precedence | `GET /api/tags` lists only the Organization's tags (second Organization's "Leaky" absent) with per-viewer `can_manage`; detail `tags` ordered; `GET /api/people` byte-identical; HTTP wire codes as a member: 400, 404-before-403 `forbidden`, 409 `tag_name_taken` / `person_tag_limit_reached` / `tag_limit_reached`, 401 without a cookie; `/api/tags` in the platform-only 401 enumeration (`db_admin.rs`) |
| 9.7 Realtime | `tags_changed` exactly once per changing add/remove, never on `changed:false`, never on rename/delete; payload never contains the tag name |
| 9.8 Operator | `get_person` returns `tags` as untrusted text; foreign Person refused; crate fences in `check` |
| 9.9 Web | chips from detail; add existing (PUT only); inline create (POST then PUT, order asserted); remove target and accessible name; inline 409 messages; 404 refetches tags **and** the Person and shows a message; preview read-only; `TagsView` controls follow `can_manage`, confirm sentence only for non-zero count, 403/404 refetch paths; Escape focus return; member reaches `/manage/tags` and sees the Tags nav entry |
| 9.10 Walkthrough | [`docs/design/qa/slice-011e-e1-2026-09-07/`](../design/qa/slice-011e-e1-2026-09-07/README.md): 11 of 11 scenarios pass in real Chrome against a scratch runtime (`:31011`/`:51011`, scratch database seeded through the HTTP API); teardown verified by the coordinator (ports free, only `crm_dev` remains, dev API healthy) |

### Review round 1 (of two) on `a563cce`

Reviewer: READY WITH FIXES, no blocking finding. Tester: no blocking
finding. Verified by both: literal Organization predicates on every tag
statement and the composite FKs; D-051 rule 1 decided under the tag
`FOR UPDATE` lock with an active-membership `FOR SHARE` re-read; no lock
cycle between add/remove (person `FOR UPDATE` → tag `FOR SHARE`) and
rename/delete (advisory → tag `FOR UPDATE` → membership `FOR SHARE`);
`tags_changed` only after commit and only on a changed row; DB failures 503,
never 404/422; ids only in spans and on the channel.

Applied in `8d7c748`:

- **defect (Web):** a tag deleted by someone else left a stale chip and no
  message on the 404. Person-tag mutations now also invalidate the Person
  query on 404 and the page shows "That tag no longer exists; refreshed."
- **implementation detail:** the non-enforcing role read that shapes
  `can_manage` on create-or-get returned `Corrupt` (503) on an unparseable
  role; it now falls back to `Member`. Unreachable through any write path
  (the column's CHECK), so no test was fabricated.
- **nit:** the popover's 40-character check used UTF-16 length; now code
  points.
- **TRUST / CONTRACT test hardenings:** `GET /api/tags` isolation with a
  second Organization; real foreign Person/tag ids in all four cross
  combinations; HTTP 400/404/403/409/401 wire bodies as a member;
  create-or-get at the 200 cap; ids-only realtime assertion; index
  enumeration; member reachability of `/manage/tags` and the nav entry.

Lane judgment calls accepted as safe defaults: a missing membership row is
`Forbidden` (fails closed; production never hard-deletes memberships); the
People-row and realtime assertions live in `db_tags.rs` rather than the
brief's `db_people.rs`/`db_realtime.rs` (equivalent coverage).

### Recorded LATER (not applied; D-050)

- `outcome` span values on the limit paths are `tag_limit_reached` /
  `person_tag_limit_reached` rather than §6's `limit` (RESTATES; ids only,
  harmless).
- Popover listbox lacks `aria-activedescendant` and option ids, so a screen
  reader does not announce the arrow-key row; keyboard behaviour is correct.
- `title` on a disabled button is not shown in Firefox (the spec prescribes
  the pattern).
- Add/remove do not re-read membership (matches the assign/stage posture;
  the membership FK keeps rows honest; a deleted membership fails closed as
  503) — BEYOND_ENVELOPE.
- A `REVOKE`-based DB-unavailable 503 route test, à la `db_saved_lists.rs`.
- A true two-connection interleave for creator-rename vs concurrent apply
  (covered structurally by the add-vs-delete race test on the same lock
  pair).

### Final-tree gates (coordinator, once, on `8d7c748`, 2026-09-07)

Run by the coordinator from the worktree root (verified by process
inspection), each once:

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | clean; the tree stayed clean afterwards (no metadata drift) |
| `./scripts/check` | all checks passed, 30 s: fmt, clippy, cargo check, crate fences, **704** Rust tests, doc tests, Web lint/typecheck/**598** Vitest/build, email-worker tests |
| `./scripts/check-db` | all checks passed, 197 s: **566 of 566** DB-backed tests on the first run; the pre-existing `db_calls` timing flake did not occur |

Not run: performance evidence (spec §8 applies to e2; e1 adds no hot
statement). Deployment, push and the shared development runtime migration
were not performed and are not authorized by this record.

### Merge readiness

Source `slice-011e-tags` at `8d7c748` plus this record; destination `main`
(at `a53220c`, four state-record commits ahead of the branch base `ad1c33b`,
all in `docs/plans/PROJECT_STATE.md`, so no code conflict is possible).
Migration impact: one additive migration, `20260909000001_tag.sql`, two new
tables, no change to existing tables; `crm_dev` must be migrated
(`./scripts/db-migrate`) and the dev API restarted by exact PID after the
merge, with the user's approval. Unresolved risks: the LATER list above; the
40-character tag-name cap is an assumption about FUB tag lengths (cheap to
raise). e2 depends on this merge.

## Rung e2 — `tags` / `not_tags` clauses across the fourteen statements

Branch `slice-011e-tag-clauses` from `main` at `c629ac0`, worktree
`../crm-worktrees/011e-e2`, one lane (Claude Sonnet 5), coordinated by Claude
Fable 5.1. No migration. Commits in order:

| Commit | Content | Lane gates (own tree) |
|---|---|---|
| `db2be0a` | Vocabulary: `Tags`/`NotTags` clause kinds, canonical-uuid pre-check, validation, `FilterError::InvalidTag`, `FilterNames.tag_names`, `describe()`, params, unit tests | — |
| `2cecee3` | Both predicates in all fourteen statements (`person_state.sql` twice), bindings at every call site, 14 `.sqlx` entries regenerated, semantics and present-clause parity tests, compile-forced explicit `InvalidTag` arms | `check` 717; `check-db` 573/573 |
| `23340d1` | `invalid_tag` at every site incl. the two former `_ =>` arms in the Today source-issue mapping; write-time and read-time tests; both name loaders | `check-db` 582/584 (two `db_calls` timing failures, pre-existing) |
| `147ef64` | Web: `FilterClause` variants, kinds and labels, FilterBar multi-select on both mounts, repair check, Today notice sentence, error unions; Vitest 610 | web gate green |
| `ad6f10a` | Performance archive `docs/design/perf/slice-011e-2026-09-08/` (UTC date) | — |
| `3d33ff7` | Round-1 fixes (below) | `check` 717 Rust / 614 Vitest; `check-db` 587/587 |

Coordinator file-list audit per checkpoint against `git diff --name-only`:
28 non-metadata files all under `backend/` after step 2; 69 files against
`main` after step 5, all under `backend/`, `web/` and the perf archive;
both predicates verified present in each of the fourteen statements.

### Criterion mapping (§9.11–9.18)

| § | Proof |
|---|---|
| 9.11 Wire | unit: round-trip (byte-stable), non-canonical uuid, empty, > 50, duplicate value, duplicate kind, unknown field → 400; db: `GET /api/people?filter=` with a foreign tag id → 422 `invalid_tag` byte-identical to a random uuid; DB failure during the probe → 503 |
| 9.12 Semantics | `db_people_filter.rs::tags_and_not_tags`: any-of one and several ids, positive and negative; none-of incl. an untagged Person; both together intersect; Organization B identically named tag never matches; `db_today_feed_equivalence.rs`: a `tags` clause on both person-state feeds (chains A and B) and on the call feed, admit and exclude |
| 9.13 Parity | absent clauses: the pre-existing parity suites unchanged and green (People, count, seven sorts, both source statements, feed equivalence); present clauses: People/count/seven sorts agree, both source statements incl. the `source_membership` built-in path, all three feed statements incl. `call_membership` |
| 9.14 Reference paths | write time: `POST`/`PUT /api/saved-lists`, `PUT /api/today/sources/{id}`, system-feed update → 422 `invalid_tag` (foreign ≡ random); read time after `DeleteTag`: detail `filter_error:"invalid_tag"` with metadata preserved, count 422, repair by saving without the clause; `GET /api/today/sources` `filter_error`; Today source issue `invalid_tag` with partial availability for a tag deleted before the snapshot; system feed `InvalidDefinition`, canonical fallback, `fallback:true`, admin `filter_error`, member effective default; `unsupported_filter` never appears. "Vanishes during evaluation" shown unreachable (spec §9.14 amendment) |
| 9.15 Today | a `tags` list source admits with the list reason and drops on tag removal |
| 9.16 describe | exact strings for one, several and the unknown placeholder; both name loaders resolve tag names |
| 9.17 Web | two chips on both mounts, URL round-trip for both kinds, unknown-id rendering and the 422 state, the decisive unresolvable-draft repair test, Today notice sentence, editors accept the chips, Lists index shows "Invalid definition" for `invalid_tag` |
| 9.18 Performance | gate 1: p95 18/1/16 ms → 18/1/16 ms with the clause absent, payloads equal; gate 2: after the §4 binding amendment, `Index Only Scan using person_tag_org_tag_person_idx` (cost 8.43, was a 567.64 sequential scan of `person_tag`); `tags` 3.3 ms, `not_tags` 27.8 ms on 25k People / 24k tag links, linear in People plus tag links (`plans/*_v2.txt`) |

### Review round 1 (of two) on `ad6f10a`

Reviewer READY WITH FIXES, tester no blocking finding. Verified: both
predicates in the spec position with correct parameter numbers and bindings
at every call site; `.sqlx` exactly the fourteen replacements, `list_summaries`
untouched; literal Organization predicate everywhere; no `_ =>` absorbs
`InvalidTag` in Rust; isolation (foreign ≡ random 422; Organization B
same-name tag); deadline discipline in `validate_references_until`; Web
contracts.

Applied in `3d33ff7`:

- **defect (Web):** `savedListCounts.ts` held a third closed code set the
  spec's enumeration missed; a count 422 `invalid_tag` rendered "Unavailable"
  with Retry on the Lists index. Now `invalid`, "Invalid definition".
- **coordinator decision (performance):** the tag subqueries bind the
  statement's literal Organization parameter instead of correlating through
  `p.organization_id`, so `person_tag_org_tag_person_idx` can serve the
  probe; the correlated form forced a full `person_tag` scan per query.
  Spec §4 amended by pointer. Re-captured plans: the planner now uses `Index Only Scan` on `person_tag_org_tag_person_idx` for both kinds (cost 567.64 → 8.43; 4.6 → 3.3 ms and 29.1 → 27.8 ms), archived as `plans/filtered_summaries_{tags,not_tags}_v2.txt` with a README addendum.
- **tests:** person_state chain B (`client_replied`) with a tags clause;
  `source_membership` exercised through a built-in Person; `call_membership`
  through a retained Person with a qualifying call, plus `not_tags` on the
  call-only path; write-time `PUT /api/today/sources/{id}`; byte-stable
  round trip; a decisive Web repair-check test; `not_tags` URL round trip.
- stale "e2 step 3" comments removed; the isolation test's snapshot comment
  corrected (first statement, not BEGIN).

### Recorded LATER (D-050)

- Reference validation issues up to 50 sequential `tag::exists` probes each
  re-arming the statement timeout; `names_for` could batch them
  (BEYOND_ENVELOPE; matters only near the value cap).
- `FilterNames` loaders use the bounded 200-row tag list rather than
  `names_for` (matches how stages and members load).
- Gate 1's `person_state` comparison ran on a fixture without inquiries
  (0 rows both sides), so it pins the guard's fold-away cost, not a
  populated chain; the populated statement is covered by the 011d archive
  and the parity tests.
- Only the 24k-row plans are archived; the 5k/12k shapes are prose.

### Final-tree gates (coordinator, once, on `3d33ff7`, 2026-09-07)

Run by the coordinator from the worktree root in one sequence, each once:

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | clean; the tree stayed clean (no metadata drift) |
| `./scripts/check` | all checks passed, 31 s: fmt, clippy, cargo check, crate fences, **717** Rust tests, doc tests, Web lint/typecheck/**614** Vitest/build, email-worker tests |
| `./scripts/check-db` | all checks passed, 191 s: **587 of 587** DB-backed tests on the first run; the pre-existing `db_calls` timing flake did not occur |

Performance evidence: §8 gate 1 passed (paired regression, payloads equal);
gate 2 passed with index use after the §4 binding amendment. Push, deployment
and the shared development runtime restart are not performed or authorized by
this record.

### Merge readiness

Source `slice-011e-tag-clauses` at `3d33ff7` plus this record; destination
`main` (at `152385b`, three docs commits ahead of the branch base `c629ac0`:
project state and the spec §4/§8/§9.14 pointers, so no code conflict is
possible). Migration impact: none (no migration in e2; the runtime needs a
binary rebuild and restart only). Unresolved risks: the LATER list above.
This completes the 011e rung pair and the Slice 011 ladder.
