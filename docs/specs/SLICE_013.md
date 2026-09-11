# Slice 013 — Operator `filter_people` and `run_saved_list`

**Slice 019b amendment (approved 2026-09-10):**
[Custom-field filtering](SLICE_019b.md). Amends the saved-list result/error behavior: run_saved_list evaluates stored custom-field clauses through the shared application path and wraps their descriptions as untrusted text. Invalid references use invalid_field/invalid_option. filter_people inputs and the tool schema/count remain unchanged.

**Status: APPROVED by the user on 2026-09-08 after independent review.**
Approval covers the declared contracts (§7) and the §9 safe defaults; it
authorizes implementation and tests in the Slice 013 lane, not commit, merge,
push or deployment. Prepared against `main`
at `b45b04f`. Companion lane:
[Slice 012](SLICE_012.md) (denormalized activity columns), planned in
parallel; see §10. Brief: [SLICE_013_IMPL.md](../tasks/SLICE_013_IMPL.md).

The AI Operator can search People by name and retrieve a Person, but cannot
use the filter vocabulary the Slice 011 ladder built. This slice adds two
**read-only** tools: `filter_people`, which takes human names (stages, tags,
member display names, `me`, sources, recency windows, the boolean axes) and
resolves them server-side into a validated `FilterDefinition`; and
`run_saved_list`, which evaluates a saved list the actor is allowed to see.
"Who are my investors I have not contacted in 30 days" and "show my Stale
Zillow list" become one tool call each. The model never sees or supplies a
trusted id, an unknown or ambiguous name comes back as a clarification the
model must put to the user, and the Web needs no change.

Authority: [D-028, D-029, D-034, D-043, D-046, D-047, D-048, D-050,
D-051](../decisions/DECISION_LOG.md); AGENTS §5 (no direct DB access,
server-owned context, minimum necessary context, action risk, never invent
trusted ids); O-010 (search parked: no free-text clause);
[005](SLICE_005.md) (the Operator crate, tools, views, tool loop, adapter,
prompt, tests), [006b](SLICE_006b.md), [011a](SLICE_011a.md) §§4/5/7,
[011b](SLICE_011b.md) §§2/4/5, [011c](SLICE_011c.md) §5,
[011e](SLICE_011e.md) §§4/5.

## 1. Scope and product behavior

In scope: two tools and their schemas; server-side name resolution; a
`FilterOutcome` view with matched, needs-clarification and list-invalid
shapes; prompt rules; telemetry; tests. Out of scope: raw `FilterDefinition`
JSON from the model (§1 rule 1); listing stages, tags or members as a tool
(names are resolved, not enumerated, except inside a clarification);
mutation tools; count-only queries; rendering `describe()` lines as chips in
the Web drawer (LATER: it would touch `web/src/api/types.ts`); selecting a
member whose display name is shared with another member (LATER: member ids
in a tool result); any change to the vocabulary, statements, saved-list
queries or migrations.

Product rules (safe defaults, veto-able; none is a human decision):

1. **Name-based parameters, never raw ids.** `PersonCard` carries names
   only and no tool lists ids, so a raw-`FilterDefinition` tool would force
   the model to invent ids (AGENTS §5.2). Names are resolved by exact
   case-insensitive trimmed match against the Organization's own stages,
   tags and active members; `me` and `unassigned` are tokens.
2. **Unknown or ambiguous is a clarification, not a strike.** The tool
   returns an `Ok` outcome listing what was unknown and the available
   vocabulary for that dimension only (§5.3), the `start_call`
   `choice_required` precedent. It counts as a successful call and resets
   the malformed-call counter, so clarifications never end the turn; the
   turn budget (four rounds of three calls) bounds the worst case. An empty
   spec is the one `invalid_arguments` strike (rule 5). The prompt tells the
   model to ask the user, never to retry with a guess. Stage resolution is
   exact match first; if more than one stage matches case-insensitively
   (impossible today: stages are seeded with case-distinct names and no
   route creates them) the tool fails closed as unknown with the available
   stages. Member resolution considers **active** members only; the tokens
   `me` and `unassigned` win over a member literally so named. Echoed
   unknown names are the model's own strings, clipped to 80 characters and
   control-stripped exactly as `search_people.query` is.
3. **Saved lists obey D-046 exactly.** `run_saved_list` resolves through the
   same visibility predicate the Lists index uses: the actor's own personal
   lists plus shared lists. Another member's personal list, a foreign
   Organization's list and a nonexistent list are byte-identical
   `not_found`, including for an admin.
4. **Context is bounded.** `limit` 1–25, default 10 (absent means 10, not
   the maximum, unlike the existing tools), cards in the query's own order
   (a saved list's stored sort applies). `count` is `min(matches, 500)` and
   `more_than_500` mirrors the People page's truncation (011b §4): the
   existing statement fetches up to 500 summaries to return at most 25
   cards, the same cost the People page pays, inside D-050; no second count
   statement. `MAX_REFERENCES` rises from 10 to 25 so the drawer shows every
   card the model was given; **side effect, accepted:** `get_today` can now
   place up to 20 cards in the drawer instead of 10, and the reference-cap
   test fixture grows to at least 25 ids.
5. **An empty `filter_people` is `invalid_arguments`**, not "everyone": the
   People page's valid-empty semantics are for a table, not a chat reply.
6. **`describe()` lines and list names are untrusted text** (tag and list
   names are user-authored; 011c §6, 011e §5).
7. **Read-only, executes immediately** (AGENTS §5.4). No proposal row, no
   confirmation, no write path reachable.

## 2. Tool contracts (declared, D-028 §3)

### `filter_people`

Parameters, `additionalProperties: false`, all optional, at least one
condition required:

| Field | Type and bounds | Resolves to |
|---|---|---|
| `stage_names` | string[1..50], each ≤ 80 chars | `stage` clause, exact case-insensitive match on the Organization's stages |
| `assignees` | string[1..50]: `"me"`, `"unassigned"`, or a member display name | `assigned_to` clause; names against active members; `me` → the actor |
| `sources` | string[1..50], each `^[a-z0-9_]{1,64}$` | `source` clause; a stale source is valid and matches nobody (011a §4b) |
| `tag_names_any`, `tag_names_none` | string[1..50] | `tags`, `not_tags` clauses (D-051 names are unique case-insensitively) |
| `created` | `{op: within_days\|not_within_days, days: 1..3650}` | `created` clause (`never` excluded, 011a §4b) |
| `last_inquiry`, `last_contact`, `last_inbound` | `{op: within_days\|not_within_days\|never, days?}` | the age clauses |
| `has_replied`, `has_phone`, `has_email`, `awaiting_response`, `client_replied_unanswered`, `awaiting_call_outcome` | boolean | the boolean clauses |
| `limit` | integer 1..25, default 10 | cards returned |

This is a one-to-one mirror of the fifteen `Clause` kinds with ids replaced
by names; a unit test iterates `kind_label()` to pin that every kind has a
field. Caps (`MAX_CLAUSES` 20, `MAX_VALUES` 50) hold by construction.

### `run_saved_list`

`{ name?: string ≤ 80, list_id?: uuid }`, exactly one required, plus
`limit`. `list_id` exists only for the duplicate-name case: the
clarification lists candidates with ids and the model passes one back, the
`start_call` `contact_method_id` pattern. The list's stored `sort` is
honoured (D-048) via the existing sorted statement.

### Result view (`views.rs`)

```rust
pub enum FilterOutcome {
    Matched(FilterResult),
    NeedsClarification {
        unknown_stages: Vec<String>, unknown_tags: Vec<String>,
        unknown_assignees: Vec<String>, ambiguous_assignees: Vec<String>,
        available_stages: Vec<String>, available_tags: Vec<UntrustedText>,
        members: Vec<String>,
        candidate_lists: Vec<SavedListRef>,   // {list_id, name: UntrustedText, scope}
    },
    ListInvalid { error: String },            // unsupported_filter|invalid_stage|invalid_assignee|invalid_tag
}
pub struct FilterResult {
    pub list: Option<SavedListRef>,
    pub description: Vec<UntrustedText>,      // describe() lines
    pub count: usize,                         // 0..=500
    pub more_than_500: bool,
    pub returned: usize,
    pub matches: Vec<PersonCard>,
}
```

`available_*` lists are populated only for the dimension that failed and are
bounded by the schema's own caps (≤ 200 tags, ≤ 50 members, a handful of
stages). Not-found saved lists are `ToolError::NotFound`; the fixed "no such
person" detail string becomes tool-aware. `FilterError::Database` →
`ToolError::Backend` → 503 `operator_unavailable` as today; every other
failure is a structured tool result, never an HTTP error.

### Crate seam (D-034)

`ToolBackend` gains two methods taking crm-operator-owned input types
(`PeopleFilterSpec`, `SavedListSelector`); no crm-app type crosses the fence.
The crm-api adapter resolves names using the three org-scoped list reads the
saved-list `FilterNames` loader already uses (stages, active members, tags),
builds `FilterDefinition { version: 1, clauses }` from the public clause
structs, runs `validate()`, `describe(&names)`, `to_query_params(actor)`
(which resolves `me`), and calls `filtered_summaries` or
`filtered_summaries_sorted` unchanged. `validate_references()` is **not**
run: every id came from the caller's own Organization-scoped list reads in
the same request, so the check would cost one query per value for nothing;
the millisecond race with a concurrent tag delete yields a filter matching
nobody or the untagged, inside the same Organization, exactly as on the
People page. Saved lists resolve through `list_saved_lists` and
`saved_list_detail`, which need the request's `AuthContext`: the Operator
route moves its owned `auth` into the spawned task and passes it to
`SqlxToolBackend::new`; the adapter uses it only for those two reads (never
`actor_email` or the Organization name); no `AuthContext` is synthesised from
`OperatorContext`, and crm-operator still sees only the opaque context. A
list whose detail carries `filter_error` or no filter returns `ListInvalid`
without re-validation; its `sort` selects the sorted statement, `None` the
default one.

## 3. Prompt and provider

`prompts/system.md`: the tool count "six" becomes "eight" (string-pinned by
a test); use `filter_people` for criteria (stage, tag, assignee, source,
recency, replied, phone, email) and `search_people` for a name, email or
phone; "my …" means `assignees: ["me"]`; "not contacted in N days" includes
the never-contacted and the reply must say so (011a §4c); on
`needs_clarification` ask the user and never retry with a guess; on
`not_found` for a list say you cannot see a list by that name; report
`count` and "more than 500" honestly and use the `description` lines as the
reason (thesis §12.3). Provider unchanged (`openai/gpt-oss-120b`); budgets
unchanged (a typical exchange is one tool call plus the answer); the tool
runs inside the tool span under the turn deadline. Tests use the scripted
provider as every Operator test does.

## 4. Authorization, isolation, risk

- Both tools run under `OperatorContext` built only from `AuthContext`; scope
  is the Organization's `PersonVisibilityScope`, exactly as `search_people`.
- No schema property is an id except `list_id`, which is re-validated
  through the visibility predicate; the existing "every schema forbids
  additional properties and has no trusted ids" test extends automatically
  (the `list_id` property is declared and justified).
- Name resolution consults only the caller's Organization, so a foreign
  stage or tag name is "unknown" and the clarification lists only the
  caller's vocabulary; `validate_references` is the same org-scoped check the
  People route runs.
- Read-only: no proposal, no `operator_proposal` row; the adapter methods are
  SELECT-only over existing prepared statements.

## 5. Web

No change. Cards flow through `references.people` and render with the
existing Operator Person card; `description` reaches the model, not the
wire, and arrives at the user as the model's prose. `web/src/api/types.ts` is
not touched. Existing Vitest passes unchanged.

## 6. Telemetry

The existing `operator.tool_call` span **declares** new fields as
`tracing::field::Empty` (an undeclared field is silently dropped by
`Span::record`) and the adapter records them: `filter_kinds` (from
`kinds_field()`), `filter_clause_count`, `resolution` (matched |
needs_clarification | list_invalid), `match_count` (≤ 500), `more_than_500`,
`returned`, `saved_list_scope`. Never names, ids, sources, day counts or list
ids (D-029). The ledger row stays `tool_name`, `outcome`, `duration_ms`,
`person_ids`; `ledger_name()` gains arms for both tools so `tool_name` is
never `unknown`.

## 7. Contract declaration

| Previous → proposed | Reason / affected | Compatibility and amendment |
|---|---|---|
| Six tools → eight (`filter_people`, `run_saved_list`) | D-043's vocabulary reaches the Operator | Additive; snapshot `tests/snapshots/tool_definitions.json` updated deliberately. SLICE_005 §3 pointer. |
| `ToolBackend` trait + two methods; `FilterOutcome` view | Seam for the new tools | Additive; fake backends in tests implement them. SLICE_005 §3 pointer. |
| `MAX_REFERENCES` 10 → 25 | Drawer shows what the model saw | Additive envelope change; `get_today` may now contribute up to 20 drawer cards. SLICE_005 §5 pointer. |
| Prompt asset | New rules and tool count | Declared contract change per D-028 §3. |

## 8. Acceptance criteria

1. `tool_definitions()` exposes both tools; the snapshot is updated
   deliberately; both schemas forbid additional properties and carry no
   trusted id other than the declared `list_id`. (unit)
2. Parser: > 50 values, wrong types, `created.never`, days outside 1–3650,
   an empty spec → `invalid_arguments`; `limit` clamped to 1–25 and absent
   `limit` → 10. (unit)
3. Every `Clause::kind_label()` has a spec field; the fake backend's
   `seen` assertions cover both new methods. (unit)
4. A composed filter with valid stage, tag and member names returns cards,
   `description`, `count`, `more_than_500: false`; the ledger's `person_ids`
   equal the card ids and its `tool_name` is the tool, never `unknown`. (db,
   scripted turn through `POST /api/operator/turns`)
5. Unknown tag name → `needs_clarification` with `available_tags` as
   untrusted text; ledger outcome `ok`; a second clarification in a row does
   not end the turn. (service unit + db)
6. Ambiguous member display name → `ambiguous_assignees` with candidates;
   `me` resolves to the actor (two actors get different results). (db)
7. `last_contact not_within_days 30` includes the never-contacted. (db)
8. Caps: 30 matches with `limit: 10` → 10 cards, `returned: 10`, `count: 30`;
   501 matches → `count: 500`, `more_than_500: true`. (db)
9. `run_saved_list` by name: own personal list; shared list for member and
   admin; another member's personal list → `not_found`, admin included;
   duplicate names → `candidate_lists` then `list_id` resolves; a list with
   `invalid_tag` → `list_invalid`; the stored sort is honoured. (db)
10. Foreign `list_id` and foreign stage or tag names: results byte-identical
    to nonexistent; no foreign name appears in any prompt (`requests_json`).
    (db)
11. A full scripted turn returns 200 with `references.people` from the
    filter and no `operator_proposal` row. (db)
12. Crate fences and the dependency test pass unchanged. (gates)
13. Span and log capture for a turn with a tag name and a list name contains
    `filter_kinds` (positive assertion, proving the fields are declared) and
    neither string, nor ids or day counts. (db)
13a. The reference-cap test's fixture holds at least 25 unique ids and
    passes at the new cap; `search_people` behaviour is otherwise unchanged.
    (unit)
14. The prompt-rule test pins the new rules and the tool count. (unit)
15. Web: existing Vitest passes; `types.ts` unchanged. (gates)
16. Final gates once on the final tree by the coordinator; reviewer and
    tester read-only, at most two rounds (D-050).

## 9. Safe defaults adopted (veto-able)

Name-based parameters; `limit` default 10 and max 25 with `MAX_REFERENCES`
25 (and the `get_today` drawer side effect); empty spec is
`invalid_arguments`; clarification as an `Ok` status that resets the
malformed counter; not-found lists as `ToolError::NotFound` with a
tool-aware detail; `description` wrapped as untrusted text wholesale; stored
sort honoured; `validate_references` not re-run after resolution; active
members only, tokens win over same-named members; stage case-insensitive
collision fails closed; same-named members not selectable by name in this
version (LATER: member ids in a tool result); `available_tags` may carry up
to 200 wrapped names in one clarification (bounded by D-051).

## 10. Delivery

One lane, one writer, worktree `../crm-worktrees/013` on
`slice-013-operator-filter` from `main`; size **S**. Owns
`crm-operator/**` (backend trait, tools, views, service, prompt, snapshot),
`crm-api/src/operator/**` (adapter plus a new pure `filter.rs` name
resolver, unit-tested without a database) and a new
`crm-api/tests/db_operator_filter.rs`. Two one-line touches outside that
boundary are granted in advance: `crm-api/src/routes/operator.rs` (move
`auth` into the spawned task and pass it to the backend constructor) and
`crm-api/tests/all.rs` (register the test module **in alphabetical position**,
after `db_operator_call`, never at the end: Slice 012 registers its own
files in their alphabetical positions, so the hunks do not overlap and the
rebase auto-resolves; if a conflict still arises the resolution is "keep
both"). The `requests_json` helper is private to `db_operator.rs`; the new
test file re-declares it or the lane moves it to `tests/common` (that file
is shared and the move is granted). No edits to `person/filter.rs`,
`person/queries.rs`, `saved_list/**`, migrations or `.sqlx`: every statement
used is already prepared. Slice 012 merges first and this lane rebases; its only exposure is
the unchanged `filtered_summaries`/`to_query_params` signatures. This
specification authorizes nothing until the user approves it after
independent review; approval will authorize implementation and tests, not
commit, merge, push or deployment.
