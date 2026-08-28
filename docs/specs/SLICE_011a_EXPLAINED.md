# Slice 011a, explained — how People filtering works under the hood

This is the developer companion to `SLICE_011a.md`. The spec is the
binding contract, written for review precision; this document walks
through the same slice in plain language: what got built, why it is
shaped the way it is, which APIs and files it touches, and the side
effects to be aware of. If the two ever disagree, the spec wins.

## 1. The one-paragraph version

Slice 011a puts a filter bar on the People page. An agent stacks up
conditions ("stage is Hot Prospect", "assigned to me", "not
contacted in 7 days"), the table narrows live, and the filter is
mirrored into the URL so the link can be shared. Nothing is stored
on the server — the real product of this slice is the **filter
language itself**: a strict, versioned JSON format plus one SQL
query that can execute any combination of it. Slice 011b saves
these filters as lists, 011c feeds them into Today, 011d rebuilds
Today's built-in rules on top of them. That is why so much care
goes into a feature that "just filters a table."

## 2. The pieces, end to end

A filtered request flows through five stations:

```
FilterBar.vue                  chips ⇄ URL ⇄ serialized JSON
  └─ web/src/lib/filter.ts     client-side (de)serializer
       └─ GET /api/people?filter=<percent-encoded JSON>
            └─ routes/people.rs           auth → decode → validate
                 └─ person/filter.rs      the language + validation
                      └─ person/queries.rs::filtered_summaries()
                                          one static SQL query
```

| Piece | File |
|---|---|
| Filter types, serde, validation, `describe()` | `backend/crates/crm-app/src/domain/person/filter.rs` |
| The filtered query | `backend/crates/crm-app/src/domain/person/queries.rs` (`filtered_summaries`) |
| `?filter=` handling on `GET /api/people` | `backend/crates/crm-api/src/routes/people.rs` |
| New `GET /api/inquiry-sources` endpoint | `backend/crates/crm-api/src/routes/inquiry_sources.rs` |
| Error mapping (`FilterError` → HTTP codes) | `backend/crates/crm-api/src/error.rs` |
| Trace-layer change (spans stop logging query strings) | `backend/crates/crm-api/src/lib.rs` |
| Wire types mirrored for TypeScript | `web/src/api/types.ts` |
| Client serializer/parser + chip labels | `web/src/lib/filter.ts` |
| Query hooks + cache keys | `web/src/api/queries.ts` |
| The filter bar UI | `FilterBar.vue`, mounted in `PeopleView.vue` |

No database migration. No new tables. The only `.sqlx` changes are
new cached entries for the new queries; every existing entry stays
byte-identical.

## 3. The filter language

A filter is JSON:

```json
{"version": 1, "clauses": [
  {"kind": "stage",        "stage_ids": ["<uuid>"]},
  {"kind": "assigned_to",  "assignees": ["me", "unassigned", {"user_id": "<uuid>"}]},
  {"kind": "source",       "sources": ["zillow", "website"]},
  {"kind": "created",      "age": {"op": "within_days", "days": 30}},
  {"kind": "last_inquiry", "age": {"op": "not_within_days", "days": 7}},
  {"kind": "last_contact", "age": {"op": "never"}},
  {"kind": "last_inbound", "age": {"op": "within_days", "days": 14}},
  {"kind": "has_replied",  "value": true},
  {"kind": "has_phone",    "value": true},
  {"kind": "has_email",    "value": false}
]}
```

Ten clause kinds. The combination rules are simple and fixed:

- **Clauses AND together.** Every clause must match.
- **Values inside a clause OR together.** `"sources": ["zillow",
  "website"]` means zillow OR website.
- **At most one clause per kind.** You cannot say "stage is X" and
  "stage is Y" as two clauses — put both ids in one clause.

There is no OR between clauses, no nesting, no free text. That is
deliberate: an AND-of-clauses model stays explainable ("here is why
this person is on your Today list") and is easy to render as chips.

### Why the decoding is so strict

The decoder rejects anything it does not recognize: unknown fields,
unknown clause kinds, unknown age ops, unknown assignee tokens, any
`version` other than 1. This "fail closed" posture looks pedantic
for a URL parameter, but remember where this type is headed: 011b
**persists** it. A saved list written by a future version of the
code must never be silently half-understood by an older reader — it
must fail loudly. Building the strictness now means 011b inherits
it for free.

One serde wrinkle worth knowing: serde does not honor
`deny_unknown_fields` on an internally tagged enum directly, so
each clause kind is a newtype variant wrapping its own
`deny_unknown_fields` struct. The unit tests pin the observable
behavior (unknown field → error), not the mechanism.

### Validation: two failure classes

`FilterDefinition::validate()` is a pure function, run first, and
maps to **HTTP 400 `malformed_request`**. It catches structural
nonsense: more than 20 clauses, more than 50 values in one array,
empty arrays, duplicate values, duplicate kinds, `days` outside
1–3650, `never` on `created` (every person has a created date, so
that filter could never match — treat it as a caller bug), and any
source string that is not already canonical (`^[a-z0-9_]{1,64}$`).

Sources are **rejected, never normalized**: `" Zillow "` is a 400.
This diverges on purpose from `Source::parse` (which trims and
lowercases on intake). The filter type gets persisted in 011b, and
a reader that normalizes would store non-canonical bytes — so only
canonical filters are ever valid. A well-formed source that no
longer exists in the org is fine, though; it simply matches nobody.

`validate_references()` runs second, hits the database, and maps to
**HTTP 422**: every `stage_id` must exist in the caller's org
(`invalid_stage`), every `user_id` must be an org member
(`invalid_assignee`). Two things matter here:

- A cross-org id and a nonexistent id produce **byte-identical**
  422s. The error must not leak whether some other org's uuid is
  real.
- Deactivated members are **valid** assignees (D-027: members are
  deactivated, never deleted, and their people remain visible — so
  you must be able to filter for them).

An empty `clauses: []` is valid and matches everyone. It still
takes the filtered code path — only a completely absent `?filter=`
parameter takes the legacy path (more on that below).

### "me" — how one link shows different people to different agents

The wire value `me` is a symbolic token. It survives validation
untouched, and only after validation does the route resolve it —
server-side — to the **calling** user's id
(`AuthContext.actor_user_id`) and append it to the bound user-id
array. The SQL never sees "me"; the URL never sees a resolved uuid.

The payoff: a link with `"assignees": ["me"]` is viewer-relative.
Alice opens it and sees her people; she sends it to Bob and he sees
his. This is the mechanism 011c/011d lean on for shareable Today
configurations. It also means `me` needs no membership check —
the caller is by definition a member.

## 4. The SQL: one query to rule all ten clauses

`filtered_summaries()` is **one** static `query_as!` with one fixed
SQL string — no query builder, no string formatting, no
per-combination variants. Every clause axis appears in the WHERE as
a NULL-guarded optional predicate:

```sql
AND ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
```

Bind NULL and Postgres short-circuits the guard — the predicate is
off. Bind a value and it filters. Ten axes, ~20 parameters, one
`.sqlx` cache entry, full compile-time checking. The alternative (a
dynamic QueryBuilder) would lose sqlx's offline verification; the
ladder records it as the future fork point if custom fields ever
join the vocabulary, but until then the fixed matrix wins.

The binding convention is the part that bites people:

- **Clause absent → every parameter of that axis binds NULL.**
- **Clause present → every parameter of that axis binds non-NULL**,
  even when the natural value is an empty array. The canonical
  case: `assigned_to: ["unassigned"]` binds an **empty** (not NULL)
  `uuid[]` plus `include_unassigned = true`. This is sound because
  in Postgres `x = ANY('{}')` is FALSE, never NULL. An empty array
  must never be used to mean "clause absent."

### What each axis actually checks

- **stage** — `p.stage_id = ANY(...)`.
- **assigned_to** — id match, OR `assigned_user_id IS NULL` when
  `unassigned` was requested.
- **source** — the source of the **latest** inquiry only (ladder
  decision 3), where "latest" is the established tie-break
  `ORDER BY received_at DESC, id DESC LIMIT 1`. A person's older
  inquiries never match, and a person with zero inquiries matches
  **no** source clause (the LATERAL yields NULL, and
  `NULL = ANY(...)` is not true). The original-attribution data
  (D-006) is untouched — this is a query-side reading only.
- **created / last_inquiry / last_contact / last_inbound** — each
  axis has a timestamp: `p.created_at`, `max(inquiry.received_at)`,
  `max(contact_attempted.occurred_at)`, and
  `max(correspondence_captured.occurred_at)` where
  `direction = 'inbound'` respectively. Two subtleties:
  - Contact-attempt **corrections** inherit their original's
    `occurred_at`, so a plain `max` needs no corrector-exclusion
    logic — a fixture test pins that equivalence.
  - A **backdated** inbound capture (D-042) filters by its email's
    own date. That is the point of backdating, and it is pinned.
- **has_replied** — simply "any inbound correspondence exists."
  Deliberately NOT "replied and we haven't answered" — that
  viewer-relative axis is 011d's `ClientRepliedUnanswered`. A test
  pins that an already-answered reply still matches, so the two
  never drift together.
- **has_phone / has_email** — a `contact_method` row of that kind
  exists (negated for `false`).

### The age operators, and the COALESCE trick

Each age axis supports three ops against a cutoff of
`now() - N days` (the clock lives in SQL, not Rust):

- `within_days` — `COALESCE(ts, '-infinity') > cutoff`
- `not_within_days` — `COALESCE(ts, '-infinity') <= cutoff`
- `never` — `ts IS NULL`

The COALESCE matters: a person who was **never** contacted has a
NULL timestamp, and NULL compares as unknown, silently dropping
them from both sides. Coalescing to `-infinity` makes "never" count
as "infinitely long ago" — so `not_within_days: 7` means "no
contact in 7 days, **including never contacted**." That is why the
op is named `not_within_days` and not `older_than`: a "stale leads"
list naturally wants the never-contacted, and an AND-only language
has no other way to express that OR. Every UI/`describe()` label
for this op carries an "(or never)" suffix so the semantics are
never invisible.

### Tenant isolation rules in the SQL

Two hard rules, both enforced in review:

1. `WHERE p.organization_id = $1` is **literal text** — the org
   boundary is never behind a NULL-guard or any parameter trick.
2. Every correlated subselect and LATERAL carries
   `AND x.organization_id = p.organization_id`. The pre-existing
   people queries filter some probes on `person_id` alone (correct,
   but index-blind — recorded debt); the new SQL does not copy that
   trap.

### Output contract and performance

Projection, ordering, and cap are identical to the unfiltered
query: same `PersonSummary` columns, `ORDER BY created_at DESC,
id ASC`, fetch 501, `truncated = len > 500`, truncate to 500. A
filter narrows rows and changes nothing else.

Performance posture, recorded not built: the query scans the org's
people slice (no new indexes — e.g. no `(organization_id,
stage_id)`). That is accepted as fine to roughly 50k people per
org; the levers (denormalized last-activity columns, etc.) live in
the ladder document, unbuilt.

## 5. The HTTP surface

### `GET /api/people?filter=<percent-encoded JSON>`

An additive amendment to the SLICE_002 contract. The rules:

- **Absent `filter` → the untouched legacy path.** The handler
  calls the same `list_summaries` as before — same SQL, same
  `.sqlx` entry, same response shape, order, and cap. This is
  pinned by regression tests; the legacy path was deliberately NOT
  refactored onto the new query.
- Present → decode, `validate()`, `validate_references()`, resolve
  `me`, run `filtered_summaries`. The response shape is identical
  to the unfiltered one: `{"people": [...], "truncated": bool}`.
- **Error order is pinned: 401 → 400 → 422 → 200/503.** A caller
  with no session gets 401 even with a garbage filter — the server
  will not spend parse effort on unauthenticated input. (In the
  handler this falls out of extractor order: `AuthContext` runs
  before the query string is even looked at.) Structural failures
  are 400 `malformed_request` in the standard `{"error": code}`
  envelope — never axum's bare rejection body. Org-scoped id
  failures are the 422s.
- Edge pins: unknown extra query params (`?foo=bar`) are ignored,
  exactly as before this slice. A present-but-**empty** `?filter=`
  is a 400 (empty string is not JSON; only truly absent means
  legacy). axum's `Query` extractor already percent-decodes, so
  the handler sees plain JSON text.

This is the API's first GET query parameter, which is why the edge
behavior is spelled out so explicitly — it sets precedent.

### `GET /api/inquiry-sources` (new)

Feeds the FilterBar's Source picker. Returns
`{"sources": ["email", "website", ...], "truncated": bool}`:
distinct sources across the org's inquiries, ascending, with the
house 501-fetch/500-cap math (sources are member-writable free-ish
text, so the list is capped like every other list). Any
authenticated member; org comes from the session only. Note the
picker is a convenience, not an authority — a filter may name a
source absent from this list and is still valid (matches nobody).

## 6. Side effects — changes beyond the feature itself

These are the changes a reviewer might not expect from the feature
description:

- **Request spans stop logging query strings, on ALL routes.**
  tower-http's `DefaultMakeSpan` records the full URI, query string
  included — which would put every filter (user-entered sources,
  stage names by id, day windows) verbatim into every span. The
  slice replaces it with a custom `make_span_with` in
  `crm-api/src/lib.rs` that records the **path only**. Every other
  span field is unchanged. The `/api/people` span additionally
  records `filter_kinds` (just the clause-kind names, comma-joined)
  and `filter_clause_count` — never clause values. A test captures
  a span for a `?filter=` request and asserts the query string is
  absent.
- **`error.rs` gains a `From<FilterError>` mapping** onto the
  existing `malformed_request` / `invalid_stage` /
  `invalid_assignee` codes — reusing the exact codes the stage and
  assignment mutations already produce, so a filter naming a bad id
  errors byte-identically to those routes.
- **SLICE_002 §5 is amended in place** (declared, not silent): the
  `GET /api/people` row notes the `filter` param, and a new row
  documents `GET /api/inquiry-sources`.
- **`backend/.sqlx/` gains new cache entries** for the new queries.
  Every existing entry — `list_summaries` above all — must remain
  byte-identical (`git diff --stat backend/.sqlx` should show only
  additions).
- **What does NOT change:** no migration, no CORS changes (GET was
  already allowed), no realtime changes, no new indexes, no
  Operator tool.

## 7. The frontend

### API layer (`web/src/api/`)

`types.ts` mirrors the wire shape as TypeScript unions
(`FilterClause`, `AgeOp`, `Assignee`, `FilterDefinition`) plus
`InquirySourcesResponse`. The client only ever constructs
well-formed filters, so it does no strict-decode enforcement of its
own — the server is the authority.

`queries.ts` has the subtlest change in the slice. The cache-key
factory becomes:

```ts
people: (orgId, serializedFilter?) =>
  serializedFilter
    ? ['org', orgId, 'people', serializedFilter]
    : ['org', orgId, 'people']
```

When unfiltered, the filter element is **omitted entirely** — never
appended as `undefined`. TanStack Query hashes `undefined` to
`null`, so `['org', orgId, 'people', undefined]` would be a
*different key* from today's `['org', orgId, 'people']`, orphaning
the existing cache and breaking test expectations. With omission,
the unfiltered key is byte-identical to before, and TanStack's
**prefix matching** means every existing invalidation (the realtime
`person.changed` handler, mutation-driven `['org', orgId]` sweeps)
automatically covers filtered queries too — `realtime/events.ts`
needed zero changes. When a person changes, filtered views refetch
along with everything else.

`usePeople(orgId, serializedFilter?)` appends
`?filter=<encodeURIComponent(json)>` when a filter is set;
`useInquirySources(orgId)` backs the Source picker.

### `web/src/lib/filter.ts`

A thin client-side reader/writer for the same wire shape:
`serializeFilter` (chips → canonical JSON string, used for both the
URL and the query key — one string, two uses, so they can never
disagree), `parseFilter` (URL → chips, returning `null` for
anything unrecognizable), and `describeClause` for chip labels.
Chip labels are rendered client-side from data the page already has
(stages, members, sources); parity with the backend's `describe()`
is explicitly not required.

### `FilterBar.vue` and PeopleView behavior

The bar sits above the DataTable: an "Add filter" control opens a
small editor per axis; active clauses render as removable chips
(built on `Badge.vue`, neutral tint per UI_STYLE — no new colors).
The day-count input applies on commit (blur/Enter), not per
keystroke — chips change discretely, so there is no debouncing.

The live count is just the existing table footer ("N people" plus
the truncated notice). Under the 500 cap that is `min(matches,
500)` — there is deliberately no true-COUNT surface in this slice.

URL sync: chip edits `router.replace` the `?filter=` query param;
mounting with `?filter=` present rehydrates the chips. Failure
degrades gracefully in both directions, identically:

- An **unparseable** URL filter (truncated paste, hand-mangled
  JSON) → chips empty, param cleared via `router.replace`, no
  error toast — the page behaves as plain People.
- A **parseable filter the server rejects** (400/422 — e.g. a link
  shared across orgs carrying the other org's stage ids) → same
  degradation: drop the filter, clear the param, refetch unfiltered.

A shared broken link lands you on the ordinary People page, never
an error screen. `me` stays symbolic in the URL throughout.

### `describe()` — built, not yet wired

The backend's `FilterDefinition::describe()` produces one
human-readable line per clause ("Stage is Lead or Hot Prospect",
"Last contact not within the last 7 days (or never)") from
pre-resolved name maps, keeping it DB-free and unit-testable. In
011a it ships fully tested but attached to no HTTP surface — 011b's
list index, 011c's Today reasons, and 011d's feeds admin are its
consumers.

## 8. Gotchas checklist for anyone touching this code

- Absent `?filter=` must keep calling the untouched
  `list_summaries`. Do not "simplify" by routing everything through
  `filtered_summaries`.
- Never bind an empty array to mean "clause absent" — NULL means
  absent, empty array is a real (matches-nothing-by-id) value.
- Never normalize a source in the filter path; reject.
- `me` must never reach SQL as a token or appear resolved in a URL.
- The org boundary stays literal SQL text; every new
  subselect/LATERAL repeats the `organization_id` correlation.
- Extend the query-key factory; never hand-write a key, and never
  append `undefined` to one.
- Anything user-entered stays out of tracing spans — kinds and
  counts only.
- Cross-org and nonexistent ids must stay byte-identical 422s.

## 9. Where the tests live

Unit tests (service-free) sit with `filter.rs`: wire round-trips,
fail-closed decoding, the validation matrix, exact `describe()`
strings. DB tests are `crm-api/tests/db_people_filter.rs`:
per-axis positive/negative pins, the viewer-relative `me` pin (two
members, same URL, different rows), latest-source and tie-break
pins, the within/not-within exact-complement pin, cap and ordering
under filter, the 401→400→422 error-order pins, tenant isolation,
and the absent-param byte-equality regression. Web tests (Vitest,
colocated) cover the FilterBar chips, URL rehydration and both
degradation paths, and the sources picker. Acceptance criteria
1–13 in the spec map each of these to its clause.
