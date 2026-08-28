# Slice 011a — Filter vocabulary + ad-hoc People filtering

**In plain language:** this slice adds filter chips to the People
page — narrow the list by stage, assignee, lead source, how long
since anyone was contacted, and so on, with a live match count.
The filter lives in the URL so it can be shared ("assigned to me"
means the person opening the link). Nothing is saved yet; saved
lists and Today integration come in later rungs. Under the hood it
defines the filter "language" the whole Smart Lists ladder builds
on — see the plain-language overview in
`docs/plans/SLICE_011_LADDER.md`, and for a detailed but
readable developer walkthrough of how this slice works,
`SLICE_011a_EXPLAINED.md`.

Status: independently reviewed READY-WITH-FIXES — all five
required fixes folded in below (F1 span query-strip §7, F2
assigned_to binding convention §4e, F3 canonical-source rule §4b,
F4 query-param edge pins §5a/§8, F5 mount-time-422 behavior §6),
plus the detail-class notes (serde tagging §4a, zero-inquiry
source semantics §4c, query-key omission §6, "or never" describe
wording §4d, tint citation §6). No BLOCKING_DECISION items found;
§10's defaults audited as genuinely safe. Charter: D-043 (the filter
model IS the Today configuration language) via the accepted ladder
`docs/plans/SLICE_011_LADDER.md` (rung 011a row + the three
ladder-level decisions, notably decision 3: `Source` matches the
LATEST inquiry). The user pre-authorized implementation for this
rung 2026-08-28 ("draft SLICE_011a.md but implement in sonnet");
content approval is collected at the commit gate. Decisions in
force: D-043, D-019 (stages are org rows), D-027 (members
deactivate, never delete), D-042 (correspondence capture), D-006
(original attribution untouched — the latest-inquiry reading is
query-side only). Planner ground-truth survey folded in throughout.

Targets `main` ≥ `be81e04`. No persistence in this rung: no
migration, no new tables, nothing saved server-side.

## 1. User-visible outcome

On the People page an agent composes a filter from chips: Stage,
Assigned to (including Me and Unassigned), Source, age clauses
(Created, Last inquiry, Last contact, Last inbound — each "within
N days" / "not within N days" / "never"), Has replied, Has phone,
Has email. The table narrows live with a visible match count; the
filter is reflected in the URL so it can be shared — a colleague
opening the link sees the same filter (with "Me" meaning *them*).
Nothing is saved; clearing the filter or navigating away discards
it. Saved lists are 011b.

## 2. In / out of scope

**In:** the typed versioned `FilterDefinition` + validation +
`describe()` in crm-app; `filtered_summaries()` as one fixed-matrix
static query; `GET /api/people?filter=` (declared additive
SLICE_002 §5 amendment; absent param = byte-identical behavior);
`GET /api/inquiry-sources` (new endpoint, SLICE_002 §5 row added);
PeopleView FilterBar (chips, live count, URL sync); unit + db +
web tests; live walkthrough.

**Out (explicit):** persistence of any kind (`saved_list` is 011b);
Today integration (011c); derived axes `AwaitingResponse` /
`ClientRepliedUnanswered` / `AwaitingCallOutcome` (011d); tags
(011e); OR-groups, absolute dates, per-list sorts; free-text/fuzzy
matching (O-010 stays parked); an Operator `filter_people` tool;
pagination or a true-COUNT surface (the 500 cap + `truncated` flag
carry over); new indexes (the ladder's O(people-in-org) posture is
accepted and recorded; levers stay unbuilt); realtime count push;
mobile.

## 3. Persistence

None. No migration. `backend/.sqlx/` gains new cached entries for
the new queries (normal `./scripts/sqlx-prepare` output); every
existing cached query — `list_summaries` above all — stays
byte-identical.

## 4. Domain — `FilterDefinition` (new `crm-app/src/domain/person/filter.rs`)

### 4a. Wire shape (the contract; serde derives are detail)

```json
{"version": 1, "clauses": [
  {"kind": "stage",       "stage_ids": ["<uuid>", "..."]},
  {"kind": "assigned_to", "assignees": ["me", "unassigned", {"user_id": "<uuid>"}]},
  {"kind": "source",      "sources": ["zillow", "website"]},
  {"kind": "created",      "age": {"op": "within_days", "days": 30}},
  {"kind": "last_inquiry", "age": {"op": "not_within_days", "days": 7}},
  {"kind": "last_contact", "age": {"op": "never"}},
  {"kind": "last_inbound", "age": {"op": "within_days", "days": 14}},
  {"kind": "has_replied", "value": true},
  {"kind": "has_phone",   "value": true},
  {"kind": "has_email",   "value": false}
]}
```

- Top level, every clause object, and every nested object are
  `deny_unknown_fields`. Unknown `kind`, unknown `op`, unknown
  `version` (anything ≠ 1), or an unknown assignee token all fail
  closed as decode errors. (Serde does not honor
  `deny_unknown_fields` directly on an internally tagged enum —
  enforce it via newtype variants wrapping per-clause
  `deny_unknown_fields` structs or a manual deserializer; unit
  test 1 pins the behavior, not the mechanism.) This same type is what 011b persists;
  fail-closed decode IS the ladder's "unknown-clause-fails-closed
  on read" — built now, inherited then.
- `clauses` is an AND. **At most one clause per `kind`** (a
  duplicate kind → invalid). Multi-value arrays within a clause are
  OR (`stage_ids`, `assignees`, `sources` — IN semantics).
- Typed ids in the parsed form: `StageId`, `UserId` (ids.rs; bind
  `.0` discipline).

### 4b. Validation (`FilterDefinition::validate`, pure; ordering = clause order, first failure wins)

Structural (→ HTTP 400 `malformed_request`):

- `clauses.len() > 20` (ladder cap; a formal backstop above the
  one-per-kind rule), any value array `len > 50`, any value array
  empty, duplicate values within one array (canonical filters
  only), duplicate clause kind;
- `days` outside `1..=3650`;
- a uuid value (`stage_ids`, `user_id`) not in canonical
  lowercase-hyphenated form (urn:/braced/simple/uppercase wire
  forms → 400 at decode; canonical filters only — 011b persists
  re-serialized values and alternate forms would not round-trip
  byte-stable). Duplicate JSON keys ANYWHERE (top level, clause,
  age, assignee objects) fail closed — last-wins acceptance would
  let an unknown `kind` token hide in wire bytes;
- `age {op: never}` on `created` (`created_at` is NOT NULL — a
  filter that can never match anything is a caller error);
- a `source` value that does not ALREADY match
  `^[a-z0-9_]{1,64}$` verbatim. Reject, never normalize: `" Zillow "`
  is a 400, a deliberate divergence from `Source::parse`'s
  trim-then-lowercase (canonical filters only — 011b persists this
  type, and a normalizing reader would persist non-canonical
  wire bytes). A stale but well-formed source is VALID and simply
  matches nobody.

Org-scoped (→ HTTP 422, non-leaking, run after structural):

- every `stage_id` must exist in the active org (`stage::exists`)
  → `invalid_stage`;
- every `user_id` must be an org member via
  `is_organization_member` — any status, deliberately: D-027 keeps
  people assigned to deactivated members visible and filterable
  → `invalid_assignee`.

Cross-org and nonexistent ids produce byte-identical 422s
(SLICE_002 §6 posture).

An empty `clauses: []` is VALID (matches everyone) and takes the
filtered code path; only an ABSENT `filter` param takes the
untouched legacy path (§6).

### 4c. Semantics per clause (binding; each gets a db pin)

- `stage`: `p.stage_id = ANY(stage_ids)`.
- `assigned_to`: `p.assigned_user_id = ANY(user_ids)`, OR'd with
  `p.assigned_user_id IS NULL` when `unassigned` is present. `me`
  resolves SERVER-SIDE to `AuthContext.actor_user_id` (routes/
  today.rs posture: the viewer is never a parameter) and is
  appended to the bound user-id array AFTER validation — `me` needs
  no membership check and appears symbolically in URLs, so one
  shared link is viewer-relative by design (D-043's control story;
  what 011c/d need).
- `source`: the LATEST inquiry's source (ladder decision 3), where
  "latest" is the established contract tie-break
  `ORDER BY i.received_at DESC, i.id DESC LIMIT 1` (SLICE_003 §14a
  / today/queries.rs). A Person's older inquiries never match, and
  a zero-inquiry Person NEVER matches any `source` clause (the
  latest-source lateral yields NULL — pinned in test 4). D-006
  original attribution is untouched data.
- Age axes — each `ts` defined as:
  - `created`: `p.created_at`;
  - `last_inquiry`: `max(i.received_at)` over the Person's
    inquiries;
  - `last_contact`: `max(ca.occurred_at)` over `contact_attempted`.
    Corrections INHERIT their original's `occurred_at`
    (person/queries.rs invariant), so plain `max` needs no
    corrector-exclusion for a pure age clause — this equivalence is
    pinned by a correction-fixture test;
  - `last_inbound`: `max(c.occurred_at)` over
    `correspondence_captured` with `direction = 'inbound'`. A
    backdated capture (D-042 retroactive forward) filters by its
    email's own date — the `backdated` flag's whole point; stated,
    pinned.

  Operators (cutoff = `now() - make_interval(days => N)`, clock in
  SQL):
  - `within_days`:     `COALESCE(ts, '-infinity'::timestamptz) > cutoff`
  - `not_within_days`: `COALESCE(ts, '-infinity'::timestamptz) <= cutoff`
    (the exact complement — includes never; this is why the op is
    named `not_within_days` and not "older_than": a "stale" list
    naturally includes the never-contacted, and AND-only filters
    cannot express the OR otherwise)
  - `never`: `ts IS NULL`
- `has_replied`: `EXISTS` any inbound `correspondence_captured`
  row for the Person, negated for `false`. DELIBERATELY the simple
  existence predicate — the viewer-relative "replied and
  unanswered" is 011d's `ClientRepliedUnanswered` axis (ladder
  decision 2); a Person whose reply was long since answered still
  matches `has_replied: true`, and a test pins exactly that so the
  distinction cannot drift.
- `has_phone` / `has_email`: `EXISTS` a `contact_method` of that
  kind for the Person, negated for `false` (served by the
  `(person_id, kind, normalized_value)` unique index).

### 4d. `describe()`

`FilterDefinition::describe(&self, names: &FilterNames) ->
Vec<String>` — pure, one human-readable line per clause in clause
order, taking pre-resolved name maps (`stage_id → name`,
`user_id → display_name`) so it stays DB-free and unit-testable.
Canonical forms (exact strings pinned by unit tests; the pattern is
binding, full wording is implementation detail):
"Stage is Lead or Hot Prospect", "Assigned to me or unassigned",
"Source is zillow or website", "Created within the last 30 days",
"Last contact not within the last 7 days (or never)", "Never
contacted", "Has replied", "No phone number" — every
`not_within_days` line carries the "(or never)" suffix so the
complement semantics are never invisible (review F10). An unresolvable id renders a
neutral placeholder ("an unknown stage"), never the raw uuid.
011a ships `describe()` unit-tested but not yet wired to any HTTP
surface — 011b/c/d consume it (list index, Today reasons, feeds
admin); the web FilterBar renders its own chip labels client-side
from data it already has.

### 4e. `filtered_summaries()` (person/queries.rs)

One `query_as!` with ONE static SQL string — the fixed matrix. No
`QueryBuilder`, no format!, no per-combination variants. Every
clause axis appears as a NULL-guarded optional predicate over bound
params, disabled by binding NULL:

```sql
($2::uuid[]  IS NULL OR p.stage_id = ANY($2))
($3::uuid[]  IS NULL OR ... assignment predicate with $4 include_unassigned ...)
($5::text[]  IS NULL OR latest_source = ANY($5))
($6::int     IS NULL OR COALESCE(ts, '-infinity'::timestamptz) > now() - make_interval(days => $6))
-- ... one within / not_within / never param set per age axis ...
($n::boolean IS NULL OR (EXISTS (...)) = $n)
```

(Param numbering illustrative; the pattern is binding.) Binding
convention (review F2, binding): CLAUSE ABSENT → every param of
that axis binds NULL; CLAUSE PRESENT → every param of that axis
binds non-NULL — in particular `assigned_to: ["unassigned"]`
binds a non-NULL EMPTY `uuid[]` plus `include_unassigned = true`,
and the assignment predicate is
`($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3) OR
($4::boolean AND p.assigned_user_id IS NULL))` — sound because
`x = ANY('{}')` is FALSE (never NULL). Never use an empty array
as a clause-absent marker. Rules:

- `WHERE p.organization_id = $1` stays LITERAL TEXT — the org
  boundary is never behind a guard or a param trick.
- Every correlated subselect/LATERAL carries
  `AND x.organization_id = p.organization_id` — including the
  latest-inquiry and last-inquiry probes, which in the EXISTING
  queries filter on `person_id` alone. The planner's trap 2 is not
  copied into new SQL. (Correct-but-index-blind existing queries
  are recorded debt, untouched here.)
- Projection = the same `PersonSummaryRow` columns; ordering and
  cap = the legacy contract exactly: `ORDER BY p.created_at DESC,
  p.id ASC LIMIT 501`, `truncated = len > 500`, truncate to 500.
- The `me`-resolved viewer id arrives as a plain member of the
  bound user-id array; the SQL knows nothing of "me".

Recorded, not built (ladder standing tension): no new indexes — a
stage clause scans the org slice (no `(organization_id, stage_id)`
index), the effective-attempt corrector probe stays unindexed.
Fine to ~50k people; the levers (denormalized last-activity
columns, custom-plan mode, QueryBuilder fork) remain recorded in
the ladder only.

## 5. HTTP contracts

### 5a. `GET /api/people?filter=<percent-encoded JSON>` — declared additive amendment, SLICE_002 §5 (AGENTS.md §11)

- **Absent `filter` → byte-identical to today**: the handler calls
  the untouched `list_summaries`; same query, same `.sqlx` entry,
  same shape `{"people": [...], "truncated": bool}`, same order,
  same cap math. Pinned by regression tests.
- Present: percent-decoded, JSON-decoded, validated (§4b),
  executed via `filtered_summaries`. Response shape/order/cap
  IDENTICAL to the unfiltered response — the filter narrows rows,
  nothing else.
- Error order, pinned: **401 first** (no filter parsing for
  unauthenticated callers — the substantial-parse counterpart to
  the path-id extractor's pre-auth 400; a deliberate, stated
  divergence from that precedent), then 400 `malformed_request`
  (undecodable/oversized/structurally invalid per §4b — mapped
  into the `{"error": code}` envelope, never axum's bare rejection
  body), then 422 `invalid_stage` / `invalid_assignee` (org-scoped,
  non-leaking), then 200/503.
- Query-string edges (review F4, pinned): UNKNOWN extra query
  params are IGNORED (`?foo=bar` stays 200 exactly as today — no
  `deny_unknown_fields` on the query struct; that discipline is
  for the filter JSON, not the query string); a PRESENT-but-empty
  `?filter=` (or `?filter`) is a 400 `malformed_request` (empty
  string is not JSON; only a truly absent param is the legacy
  path).
- This is the API's first GET query param; the parse lives behind
  the envelope discipline from day one.

### 5b. `GET /api/inquiry-sources` — new endpoint, SLICE_002 §5 row added

200 `{"sources": ["email", "website", ...], "truncated": bool}` —
org-scoped `SELECT DISTINCT source FROM inquiry WHERE
organization_id = $1 ORDER BY source ASC LIMIT 501` + the house
truncate-to-500 math (sources are member-writable free-ish text;
capped like every other list). Any authenticated member; org from
session only (`auth.active_organization_id` directly — org data,
not Person visibility; the `GET /api/stages` pattern). 401/503
standard. Feeds the FilterBar's Source picker; a filter may still
name a source not in this list (it matches nobody — §4b).

CORS method list unchanged (GET already allowed). The SLICE_002 §5
pointer amendments (filter param on the `GET /api/people` row +
the new row) ride with this slice's implementation.

## 6. Web — PeopleView FilterBar

- New `FilterBar.vue` above the DataTable: an "Add filter" control
  (existing `Select`/popover primitives, `controls.ts`
  pass-throughs) opening one small editor per axis; active clauses
  render as removable chips built on `Badge.vue` — neutral tint
  (UI_STYLE §3 tint table: warm stays reserved for source/origin
  accents, one accent color, no new palette).
- Chip labels client-side from data already loaded: stages
  (`GET /api/stages`), members (existing members query), sources
  (new `useInquirySources`), "Me"/"Unassigned" literals.
- Live count = the existing DataTable footer (`N people` +
  truncated notice) — under the 500 cap that is `min(matches,
  500)`, and the truncated notice already says so. No new count
  surface.
- URL sync: the serialized filter lives at `/people?filter=<same
  percent-encoded JSON as the API>`; chip edits `router.replace`
  the query (PersonDetailView `?outcome=` precedent); mounting
  with `?filter=` present rehydrates the chips. An invalid or
  undecodable URL filter is DROPPED on mount (chips empty, query
  param cleared via `router.replace`, no error toast — a shared
  broken link degrades to the plain People page). A DECODABLE
  filter the SERVER rejects (a 400/422 on the rehydrated fetch —
  e.g. a link shared across orgs carrying org-B stage ids)
  degrades IDENTICALLY: drop the filter, clear the param, refetch
  the plain list (review F5; web-test-pinned) — but ONLY for
  filters arriving FROM the URL; a 5xx on any filtered fetch keeps
  chips and URL intact (error banner, the F5 complement). History
  navigation (back/forward) re-rehydrates chips from the URL, not
  just mount (watcher on `route.query.filter`). DRAFT clauses
  with empty value arrays are never serialized to the URL or the
  wire (they are wire-invalid §4b; chips render while editing,
  the fetch fires on the first committed value). `me` stays
  symbolic in the URL: the same link is viewer-relative.
- Data flow: `queryKeys.people(orgId)` gains an optional
  normalized-filter component — `['org', orgId, 'people',
  serializedFilter?]` — extended IN THE FACTORY (SLICE_002 §10
  rule: never hand-write a key). Unfiltered, the element is
  OMITTED ENTIRELY (`['org', orgId, 'people']`, byte-identical to
  today's key) — never appended as `undefined`, which TanStack
  hashes to `null` and which would orphan existing cache and test
  expectations (review F9). TanStack prefix matching keeps
  every existing invalidation (realtime `person.changed`,
  mutation-driven `['org', orgId]` sweeps) covering filtered
  queries with zero changes to `realtime/events.ts`.
- No debounce complexity: chips change discretely; the days input
  applies on commit (blur/Enter), not per keystroke.

## 7. Authorization, isolation, observability, failure

- Both endpoints: any authenticated member; org from session only;
  Person visibility remains `Organization(active_organization_id)`
  (AGENTS §4.4) — a filter narrows the org slice and can never
  widen it. `me` is derived from `AuthContext.actor_user_id`,
  never a request value.
- Tenant isolation pins: a filter matching org-B people returns
  zero org-B rows; an org-B `stage_id`/`user_id` in the filter →
  422 byte-identical to a random uuid; `GET /api/inquiry-sources`
  never lists another org's sources.
- Observability: the `/api/people` span gains `filter_kinds` (the
  static clause-kind vocabulary, comma-joined) and `filter_clause_
  count`. NO clause values, ids, sources, or day counts in spans
  (minimum-necessary posture; sources are user-entered text).
  **Declared slice-owned trace-layer change (review F1)**: the
  request span currently uses tower-http's `DefaultMakeSpan`
  (crm-api `lib.rs`), which records the FULL URI — query string
  included — so the filter JSON would land in every span verbatim.
  This slice replaces it with a custom `make_span_with` recording
  method + PATH ONLY (query string stripped) for ALL routes;
  every other recorded field stays as-is. Without this, §7 is
  unsatisfiable. Pinned by a test capturing the span for a
  `?filter=` request and asserting the query string is absent.
  `/api/inquiry-sources` gets the standard request span.
- Failure: DB down → 503 `unavailable` (both endpoints); every
  filter-decode failure → 400 envelope (§5a); an over-long URL is
  bounded by the ≤20/≤50 caps long before server URI limits
  matter (noted, untested).

## 8. Acceptance criteria

Unit (service-free, crm-app):
1. Wire round-trip of every clause kind + every age op; unknown
   kind / op / version / assignee token / field all fail closed;
   `deny_unknown_fields` at every level.
2. Validation matrix: >20 clauses, >50 values, empty array,
   duplicate value, duplicate kind, days 0 / 3651, `never` on
   `created`, malformed source, NON-CANONICAL source (`" Zillow "`,
   `"ZILLOW"` — rejected, never normalized, §4b) — each rejected
   with the structural class; well-formed-but-stale source
   accepted.
3. `describe()` exact strings for every clause kind and every age
   op, incl. multi-value joins, `me`/`unassigned` wording, and the
   unresolvable-id placeholder.

DB (`crm-api/tests/db_people_filter.rs`; harness + fixture style
per `db_people.rs` / `db_today_client_replied.rs` — direct
migrator-pool fixture rows for correspondence):
4. Per-axis positive + negative for all ten clause kinds,
   including: `unassigned`; mixed users+unassigned; `me` resolving
   to the CALLER (two members, same URL, different rows — the
   viewer-relative pin); latest-source (older zillow + newer
   website matches `website`, NOT `zillow`); a zero-inquiry
   Person matching NO `source` clause (§4c); latest-inquiry
   tie-break (equal `received_at` → higher id wins); last-contact
   correction fixture (plain-max ≡ effective-attempt); backdated
   inbound filtering by its inner date; `has_replied: true`
   matching a Person whose reply was ANSWERED (the 011a-vs-011d
   distinction pin); `within`/`not_within` exact complement on one
   fixture set; `never` per nullable axis.
5. AND composition (two clauses intersect); empty `clauses: []` =
   the full list through the filtered path.
6. Absent-param regression: response byte-equal to the pre-011a
   contract (existing `db_people.rs` pins stay green, untouched).
7. Cap: 501 matching people → 500 rows + `truncated: true` under a
   filter; ordering `created_at DESC, id ASC` holds filtered.
8. Error contract: 401 with no session even with a garbage filter
   (401-before-400 pin); 400 for undecodable JSON in the envelope
   shape; unknown extra query param (`?foo=bar`) → 200 exactly as
   today; present-but-empty `?filter=` → 400 (§5a edges); 422
   `invalid_stage`/`invalid_assignee` cross-org ≡ nonexistent
   (byte-identical bodies); first-failure-wins clause order.
9. Tenant isolation per §7 (org-B rows, org-B ids, org-B sources).
10. `GET /api/inquiry-sources`: distinct, ascending, org-scoped,
    member-allowed, cap math.

Web (Vitest, colocated):
11. FilterBar: add/remove chip → refetch with the new factory key;
    count + truncated notice render; days input commits on blur.
12. URL: mount with `?filter=` rehydrates chips; chip edit
    `router.replace`s the query; invalid URL filter → dropped +
    param cleared; decodable-but-server-rejected filter (mocked
    422) → same drop/clear/refetch degradation (§6); `me`
    serialized symbolically.
13. Sources picker populated from the new query.

Live walkthrough (before the commit gate): on the real dev stack —
filter seeded people by stage + assigned-to-me + a source; share
the URL between alice and bob and see viewer-relative results;
never-contacted and not-within-7-days lists; sources endpoint
showing the real org's intake history; count/truncated behavior.

## 9. Lane and checks

Single lane, one writer (Sonnet implementation subagent under
coordinator review — the proven Option A workflow), branch
`slice-011a-filter-vocabulary` off `main`. No migration owner
(none exists). The ladder's a1/a2 split seam (backend | web) stays
dormant unless the rung runs hot. Gates: `./scripts/check` +
`./scripts/check-db` (source `~/.nvm/nvm.sh` first; run
`./scripts/sqlx-prepare` after adding queries); independent review
+ adversarial testing; live walkthrough; Phase 9 commit/merge
approvals as always.

## 10. Safe defaults adopted (reviewer/user may veto)

(a) Error split: structural → 400 `malformed_request`, org-scoped
id failures → existing non-leaking 422 codes; (b) `me` symbolic in
wire + URL, resolved server-side (viewer-relative shared links);
(c) live count = capped footer count + truncated notice, no COUNT
surface; (d) `has_replied` = simple inbound-existence (derived
unanswered axis is 011d's); (e) age ops are
`within_days`/`not_within_days`/`never` with `not_within` the
exact complement (includes never); (f) one clause per kind,
values-OR within a clause, clauses-AND across; (g) canonical-only
filters (duplicate values/kinds → 400); (h) 401 before filter
parsing (divergence from the path-id pre-auth-400 precedent,
stated); (i) empty `clauses` valid, absent param = legacy path;
(j) `days` bounds `1..=3650`; (k) `never` rejected on `created`;
(l) deactivated members are valid `assigned_to` values (D-027);
(m) sources endpoint caps at 500 with the house truncate math;
(n) invalid AND server-rejected URL filters both degrade to the
plain People page (drop + clear, no toast);
(o) `describe()` ships unwired to HTTP (011b+ consumes it);
(p) the request-span layer records path-only (query stripped) for
all routes — the F1 fix, a slice-owned trace-layer change;
(q) wire sources must be pre-canonical (reject, never normalize —
divergence from `Source::parse` stated);
(r) unknown extra query params ignored; empty `?filter=` → 400.
