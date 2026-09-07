# Slice 011b — Saved lists

**Approved amendment — Slice 011c (2026-09-06):**
[SLICE_011c §§2–6](SLICE_011c.md) extend §§3–7 with per-agent Today source
controls/routes, live saved-definition evaluation, atomic source-preference
cleanup on list deletion, and actor-aware cache/refresh behavior. Existing
personal-list privacy, definition limits and save/copy permissions remain.
Only the viewer's saved baseline can be enabled; unsaved filter edits do not
silently change a source.

**Status: APPROVED — user, 2026-09-06, after independent Astra/ultra review
and the plain-language companion. Implementation is authorized.** Drafted
against `main` at `1635fc4`; D-046 supplies the accepted privacy and limits.
Specification/coordination/review use the user-requested Astra / ultra;
implementation, tests and fixes use Terra / ultra. Implementation branch:
`codex/slice-011b-saved-lists`.

In plain language: save a People filter under a name and return to its current
matches later. Personal lists are private to their creator, including from
administrators. Shared lists are visible to the Organization and maintained
by its administrators; agents can make their own copy. A list saves criteria,
not a snapshot of People. “Me” always means the person viewing the list.

Authority: [D-043, D-046](../decisions/DECISION_LOG.md), the accepted
[011 ladder](../plans/SLICE_011_LADDER.md), and [011a](SLICE_011a.md).
D-004/D-005, D-007/D-008, D-019/D-027, D-023 and D-045 also apply.
The [architecture baseline](../architecture/ARCHITECTURE_BASELINE.md)
and [implementation brief](../tasks/SLICE_011b_IMPL.md) supply context.

## 1. Outcome, scope and boundaries

Add a Lists navigation item and index; create personal/shared lists from the
current People filter; open a list in the existing People workspace; explicitly
save edits; Save as, Duplicate, and Delete. Include typed commands, persistence,
HTTP, capped membership counts, privacy/tenant isolation and meaningful tests.

Reuse v1 `FilterDefinition` unchanged, including empty clauses (all People),
latest-inquiry Source, inactive assignees, symbolic `me`, and fail-closed
validation. Membership is evaluated when read against authoritative PostgreSQL
data. Saving, copying or deleting a list never mutates a Person or its history.

Out of scope: Today integration (011c), built-in feeds (011d), tags (011e),
per-list sorting (the separately queued 011b-sort follow-up),
People search, OR groups, collections, manual member selection, pinned/default
lists, shared-list ownership transfer, personal-to-shared conversion in place,
bulk operations, mobile and Operator tools. No new dependency, service, general
idempotency framework, materialized membership, realtime count push or list
event is needed. Existing People ordering and first-500 selection stay intact.

The existing compact toolbar, applied-chip/editor behavior, URL races and
fallback rules on `/people`, full-profile links, and D-045 Person inspector
are regression requirements. This slice extends that workspace rather than
replacing it with a separate table implementation.

### Follow Up Boss evidence

Reverified 2026-09-06 against official public documentation: FUB creates lists
from People filters, requires an explicit update to persist edits, and evaluates
criteria dynamically; sharing a list does not grant contact visibility.
[Smart Lists Overview](https://help.followupboss.com/hc/en-us/articles/1500008374882-Smart-Lists-Overview).
Administrators manage shared lists; duplication produces an independent copy;
deletion removes the definition, retaining contacts.
[Manage Smart Lists](https://help.followupboss.com/hc/en-us/articles/360012323974-Manage-Smart-Lists).
Our creator-only privacy and numeric caps are explicit D-046 decisions, not
claims of complete FUB parity; FUB's overview says it enforces no list limit.

## 2. Ownership, permissions and limits

Every request derives Organization, actor and active membership from the
server. The request never accepts `organization_id`, owner, creator, role,
permissions or a resolved value for `me`. `scope` is exactly `personal` or
`shared`, chosen on creation and immutable thereafter. A personal list's owner
is its `created_by_user_id`; a shared list remains administratively manageable
after its creator is demoted or deactivated.

| Caller in the active Organization | Own personal | Someone else's personal | Shared |
|---|---|---|---|
| Active member | Read/create/update/delete/duplicate | Invisible, including name and criteria | Read/duplicate to personal |
| Active Organization admin | Same personal rights | Invisible; no admin exception | Read/create/update/delete/duplicate |
| Platform admin without active Organization membership | No tenant access | No tenant access | No tenant access |

An admin can Save as/Duplicate into either scope; a member can create only
personal copies. Copying to shared is an explicit scope choice in the dialog,
never an implicit publication of a personal definition. No share toggle.
The name and criteria of someone else's personal list remain hidden through
the index, direct reads, counts, writes, error bodies, logs and realtime.
This does not narrow Organization-wide Person visibility.

D-046 limits are **200 live shared lists per Organization plus 50 live personal
lists per owner, with no combined cap**. Deleted definitions do not consume
either quota. Other owners' personal lists do not affect the caller's quota or
index. At most 250 visible definitions exist for one actor. Deactivation retains
the creator's lists, without making them readable by another person; reactivation
restores access. Departure/transfer policy O-004 is not resolved by this slice.

Names: trim surrounding whitespace, then require 1–80 Unicode scalar values
and no control characters. Preserve interior whitespace and case. Duplicate
names are permitted: use IDs, not names, as identity. Render names as text.

## 3. Persistence and concurrency

One additive migration introduces `saved_list`; no backfill or seeds. Use the
repository's next migration timestamp, later than `20260904000001`. This is
normal relational CRUD, not a history fact or event store.

| Column | Type / constraint |
|---|---|
| `id` | UUID primary key, server-generated `gen_random_uuid()` |
| `organization_id` | UUID not null, FK to Organization |
| `created_by_user_id` | UUID not null; composite FK `(organization_id, created_by_user_id)` to `organization_membership(organization_id,user_id)` |
| `scope` | TEXT not null, check `personal` / `shared` |
| `name` | TEXT, not null for a live row, null for a deleted row; live length 1–80 |
| `filter` | JSONB, not null for a live row, null for a deleted row; live value must be an object |
| `revision` | BIGINT not null, initially 1, check positive |
| `create_request_id` | UUID not null, caller-generated retry token |
| `create_fingerprint` | BYTEA not null, 32-byte SHA-256 digest defined below |
| `created_at`, `updated_at` | TIMESTAMPTZ not null, DB-generated |
| `deleted_at` | TIMESTAMPTZ nullable |

Add unique `(organization_id, created_by_user_id, create_request_id)` and
partial indexes for live rows: `(organization_id, scope, created_at, id)` and
`(organization_id, created_by_user_id, created_at, id)` with
`scope = 'personal'`. A check enforces the live/deleted nullability pair.
Grant `crm_app` SELECT, INSERT, UPDATE only; no DELETE or TRUNCATE grant.
The application validates typed filters; JSONB's object check does not replace
that validation. Existing stage, Person, membership and history schemas do
not change. New SQL uses static `query!`/`query_as!` and generated `.sqlx`
metadata. Existing summary SQL/cache entries remain unchanged.

Deletion clears `name` and `filter`, increments revision once and sets
`deleted_at`/`updated_at`. A minimal internal tombstone retains the identifiers,
scope, creation digest and timestamps to prevent a delayed create retry from
resurrecting a deleted definition. It is never returned by the normal reads,
counted against a quota or converted into a Person history fact. No automatic
purge/reuse of retry tokens in this rung; later erasure policy must preserve
the no-resurrection guarantee or explicitly replace it.

### Transaction rules

All three mutation commands read the current membership row for the trusted
actor inside their transaction, holding `FOR SHARE` through commit so a
concurrent role/status change cannot interleave with authorization. Inactive
or missing membership is unauthenticated. Do not add a role to the shared
`CommandContext`; the saved-list commands own this database check.

Use one transaction-scoped advisory lock per Organization in a new distinct
`saved-lists:` namespace for saved-list mutations. Order: membership read/lock,
then advisory lock, then saved-list row access. The bounded, infrequent CRUD
does not need finer locking. Every command uses the same order; one writer's
cap count and insert are atomic against another. Reads/count evaluation take
no mutation lock. A quota race cannot admit list 201/51.

Update/Delete require `expected_revision`; compare inside the transaction.
There is no force-save endpoint or last-write-wins overwrite. A changed update
increments revision once; an equal name+typed filter at the current revision
is a successful no-op without changing timestamps/revision. A stale revision
always conflicts, even if current data happens to equal the requested data.

### Creation idempotency

Create/Save as/Duplicate share `CreateSavedList`. The browser generates and
retains a UUID `request_id` for a single submitted logical create. Its identity
is scoped to trusted Organization **and creator**, independently of resource
IDs, so one user cannot collide with or inspect another user's retry token.

Normalize the name and strictly decode/structurally validate the filter, then
fingerprint deterministic UTF-8 JSON serialization of the typed tuple
`[scope, normalized_name, filter]`. Serialize typed struct fields in fixed
order; preserve clause/value-array order. This is request equality, not
Boolean-equivalence normalization. `request_id` is not part of the digest.
Use existing `sha2`; never retain a second plaintext creation payload.

After current membership/scope authorization, look up the actor-scoped retry
token **before quota checking or reference validation**. Matching digest on a
live row returns its current metadata with `created: false`; updates made
since the original create are not overwritten. A different digest returns
409 `saved_list_request_conflict`. A matching deleted row returns 409
`saved_list_deleted`; neither path creates a new definition. A demoted member
cannot replay a shared create to bypass current permissions. Unknown token:
validate references, enforce the appropriate quota, insert and commit once.
Two simultaneous identical requests return one ID and consume one slot.

## 4. Typed commands, reads and filter evaluation

Add `SavedListId` under the existing typed-ID discipline and a small
`domain/saved_list` module. Public command inputs, alongside server-built
`CommandContext`, are:

```text
CreateSavedList { request_id: Uuid, scope: SavedListScope,
                  name: String, filter: FilterDefinition }
UpdateSavedList { list_id: SavedListId, expected_revision: i64,
                  name: String, filter: FilterDefinition }
DeleteSavedList { list_id: SavedListId, expected_revision: i64 }
```

Commands enforce structure, ownership, current role, revision and references
themselves; routing/UI checks are not authorization. In particular, explicitly
require `filter.version == 1` before `.validate()` in Create/Update: the current
pure validator checks clauses while the wire deserializer enforces version,
and direct Rust callers can construct the public struct with another version.
Reject that input without a write; no shared-validator refactor is necessary.
Duplicate is a new create
of an explicitly named copy of the currently displayed, validated criteria,
not a second mutation mechanism. Native/Operator callers can later use the
same commands; no tools or native UI are added here.

Every saved-list select/update uses literal `organization_id = $org` and the
visibility predicate `(scope = 'shared' OR created_by_user_id = $actor)`.
Write authorization additionally requires creator for personal or current
admin for shared. Invisible, cross-org and nonexistent resource IDs all yield
the same 404 `not_found`; lookup precedes evaluation or shared-write rejection.
UUID equality alone never authorizes a query.

### Read shapes and stale definitions

The index reads metadata only: no filter evaluation and no per-list membership
lookups. Detail and count first resolve a live visible row, then deserialize
its JSONB value through `FilterDefinition` and call `.validate()`. Unknown
version/kind/field or corrupt structural content is `unsupported_filter`.
Never discard an unknown clause or interpret a malformed definition as all
People. A bad row must not poison other index rows.

For a typed definition, use the unchanged Organization-scoped reference
validation and `describe()` with Organization-scoped stage/member names.
Missing and cross-org stage/user references produce `invalid_stage` or
`invalid_assignee`. Inactive membership remains a valid assignee (D-027).
A well-formed stale Source is valid and can have zero matches. Changed stage
or member display names change labels, not saved IDs or revision.

Detail preserves metadata even for an invalid definition. It returns the typed
filter when structurally supported, or null for unsupported content; it never
returns the raw invalid JSON. A reference-invalid typed filter can be repaired
by its writer by replacing/removing the stale clause and saving; an unsupported
definition has no filter editor or Save as action but can still be deleted.
No automatic migration or sanitization of a stored definition.

### Count evaluation

`count_matches` uses `PersonVisibilityScope::from_auth` and the same
`PersonFilterParams`/server-resolved `me` as `filtered_summaries`. Implement one
additional static SQL matrix with the identical predicate semantics, selecting
only Person IDs with `LIMIT 501`, then counting that bounded subquery. Do not
load up to 500 complete summaries for each index row; no dynamic SQL or
alternative evaluator. Ordering is unnecessary for capped cardinality.

Return `count = min(matches, 500)` and `truncated = matches > 500`; display
exact 0–500 or **500+**, never claim a total beyond the cap. Count parity tests
against the existing filtered-summary query are required for every clause
kind and mixed filters, with shared `me`, stale references, 0/500/501 matches
and tenant isolation. The index and opened list may differ after a concurrent
Person edit or clock boundary; they are separately fetched current reads.

Opening a valid list loads its existing People query with the saved filter;
the People endpoint keeps its shape, order and cap. The saved-list page must
disable that query until the visible list definition is successfully loaded.
An intentionally saved empty definition still sends the explicit serialized
`{"version":1,"clauses":[]}` filter through the existing filtered path. Do not
confuse an absent/unreadable saved definition with that valid empty definition;
the ordinary `/people` empty-filter URL normalization remains unchanged.
A saved-definition error must never invoke 011a's ad-hoc URL fallback to all
People. A reference that disappears between detail and People fetch yields a
recoverable invalid-list state, not an unfiltered table.

## 5. HTTP contract

All routes are under the existing authenticated `/api` surface. Request JSON
denies unknown fields and duplicate keys, including nested filter fields;
malformed bodies, UUIDs, revisions or names use the existing error envelope
`{"error":"<code>"}`. UUIDs in request bodies use canonical lowercase
hyphenated representation; `expected_revision` is an integer from 1 through
9,007,199,254,740,991 (also the wire revision ceiling). An increment at the
ceiling fails 503 rather than overflowing. Set a 128 KiB JSON-body cap on these
routes and map its rejection to 400 `malformed_request`.

Metadata is exactly:

```json
{"id":"<uuid>","name":"My stale leads","scope":"personal",
 "revision":1,"created_at":"<RFC3339>","updated_at":"<RFC3339>",
 "can_edit":true,"can_delete":true}
```

Capabilities are server-computed conveniences, not trusted mutation inputs.
No Organization ID, owner name/ID or creation retry token is returned.

| Method/path | Request | Success |
|---|---|---|
| GET `/api/saved-lists` | None | 200 `{"lists":[Metadata,...]}`; live visible rows ordered `created_at ASC, id ASC`; at most 250 by enforced quotas |
| GET `/api/saved-lists/{id}` | None | 200 `{"list":Metadata,"filter":FilterDefinition-or-null,"description":[string,...],"filter_error":null-or-code}` |
| GET `/api/saved-lists/{id}/count?revision=1` | Required positive canonical integer revision | 200 `{"list_id":"<uuid>","revision":1,"count":42,"truncated":false}` |
| POST `/api/saved-lists` | `{"request_id":"<uuid>","scope":"personal","name":"...","filter":FilterDefinition}` | 201 `{"list":Metadata,"created":true}`; exact create retry 200 same envelope with `created:false` and current Metadata |
| PUT `/api/saved-lists/{id}` | `{"expected_revision":1,"name":"...","filter":FilterDefinition}` | 200 `{"list":Metadata,"changed":true-or-false}` |
| DELETE `/api/saved-lists/{id}` | `{"expected_revision":1}` | 200 `{"deleted":true}` |

Detail `filter_error` is one of `unsupported_filter`, `invalid_stage`,
`invalid_assignee`; null means evaluable. `description` is `describe()` for a
structurally valid filter (neutral placeholders for missing references), empty
for unsupported content. DB failure during validation/names loading is 503,
never an invalid-definition label. Count on an invalid definition is 422 with
the same error code, never `count:0`. Count revision mismatch is 409
`saved_list_conflict`; this prevents an old index row receiving a count for a
new definition. Only one `revision` parameter is valid; unrelated query keys
are ignored, matching the existing GET query posture.

### Error precedence and retry semantics

Resource routes retain the existing bare typed path-extractor precedent:
malformed path UUID → 400 before auth. Otherwise 401 auth first, 400
JSON/query/structural validation second, then live visible lookup (404), shared
write authorization (403 `forbidden`), revision (409 `saved_list_conflict`),
reference validation (422 `invalid_stage`/`invalid_assignee`), success. Lookup
does not disclose hidden rows. Infrastructure failure at any required DB
operation is 503 `unavailable`, not a fake 404/422.

Create has no resource lookup: 401 → 400 → current shared-scope permission
403 → actor-scoped idempotency branch → reference validation 422 → quota
409 `saved_list_limit_reached` → insert. Quota errors reveal only the quota
the caller is allowed to use. Name/control validation is structural 400.

For an already-deleted row, DELETE with current deletion authority returns
200 `{"deleted":true}` regardless of the old expected revision, without
changing the tombstone; invisible tombstones remain 404. This is the only
resource mutation that treats a tombstone as an idempotent success. PUT and
normal reads/counts treat it as 404. A live DELETE with stale revision still
conflicts and requires renewed confirmation against the latest definition.

Create retries reuse the exact retained request/payload; retry-token conflict
or deletion is never fixed by silently generating a new token. Update/Delete
do not retry with a freshly fetched revision automatically. On an uncertain
update outcome, refetch; if the intended state is now present, show it as saved,
otherwise retain the draft and require explicit Save against the reviewed
version. A 503 retains inputs; no success message before a server result.
No automatic mutation retries. CORS already allows GET/POST/PUT/DELETE and
Content-Type; no CORS or header-contract change is needed.

## 6. Web flows and state

### Lists index

Add “Lists” beside People in the normal app navigation, available to all active
members, with route `/lists`. Use the D-045 white workspace, restrained glass
controls and existing accessible components. Show two sections, “My lists” and
“Shared lists”, with name, scope, match count and an open-list link. Preserve
server order within each section, concatenate My lists before Shared lists,
then paginate that combined sequence in **25-definition pages**. This bounds
rendered/evaluated rows and keeps personal lists first. No People search or
per-list sort UI.
Show both quota usages from the complete visible metadata, with separate
shared/personal limit messages. “Create a list” leads to `/people?guide=create-list`, where the
agent composes criteria. Empty state explains how to save a filter.
*Polish, 2026-09-06 (user request, after the merge):* that `guide` flag makes
`/people` show a three-step “Creating a list” card with a Dismiss control and
a “Show me” dialog that plays a recorded walkthrough video with native
controls (`web/public/guides/`, regenerable with `pnpm run guide:capture`;
no autoplay under reduced motion). The flag survives filter edits and disappears on navigation.

Fetch metadata once, then counts only for the current 25 rows, with at most
four count requests in flight. This bound applies to **every fetch path**:
initial load, cached-row invalidation, focus, navigation, reconnect, manual
Refresh and retries. Use one small scheduling gate per mounted index with
existing TanStack Query; disable autonomous count refetch/retry paths that
would bypass it. After all 25 rows have populated, `person.changed` must still
queue at most four requests, not launch 25. No new scheduling dependency.
Each row distinguishes
loading, 0, exact count, 500+, invalid definition, and unavailable/retry.
One failed count does not replace the index with an error or hide other rows.
Metadata errors have a full-page retry. Count loading/retries must not refetch
the entire People table. Cancel queued/active work on page, actor, Organization
or revision change; stale responses cannot attach to a different row.

### Create, Save, Save as and Duplicate

1. `/people` gains **Save as list** for any committed filter, including all
   People. Opening the dialog does not commit an incomplete FilterBar draft.
   The dialog shows Name and scope; personal is default, shared is offered
   only to admins. It explains that a shared list is visible Organization-wide.
   Empty criteria explicitly say “All people”; `me` explicitly says it changes
   with the viewer. Cancel does not mutate or navigate.
2. Successful create closes the dialog and opens `/lists/{id}` using the
   returned ID. Repeated click while pending is disabled. Keep the submitted
   frozen request/token after an uncertain network/503 failure for a safe
   “Try again”. While that attempt is pending or unresolved, do not mutate its
   payload or silently generate a new token. Definite validation/quota
   rejection allows editing and a new deliberate submission. Closing an
   uncertain attempt warns “This list may already have been saved” and offers
   opening the refreshed Lists index; the user may explicitly abandon it.
   Do not automatically resubmit after abandonment or reload. Retry identity
   is local component state, not browser storage: closing/reloading loses it,
   so no exactly-once guarantee is claimed across a new deliberate create.
3. `/lists/{id}` shows the saved name/scope and the same FilterBar, People
   table, count and inspector as People. Start only after detail is readable
   and evaluable. It uses a clean named URL; unsaved filter edits remain local
   to this page, with a visible **Unsaved changes** label. The ordinary
   `/people?filter=` contract and its query/hash preservation are unchanged.
4. Filter changes preview immediately but never write a saved definition.
   **Save** writes the complete current name+filter using the loaded revision,
   and is available only to writers. Name changes use a compact rename/edit
   control and participate in the same Save; do not add a second rename route.
   **Reset changes** restores the latest loaded saved definition. All People
   and My People controls, if shown, change only the local working criteria.
5. **Save as** creates a new list from the working criteria under a new name;
   personal is default. **Duplicate** creates an independent copy of the last
   successfully loaded saved criteria, even when a local draft differs, and
   pre-fills “Copy of …” within the name limit. Its dialog labels the source
   as the saved version. After creation both copies can diverge. No linked
   source ID, automatic update propagation or original modification exists.
   When Duplicate succeeds while the original has unsaved changes, remain on
   the original with its draft intact and show a link to the new copy; following
   that link uses the normal dirty-navigation guard. With no dirty draft,
   Duplicate opens the copy normally.
6. A non-admin viewing shared criteria can adjust the preview and Save as or
   Duplicate to personal. It never offers Save/Rename/Delete for the original.
   Agents may remove/add filters to their local preview; this does not grant
   write access. Copy preserves symbolic `me`, never its resolved UUID.
7. Navigating away from a dirty list within the app requires a discard-changes
   confirmation; Cancel stays, Discard leaves. Register a browser unload guard
   while dirty (native browser wording). Successful Save/Save as may navigate
   without that guard after success because they persisted the working draft;
   Duplicate follows the preceding dirty-draft rule. Closing a filter
   editor preserves 011a's independent draft rules.

Named-list routes do not interpret `?filter=` as an override. Ignore unrelated
query parameters and preserve them through incidental router operations; saved
criteria come from the authorized detail endpoint. A copied named URL opens the
latest saved definition, subject to the recipient's authorization. On `/people`,
the existing shareable filter URL still means an ad-hoc filter.

### Delete, invalid data and concurrency

Delete requires an explicit dialog naming the list and explaining that People
are retained; shared deletion also explains that everyone loses this list.
Submit its loaded revision. Success returns to `/lists`; network failure retains
the dialog and allows the same retry. A conflict reloads the metadata and asks
the user to review/reconfirm; never re-delete with a new revision automatically.

A missing/hidden/deleted named link shows “List not available” with a Lists
link; no unfiltered People. An unsupported filter displays a clear unavailable
state and Delete for an authorized writer. A stale-reference filter displays
editable invalid chips for the writer and an explanation to fix them; People
evaluation stays disabled until the working criteria are valid. Reader-only
shared views show the error with no admin mutation controls. A 503 retains
the current draft and offers Retry; it never clears criteria.

On a 409 Save conflict, keep the working draft and show **Reload saved version**
(explicitly discards the draft) and **Save as a new list**. A clean page can
adopt a newly fetched revision; a dirty page never silently rebases its baseline
or replaces its draft on background refresh. Keep baseline revision, working
criteria and latest-server metadata distinct. A 404/401 or lost write permission
disables mutations and removes inaccessible content; drafts never cross actors
or Organizations. Switching lists/filter/Organization closes the Person
inspector; retained rows from a superseded filter stay labeled and inert per
011a. Controls have labels, keyboard operation, focus restoration and readable
loading/error states; test narrow layouts with the inspector open.

## 7. Cache, refresh, privacy and operations

Extend the central key factory, never hand-write keys:

```text
savedLists(org, actor) = ['org', org, 'saved-lists', actor]
savedList(org, actor, id) = [...savedLists(org, actor), 'detail', id]
savedListCounts(org) = ['org', org, 'saved-list-counts']
savedListCountsForActor(org, actor) = [...savedListCounts(org), actor]
savedListCount(org, actor, id, revision)
  = [...savedListCountsForActor(org, actor), id, revision]
```

Build requests from query-key values and consume cancellation signals. Never
reuse metadata/count placeholder data from another actor or Organization.
Existing People keys stay byte-identical. Reuse `usePeople` with a small
explicit enabled input/list mode so it cannot issue an unfiltered request while
loading a list. Its existing `/people` callers keep their current behavior.

Create/update/delete invalidate this actor's saved-list and count prefixes.
Refetch visible
metadata on every list-page entry and window focus, regardless of the global
30-second staleTime; a manual Refresh button refetches metadata and current-page
counts. Existing reconnect invalidation of `queryKeys.org(org)` covers the new
keys. Extend the **client invalidation mapping only** so any existing
`person.changed` also invalidates `savedListCounts(org)`. All active-count
refetches, including those marked stale by this event or the reconnect sweep,
pass through §6's four-request scheduling gate; inactive rows stay stale until
their page is shown.
Do not publish saved-list IDs, criteria, names or actor identifiers to the
Organization realtime channel. No new realtime wire contract is introduced.

List changes in another browser and time-only age transitions become visible
on navigation, focus, reconnect or manual Refresh; there is no promise of live
counting while an idle tab stays focused. Metadata refresh invalidates/removes
obsolete count revisions and all cached detail/counts for removed IDs. An active
detail view refetches the detail itself, not only index metadata, on those same
triggers. After revalidating readable list metadata, explicitly refetch the
current working People query regardless of its existing 30-second staleTime
or unchanged filter/revision; refreshing detail alone would leave time-relative
matches stale. Dirty drafts and their baseline revision remain intact, and a
missing/invisible detail disables the People query instead of displaying its
cached rows. Force current-page count refresh through the same scheduler even
when metadata/revisions are unchanged. Ordinary `/people` retains its existing
refresh policy. Log out/session changes clear cache as today, with explicit same-org
different-actor regression tests. Counters are current query results, not
membership history or proof that a particular person is new to a list.

Use existing tracing, `skip_all` on commands, and safe IDs/correlation,
scope, outcome/error kind and timings. Never log names, filter JSON/values,
descriptions, create tokens/fingerprints, SQL parameter/debug errors or People
content. Existing path-only request spans stay; pin their query-string
exclusion plus the positive static `filter_kinds` field on evaluation. No
new audit fact is needed for this reversible configuration CRUD.

Performance: metadata is one bounded read without membership evaluation;
only 25 displayed definitions have count work, at most four HTTP requests at
once, each selecting at most 501 IDs. Predicate work can still scan an
Organization for sparse matches, as in 011a. Record representative 50k-Person
`EXPLAIN (ANALYZE, BUFFERS)` results for a dense and sparse count alongside the
existing filter query; compare the shape with [PERF_BASELINE](../design/PERF_BASELINE.md)
(notably sparse/absence predicates and the ten-connection pool). Use a targeted
throwaway fixture, not a full shared-dataset reseed or wipe. Investigate a
material regression before completion.
No speculative index, cache or denormalized activity fields without evidence.

## 8. Declared contract changes and amendments

This specification declares these approved changes under AGENTS.md §11; approval of
the concrete spec is the implementation gate. The single implementation lane
owns the new contracts; it must not reinterpret existing ones silently.

| Current → proposed | Why / affected components | Compatibility, migration and amendment owner |
|---|---|---|
| No saved-list persistence → §3 `saved_list` with creator privacy, caps, revisions and retry tombstones | Persist criteria safely; crm-app, crm-api, PostgreSQL | One additive migration, no backfill; old binary ignores table; do not drop it on rollback. This spec/brief own it. |
| No saved-list HTTP/commands → §4/§5 typed commands and six routes | One authorized mutation path, safe writes/copies/count reads; API/Web/future clients | Additive endpoints, stable existing error envelope; no change to People response or filters. Add a pointer in SLICE_002 §5 and §6 to this spec's new surface, not a second full contract. |
| 011a has one filtered summary query and no count surface → an additional capped ID-only static projection | Index counts without returning 500 People per row; crm-app/SQLx/Web | Filter vocabulary and semantics unchanged. Add amendment pointer in SLICE_011a §2/§4e/§6 naming this count projection and saved-list wrapper; existing SQL/cache unchanged. |
| `/people` ad-hoc workspace only → `/lists` index and `/lists/:id` wrapper around same workspace | First-class lists; web routes/navigation/query keys | `/people?filter=` fallback and history behavior unchanged; saved-list failures fail closed. D-045 and amended 011a UI remain binding. |
| Client `person.changed` invalidates People/Today → also invalidates saved-list counts | Counts recover after relevant Person facts; Web realtime handler | Wire event unchanged, no server list publication. Add SLICE_003 §6 client-mapping amendment pointer. |

The coordinator owns shared spec-pointer/ladder/state edits; the implementation
lane supplies the exact pointers for integration. The ladder includes the sort
follow-up immediately after 011b, as required by the accepted 2026-08-29 decision.
No unresolved privacy/cap decision remains after D-046. The user approved
this specification after the independent READY review on 2026-09-06.

## 9. Acceptance criteria and evidence

Each numbered item is required; new test names may follow repository conventions.

1. **Migration/grants:** a fresh migration has the declared constraints, keys,
   live/tombstone invariants and crm_app grants; invalid FK/scope/length/null
   combinations fail; app-role hard deletion fails. DB tests + prepare-check.
2. **Creation and replay:** create both allowed scopes; preserve symbolic `me`
   and v1 JSON; empty filter is valid; same request retry/concurrent duplicate
   returns one ID/one slot even at quota; changed payload/token conflicts;
   same token in another actor/org does not collide. DB/route tests.
3. **Retry after edit/delete:** original create retry returns current metadata
   without reverting an edit; deleted replay cannot resurrect; tombstone name
   and filter are null. Delete retry succeeds only with current deletion
   authority; no repeated revision increment. DB tests.
4. **Privacy/tenant isolation:** index/detail/count/PUT/DELETE for someone else's
   personal list are 404 for members AND admins; shared is readable org-wide,
   member writes are 403, platform-only is 401; cross-org ≡ random missing ID
   bodies; direct typed-command calls enforce the same policy. DB/API tests.
5. **Quotas under contention:** 50 personal and 200 shared live rows accepted,
   the next rejected; synchronized final-slot creates cannot exceed caps;
   deletion frees only its scope's slot; other owners' personal rows never
   consume this owner's quota; 200 shared + 50 own personal coexist. DB tests.
6. **Revisions/authorization races:** two same-revision updates have one winner;
   update/delete race cannot lose an update silently; equal update is a no-op;
   stale update conflicts; demotion/deactivation cannot authorize a later
   shared mutation; creator deactivation preserves shared admin continuity and
   personal privacy. DB tests with deliberate barriers/locks, not sleeps.
7. **Validation/error precedence:** strict new envelopes, nested duplicate and
   unknown fields, request/body limits, malformed UUID/revision/name, all
   400/401/403/404/409/422/503 ordering and codes in §5; DB outages never become
   invalid definitions. Direct Create/Update command calls with a manually
   constructed unsupported `FilterDefinition.version` must reject without a
   write, independently of the HTTP deserializer. Service-free extractor
   tests + DB route/command tests.
8. **Stored-filter failure:** inject unsupported version/kind/field, bad
   structure and stale/cross-org references; index still loads, detail returns
   bounded error shape, count never reports zero or all People; stale Source
   and inactive assignee remain valid; repair writes valid criteria only.
   DB tests, plus web invalid-list states.
9. **Membership parity:** count vs filtered summaries for each existing axis,
   combinations, latest source, nullable-age complements, 0/500/501+, and the
   same shared `me` list viewed by two agents; no cross-org matches. Existing
   011a tests + new DB parity tests; no new count-query semantics.
10. **Create/copy flows:** browser tests prove valid committed criteria only,
    explicit scope, copy independence, Duplicate uses saved baseline whereas
    Save as uses draft, symbolic `me`, pending/double-click protection,
    same-token retry, frozen uncertain payload, abandon/reload warning without
    automatic resubmission, quota errors, and no Person mutation.
11. **Edit/delete conflict flows:** retain drafts on error/409/background update,
    no auto-force-save/delete, explicit reload/discard and delete confirmation,
    correct dirty navigation guards, no unsupported-filter editor and no
    shared write controls for members. A successful Duplicate from a dirty
    original preserves its draft and guards navigation to the copy.
    Vitest component/router tests.
12. **Existing workspace regression:** all current 011a FilterBar/PeopleView
    tests, shareable `/people` URL/history/rapid-clear/hash behavior and D-045
    inspector continue to pass. Named-list invalid/missing/error states never
    issue an unfiltered fetch or degrade to all People; list switch cancels
    stale work and closes inspector. Vitest integration tests.
13. **Counts/cache/recovery:** 25-row page/4-request bounds; metadata fetched
    without counts; independent row retries; stale responses cannot cross
    actor/org/id/revision; accurate 0/500+ labels; person.changed, focus,
    navigation, manual refresh and reconnect recover counts/detail/working
    People. After populating 25 count rows, invalidate them and prove every
    refresh/retry path still respects four in flight. Cross an age cutoff with
    unchanged definition and focus within 30 seconds: People/counts refresh
    despite fresh cache, dirty draft unchanged. No list mutation publication.
    Vitest + publisher-spy/API tests.
14. **Telemetry/performance:** sentinel list names/criteria/tokens absent from
    captured logs and spans; static filter kinds present, query stripped;
    dense/sparse 50k count plans and bounded scheduling recorded. Targeted
    telemetry tests + measured query-plan evidence, no invented timing claims.
15. **Full checks:** run `./scripts/check` and then `./scripts/check-db` on the
    final implementation tree; regenerate SQLx metadata and report existing
    summary entries unchanged. Never overlap DB-backed runs in this checkout.
16. **Live walkthrough:** synthetic fixtures, two agents plus admin and a
    second Organization: save/reopen dynamic criteria; edit People to change
    membership; demonstrate shared `me`, owner-only privacy including admin;
    copy/edit/delete without losing People; two-tab conflict; offline/retry;
    unknown/missing list; keyboard/narrow layout/inspector. Record exact
    scenarios and outcomes; do not substitute API-only checks for browser QA.

## 10. Delivery boundary and review gate

One implementation lane owns the database/backend/Web change; coordination
cost outweighs parallel writing through shared query/types/PeopleView files.
This is M, with a predeclared split seam if review or implementation evidence
requires it: **011b1** persistence/commands/routes/counts plus tests, then
**011b2** index/save flows plus Web/live tests. The complete user-visible saved
lists rung is not marked done until both halves land; splitting requires an
updated brief, not silent scope removal. No later-rung work fills spare time.

Review dispositions (2026-09-06): independent reviewer SL011B-A1 (direct-command
filter version validation) and A2 (force working-People refresh for unchanged
criteria) are folded into §§4/7/9. Coordinator R1 (Duplicate preserving a dirty
draft), R2 (uncertain-create abandonment/reload), and R3 (bounded counts across
every refetch path) are folded into §§6/7/9. A separate count-key branch,
explicit empty-filter serialization and personal-first index pagination remove
the corresponding implementation ambiguities. Focused independent re-review
returned **READY** on 2026-09-06: all five findings closed, no remaining
blocking finding or new human decision. This is readiness, not user approval.

The independent [04-review-plan](../prompts/04-review-plan.md) gate is complete.
The user approved this specification on 2026-09-06 after reading its
plain-language companion. Terra / ultra implementation, tests and fixes are
authorized, followed by independent Astra / ultra verification. After completed
verification, the user explicitly approved local commit and merge on 2026-09-06.
Push and deployment remain outside that authorization.
