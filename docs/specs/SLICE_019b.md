# Slice 019b — Custom-field filters across People, lists and Today

Release follow-up: the user subsequently authorized “commit, deploy, merge
with main and delete loose branches.” That authorization supersedes the
implementation-only release restrictions below. The verification results
remain the evidence for this unchanged source tree.

**Status: APPROVED by the user (2026-09-10): “approved. go ahead and implement it.”**
Approval covers the defaults, declared contracts, implementation and required
verification in this worktree. Commit, merge, push and deployment remain separate.
Astra high completed the specification and brief; a separate Astra high
reviewer returned READY after the two planning review rounds. Both blocking
corrections are resolved; dispositions are recorded in the brief. This is
documentation/code inspection evidence, not an implementation or test result.
Prepared against
`6bad52a`, branch `codex/slice-019b-custom-field-filters`, checkout
`../crm-worktrees/019b`. D-058 §1 schedules this rung with its own specification
and approval; the user's explicit approval above satisfies this contract gate.
Implementation brief: [SLICE_019b_IMPL.md](../tasks/SLICE_019b_IMPL.md).

Authority: AGENTS.md; D-004/005/008/011/043/046/047/048/050/058;
accepted ARCHITECTURE_BASELINE; SLICE_011a/b/b_SORT/c/d/e; SLICE_019 (019a).
The defaults introduced below were accepted through this specification's approval.

Implementation and verification completed on 2026-09-10 in the uncommitted
worktree. Results: [SLICE_019b_VERIFICATION.md](../tasks/SLICE_019b_VERIFICATION.md).

## 1. Outcome and boundary

An active member can filter People by the Organization's custom fields, save
those criteria in a personal list, and use the list on Today. An admin can use
the same clauses alongside the locked anchors in Today rules. Examples:
Budget at least 500000; Anniversary between two calendar dates; Referrer
contains a name; Lead temperature is Cold or Warm; Budget is empty.

One typed vocabulary and the existing static SQL evaluation paths own all
membership. Four existing types only: text, number, date, single choice.
No custom sort, People column or PersonSummary change; no new HTTP route,
Operator tool or free-form Operator filter input; no new field type, recurring
date, relative custom-date operator, nested boolean expression, import,
bulk value mutation, dependency or service. Existing saved-list sorting,
permissions, revisions, idempotency, Today ordering/caps and locked rule
anchors remain binding.

Defaults accepted with this spec's approval: five custom clauses within
the existing twenty-clause total; one clause per custom field; operators in
§2; negative predicates include empty values; an archived field invalidates
its filter, while an archived option remains a valid filter choice. These
do not change D-058's stored-value or archive policy.

## 2. Typed v1 wire contract

`FilterDefinition { version: 1, clauses: Clause[] }` remains unchanged.
Fifteen existing clause kinds gain four kinds with these **exact** shapes:

```text
{ kind: "custom_text", field_id: UUID, test: TextTest }
{ kind: "custom_number", field_id: UUID, test: NumberTest }
{ kind: "custom_date", field_id: UUID, test: DateTest }
{ kind: "custom_choice", field_id: UUID, test: ChoiceTest }

PresenceTest = { op: "is_set" } | { op: "is_not_set" }
TextTest = PresenceTest
         | { op: "contains" | "not_contains", text: string }
NumberTest = PresenceTest
           | { op: "range" | "not_range", min?: decimal-string, max?: decimal-string }
DateTest = PresenceTest
         | { op: "range" | "not_range", min?: "YYYY-MM-DD", max?: "YYYY-MM-DD" }
ChoiceTest = PresenceTest
           | { op: "any_of" | "none_of", option_ids: UUID[] }
```

Question marks mean omitted optional properties, not nullable values. A range
requires at least one bound. Equal bounds express equality. No generic JSON
predicate or arbitrary string operator reaches SQL. Rust uses four typed clause
payloads and typed test enums; `Clause` keeps its duplicate-safe deserializer.

Structural validation (400 `malformed_request`) retains clause order and first
failure. Total cap stays 20. Existing kinds remain one per kind. Custom kinds
may repeat **for different field IDs**, with a separate five-clause cap and one
clause per field ID across all four kinds. Two differently typed clauses for
the same ID are malformed before reference lookup. No sorting/deduplication
of clauses or choice IDs; serialization preserves order and existing v1 bytes.

All UUIDs require the existing canonical lowercase hyphenated form. Unknown
keys/kinds/operators, duplicate keys at any nesting level, wrong JSON types,
explicit nulls, empty/duplicate option arrays, more than 50 option IDs, and
extraneous operands on presence tests fail closed. Existing limits and HTTP
body/query handling are unchanged.

Reuse `custom_field::model::validate_text_value`,
`validate_number_pattern` and `validate_date_range`, with filter errors mapped
to `Malformed`, not value-write 422 codes. Text is 1–500 code points, no control
characters, already trimmed of space/tab/CR/LF: validation must reject input
that the value validator would trim, rather than silently change saved criteria.
Case is retained in serialized text. Numbers stay strings matching
`^-?[0-9]{1,15}(\.[0-9]{1,4})?$`; preserve valid literal spelling in the saved
filter, including `"12.50"`, as part of its retry identity. Compare numeric
bounds using checked decimal-to-scaled-`i128` arithmetic (scale 10,000), never
floating point or lexicographic strings. No decimal dependency. Dates require
exact ten-character `YYYY-MM-DD`, a real calendar date, and the existing
1900-01-01…2200-12-31 range. Validate `min <= max`; reject reversed bounds.

## 3. Membership, absence and references

All clauses are ANDed; choice IDs within one clause are ORed. An unset field
means **no `person_custom_field_value` row** for that Person and field. A blank
text string is not a separately stored empty state; clearing removes the row.
Zero and negative numbers are set; every valid date is set; a held archived
choice option is set. Never use truthiness or NULL comparisons to infer this.

| Test | Match |
|---|---|
| `is_set` / `is_not_set` | Row exists / does not exist for the typed field. |
| text `contains` | Set text contains the literal substring, case-insensitively using PostgreSQL `lower` on both operands. `%`, `_` and backslash are literal, with no wildcard, regex or fuzzy semantics. |
| text `not_contains` | Exact complement of `contains`, including unset. |
| number/date `range` | Set value satisfies every supplied inclusive bound. No timezone conversion for DATE. |
| number/date `not_range` | Exact complement of `range`, including unset. |
| choice `any_of` | Set option ID is one of the IDs. |
| choice `none_of` | Exact complement of `any_of`, including unset. |

Use `EXISTS` for positive tests and `NOT EXISTS` for their negative complements;
do not negate a nullable scalar comparison. Display negative labels with
“(or empty)” so the inclusion is visible. Presence is available for every type.

After all structural validation, resolve references in submitted clause order:
the field must belong to the trusted active Organization, be live, and have the
type named by the clause; otherwise 422 **`invalid_field`**. For choice tests,
then validate each option in array order against the same field AND Organization;
otherwise 422 **`invalid_option`**. Missing, foreign and wrong-type fields give
byte-identical `invalid_field` envelopes; missing, foreign and wrong-field
options give byte-identical `invalid_option` envelopes. Neither error echoes
IDs, values, labels or existence details. A valid field with an invalid option
does not become `invalid_field`. Database errors remain 503 `unavailable`.

Archived **options remain valid**, including on newly composed filters: the
filter picker offers them marked “Archived”, since People can still hold them.
This differs deliberately from 019a value editors, which cannot newly assign
an archived option. Archived **fields are invalid references** even for
presence/negative tests. Restore makes the same saved filter valid again.
Renaming fields/options affects descriptions only, never membership or stored
IDs. No archive block, private-list usage enumeration, cascade rewrite or list
revision bump: D-046 forbids leaking private list criteria to admins. The field
archive confirmation gains a general explanation that filters using the field
will need repair or restoration; no count of referencing lists is shown.

Use lightweight, ID-bounded, Organization-scoped static lookups for reference
and label resolution. Do **not** call 019a `list_definitions`/its grouped
`person_count` scan from each filter evaluation or description. The normal and
`validate_references_until` paths must share the semantics; the latter re-arms
the SQL statement timeout before every new probe from the remaining absolute
deadline, never a fresh allowance. Cap work at five fields and fifty option IDs
per choice clause; one field probe and one option-array probe per clause is
sufficient. Preserve first failing clause even if metadata was bulk-loaded.

## 4. SQL slots and inventory

Extend `PersonFilterParams` with exactly five ordered optional custom slots.
For each slot bind nine nullable SQL values, always in this order:

```text
field_id uuid; field_type text; operation text; text_operand text;
number_min text; number_max text; date_min date; date_max date; option_ids uuid[]
```

Operations/type are closed server-produced tokens from typed enums. Missing
slots bind all nine NULLs. Filled slots follow submitted custom-clause order;
unused operand columns are NULL. Numbers bind as text and cast to numeric in
the static statement. A NULL field ID makes the whole slot inert. A present
slot dispatches its fixed typed predicate; unknown operation/type fails false
in SQL as defense in depth. No runtime SQL construction, SQL function predicate
engine, JSON iteration, value interpolation, dynamic column name or model SQL.

Append five NULL-guarded blocks **after existing tag predicates and before
consumer-specific tail restrictions/ORDER/LIMIT**, using the literal trusted
Organization parameter `$1` in every value/definition probe. Existing parameter
positions remain unchanged; append the following new ranges to the binding
lists. Every probe also binds `person_id = p.id`, `field_id` and `field_type`.
Check live matching definition in the predicate as well as reference validation
so a definition archived between statements cannot accidentally match an empty
negative clause; the next read reports `invalid_field` normally. Do not promise
atomic multi-statement snapshots beyond each consumer's existing transaction.

| Existing statement / binding owner | Old last bind | New slots |
|---|---:|---|
| `person/sql/filtered_summaries.sql`, default Added descending; `person/queries.rs` | 27 | 28–72 |
| `filtered_summaries_created_asc.sql` | 27 | 28–72 |
| `filtered_summaries_name_asc.sql` | 27 | 28–72 |
| `filtered_summaries_name_desc.sql` | 27 | 28–72 |
| `filtered_summaries_stage_asc.sql` | 27 | 28–72 |
| `filtered_summaries_stage_desc.sql` | 27 | 28–72 |
| `filtered_summaries_assignee_asc.sql` | 27 | 28–72 |
| `filtered_summaries_assignee_desc.sql` | 27 | 28–72 |
| Inline `person/queries.rs::count_filtered_matches` | 26 | 27–71 |
| `today/source_membership.sql`; `today/sources.rs` | 28 | 29–73 |
| `today/source_candidates.sql`; `today/sources.rs` | 30 | 31–75 |
| `today/system_feeds/sql/person_state.sql`; `system_feeds/evaluate.rs` | 55 | A: 56–100; B: 101–145 |
| `system_feeds/sql/call_membership.sql`; `system_feeds/evaluate.rs` | 28 | 29–73 |
| `system_feeds/sql/call_only.sql`; `system_feeds/evaluate.rs` | 29 | 30–74 |

Fourteen statements contain **fifteen matrices**; person-state A and B need
independent slots. Apply predicates before each existing cap and preserve all
sorts/tie-breakers, candidate exclusion, source membership tail, reason
precedence, hydration boundary and time basis. `list_summaries` stays untouched.
Regenerate `.sqlx` for all changed production and fixture macro statements.

Existing indexes: value PK `(organization_id, person_id, field_id)` gives at
most one value per correlated probe; `person_custom_field_value_field_idx`
on `(organization_id, field_id)` serves field reads; composite definition and
option keys enforce tenant/type/field association. Start with these, no schema
migration. At the fixed five-slot cap, indexed probes are bounded per Person;
do not claim the 500-row response cap bounds scanning. A new index requires
measured plan evidence and coordinator review, with explicit migration ownership
assigned before creating it; no speculative set of per-type indexes.

## 5. HTTP, persistence and stale-filter behavior

No new routes or top-level success fields. Four new kinds enter all existing
v1 read/write consumers. Add `InvalidField`/`InvalidOption` explicitly wherever
`InvalidTag` currently travels: `FilterError`, `ApiError`, saved-list command
and filter errors, Today source filter-error accessor/evaluation/issues (including
outcome telemetry), system-feed errors/loaders/read views, and Web closed unions.
No wildcard may map these repairable failures to `unsupported_filter`.

| Surface | Required result |
|---|---|
| People query | New clauses narrow the same People rows; absent filter/default sort uses legacy query. |
| Saved-list create/update and Today-source connection | Validate through the same typed/reference path. Existing authorization, quota, revision and retry ordering remains. |
| Saved-list detail | Supported but stale custom reference retains typed filter, sort and metadata, with `filter_error: invalid_field|invalid_option` and a repairable description. Unsupported structure/version still returns null filter/sort and `unsupported_filter`. |
| Saved-list count | Invalid reference returns 422, not zero. |
| Today source settings | Existing row reports the same `filter_error`; no silent removal. |
| Today source evaluation | Invalid source contributes an issue and no membership; available sources/built-ins remain with existing partial availability. Database timeout remains unavailable. |
| System-feed edit/preview/read | New clauses work beside anchors; invalid reference reports new code. A stored invalid rule keeps the existing `invalid_definition` canonical fallback on Today, visibly reported; no fallback-definition persistence. |

Preserve exact precedence from current code: People authenticates before query
extraction, then obtains its connection, decodes/validates the filter, validates
references and evaluates (infrastructure failure can occur at acquisition).
Saved-list resource paths validate path UUID before auth, then structure,
visible lookup, write permission, revision, references. Create checks shared
permission and idempotency before references and quota. Feed update checks
admin extractor before body, structure before row revision, references before
feed-rule validity. Preview checks structure, then the subject's current active
membership (404), then references/rules (422); a missing/inactive subject wins
over an invalid custom reference or rule. Insert custom reference checks only
at these existing positions. The stale helper comment in
`system_feeds/commands.rs` that says the reverse may be corrected when touched;
the implementation at `preview_today_system_feed_attempt` is the precedent.

Stored filter JSONB gets the new kinds; no table change or existing-row rewrite.
Existing request digest/equality/revision behavior persists, including ordered
arrays and number literal spelling. Older binaries reject new kinds and report
stored definitions as unsupported; never remove unknown clauses or interpret
them as all People. Deploy API and Web supporting the vocabulary together;
rollback of code leaves new stored definitions unreadable until upgraded.

Criteria may contain personal text or numbers, as existing saved-list source
criteria do. They remain erasable saved-list/feed configuration in the existing
JSONB columns, with D-046 personal-definition privacy and ordinary shared-list
visibility. Person erasure cascades that Person's values, **not independently
authored filter criteria**; this slice adds no retention promise or automatic
criteria erasure. Labels and operands must never enter facts, audit metadata,
logs, spans, realtime payloads or error envelopes.
New operand-bearing clause/test/parameter/name types require redacting `Debug`
implementations, including wrappers whose derived `Debug` would expose nested
contents. Instrumentation remains `skip_all`, recording only the static
`filter_kinds` and clause count, safe resource IDs and outcome/timing fields.
Never format a filter parser error containing an operand into logs or responses.

## 6. Descriptions, Operator and Web

`FilterNames` gains ID-keyed custom-field labels and field-scoped option labels,
resolved only from the active Organization for referenced IDs. Unknown field:
“an unavailable custom field”; unknown option: “an unavailable option”; never
raw UUID. Live field descriptions use the current label; archived metadata may
be named with “Archived” only after Organization-scoped lookup. Extend all
`FilterNames` constructors, including the Operator adapter.

The existing Operator `run_saved_list` tool continues to work with lists
containing these clauses, including new errors and existing `UntrustedText`
wrapping of descriptions. `list_saved_lists` is a domain metadata query, not
an Operator tool; its existing consumers remain unchanged. Field/option labels
and criteria remain untrusted;
an instruction-like value cannot authorize a tool or change the system prompt.
No second predicate path: saved-list tools use the application evaluation.
`filter_people` free-form construction retains its existing
vocabulary and tool snapshot; they cannot compose custom-field clauses in this
rung. The Operator may run an existing list or read a Person's values; do not
claim custom-filter construction support in prompts/UI.

Web changes stay in the current FilterBar, used by People/list workspace and
TodayFeedsView. Fetch `useCustomFieldsQuery` once per consumer using existing
Organization keys and 10-second stale time. Picker has a searchable “Custom
fields” group, live fields in position order, with field label and type. Already
used field opens its existing editor; different fields of the same type remain
independently selectable. Key edit/remove/draft identity by field ID for custom
clauses, not kind. At five custom clauses (or twenty total), disable adding
another with a visible explanation; editing/removing is still possible.

Editors offer the exact §2 operators with readable labels. Text uses a normal
input; numbers use `inputmode="decimal"` and string validation; date inputs use
calendar dates; ranges show From/To with “at least/on or after”, “at most/on or
before” or “between” descriptions as applicable. Choice is the existing
searchable checkbox pattern, including archived options labeled “Archived”.
All allow Is empty/Is not empty. No new value is applied merely by opening a
field. Explicit Apply commits a valid custom draft; Enter applies valid text
or range drafts, Escape cancels, and outside dismissal cancels uncommitted
custom drafts. An invalid draft shows an inline reason and cannot alter the
committed filter. Preserve current non-custom editor behavior.

Use accessible labels for field/operator/operands, full chip labels for screen
readers, visible focus, keyboard-operable controls, return focus to trigger on
close, and reachable overflow at narrow viewport. Pending definitions show a
loading state; fetch failure offers Retry without erasing existing clauses.
No live fields shows “No custom fields available.” Missing/archived field chips
remain removable in a repairable saved definition. Never replace an invalid
saved list with an unfiltered table. Existing URL-origin rejection behavior
from 011a stays unchanged: invalid URL criteria are dropped with notice; an
in-session draft rejection remains an error, and 503 keeps criteria. Preserve
navigation/dirty guards, pending-row inertness and locked Today anchors.

## 7. Cache and realtime recovery

The existing ids-only `person.changed {custom_field_changed}` token now affects
membership. Remove it from the Person-only branch (leave `note_changed` there):
invalidate Person detail, People prefix, Today, saved-list count prefix and
custom-field definitions (their `person_count` can change). Apply the same
set in local value-mutation settling through the existing Person mutation hold,
including uncertain error outcomes. No optimistic membership calculation.

Definition/option mutations publish no new realtime event. On successful
rename/archive/restore, locally invalidate the Organization query prefix, which
covers definitions, cached Person details, People, lists/descriptions/counts,
Today source settings and feeds. Create/add/reorder can retain definitions-only
invalidation. This adopts the existing no-push definition precedent and
D-050's one-active-tab envelope; other agents get authoritative definitions
on normal refetch/navigation, and reconnect invalidates the Organization.
Do not add private list IDs/criteria to Organization publications.

## 8. Contract declaration and amendment pointers

Implementation owner is the single lane in the brief; coordinator owns pointer
amendments. The user's approval covers precisely these contract changes:

| Current → proposed | Why / affected components | Compatibility and amendment |
|---|---|---|
| 15 kinds, one per kind → 19 kinds, custom uniqueness per field, five slots | Custom membership across API/application/Web | Additive v1; old kinds unchanged; old binaries fail closed. SLICE_011a §§4a–4e/5/6; SLICE_011e §4. |
| Reference errors end at `invalid_tag` → add `invalid_field`, `invalid_option` | Repairable lists/sources/rules | Closed unions grow; no success envelope change. SLICE_011b §§4/5, SLICE_011c §§3/5, SLICE_011d §§2/4/6. |
| 14 statements without custom operands → five slots per matrix | Single membership semantics before caps | Regenerated SQLx metadata; no schema migration. SLICE_011a §4e, SLICE_011b §4, SLICE_011c §4, SLICE_011d §§2/5. |
| JSONB v1 filters cannot contain custom predicates → may persist typed criteria | Saved lists and feed configuration | No rewrite/backfill; rollback limitation and criteria privacy in §5. SLICE_011b §3, SLICE_011d §3. |
| Person-only custom value invalidation → membership/definition-count invalidation | Visible results stay current | Same realtime wire, broader client semantics. SLICE_019 §§1/5/7/10, SLICE_003 §6. |
| Operator saved-list descriptions have old kinds → new descriptions/errors | Existing list tools remain truthful | No tool schema/count change; untrusted wrapper retained. SLICE_013 saved-list tool sections. |

No conflict with accepted decisions is known. Archive/reference semantics,
operators and slot cap are accepted defaults under this specification;
any later expansion requires its own declared contract change.

## 9. Acceptance criteria and verification

1. **CONTRACT:** every new wire variant round-trips; strict key/type/UUID rules,
   five versus six slots, 20 versus 21 total, repeated kind on distinct fields,
   duplicate same-field across kinds, 0/50/51 choices, text controls/trim/500,
   numeric boundaries/precision/reversed bounds, invalid date/leap/window cases.
   Pure Rust tests plus thin Web validation tests; legacy serialization unchanged.
2. **BOUNDARY:** synthetic set/absent/zero/negative/boundary/archived-option
   fixtures prove §3, including literal `%_\\`, case-insensitive text, exact
   negative complements and inclusive single/equal/two-bound ranges. DB tests.
3. **TRUST:** field/option IDs from Organization B, absent IDs, type mismatch,
   wrong-field option, archived field, member/admin/platform-only sessions;
   byte-identical rejection envelopes and zero foreign People across read/write
   consumers. References respect clause order; stale revision beats bad reference;
   feed preview with both a missing/inactive subject and invalid custom reference
   returns subject 404 (and valid subject plus invalid reference returns 422).
4. **CONTRACT:** one table-driven membership suite executes all fourteen
   statements, all five occupied slots and each type; count and all eight sorts
   agree with expected IDs, capped at 0/500/501 as before. Source membership
   includes retained IDs beyond a list's first 500; candidates exclude built-ins.
   Both independent person-state matrices and both call paths narrow correctly.
5. **CONTRACT:** regression cases for all fifteen old kinds, empty filter and
   absent filter preserve results/order/reasons. New clauses mix with stage,
   assigned-to-me, tags, negated tags and locked feed anchors; no custom sort.
6. **BOUNDARY/TRUST:** save/duplicate/update/retry/conflict, list count, source
   connect/settings/evaluation, feed edit/preview/fallback; field archive creates
   repairable invalidity and restore recovers without changing list revision;
   option archive/rename preserves match by ID. Deadline/DB failure is unavailable,
   with existing source partial availability. Reuse existing deadline harness.
7. **BOUNDARY:** FilterBar can edit/remove two same-type fields independently,
   cancel/apply drafts, use all controls by keyboard, show limits/errors/loading/
   Retry/empty/archived states; People URL, saved-list repair, dirty navigation
   and Today locked-rule behavior stay correct. Vitest plus one browser walkthrough.
8. **CONTRACT:** value changes invalidate all §7 keys through local settling and
   realtime; note-only invalidation remains narrow; definition changes refresh
   dependent caches; reconnect refetches. Query/realtime Vitest.
9. **TRUST:** saved-list tools return the new descriptions as untrusted text and
   reject invalid definitions; synthetic instruction-like label/value sentinel
   absent from telemetry/ledger/realtime. Tool snapshot and crate fences unchanged.
10. **PERFORMANCE:** §10 gate evidence plus final `sqlx-prepare`, `check`,
    `check-db` on the delivered tree, recorded with exact commands and results.

## 10. Performance and delivery budget

D-050 binds: 25,000 People, 50 members, at most five concurrent Today loads,
one active browser tab per agent. Synthetic fixtures only. One benchmark run
unless the paired regression fails; at most two review/fix rounds. No new
absolute-latency, pool-wait, multi-tab or concurrency-above-five gates.

Freeze the old statements from `6bad52a` into a narrowly scoped test fixture,
following `fixtures/statements_b45b04f`. Frozen SQL timing alone does **not**
meet the authenticated-request gate: Slice 012's retained evidence is a SQL
comparison, and 011d's old HTTP legacy dispatch was removed. Add the following
minimal test-only dispatch so old/new paths run in the **same build, machine,
fixture and clock**, through current request/auth/application orchestration.
Do not clone an old router, Today algorithm, source validation or whole product.

Under the existing `crm-app/test-support` feature, a task-local
`FilterStatementOverrides` seam (new `person/filter_test_support.rs`, registered
by `person/mod.rs`) can supply frozen execution at the seven existing query
wrapper families: summaries (sort selects eight statements), count, source
membership, source candidates, person-state, call membership, call-only. The
seam receives the **same** connection, typed parameters and existing wrapper
arguments and returns the **same** decoded result; ordinary validation,
authorization, transaction/timeout/deadline management, ranking, truncation,
source merge and HTTP serialization continue through current code. Hooks sit
in `person/queries.rs`, `today/sources.rs` and `system_feeds/evaluate.rs`.
Mirror the boxed-future/task-local scope pattern in `today/test_support.rs`.
Frozen fixture implementations carry only the old static SQL, old bindings and
necessary row decoding; they assert no custom slots are populated. Frozen SQL
may use fixed `include_str!` text with bound runtime SQLx fixture queries, so
feature-only fixture SQL need not add macro metadata to the ordinary prepare
gate. No constructed/interpolated SQL. Production statements keep their macros.

The harness builds two loopback applications with
`AppState::for_tests`, `build_app_with_today_router_and_perf_collector` and
`routes::today::router_with_test_clock`; log in once per arm through the normal
`POST /api/session` and retain the synthetic session cookie in memory. Wrap the
frozen app's service in a harness-local middleware that scopes its fixed
overrides around `next.run(request)`. The selected arm is captured when building
the test server, never selected by a header, query, body or product environment
flag. All tasks executing DB work must retain that scope. The live app takes the
normal SQL path; both include the same small middleware/collector overhead.
Use existing HTTP capture hooks for timing evidence and the existing common
fixed Today clock. People/count timing cases use stable non-relative predicates
so their existing database-clock calls cannot move membership between arms.
Test feature code must compile away in the ordinary `cargo check --workspace`.

Register new `db_custom_field_filter_perf.rs` in `tests/all.rs` only under
`#[cfg(feature = "perf-harness")]`; its single ignored test is named
`slice_019b_authenticated_request_performance`. No Cargo/dependency edits.
Reuse loopback/auth and `run_serial`, `run_wave`, nearest-rank percentile helpers
from the existing HTTP fixture; do not inherit its old 10/20-concurrency waves
or absolute latency gates. The new bounded protocol is:

| Request case | Workload and samples per arm |
|---|---|
| Eight People sort cases | Existing explicit empty v1 filter, each existing sort; 5 serial warmups then 40 measured serial requests per sort. |
| Saved-list count | Existing mixed stage/tag filter with current revision; 5 serial warmups then 40 measured serial requests. |
| Today, zero sources | Book with both person-state arms and call membership/call-only exercised; 5 serial warmups then 40 measured serial requests, plus 2 warmup waves and 8 measured waves at concurrency 5. |
| Today, five sources | Same book, five existing mixed filters with overlapping matches and room for source candidates; same serial and concurrency-5 samples. |

Pair arms case by case, sequentially on equal-sized pools with identical
settings; alternate frozen-first/live-first by case. Retain raw warmups
separately; never blend serial and concurrency-5 p95 or omit a failed request.
Time from client dispatch to fully consumed body (auth, acquisition, SQL and
serialization included). Require all responses successful/complete, compare
bodies for equality, and gate each paired measured series on request p95 growth
no more than `max(25 ms, old p95 * 10%)`. Report SQL timings separately; never
substitute them for request p95. Record statement hit counters once outside
timed waves to prove both arms execute all fourteen statement paths. New custom
predicates have no old-code counterpart and are covered by parity plus plans,
not a fabricated old request comparator. The brief pins the exact invocation.

In that same run capture one `EXPLAIN (ANALYZE, BUFFERS)` per changed hot
statement (all fourteen, both person-state arms populated), and new reference/
label statements. Use a 25k-person book with five populated custom fields per
Person, mixed sparse/selective/nonselective predicates, five filled slots and
a second Organization; include text negation and ranges/choice tests. Record
the 50-definition setting as metadata, not 50 query slots. Require indexed
probes or bounded set scans, no full value-table scan per Person or other
super-linear growth. Report plans, buffer counts, absolute p95 and fixture
shape; do not extrapolate production capacity. If the realistic fixture or
plan gate fails, stop for a bounded design correction before expanding scope.

One Terra high implementation writer, backend checkpoint then Web. Separate
Astra review at the backend checkpoint and final Web/integration review within
the two-round cap; coordinator owns final-tree verification, browser evidence,
amendment pointers and approval gates. No commit, merge, push or deployment is
authorized by this implementation approval.
