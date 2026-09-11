# Slice 011d — Tweakable built-in Today rules

**Slice 019b amendment (approved 2026-09-10):**
[Custom-field filtering](SLICE_019b.md). Amends §§2–6: custom-field predicates apply independently to both person-state matrices and the call paths; admin edits/previews and stored-rule fallback carry the new reference errors. Locked anchors and canonical fallback remain binding.

**Status: APPROVED by the user on 2026-09-07 after independent review; implementation deliberately held for a later session.** Approval covers the declared contracts and the seven §1 safe defaults; it does not authorize commit, merge, push or deployment of implementation work.
Prepared on 2026-09-06 against local `main` at `f51bff8` (011c merged and
pushed; 011b-sort merged). Independent review on 2026-09-06 returned READY
WITH CORRECTIONS; every correction is applied below. Branch: `slice-011d-today-system-feeds`.
Coordination, specification and review in this session; implementation lanes
per the [implementation brief](../tasks/SLICE_011d_IMPL.md).

The three rules that put People on Today today — an unanswered inquiry, a
client reply nobody has answered, and a call of yours that still needs an
outcome — stop being compiled-in SQL and become three **system feeds** per
Organization, expressed in the same filter vocabulary as People filters and
saved lists. An Organization admin can adjust each rule's criteria and
freshness window, preview the result for a chosen agent before saving, turn a
rule off, and revert to the default. Every change is recorded as an
append-only fact. Agents see the same Today they see now until an admin
changes something, and then they see that the rule was changed.

Authority: [D-005, D-010, D-022, D-033, D-042, D-043 §3, D-046, D-047](../decisions/DECISION_LOG.md),
the [accepted ladder](../plans/SLICE_011_LADDER.md) including its decision 2
(all three built-ins re-expressed, strict), the
[architecture baseline](../architecture/ARCHITECTURE_BASELINE.md),
[011a](SLICE_011a.md), [011b](SLICE_011b.md), [011c](SLICE_011c.md),
[003](SLICE_003.md), [004](SLICE_004.md), [006c](SLICE_006c.md) and
[009](SLICE_009.md). On 2026-09-06 the user chose to deliver 011d as **one
L-sized rung with parallel backend and web lanes** rather than splitting it,
because the ladder's pre-declared d1/d2 seam does not match the code: the
three arms are one SQL statement with one cap and one precedence rule. See the
[plain-language companion](SLICE_011d_EXPLAINED.md).

## 1. Scope and product behavior

In scope:

- Three new **derived boolean clause kinds** in the v1 filter vocabulary,
  usable everywhere the vocabulary is accepted (People page, saved lists,
  list sources): `awaiting_response`, `client_replied_unanswered` and
  `awaiting_call_outcome`.
- Three **system feeds** per Organization, seeded for existing and new
  Organizations, each carrying an enabled flag, an optional edited definition
  and (for the two person-state feeds) a freshness window in hours. A feed
  with no edited definition uses the **canonical default regenerated from
  code**, so unedited Organizations track future canonical changes.
- Admin-only typed commands and routes: update, revert, enable/disable, and a
  preview that evaluates a candidate definition for a chosen member without
  persisting anything.
- Today evaluation served **only** through the feed path: the three feeds are
  evaluated with the same static-SQL, one-snapshot, savepoint and D-047
  partial-availability posture 011c established for list sources. Priority
  tiers, reason codes and payloads, `waiting_since`, precedence and display
  order are unchanged for an unedited Organization.
- An **equivalence gate**: the compiled-in arms are deleted only after
  database tests prove the feed path reproduces them item-for-item, reason-
  for-reason, in order, at the cap.
- A `today_feed_changed` append-only fact and a member-visible "changed from
  default" marker, so admin edits never become a secret priority (thesis §8).
- A performance re-run of the 011c Phase B harness against the new
  person-state statement, which is also expected to remove the built-in
  query's dependence on the transaction-local merge-join toggle (§8).

Out of scope: per-Organization changes to the **priority tiers or their
order** (fixed system policy in v1; Organizations tweak membership, not
tiers); org-pushed list sources; new reason codes; new Operator tools;
denormalized activity columns (queued separately); tags (011e); a member
picker for anything other than preview; per-member overrides; realtime push of
rule changes; scheduling or time-of-day rules; mobile.

Product rules (safe defaults adopted at specification, veto-able by the user):

1. **Only Organization admins** may view the editing surface or mutate feeds.
   Platform admins have no tenant access and cannot. Members see a read-only
   summary of the effective rules.
2. **Disabling** any feed is allowed. Disabling the unanswered-inquiry feed
   requires typed confirmation in the Web client, and every member's Today
   shows that the rule is off.
3. The two person-state feeds must keep an `assigned_to` clause that includes
   `me`. An admin may add assignees (for example `unassigned`) but cannot
   remove viewer-relativity, so an edit cannot flood every member's Today with
   the whole Organization's inquiries. The call feed is viewer-relative by its
   axis (the caller is the viewer) and needs no such rule.
4. Each feed's **anchor clause** (its derived axis with `value: true`) must be
   present and cannot be negated or removed; any other clause within the
   existing validation caps may be added, changed or removed.
5. **Preview** evaluates the candidate definition for a chosen active member
   of the Organization, defaulting to the admin. Person data is Organization-
   visible (D-005); no personal list or other private data is involved.
6. If a **stored** definition becomes invalid after saving (a referenced
   stage was deleted, or the stored JSON is unsupported by the running
   binary; a deactivated assignee is still a member and stays valid, 011a
   §4b), Today evaluates that feed's canonical
   default and reports the fallback in the response and on the admin page.
   Availability of the core queue beats strict skipping.
7. All three v1 system feeds, the call feed included, consider only People
   with at least one inquiry, exactly as the compiled-in query does today
   (`latest.id IS NOT NULL` applies to every arm). This is a
   declared, pinned feed-evaluation constraint, not part of any clause's
   meaning; lifting it is a later, separately reviewed change.
   *Amendment pointer (Slice 016b, 2026-09-09, D-054):* this constraint is
   not lifted; the built-in task axis is not a feed and runs without it
   (a recorded temporary exception to "every built-in reason is a
   tweakable feed"). See [SLICE_016.md](SLICE_016.md) §5.

## 2. Vocabulary extension (amends 011a §4)

Three boolean clause kinds join `Clause`, all `{"kind": K, "value": bool}`
like `has_replied`, one per kind, counted against the 20-clause cap:

| Kind | True when (evaluated at the one Today or People clock) | Viewer-relative |
|---|---|---|
| `awaiting_response` | The Person has at least one inquiry whose `received_at` is later than the Person's effective last contact attempt by **any** member, or has inquiries and no attempt at all. Equivalent to the compiled-in `waiting.received_at IS NOT NULL`: "exists an inquiry after X" is "max(received_at) > X", and the effective-attempt maximum equals the plain maximum because corrections inherit `occurred_at` (011a §4c). | No |
| `client_replied_unanswered` | The Person's latest inbound correspondence exists and is later than both the effective last contact attempt and the latest outbound correspondence. Exactly the compiled-in `by_reply` predicate without its assignment term. Relation to `has_replied` (011a §10d): `has_replied` is "ever replied"; this is "replied and still unanswered". | No |
| `awaiting_call_outcome` | A call by the **viewer** to the Person with status `ended` or `failed` and a non-null `ended_at` exists whose root automatic contact attempt (`causation_id = call.id`, `corrects_id IS NULL`) has no correction. Exactly the compiled-in `outcome_call` membership. | Yes: the viewer is the caller |

`value: false` negates each predicate. `describe()` lines: "Awaiting a
response" / "Not awaiting a response"; "Client replied, unanswered" / "No
unanswered client reply"; "A call of mine needs an outcome" / "No call of mine
needs an outcome". `kind_label()` returns the wire kind.

`PersonFilterParams` gains three `Option<bool>` parameters and, for the first
time, a bound **viewer id** used only by the call probe. `to_query_params`
keeps resolving `me` into the assignee array; the viewer is bound separately.
All **eleven** statements that share the parameter set gain the same
NULL-guarded predicates in the same positions: `filtered_summaries` and its
seven sorted copies, `count_filtered_matches`, `source_membership` and
`source_candidates`. The new probes (latest outbound correspondence, the call
existence test) are guarded on their parameter so an absent clause adds no
work. **Absent clauses keep every statement byte-identical in results**,
pinned by parity tests per axis across People, count, every sort and both
Today source statements. No dynamic SQL; the Organization predicate stays
literal text; `.sqlx` metadata is regenerated for the eleven statements.

*Amendment pointer (Slice 011e, 2026-09-07):* the statement count above is
superseded. §5 of this specification added three system-feed statements
(`person_state.sql`, carrying the parameter chain twice, `call_membership.sql`
and `call_only.sql`) that bind the same `PersonFilterParams`, so **fourteen**
statements share the parameter set on `main`. Slice 011e appends its `tags`
and `not_tags` predicates to all fourteen. See [SLICE_011e.md](SLICE_011e.md) §4.

The Web filter bar gains three boolean chips (§6). Saved lists and list
sources accept the kinds with no further change; a list using
`awaiting_call_outcome` as a Today source binds the viewer as the caller, like
`me`.

## 3. System feed persistence

One additive migration owned by the backend lane creates:

```text
today_system_feed (
  organization_id      UUID NOT NULL REFERENCES organization (id),
  feed_key             TEXT NOT NULL CHECK (feed_key IN
                         ('unanswered_inquiry','client_replied','call_outcome_needed')),
  enabled              BOOLEAN NOT NULL DEFAULT true,
  filter               JSONB NULL CHECK (filter IS NULL OR jsonb_typeof(filter) = 'object'),
  fresh_within_hours   INTEGER NULL CHECK (fresh_within_hours BETWEEN 1 AND 8760),
  revision             BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
  updated_by_user_id   UUID NULL REFERENCES app_user (id),
  created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (organization_id, feed_key),
  CHECK (feed_key <> 'call_outcome_needed' OR fresh_within_hours IS NULL)
)
GRANT SELECT, INSERT, UPDATE ON today_system_feed TO crm_app;   -- no DELETE
```

`filter IS NULL` means "use the canonical default from code"; likewise a
NULL `fresh_within_hours` means the canonical 24 hours. `is_default` is
`filter IS NULL AND fresh_within_hours IS NULL`. Revert sets both NULL. The
migration also **backfills** one row per feed for every existing Organization
with NULL definition columns, so it embeds no definition. `create_organization`
seeds the same three rows beside stage seeding with `ON CONFLICT DO NOTHING`.
The read path treats a **missing** row as canonical-enabled, so an
Organization created by an older binary after the migration still gets its
rules. Membership deactivation, list deletion and Person changes never touch
this table.

Canonical defaults, regenerated from code by `canonical_default(feed_key)`:

| Feed | Canonical definition | Freshness | Reasons it can emit |
|---|---|---|---|
| `unanswered_inquiry` | `[assigned_to: [me], awaiting_response: true]` | 24 h | `new_inquiry` (if fresh), `no_contact_attempt`, `repeat_inquiry` |
| `client_replied` | `[assigned_to: [me], client_replied_unanswered: true]` | 24 h | `client_replied` |
| `call_outcome_needed` | `[awaiting_call_outcome: true]` | none | `call_outcome_needed` |

Freshness is a feed parameter, not a clause. It decides `high` versus `normal`
and the `new_inquiry` reason, exactly as the compiled-in 24-hour window does:
for the inquiry feed the test is `latest inquiry received_at > now - window`;
for the reply feed `latest inbound occurred_at > now - window`. Both strict.
The window binds as `make_interval(hours => $h)`, which equals the current
literal for 24.

The same migration creates the append-only fact:

```text
today_feed_changed (
  <standard D-015 envelope: id, organization_id, actor_kind, actor_user_id,
   on_behalf_of_user_id, origin, occurred_at, recorded_at, correlation_id,
   causation_id, corrects_id>,
  feed_key                  TEXT NOT NULL (same CHECK),
  change                    TEXT NOT NULL CHECK (change IN ('updated','reverted','enabled','disabled')),
  from_revision             BIGINT NOT NULL,
  to_revision               BIGINT NOT NULL,
  enabled_after             BOOLEAN NOT NULL,
  filter_after              JSONB NULL,        -- NULL = canonical
  fresh_within_hours_after  INTEGER NULL
)
GRANT SELECT, INSERT ON today_feed_changed TO crm_app; append-only triggers as the other facts.
```

It is PII-free: ids, clause JSON (ids and tokens only) and integers. Seeding
and backfill write no fact. `db_schema.rs`'s fact-table enumerations gain it.

## 4. Commands and preview

Typed commands take server-owned `CommandContext`. Each runs in one
transaction: lock and re-check the actor's current active membership with
011b's `FOR SHARE` posture and require `Role::Admin` from that locked row, take the Organization advisory lock in a new
`today-feeds:` namespace, then read the feed row `FOR UPDATE`. Typed callers
cannot bypass authorization or validation.

```text
UpdateTodaySystemFeed     { feed_key, expected_revision, filter: FilterDefinition,
                            fresh_within_hours: Option<i32> }
RevertTodaySystemFeed     { feed_key, expected_revision }
SetTodaySystemFeedEnabled { feed_key, expected_revision, enabled: bool }
```

Update requires `version == 1`, structural `validate()`, `validate_references`
against the Organization (011b's stage/assignee checks), and the feed rules of
§1: the anchor clause present with `value: true`; for the two person-state
feeds an `assigned_to` clause containing `me`; `fresh_within_hours` present in
`1..=8760` for those feeds and absent for the call feed. A definition and
window equal to the stored ones at the current revision is a successful
no-op (`changed:false`, no fact). Otherwise store the definition (or NULL when
it equals the canonical default **and** the window is 24, so a hand-typed
default reads as default), bump `revision`, set `updated_by_user_id` and
`updated_at`, and write one fact. A stale `expected_revision` conflicts.

Revert sets both definition columns NULL, bumps the revision and writes a
`reverted` fact; reverting a default feed is a no-op. Enable/disable writes
`enabled`/`disabled` facts; setting the current state is a no-op. All three
require `expected_revision`.

**Preview** is a read, admin-only, never persisted:

```text
PreviewTodaySystemFeed { feed_key, filter, fresh_within_hours, subject: UserId }
```

It validates like Update (the definition need not be saved), verifies the
subject is a current active member of the actor's Organization, then evaluates
**that one feed alone** for that subject at one clock, meaning the feed with
the other person-state feed treated as disabled (so a Person in both feeds
previews under feed A with inquiry reasons even though Today would show the
reply reason): the same statements as
§5 with the candidate definition bound, ordered as the feed orders inside
Today, capped at 200 with `truncated`. Items carry the real reasons and
priorities that feed would produce; no list sources and no other feeds take
part. It runs in a read-only transaction with §5's `SET LOCAL` policy and a
statement timeout of 1,250 ms; exceeding it is an error, never a partial
result. Preview returns `describe()` lines for the candidate definition.

## 5. Today evaluation through feeds

Keep one `today::query` path for HTTP and Operator callers, the one
repeatable-read read-only transaction, one `now = statement_timestamp()`, and
the transaction-local `jit = off` and `enable_mergejoin = off` settings from
011c §8. Inside it, in order:

1. **Load the three feed rows** (missing row = canonical enabled). Decode
   and validate each stored definition; a decode or reference failure marks
   that feed `fallback` and substitutes the canonical default (§1 rule 6).
   A read failure here is a 503, as a built-in failure is today.
2. **Person-state statement** (feeds `unanswered_inquiry` and
   `client_replied`, one static statement, `query_file_as!`): binds both
   feeds' parameter sets, enabled flags and windows. Membership is
   `(enabled_a AND matrix_a AND awaiting) OR (enabled_b AND matrix_b AND
   replied)`, plus the §1 rule 7 inquiry constraint. `by_reply` wins
   precedence when both hold, evaluated against feed B's **edited**
   definition. Ordering is the compiled-in order: `fresh DESC`, then the
   reply timestamp for reply members or the waiting inquiry's `received_at`
   for inquiry members, then `id`, `LIMIT 201`. Membership uses plain
   `max()` probes over the existing indexes. The ordering and tier inputs
   are computed **before** the limit, in the inner statement: the waiting
   key is the **earliest** inquiry later than the last contact
   (`min(received_at) WHERE received_at > last_contact`), and freshness
   uses `max(received_at)` or `max(inbound occurred_at)`. Only the
   effective-attempt correction chain, latest inquiry ref, inquiry count and
   summary columns are hydrated in an outer query over the at-most-201
   prefix, which is the structural fix for the planner hazard (§8). One statement rather than one
   per feed because the reply-wins precedence and the shared 201-row cap are
   only exact when both arms are ordered together (a Person in both feeds
   sorts by the inquiry key in feed A alone but by the reply key globally).
   This statement failing is a 503.
3. **Rank** the rows with the existing `rank()` into `high`/`normal` items
   with their reasons; retained set **P** (at most 200), `truncated_p`.
4. **Call feed** (`call_outcome_needed`) under a savepoint with the 011c
   500 ms whole-source budget and statement-timeout discipline. Both of its
   statements carry the §1 rule 7 inquiry constraint and select **one call
   per Person, the most recent by `ended_at DESC, id DESC`**, as the
   compiled-in `outcome_call` does. (a) For every retained P id, membership
   with payload (`call_id`, `ended_at`), appending `call_outcome_needed`
   last. (b) Only if the person-state statement was **not truncated**
   (at most 200 rows), the call-only prefix ordered `ended_at ASC, id ASC`,
   limited to `(200 − |P|) + 1`, hydrated, as `low` items with action
   `set_outcome` and `waiting_since = ended_at`; the extra row only sets
   `truncated`, exactly as the compiled-in 201-row sentinel does when 200
   person-state rows are followed by a call-only row. If the person-state
   statement was truncated, skip (b); never admit a discarded row.
   A failure here is reported as `partial` with a `system_feed_issues` entry
   and available work is returned (D-047). A disabled call feed contributes
   nothing and no issue.
5. Built-in set **B = P ∪ call-only**, `K = 200 − |B|`; then list sources
   exactly as 011c §4, unchanged.

A disabled person-state feed contributes no members and no issue. Tiers,
reason codes and payloads, `waiting_since`, `latest_inquiry`,
`last_contact_attempt`, `recommended_action` and the four-band display order
are unchanged. `TodayReason` gains no variant (*superseded by Slice 016b,
2026-09-09, D-054: `task_due` and `task_overdue`; see
[SLICE_016.md](SLICE_016.md) §5*). `rank.rs` remains the built-in
mapper; a new pure merge function owns P/call-only/list assembly.

`GET /api/today` keeps its shape; `sources` gains one additive field:

```text
sources: { status, issues: [SourceIssue,...],
           system_feed_issues: [ { feed_key, error: "unavailable" | "invalid_definition",
                                   fallback: bool }, ... ] }
```

`status` is `partial` when either list is non-empty, except that 011c's
`unavailable` (list-source enumeration failed) keeps precedence; the early
return paths that build `sources` on enumeration failure carry
`system_feed_issues` too (*Slice 016b, 2026-09-09: the token `task_due`,
which has no feed row, joins the issue keys with `error: unavailable,
fallback: false`; see [SLICE_016.md](SLICE_016.md) §5*). `fallback:true` means
the canonical default was evaluated in place of an invalid stored definition
(`invalid_definition`), `false` that the feed contributed nothing
(`unavailable`). No definition JSON or names appear.

## 6. HTTP, Web and Operator

### Routes (additive; SLICE_004 §5 admin routes, SLICE_003 §5 Today pointer)

Admin routes require `OrgAdminContext` on the route **and** the in-transaction
re-check of §4. Bodies deny unknown fields; 128 KiB ceiling; 011b's positive
safe-integer revision validation; error precedence 401 → 403 (the
`OrgAdminContext` extractor runs before body parsing, as on every existing
admin route) → 400 `malformed_request` → 404 `not_found` (unknown
`feed_key`) → 409 `today_feed_conflict` →
422 with the filter-error code (`unsupported_filter|invalid_stage|invalid_assignee`)
or `invalid_feed_rule` (anchor/`me`/window violations) → 503 `unavailable`.

*Amendment pointer (Slice 011e, 2026-09-07, declared additive, AGENTS.md
§11):* the filter-error code set here and in `Feed.filter_error` also admits
`invalid_tag`; a stored feed definition naming a deleted tag falls back to the
canonical default like a deleted stage. See [SLICE_011e.md](SLICE_011e.md) §4.

| Method/path | Input | Success |
|---|---|---|
| GET `/api/organization/today-feeds` | none | 200 `{"feeds":[Feed,Feed,Feed]}` in fixed key order |
| PUT `/api/organization/today-feeds/{feed_key}` | `{expected_revision, filter, fresh_within_hours}` | 200 `{"feed":Feed,"changed":bool}` |
| POST `/api/organization/today-feeds/{feed_key}/revert` | `{expected_revision}` | 200 `{"feed":Feed,"changed":bool}` |
| PUT `/api/organization/today-feeds/{feed_key}/enabled` | `{expected_revision, enabled}` | 200 `{"feed":Feed,"changed":bool}` |
| POST `/api/organization/today-feeds/{feed_key}/preview` | `{filter, fresh_within_hours, subject_user_id?}` | 200 `{"subject":UserRef,"items":[TodayItem,...],"truncated":bool,"description":[...]}` |
| GET `/api/today/feeds` (any active member) | none | 200 `{"feeds":[MemberFeed,MemberFeed,MemberFeed]}` |

```text
Feed       = { feed_key, enabled, revision, is_default, filter, fresh_within_hours,
               description: [string], filter_error: null | code,
               updated_at, updated_by: UserRef | null,
               default: { filter, fresh_within_hours } }
MemberFeed = { feed_key, enabled, is_default, description: [string] }
UserRef    = { id, display_name }
```

`filter` in `Feed` is the **stored typed definition** (the canonical one
when `is_default`), so an admin can see and repair a rule whose reference
broke; it is `null` only when the stored JSON is `unsupported_filter`.
`filter_error` marks structural or reference invalidity; the page derives
"effective rule = default" from a non-null `filter_error`. `description`
describes the same object, with 011b's neutral placeholders for unresolvable
names. `updated_by` is null only when never edited; a deactivated editor's
`app_user` row persists and still resolves. `fresh_within_hours` may be
absent or `null` for the call feed (both mean none); absent or `null` on a
person-state feed is 422 `invalid_feed_rule`. Canonical collapse is
per column: `filter` stores NULL iff it equals the canonical definition by
typed, order-sensitive equality; `fresh_within_hours` stores NULL iff 24;
`is_default` is both NULL. `MemberFeed` reflects the **effective** rule
(canonical under fallback); the fallback itself is reported through Today's
`system_feed_issues`, not here. A preview subject outside the Organization,
inactive, or unknown returns the same 404 as an unknown id. Member reads
expose no editor identity, revision or raw JSON. Everything is
Organization-scoped from the session; no client Organization id.

### Web (web lane)

- **Filter bar:** three boolean chips in the existing style, available on
  People and list editing; `describe()` wording as above.
- **Manage → Today rules** (`/manage/today-feeds`, admin-only route meta, nav
  beside Intake and Members): one card per feed in fixed order showing the
  effective rule as chips, freshness (hours) for the two person-state feeds,
  status (default / customized by *name* on *date* / off / using default
  because the saved rule is invalid), and controls Edit, Preview, Revert,
  Turn off / Turn on. Edit opens the filter editor with the anchor chip and
  the `me` assignee **locked** (visible, not removable); Preview shows the
  §4 result in the Today row rendering with a member picker defaulting to
  the admin, with an honest empty state and the 200 cap notice. Save is a
  PUT with the loaded revision; a 409 reloads the feed for review and needs
  a new click. Revert and Turn off confirm; turning off the unanswered-
  inquiry feed requires typing the feed name. No auto-retry; uncertain
  mutations refetch.
- **Today:** the Manage sources panel gains a **Rules** section listing the
  three feeds from `GET /api/today/feeds` with "Default", "Changed by your
  admin" or "Off" markers and the description lines. A `system_feed_issues`
  entry renders in the existing partial notice, generalized to "Some Today
  rules or sources could not load"; a `fallback:true` entry reads "The
  *feed* rule is invalid; the default rule is being used". Query keys:
  `todayFeeds(org, actor)` under the existing Today prefix; admin feeds under
  `['org', org, 'today-feeds-admin']`. Mutations invalidate both and Today.
- Reuse D-045 controls, focus management, the 011b session-identity fence
  and the actor-aware Today keys. No names or rule JSON on the realtime
  channel.

### Operator

No new tool and no input change. `TodaySourcesView` (and its copies on
`NextWorkItem`, `PersonDetail` and both `PriorityExplanation` variants) gains
the additive `system_feed_issues` list with the same fields; a partial result
remains "available work". `explain.rs` and `ORDERING_RULE` are unchanged
because reasons, tiers and order are unchanged. Feed definitions never enter
prompts; feed keys are static tokens. Declared in SLICE_005 §§3/7 pointers.

## 7. Contract declaration and amendment ownership

Approval of this specification satisfies AGENTS.md §11 for:

| Previous → proposed | Reason / affected | Compatibility and required amendment |
|---|---|---|
| Ten clause kinds → thirteen (§2) | D-043 §3, ladder decision 2; crm-app filter, eleven statements, Web FilterBar | Additive; unknown kinds already fail closed on older binaries (011a §4). 011a §§4a/4c/4d/4e and 011b §4 pointers. |
| `PersonFilterParams` without viewer → viewer bound for the call probe | Viewer-relative call axis | Internal; `me` resolution unchanged. 011a §4c pointer. |
| Compiled-in Today arms → §§3/5 seeded system feeds evaluated through the feed path | D-043 §3; crm-app Today, migration, create_organization | Items, reasons, tiers, order and payloads unchanged for default feeds (§9 equivalence). 003 §§3/4/14a, 006c §5a, 009 §6 amendment pointers; the original SQL remains an executable comparison fixture. |
| No feed configuration → §§3/4/6 table, fact, commands, six routes | Admin control with audit; API/Web | Additive routes, same envelope. 004 §5 and 003 §5 pointers. |
| `sources` envelope → `+ system_feed_issues` (§5) | D-047 for feed failures; API/Web/Operator | Additive field; `status` semantics extended. 003 §5, 011c §5 and 005 §§3/7 pointers. |
| 011c §8 built-in query corrections and toggles → new person-state statement | Feed evaluation; planner hazard | Both `SET LOCAL` settings kept unless §8 evidence shows the new statement is plan-stable without `enable_mergejoin = off`; dropping it is a recorded decision in the verification record, not silent. |

The coordinator owns the amendment pointers, ladder, state, this
specification and the brief. No historical fact, Person ownership, mutation
permission, D-042 capture behavior or `PersonVisibilityScope` changes.

## 8. Performance and observability

*Amendment pointer (D-050, 2026-09-07):* this section's absolute request-p95
caps, whole-source limit, pool-wait headroom and the merge-join toggle
question are superseded by [D-050](../decisions/DECISION_LOG.md). The slice
gates only on evidence item 1 (paired `Legacy` versus `Feeds` regression) and
the person-state `EXPLAIN` in item 4 showing index use and no super-linear
growth; items 2 and 3 and the toggle comparison are reported for trend-
watching, never gated, and both `SET LOCAL` settings stay as they are.

Baselines: [PERF_BASELINE](../design/PERF_BASELINE.md) and the 011c
[Phase B archive](../design/perf/slice-011c-http-2026-09-06/README.md) (50k
People, 30k-Person concentrated book, five-source concurrency-20 p95 2,710 ms
against 4,500 with the merge-join toggle). The planner hazard is recorded
there: with a filled visibility map, PostgreSQL 18 replans the compiled-in
per-Person effective-attempt anti-join into a Merge Anti Join over the whole
corrections index per Person.

Required evidence, produced by the backend lane with the existing
`db_today_http_perf.rs` harness, fixture and protocol, all runs retained:

1. **Paired zero-source regression** against the `Legacy` provider of §9.2
   (the 011c-final path) in the same build, on the same fixture and clock: request p95 may regress by at most
   **max(25 ms, 10%)**; items and order exactly equal apart from the declared
   envelope field.
2. The 011c matrix at 1/10/20 concurrency with the same **1,250 / 2,500 /
   4,500 ms** request-p95 caps, every normal sample complete, whole-source
   p95 below 450 ms, five-source concurrency-20 repeated with pool-wait p95,
   maximum and margin reported.
3. New cases: feed A customized with a stage clause; feed A disabled; feed C
   disabled; preview p95 for the concentrated book (cap 1,250 ms serial).
4. `EXPLAIN (ANALYZE, BUFFERS)` of the person-state statement on the
   concentrated book **with a filled visibility map**, with and without
   `enable_mergejoin = off`. If plan and timing are stable without the
   toggle, the coordinator may record its removal in the verification record
   and the 011c §8 pointer; otherwise both toggles stay.

No new index, pool change, materialized activity or SQL beyond the statements
enumerated here without a separately reviewed amendment. A deadline is
containment, not validation; routine partial results are a failure.

Tracing: `today.query` gains per-feed status (`default | customized |
disabled | fallback`), person-state and call-only candidate counts; feed
commands and preview use `#[instrument(skip_all, fields(organization_id,
actor_id, feed_key, filter_kinds, outcome))]`. Never the definition JSON, a
Person, a subject's items or bound parameters.

## 9. Acceptance criteria and verification

1. **Vocabulary parity:** per axis, present-true, present-false and absent,
   across all eleven statements, on a common-clock fixture with corrections,
   zero-inquiry People, outbound-only correspondence, calls by two callers,
   corrected call attempts and cross-tenant rows; absent equals the pre-011d
   statement byte-for-byte. Unit tests for validate, describe, kinds field
   and params; `db_people_filter.rs` extended.
2. **Equivalence gate (before arm deletion):** the backend lane first lands
   the feed path behind a **built-in provider seam** inside the Today query
   (`Legacy | Feeds`, selectable only under `test-support`, default
   `Feeds`), keeping the compiled-in statement as the `Legacy` provider so
   the whole path, list-source merge included, runs both ways. It then
   proves on generated fixtures and the existing frozen `today_9d62e86`
   fixture that `today::query_at` under `Feeds` equals `Legacy` in full
   item JSON, order and `truncated`, with zero list sources and with sources
   enabled: fresh boundary at exactly `now − window` for **both** person-
   state feeds (not fresh); reply equal to last attempt or last outbound
   (excluded); correction chains; dual inquiry-and-reply People beyond 201;
   person-state 199/200/201 with 0..3 call-only rows; a zero-inquiry Person
   with a call awaiting outcome (excluded, rule 7); two qualifying calls for
   one Person by the same viewer (most recent selected); caller ≠ assignee
   across two viewers; two tenants; deactivated caller; corrected call
   outcome leaves; `repeat_inquiry` counts; call feed disabled/enabled.
   Only after this passes and the reviewer confirms may the `Legacy`
   provider be deleted, with its SQL frozen under
   `tests/fixtures/today_f51bff8/` in the existing README/SHA pattern.
3. **Schema/commands:** migration on a fresh database and on a database with
   existing Organizations (backfill count = organizations × 3); seed on
   create; missing-row canonical read; grants (no DELETE on feeds; fact
   append-only triggers); revisions and 409, including Update and Revert racing on the same
   `expected_revision` (exactly one succeeds); no-op detection; per-column
   canonical collapse to NULL; fact per change with the correct envelope and none on
   no-op, seed or backfill; §1 feed rules enforced by typed callers.
4. **Authorization/tenant isolation:** member 403 on every admin route and
   direct command; platform-only session 401 on the admin routes and on
   `GET /api/today/feeds`; admin of Organization B cannot
   read, edit, preview or affect Organization A (same 404 for its feed
   state); preview subject from another Organization or inactive is 404;
   demoted admin loses access in the same transaction the command re-checks;
   `db_admin.rs`'s admin-route enumeration extended.
5. **Evaluation:** customized feed A (stage clause, extra assignee) and
   customized window (1 h, 8760 h) produce the expected members, tiers and
   reasons; disabled feeds contribute nothing and no issue; a Person in both
   person-state feeds shifts to inquiry reasons and the inquiry key when
   feed B is disabled or its edited definition excludes them; a stored
   definition invalidated by a deleted stage or unsupported JSON falls back
   with `fallback:true`; call-feed failure injection yields `partial` with
   an issue and intact person-state work; call-feed failure together with
   list-source enumeration failure yields `unavailable` with the system
   feed issue still present; person-state failure is 503; both `SET LOCAL` settings restore after success, failure,
   rollback and pooled reuse.
6. **Preview:** equals Today's contribution for the subject on the same
   fixture and clock when Today is computed with the other person-state
   feed disabled (§4); cap and truncated; timeout is an error; nothing
   persisted; subject default is the admin.
7. **Web:** chips; locked anchor and `me` in the editor; preview with member
   picker and empty state; revert/off confirmations including the typed
   confirmation; 409 reload; members' Rules section markers; fallback and
   partial notices; keyboard/focus; Vitest plus a synthetic browser
   walkthrough recorded under `docs/design/qa/slice-011d-<date>/`.
8. **Operator parity:** same items as HTTP under customized, disabled and
   fallback feeds; `system_feed_issues` present on every view carrying
   source state; no new input; existing crate fences pass.
9. **Telemetry:** feed statuses and counts present; definition JSON, names
   and subject items absent.
10. **Performance:** §8's four evidence items, all runs retained under
    `docs/design/perf/slice-011d-<date>/`.
11. **Final gates:** on the final merged tree the coordinator runs
    `./scripts/sqlx-prepare`, `./scripts/check` and `./scripts/check-db` once;
    lanes run them per round on their own worktrees. Independent review by
    the reviewer and adversarial analysis by the tester, both read-only.
12. **Live walkthrough:** an admin edits feed A with a stage clause, previews
    for an agent with a full book, saves, and the agent's Today changes with
    the Rules marker; revert restores; turning off feed A shows Off to
    agents; a second Organization is unaffected throughout.

## 10. Delivery

Two lanes in two worktrees, one writer each, contracts frozen by this
specification:

- **Lane B (backend, owns the migration and all SQLx metadata):** filter
  vocabulary and the eleven statements; feed model, canonical defaults, seed,
  backfill, fact; commands, preview, routes, Operator view field; feed-path
  evaluation and merge; equivalence gate; performance evidence; all Rust and
  database tests.
- **Lane W (web):** chips, Today rules page, editor locking, preview,
  confirmations, Today Rules section and notices, query keys, Vitest and the
  walkthrough. Lane W codes against §6's contracts and may stub the API in
  tests until Lane B's routes exist in the integration branch.

Neither lane edits the other's files; `web/src/api/types.ts` belongs to Lane
W and mirrors §6 verbatim. Sizing: Lane B M+, Lane W M, the rung L by the
user's explicit choice. Arm deletion, the toggle question and any contract
drift are reported to the coordinator, never resolved inside a lane. This
specification authorizes nothing until the user approves it after
independent review; approval will authorize implementation and tests, not
commit, merge, push or deployment.
