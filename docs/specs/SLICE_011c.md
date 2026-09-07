# Slice 011c — Saved lists feed Today

**Status: APPROVED — implementation authorized by the user on 2026-09-06.** Prepared
on 2026-09-06 against local `main` at `9d62e86`, including saved-list
implementation `2af023c`. Branch: `codex/slice-011c-today-sources`.
Specification, coordination and independent review use **Astra / xhigh**;
implementation, tests and fixes use **Terra / xhigh**, per the user's current
assignment. The user moved 011c ahead of the separately queued 011b-sort rung.

An agent can turn a saved list into a source for their own Today. Matching
People appear with the list's name as a reason, including People who do not
qualify for an existing Today rule. The current rules keep working, and their
items retain admission priority when Today reaches its 200-item limit.

Authority: [D-010, D-022, D-033, D-042, D-043, D-046 and D-047](../decisions/DECISION_LOG.md),
the [accepted ladder](../plans/SLICE_011_LADDER.md),
[architecture baseline](../architecture/ARCHITECTURE_BASELINE.md),
[011a](SLICE_011a.md), [011b](SLICE_011b.md), [003](SLICE_003.md),
[006c](SLICE_006c.md) and [009](SLICE_009.md).
On 2026-09-06 the user chose **up to five enabled sources per viewer** and
**show available work with a notice when a source cannot load**. Following
independent READY review, the user explicitly approved this complete plan,
including its declared contracts, query corrections and acceptance limits. See the
[plain-language companion](SLICE_011c_EXPLAINED.md).

## 1. Scope and product behavior

Include self-service source enable/disable on saved-list detail, a small source
manager on Today, persistent preferences, current membership evaluation,
deterministic ranking and deduplication, Web and existing Operator read-tool
parity, recovery, privacy and performance evidence. No new external dependency
or service is required.

Reuse v1 `FilterDefinition` unchanged. A source refers to the **current saved
definition**, not a copied filter or a saved set of People. An unsaved preview
does not change Today. A saved update changes that source on its next read;
renaming changes its reason label. Enabling never edits the list or a Person.
Duplicating a list does not copy its source preference.

Any currently visible list may be enabled: one's own personal list or an
Organization-shared list. Source choices affect only the actor making them.
Members may enable shared lists without admin rights. An admin has no ability
to inspect or edit someone else's choices or personal list names/criteria.
The five-source limit is separate from D-046's saved-definition quotas; an
enabled but invalid live definition consumes a slot until disabled or deleted.

List evaluation keeps Organization-wide Person visibility. There is **no
additional assignment restriction**: a list without `assigned_to` may supply
unassigned People and People assigned to colleagues. `me` resolves to the
viewer. A shared `me` list can therefore supply different People to two viewers.
Existing inquiry/reply ownership remains by assignee; outcome-needed ownership
remains by caller. This is the accepted ladder's “any visible list” behavior,
not a change to `PersonVisibilityScope` or Person authorization.

Logging contact only removes a list reason if the resulting data no longer
matches that list. A list filtered only by stage/source can keep showing someone
after contact. Show this guidance while enabling; suggest an activity criterion
when a repeat-contact queue is intended. Do not force a criterion, secretly
exclude contacted People, add a stage-name rule, or introduce Done/Snooze.
Another qualifying reason can keep the Person on Today after one reason clears.

Out of scope: editable built-ins and new derived filter axes (011d), sorting
saved People tables (011b-sort), tags (011e), org-pushed feeds, source weights or
manual ordering, membership-entry timestamps, alerts on entry, snapshots,
materialization/denormalized activity maintenance, background feed jobs, count
push, search, new Operator mutation tools, mobile UI, bulk work and dismissal.
The existing built-in SQL's membership/ranking and 011a/011b People ordering
and 500-item cap are regression requirements, not inputs to source selection.
The original built-in query remains the comparison baseline; §8 permits only
the enumerated, measured SQL predicate/planning changes after parity review.
It does not permit a general rewrite of Today or the existing People queries.

## 2. Source persistence and commands

One additive migration, owned by the implementation lane, introduces normal
CRUD `today_work_source`:

| Column | Constraint |
|---|---|
| `organization_id`, `user_id`, `list_id` | UUID, not null; composite primary key of these three columns |
| `created_at` | TIMESTAMPTZ, not null, database default `now()` |

Composite FK `(organization_id,user_id)` references Organization membership.
Composite FK `(organization_id,list_id)` references `saved_list`; add the
supporting unique `(organization_id,id)` constraint there in the same migration.
Index `(organization_id,list_id)` supports list deletion cleanup. Grant the app
role SELECT/INSERT/DELETE, no UPDATE/TRUNCATE. Absence means disabled. There are
no default sources, seeds, backfill, copied names/filters, history facts, or new
generic retry framework. Membership deactivation retains preferences;
reactivation restores them under current list visibility. O-004 stays open.

Commands take server-owned `CommandContext` and typed `SavedListId`:

```text
EnableTodayWorkSource { list_id, expected_list_revision }
DisableTodayWorkSource { list_id }
```

Each command locks/rechecks current active membership in its transaction,
following 011b's `FOR SHARE` posture. Use the existing `saved-lists:`
Organization advisory-lock namespace and ordering (membership, advisory lock,
list row), shared with saved-list mutations. These infrequent configuration
writes serialize the final source slot and enable-versus-edit/delete races;
Today reads do not take this mutation lock. Typed callers cannot bypass this
authorization or validation.

Enable resolves a live visible list, verifies the expected revision, strictly
decodes and validates v1 criteria and Organization references as 011b does,
then inserts if absent and below five. An already-enabled matching revision is
a successful no-op even at the cap. A stale revision conflicts even if already
enabled; the client must review the saved definition rather than silently
enable changed criteria. Unknown clauses/version/fields never become all People.

The quota counts **live visible enabled definitions**, using the same
Organization/creator-or-shared/live join as configuration reads, not raw
`today_work_source` rows. This also covers supported rollback: an older 011b
binary can soft-delete a list without knowing to remove its preferences.
After upgrade those dormant rows remain invisible and consume no slot; the
source manager and command quota cannot disagree. Current deletion still
performs atomic cleanup as below. Do not add cleanup writes to a read path.

Disable requires current visibility but does not require valid criteria or an
expected revision; it is idempotent for an already-disabled list. It also
accepts a visible tombstone under the same creator/shared visibility rule, so
an uncertain disable can complete after concurrent deletion. Unknown,
cross-Organization and someone else's personal IDs return the same 404.
No separate source ID is exposed.

Extend `DeleteSavedList` to delete all preferences referring to the list inside
its existing transaction. Soft deletion still clears list name/filter and
retains the 011b retry tombstone. Its confirmation explains that the list is
also removed from Today sources (for everyone when shared). No preference
cleanup occurs during a read. A defensive read join always requires live,
visible, same-Organization definitions, so an orphaned/hidden preference
cannot contribute a name, reason or Person even if persistence is corrupt.

## 3. Configuration HTTP

Add authenticated routes with the existing `{"error":"<code>"}` envelope:

| Method/path | Input | Success |
|---|---|---|
| GET `/api/today/sources` | None | 200 `{"limit":5,"sources":[Source,...]}` |
| PUT `/api/today/sources/{list_id}` | `{"expected_list_revision":1}` | 200 `{"enabled":true,"changed":true-or-false}` |
| DELETE `/api/today/sources/{list_id}` | No body | 200 `{"enabled":false,"changed":true-or-false}` |

`Source` is exactly `{list_id,name,scope,revision,filter_error}`. `scope` is
`personal|shared`; `filter_error` is null or 011b's
`unsupported_filter|invalid_stage|invalid_assignee`. At most five live visible
enabled definitions, ordered by `list_id ASC`. This is configuration, with
validation but **no membership/count evaluation**. No other user's choices,
owner IDs, filters, descriptions, tokens or Organization IDs are returned.

PUT bodies deny unknown/duplicate fields; use 011b's canonical positive safe
integer revision validation and 128 KiB body ceiling. Malformed path UUID uses
the existing bare path-extractor 400 precedence. Otherwise: authentication
401; body/structure 400 `malformed_request`; visible lookup 404 `not_found`;
revision 409 `saved_list_conflict`; invalid stored definition/reference 422
with its filter-error code; cap 409 `today_source_limit_reached`; success.
GET infrastructure errors and unavailable commands return 503 `unavailable`.
Never label a DB failure as an invalid definition or a zero-match result.

No automatic mutation retries. On uncertain PUT/DELETE, refetch configuration;
the explicit retry repeats the same request. Do not auto-advance the expected
revision on conflict. Enable/disable have idempotent target-state effects;
opposing later user actions may change that state, as with ordinary preferences.

## 4. Today evaluation, ranking and the cap

Keep one `today::query` application path for HTTP and Operator callers.
Evaluate at query time using static SQL, typed bound parameters and literal
Organization predicates; no dynamic SQL, SQL interpreter, new authorization
scope or fetching full lists into Rust.

### One clock and read snapshot

The multi-statement read uses one read-only repeatable-read transaction and
one server-selected evaluation timestamp for built-ins and every filter age
cutoff. New source SQL binds that timestamp; the existing ad-hoc/count queries
keep their contracts. The API's `generated_at` is that evaluation timestamp.
Resolve enabled definitions, their revisions/names, references, matching IDs
and Person details from this same snapshot. A concurrent rename/filter edit,
contact attempt or deletion appears in the next read, never as old criteria
with a new label. A single request does not promise to observe a commit that
happens after its snapshot.

### Bounded selection algorithm

1. Run the behavior-preserving built-in candidate query, including its existing
   201-row sentinel, then existing ranking. Only §8's bounded, measured
   predicate corrections may change that SQL text. Let **B** be its retained set
   (at most 200), in its established relative order, and **K = 200 − |B|**.
   Every member of B stays in the response, including outcome-needed rows.
2. For each live valid enabled source, evaluate its current v1 criteria with
   the existing `PersonFilterParams` semantics. Test membership for **every
   retained B ID**, even when it lies beyond that source's first candidates.
   This bounded-ID evaluation appends all applicable list reasons to B.
3. If K > 0, select at most **K+1 matching non-B IDs per source**, ordered
   by the common list key below, before any limit. If B has 200 items and the
   built-in query was not truncated, select at most one non-B match per source
   solely to establish truncation. If B was already truncated, omit that
   non-B work; never admit a discarded built-in row into the list band.
4. Hydrate the bounded non-B prefixes in the same snapshot, within each
   source's failure/budget boundary; perform the ordered ID selection before
   full projection. Union/deduplicate successful candidates in Rust by Person
   ID, sort by the same key and retain the first K. New static queries may
   combine these projections but must preserve their bounds and semantics.
5. Append each successful matching source's `list_member` reason exactly once
   to each retained item, sorted by list UUID. Existing inquiry/reply reasons
   come first, then list reasons, then `call_outcome_needed` if present. Existing
   reasons retain their relative order and the call reason remains last.
   No duplicate Person or reason.

The per-source K+1 limit is valid because every source uses the same total
order: a Person beyond its first K+1 has at least K+1 distinct eligible People
ahead in the global union. For any retained non-B Person, every successful
source it matches includes it within that source's prefix, so reasons cannot
be lost. Excluding B **before** limiting avoids duplicate built-ins exhausting
slots. Test this proof with heavily overlapping sources and at cap boundaries.

The People display/count endpoint's first 500 is **never** the source input.
Match beyond that boundary must participate in Today ordering. Nor may a
per-source count or output limit be described as a bound on PostgreSQL scans.
Each source hydrates at most K+1 (at most 201) non-B summaries, plus retained
built-ins already read; it never hydrates an Organization or saved list in full.
Implementations may discard losing prefixes during merge. The source cap bounds
fan-out and memory but does not establish performance feasibility (§8).

### Membership and ordering

*Pointer (Slice 011b-sort, 2026-09-06):* a saved list's own sort (D-048) is
explicitly not an ordering input here; source evaluation reads the filter only,
and a parity test pins identical Today bodies for a sorted list and its
unsorted duplicate.

Existing inquiry/reply candidates keep their current priority, action,
`waiting_since` and precedence. A B member qualifying only by outcome keeps
`low`/`set_outcome`, even if it also matches a list. A new list-only item uses
priority **`list`**. Final **display order** is:

1. Existing `high` items, unchanged relative order.
2. Existing `normal` items, unchanged relative order.
3. List-only items: never contacted first; then effective last-contact
   `occurred_at ASC`; then Person UUID ascending to break equal/null ties.
4. Existing `low` outcome-needed items, unchanged relative order.

Admission and display order are deliberately separate: reserve B first, then
insert the list-only band into the displayed queue. At capacity, list-only
items are shed before any retained built-in, including low items. Explain
this in the source manager and cap notice. The server alone orders the result;
the Web and model must not re-sort it. Existing `rank()` may remain the
built-in mapper; a new pure merge function owns this explicit final order.

“Effective contact” uses the existing corrected-attempt chain semantics and
`occurred_at DESC,id DESC` payload tie-break. Any member's contact attempt
counts; captured outbound correspondence already writes that fact (D-042).
The filter's max-contact timestamp remains equivalent under corrections,
which inherit occurred_at. Never use a source-enabled timestamp, list name,
list order, created time or LLM score as a hidden ordering key.

`truncated` is true if the built-in query had more than 200 candidates or the
known successful non-B union contains more than K. Otherwise false. A source
failure is represented separately; `truncated:false` under incomplete source
evaluation is not a claim that all potential work was evaluated.

## 5. Today response and partial availability

`GET /api/today` keeps no client viewer/filter parameters. Its response becomes:

```text
{ generated_at, items: [TodayItem,...], truncated,
  sources: { status: "complete" | "partial" | "unavailable",
             issues: [SourceIssue,...] } }
SourceIssue = { list_id, name, revision,
                error: "unsupported_filter" | "invalid_stage" |
                       "invalid_assignee" | "unavailable" }
```

With no enabled sources, `sources` is `{status:"complete",issues:[]}` and
existing item selection/order/content is unchanged. `partial` means enabled
sources were enumerated but at least one failed; `unavailable` means source
enumeration itself failed, so issues is empty and the enabled set is unknown.
Only live visible list metadata from the snapshot may appear in issues.
Invalid filters contribute no candidates/reasons; successful sources still
contribute theirs. No stored raw invalid filter is returned.

Extend `TodayItem` explicitly:

- `priority` gains `list`.
- `reasons` gains `{code:"list_member",list_id:"<uuid>",name:"<name>"}`.
- `latest_inquiry` becomes `InquiryRef|null`. A zero-inquiry Person can match
  existing v1 lists, so it must not be silently excluded or assigned a fake
  inquiry. Built-in items continue to have a real InquiryRef.
- `recommended_action` gains `review_person`. List-only items recommend Call
  if a phone exists, otherwise Email if an email exists, otherwise Review
  person, linking to the existing profile. Existing built-in actions do not
  change; no sending/calling permission is granted by membership.
- `waiting_since` becomes timestamp-or-null: built-in timestamps stay unchanged;
  list-only items have null because membership does not prove a waiting or entry
  time. Never substitute Person created_at or source-enabled time. The displayed
  list key is the existing `last_contact_attempt.occurred_at` or “Never contacted.”

Built-in query failure remains 503, not a fabricated empty Today. Source work
uses savepoints inside the read transaction: stage one source's results until
all its required reads succeed, roll back on failure, discard that source's
staged contribution and continue. An infrastructure error is `unavailable`,
not `filter_error`. A source timeout must actually cancel its SQL and recover
the connection; timing out only the Rust future while SQL continues is invalid.
If the connection cannot be recovered, stop source work, mark remaining known
sources unavailable and return the already captured built-ins/successful
sources with a notice; ensure a failed connection is not returned poisoned to
the pool. Partial replies contain one coherent successful snapshot.

All source work has a finite deadline and no unbounded automatic retry or
per-source connection fan-out. This specification fixes the operational bounds at
**250 ms to enumerate source definitions**, **500 ms total per source**, and
the existing **2-second pool-acquisition timeout** with ten connections. A
source's whole allowance covers decoding/reference validation, B membership,
candidate selection and hydration together. Evaluate serially in UUID order,
giving each of the at most five known sources its own allowance. Set every SQL
statement timeout to its source's remaining time, not a fresh 500 ms; do not
starve later lists by spending a smaller shared budget on early UUIDs.

After a failed source, allow at most **100 ms combined grace** for cancellation,
rollback and connection release/disposition. This is cleanup time, not extra
query work, a fresh source allowance or a retry. Start another source only
after recovery succeeds. If it cannot finish within that grace, discard the
connection, stop further source work and report remaining known sources
unavailable alongside available work. A failure before enumeration completes
yields the generic unavailable state. Pin fair attempts, exhausted budgets,
actual SQL cancellation, rollback and connection disposition in failure tests.
These containment bounds accompany §8's final HTTP thresholds; they are not
a production-capacity guarantee and cannot be raised to pass verification.

## 6. Web and Operator behavior

### Web

On valid named-list detail show **Use as a Today source**, with explicit
“For your Today only” wording, current enabled state and `N of 5` capacity.
Use saved baseline metadata/revision, never unsaved working criteria. When the
list has a dirty preview, state “Today uses the saved version”; offer Save
through the existing flow before enabling if the viewer wants their draft.
Readers of shared lists can enable the saved original without write controls.
Invalid definitions cannot be enabled; already-enabled invalid ones can always
be disabled. Pending disables duplicate submissions. A conflict retains the
current UI, reloads the saved definition for review and requires a new click.
Capacity errors link to Manage sources; no automatic replacement of a source.

Today gains **Manage sources**, a compact panel showing enabled names, links,
invalid-state explanation and Remove controls, plus a Lists link for adding
another. This is not a second filter editor or a dashboard of membership counts.
Explain broad-list persistence, assignment behavior, ordering and the cap here.
Use existing D-045 controls, focus management and responsive layout.

Render list reasons as text links to the named list, preserving the server
sequence and with keys including `list_id` (keying only on reason.code would
collapse multiple list reasons). Never HTML-render names or use duplicate names
as identity. Show list-only priority as “From your lists”; show actual last
contact or “Never contacted” with absolute timestamp tooltip where applicable.
Keep Log contact, Set outcome, Person links and built-in behaviors. Use “items”
or “work” in the heading, not a claim that every row needs a response.

A partial/unavailable `sources` status gets a persistent inline notice above
the available rows: “Some Today sources could not load. Available work is shown.”
List known failed visible sources with retry/manage links; enumeration failure
gets a generic notice with no guessed names. An empty partial result says
“No available work to show; some sources could not load,” never “all caught up.”
A complete empty result may say no work matches current Today rules/sources.
The cap notice says the first 200 are shown and built-in work is kept first.
503 built-in failure remains the existing whole-page recoverable error state.

Extend the central query factory:

```text
today(org) = ['org', org, 'today']                       // invalidation prefix
todayForActor(org, actor) = [...today(org), actor]       // actual read key
todaySources(org, actor) = ['org', org, 'today-sources', actor]
```

All reads use actor+Organization keys, key-derived request context and abort
signals; no private-name placeholder data across identities. Reuse 011b's
session-identity fence for late mutation completions and same-org actor changes.
Logout/session replacement cancels/removes private caches, dialogs and notices.
Update all Today consumers to use the actor-aware read key while keeping the
Organization prefix for existing generic invalidations.

Enable/disable and saved-list update/delete invalidate that actor's source
configuration and Today; uncertain-mutation reconciliation that detects the
committed state performs the same invalidation. Deletion removes cached
definition/reasons promptly.
Existing `person.changed` and reconnect invalidations remain ids-only and cover
Today. **No list/source event or personal identifier/name is published to the
Organization channel.** On Today entry, window focus and manual Refresh, force
fresh Today/source reads regardless of the global 30-second staleTime; keep
the existing focused 60-second Today interval, which also revalidates sources.
An idle tab's time-driven changes and other-browser list edits recover via
those triggers. Source controls on list detail revalidate configuration on
entry/focus/manual refresh. An unavailable/deleted named link fails closed as
011b requires; it never opens all People. No realtime guarantee is invented.

### Existing Operator read tools

`get_today`, `get_next_work_item`, `get_person.on_your_today` and
`explain_priority` call the same Today query, with the same server viewer,
snapshot, cap and ordering. No new tool accepts list IDs, criteria, source
preferences or trusted context from the model; no new crate dependency edge.

Declare the following tool-output changes, while keeping tool input schemas:

- `TodayItemView` accepts priority `list` and action `review_person`, gains the
  list reason and makes `waiting_since` nullable. Its existing nullable
  last-contact timestamp supplies the list sorting explanation; no inquiry or
  waiting date is fabricated in Person cards. `PriorityExplanation::OnToday`
  likewise makes `waiting_since` nullable and adds `last_contact_attempt`
  (timestamp-or-null), so every displayed ordering key reaches the model.
- Add the same source availability object to `TodayView`, `NextWorkItem`,
  `PersonDetail` and both `PriorityExplanation` variants. Tool `total` still
  counts returned/admitted People, not the uncapped or unknown total; a partial
  result must be described as available work. Preserve the get_today tool-limit
  truncation behavior in addition to Today truncation. Add `truncated` to
  `NextWorkItem` and `today_truncated` to `PersonDetail`; existing
  `on_your_today` is explicitly membership in the **returned** queue, not an
  uncapped membership verdict. These flags let the caller qualify a false
  value even when source evaluation itself is complete.
- Extend `Ahead` with `list`; its four counts sum to position−1. Replace
  `ORDERING_RULE` with a deterministic explanation of §4's display order,
  list last-contact/null ordering, and built-in-first cap admission. The model
  cannot invent scores, entry dates or priority from list names.
- For a Person absent from the response, replace assignment-based guessed
  absence with `reason:"not_in_returned_today"`, plus `truncated` and source
  availability. This says only what the query proves. Do not say “already
  contacted,” “not assigned to you,” or “does not match your lists” without
  membership evidence. The same caution applies to `on_your_today:false`.
  No uncapped second scan is introduced just to explain absence.

List names are user-authored untrusted text. In Operator reason and issue
objects, `name` is `UntrustedText` (`{"untrusted_text":"..."}`), following
existing clipping/format-control handling. Fixed reason explanations say
“matches a saved list you enabled as a Today source”; never interpolate the
name into an otherwise trusted explanation string. Filters/descriptions and
other viewers' private choices do not enter prompts or the PII-free ledger.
Pin malicious-name, same-name/multiple-list, partial-empty and cap cases with
the scripted provider and adapter tests.

The browser's Operator conversation is also private session state. The current
`AppShell` keeps `OperatorPanel` mounted while it is merely hidden, and the
panel holds reply text that it resends as turn history. Scope this state to
Organization, actor **and authentication-session lifetime**, not just whether
the drawer is available. Logout/session replacement, same-Organization actor
change, Organization switch and the same actor logging in again must discard
transcript/history, reference cards, message draft, errors, proposal cards and
their local confirmation state. Close or remount the drawer on that identity
boundary; preserve its normal behavior within one session.

Capture the session identity when sending a turn, starting a proposal action
or scheduling a deferred drawer/navigation callback. Before rendering a late
success/error, reopening the drawer or applying proposal completion state,
verify that identity still matches the active session and component lifetime.
Cancel pending requests where supported, but cancellation alone is not the
fence: an old response must never repopulate cleared state. A new session's
next request contains only its own newly entered message/history. Do not store
transcripts/drafts/proposals in persistent browser storage or a shared query
cache. This bounded `AppShell`/`OperatorPanel` change does not alter server call
authorization or proposal/confirmation semantics.

## 7. Contract declaration and amendment ownership

The user approved these declared contracts and the implementation brief on
2026-09-06, satisfying AGENTS.md §11. The single implementation lane owns the
following changes together:

| Previous → approved | Reason / affected components | Compatibility and required amendment |
|---|---|---|
| No source preference → §2 table, FK support and saved-list deletion cleanup | Persistent per-self opt-in; PostgreSQL/crm-app | Additive migration, no backfill; older binaries ignore preferences. Keep data on rollback. 011b §§3/4/6 deletion pointer. |
| No source commands/routes → §§2/3 typed self commands and three routes | Safe configuration; API/Web | Additive routes, same auth/error envelope. 003 §5 and 011b §§4/5 pointers. |
| Built-in-only Today → §§4/5 ordered merge, four tiers and partial source status | Lists add work and explain their reason; API/Web/Operator | `list_member`, `list`, `review_person` are new enum values; latest_inquiry and waiting_since become nullable. Call reason remains last. These need coordinated client updates, not a claim that every old consumer safely ignores them. 003 §§2–5/10, 006c §5a and 009 §6 pointers. |
| rank() preserves one SQL order → preserves built-ins inside an explicit deterministic merge | Correct display and cap admission | Existing built-in relative order retained; new list ordering not delegated to Web/model. 003 §§3/4/14a pointer. |
| Only People/count static filter projections → bounded Today ID/key and hydration projections | Match before Today cap without 500-row bias | Vocabulary/membership semantics unchanged; new SQLx metadata and parity tests. 011a §§4c/4e and 011b §4 pointer. |
| Original built-in SQL/planning → §8's three enumerated same-Organization predicates, unused-source probe guard and transaction-local JIT policy | Avoid demonstrated query-planning/probe costs; crm-app Today SQL and pooled read transaction | No wire, membership, ranking or persistence change. Existing SQL remains the comparison fixture; regenerate SQLx metadata for changed statements and pin result/setting restoration parity. 003 §§3/4/14a amendment pointer; no blanket performance-refactor authorization. |
| Today tool results/absence inference → §6 source state, list tier/reason, truthful bounded absence, untrusted names | Tool parity and privacy; crm-api/crm-operator | Tool input schemas unchanged; output fixtures/prompts/serialization consumers updated together. 005 §§3/7/14 and 006c §5a pointers. |
| Organization-only Today read cache → actor-aware read cache and source refresh | Personal-list names are creator-only; Web | Internal key change, preserve org invalidation prefix; no new realtime wire shape. 003 §§6/10 and 011b §7 pointers. |
| Operator conversation retained while drawer remains mounted → session-scoped conversation and completion fences | Private list names must not cross browser sessions; AppShell/OperatorPanel | Internal lifecycle correction; HTTP turn inputs and server proposal semantics unchanged. 005 §10 and 006b §6 privacy/recovery pointers. |

The coordinator owns shared amendment pointers, ladder/state, the specification
and the brief after the specification writer's handoff. No historical fact,
raw payload, Contact/Inquiry ownership, mutation permission or D-042 capture
behavior changes. Source rows are preference CRUD, not history/audit events.

## 8. Performance checkpoint and observability

[PERF_BASELINE](../design/PERF_BASELINE.md) measured roughly 966 ms average
Today at 100k People/67k attempts with a 60,560-Person book, and ten-connection
pool contention causing 503s. Its explicit follow-up was activity/read-model
work before or with 011c. The [011b evidence](../design/perf/slice-011b-2026-09-06/README.md)
only measured has-phone SQL projections on 50k People **without inquiry/contact
history**. It does not prove five ordered source evaluations are feasible.
The old ladder's “fine to ~50k” assertion is not an accepted latency guarantee.

**Phase A — measured SQL prototype.** The [isolated 011c evidence](../design/perf/slice-011c-2026-09-06/README.md)
covers 50k synthetic People, 46,064 inquiries, 52,306 contact records including
corrections, and 2,843 inbound records. The critical viewer has a 30k-Person
book with 100 retained built-in items, leaving room for source candidates.
The original query design and JIT-only comparison failed under concurrent
load; both remain recorded in the evidence rather than replaced by the final
passing run. No shared data, running API or production code was changed.

The three changes below together completed all **272 critical-matrix** and
**210 supplemental-matrix** requests without unexpected partial results or
failures. The [raw repeated five-source case at concurrency twenty](../design/perf/slice-011c-2026-09-06/metrics/five-source-c20-raw-candidate-guards-jit-off.json)
completed 40/40 requests and 200/200 source evaluations: request p95 3,017 ms,
pool-wait p95 1,622 ms and maximum wait 1,662 ms. An earlier combined run had
pool-wait p95 **1,967 ms**, just 33 ms below the 2-second timeout. Retain that
headroom caveat: additional application work or environmental variation can
exhaust the margin. The newer result does not erase the earlier one.

[Full selected-row and order parity](../design/perf/slice-011c-2026-09-06/metrics/parity-candidate.json)
passed for four built-in books, all five source definitions and a narrow
`assigned_to: me` plus Source filter, using one snapshot/clock. These are
**SQL/prototype results**, not HTTP latency, all production edge cases or a
production-capacity guarantee. Final application verification remains required.

**The approved implementation includes exactly these three planning changes:**

1. In the **new Today source SQL only**, guard the latest-source LATERAL probe
   with the nullable Source-filter parameter when that clause is absent. The
   outer filter already treats an absent Source clause as inactive; avoiding
   its unused probe must not alter matches when Source is present or absent.
   Keep the literal Organization predicate unconditional.
2. In the built-in query, add explicit same-Organization predicates to only
   the latest-inquiry, waiting-inquiry and inquiry-count lookups, matching the
   already trusted Person Organization and enabling existing index prefixes.
   Keep every candidate arm, comparison, tie-break, projection, reason,
   priority, action and sentinel limit otherwise unchanged.
3. Set `jit=off` **transaction-locally** for the complete Today read transaction,
   consistently across Web/Operator and zero-source reads. Do not change PostgreSQL globals,
   pool-wide/session defaults, deployment configuration or existing unrelated
   People/count reads. Commit, rollback, timeout and connection reuse must not
   carry the setting into another query's transaction.

**Fourth planning change (proposed by the coordinator after Phase B run 1;
APPROVED by the user on 2026-09-06, late evening, together with the Phase B
pairing limitation recorded in the verification record):**

4. Set `enable_mergejoin = off` **transaction-locally** in the same Today read
   transaction as change 3. Evidence in the
   [Phase B archive](../design/perf/slice-011c-http-2026-09-06/README.md):
   on the 50k-Person fixture, once autovacuum fills the visibility map,
   PostgreSQL 18 replans the built-in query's per-Person effective-contact
   probe from a Nested Loop Anti Join (about 210 ms for the 30,000-Person
   book) into a Merge Anti Join whose inner side is an index-only scan of the
   entire `contact_attempted_corrects_once` index per Person (about 2,200 ms).
   The frozen original query shows the same 2.6 s in that state, so this is a
   pre-existing Today hazard that the first Phase B run exposed mid-run. The
   toggle pins the fast plan, leaves every source statement's plan and timing
   unchanged, changes no result, and dies with the transaction exactly as the
   JIT setting does. Without it the concentrated cases cannot meet the
   1,250 ms serial cap regardless of source configuration.

These bounded changes do not authorize tuning arbitrary SQL, adding indexes
or introducing materialized activity. Preserve the original query as an executable
comparison fixture in implementation tests. Compare exact candidate content,
reasons and ordering on valid Organization-consistent fixtures, including
inquiry ties, nulls, corrections, reply/outcome precedence, 199/200/201 boundaries
and multiple viewers; keep adversarial tenant-isolation tests. Compare source
membership with 011a for Source present/absent and all v1 clauses. Prove local
JIT restoration after success, failure, rollback and pooled reuse. The prototype
parity run does not replace those production implementation tests.

**Phase B — final HTTP acceptance during implementation.** Repeat the
same representative workload through the completed authenticated HTTP path,
plus Web/Operator parity checks. Cover zero, one dense, one absence and five
overlapping sources; empty/partial/full built-in queues; typical and concentrated
books; history/corrections; warm prepared queries; ten pool connections; and
1/10/20 concurrent requests. Keep §5's 250 ms enumeration, 500 ms whole-source,
2-second acquisition and 100 ms cleanup-grace limits. The final thresholds are:

| Concurrent requests | Maximum request p95 |
|---|---:|
| 1 | 1,250 ms |
| 10 | 2,500 ms |
| 20 | 4,500 ms |

**Every normal sample must complete**, with zero unexpected partial/unavailable
responses, source timeouts, pool timeouts or HTTP 503s. Normal samples have valid
source definitions and available dependencies; intentional failure-injection
cases separately prove the required partial behavior. Request timing includes
authentication, **all** pool acquisition, transaction setup, definition loading
and validation, queries, merge and serialization; do not time only the feed SQL
or exclude slow/failed requests from reported percentiles.

Within each normal benchmark case, whole-source evaluation p95 must also stay
**below 450 ms**, preserving margin within the 500 ms containment deadline.
Count its complete validation/membership/selection/hydration work. The measured
critical and supplemental maxima were approximately 400 ms and 353 ms; the
450 ms requirement is an implementation acceptance target, not a claim that
every source finishes within that time. The user approved these technical
thresholds and budgets with the complete specification on 2026-09-06. The earlier
D-047 answers established the five-source limit and partial-availability policy.

For zero sources, compare paired original and final HTTP paths on the same
fixture, clock, build and concurrency. Final request p95 may regress by no more
than **max(25 ms, 10% of the paired baseline p95)**, while item payload and order
remain exactly equivalent apart from the declared response-envelope addition.
The original baseline keeps its original query/planning policy; do not change
the baseline to make the comparison easier.

Use the evidence harness's declared warm-up and sample protocol, retaining raw
per-attempt results for final HTTP cases. Repeat the critical five-source
concurrency-twenty case with all 40 attempts and 200 source evaluations counted;
report each run's completion counts, request p95, pool-wait p95 and maximum,
plus remaining margin to the 2-second acquisition timeout. Preserve earlier
runs and explain variance; do not cherry-pick the best run, raise budgets/pool
capacity, reduce the fixture or omit a failing scenario to pass. Record exact
code/SQL revision, build, fixture, clock, preparation/warm-up, plans and relevant
buffer/work counts. These are bounded fixture acceptance targets, not a promise
of a particular production user capacity or an external SLA.

The feed path must not routinely produce partial results to hide a performance
regression; a deadline is failure containment, not successful validation. Start
with bounded projections and existing indexes. Any additional index or SQL
change beyond the enumeration above needs a separately reviewed scope amendment.
Do not raise pool size or add materialized activity state to make this rung
appear complete.

**Predeclared stop/split:** if the bounded query approach cannot meet the agreed
threshold, stop and specify a separately reviewed prerequisite performance
rung. Write-time activity columns would touch all attempt/correction/capture/
intake paths, backfill, recovery and query parity; that is not a routine detail
inside this S–M feed slice. Record the blocked acceptance criterion and update
sequencing, without silently reducing matching fidelity or building a read model.
Passing Phase A supports this bounded implementation; passing the
final HTTP and parity gates is required before the slice can be called complete.

Trace `today.query` and source evaluation with safe Organization/actor/list IDs,
static filter kinds, duration, enabled/success/failed source counts, candidate
counts, output count, built-in/list truncation and outcome/error kind. No list
name, filter JSON/value/description, Person content, SQL bound parameters or
debug database errors. No high-cardinality IDs as metric labels. Preserve
path-only request spans and existing PII-free Operator ledger behavior.

## 9. Acceptance criteria and verification

1. **Schema/command boundary:** declared FK/PK/grants; no cross-org reference;
   direct typed callers enforce membership, visibility, revision, valid filters
   and cap. Fresh migration, app-role DB tests and SQLx prepare-check.
2. **Five-source concurrency:** fifth succeeds, sixth conflicts; concurrent
   final-slot enables cannot exceed five; duplicate enable is a no-op at cap;
   disable/re-enable, changed revisions and uncertain repeats behave as §§2/3.
   Synchronized DB tests, including opposing commands and list deletion races.
   A rollback fixture enables five, performs old-binary-style list soft deletion
   without preference cleanup, then proves upgraded configuration shows zero
   and five new enables succeed: dormant rows cannot consume the visible quota.
3. **Privacy/tenant isolation:** other users' personal IDs are indistinguishable
   from missing IDs for member/admin across routes and direct commands; no
   other-org matches; current membership enforced; shared readers can enable;
   deactivation/restore and list deletion cleanup. DB/HTTP/publisher-spy tests.
4. **Filter parity:** all v1 clauses and combinations, symbolic me for two
   viewers, other/unassigned owners, stale Source, inactive assignee, zero
   inquiries, no contact methods, corrections, outbound auto-attempt, null ages
   and exact age boundary. Match against 011a semantics in a common-clock DB
   fixture; an invalid definition never supplies all People or fake zero.
5. **Cap and complete reasons:** duplicate IDs across five sources; built-in
   rows beyond a source's first prefix still get reasons; globally selected
   list items carry every successful matching source; 199/200/201 built-ins;
   overlapping source prefixes; duplicate names; source matches beyond the
   People 500 display cap. Pure merge tests plus DB/HTTP fixtures prove §4.
6. **Built-in regression:** with zero sources, items/content/order match current
   query apart from the declared envelope addition. Inquiry/reply precedence,
   fresh/stale ordering, caller ownership, correction and outcome actions stay
   unchanged; enabling sources never evicts a retained built-in. Existing Today
   suites plus exact original-versus-corrected SQL comparison fixtures under
   §8; all declared content and ordering must agree before a predicate/planning
   change is adopted. Source-probe guard parity covers Source present/absent.
7. **List ordering/actions:** never-first, contact timestamp/UUID tie-break,
   displayed key, no hidden created/list-name/time-enabled sort; four bands;
   low+list stays low; no-inquiry/null payload and no-contact review action.
   Pure tests, API shape pins and Web component tests.
8. **Snapshot/failure:** concurrent source edit/delete/contact cannot mix label,
   revision and membership; one malformed source leaves others usable; actual
   query timeout rollback, enumeration outage, mid-source failure, unrecoverable
   connection, source-budget expiry and cancellation preserve honest statuses
   and release resources. Pin whole-source deadlines and the combined 100 ms
   cancellation/rollback/release grace; unrecoverable connections are discarded
   with remaining sources marked unavailable. Controlled DB barriers/failure
   injection, not sleeps for synchronization.
9. **Web controls/states:** saved-vs-draft enabling, shared-reader controls,
   capacity, retry/conflict, invalid disable, deleted sources, full/partial/empty
   state, multi-reason keys/links, narrow keyboard/focus behavior. Vitest plus
   synthetic browser walkthrough; never only API checks for UI acceptance.
10. **Private cache/recovery:** same-org actor switch, logout/login and delayed
    mutation/read responses never reveal prior names; new query-key factory
    adopted by all Today consumers; successful toggle/list edits/person events,
    entry/focus/manual refresh/60-second timer/reconnect recover current work
    despite unchanged revision or age-only transition. Fake-clock/component
    tests; no names/IDs published on new Organization realtime events.
    Operator component tests additionally retain a sentinel private name in
    reply/history, draft, cards and proposal state, switch actor within the same
    Organization, and prove it is neither visible nor replayed in the next
    request. Cover Organization change and same-actor logout/relogin, plus late
    turn success/error, proposal completion and deferred drawer-open callbacks
    after the switch; cancellation races cannot restore the previous state.

*Amendment recorded at acceptance review (coordinator, 2026-09-06):* the
fail-closed pause above needs an explicit exit. While an auth attempt begun
in another tab is outstanding, AppShell states that a sign-in or sign-out
started elsewhere has not finished and offers one user-driven **Reset and
continue** control. Reset removes every outstanding attempt record, publishes
a fresh settled boundary (which discards private state everywhere) and
re-verifies the actual shared cookie through `/me`; no timer ever releases a
pause. Ordinary verification of the current session renders a neutral loading
state, never that copy. This keeps the privacy posture of this section while
removing a permanent lockout when the originating tab is gone.
11. **Operator parity:** same first item/order/positions/cap as HTTP; list-only
    and outcome rows, ahead counts, truthfully bounded absence, partial-empty
    and no-inquiry cards; malicious names wrapped once without trusted-string
    interpolation; no new tool input or crate edge. Adapter/scripted-provider
    tests, tool-output snapshots and existing boundary fences.
12. **Telemetry/performance:** captured sentinel names/criteria absent, static
    filter kinds and safe outcomes present. Final HTTP evidence meets §8's
    1,250/2,500/4,500 ms request-p95 limits at 1/10/20 concurrency and every
    normal sample completes without unexpected partial/unavailable/503 or
    source/pool timeout; per-case whole-source p95 stays below 450 ms.
    Zero-source paired regression satisfies max(25 ms,10%)
    and exact item/order parity; repeated raw concurrency-twenty results expose
    pool-wait p95/max and remaining margin. Transaction-local JIT policy restores
    after success/failure/rollback and pooled reuse, without changing unrelated
    queries. Preserve all attempts/runs and report unmeasured cases honestly.
13. **Final gates:** on the final implementation tree run `./scripts/check`,
    then `./scripts/check-db` with the sole DB lane; regenerate/review SQLx
    metadata. Independent Astra/xhigh review; record commands and actual results.
14. **Live walkthrough:** two agents, an admin and another Organization; enable
    personal/shared/me/broad lists, demonstrate dynamic entry/exit and persistence
    after contact when criteria still match, preserved built-ins/multiple reasons,
    source failure/recovery, cap message, saved update versus dirty preview,
    deletion and privacy. Use synthetic data and record results.

## 10. Delivery and remaining gate

One primary implementation writer; no simultaneous edits to shared Today,
filter, Operator or Web files. Target size M. If the accepted behavior itself
needs a split, **011c1** is source persistence/configuration routes plus backend
Today/Operator read parity and tests; **011c2** is the Web controls, actor-aware
cache, rendering and walkthrough. The backend contracts must be consumed safely
before enabling the feature; update the brief and coordinated rollout boundary
before exercising this seam. Do not call the complete rung done without both.

The user approved this specification and brief after independent READY review
on 2026-09-06. Implementation, tests and fixes are authorized within the declared
scope. Phase A supports the selected query corrections; §§5/8 define the accepted
containment and final HTTP gates. Phase B HTTP, edge-case/privacy tests, live
walkthrough and final implementation review remain required before completion.
This approval does not authorize commit, merge, push or deployment.
