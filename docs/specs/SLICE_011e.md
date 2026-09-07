# Slice 011e — Tags

**Status: APPROVED by the user on 2026-09-07 after independent review and
D-051.** Approval covers the declared contracts (§7) and the §1 safe
defaults; it authorizes implementation of rung e1 after the Phase 6 gate, not
commit, merge, push or deployment. Prepared against local `main` at `af5e9d8`
(011d merged, pushed and serving on the shared development runtime).
Independent review on 2026-09-07 returned READY WITH CORRECTIONS; every
correction is applied below (the load-bearing one: fourteen statements bind
the filter parameters on `main`, not eleven). Coordination, specification
and review in this session; implementation per the
[implementation brief](../tasks/SLICE_011e_IMPL.md). Plain-language
companion: [SLICE_011e_EXPLAINED.md](SLICE_011e_EXPLAINED.md).

Free-form **tags** on People ("investor", "past client", "sphere"), created
inline by any member, applied and removed on the Person page, and usable as
two new filter clauses (`tags`, `not_tags`) everywhere the v1 filter
vocabulary is accepted: the People page, saved lists, list Today sources and
the admin-edited system feeds. This is the last rung of the Slice 011 ladder
and its declared purpose is to prove the vocabulary grows a new clause family
without a rebuild: the same additive extension pattern 011d used for its three
derived kinds, applied to a new tenant-owned entity.

Authority: [D-004, D-005, D-007, D-008, D-019, D-023, D-043, D-045, D-046,
D-047, D-050, D-051](../decisions/DECISION_LOG.md), the
[accepted ladder](../plans/SLICE_011_LADDER.md) (011e row and decision 1:
tags depend only on 011b), the
[architecture baseline](../architecture/ARCHITECTURE_BASELINE.md), AGENTS §4.6
(tags are relational CRUD, not history), [011a](SLICE_011a.md) §4,
[011b](SLICE_011b.md) §§2–5, [011c](SLICE_011c.md) §§3/5,
[011d](SLICE_011d.md) §2, [002](SLICE_002.md) §5, [003](SLICE_003.md) §6,
[005](SLICE_005.md) §§3/5 and [UI_STYLE](../design/UI_STYLE.md).

## 1. Scope and product behavior

In scope, delivered as two independently landable rungs (§10):

- **e1 — the tag model and Person tagging.** `tag` and `person_tag` tables;
  typed commands to create a tag inline, apply and remove it on a Person,
  and rename or delete it under rule 1 (admins, or the creator while
  unused); six additive routes; `tags` on the Person detail read (HTTP and
  Operator); a `tags_changed` Person change on the existing realtime event;
  monochrome chips with add/remove on the Person page, read-only chips in
  the People preview; a small **Tags** page under Manage for rename and
  delete, open to every member with the controls live only where rule 1
  allows.
- **e2 — the vocabulary extension.** Two clause kinds, `tags` (any of) and
  `not_tags` (none of), validated like `stage_ids`, described with tag
  names, evaluated by NULL-guarded predicates appended to all fourteen static
  statements that bind the filter parameters (011d's own §2 said eleven and
  then added three feed statements in its §5), with the `invalid_tag`
  reference error flowing through saved
  lists, list sources and system feeds exactly as `invalid_stage` does; two
  FilterBar chips.

Out of scope: importing Follow Up Boss tags (parked Slice 010; this rung
builds the destination model 010f needs and nothing more); tag colours or
icons; tag categories, descriptions or hierarchy; tags on the People table
rows or on the Operator's search results; Operator `add_tag`/`remove_tag`
tools (a later S rung: mutation tools need §5.4 risk classification and the
proposal flow); bulk tagging; "tagged A **and** B" (§4, deferred with its
additive path recorded); merging two tags; a tag history or fact table
(AGENTS §4.6); realtime push of rename/delete; mobile.

Product rules (rule 1 is the accepted decision D-051; the rest are safe
defaults adopted at specification, veto-able by the user):

1. **Any active member creates, applies and removes tags. A tag is renamed
   or deleted by an Organization admin, or by its creator while no Person
   carries it** (D-051, user, 2026-09-07). So a member who mistypes a new
   tag fixes or deletes it themselves before anyone applies it; once a tag
   is in use, changing or removing the definition is an admin action.
   "Creator" is the `created_by_user_id` on the row; "unused" is zero
   `person_tag` rows at the moment of the command, checked under the tag's
   row lock. Platform admins have no tenant access and cannot.
2. **Tag names are unique per Organization, case-insensitively, and the
   first spelling is kept.** Creating "Investor" when "investor" exists
   returns the existing tag (`created: false`); it never 409s. Names are
   trimmed, 1–40 characters, no control characters (the saved-list name
   rule with a shorter cap because tags render as chips).
3. **Limits:** 200 tags per Organization; 20 tags per Person. Both are
   command-enforced 409s, not schema CHECKs. Within D-050's envelope a team
   of 50 runs dozens of tags, not hundreds; past 20 tags a Person's tag is
   a note.
4. **Deleting a tag is a hard delete.** Its `person_tag` rows go with it,
   and any saved list, list source or system feed whose definition names the
   deleted id reports `invalid_tag` through the existing `filter_error`
   paths (011b §4 detail-preserved, count 422; 011c source issue with
   partial availability; 011d canonical fallback). Delete is **not** blocked
   while referenced: D-046 hides personal lists from admins, so a block
   could be neither shown nor repaired by the admin, and would leak that a
   private list exists. The delete confirmation names the consequence.
5. **Applying or removing a tag is target-state idempotent**: re-applying
   an applied tag or removing an absent one is 200 `changed: false`, the
   assignment/stage convention.
6. **Tags appear on the Person detail read, not on People rows.**
   `PersonSummary` is the People row, every mutation receipt and the
   Operator card base; widening it touches the projection of all eleven
   statements. The People preview already reads the detail query and shows
   tags with no new request.
7. **No revision on tags.** Rename is last-write-wins on a label;
   `expected_revision` can be added additively if it is ever missed.

## 2. Persistence (e1; one additive migration, backend-owned)

`crm-api/migrations/20260909000001_tag.sql` (the 20260908 stamp is taken by
011d):

```sql
CREATE TABLE tag (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    name TEXT NOT NULL
        CHECK (char_length(name) BETWEEN 1 AND 40 AND name = btrim(name)),
    created_by_user_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Composite-FK anchor (stage / person convention).
    UNIQUE (id, organization_id),
    FOREIGN KEY (organization_id, created_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
-- Case-insensitive uniqueness per Organization; ON CONFLICT infers it.
CREATE UNIQUE INDEX tag_org_lower_name_key ON tag (organization_id, lower(name));

CREATE TABLE person_tag (
    organization_id UUID NOT NULL,
    person_id UUID NOT NULL,
    tag_id UUID NOT NULL,
    added_by_user_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, person_id, tag_id),
    -- A row for a Person or tag of another Organization can never be
    -- persisted, even if an application check regresses.
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id, organization_id)
        REFERENCES tag (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, added_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
-- Usage counts, delete, and a tag-led probe.
CREATE INDEX person_tag_org_tag_person_idx ON person_tag (organization_id, tag_id, person_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON tag TO crm_app;
GRANT SELECT, INSERT, DELETE ON person_tag TO crm_app;
```

- The `person_tag` primary key serves the detail read and the filter's
  correlated `EXISTS (… pt.person_id = p.id …)`; the second index serves
  counts and deletion.
- `tag` has no tombstone: a tag carries no retry-identity problem (create is
  create-or-get by name, §3) and no quota that a resurrected row could
  bypass. A second delete of the same id is 404, stated.
- No `person.updated_at` bump, no fact table, no `today_feed_changed` or
  other history row: AGENTS §4.6 lists tags under relational CRUD.
- `db_schema.rs` table, grant and index enumerations gain both tables.
- `TagId` newtype in `ids.rs` (the `SavedListId` pattern; canonical-uuid
  discipline on the wire).

## 3. Typed commands (e1; new `crm-app/src/domain/tag/`)

All commands run inside one transaction, take `CommandContext`, use the
Organization from the session only, and follow the 011b error conventions
(DB failure → `unavailable`, never a fake 404/422).

| Command | Caller | Semantics |
|---|---|---|
| `CreateTag { name }` | any active member | **Create-or-get.** Trim and validate (rule 2; violation → `malformed_request`). Take the per-Organization advisory transaction lock `tags:<org>` (the `saved-lists:` pattern; `RenameTag` and `DeleteTag` take the same lock, so a rename cannot race a create into a duplicate). Select by `lower(name) = lower($1)` in SQL; hit → return it with `created: false`. Miss → count live tags; `>= 200` → `TagLimitReached`; else insert, `created: true`. The lock, not `ON CONFLICT`, removes the race so two members typing the same new tag get one row and neither sees an error. |
| `AddPersonTag { person_id, tag_id }` | any active member | `lock_person` in the active Organization (`FOR UPDATE`; not visible → `NotFound`); load the tag `WHERE id = $1 AND organization_id = $2` **`FOR SHARE`** (→ `NotFound`; the share lock makes a concurrent `DeleteTag` wait behind this command instead of turning the insert into an FK failure); count the Person's tags, `>= 20` and not already applied → `PersonTagLimitReached` (the person row lock serialises count-then-insert); `INSERT … ON CONFLICT DO NOTHING`; publish `person.changed{tags_changed}` **only when a row was inserted**; return `(tags, changed)` where `tags` is the Person's full ordered tag list. |
| `RemovePersonTag { person_id, tag_id }` | any active member | Same lookups, locks and 404s; `DELETE` the row; publish only when a row was deleted; return `(tags, changed)`. |
| `RenameTag { tag_id, name }` | admin, or creator while unused (rule 1) | `tags:<org>` lock; validate; load the tag `WHERE id = $1 AND organization_id = $2` **`FOR UPDATE`** (→ `NotFound`; the update lock waits for any in-flight `AddPersonTag` holding the row `FOR SHARE`, so the usage count read next is exact); membership `FOR SHARE` re-read requiring `status = 'active'` (the 011d command posture); **permission:** `Role::Admin`, or `created_by_user_id = actor` and `count(person_tag) = 0` → else `Forbidden`; name byte-equal to the current name after trim → `changed: false`, nothing written (a case-only rename of the same tag is a real update); another live tag matches `lower(name) = lower($1)` with `id <> $tag_id` → `TagNameTaken`; else update `name`, `updated_at`. Case-insensitive comparison runs in SQL against the index expression, never via Rust lowercasing. |
| `DeleteTag { tag_id }` | admin, or creator while unused (rule 1) | Same lock, load, membership re-read and permission check; explicit Organization-scoped `DELETE FROM person_tag` (returns `removed_from_people`, necessarily 0 on the creator path), then `DELETE FROM tag`; the FK cascade stays as belt. No referencing definition is rewritten (rule 4). Not found → `NotFound`. |

Reads (`domain/tag/queries.rs`): `list_for_organization` (id, name,
`person_count`, `created_by_user_id`, ordered `lower(name), id`; the route
derives the viewer's `can_manage` from it), `list_for_person` (ordered the
same), `exists(conn, id, org)` (the `stage::exists` twin used by §4), and
`names_for(ids)` for `FilterNames`.

Neither add nor remove is a history fact; the actor is kept as
`added_by_user_id` on the live row only. The Organization boundary is the
literal predicate in every statement, never a parameter the client supplies.

## 4. Vocabulary extension (e2; amends 011a §4, follows 011d §2)

Two clause kinds join `Clause`, both value-array clauses like `stage`, one
per kind, counted against the 20-clause cap:

```json
{"kind": "tags",     "tag_ids": ["<uuid>", "..."]}
{"kind": "not_tags", "tag_ids": ["<uuid>", "..."]}
```

| Kind | True when | Notes |
|---|---|---|
| `tags` | The Person carries **at least one** of the listed tags (`EXISTS`). | Values are OR, like every value array in the vocabulary. |
| `not_tags` | The Person carries **none** of the listed tags (`NOT EXISTS`). An untagged Person matches. | Exact complement of `tags` with the same ids. |

- **Decode:** the canonical-uuid pre-check that guards `stage_ids` applies
  to `tag_ids` for both kinds; `deny_unknown_fields`, duplicate-key and
  unknown-kind fail-closed behaviour is inherited. Older binaries reject
  the new kinds as unknown, which is the ladder's designed posture.
- **Structural validation (400 `malformed_request`):** non-empty, ≤ 50
  values, no duplicate values, at most one clause per kind. The `stage` arm
  verbatim.
- **Reference validation (422 `invalid_tag`, non-leaking):** every id must
  exist in the active Organization via `tag::exists`; nonexistent and
  cross-Organization ids produce byte-identical responses. Both
  `validate_references` and the deadline-armed
  `validate_references_until` gain the arm. `FilterError::InvalidTag` maps
  to `ApiError::InvalidTag`, joins the `SavedListError` and system-feed
  error mappings, and the system-feed loader treats it as
  `InvalidDefinition` (canonical fallback), the deleted-stage path.
  **Every site that names the closed code set gains an explicit
  `InvalidTag` arm**, in particular the two wildcard arms in the Today
  source-issue mapping (the `outcome` span field and the
  `TodaySourceIssueError` conversion) that would otherwise silently label
  the new code `unsupported_filter`, which the Web treats as unrepairable.
  The brief enumerates the sites: `SavedListFilterError` and its string
  form, `TodaySourceIssueError`, `SourceEvaluation`, the source
  `filter_error()` accessor, and the Web `SavedListFilterError` /
  `TodaySourceIssueError` unions (`TodayFeedFilterError` is an alias of the
  first).
- **`describe()`:** `FilterNames` gains `tag_names: HashMap<TagId, String>`.
  Lines: "Tagged Investor or Past client", "Not tagged Investor"; an
  unresolvable id renders "an unknown tag", never the uuid. Both existing
  name-loading sites (saved-list detail, system-feed reads) load tag names.
- **Parameters:** `PersonFilterParams` gains `tag_ids_any: Option<Vec<Uuid>>`
  and `tag_ids_none: Option<Vec<Uuid>>`; `to_query_params` binds them;
  `kinds_field` emits `tags`/`not_tags`.
- **Statements:** the same two NULL-guarded predicates are appended, in the
  same position, to **all fourteen** statements that bind the parameter
  set: `filtered_summaries` and its seven sorted copies,
  `count_filtered_matches`, `source_membership`, `source_candidates`, and
  011d's three system-feed statements `person_state.sql` (which carries the
  chain **twice**, once per person-state feed), `call_membership.sql` and
  `call_only.sql`. The insertion point is directly after the three 011d
  predicates and before each statement's own tail predicate (for example the
  `p.id = ANY(...)` candidate restriction in `source_membership.sql`).
  Bindings are positional, so a statement left untouched would compile and
  silently ignore the clause; the parity and feed tests in §9 exist to make
  that impossible to miss:

  ```sql
  AND ($A::uuid[] IS NULL OR EXISTS (
        SELECT 1 FROM person_tag pt
        WHERE pt.organization_id = p.organization_id
          AND pt.person_id = p.id AND pt.tag_id = ANY($A)))
  AND ($B::uuid[] IS NULL OR NOT EXISTS (
        SELECT 1 FROM person_tag pt2
        WHERE pt2.organization_id = p.organization_id
          AND pt2.person_id = p.id AND pt2.tag_id = ANY($B)))
  ```

  Absent clauses bind NULL and keep every statement byte-identical in
  results, pinned by parity tests per axis across People, count, every sort,
  both Today source statements and the three feed statements. No dynamic
  SQL; the Organization predicate stays literal; `.sqlx` metadata is
  regenerated for the fourteen statements and `list_summaries` is untouched.
- **Limitation, stated:** "tagged A **and** B" is not expressible: one
  clause per kind, values OR. `tags` plus `not_tags` together is
  expressible ("A or B, and not C"). The additive path, if ever wanted, is
  an optional `"match": "any" | "all"` field on `tags` defaulting to `any`
  so stored definitions decode unchanged. Not built here.

Saved lists, list sources and system feeds accept the kinds with no further
change; a list using `tags` as a Today source admits matching People with
the list reason (011c), and an admin may add a `tags` or `not_tags` clause to
a system feed alongside its locked anchor (011d rule 4).

## 5. HTTP, realtime, Operator and Web

### Routes (e1; additive; SLICE_002 §5 gains rows by amendment pointer)

All routes require an authenticated session with an active Organization
(`AuthContext`); platform-only sessions are 401 as on every tenant route.
Rename and delete are member routes whose **command** decides permission
under rule 1 (admin, or creator while unused); there is no route-level
admin extractor, because a member may legitimately reach them. Path ids
follow the existing typed path extractors. Bodies observe the house 128 KiB
cap; malformed JSON is 400 `malformed_request`. **Error precedence** (the
011b §5 / 011d §6 house order): malformed path uuid 400 before
authentication (typed-extractor precedent) → 401 → 400 body → 404 → 403
(the tag must exist before permission can be judged; tag ids are visible to
every member through `GET /api/tags`, so 404-before-403 leaks nothing) →
409 → 503. `POST /api/tags` has no path id: 401 → 400 → 409 → 201/200.

`Tag` in responses is `{"id","name","person_count","can_manage"}`;
`can_manage` is the server's rule-1 verdict for the **viewer** at read time
(admin, or creator while `person_count` is 0). It is a display hint; the
command re-decides under the row lock.

| Route | Auth | Success | Errors |
|---|---|---|---|
| `GET /api/tags` | member | 200 `{"tags":[Tag]}` ordered `lower(name), id`; unpaginated (≤ 200) | 401, 503 |
| `POST /api/tags` `{"name"}` | member | 201 `{"tag":Tag,"created":true}` (`can_manage` true for the creator); existing → 200 with `"created":false` and the existing tag | 400, 409 `tag_limit_reached` |
| `PUT /api/tags/{tag_id}` `{"name"}` | member; rule 1 in the command | 200 `{"tag":Tag,"changed":bool}` | 400, 403 `forbidden`, 404 `not_found`, 409 `tag_name_taken` |
| `DELETE /api/tags/{tag_id}` | member; rule 1 in the command | 200 `{"deleted":true,"removed_from_people":n}` | 403 `forbidden`, 404 (also on a repeat) |
| `PUT /api/people/{person_id}/tags/{tag_id}` | member | 200 `{"tags":[{"id","name"}],"changed":bool}` | 404 `not_found` (person or tag, identical for other Organizations' ids), 409 `person_tag_limit_reached` |
| `DELETE /api/people/{person_id}/tags/{tag_id}` | member | same shape | 404 |

`GET /api/people/{id}` gains a top-level `"tags": [{"id","name"}]` beside
`contact_methods`, ordered `lower(name), id`. `GET /api/people` rows are
unchanged byte-for-byte (rule 6). Inline "create and apply" is two requests
(`POST /api/tags`, then `PUT …/tags/{tag_id}`); there is no mixed
`{tag_id}|{name}` body. PUT, not PATCH: the API has no PATCH anywhere and
CORS already allows PUT.

### Realtime (e1; SLICE_003 §6 pointer)

`PersonChange` gains `TagsChanged` (wire `tags_changed`), published on the
existing `person.changed` event after an add or remove that changed a row.
The Web handler already invalidates the person, People, Today and saved-list
count queries for any Person change, so tag-clause lists and counts refresh
with no handler change; the Web's closed `PersonChange` token union gains
`'tags_changed'`. Rename and delete publish **nothing**: the mutating
client invalidates its own tags query; other tabs show the old label until
their normal refetch (labels, not identity; D-050's one-active-tab
envelope). No tag name travels on the channel (D-023 ids-only).

### Operator (e1; SLICE_005 §5 pointer)

`PersonDetail` (the `get_person` tool view) gains `tags: Vec<UntrustedText>`,
populated in the `crm-api` tool backend from `list_for_person`. Tag names are
user-authored text and carry the untrusted marker like list names (011c).
`PersonCard` (search results) is unchanged. No new tools; the existing crate
fences pass unchanged.

### Web (e1 chips and Tags page; e2 FilterBar; UI_STYLE and D-045 bind)

- **Person page:** a chip row in the identity header beside the stage and
  assignee controls. Chips are neutral monochrome (no per-tag colour),
  small text, hairline border; each carries an icon-only remove button with
  a 40px target and the accessible name "Remove tag <name>". An **Add tag**
  ghost button opens a `glass-panel` popover: a labelled search input over
  the Organization's tags minus those applied, keyboard navigable, with a
  "Create '<typed>'" row when no case-insensitive match exists. Choosing a
  tag sends the PUT; the create row runs `POST /api/tags` then the PUT.
  Pending state disables the control; 409s render inline ("This person
  already has 20 tags" / "This Organization already has 200 tags"); 404
  on a tag that vanished refetches the tags query. Escape closes and
  returns focus to the button. No modal focus trap.
- **People preview:** the same chips, read-only, under the name; no
  controls and no new fetch (the preview reads the detail query).
- **Tags page:** `/manage/tags`, reachable by **every member** (no
  `requiresOrgAdmin` meta; rule 1 lets creators manage unused tags), with a
  Manage navigation entry. A flat table: name, people (`person_count`),
  and, only where the row's `can_manage` is true, Rename (inline field,
  Enter/Escape) and Delete; other rows show the controls disabled with the
  tooltip "Only an admin can change a tag that is in use". Delete opens
  the existing ConfirmDialog stating the count and, when the count is
  non-zero, the sentence "Saved lists and Today rules that use this tag
  will show an invalid-filter notice until they are edited." Success
  invalidates the tags query; a 409 on rename shows the taken name inline;
  a 403 (the tag was applied by someone else between read and write) or a
  404 refetches and explains.
- **FilterBar (e2):** `tags` and `not_tags` join the clause kinds as
  multi-value kinds using the same multi-select editor as `stage`, options
  from the tags query, labels "Tagged" and "Not tagged", chip text
  "Tagged: Investor, Past client" / "Not tagged: Sphere", 50-value cap.
  Both round-trip through the URL like every other clause; a URL carrying
  an unknown tag id renders the chip with "an unknown tag" and the server's
  422 as the bar's existing invalid-filter state. The FilterBar receives
  tags the way it receives stages (props from each mount: the People page
  and the Today rules page). Saved-list editing and the system-feed editor
  inherit the chips. **Repair and notices:** the People page's
  resolvable-references check, which today knows stages and assignees,
  gains tags so a draft naming a deleted tag is treated as unresolvable
  and the writer is shown the repair path instead of a round-trip 422; the
  Today sources notice gains the sentence "This list refers to a tag that
  no longer exists." for `invalid_tag`, beside the existing stage and
  assignee sentences.
- **Types and keys:** `Tag`, `TagRef`, `PersonDetailResponse.tags`, the two
  `FilterClause` variants, `SavedListFilterError` and
  `TodaySourceIssueError` unions gain `'invalid_tag'`, and
  `queryKeys.tags(orgId)` joins the key factory (the SLICE_002 §10 rule).

## 6. Authorization, tenant isolation, failure and observability

- **Authorization:** every tag route needs an active membership
  (`AuthContext`). Rename/delete are decided inside the command under the
  tag row's `FOR UPDATE` lock and a `FOR SHARE` membership re-read: admin,
  or creator of a tag with zero `person_tag` rows (rule 1, D-051). An admin
  demoted concurrently, or a creator whose tag was applied by someone else
  a moment earlier, gets 403 and writes nothing. The creator path never
  applies to a tag in use, so it can never invalidate anyone's saved list.
- **Isolation:** every statement carries the literal Organization
  predicate; the composite FKs make a cross-Organization `person_tag` row
  unpersistable. A foreign or nonexistent tag id **in a filter** is 422
  `invalid_tag` with a body identical to a random uuid's; a foreign or
  nonexistent Person or tag **on a mutation path** is 404 `not_found`,
  identical to nonexistent (SLICE_002 §6 posture). `GET /api/tags` never
  lists another Organization's tags; People of Organization B never match
  Organization A's tag filter.
- **Failure:** database unavailability is 503 `unavailable` on every route;
  a failed reference probe is `FilterError::Database`, never reported as an
  invalid id (011a review R2). A concurrent delete between a client's
  tags fetch and its PUT is a 404 the client resolves by refetching.
- **Idempotency:** create-or-get, add, remove and rename-to-same are
  target-state idempotent (`created`/`changed` false); the advisory lock
  serialises create and rename per Organization; a repeated delete is 404.
- **Observability:** spans `tag.create` (`outcome` created|existing|limit),
  `person_tag.add` / `person_tag.remove` (`person_id`, `tag_id`, `outcome`
  changed|unchanged|limit), `tag.rename` (`tag_id`, `outcome`), `tag.delete`
  (`tag_id`, `removed_count`); `filter_kinds` already records the new kinds.
  **Tag names are never logged** — low sensitivity, but AGENTS §9 forbids
  unnecessary customer content and ids suffice.

## 7. Contract declaration and amendment ownership

Approval of this specification satisfies AGENTS §11 for:

| Previous → proposed | Reason / affected | Compatibility and required amendment |
|---|---|---|
| No tag model → §2 tables, §3 commands | Ladder 011e; D-043 FUB parity | Additive migration; no existing table changes. SLICE_002 §2 pointer. |
| `GET /api/people/{id}` without tags → `+ tags` | Person page and preview | Additive field. SLICE_002 §5 row pointer. |
| Six new routes (§5) | Tag management and Person tagging | Additive; same error envelope; two new 409 codes; rule-1 permission decided in the command (D-051). SLICE_002 §5 rows. |
| `PersonChange` five variants → six (`tags_changed`) | Invalidate on tag changes | Additive; unknown change tokens are already tolerated by the Web handler. SLICE_003 §6 pointer. |
| Operator `PersonDetail` → `+ tags` | Operator sees what the page shows | Additive view field, untrusted text. SLICE_005 §5 pointer. |
| Thirteen clause kinds → fifteen (§4, e2) | Ladder 011e purpose | Additive; older binaries fail closed. 011a §§4a/4b/4c/4d/4e, 011b §4 and 011d §2 (statement count corrected to fourteen) pointers. |
| `filter_error` codes `unsupported_filter\|invalid_stage\|invalid_assignee` → `+ invalid_tag` (e2) | First deletable referenced entity | Additive code in a closed union: 011b §§4/5, 011c §§3/5, 011d §6 and the Web unions. The e2 lane owns the code; the coordinator owns the pointers. |

The coordinator owns the amendment pointers, ladder, state, this
specification and the brief. No Person ownership, visibility scope, history
fact, D-042 capture behaviour or Today tier changes.

## 8. Performance (D-050)

The envelope is 25,000 People and at most 200 tags per Organization. The
predicates are correlated primary-key probes per candidate Person, the same
shape as `has_phone`. Gates, exactly two:

1. **Paired relative regression:** `filtered_summaries`,
   `count_filtered_matches` and `person_state.sql` with **no** tag clause,
   new statement against the pre-e2 statement, same build, machine,
   fixture and clock; p95 within max(25 ms, 10%); payloads equal.
2. **Plan shape:** one `EXPLAIN (ANALYZE, BUFFERS)` each of
   `filtered_summaries` with a `tags` clause and with a `not_tags` clause on
   the perf book, showing the `person_tag` primary-key probe (or the
   tag-led index) and no super-linear growth with People.

Absolute latency is reported, never gated. Evidence under
`docs/design/perf/slice-011e-<date>/`. e1 adds no hot statement and needs
no performance evidence beyond its tests.

## 9. Acceptance criteria and verification

e1:

1. **Schema:** migration applies on a fresh database and one with existing
   Organizations; a cross-Organization `person_tag` row is rejected by the
   composite FKs; `lower(name)` uniqueness holds; grants as §2;
   `db_schema.rs` enumerations. (db)
2. **Create:** new name → 201 `created:true`; same name in any case → 200
   `created:false` with the first spelling; the 201st tag → 409
   `tag_limit_reached`; two concurrent identical creates yield exactly one
   row and two successes. Trim, 1–40, control characters → 400. (unit + db)
3. **Apply/remove:** 200 `changed:true` then `changed:false` on repeat; the
   21st distinct tag → 409 `person_tag_limit_reached` while re-applying an
   existing one at 20 stays 200; foreign or nonexistent Person or tag → 404
   identical bodies and no row; `tags` in the response is complete and
   ordered. (db)
4. **Rename:** admin 200 on any tag; the creator 200 while the tag is
   unused and 403 once any Person carries it (including one applied by a
   concurrent `AddPersonTag` that committed first); a non-creator member
   403 on an unused tag; same name `changed:false`; case-only rename of the
   same tag is `changed:true`; case-insensitive collision 409
   `tag_name_taken`; admin demoted or deactivated inside the transaction
   403 and no write; 404 for another Organization's id; no realtime event
   published. (db)
5. **Delete:** admin: `person_tag` rows removed and counted in
   `removed_from_people`; creator: 200 with `removed_from_people: 0` while
   unused, 403 once in use; non-creator member 403; second delete 404;
   another Organization's Persons and tags untouched; no realtime event
   published; an `AddPersonTag` racing an admin delete ends 404 or a
   consistent success, never 503. (db)
6. **Reads and precedence:** `GET /api/tags` lists only the Organization's
   tags with correct counts and a per-viewer `can_manage` (admin: all true;
   member: true only for own unused tags) in `lower(name), id` order;
   `GET /api/people/{id}`
   includes ordered `tags`; `GET /api/people` rows are byte-identical to
   pre-e1; the §5 error precedence holds on each route; `/api/tags` joins
   the platform-only-session 401 enumeration in `db_admin.rs`. (db)
7. **Realtime:** `person.changed{tags_changed}` published exactly once per
   changing add/remove and not on `changed:false`; the Web handler
   invalidates person/People/Today/counts (existing tests extended). (db +
   Vitest)
8. **Operator:** `get_person` returns `tags` as untrusted text; a foreign
   Person is still refused; crate fences pass. (operator tests)
9. **Web:** chips render from the detail response; add via existing tag;
   create inline (two requests, in order); remove with a 40px target and
   accessible name; inline 409 messages; preview read-only chips; Tags page
   controls enabled exactly where `can_manage` is true and disabled with
   the tooltip elsewhere; delete confirm sentence only for a non-zero
   count; 403 and 404 refetch paths; keyboard and focus return. (Vitest)
10. **Walkthrough:** alice (member) creates "Investr" on a Person, notices
    the typo, removes it, renames it to "Investor" on the Tags page while
    unused and re-applies it; bob sees the chip after refetch and in the
    preview; alice can no longer rename it (in use); the admin renames and
    then deletes the tag and the chip disappears for bob; a second
    Organization is unaffected throughout.

e2:

11. **Wire:** both kinds round-trip; non-canonical uuid, empty array, > 50,
    duplicate value, duplicate kind, unknown field → 400; a foreign tag id
    → 422 `invalid_tag` byte-identical to a random uuid; a DB failure
    during the probe → 503. (unit + db)
12. **Semantics:** `tags` any-of with one and several ids, positive and
    negative; `not_tags` none-of including an untagged Person matching;
    both together intersect; an Organization B Person carrying an
    identically named tag never matches; a system feed edited to carry a
    `tags` clause admits and excludes the right People on Today, for both
    person-state feeds and the call feed. (db)
13. **Parity:** absent clauses ⇒ byte-identical results across all
    fourteen statements (People default and seven sorts, count, both Today
    source statements, the three feed statements) on the existing parity
    and feed-equivalence fixtures; present clauses agree between People,
    count, Today source membership and feed evaluation. (db)
14. **Reference paths:** write time: `POST`/`PUT /api/saved-lists`,
    `PUT /api/today/sources/{list_id}` and the system-feed update with a
    foreign or deleted tag id → 422 `invalid_tag`. Read time: a saved list
    naming a deleted tag → detail `filter_error:"invalid_tag"` with metadata
    preserved, count 422, the writer repairs by editing; `GET
    /api/today/sources` reports `filter_error:"invalid_tag"`; as a Today
    source → source issue `invalid_tag` with partial availability, both when
    the tag was deleted **before** Today enumerated the source and when it
    vanishes **during** evaluation; in a system feed → `InvalidDefinition`
    with canonical fallback, `fallback:true`, the admin feed read showing
    `filter_error:"invalid_tag"` and the member read showing the effective
    default. Never `unsupported_filter`. (db)
15. **Today:** a saved list with a `tags` clause enabled as a source admits
    matching People with the list reason and drops them when the tag is
    removed from the Person. (db)
16. **`describe()`:** exact strings for one and several names and the
    unknown placeholder; both name-loading sites resolve tag names. (unit +
    db)
17. **Web:** two FilterBar chips with the multi-select editor on both
    mounts; URL round-trip; unknown-id rendering and the 422 state; a draft
    naming a deleted tag is unresolvable and shows the repair path, never a
    round-trip 422; the Today sources notice sentence for `invalid_tag`;
    the list editor and the system-feed editor accept the chips. (Vitest)
18. **Performance:** §8's two items, evidence retained.
19. **Final gates (each rung):** on the final tree the coordinator runs
    `./scripts/sqlx-prepare`, `./scripts/check` and `./scripts/check-db`
    once; the lane runs them per round. Independent review by the reviewer
    and adversarial analysis by the tester, read-only, at most two rounds
    (D-050).

## 10. Delivery

Two rungs, each **one lane, one writer, one short-lived branch from
`main`**, e1 first:

- **e1 (S–M):** migration (sole owner of migrations and `.sqlx`), `tag`
  module and five commands, six routes, detail `tags` on HTTP and Operator,
  `TagsChanged`, Web chips on the Person page and preview, `/manage/tags`,
  tests §9.1–9.10. Backend and Web are written by the same lane in
  sequence (backend first, then Web against real routes) because the Web
  half is small and the wire contract is short.
- **e2 (S–M):** the two clause kinds, fourteen statements and `.sqlx`,
  `invalid_tag` across saved lists, sources and system feeds, `describe()`
  and `FilterNames`, FilterBar chips, parity and plan-shape evidence, tests
  §9.11–9.18. Depends on e1's merge.

If the user prefers one rung, D-049's parallel backend and Web lanes apply
with §5's contracts frozen first and the backend lane owning the migration
and `.sqlx`; the ladder's standing S–M rule makes the split the default.
Both rungs are named in the ladder and PROJECT_STATE before implementation
starts. This specification authorizes nothing until the user approves it
after independent review; approval will authorize implementation and tests,
not commit, merge, push or deployment.
