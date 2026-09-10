# Slice 018 — Operator `create_task` and `complete_task`

**Status: APPROVED by the user on 2026-09-10 after independent review
(READY WITH CORRECTIONS; thirteen items applied, none a human decision).
Approval authorizes committing the planning documents and implementing in
the Slice 018 lane; the code commit, merge, push and deployment are asked
for separately.** Prepared against `main` at
`b233d46` (D-057 recorded, the LATER batch of 2026-09-10 merged). One M
rung, one lane, backend then Web. Brief: [SLICE_018_IMPL.md](../tasks/SLICE_018_IMPL.md).
Decisions: [D-057](../decisions/DECISION_LOG.md) (risk classes and the
mechanism default), D-054 §3 (the rung), D-053 (task authorization), D-034
(no `crm-operator -> crm-app` edge), D-033 (forced outcome is call-only),
D-029 (PII-free ledger), D-050 (envelope).

---

## 1. User-visible outcome

In the Ask panel, as the signed-in member:

- "Mark the call task for Grace done." The Operator completes that task at
  once. The panel shows a **receipt card** built only from server data:
  "Completed: *Call about the listing* · Call · was due today", with an
  **Undo** button. Undo reopens the task through the existing reopen
  route and the card reads "Reopened". The reply text may say it is done;
  the card is the truth. This is the first AGENTS §5.4 "low-risk and
  reversible" action (D-057 §1).
- "Add a task to call Grace on Friday." The Operator replies and the panel
  shows a **proposal card** built only from server data: "Add task for
  **Grace Hopper**: *Call Grace* · Call · due Fri 12 Sep, 11:59 PM ·
  assigned to you", with **Confirm** (primary) and **Dismiss**. Confirm
  creates the task as the member; the Person page Tasks card, Today and
  the panel update. Nothing exists before the click (D-057 §2).
- The Operator's own reading of tasks (Slice 016) now carries the task
  id, so it can act on "the call task" it just listed.

Out of scope (explicit): snooze, reopen, delete or edit through the
Operator (Undo is a human click on the existing reopen route, not a tool);
a task body; reminders; bulk actions ("complete all my overdue tasks");
completing a task the Operator has not read this turn or earlier in the
session (it needs the id); an Undo for a confirmed create (the Person page
Delete exists); a receipt or Undo that survives a reload or identity
change (session-only, the SLICE_006b proposal precedent); server-side
natural-language date parsing ("Friday" is the model's job); an
Organization timezone; a "completed via Operator" marker on the task row
(LATER, §13); mobile.

## 2. Design

Both tools are `ToolBackend` seam methods (D-034 stands, D-057 §4). The
crm-api adapter implements them over the existing task commands as the
signed-in member with `CommandContext::for_operator`, so the task row's
`origin` and `correlation_id` (SLICE_016 §2 reserved them for exactly
this) are set on a confirmed create. crm-operator names no crm-app type;
the fence test and the `scripts/check` graph fence are unchanged.

### Seam (crm-operator `backend.rs`, `views.rs`)

```rust
async fn complete_task(&self, ctx: &OperatorContext, person_id: Uuid, task_id: Uuid)
    -> ToolResult<CompleteTaskOutcome>;
enum CompleteTaskOutcome { Completed(Box<TaskReceiptView>), AlreadyCompleted(Box<TaskReceiptView>), Forbidden }
struct TaskReceiptView { task_id: Uuid, person: PersonCard, title: UntrustedText, kind: String,
    due_at: Option<DateTime<Utc>>, completed_at: Option<DateTime<Utc>>,
    completed_by_display_name: Option<String> /* None for imported rows */ }

async fn propose_create_task(&self, ctx: &OperatorContext, spec: &CreateTaskSpec)
    -> ToolResult<CreateTaskProposalOutcome>;
struct CreateTaskSpec { person_id: Uuid, title: String, kind: String,
    due_at: Option<DateTime<Utc>>, assignee: Option<String> }
enum CreateTaskProposalOutcome { Proposed(Box<TaskProposalView>),
    NeedsClarification { unknown_assignees: Vec<String>, ambiguous_assignees: Vec<String>, members: Vec<String> } }
struct TaskProposalView { proposal_id: Uuid, person: PersonCard, title: UntrustedText, kind: String,
    due_at: Option<DateTime<Utc>>, assignee: MemberRef { id: Uuid, display_name: String }, expires_at: DateTime<Utc> }
```

The due instant is composed in crm-operator's argument parser (pure
`NaiveDate` + `NaiveTime` + `FixedOffset` → `DateTime<Utc>`, no database),
so the offset never crosses the seam; the parser reads it from
`TurnState`, copied from `TurnInput`. The adapter (`SqlxToolBackend`)
gains a `Publisher` because every task command takes one. Both new views
carry the redacting `Debug` of `TaskView`.

Three facts in the code the decision did not anticipate, each resolved
here without a new decision:

1. `TaskView` (crm-operator `views.rs`) has no id. It gains
   `task_id: Uuid` (model-facing, additive). Today reasons already carry
   `task_id`.
2. `operator_proposal` is PII-free by construction (SLICE_006b §2: ids,
   timestamps and code strings) and its DDL requires `contact_method_id`
   and, when confirmed, `call_id`. A `create_task` proposal carries a
   model-authored title, so the payload lives in a **sidecar table**
   `operator_task_proposal`, and the parent's CHECKs are rewritten to
   admit the second tool (§4).
3. The confirm handler requires telephony before executing, and the
   system prompt says the Operator cannot create tasks. Both change (§5,
   §7).

### Loop rules (crm-operator `service.rs`)

- At most **one executed action per turn**: `state.receipt` beside
  `state.proposal`. A second `complete_task` in the same turn is a
  structured `invalid_arguments` (no backend call), the existing
  one-proposal guard's shape. A proposal and an execution may share a
  turn.
- `Completed` sets `state.receipt`; `AlreadyCompleted` and `Forbidden` do
  not. A 503 outcome never surfaces a receipt or a proposal (SLICE_006b
  §10).
- The "current time" line in `build_messages` renders the local time
  with the client's offset (§3) instead of `Z`, so the model can resolve
  "Friday". When no offset is supplied the line stays `Z`.

## 3. Tool contracts (declared additive changes; snapshot
`tool_definitions.json` +2)

**`complete_task`** identifies the task by `person_id` and `task_id`, both
required. The command binds `id AND organization_id AND person_id`, so
passing both through preserves the "reached through the Person path"
invariant with no extra lookup; a title match would be untrusted text and
ambiguous. A mismatch is `not_found`, byte-identical to foreign or
nonexistent.

```json
{"type":"object","properties":{
  "person_id":{"type":"string","format":"uuid","description":"The Person's id, from a previous tool result."},
  "task_id":{"type":"string","format":"uuid","description":"The task's id, from that Person's tasks or a Today reason."}},
 "required":["person_id","task_id"],"additionalProperties":false}
```

Result to the model: `{"status":"completed"|"already_completed",
"task":{"task_id","title":{"untrusted_text":...},"kind","due_at","completed_at",
"completed_by_display_name": string|null}}`, or `{"status":"forbidden"}`
alone (the command returns no row on `Forbidden`; the model already holds
the assignee's name from `TaskView`). `not_found` gains a task arm: "no
such task on that Person".

**`create_task`** only proposes.

```json
{"type":"object","properties":{
  "person_id":{"type":"string","format":"uuid"},
  "title":{"type":"string","minLength":1,"maxLength":500,"description":"One line, in the member's own words."},
  "kind":{"type":"string","enum":["call","email","text","follow_up","other"],"description":"Defaults to follow_up."},
  "due_date":{"type":"string","pattern":"^\\d{4}-\\d{2}-\\d{2}$","description":"Calendar date in the member's local time; omit for no due time."},
  "due_time":{"type":"string","pattern":"^\\d{2}:\\d{2}$","description":"24-hour local time; omit for end of day. Requires due_date."},
  "assignee":{"type":"string","maxLength":80,"description":"\"me\" (default) or a member's display name."}},
 "required":["person_id","title"],"additionalProperties":false}
```

Result: `{"status":"proposed","proposal":{...}}` or
`{"status":"needs_clarification","unknown_assignees":[..],"ambiguous_assignees":[..],"members":[..]}`
(an `Ok` outcome, not a strike; the `filter_people` precedent).

Rules for both:

- The model never supplies an instant, a user id, an Organization or an
  actor. The snapshot test's forbidden-property check (`user_id`,
  `actor_user_id`) still applies.
- **Due instant.** The turn request gains an additive optional
  `utc_offset_minutes` (integer, −840..=840; the Web sends
  `-new Date().getTimezoneOffset()`), threaded on `TurnInput` as
  client-supplied, harmless data; `OperatorContext` stays "from
  `AuthContext` and nothing else". The crm-operator parser composes
  `due_date` + (`due_time` or `23:59:59`) at that offset into an instant:
  the D-054 §3 client rule with the browser's offset as "the client".
  `due_date` with no offset in the request → `invalid_arguments` (fail
  closed; never a UTC guess); the prompt tells the model to propose
  without a due date and say so (§7). A fixed offset is
  one hour off on end-of-day across a DST change for a date in the other
  regime: BEYOND_ENVELOPE, LATER (an IANA zone is the upgrade).
- **Assignee** by display name, `"me"` by default, resolved against
  active members of the caller's Organization with the existing
  `find_by_name` / `NameMatch` helper and the `"me"` convention in
  `operator/filter.rs`. Unknown or ambiguous → `needs_clarification`.
- **Kind** defaults to `follow_up` (`TaskKind::default`).
- crm-operator's argument parser mirrors `TaskTitle::parse` structurally
  (trim; 1–500 code points; any control character, U+2028 or U+2029, or
  a title empty after removing default-ignorable code points →
  `invalid_arguments`), the way `FILTER_ARRAY_MAX` mirrors crm-app; the
  adapter runs the real `TaskTitle::parse` again.

## 4. Persistence (one additive migration, lane-owned)

`crm-api/migrations/20260914000001_operator_task_proposal.sql`:

- `ALTER TABLE operator_proposal`. The existing auto-named constraints,
  verified on a fresh migrate (Postgres 18): `operator_proposal_tool_check`
  (tool), `operator_proposal_check` (proposed), `operator_proposal_check1`
  (confirmed), `operator_proposal_check2` (failed, unchanged),
  `operator_proposal_status_check` (unchanged). The migration drops the
  first three by those names and re-creates them with explicit names:
  `tool IN ('start_call','create_task')`;
  `ALTER COLUMN contact_method_id DROP NOT NULL` plus
  `CHECK ((tool = 'start_call') = (contact_method_id IS NOT NULL))`;
  `ADD COLUMN task_id UUID`; proposed:
  `status <> 'proposed' OR (call_id IS NULL AND task_id IS NULL AND failure_code IS NULL AND confirmed_at IS NULL)`;
  confirmed:
  `status <> 'confirmed' OR (confirmed_at IS NOT NULL AND ((tool = 'start_call' AND call_id IS NOT NULL) OR (tool = 'create_task' AND task_id IS NOT NULL)))`;
  two hygiene CHECKs making the per-tool arms exclusive:
  `tool = 'create_task' OR task_id IS NULL` and
  `tool = 'start_call' OR call_id IS NULL`; `GRANT UPDATE (task_id)`
  (additive to the existing column grant). The claim's `RETURNING`
  changes type once `contact_method_id` is nullable, so `.sqlx` is
  regenerated; the `start_call` branch maps a `None` there to 503
  rather than panicking (unreachable by CHECK; one assertion).
- `CREATE TABLE operator_task_proposal (proposal_id UUID PRIMARY KEY
  REFERENCES operator_proposal(id) ON DELETE CASCADE, organization_id UUID
  NOT NULL, person_id UUID NOT NULL, title TEXT NOT NULL <the task title
  CHECK verbatim>, kind TEXT NOT NULL <the task kind CHECK verbatim>,
  due_at TIMESTAMPTZ, assignee_user_id UUID NOT NULL, FOREIGN KEY
  (person_id, organization_id) REFERENCES person (id, organization_id) ON
  DELETE CASCADE, FOREIGN KEY (organization_id, assignee_user_id)
  REFERENCES organization_membership (organization_id, user_id))`;
  `GRANT SELECT, INSERT` only (no UPDATE, no DELETE for `crm_app`). The
  title and kind CHECKs are the `task` table's verbatim minus the
  tombstone arm (`char_length(title) BETWEEN 1 AND 500 AND title =
  btrim(title, E' \t\r\n') AND position(E'\n' IN title) = 0`). The
  sidecar holds the only model-authored text outside the ledger's
  content-free rows. **Retention (safe default):** a title from a
  proposal that expires, is dismissed or fails stays in the sidecar until
  the Person is erased (the cascade), the same class as `inquiry.message`
  and the task row itself; there is no sweep and no update path. LATER:
  empty-on-finalize or a sweep. The O-013 runbook lists
  `operator_task_proposal` in the erasable set.

Rows after each path:

- complete: the `task` row's completion columns; the ledger
  `operator_turn` and `operator_tool_call (tool_name='complete_task',
  outcome, person_ids)`. No proposal row, no receipt row, no migration
  for this path.
- create: `operator_proposal (tool='create_task', contact_method_id NULL)`
  + `operator_task_proposal`; on confirm the `task` row with
  `origin='operator'` and `correlation_id = turn_id`, and
  `operator_proposal.status='confirmed', task_id, confirmed_at`.
- Undo: the `task` row reopened through the Web session's own reopen
  mutation; no marker.

Migration ownership: the lane, one file, plus `.sqlx` regeneration.

## 5. HTTP contracts (frozen at approval; AGENTS §11)

- `POST /api/operator/turns` — request gains additive optional
  `utc_offset_minutes` (integer −840..=840; absent or null = unknown;
  out of range → 400 `invalid_request`). Response gains additive nullable
  `"receipt": {"kind":"complete_task","task_id","person": WirePersonCard,
  "title": string,"task_kind","due_at": ts|null,"completed_at": ts}`,
  present only on 200 outcomes; and `"proposal"` becomes a
  `kind`-discriminated union: the existing `start_call` shape unchanged,
  plus `{"id","kind":"create_task","person": WirePersonCard,"title":
  string,"task_kind","due_at": ts|null,"assignee": {"id","display_name"},
  "expires_at"}`.
- `POST /api/operator/proposals/{id}/confirm` — unchanged request. The
  claim now returns `tool` and the handler branches **after** the claim:
  `start_call` exactly as today; `create_task` needs no telephony and no
  operator runtime, reads the sidecar row, calls
  `crm_app::domain::task::create_task` with `CommandContext::for_operator`,
  finalizes `confirmed` + `task_id`, and answers **201 `{"task": Task}`**,
  byte-identical to `POST /api/people/{id}/tasks`. Errors: 401; 404
  `not_found` (nonexistent, foreign or another user's, byte-identical);
  409 `proposal_expired`; 409 `proposal_consumed` whose body widens
  additively to `{"call_id": uuid|null, "task_id": uuid|null}`;
  pass-through task errors 404 (Person gone), 403 (actor deactivated),
  422 `invalid_assignee` (assignee deactivated since proposal), 503,
  each finalizing the proposal `failed` with the error's `kind()` code.
  Two branch cases stated: a Person deleted between propose and confirm
  cascades the sidecar away, the scoped claim still succeeds, the sidecar
  read finds no row → finalize `failed` with `not_found`, answer 404; a
  crash between claim and finalize after `create_task` committed leaves
  the task in place and the row `claimed`, so a re-ask makes a second
  task (the SLICE_006b "no recovery path" default, accepted). The widened
  `ProposalConsumed` variant lives in `crm-api/src/error.rs`.
- No new route. Undo uses `POST /api/people/{person_id}/tasks/{task_id}/reopen`
  as it stands.
- `TaskView` gains `task_id` (model-facing only; not a wire change).

## 6. Authorization and tenant isolation (D-053, unchanged)

Every mutation runs inside `crm_app::domain::task::commands` as the
signed-in member: lock the Person in the Organization, lock the task by
`(id, organization_id, person_id)` not deleted, re-read the actor's own
membership `FOR SHARE` active, then rule 1 (admin, assignee or creator);
for create, the explicit assignee re-checked as an active member. The
adapter adds and removes nothing. The model never names an Organization, a
user id or an actor (schema test); never sees another Organization's
Person or task (`visible_summary` first, then the composite lookups →
byte-identical `not_found`); never completes a task rule 1 forbids (the
`Forbidden` outcome writes nothing and publishes nothing); never creates a
task without the human click; never sets `completed_by`, `origin` or
`correlation_id`; has no snooze, reopen, delete or edit path. The confirm
is bound to `(organization_id, actor_user_id)` as in SLICE_006b §8.
`SqlxToolBackend`'s context-mismatch guard (the `run_saved_list` shape)
applies to both new methods.

## 7. Prompt

`crm-operator/prompts/system.md`: ten tools; the Operator **may complete
a task** the member asks it to complete, using the task's id from what it
has read, and must say so only after the tool reports `completed`; it
**may propose a task** with `create_task` when the member asks for one,
which creates nothing until the member confirms in the app; it must never
claim a task exists before Confirm; on `needs_clarification` it asks
which member was meant using the returned options; on `forbidden` it says
who the task is assigned to; on `already_completed` it says who completed
it and when; the "cannot create tasks" sentence is removed; dates come
from the member's words and the local current-time line, never invented;
when the tool reports the due date cannot be used (no client offset), it
proposes without a due date and tells the member, never guessing a time
zone.
Prompt-rule tests updated (SLICE_005 §13 style).

## 8. Web

`OperatorPanel.vue` and its test; `api/types.ts`, `api/queries.ts`:

- The turn request carries `utc_offset_minutes` from
  `-new Date().getTimezoneOffset()`.
- **Receipt card** rendered only from `receipt`: the completed title (as
  untrusted text, interpolated), kind label, "was due …" from `due_at`,
  and an **Undo** ghost button. Undo calls the existing
  `useReopenTaskMutation`; 200 `changed:true` → "Reopened", final; 200
  `changed:false` (someone reopened first) → same copy, final; 404 →
  "This task no longer exists."; 403 → "You can no longer change this
  task."; network error → retryable. `already_completed` and `forbidden`
  tool outcomes produce no receipt (the reply text carries them).
- **Create proposal card** rendered only from the `create_task` union
  member: Person name, title, kind, due (local, from `due_at`), assignee;
  Confirm (primary) posts through a new `useConfirmTaskProposal` in
  `api/queries.ts` (same route; typed `{task}`; settles through
  `settleTaskMutation`), because the existing `useConfirmProposal` is
  typed for the call response and reads `data.call.id`; 201 → "Task
  added." and invalidation of the person, today and tasks keys; Dismiss
  is local; expired → Confirm disabled with the existing copy; 409
  consumed → "This task was already added." pinned in the panel (the
  telephony copy says "call"); 409 expired as today; task errors through
  the existing `TASK_ERROR_COPY` in `lib/errors.ts` (already covers
  `invalid_assignee` and `forbidden`; not edited). No Undo on a confirmed
  create.
- Types: the existing `OperatorProposal` interface is renamed
  `OperatorStartCallProposal`, `OperatorCreateTaskProposal` is added, and
  `OperatorProposal` becomes their union discriminated on `kind`. The
  panel narrows on `kind` before calling `callHost.startFromProposal`
  (which reads only `id` and `person`, so `telephony/callHost.ts` and
  `telephony/errors.ts` are not edited); the existing `start_call` card
  tests type-check unchanged.
- On a receipt the panel also invalidates the person, today and tasks
  keys (the `settleTaskMutation` key set), so a missed realtime event
  cannot leave the Today panel stale. Undo settles through its own
  mutation.
- Receipts clear with proposals on identity change and unmount (011c §6).
- UI_STYLE binds: one primary per region (Confirm on the create card;
  the receipt card has none), focus rings kept, no model prose in cards.

## 9. Realtime

Nothing new. `complete_task`, `reopen_task` and `create_task` already
publish `person.changed { task_changed }` after commit; the Web handler
invalidates person, today and tasks. §8's invalidation on receipt covers
the missed-event case.

## 10. Failure behaviour

| Condition | Model / UI sees |
|---|---|
| Provider down or key unset | turn 503 `operator_unavailable`, nothing executed; confirm of an existing `create_task` proposal still works (model-free) |
| Invalid arguments (bad uuid, empty or over-long title, control char, bad date, `due_time` without `due_date`, no offset with a date) | `invalid_arguments` strike; nothing written |
| Task not found, foreign, other Person's path, tombstone | `not_found` "no such task on that Person"; ledger `not_found` |
| Rule 1 forbidden | `{"status":"forbidden"}`, no write, no publish; reply names the assignee |
| Already completed (a panel click won the race) | `already_completed` with completer and time; no receipt, no Undo |
| Database failure inside the tool | `ToolError::Backend` → turn 503 `tool_error`; the command's transaction rolled back |
| Turn timeout mid-command | rolled back if before commit; if after commit the completion stands, the turn is a 503 with no receipt and no Undo card, the panel shows the 503 copy, the Person page and Today show it completed on the next refetch and the ordinary Reopen is available (accepted at millisecond transactions, D-050); ledger shows the tool `error` |
| Second `complete_task` or second proposal in a turn | structured `invalid_arguments`, no backend call |
| Confirm expired, consumed, double | 409 `proposal_expired`; 409 `proposal_consumed {call_id, task_id}`; a concurrent double-confirm yields one 201 |
| Person deleted, assignee or actor deactivated between propose and confirm | 404 / 422 `invalid_assignee` / 403; proposal `failed` with that code |
| Undo: task deleted, rights lost, already reopened | 404 copy / 403 copy / `changed:false` treated as success |

## 11. Observability

`operator.tool_call` gains `task_id`, `action_outcome` (completed,
already_completed, forbidden, proposed, needs_clarification) and
`title_chars`; `operator.proposal_confirm` gains `tool` and `task_id`.
Titles never appear in spans, logs, error envelopes, the ledger or the
realtime payload; the model-supplied title appears only in the tool
result, the sidecar row, the wire card and the created task. The
CaptureWriter test in `db_operator.rs` gains a sentinel in `create_task`
arguments.

## 12. Acceptance criteria and required tests

- **crm-operator unit:** snapshot (+2 tools); the forbidden-property test
  green; parser matrix for both tools (title trim, 500, control, U+2028,
  ignorable-only; kind enum; date and time patterns; `due_time` without
  `due_date`; assignee clip at 80); one receipt per turn and the shared
  proposal slot; `receipt` and `proposal` absent on 503 outcomes;
  redacting `Debug` on the new views; prompt-rule tests (ten tools, "may
  complete", "may propose a task", "never claim a task exists before
  Confirm", the local-time line with and without an offset).
  Due-instant composition in the parser (end of day, explicit time,
  negative and positive offsets, the ±840 bounds, missing offset →
  `invalid_arguments`).
- **crm-api unit:** the context-mismatch guard on both new methods;
  (TRUST) `POST /api/operator/turns` answers 400 for `utc_offset_minutes`
  of 841, −841, 1.5 and a string.
- **DB (`db_operator_task.rs`, new; registered in `tests/all.rs`):**
  `complete_task` authorization matrix (assignee, creator, admin →
  completed; third member → forbidden with no write and no publication,
  with a positive control; foreign Organization, other Person's path,
  tombstone, random id → `not_found` byte-identical); idempotency
  (`already_completed`, no publish); `tokio::join!` Operator-versus-panel
  completion gives one `changed:true`; the task row's `origin` is
  unchanged by a completion and is `operator` with `correlation_id =
  turn_id` on a confirmed create; the proposal chain (parent and sidecar
  row shapes, cascade on Person delete, CHECK matrix including
  `contact_method_id` null-iff-`create_task` and the confirmed CHECK per
  tool, grants: no UPDATE and no DELETE on the sidecar); confirm without
  the operator runtime and without telephony; expired, consumed, double
  and stuck-claimed; the in-flight races (Person deleted, assignee
  deactivated, actor deactivated); (TRUST) confirm of a `create_task`
  proposal by another member of the same Organization and by the same
  user under another Organization's session → 404 both ways; (CONTRACT)
  `proposal_consumed` carries `task_id` after a confirmed create and null
  while `claimed`; assignee resolution (`me`, exact, ambiguous, inactive →
  clarification with no row written); ledger rows with no title in them;
  the capture sentinel; Undo through reopen (200 `changed:true`,
  `changed:false`, 404, 403); the existing `db_operator_call.rs` chain
  green under the rewritten CHECKs (free); the `None` `contact_method_id`
  on a `start_call` row → 503 (one assertion).
  Calibration (06-verify): the authorization matrix, the isolation
  probes, the CHECK matrix, the race pair and the capture sentinel are
  TRUST or CONTRACT and stay; "Dismiss local", the assignee clip, the
  kind enum and the `changed:false` Undo copy are RESTATES and are
  one-line assertions or LATER.
- **Web (Vitest):** the receipt renders only from `receipt` (a reply
  claiming a different title does not reach the card); Undo order (POST
  reopen, then refetch), final after success, 404 and 403 copy, retry on
  network error; no receipt for `already_completed`; the create card
  renders the union member, Confirm posts, 201 → "Task added." with
  invalidation, Dismiss local, expiry disables Confirm, consumed and
  expired copy; identity-key change clears receipts; the turn request
  carries `utc_offset_minutes`; the `start_call` card tests still pass
  against the union type.
- **Fences:** `operator_deps.rs` and the `scripts/check` graph fence
  unchanged and green.
- **Gates:** `sqlx-prepare` (statement changes), `check`, `check-db`,
  once on the final tree by the coordinator.
- **Walkthrough (coordinator, dev runtime):** alice completes a Today
  task through the Operator; Undo reopens it; the panel, Today and the
  Person page agree. "Add a task to call Grace on Friday" → the card
  shows Friday 11:59 PM local → Confirm → the Person page shows the task;
  after completing it from the page, history shows origin operator. bob
  cannot confirm alice's proposal (404). Provider key unset → confirm
  still works. A second `complete_task` request in one turn is refused
  in the reply.

## 13. Safe defaults adopted (veto-able; not re-litigated in-lane)

The task identified by `person_id` + `task_id`; one executed action and
one proposal per turn; `already_completed` shows no Undo; receipt and
Undo session-only; assignee by display name against active members,
`"me"` default; `due_date` / `due_time` plus the client's
`utc_offset_minutes` composed server-side with end of day 23:59:59, a
missing offset failing closed; the same proposal TTL; a sidecar payload
table cascading with the Person; confirm answers 201 `{task}` for
`create_task`; no Undo on a confirmed create; Undo through the existing
reopen route with `changed:false` treated as success; no
`completed_origin` marker on the task row: the D-029 ledger row records
that the Operator ran `complete_task` on this Person at this time (its
`outcome` is `ok` even for `forbidden` and `already_completed`, and it
carries no task id), which correlates with the task's `completed_at`
and the timeline's completer; a per-row marker is LATER if a product
need appears; the sidecar title retention above.

## 14. Delivery

One lane (Claude Sonnet 5), one writer, branch `slice-018-operator-tasks`
from `main` in `../crm-worktrees/018`, backend first with a checkpoint
after the migration, seam, adapter and confirm route are green on
targeted DB tests, then the Web. Ownership: `backend/crates/crm-operator/**`
(tools, backend trait, service, views, prompt, snapshot, tests),
`backend/crates/crm-api/src/operator/**`, `backend/crates/crm-api/src/routes/operator.rs`,
`backend/crates/crm-api/src/error.rs` (the `ProposalConsumed` body only),
the new migration, `.sqlx/`, `backend/crates/crm-api/tests/db_operator_task.rs`
and `tests/all.rs`, the existing `db_operator*.rs` only where the
snapshot or capture tests need it, `web/src/components/OperatorPanel.vue`
and its test, `web/src/api/types.ts` and `web/src/api/queries.ts` for the
declared shapes and the new confirm helper. Not edited:
`web/src/telephony/**`, `web/src/lib/errors.ts`. Not owned: `crm-app` (the task commands are used as they
are; a needed change stops the lane), `docs/`, `Cargo.*`. The coordinator
runs review and test analysis (two rounds at most, D-050), the once-only
final-tree gates, the walkthrough, and the commit and merge gates.
