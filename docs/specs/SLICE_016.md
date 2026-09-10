# Slice 016 — Tasks

**Status: APPROVED by the user on 2026-09-09 ("commit and then Go for
016a") after independent review and D-054.** Approval covers the declared
contracts (§10) and the §1 safe defaults; it authorizes implementation of
rung 016a after the Phase 6 gate (passed 2026-09-09), not commit, merge,
push or deployment; rung 016b needs its own gate. Implementation per the
[implementation brief](../tasks/SLICE_016_IMPL.md). Prepared
against local `main` at `6525308` (Slice 015 merged, pushed and serving on
the shared development runtime). Independent review on 2026-09-09 returned
READY WITH CORRECTIONS and the adversarial test analysis added sixteen
items; every correction and every adopted item is applied below (the
load-bearing ones: task-only items carry `waiting_since: null` and join the
built-in set before the list stage; the axis fails all-or-nothing on its
own budget; `low` items are never raised; the Web types the new issue token
and falls back to a generic label; Complete on the Person page only where
`can_manage`). No blocking decision remains.

**Tasks** on a Person: a typed to-do with a due time and an assignee,
created by any member from the Person page, completed with one click from
the Person page or from Today, shown on the Person timeline once completed,
surfaced on Today both as a reason in the ranked queue ("overdue task") and
as the industry-familiar Overdue / Due soon panel with complete and snooze.
Tasks are the last unbuilt CRM-core model (thesis §11) and the destination
the parked FUB migration's tasks rung needs.

Authority: [D-004, D-005, D-010, D-015, D-021, D-022, D-023, D-027, D-029,
D-033, D-034, D-043, D-045, D-047, D-050, D-051, D-052, D-053,
D-054](../decisions/DECISION_LOG.md) (D-054 is the accepted decision on the
Today shape and records the temporary exception to D-043/011d), the
[architecture baseline](../architecture/ARCHITECTURE_BASELINE.md), AGENTS
§4.6 (tasks are relational CRUD) and §4.8 (`CreateTask` is a named typed
command), [002](SLICE_002.md) §§2/5/6, [003](SLICE_003.md) §§3/5/6,
[005](SLICE_005.md) §5, [011c](SLICE_011c.md) §§4/5, [011d](SLICE_011d.md)
§§1/5/6/8, [013](SLICE_013.md) §2, [015](SLICE_015.md) (the new-model
pattern this slice copies) and [UI_STYLE](../design/UI_STYLE.md).

## 1. Scope and product behavior

In scope, two sequential rungs (§13):

- **016a — the task model and the Person page.** A `task` table and
  `TaskId`; typed commands `CreateTask`, `UpdateTask`, `CompleteTask`,
  `ReopenTask`, `SnoozeTask`, `DeleteTask`; six routes nested under
  `/api/people/{person_id}/tasks`; `tasks[]` (open) on the Person detail
  read; a `task_completed` history kind; a `task_changed` Person change on
  the existing realtime event; Operator `PersonDetail.tasks` (open, untrusted
  titles) with the tool view's `history` excluding the kind; Web: a Tasks
  card on the Person page above History and completed rows in the timeline.
- **016b — Today.** The fixed built-in task axis in `today::query` (D-054
  §1) with reasons `task_due` and `task_overdue`; the member-level read
  `GET /api/tasks?scope=mine` for the Today panel; Operator Today
  explanations for the two reasons and the ordering rule; Web: the Today
  item's task badge with a Complete button, the Tasks panel (Overdue / Due
  soon, complete, snooze to tomorrow), and the generic fallback for an
  unknown `system_feed_issues` key.

Out of scope (LATER unless stated): a task body or description (additive
nullable column when 010f verifies FUB carries one); recurrence; priority;
reminders, push or email notifications (first to pull forward with mobile);
action plans that auto-create tasks; the task filter-clause family
(`has_open_task`, `task_due_within`, `task_overdue`) and a `next_task_due_at`
D-052 column, which together close the D-054 exception by turning the axis
into feed four (**trigger, also:** after a FUB import, hundreds of stale
overdue tasks would enter the `high` tier ahead of every `normal` inquiry
item; the feed-four rung or an overdue horizon is the remedy); an
admin-tweakable or disable-able task feed; a per-Organization lookahead
window; a full `/tasks` page and an Upcoming group; tasks for other members
on Today (admin overview); personal tasks with no Person; Operator
`create_task` / `complete_task` (next S rung, own decision); the FUB task
importer (schema-ready only); appointments and calendar; mobile;
Members-page open-task counts for deactivated members; a client-minted
`TaskId` for an idempotent create.

Product rules (rules 1–4 follow D-054 §3; all are safe defaults adopted at
specification, veto-able by the user):

1. **Any active member creates a task on any visible Person, assigned to
   any active member. The assignee, the creator, or an Organization admin
   edits, completes, reopens, snoozes or deletes it.** Other members read
   only. The assignee defaults to the creator. A member who reassigns a task
   they do not also own loses `can_manage` on it (the creator keeps it).
   Imported tasks whose assignee and creator matched no member (§2) are
   admin-only until reassigned. A deactivated assignee's tasks stay, keep
   the inactive member's name on the Person page, appear on nobody's Today,
   and an admin reassigns them (O-004 stays open; D-027 §2). Any member may
   therefore put a Person on another member's `high` tier by assigning them
   an overdue task; this is the FUB posture and D-054 §3 accepts it.
2. **A task is a single-line title, a kind, an optional due time and an
   assignee.** Title trimmed, 1–500 characters, no control characters at
   all (single line; `\n` and `\t` rejected, unlike a note body). Kind is a
   closed enum `call | email | text | follow_up | other`, default
   `follow_up`; it drives the recommended action on Today and is the FUB
   `type` destination. `due_at` is an instant (`TIMESTAMPTZ`; the wire
   carries RFC 3339 in any offset, responses are `Z`); a date-only pick is
   converted **by the client** to local end of day (23:59:59 in the
   browser's zone), which makes "due today" visible all day and "overdue"
   begin at local midnight with no server timezone logic (no Organization
   timezone exists). Editing a task without touching its date re-sends the
   stored instant unchanged. A task with no `due_at` lives on the Person
   page and never reaches Today.
3. **Complete, reopen and snooze are target-state idempotent, and act on
   the task row, never on the Today item** (D-022's deferral of
   done/snooze/dismiss on Today stands). Completing a completed task,
   reopening an open one, or snoozing to the time it already has is
   `changed: false` and writes nothing. **Snooze applies to open tasks
   only:** a snooze on a completed task returns `changed: false` with the
   current (completed) row and writes nothing; the receipt's `completed_at`
   tells the client why; a snooze never reopens. `UpdateTask` is allowed on
   a completed task under rule 1 (its fields are not completion state) and
   never touches the completion columns; retitling a completed task
   changes its `task_completed` history line.
4. **Delete is a tombstone** (title set to `''`, `deleted_at` and
   `deleted_by_user_id` stamped), invisible to every read, for the 015
   reasons: the import idempotency key survives so a re-run cannot
   resurrect a deleted task, and no text lingers. A second delete or any
   write to a tombstone is 404, identical to a nonexistent id.
5. **Only a completed task is history.** `task_completed` is projected from
   the row at `completed_at` (rank 8, after `note`); reopening removes the
   entry and completing again re-adds it at the new time. No `task_created`
   entry, no fact row (AGENTS §4.6), no `person.updated_at` bump, no D-052
   column, no filter statement change.
6. **Tasks appear on the Person detail read, not on People rows.**
   `PersonSummary` is untouched (011e rule 6).
7. **Titles are emitted at bounded sites** (D-053 posture): the detail's
   `tasks[]` and the `task_completed` detail, the mutation receipts, the
   `GET /api/tasks` panel rows, the Today reason payload for the Web, and
   the Operator's `tasks` view and `reasons_json` as `UntrustedText`
   (clipped to 500 characters). Never on the realtime channel, in the
   Operator ledger, in spans, logs or error envelopes. `Task`, `TaskView`
   and the widened `TodayReason` carry a **redacting `Debug`** (the `Note`
   pattern: a character count, never the text), because `TodayList` is
   `Debug`-printed in tests and error paths. The admin feed preview
   (`preview_today_system_feed`) evaluates person-state candidates directly
   and does **not** run the axis, so no other member's task title reaches
   an admin through preview.
8. **Today (016b; D-054 §1):** a Person joins the viewer's Today when the
   viewer holds at least one open, dated task on that Person with `due_at
   <= now + 24 h` (`now` is the query's single bound timestamp, passed as a
   parameter like every other Today statement, never SQL `now()`). Overdue
   (`due_at < now`, strict) is reason `task_overdue` in the `high` tier;
   `due_at >= now` is `task_due` in `normal`. One item per Person, carrying
   the viewer's earliest such task (earliest is chosen **among the viewer's
   own tasks**, never before filtering by assignee). Task-only items carry
   `waiting_since: null` (the list-only precedent; SLICE_003 §3's meaning
   is kept) and sort by the reason's `due_at ASC, id ASC` after the
   existing items of their tier. A Person with no inquiry (an imported
   contact) surfaces with `latest_inquiry: null` (011c §5 and 003 §3
   amended by pointer). A retained `normal` item whose task is overdue is
   raised to `high`; `low` items (call-outcome-only, 011c §4) are never
   raised, and nothing is ever lowered. Admins cannot tweak or disable the
   axis in this slice (the recorded exception).

## 2. Persistence (016a; one additive migration, lane-owned)

`crm-api/migrations/20260913000001_task.sql` (the 20260912 stamp is taken
by 015). Both indexes ship here so 016b adds none:

```sql
CREATE TABLE task (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    person_id UUID NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'follow_up'
        CHECK (kind IN ('call', 'email', 'text', 'follow_up', 'other')),
    due_at TIMESTAMPTZ,
    -- NULL only for an imported task whose FUB assignee matched no member.
    assignee_user_id UUID REFERENCES app_user (id),
    -- NULL only for an imported task whose FUB creator matched no member.
    created_by_user_id UUID REFERENCES app_user (id),
    completed_at TIMESTAMPTZ,
    completed_by_user_id UUID REFERENCES app_user (id),
    -- Origin::as_str: 'web_session' | 'operator' | 'migration' | ...
    origin TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    -- Import provenance (Slice 010 tasks rung); NULL for tasks created here.
    source TEXT,
    source_external_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    deleted_by_user_id UUID REFERENCES app_user (id),
    CHECK (assignee_user_id IS NOT NULL OR origin = 'migration'),
    CHECK (created_by_user_id IS NOT NULL OR origin = 'migration'),
    CHECK (completed_by_user_id IS NULL OR completed_at IS NOT NULL),
    CHECK (completed_at IS NULL OR completed_by_user_id IS NOT NULL
           OR origin = 'migration'),
    CHECK ((source IS NULL) = (source_external_id IS NULL)),
    CHECK (
        (deleted_at IS NULL
            AND char_length(title) BETWEEN 1 AND 500
            AND title = btrim(title, E' \t\r\n')
            AND position(E'\n' IN title) = 0)
        OR (deleted_at IS NOT NULL AND title = '')
    ),
    CHECK ((deleted_at IS NULL) = (deleted_by_user_id IS NULL)),
    CHECK (updated_at >= created_at),
    UNIQUE (id, organization_id),
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, assignee_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, created_by_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, completed_by_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, deleted_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
-- The Person detail and timeline: one Person's tasks by due time.
CREATE INDEX task_org_person_due_idx
    ON task (organization_id, person_id, due_at, id);
-- The Today axis and the panel: the viewer's open, live, dated tasks.
CREATE INDEX task_org_assignee_due_open_idx
    ON task (organization_id, assignee_user_id, due_at, id)
    WHERE completed_at IS NULL AND deleted_at IS NULL AND due_at IS NOT NULL;
-- Import idempotency and the tombstone resurrection guard.
CREATE UNIQUE INDEX task_org_source_external_idx
    ON task (organization_id, source, source_external_id)
    WHERE source_external_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE ON task TO crm_app;
```

- `origin`/`correlation_id` on the row let the timeline fill
  `HistoryEntry.origin`/`correlation_id` without a fact and let a later
  Operator-originated task chain to its turn.
- **Import readiness, schema only:** a future `ImportTask` domain function
  supplies `created_at`, `updated_at` and `completed_at` explicitly,
  `origin = 'migration'`, `correlation_id` = run id, the three actor columns
  by member email match else NULL, `kind` by FUB type name else `other`
  (counted and reported), `due_at` from FUB `dueDateTime` else `dueDate` at
  end of day in the importing admin's browser offset, `source = 'fub'` and
  `source_external_id` = the FUB task id. FUB field names are TO-VERIFY at
  010a; the nullable columns and the 500-character cap are chosen so none
  of them forces a migration. `CreateTask`'s API is not widened.
- `db_schema.rs` table, grant and index enumerations gain `task`; the
  schema test pins no `DELETE` for `crm_app`. `TaskId` in `ids.rs`.
- `task` joins the erasable CRUD set (SLICE_002 §2 pointer; O-013 runbook).

## 3. Typed commands (016a; new `crm-app/src/domain/task/`)

Module layout follows `domain/note/`. All commands run inside one
transaction, take `CommandContext`, use the Organization from the session
only, call `lock_person(person_id, organization_id)` first (absent →
`NotFound`), then load the task `WHERE id = $1 AND organization_id = $2 AND
person_id = $3 AND deleted_at IS NULL` **`FOR UPDATE`** (→ `NotFound`), then
the actor's own membership **`FOR SHARE`** requiring `status = 'active'`
(the note `lock_current_membership`), then decide rule 1 (`Role::Admin`, or
`assignee_user_id = actor`, or `created_by_user_id = actor` → else
`Forbidden`), and only then validate a changed assignee. Command structs
carrying a title have no `Debug` derive (or a redacting one).
`TaskTitle::parse` in `model.rs` is a pure function: trim, 1–500 code
points, any control character rejected.

| Command | Caller | Semantics |
|---|---|---|
| `CreateTask { person_id, title, kind, due_at?, assignee_user_id? }` | any active member | Validate; `lock_person`; assignee = supplied or actor; a supplied assignee must be an **active** member of the Organization, checked with a `FOR SHARE` read of that membership row (else `InvalidAssignee`, the existing 422; a foreign or random id is byte-identical); insert with `created_by = actor`, `origin`, `correlation_id`; publish `person.changed{task_changed}` after commit; return the `Task`. |
| `UpdateTask { person_id, task_id, title, kind, due_at?, assignee_user_id }` (full replace of the four fields) | rule 1 | Validate title/kind; locks and permission; the assignee is re-validated as an active member **only when it changes** (so a task held by a deactivated member can be retitled); all four fields byte-equal → `changed: false`, nothing written; else update them and bump `updated_at`; the completion columns are never touched; publish only when changed. |
| `CompleteTask { person_id, task_id }` | rule 1 | Already completed → `changed: false`; else stamp `completed_at = now()`, `completed_by = actor`; publish. |
| `ReopenTask { person_id, task_id }` | rule 1 | Not completed → `changed: false`; else clear both completion columns; publish. |
| `SnoozeTask { person_id, task_id, due_at }` | rule 1 | Rule 3: completed → `changed: false` with the current row, nothing written; `due_at` equal → `changed: false`; else set `due_at` only, bump `updated_at`; publish. Implemented internally as `UpdateTask` with one field changed (no second lock or permission path); it exists as its own route so the Today panel never full-replaces from a stale row. |
| `DeleteTask { person_id, task_id }` | rule 1 | `UPDATE … SET title = '', deleted_at = now(), deleted_by_user_id = actor` (`updated_at` unchanged; every other column byte-identical); publish. |

The `FOR SHARE` on a supplied assignee's membership serialises against a
concurrent deactivation: whichever commits first wins, so the outcome is
either 422 (deactivation first) or a task on a member who is deactivated a
moment later, the state rule 1 already accepts; never a dangling reference.
**Stated divergence:** `AssignPerson` accepts inactive members
(`is_organization_member` has no status filter, a recorded D-027/O-004
gap); tasks deliberately require an active assignee, and nobody should
"harmonise" the two.

Reads (`domain/task/queries.rs`): `open_for_person(conn, org, person)`
(ordered `due_at ASC NULLS LAST, created_at, id`) for the detail read and
the Operator; `completed_history(conn, org, person)` for the timeline;
016b adds `open_for_assignee(conn, org, user, now)` for the panel (dated,
`due_at <= now + 24 h`, ordered `due_at ASC, id`, fetch 201) and the two
axis statements (§5).

`TaskError`: `NotFound`, `Forbidden`, `MalformedRequest`, `InvalidAssignee`,
`Database`, `Corrupt`; all map to existing `ApiError` variants
(`Corrupt | Database` → 503); no new error code.

## 4. HTTP (016a routes; 016b list route)

All routes require an authenticated session with an active Organization;
platform-only sessions are 401. Path extractors: `PersonIdPath` on POST, a
`PersonTaskIdsPath(PersonId, TaskId)` pair on the rest. POST and PUT carry
the house 128 KiB `DefaultBodyLimit` per route; `deny_unknown_fields`;
malformed JSON is 400 without its message. **Error precedence:** malformed
path uuid 400 → 401 → 400 body → 404 → 403 → 422 `invalid_assignee` → 503
(the command's own order: lock, membership, permission, then the changed
assignee).

`Task` in responses:

```json
{"id","person_id","title","kind","due_at": ts|null,
 "assignee": {"id","display_name"} | null,
 "created_by": {"id","display_name"} | null,
 "completed_at": ts|null, "completed_by": UserRef|null,
 "created_at","updated_at","can_manage": bool}
```

`can_manage` is the viewer's rule-1 verdict at read time (admin, or the
viewer is `assignee.id` or `created_by.id`); a display hint, re-decided
under the lock.

| Route | Success | Errors |
|---|---|---|
| `POST /api/people/{person_id}/tasks` `{"title","kind"?,"due_at"?,"assignee_user_id"?}` | 201 `{"task": Task}` | 400, 404, 422 `invalid_assignee`, 503 |
| `PUT …/tasks/{task_id}` `{"title","kind","due_at","assignee_user_id"}` | 200 `{"task","changed"}` | 400, 403, 404, 422, 503 |
| `POST …/tasks/{task_id}/complete` | 200 `{"task","changed"}` | 403, 404, 503 |
| `POST …/tasks/{task_id}/reopen` | 200 `{"task","changed"}` | 403, 404, 503 |
| `POST …/tasks/{task_id}/snooze` `{"due_at"}` | 200 `{"task","changed"}` | 400, 403, 404, 503 |
| `DELETE …/tasks/{task_id}` | 200 `{"deleted": true}` | 403, 404 (also on repeat), 503 |
| **016b** `GET /api/tasks?scope=mine` | 200 `{"tasks": [TaskWithPerson], "generated_at": ts, "truncated": bool}`: the viewer's open, dated tasks with `due_at <= generated_at + 24 h`, ordered `due_at, id`, fetch 201 return 200; `TaskWithPerson` = `Task` + `"person": {"id","display_name"}` | 400 (missing, empty, unknown or extra query keys; fail closed), 401, 503 |

`GET /api/people/{id}` gains a top-level `"tasks": [Task]` (open only, §3
order) beside `tags`, and `history[]` admits the `task_completed` kind.
`GET /api/people` rows are unchanged.

### The `task_completed` history kind (SLICE_002 §5 pointer)

| Field | Value |
|---|---|
| `kind` | `"task_completed"`, `kind_rank` 8 (after `note`, 7) |
| `id` | the task id |
| `occurred_at`, `recorded_at` | both `completed_at` |
| `actor` | the completer's `UserRef`, or `null` (imported) |
| `origin`, `correlation_id` | from the row |
| `detail` | `{"title","kind","due_at","assignee": UserRef|null,"created_by": UserRef|null,"can_manage"}` (`can_manage: false` from the query; the detail route overwrites it: admin, or the viewer's id equals `assignee.id` or `created_by.id`) |

Tombstones and open tasks are excluded. The Web `HistoryEntry` union gains
the arm; the existing generic "Activity" fallback covers older bundles.

## 5. Today (016b; D-054 §1)

Inside the existing `today::query` transaction (one repeatable-read
read-only transaction, one bound `now`), the task axis runs **after
`evaluate_feeds_builtins` returns and after the call-feed unrecoverable
check, and before `builtin_ids` and the list-source `k` are computed**, so
task-only items are part of the built-in set the list stage excludes and
counts against. It runs on its own savepoint with **its own
`SOURCE_BUDGET`** (not the call feed's remainder), the same
statement-timeout discipline, the same 100 ms rollback grace (seeding
`final_recovery_deadline` the way the call feed's recovery does) and the
same unrecoverable handling (`unavailable`, detach). Two static statements
against `task_org_assignee_due_open_idx`, no dynamic SQL, no rule-7 inquiry
constraint, `now` bound as a parameter:

- **(a) Membership for retained ids** (retained = P ∪ call-only): for every
  retained Person id, the viewer's earliest open task with `due_at <= now +
  interval '24 hours'` — filtered by assignee **before** choosing the
  earliest — one row per Person; append the reason (`task_overdue` when
  `due_at < now`, else `task_due`) **after any list reasons and before
  `call_outcome_needed`**, which stays last; the list stage's reason
  appender keeps that order (a task-only item that later gains a list
  reason reads `[list_member, task_*]`).
- **(b) Task-only prefix:** only when neither the person-state statement
  nor the call prefix was truncated: the viewer's such tasks on Persons not
  retained, one row per Person (earliest), joined to `person` for the
  summary, `LIMIT (200 − |retained|) + 1`; the extra row sets
  `truncated_task`. Items are built directly as `TodayItem` with
  `latest_inquiry` and `last_inquiry_at` hydrated **exactly as the 011c
  list-only band does** (a real `InquiryRef` when the Person has an
  inquiry, `null` only when none exists; *amended at review round 1,
  2026-09-09: an earlier draft said "null" unconditionally, contradicting
  rule 8, D-054 and the 011c pointer*), `waiting_since: null`,
  `last_contact_attempt` hydrated the way the call-only statement does
  (the D-052 columns cannot supply the reference shape), `priority` `high`
  when overdue else `normal`, and
  `recommended_action` from `kind` using the existing variants only:
  `email` → `Email` if an email exists, else `Call` if a phone, else
  `ReviewPerson`; every other kind → the list-only chain (`Call` if a
  phone, else `Email` if an email, else `ReviewPerson`). Retained items
  keep their action.
- **Truncation composition:** `builtin_truncated = truncated_p ||
  truncated_call || truncated_task`; (b) runs only when `!(truncated_p ||
  truncated_call)`; the list-source prefixes skip on `builtin_truncated`
  as today; the response's `truncated = builtin_truncated ||
  source_truncated`.
- **Reasons:** two new `TodayReason` variants, `TaskOverdue { task_id,
  title, kind, due_at }` and `TaskDue { task_id, title, kind, due_at }`.
  The title rides the reason for the Web (the `list_member.name`
  precedent); the Operator's `reasons_json` wraps it as `UntrustedText`
  (a new arm beside `list_member`) and its fixed `reason_text` line uses
  only `due_at` and `kind`.
- **Ordering and tiers:** a retained `normal` item with an overdue task is
  raised to `high`; `low` items are never raised; nothing is lowered.
  Within `high`: fresh person-state items in their statement order, then
  raised items in their person-state order, then task-only items by
  `due_at ASC, id ASC`. Within `normal`: person-state items in order, then
  task-only items by `due_at ASC, id ASC` (the tie-break `id` is the
  Person id, the call-only precedent; the panel breaks ties on the task
  id; §12.13 parity is set-based). `low` unchanged. The compiled-in
  `ORDERING_RULE` string gains the clause
  `overdue_task_raises_normal_to_high_after_fresh; task_only_items_follow_their_tier_by_due_at_then_id`.
- **Failure, all-or-nothing:** if either statement fails or the budget
  expires, the axis contributes nothing: no `task_*` reason on any retained
  item, no task-only item, `system_feed_issues` gains the token `task_due`
  (`error: unavailable, fallback: false`, declared additive; the token has
  no feed row), the response is `partial`, the person-state and call
  reasons are byte-identical to a no-task run, and both `SET LOCAL`s are
  restored. `TodayQueryPhase` gains `TaskAxisAfterSavepoint`,
  `TaskAxisAfterMembership` and `BeforeTaskAxisRelease` so the
  failure-injection harness can target each. An unrecoverable call feed
  returns before the axis runs.
- **Telemetry:** `task_candidate_count` and `task_axis_ms` on `today.query`.
- Statement text of `person_state.sql`, the call feed and both source
  statements is untouched; `.sqlx` gains only the new statements.

**Web contract for the issue token:** the Web types the issue key as
`TodayFeedKey | 'task_due'` (it does not widen `TodayFeedKey`, which the
admin PUT paths reuse), adds a `TODAY_FEED_LABEL` entry ("Due tasks") and
renders any unknown key as "A Today rule could not load." instead of
`undefined`.

**What this is not:** not a feed row, not tweakable, not disable-able, not
in the admin Rules page, the member Rules section or the feed preview; no
filter clause; no `next_task_due_at` column (the partial index bounds the
scan by the viewer's own dated open tasks, not the Organization). All of
that is the LATER rung that closes the D-054 exception.

## 6. Realtime (016a; SLICE_003 §6 pointer)

`PersonChange` gains `TaskChanged` (wire `task_changed`), published on the
existing `person.changed` event after commit on create, changing update,
complete, reopen, snooze and delete; never on `changed: false`. The Web
handler's `invalidationsFor` gives `task_changed` **`queryKeys.person`,
`queryKeys.today` and (016b) the `tasks` prefix** (a due task changes the
viewer's Today and panel; it changes no People row and no list count);
older bundles fall through to the wide default, which is safe. Ids only;
no title on the channel, pinned by a parsed-payload test.

## 7. Operator (016a reads; 016b Today; SLICE_005 §5 pointer)

- `PersonDetail` gains `tasks: Vec<TaskView { title: UntrustedText, kind,
  due_at, assignee_display_name: Option<String> }>`: open tasks in the
  `open_for_person` order (`due_at ASC NULLS LAST, created_at, id`), at
  most ten. The tool view's `history` filters `task_completed` before the
  `MAX_HISTORY` truncation (the 015 note pattern); `history_detail` gains
  no arm. The prompt's untrusted-text parenthetical gains "task titles".
- 016b: `reason_text` gains fixed lines for `task_overdue` ("a task was due
  <relative>") and `task_due` ("a task is due <relative>") from `due_at`
  and `kind` only; `reasons_json` gains the two arms carrying the title as
  `UntrustedText`; `ORDERING_RULE` amended (§5); `get_today`,
  `get_next_work_item` and `explain_priority` see task items with no other
  change (they read the same `today::query`); `explain_priority`'s `ahead`
  counts follow the §5 order.
- No new tool; the tool count and the crate fences are unchanged. The
  ledger (D-029) holds no text. `create_task` / `complete_task` are the
  next S rung: `complete_task` is the first candidate for AGENTS §5.4
  "low-risk, reversible" execute-with-receipt, `create_task` stores
  model-authored text; both need a decision on mechanism (D-034's
  `operator_proposal` CHECK and the `crm-operator → crm-app` edge).

*Amendment pointer (Slice 018, 2026-09-10, declared additive, AGENTS.md
§11): D-057 took the decision; `TaskView` gains `task_id`; the two tools
land as seam methods with no crate edge. See [SLICE_018.md](SLICE_018.md).*

## 8. Web (016a Person page; 016b Today; UI_STYLE and D-045 bind)

**Person page (016a):** a **Tasks** card above History listing open tasks
in §3 order; each row shows the kind as a small monochrome label, the
title, the assignee (with an "(inactive)" suffix when deactivated), and a
due badge ("Overdue", "Due today", or the date; monochrome per D-045).
**Complete, Edit and Delete appear only where `can_manage`** (a 40 px
Complete control named "Complete task"; ghost buttons named "Edit task" and
"Delete task"); other rows are read-only. An **Add task** form at the top:
title input, kind select defaulting to Follow up, date input with an
optional time, assignee select from the Organization's members filtered
client-side to `status === 'active'` (the feed-preview dialog precedent)
defaulting to the viewer; Add disabled while the title is empty,
whitespace-only or pending; Ctrl/Cmd+Enter submits; a failed add keeps the
draft with the `lib/errors.ts` copy; 422 `invalid_assignee` renders "That
member is not active" and refetches the members list. Date-only picks are
converted client-side to local end of day (rule 2); an untouched date on
Save re-sends the stored instant. Edit swaps the row for the same form with
Save/Cancel (Escape cancels and returns focus; Cancel and Escape are inert
while pending); Delete uses the `ConfirmDialog` ("Delete this task? This
cannot be undone.") with the confirm disabled while pending. Completed tasks
render in History as `task_completed` rows ("Completed task: <title>",
"<kind> · was due <date>") with a ghost **Reopen** where `can_manage`.

**Today (016b):** the ranked item gains a `task_overdue` / `task_due` badge
showing the clipped title (the `list_member` chip precedent); the Waiting
cell shows "Due <relative>" from the task reason when `waiting_since` is
null; **whenever the item carries a task reason**, regardless of
`recommended_action`, a **Complete** button on the row completes that
`task_id` (the `set_outcome` control precedent), pessimistic, held while
pending, the item leaving only after the refetch. Below the queue header, a
**Tasks** panel from `GET /api/tasks?scope=mine`, grouped client-side
against the response's `generated_at`: **Overdue** (`due_at <
generated_at`) and **Due soon** (every other returned row; a row past local
midnight shows its date in the due cell); each row shows the Person's name
as a link, the kind label, the title and the due time; a Complete control
and a **Snooze** ghost button ("Tomorrow") posting snooze with tomorrow at
local end of day (a task already due then is `changed: false` and the row
stays, no error copy); empty groups collapse to one line ("Nothing due");
`truncated` shows "Showing the first 200". `reasonLabel` gains a default
arm ("Work item"), and the system-feed notice the generic label (§5), so
an older bundle never renders an empty badge or `undefined`.

**Types and keys:** Web types `Task`, `TaskWithPerson`, `TaskCompletedDetail`,
the `HistoryEntry` arm, `PersonChange` `'task_changed'`, the two
`TodayReason` arms, the issue-key union. `queryKeys.tasks(orgId, actorId)
= ['org', orgId, 'tasks', actorId]` (viewer-relative, like
`todayForActor`), invalidated by prefix. **Mutations:** all pessimistic,
keyed with `personMutationKey(orgId, personId)`; `settlePersonMutation`
gains an array form so one settle invalidates `[queryKeys.person,
queryKeys.today, queryKeys.tasks-prefix]`. 403 and 404 on any task action
refetch and explain inline. `PersonPreview.vue` narrows the widened
`HistoryEntry` union (the 015 compile touch); it shows no tasks.

## 9. Authorization, tenant isolation, failure and observability

- **Authorization:** rule 1 decided inside the command under the task
  row's `FOR UPDATE` and a `FOR SHARE` membership re-read; an admin demoted
  or an actor deactivated inside the transaction gets 403 and writes
  nothing. A changed assignee is checked as an active member under a `FOR
  SHARE` read of that row (§3 states the race outcome). The Today axis and
  `GET /api/tasks` scope by the session's user id **and** the session's
  Organization: a user active in two Organizations sees only the session
  Organization's tasks; a deactivated member has no session.
- **Isolation:** every statement carries the literal Organization
  predicate; the composite FKs make a cross-Organization task or a task
  with a non-member assignee, creator, completer or deleter unpersistable;
  foreign or nonexistent Person or task ids are 404 byte-identical to
  nonexistent; a foreign assignee id is 422 byte-identical to a random
  uuid; Organization B's Today and panel never carry Organization A's
  tasks, including a task the same user holds in B.
- **PII (D-053 posture):** titles at the rule-7 sites only; redacting
  `Debug` on `Task`, `TaskView` and `TodayReason`; spans record ids,
  `title_chars` and outcomes (`skip_all`); the `MalformedRequest` envelope
  never echoes input; tombstones empty the title; the Person cascade
  erases; the feed preview does not run the axis.
- **Failure:** 503 `unavailable` on every route for database failure; the
  Today axis degrades all-or-nothing to `partial` with the `task_due` issue
  (§5); a task deleted between a client's read and its action is a 404 the
  client resolves by refetching.
- **Idempotency:** create is not idempotent (the pending guard prevents a
  same-tab double-submit; a lost response can duplicate on resubmit, as
  for notes; the client-minted id is LATER); update-to-same, complete,
  reopen and snooze-to-same are `changed: false`; repeated delete is 404.
- **Observability:** spans `task.create`, `task.update` (`outcome`
  changed|unchanged), `task.complete`, `task.reopen`, `task.snooze`,
  `task.delete`, all `skip_all`; `today.query` gains
  `task_candidate_count` and `task_axis_ms`. **Titles are never logged.**

## 10. Contract declaration and amendment ownership

Approval satisfies AGENTS §11 for:

| Previous → proposed | Reason / affected | Compatibility and amendment |
|---|---|---|
| No task model → §2 table, §3 commands | Thesis §11 CRM core; FUB tasks destination | Additive migration. SLICE_002 §2 erasable-set pointer. |
| Six nested routes + `GET /api/tasks` (§4) | Person page, Today panel | Additive; existing error codes only (`invalid_assignee` reused). SLICE_002 §5 rows. |
| `GET /api/people/{id}` `+ tasks[]`; `history[]` eight kinds → nine (`task_completed`) | Person page, timeline | Additive; Web adds the arm; Operator filters the kind. SLICE_002 §5 pointer. |
| `PersonChange` seven → eight (`task_changed`) | Invalidate person, today, tasks | Additive. SLICE_003 §6 pointer. |
| `TodayReason` six → eight (`task_due`, `task_overdue`); `system_feed_issues` token `task_due`; `ORDERING_RULE` clause; built-in items with `latest_inquiry: null` and `waiting_since: null`; `normal` → `high` raise | D-054 §1 | Additive variants in a closed union; Web `reasonLabel` default arm and issue-key union with a generic label. Pointers: 011c §5 ("real InquiryRef"), 003 §3 ("no Inquiry never on Today"; "high iff new_inquiry"), 011d §1 rule 7 (feed constraint not lifted; the axis is not a feed), 011d §5 ("`TodayReason` gains no variant" superseded), 011d §6 (issue token without a feed row). |
| Operator `PersonDetail` `+ tasks`; `reason_text`/`reasons_json` arms | Operator sees tasks and explains task reasons | Additive, untrusted text. SLICE_005 §5 pointer. |
| D-043 / 011d "every built-in reason is a tweakable feed" | D-054 §1 | Recorded temporary exception; closed by the LATER clause-family rung. |

The coordinator owns the amendment pointers, PROJECT_STATE, this
specification and the briefs. No Person ownership, visibility scope, fact
table, filter vocabulary, D-052 column or admin Rules surface changes.

## 11. Performance (D-050)

016a adds no hot statement. 016b's axis is two index range scans bounded by
the viewer's own dated open tasks (dozens, not thousands, inside the
envelope) plus a `person` join on at most 201 rows. Gates, exactly two,
both on the perf book: (1) **paired equivalence:** for an Organization with
no tasks, `GET /api/today` JSON (minus `generated_at`) is byte-identical
before and after 016b, for an admin and a member; the person-state, call
and source statements are byte-identical in text; (2) **plan shape:** one
`EXPLAIN (ANALYZE, BUFFERS)` of the task-only prefix statement showing
`task_org_assignee_due_open_idx` and no super-linear growth with People.
Absolute latency reported, never gated. Evidence under
`docs/design/perf/slice-016-<date>/`.

## 12. Acceptance criteria and verification

House test rules (from the 015 rounds): routers for "publishes nothing"
assertions are built with `build_router_with_publisher` and every such
assertion is paired with a positive control in the same test; mock swaps
precede the click; "the item leaves only after refetch" asserts request
order, not the final DOM; `can_manage` is asserted inside each row's list
item, never by count; tie-break tests insert equal explicit timestamps;
capture tests carry positive controls and sentinels in rejected bodies.

016a:

1. **Schema:** migration applies fresh and on a populated database;
   composite-FK rejections for a cross-Organization Person and a non-member
   assignee, creator, completer and deleter; the CHECK matrix (empty live
   title, 501 characters, a title with `\n`, untrimmed, unknown `kind`,
   tombstone with a title, tombstone without `deleted_by`, `source` without
   external id, NULL assignee or creator with `origin <> 'migration'`,
   `completed_by` without `completed_at`, `completed_at` without
   `completed_by` outside migration); partial unique per Organization with
   the same external id allowed in another; grants with no `DELETE`; Person
   cascade; 500 four-byte code points accepted. (db)
2. **Validation (unit):** trim, 500 cap, any control character rejected
   including `\n` and `\t`, kind enum, `due_at` RFC 3339 in any offset or
   null.
3. **Create:** 201 shape with `can_manage` true; default assignee = actor;
   an explicit active member accepted; an inactive member, a member of
   another Organization and a random uuid → 422 `invalid_assignee`
   byte-identical, no row; an assignee deactivated **in flight** on a
   second uncommitted connection → 422 after it commits, no row, no
   publication; `tokio::join!` create-vs-deactivate → {201, 422}, never
   503; foreign or nonexistent Person → 404 byte-identical over HTTP against
   a real Organization-B Person and a random uuid, row counts and publisher
   unchanged; detail `tasks[]` order; `person.updated_at`, the D-052
   columns, People rows and (in 016a) Today byte-identical. (db)
4. **Update / complete / reopen / snooze / delete:** assignee, creator and
   admin 200; a third member 403 with no write and no publication;
   `changed: false` paths (equal PUT, complete twice, reopen an open task,
   snooze to the same instant, snooze on a completed task with a different
   `due_at`) write nothing and publish nothing, each with a positive
   control; the assignee is re-validated on change (random uuid → 422)
   and not otherwise (a task held by a deactivated member can be retitled;
   a third member's PUT with an unchanged assignee is 403, not 422);
   an assignee who reassigns to another member gets 200 with `can_manage:
   false` and 403 on the next PUT; `UpdateTask` on a completed task leaves
   both completion columns byte-identical and changes the history line's
   title; complete blocked by an in-flight uncommitted delete → 404 after
   it commits, no completion write; `tokio::join!` complete-vs-delete →
   {200, 200} or {404, 200}, never 503; actor deactivated, and separately
   admin demoted, inside the transaction (out-of-band UPDATE; and the
   in-flight uncommitted deactivation on a second connection for one
   command) → 403, no write, no publication; tombstone byte-identical except
   its three columns; another Person's path, another Organization's id, the
   viewer's own task in another Organization through this Organization's
   Person path, and a tombstone → 404 identical to random; precedence per
   route incl. 403 before 422; the routes join the platform-only 401
   enumeration with explicit POST/PUT/DELETE. (db)
5. **History:** `task_completed` at `completed_at`, rank 8 tie-break against
   a `note` with equal explicit timestamps; reopen removes it; complete
   again re-adds it at the new time; the detail carries `assignee` and
   `created_by` and `can_manage` is overwritten for the assignee, the
   creator and an admin and false for a third member; an imported-shape
   row (NULL actors) renders `actor: null` with admin-only `can_manage`;
   a member's edit of it is 403. (db)
6. **Realtime:** exactly one `task_changed` per changing write across all
   six commands and none on `changed: false`; the parsed payload has no
   `title`; the Web handler invalidates person and today (and the tasks
   prefix) and not People or list counts, and `note_changed` does not
   invalidate tasks. (db + Vitest)
7. **Operator (016a):** eleven open tasks: the ten earliest by `due_at
   NULLS LAST` present and the NULL-due eleventh absent, and the reverse
   (ten NULL-due, one dated → the dated one first); sentinel titles each
   once in `tasks` as untrusted text, completed and tombstoned titles
   never; `history` has no `task_completed` entry; the ledger row holds no
   sentinel; a `CaptureWriter` capture test at TRACE across create,
   update, complete, reopen, snooze, delete, a rejected title (its own
   sentinel), a 422 (a sentinel in the body), a 403, a 404 and the tool
   call, with positive controls for every span and warn line, finds no
   sentinel outside the 201/200 receipts and the detail read. (db)
8. **Web (016a):** Tasks card states (empty, open rows, badges Overdue /
   Due today / date, inactive assignee suffix); Complete, Edit and Delete
   present only inside rows with `can_manage` and absent inside rows
   without; add form validation, kind default, assignee picker limited to
   active members, date-only → local end of day under a fixed `TZ`
   (`2026-11-01` → `2026-11-02T04:59:59.000Z` and `2026-03-08` →
   `2026-03-09T03:59:59.000Z` in America/New_York), an untouched date
   re-sends the stored instant and an untouched Save yields
   `changed: false`; Ctrl/Cmd+Enter (both modifiers; plain Enter no
   POST); pending disabled and one POST across a second click; failed add
   keeps the draft; 422 copy; Edit/Save/Cancel/Escape with focus return and
   Escape inert while pending; Delete confirm disabled while pending;
   Reopen on a completed row; 403/404 refetch and explain; all mutations
   keyed and settled on the person, today and tasks keys; a
   literal-markup title renders as text with no element; `actor: null`
   rows render without "undefined"; an unknown history kind still renders
   the generic row. (Vitest)

016b:

9. **Membership and boundaries** (one `query_at` test with a fixed clock):
   `due_at = now` → `task_due`; `= now − 1 s` → `task_overdue`; `= now +
   24 h` admitted; `= now + 24 h + 1 s` absent and the response
   byte-identical to a no-task run; no due date, completed, tombstoned,
   another member's task, another Organization's task **held by the same
   user** (multi-membership), all excluded with positive controls; one item
   per Person carrying the viewer's earliest task (bob's earlier task on the
   same Person does not displace alice's later one from alice's Today and
   vice versa); a zero-inquiry Person surfaces with `latest_inquiry: null`
   and `waiting_since: null`, `get_next_work_item` returns it, and
   `recommended_action` follows `kind` with and without a phone or email.
   (db)
10. **Tier and order:** overdue → `high`, due → `normal`; a retained
    `normal` item with an overdue task is raised and placed after the fresh
    `high` items and before task-only `high` items; a retained `high` item
    is unchanged; a `low` item keeps `low` with the reason appended before
    `call_outcome_needed`; task-only items follow existing items within a
    tier by `due_at, id`; reason placement on a retained item (after list,
    before `call_outcome_needed`) and on a task-only item that also
    matches a list (`[list_member, task_*]`, exactly one item, the task
    tier); `explain_priority` `ahead` counts and the amended ordering rule
    string on a raised and a task-only item. (db)
11. **Cap and truncation:** 199, 200 and 201 built-in items with zero to
    three task-only rows; `truncated` set exactly by the extra row; the
    prefix skipped when the person-state statement or the call prefix was
    truncated; a truncated task prefix skips the list prefix; task-only
    items count against the list stage's cap; the person-state, call and
    source statements byte-identical in text. (db)
12. **Failure, all-or-nothing:** injected failure at each of the three new
    phases → no `task_*` reason on any item, no task-only item, exactly one
    `task_due` issue, person-state and call reasons byte-identical to a
    no-task baseline, `partial`, both `SET LOCAL`s restored; an
    unrecoverable call feed returns before the axis (no task statement
    ran); call feed and axis both failing yield two issues and recover
    within the seeded deadline; the axis completes within its own budget
    after a slow call feed (checkpoint delay hook: 400 ms + 300 ms). (db)
13. **`GET /api/tasks?scope=mine`:** the same membership rule with the
    three boundary tasks against `generated_at`; ordered `due_at, id`;
    `truncated` at 201; set-based parity with Today (panel Persons equal
    the task-reason Persons and the earliest per Person equals the reason's
    `task_id`; a Person with two in-window tasks gives two rows and one
    item); missing, empty, unknown and extra query keys 400; another
    Organization's and another member's tasks absent; platform-only 401.
    (db)
14. **Operator (016b):** `get_today`, `get_next_work_item` and
    `explain_priority` agree with HTTP on a task item; the fixed
    explanation lines contain no title; `reasons_json` carries it as
    untrusted text; a `CaptureWriter` capture across `GET /api/today`,
    `GET /api/tasks` and the three Operator Today tools with a sentinel
    title on a task-only item finds it only in the HTTP bodies and the
    untrusted `reasons_json`, never in the explanation line, the ledger
    row or any span. (db)
15. **Equivalence and plan:** §11's two gates with evidence retained. (db +
    archive)
16. **Web (016b):** task badges on the item and "Due <relative>" in the
    Waiting cell; Complete on any item carrying a task reason, held while
    pending, POST-then-GET order asserted; the Tasks panel groups against
    `generated_at`, a row past local midnight shows its date, empty states,
    Complete and Snooze (tomorrow at local end of day; an already-tomorrow
    task stays with no error copy), `truncated` line; `reasonLabel` default
    arm; the issue notice for `feed_key: 'task_due'` and for an unknown key
    renders a label and never "undefined"; settle keys include today and
    tasks; `task_changed` invalidates the panel. (Vitest)
17. **Walkthrough (live, dev runtime):** alice creates a task on a Person
    due today and one due next week; Today shows the Person with the
    `task_due` badge and the panel lists the first task only; alice snoozes
    it to tomorrow and it stays in the panel's Due soon group with
    tomorrow's date and stays on Today; she completes it from Today and it
    leaves both and appears in History as completed; carol (not assignee,
    not creator) sees the task on the Person page without controls; the
    admin reassigns a task from a deactivated member to carol and it
    appears on carol's Today; a second Organization sees nothing; the
    Operator lists the open task and explains the Today reason without
    quoting the title in its fixed line.
18. **Final gates** once per rung on the final tree by the coordinator;
    independent review and adversarial analysis, at most two rounds per
    rung (D-050).

## 13. Delivery

Two rungs, each **one lane, one writer, one short-lived branch from
`main`**, backend then Web within each, 016a first (016b's statements bind
016a's index and both touch `queries.ts`/`types.ts`):

- **016a (M, about 1.2× 015):** migration (sole owner; both indexes),
  `task` module and six commands, six routes, detail `tasks[]`,
  `task_completed`, `TaskChanged`, Operator `tasks`, Tasks card and
  History rows, tests §12.1–12.8. Only the coordinator runs `check-db`.
- **016b (M):** the axis with its savepoint, budget and failure phases, the
  two reasons and tier/order rules, the `task_due` issue token, telemetry,
  `GET /api/tasks`, Operator explanations and `ORDERING_RULE`, the Today
  badge, Complete button and Tasks panel, the issue-key union and label,
  tests §12.9–12.17, the §11 evidence. Depends on 016a's merge.

Expected files (016a): `crm-api/migrations/20260913000001_task.sql`;
`crm-app/src/domain/task/` (new), `domain/mod.rs`, `ids.rs`,
`domain/person/queries.rs`, `realtime/events.rs`; `crm-api/src/routes/tasks.rs`
(new), `routes/mod.rs`, `lib.rs`, `routes/people.rs`, `src/error.rs`
(mapping only); `crm-operator/src/views.rs`, `prompts/system.md`,
`crm-api/src/operator/backend.rs`; tests `db_tasks.rs` (new), `db_schema.rs`,
`db_admin.rs`, `db_people.rs`, `db_realtime.rs`, `db_operator.rs`;
`web/src/api/{types,queries}.ts`, `realtime/events.ts`, `lib/errors.ts`,
`views/PersonDetailView.vue` (+test), `components/PersonPreview.vue`.
016b adds `crm-app/src/domain/today/{mod,model}.rs` and
`today/sql/task_*.sql` (new), `crm-api/src/routes/tasks.rs` (the list route),
`crm-api/src/operator/explain.rs`, `crm-operator/src/views.rs`
(`ORDERING_RULE`), tests `db_today_task_axis.rs` (new),
`db_today_builtin_parity.rs`, `db_today_source_failures.rs` (phases), the
Operator Today tests; `web/src/views/TodayView.vue` (+test),
`api/{types,queries}.ts` (`TaskWithPerson`, the issue-key union,
`queryKeys.tasks`, the settle array form), `lib/todayFeeds.ts` (label and
fallback).

This specification authorizes nothing until the user approves it after
independent review; approval will authorize implementation and tests of
rung 016a, not commit, merge, push or deployment.
