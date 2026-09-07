# Slice 011b-sort — Per-list sorting for saved People lists

**Status: REVIEWED (READY-WITH-FIXES applied 2026-09-06); §2 decided by the
user (D-048); awaiting the implementation gate.**
Prepared 2026-09-06 against `main` at `31c9980` (after 011c). Branch for
implementation: `slice-011b-sort`. Size S. Authority: the user's 2026-08-29
decision that sorting stays out of 011b and becomes its own rung, restricted to
non-derived columns, applied before the 500-Person cap while preserving static
SQL verification ([ladder](../plans/SLICE_011_LADDER.md), [state](../plans/PROJECT_STATE.md));
[D-043](../decisions/DECISION_LOG.md) (FUB-shaped lists); [D-046](../decisions/DECISION_LOG.md)
(privacy, caps); [011a](SLICE_011a.md) (vocabulary, fixed-matrix SQL, 500 cap, URL
contract); [011b](SLICE_011b.md) (definition, revision, save flows); [011c](SLICE_011c.md)
§4 (Today never uses list order); [UI_STYLE](../design/UI_STYLE.md).

An agent chooses which column orders a People list and in which direction.
The server orders before the 500-row cap, so on a list with more than 500
matches the sort decides which 500 are shown. A saved list remembers its sort
as part of its definition; Save, Save as and Duplicate carry it; Today ignores
it. Without a sort, everything is byte-identical to today.

## 1. Scope

In: a typed `PersonSort` with four keys and two directions; seven static
sorted People statements beside the untouched default; `GET /api/people?sort=`;
`sort` on saved-list create, update and detail; two nullable columns on
`saved_list`; clickable column headers on the People and list table plus a
compact "Added" column; URL-synced `?sort=` on `/people`; sort as part of the
dirty draft; a cap message naming the sort; tests; a proportionate performance
measurement; amendment pointers.

Out: sorting on derived columns (inquiry count, last inquiry, last contact,
primary contact, display name), which are SELECT-list subselects or LATERAL
probes and would be evaluated for every candidate row before the top-N; they
become eligible only after the queued denormalized last-activity chunk.
Per-viewer sort preferences, multi-column sort, new indexes, sorting in the
Lists index, Today, Operator and mobile changes.

## 2. Decisions (taken by the user on 2026-09-06, recorded as D-048)

1. **The sort is part of the list definition**, persisted with the list,
   bumping its revision, editable by whoever may edit the list (creator for
   personal, current admin for shared), and copied by Save as and Duplicate.
   Everyone opening a shared list sees the same first 500. A member who wants
   a different order duplicates the list, exactly as for criteria. The
   alternative, a per-viewer preference, needs a second table and mutation
   path and makes "which 500" viewer-dependent. **Decided: definition.**
2. **The control is the column header**, plus a new compact "Added" column
   (created time) so the default order has a visible header. Alternative: a
   separate "Sort by" menu and no new column. **Accepted default: headers.**

## 3. Domain: `PersonSort`

New `crm-app/src/domain/person/sort.rs`, a sibling of the filter, never inside
`FilterDefinition` (that vocabulary is shared with Today and the 011d feeds,
is `deny_unknown_fields`, and its fingerprint semantics must not move).

```text
SortKey       = created | name | stage | assignee
SortDirection = asc | desc
PersonSort { key, direction }; DEFAULT = created.desc
token form    = "<key>.<direction>", lowercase only ("name.asc")
normalized()  = None for DEFAULT, else Some(sort)
```

Serde through `TryFrom<String>` / `Into<String>`: an unknown or malformed token
is a decode error, so commands cannot hold an invalid state.

| Key | Sorts on | asc | desc | Then |
|---|---|---|---|---|
| `created` | `person.created_at` | oldest first | newest first (today's default) | `person.id ASC` |
| `name` | `person.last_name`, then `person.first_name` | A→Z, missing last | Z→A, missing last | `created_at DESC, id ASC` |
| `stage` | `stage.position` (org pipeline order) | pipeline order | reverse | `created_at DESC, id ASC` |
| `assignee` | `app_user.display_name` (unassigned = missing) | A→Z, missing last | Z→A, missing last | `created_at DESC, id ASC` |

Missing values sort last in both directions (`NULLS LAST` written explicitly
because PostgreSQL's DESC default is NULLS FIRST). Within equal keys the
existing default order applies, so a sorted list is a stable refinement of the
order agents already know, and `id` is the final tie-break everywhere.

Text order is the database's default collation, as the Operator's
`search_summaries` already relies on. The intended behaviour is locale-aware
ordering: the development container is the stock PostgreSQL image, which
initialises `en_US.utf8`, whereas CloudNativePG's `initdb` defaults to `C`,
under which "van Dyke" sorts after "Zed". **Deployment requirement (recorded
here, owned by the production-deployment slice):** initialise the production
cluster with the same locale collation as development. No `lower()` or
`COLLATE` clause in v1. An empty-string last name, should any write path store
one, orders as text (first), unlike a missing name; a DB pin records which
holds.

## 4. Queries: one static statement per non-default order

`created.desc` is never a new statement: with a filter it is `filtered_summaries`,
without one it is `list_summaries`, both byte-identical to today including
their `.sqlx` entries. Seven new files under `crm-app/src/domain/person/sql/`,
each a textual copy of the filtered matrix differing only in its `ORDER BY`
line, bound through the existing `PersonFilterParams` and executed with
`query_file_as!`; `LIMIT 501` and the truncate-to-500 rule are unchanged:

```sql
-- created.asc
ORDER BY p.created_at ASC, p.id ASC
-- name.asc / name.desc
ORDER BY p.last_name ASC  NULLS LAST, p.first_name ASC  NULLS LAST, p.created_at DESC, p.id ASC
ORDER BY p.last_name DESC NULLS LAST, p.first_name DESC NULLS LAST, p.created_at DESC, p.id ASC
-- stage.asc / stage.desc
ORDER BY s.position ASC,  p.created_at DESC, p.id ASC
ORDER BY s.position DESC, p.created_at DESC, p.id ASC
-- assignee.asc / assignee.desc
ORDER BY u.display_name ASC  NULLS LAST, p.created_at DESC, p.id ASC
ORDER BY u.display_name DESC NULLS LAST, p.created_at DESC, p.id ASC
```

`filtered_summaries_sorted(conn, scope, params, sort)` dispatches on the
sort. A sort without a filter runs the sorted statement with all-NULL clause
parameters, which 011a pins as equal to the full list. The canonical matrix
text moves into `filtered_summaries.sql` and is loaded the same way; its
`.sqlx` hash depends only on the SQL text and must remain the same hash.

A service-free unit test reads the seven files and asserts that, with **the
top-level `ORDER BY` line immediately preceding `LIMIT 501`** removed (the
matrix contains other `ORDER BY` lines inside its subselects and LATERAL
probes, which must match exactly), each is identical to the canonical text
after per-line trimming. One further difference is tolerated, and only if the
performance gate below requires it: the sorted copies may guard the
`latest_src` LATERAL with `ON ($5::text[] IS NOT NULL)` exactly as
`source_candidates.sql` does (011c §8 change 1), so an absent Source clause
skips that probe. Moving the canonical text into a file may change its cache
entry's hash (the hash is of the exact bytes); that single replaced entry is
acceptable, while `list_summaries`, the count statement and every Today entry
must remain untouched. Saved-list statements that name columns (insert,
update, tombstone update, retry lookup, visible-row reads) legitimately
regenerate because they now carry the two sort columns. Measurable as
`git status --porcelain backend/.sqlx` showing only added files plus those
replacements, with the untouched entries' hashes listed in the record. This is the
static-SQL discipline of 011a: no dynamic SQL, every statement in the offline
cache, no shared-text drift.

Why not a bound `CASE` in one statement: under the generic plan PostgreSQL
adopts after five executions, the keys cannot fold and the default order loses
its `created_at` index early-stop for every request; 011c's Phase B just had
to pin exactly that class of flip with a transaction-local setting. Why not a
query builder: dynamic SQL is outside the accepted discipline and reserved for
custom fields.

### Performance gate

`created.asc` walks the existing index and stops early. The other six pass the
whole Organization slice through the WHERE and top-N sort 501 rows. Two costs
must be observed, not assumed: the SELECT-list subselects (expected to be
postponed until after the sort and limit) and the FROM-side LATERAL probes,
which run once per Organization row before the top-N under any plan. The
three `max()` probes are removable when unreferenced; `latest_src` (an
`ORDER BY … LIMIT 1` probe joined `ON true`) is not, which is why 011c guarded
it. Before merge, extend `./scripts/perf bench` with the six unindexed sorts,
unfiltered and with one positive clause, against the harness Organization
seeded to **100k People in the same run** (the hazard is linear in
Organization size; re-seed if absent); capture plans with `PREPARE`/`EXECUTE`
executed at least six times so the generic plan is the one observed, and
record inner-node loop counts from `EXPLAIN (ANALYZE)` on those later runs.
Acceptance: SELECT-list subselect loops at most 501 per request; no LATERAL
node with loops of the order of the Organization size unless that node is
removable and shown removed; and the unfiltered sorted p95 at or below the
same run's `4-clause combo` p95 from `scripts/perf bench`. If the `latest_src`
probe dominates, apply the pre-declared guard above to the sorted copies and
re-measure; if a generic plan still fails, the second lever is `SET LOCAL
plan_cache_mode = force_custom_plan` around the sorted statements only,
measured again. No new index in this rung; the only helpful candidate,
`person (organization_id, last_name, first_name, id)`, serves `name.asc`
alone and is recorded for later.

## 5. Persistence and commands

One additive migration, next timestamp after `20260906000002`:

```sql
ALTER TABLE saved_list
    ADD COLUMN sort_key TEXT,
    ADD COLUMN sort_direction TEXT,
    ADD CONSTRAINT saved_list_sort_key_check
        CHECK (sort_key IS NULL OR sort_key IN ('created','name','stage','assignee')),
    ADD CONSTRAINT saved_list_sort_direction_check
        CHECK (sort_direction IS NULL OR sort_direction IN ('asc','desc')),
    ADD CONSTRAINT saved_list_sort_pair_check
        CHECK ((sort_key IS NULL) = (sort_direction IS NULL));
```

A NULL pair means the default order. No backfill, no new grant, no drop on
rollback. Deliberately no tombstone constraint: the older binary's delete
updates only name, filter, revision and `deleted_at`, so a constraint tying
sort to `deleted_at` would make every rollback-era delete of a sorted list
fail; a lingering sort token on a tombstone leaks nothing and is never read.
The older binary's column-listed INSERT leaves the pair NULL.

`CreateSavedList` and `UpdateSavedList` gain `sort: Option<PersonSort>`; the
command normalizes the default to `None` before fingerprinting and storage.
The create fingerprint stays `[scope, name, filter]` when the sort is `None`
and becomes `[scope, name, filter, sort_token]` otherwise, so every existing
digest and in-flight retry is unchanged. The equal-definition no-op compares
name, filter and sort. Delete clears both columns with the tombstone. The
011c source cleanup is untouched.

Reads: detail returns `sort`; the index metadata does not change; the count
ignores sort. A stored pair the binary does not recognise (possible only under
migration or binary skew, since the CHECK constraints block other values) fails
closed exactly like an unknown clause: detail returns `filter: null`,
`sort: null`, `filter_error: "unsupported_filter"` (the one shape the Web
already handles); the count returns 422 `unsupported_filter`; no People fetch
and no Save as; Delete still allowed. Today keeps evaluating such a list,
because source evaluation reads the filter only. Falling back to the default
order would silently change which 500 rows the author chose. When the stored
filter itself is unreadable, detail reports `sort: null` alongside
`filter: null`, so `filter_error` and the two nullable fields never diverge.

## 6. HTTP contract

`GET /api/people?filter=…&sort=name.asc` → the unchanged
`{"people":[…],"truncated":bool}`; only order and the surviving 500 change.
Valid tokens are exactly the eight lowercase dotted forms; `sort=created.desc`
is byte-identical to absent. `sort` is decoded by the typed query extractor
(`Option<PersonSort>` through `TryFrom<String>`), so a malformed token is 400
before any pool acquisition; that also holds for a repeated `sort` parameter
and for `?sort=` present but empty, as 011a already pins for `filter`. Error
precedence: 401, then 400 `malformed_request` (the relative order of a filter
and a sort 400 is unobservable and not pinned), then 422 `invalid_stage` /
`invalid_assignee`, then 200 or 503; the existing 503-before-empty-filter
behaviour of the handler is unchanged. Unknown extra parameters are still
ignored. The request span records the static sort token beside
`filter_kinds`, never a value; a sort without a filter records no
`filter_kinds`.

Saved lists (bodies remain `deny_unknown_fields`; `sort` optional, `null` or
absent meaning default):

```json
POST /api/saved-lists
{"request_id":"…","scope":"personal","name":"Stale Zillow","filter":{…},"sort":"name.asc"}
PUT /api/saved-lists/{id}
{"expected_revision":3,"name":"Stale Zillow","filter":{…},"sort":null}
GET /api/saved-lists/{id}
{"list":{…unchanged metadata…},"filter":{…}|null,"sort":"name.asc"|null,"description":[…],"filter_error":null|code}
```

An invalid `sort` in a body is 400 `malformed_request` from the decoder, after
401 and before the 403/404 resource checks, keeping 011b's precedence. Create and update responses and the
index are unchanged; the Web rebuilds its baseline from what it submitted, as
it does for the filter.

## 7. Authorization, isolation and Today

Unchanged policy: the sort rides the same requests and rights as the filter.
`me` resolution and `PersonVisibilityScope` do not change; a sort can never
widen the Organization slice. New pins: an Organization-B Person with the
alphabetically first name never appears in Organization A's `name.asc` list;
another Organization's list sort is invisible through detail. Today source
evaluation takes `PersonFilterParams` only and keeps its own key in
`source_candidates.sql`; a sorted list and its unsorted duplicate enabled as
sources must produce identical Today bodies.

## 8. Failure behaviour

Database failure is 503 on every path and never an invalid-definition label.
A malformed `sort` in query or body is 400, never coerced. A stored sort the
binary cannot read fails closed per §5. Web: an invalid `?sort=` from the URL
is dropped and the parameter cleared on mount, mirroring the invalid-filter
rule; a URL-origin 400 or 422 clears **both** `filter` and `sort` (the server
does not say which failed); a user-origin edit or any 5xx keeps state with
retry; a 409 on Save keeps the sort in the draft as the filter is kept. The
uncertain-write reconciliation of 011b §5 compares name, filter and sort when
deciding that the intended state is already saved.

## 9. Web

- `DataTable.vue` gains an optional `sort` prop and an `update:sort` emit. A
  column is sortable only when its definition carries `meta.sortKey`; other
  consumers are unaffected. Sortable headers render a button of at least 40px
  with an accessible name such as "Sort by Name, ascending", `aria-sort` on
  the header cell, and a 16px monochrome arrow on the active column. No
  client-side sorting model is introduced; the server orders. The first click
  uses the column's natural direction (Name, Stage and Assignee ascending;
  Added descending), later clicks toggle; there is no unsorted state because
  the list is always sorted. Inquiries and Last inquiry headers stay plain.
- A compact "Added" column shows `created_at` as relative time with an
  absolute title, like Last inquiry.
- `SavedListBaseline` gains `sort`; dirty = name, filter or normalized sort
  differs; Reset restores it; Save submits the working sort; Save as uses the
  working sort; Duplicate uses the baseline sort. The Save as / Duplicate
  dialog gains one summary line, for example "Sorted by Name (A–Z)".
- `/people` gains URL-synced `?sort=` through the same replace path as
  `filter`; default means absent; named-list routes ignore `?sort=` as they
  ignore `?filter=`.
- Query keys extend the factory only: without a sort the keys are byte-identical
  to today; with one, `['org', org, 'people', filter ?? '', sortToken]`, still
  covered by every prefix invalidation.
- Cap message: "Showing the first 500 by Name (A–Z) — more exist." with the
  labels Added newest/oldest first, Name A–Z/Z–A, Stage pipeline order/reverse,
  Assignee A–Z/Z–A. Counts near the filters and in the Lists index are unchanged.

## 10. Declared contract changes (AGENTS.md §11)

| Current → proposed | Why / components | Compatibility and pointer owner |
|---|---|---|
| `GET /api/people` accepts `filter` only → also `sort` | Ordered results before the cap; crm-api, Web | Additive; absent or `created.desc` byte-identical. Pointers in SLICE_002 §5 and SLICE_011a §4e/§5a/§6. |
| Saved-list create/update/detail carry `filter` → also `sort` | Sort is part of the definition; crm-app, crm-api, Web | Additive optional field; older clients unaffected; fingerprint unchanged without sort. Pointers in SLICE_011b §3/§4/§5/§6. |
| One filtered summary statement → seven sorted static copies plus the canonical text as a file | Static-SQL discipline with variable order; crm-app, `.sqlx` | `list_summaries`, the count statement and the Today entries unchanged; at most the canonical entry replaced; new entries added. Pointer in SLICE_011a §4e. |
| `saved_list` has no sort → two nullable columns with CHECK constraints | Persist the definition's order; PostgreSQL, crm-app | Additive migration, no backfill, no new grant, no drop on rollback; the older binary's INSERT and delete keep working (no tombstone constraint). Pointer in SLICE_011b §3. |
| `filter_error: unsupported_filter` means an unreadable clause → also an unreadable stored sort | One fail-closed disposition; crm-api, Web | Same wire shape and Web handling. Pointer in SLICE_011b §5. |
| Request span records `filter_kinds` → also the static sort token | Observability; crm-api | Additive, no values. Pointer in SLICE_011a §7. |
| People workspace has no sort state → `?sort=` and header controls; new Added column | Visible, shareable order; Web | UI_STYLE §1's "no fake sorting" becomes real sorting on these columns only; amendment note in UI_STYLE §1 and the D-045 concept. SLICE_011a §6 and SLICE_011b §6 pointers. |

Not changed: Today, Operator tools, counts, realtime, `FilterDefinition`. A
one-line pointer in SLICE_011c §4 records that list sort is explicitly not an
ordering input.

## 11. Acceptance criteria and required tests

1. **Parsing.** All eight tokens round-trip; `NAME.asc`, `name`, `name.`,
   `name.up`, `inquiry_count.asc`, empty and whitespace are rejected;
   `created.desc` normalizes to none. Unit tests.
2. **Statement parity.** Each sorted file minus its top-level `ORDER BY` line
   equals the canonical text after per-line trimming, with the `latest_src`
   guard as the only tolerated difference if adopted; `git status --porcelain
   backend/.sqlx` shows only added files plus the canonical entry and the
   saved-list statements that now name the sort columns.
   Unit test plus the verification record listing the hashes.
3. **Default-order parity.** `?filter=X` with and without `sort=created.desc`,
   and `/people` with and without it, return byte-identical bodies; existing
   People order and cap tests pass untouched. DB tests.
4. **Every key and direction.** Order, missing-last in both directions,
   tie-break chains, and the empty-string versus missing name pin. DB tests.
5. **Sort before truncation.** With more than 500 matches seeded so the
   newest 500 and the alphabetical 500 differ, `name.asc` returns the
   alphabetical set with `truncated: true`. DB test.
6. **Composition.** Sort with a positive clause, with `me`, and with empty
   clauses matches `filtered_summaries` membership. DB tests.
7. **Error contract.** 401 before sort parsing; 400 for each malformed form,
   for an empty `sort` and for a repeated `sort`, before pool acquisition
   (service-free route tests); a foreign-Organization stage with a bogus sort
   is 400, not 422; unknown extra parameters still 200; a sort without a
   filter records no `filter_kinds`. HTTP tests.
8. **Tenant isolation** per §7. DB tests.
9. **Saved lists.** Create and update with sort; revision bump on a sort-only
   change; default stored as NULL and re-saving it is a no-op; same retry token
   with a different sort conflicts; Duplicate copies sort; delete clears both
   columns; detail returns sort; count ignores it; the same retry token with
   `sort` absent versus `"created.desc"` replays 200 `created: false`; a
   member's sort-only PUT on a shared list is still 403; hidden personal is
   still 404; CHECK constraints reject bad values and half pairs; the
   fail-closed decode is exercised with a row injected through the migrator
   role, since the CHECKs block the app role. DB and schema tests.
10. **Today.** A sorted list and its unsorted duplicate as sources produce
    identical Today bodies, and a list whose stored sort is unreadable still
    evaluates as a source. DB tests.
11. **Telemetry.** The span carries the static token and no values.
12. **Web.** Sort parse/label unit tests; DataTable sortable-only-with-meta,
    `aria-sort`, keyboard activation, other consumers unchanged; PeopleView
    header click refetches with the five-element key and `&sort=`; URL write,
    read, normalize, invalid drop and degrade; sort dirties a named list;
    Reset, Save, Save as and Duplicate carry the right sort; a shared reader
    has no Save; a named route ignores `?sort=`; a URL-origin 400 clears both
    parameters; uncertain-PUT reconciliation compares sort; the narrow layout
    with the Added column and the Person inspector open; cap message names
    the sort; existing 011a/011b suites pass.
13. **Performance gate** per §4, evidence archived under
    `docs/design/perf/slice-011b-sort-<date>/`.
14. **Final gates.** `./scripts/sqlx-prepare`, `./scripts/check`,
    `./scripts/check-db` once on the final tree; independent review.

## 12. Delivery

Single lane, one writer, branch `slice-011b-sort` off `main`; the lane owns
the migration and the `.sqlx` regeneration. Split seam if it runs long:
`sort1` backend (type, statements, routes, persistence, DB tests, measurement),
then `sort2` Web; the rung is not done until both land. The coordinator owns
this specification, the pointer amendments and the state documents.
