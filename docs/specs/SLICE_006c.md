# Slice 006c — Call outcome correction

Status: APPROVED (user, 2026-08-22; planner pass in
`docs/plans/SLICE_006c_PLAN.md`, independent review — 11 findings, all
applied as safe defaults / implementation details).
Builds on: Slice 006 (`main` at `332e78a`).
Targets: D-032 (agents may correct a call's outcome; notes deferred),
D-031 (the automatic attempt stays exactly as written), D-022 (an
attempt is the unit of response for Today), D-015 (facts are immutable;
`corrects_id` is the correction mechanism), D-008 (one typed command
layer), D-023 (ids-only realtime, invalidate by refetch), D-005
(Organization visibility). Supersedes SLICE_006 §12's exclusion of
"corrections of auto-logged attempts" for call-derived attempts only.

Why: the system cannot tell a person answering from voicemail, a full
mailbox, or a carrier message (a first live call was "answered" in
2.8 s with no ring). The agent knows; the record should say what the
agent knows without losing what the system observed.

Narrowest cut: one new command and route, one migration widening a
CHECK and adding a partial unique index, the first writer of
`corrects_id`, a post-call prompt in the existing call panel, history
rendering of superseded/correcting rows, and the Today query reading
the effective attempt. No notes (D-032 note), no Today resurfacing
rule, no Operator change beyond a history-text fix (§4), no dependency
on 006a.

## 1. User-visible outcome

As `alice` on `/people/:id` after a call placed from the Slice 006 panel:

1. The call ends (either side hangs up, ring-out, busy, declined). The
   panel's post-call block becomes **"How did it go?"** with five
   choices — *Talked to them*, *Voicemail*, *No answer*, *Busy*, *Wrong
   number* — pre-selected from what the system observed (answered →
   Talked to them; a `no_answer` attempt → No answer), with **Save
   outcome** (primary) and **Skip** (ghost; replaces Done).
2. **Skip** writes nothing; the panel closes as today.
3. **Save** with *Voicemail*: History shows **one line per call**
   (Follow Up Boss style; user decision 2026-08-23 after seeing two
   corrected calls rendered as indistinguishable rows): "Call —
   voicemail, 7 s · Alice · 11 hours ago · outcome corrected from talked
   to them 5 hours ago". Call-derived `contact_attempted` rows and
   correction rows are folded into that line by the web client (the
   stored facts are unchanged); manual attempts stay their own lines.
   The Person's Today card (for any member who
   still has it) shows `last_contact_attempt` = voicemail. Every open
   tab updates within a second (`person.changed`).
4. Saving the outcome already recorded changes nothing (the panel
   closes; `changed: false`).
5. Calls where nothing reached the callee (`agent_not_joined`,
   `provider_error`, `expired`, cancel before ringing) show the existing
   post-call line and no prompt — there is no attempt to correct.
6. As `carol` (same Organization, not the caller): the prompt never
   appears for alice's call; a direct `POST …/outcome` is 403. As
   `bob` (other Organization): 404.
7. Later, on the Person's History, the caller sees a small **Change
   outcome** ghost action on their own call-derived, non-superseded
   attempt rows, opening the same picker. (Lane B's last item;
   cuttable at approval if the lane runs long.)

The manual "Log contact" dialog gains *Busy* and *Wrong number*
automatically (its options derive from the label map).

## 2. Domain and schema

### Concepts

- **Correction** — a new `contact_attempted` row whose `corrects_id`
  points at the row it supersedes. The original is never updated
  (D-015; `reject_mutation` trigger; `crm_app` has no UPDATE on fact
  tables). This slice establishes the `corrects_id` convention for all
  fact tables: corrections form a linear chain; the **effective** row
  is the one with no corrector.
- **Head attempt** of a call — the `contact_attempted` row with
  `causation_id = call.id` that has no corrector. A second correction
  corrects the first correction, never the original.

### Correction row

| column | value |
|---|---|
| `person_id`, `channel` | copied from the corrected row (`call`) |
| `outcome` | the chosen value |
| `occurred_at` | **= corrected row's `occurred_at`** — the attempt happened then; only its description changes. Keeps the D-022 Today predicate and the history timeline position stable. `recorded_at` is set explicitly to `clock_timestamp()` *after* the call lock is taken (not the `now()` default, which is transaction-start time and could precede a competing transaction's insert), so it is strictly later than the head's. |
| `actor_kind` / `actor_user_id` | `user` / the correcting user |
| `origin` | from `CommandContext` (`web_session`) |
| `correlation_id` | `call.correlation_id` |
| `causation_id` | `call.id` (so "all facts for this call" stays one lookup; `envelope.rs` doc: "`call.id` when written by or about a call") |
| `corrects_id` | the head attempt's id |

### Migration (one file, Lane A): `20260826000001_contact_outcome_correction.sql`

```sql
ALTER TABLE contact_attempted DROP CONSTRAINT contact_attempted_outcome_check;
ALTER TABLE contact_attempted ADD CONSTRAINT contact_attempted_outcome_check
    CHECK (outcome IN ('reached','no_answer','left_message','sent','busy','wrong_number'));
-- Linear correction chains: a row is corrected at most once.
CREATE UNIQUE INDEX contact_attempted_corrects_once
    ON contact_attempted (corrects_id) WHERE corrects_id IS NOT NULL;
```

(The constraint name must be read from the live schema by the
implementer; if it is unnamed/auto-named, drop by the discovered name.
Nothing else changes; append-only triggers and grants are untouched.
The partial unique index also serves the `NOT EXISTS` lookups below.)

### Vocabulary

| Label | stored `outcome` | note |
|---|---|---|
| Talked to them | `reached` | existing |
| Voicemail | `left_message` | existing; relabelled "Voicemail / left message" in `web/src/lib/labels.ts` so the manual dialog and the prompt agree |
| No answer | `no_answer` | existing |
| Busy | `busy` | new — the agent's explicit statement must not vanish into `no_answer` |
| Wrong number | `wrong_number` | new |

D-031's automatic mapping (busy/declined/ring-out → `no_answer`) is
unchanged: it records what the system observed, not a user statement.
`sent` is not a call outcome and is rejected on this route.

### Declared change to an existing contract (AGENTS §11, additive)

`GET /api/people/{id}` `history[]` entries of kind `contact_attempted`
gain `detail.call_id: uuid | null` (= `causation_id` when the attempt is
call-derived), `detail.corrects_id: uuid | null`, `detail.superseded:
bool`. `kind`/`kind_rank` unchanged. **Timeline placement of a
correction** (user decision after the first live correction,
2026-08-23): in `history[]` a correction's `occurred_at` is its
`recorded_at` — the moment the agent corrected it — so the timeline
reads original attempt → `call_completed` → correction, and the
correction shows "2 minutes ago", not the call's time. The stored fact
still inherits the attempt's `occurred_at` (Today semantics, §3). Lane B
renders superseded/corrected rows from `superseded`/`corrects_id`,
never from position. Pointer lines go in: SLICE_002 §5 (history detail); SLICE_003 §2 (the
widened CHECK) and §5 (`POST /api/people/{id}/contact-attempts` now
accepts `busy`/`wrong_number` — the manual route's vocabulary widens
with `ContactOutcome`); SLICE_003 §3 (`last_contact_attempt` is now the
*effective* attempt — greatest `occurred_at` among rows with no
corrector, tie-break `id DESC`; also the `today/queries.rs` doc
comment); SLICE_006 §12 (the exclusion is superseded by 006c). `CallView`, `TodayItem`, the Operator tool outputs
keep their shapes (`last_contact_attempt.outcome` simply widens its
value set; the Operator crate does not enumerate outcomes).

## 3. Commands and queries

`correct_call_outcome(pool, publisher, ctx, CorrectCallOutcome {
call_id, outcome: CallOutcomeCorrection }) -> CorrectionResult`
(`src/domain/commands/correct_call_outcome.rs`), one transaction:

1. `lock_call(tx, organization_id, call_id)` → none → `CallNotFound`
   (404, byte-identical for foreign).
2. `call.caller_user_id != ctx.actor_user_id` → `Forbidden` (403).
3. `status ∉ {ended, failed}` → `InvalidCallState` (409).
4. Head attempt: `SELECT … FROM contact_attempted ca WHERE
   organization_id = $1 AND causation_id = $2 AND person_id = $3 AND
   NOT EXISTS (SELECT 1 FROM contact_attempted c WHERE c.corrects_id =
   ca.id) ORDER BY recorded_at DESC LIMIT 1` → none →
   `NoContactAttempt` (422).
5. `head.outcome == requested` → roll back; return
   `{attempt: head, changed: false}`; publish nothing.
6. Insert the correction row (§2) via `facts::insert_contact_attempted`
   extended with `corrects_id` and an explicit `recorded_at`. Because
   step 1 holds the call row lock, concurrent saves **serialize**: the
   later one sees the earlier correction as head and chains or no-ops.
   `23505` on `contact_attempted_corrects_once` is therefore unreachable
   from this command; it is still mapped to `CorrectionConflict` (409)
   defensively for any future writer that does not lock the call.
7. Commit; publish `person.changed {contact_attempted}` with the
   call's `correlation_id`.

`CallOutcomeCorrection` is a serde enum of exactly the five values.
`ContactOutcome` gains `Busy`, `WrongNumber`.

Queries: `today/queries.rs`'s `last_attempt` LATERAL selects the
**effective** row — add `AND NOT EXISTS (SELECT 1 FROM contact_attempted
c WHERE c.corrects_id = ca.id)` — otherwise the equal-`occurred_at`
tie-break (`id DESC` on random UUIDs) picks original or correction at
random. `person/queries.rs`'s history source adds the three detail
fields (`superseded` = `EXISTS corrector`). No other query changes;
`rank.rs`/`model.rs` untouched.

## 4. Operator tools

No tool-shape change. One rendering fix in `operator/backend.rs`: the
`get_person` history text for `contact_attempted` (`"{channel}:
{outcome}"`) appends " (superseded)" when `detail.superseded` and
prefixes "corrected outcome" when `corrects_id` is set — otherwise the
model would report the superseded row as a live attempt. 006b remains
the Operator calling slice.

## 5. HTTP contract (frozen at approval)

| Route | Request | Success | Errors |
|---|---|---|---|
| `POST /api/calls/{id}/outcome` | `{"outcome": "reached" \| "left_message" \| "no_answer" \| "busy" \| "wrong_number"}` (`deny_unknown_fields`; any other value incl. `sent` → 400) | 200 `{"attempt": CorrectedAttemptRef {id, channel, outcome, occurred_at, recorded_at, corrects_id}, "changed": bool}` — a new type; `ContactAttemptRef` (used by `TodayItem.last_contact_attempt` and `POST …/contact-attempts`) is unchanged | 400 `malformed_request`; 401; 404 `not_found`; 403 `forbidden` (not the caller); 409 `invalid_call_state` (call not terminal); 422 `no_contact_attempt`; 409 `correction_conflict`; 503 `unavailable` |

Works with telephony disabled (it is a pure fact write; no provider).
New `ApiError` variants: `NoContactAttempt`, `CorrectionConflict`.

## 5a. Amendment — forced outcome and the "outcome needed" Today tier (D-033, 2026-08-23)

Supersedes §1.1–§1.4, §1.7 and the §10 prompt/rendering text where they
conflict. Storage (§2), the command (§3) and the route (§5) are
unchanged.

**Post-call prompt.** Shown whenever an automatic attempt exists
(`attemptOutcome(call) !== null`). No choice is pre-selected; **Save
outcome** is disabled until one is picked and until the server reports
`ended|failed`; there is **no Skip**. The panel stays until Save
succeeds ("Outcome saved — <label>", then Done). Navigating away is
allowed (nothing is lost: the automatic row stands and the Today tier
below nags).

**Timeline (one line per call, §1.3).** The effective attempt decides
the label: if it is an agent choice (`corrects_id !== null`) → "Call —
voicemail, 7 s"; if it is the automatic root → "Call — 7 s · outcome
needed" (duration only when answered; "Call · outcome needed"
otherwise) plus a **Set outcome** ghost action for the caller. Failed calls with no attempt: unchanged ("Call —
failed"). The system's observation is never rendered as the outcome;
manual Log-contact rows are unchanged.

**Today (SLICE_003 §3/§5, additive).** A second membership source: a
`call` of the viewer's Organization whose `caller_user_id = viewer`,
status `ended|failed`, whose effective attempt is the automatic root
(no corrector). Such a Person is a Today item with `priority: "low"`
(new value; sorts after `normal`), `recommended_action: "set_outcome"`
(new value), `reasons` containing `{"code": "call_outcome_needed",
"call_id", "ended_at"}` (new code; may be combined with the existing
reasons when the Person also qualifies by inquiry — then the inquiry
tier wins and the reason is appended), `waiting_since` = the call's
`ended_at` when it is the only reason. `TodayItem` gains no other
field. Ordering: existing tiers first, then `low` by `ended_at ASC`.
The Operator's `get_today`/`explain_priority` carry the new reason
code and a one-line explanation ("a call on <date> has no outcome
yet"); no tool-schema change (reasons are already a list of coded
objects). Choosing an outcome removes the item; `person.changed
{contact_attempted}` already invalidates Today.

Web: the Today card for a `low` item shows "Outcome needed" with a
**Set outcome** action linking to the Person page (which opens the
Change-outcome dialog for that call, pre-selection none). `TodayPriority`
gains `'low'`, `RecommendedAction` gains `'set_outcome'`, `TodayReason`
gains `call_outcome_needed`.

Tests: Lane A — the new membership source (caller only; not the
assignee; not other members; foreign org never), tier ordering, reason
payload, removal after an outcome, Operator explanation text, and that
a Person qualifying both ways keeps the inquiry tier with the reason
appended. Lane B — forced choice (no default, Save disabled until a
pick, no Skip), the "outcome needed" timeline row and Set outcome
action, the Today low-tier card, the Today → Person → dialog path.

## 6. Realtime

`person.changed {contact_attempted}` (existing) after commit; it already
invalidates `person`, `people`, `today`. No `call.changed` (the `call`
row does not change). No new event type.

## 7. Authorization and tenant isolation

Organization and actor enter only via `AuthContext → CommandContext`;
the call is loaded org-scoped (foreign = 404 on every path, probed both
directions). **Caller only** may correct (safe default, consistent with
SLICE_006 §14.3 caller-only control; the fact is the caller's own
account of the call) — any member may read. Concurrent corrections serialize on the call row
lock (§3); the partial unique index is a schema invariant (a row is
corrected at most once), not the concurrency guard.

## 8. Notification behavior

None beyond Centrifugo invalidation.

## 9. Observability and failure behavior

Span `call.outcome` with `call_id`, `person_id`, `organization_id`,
`actor_id`, `correlation_id` (the **call's**, as on the fact and the
event; the request span still carries the ctx correlation id),
`contact_outcome` (the chosen value), `outcome` (the result tag:
`corrected` / `unchanged` / error kind, as other commands use it),
`changed`. No free
text exists to leak; the log-capture test from Slice 006 still applies.

| Condition | Behavior |
|---|---|
| Save same outcome | 200 `changed: false`, nothing written or published |
| Two concurrent saves on one call | serialize on the call lock; both 200 — the later chains onto the earlier (or `changed: false` if equal); at most one new row each |
| Call still active | 409 `invalid_call_state`; the panel only offers the prompt after the terminal state |
| No attempt (nothing reached the callee) | 422; the panel never shows the prompt in that case |
| Publish failure | logged, never a failed command (D-023) |

## 10. Frontend (Lane B, `web/**`; `UI_STYLE.md` binds)

- `api/types.ts`: `ContactOutcome` gains `busy`, `wrong_number`;
  `CallOutcomeCorrection`; `ContactAttemptedDetail` gains `call_id`,
  `corrects_id`, `superseded`; `CorrectOutcomeResponse`.
- `api/queries.ts`: `useCorrectCallOutcome(orgId)` — on success
  invalidate `person`, `today`.
- `lib/labels.ts`: `left_message` → "Voicemail / left message"; labels
  for `busy`, `wrong_number`.
- `CallPanel.vue`: post-call block becomes the prompt when
  `attemptOutcome(call) !== null` (props: observed outcome, server
  status; emits `save-outcome(outcome)` / `skip`). **Save is enabled only
  when the server's `call.status ∈ {ended, failed}`** (the client phase
  turns `ended` before the hangup request completes, and `GET
  /api/calls/{id}` is refetched on `call.changed`); until then the
  picker is visible and Save disabled. Save is the one primary; Skip is
  ghost. After save: "Outcome saved — voicemail" then Done. Error copy:
  `invalid_call_state` → "The call hasn't finished yet."; `no_contact_
  attempt` → "There's no contact attempt to correct."; `correction_
  conflict` → "This outcome was just changed — refreshed."; `forbidden`
  → "Only the caller can change this outcome."; `not_found` → "This
  call no longer exists." (both added at implementation, 2026-08-22);
  others → the generic pattern. Ringback (§12) shipped in Lane B: a
  local WebAudio tone while `phase === 'ringing'`, independent of mic
  mute.
- `PersonDetailView.vue` owns the mutation and folds history into one
  row per call (§1.3): for each `call_completed` entry, the effective
  attempt (same `call_id`, `superseded === false`) supplies the outcome
  label; a `corrects_id` on it adds "outcome corrected from <root
  outcome> <when>"; call-derived attempt rows are hidden (fallback: if
  no `call_completed` row exists they render as before). **Change
  outcome** ghost action sits on the call row for the caller's own calls
  with a non-superseded attempt (§1.7).
- `LogContactDialog.vue`: no change needed (derives options from the
  label map); verify.

## 11. Development environment and configuration

No new configuration. Migration runs with `db-migrate`.

## 12. Explicit exclusions

Notes (D-032 note; a Note-on-a-Person slice later); answering-machine
detection; Today resurfacing rule for voicemail (a separate decision —
would also change manual-attempt behaviour); correcting manual
(non-call) attempts (the route is call-scoped); a generic "correct any
fact" mechanism; corrections of `call_completed` (it records what the
system observed and is correct as such); disposition analytics;
ringback tone (a separate tiny Lane B item, may ride along in Lane B's
branch if trivially small — `UI_STYLE`-neutral, local audio while
`ringing`); Operator involvement beyond the §4 rendering fix; a cap on
chain length (a caller may correct repeatedly; each is one row).

## 13. Checks and tests

1. **Service-free (`./scripts/check`)** — `CallOutcomeCorrection` rejects
   `sent` and unknown values; `ContactOutcome` round-trips the two new
   values; 401/400 on the route; error-code mapping.
2. **DB-backed (`check-db`)** — migration: CHECK accepts `busy` /
   `wrong_number` and still rejects others; partial unique index exists;
   append-only still holds for `crm_app` and owner (`db_schema.rs`
   enumerations updated); answered scripted call → correct to
   `left_message`: exactly two rows, original untouched, correction has
   `corrects_id = original`, same `occurred_at`, later `recorded_at`,
   `causation_id = call.id`, `correlation_id = call.correlation_id`,
   actor = caller, origin `web_session`; the recording publisher saw
   exactly one `person.changed{contact_attempted}`; same outcome →
   `changed: false`, row count unchanged, nothing published; second
   correction corrects the first (chain), head lookup returns it;
   concurrent double correction (`tokio::join!`) → both 200, exactly
   one or two new rows forming a chain, never two corrections of the
   same head; `recorded_at` strictly increasing along the chain; history
   order pinned as original, `call_completed`, then correction (the
   correction's history `occurred_at` = its `recorded_at`) for both an
   answered and a busy call (correct `no_answer` → `busy`; also proves a failed-with-attempt
   call is correctable); a `placing → cancelled` call (no attempt) → 422;
   non-caller on an *active* call → 403 (403 precedes 409);
   `tokio::join!(hangup, correct)` → 409 or 200, at most one correction; a
   correction row is itself append-only (UPDATE/DELETE rejected for
   owner and `crm_app`); the manual `POST …/contact-attempts` accepts
   `busy` and `wrong_number`; Operator `get_person` marks the superseded
   row and the correction (`db_operator.rs`);
   non-caller 403; foreign 404 both directions; `answered` (active)
   call 409; `provider_error` call 422; Today: membership unchanged
   for caller and a non-caller member, and a third member who has the
   Person by assignment sees `last_contact_attempt.outcome =
   left_message`, never the superseded row; history: original
   `superseded: true` then correction with `corrects_id` and `call_id`;
   a manual attempt has `call_id: null`; the route works with
   `AppState.telephony = None`; Slice 006's log-capture test still green.
3. **Web (Vitest)** — prompt iff an attempt exists; pre-selection per
   observed outcome; Save disabled while `call.status = 'answered'` and
   enabled after the refetch shows `ended`; Skip sends nothing; Save
   posts the exact body and shows the saved line; same-outcome save closes; error copy per code;
   one primary; history rendering of superseded/corrected rows; Change
   outcome visible only to the caller on non-superseded call rows; the
   manual dialog offers the new values.
4. **Live walkthrough** — one answered call corrected to Voicemail from
   `app.tarams.org`; a second tab's Today card shows the corrected
   outcome; History shows both rows.

## 14. Safe defaults adopted (overridable at approval)

1. Correction = new `contact_attempted` row with `corrects_id`; linear
   chain via partial unique index; `occurred_at` inherited.
2. Voicemail → `left_message` (relabelled); `busy` and `wrong_number`
   added; `sent` rejected on this route.
3. Caller-only correction.
4. Prompt only when an automatic attempt exists; equal outcome no-op.
5. History detail gains `call_id`, `corrects_id`, `superseded`.
6. No Today membership change; effective-row filter in the Today query.
   D-032's "may resurface voicemail sooner" is deliberately not
   exercised in 006c (it would also change manual-attempt behaviour);
   it stays a separate decision.
7. Change-outcome-from-History included as Lane B's last, cuttable item.
8. 006c before 006a (no technical dependency).

## 15. Lane ownership and sequencing

- **Lane A — backend** (`backend/**`, the migration): migration;
  `ContactOutcome` variants; `facts.rs` `corrects_id`; command + route +
  `ApiError` variants; Today effective-row filter; history detail;
  `CorrectedAttemptRef`; the Operator history-text fix; `.sqlx`; tests.
  ≈ 1.5 days. **HTTP contract reachable after the route
  lands.**
- **Lane B — web** (`web/**`): types/queries/labels; `CallPanel` prompt;
  `PersonDetailView` mutation + rendering; optional Change outcome;
  optional ringback; Vitest. ≈ 1 day; integrates after Lane A's route.
- **Coordinator**: this spec, the pointer lines listed in §2,
  `envelope.rs` doc intent, decision log, `PROJECT_STATE.md`.

## 16. Next

006a — the `crm-app` extraction. 006b — Operator `start_call`. Later:
Note on a Person; Today resurfacing for voicemail (decision); inbound
calling.
