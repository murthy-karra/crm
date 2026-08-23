# Slice 006c — Call outcome correction: plan (crm-planner pass, 2026-08-22)

Status: PLAN (not a spec). Driving decision: D-032. Spec to follow once
the one blocking decision (notes) is resolved.

## Dependency

006a (`crm-app` extraction) is not a technical prerequisite: no
Operator involvement. Order is a preference.

## User-visible outcome

1. When a call ends, the panel shows **"How did it go?"**: Talked to
   them / Voicemail / No answer / Busy / Wrong number, pre-selected from
   what the system observed (answered → Talked to them; `no_answer`
   failure → No answer). **Save outcome** (primary) / **Skip**.
2. Skip writes nothing. Save records a correction; History shows the
   original "Contact attempted — call, reached" muted as superseded,
   then "Outcome corrected — voicemail · Alice". Today's
   `last_contact_attempt` shows the corrected outcome; open tabs update
   via `person.changed`.
3. Saving the already-recorded outcome is a no-op (`changed: false`).
4. Calls where nothing reached the callee (`agent_not_joined`,
   `provider_error`, `expired`, cancel before ringing) show no prompt.
5. Optional (cuttable): **Change outcome** on a call-derived history row,
   caller only.
6. No notes field (pending decision).

## Domain model

A correction is a new `contact_attempted` row with `corrects_id` = the
head of the chain (the latest uncorrected row for the call). Facts stay
immutable (D-015); this is the first writer of `corrects_id`. Row:
`person_id`/`channel` copied; `outcome` chosen; **`occurred_at` =
corrected row's** (only the description changes; keeps Today/timeline
position stable), `recorded_at = now()`; actor = correcting user;
`origin = web_session`; `correlation_id = call.correlation_id`;
`causation_id = call.id`. Migration adds
`UNIQUE (corrects_id) WHERE corrects_id IS NOT NULL` (linear chain;
concurrent double correction → 409). Effective attempt = row with no
corrector.

## Vocabulary

`contact_attempted.outcome` CHECK widened to
`reached | no_answer | left_message | sent | busy | wrong_number`.
Talked to them → `reached`; Voicemail → `left_message` (relabel to
"Voicemail / left message" in `web/src/lib/labels.ts`); No answer →
`no_answer`; Busy → `busy` (new); Wrong number → `wrong_number` (new).
D-031's automatic busy → `no_answer` mapping unchanged. The manual
Log-contact dialog gains the two new values automatically. Operator
`get_today` output widens additively.

## Command and HTTP

`correct_call_outcome(pool, publisher, ctx, {call_id, outcome})`, one tx:
lock call (org-scoped) → 404; caller only → 403; status not terminal →
409 `invalid_call_state`; head attempt by `causation_id = call.id` with
no corrector → none → 422 `no_contact_attempt`; equal outcome → no
write, `changed: false`; insert correction; 23505 → 409
`correction_conflict`; commit; publish `person.changed{contact_attempted}`.

`POST /api/calls/{id}/outcome` body `{"outcome": <one of five>}`
(`deny_unknown_fields`; `sent` → 400) → 200 `{"attempt": {id, channel,
outcome, occurred_at, corrects_id}, "changed": bool}`; errors 400/401/
403/404/409/422/503 as above.

Declared additive change (AGENTS §11): `history[].contact_attempted.
detail` gains `call_id`, `corrects_id`, `superseded`. `CallView` and
`TodayItem` shapes unchanged.

## Today

No membership change in 006c; the correction inherits `occurred_at`
so it can never move a Person on/off Today. Required: the
`last_attempt` LATERAL in `today/queries.rs` must select the effective
row (`NOT EXISTS corrector`), else the equal-`occurred_at` tie-break is
random. Resurfacing voicemail sooner is a separate decision.

## Realtime

`person.changed{contact_attempted}` only; no `call.changed`.

## Tests

Service-free: enum rejects `sent`/unknown; 401/400 on route.
DB-backed: migration CHECK + partial index + append-only; two rows
after correction with the exact envelope; equal outcome no-op and
nothing published; chain (second correction corrects the first);
concurrent double correction → one 200 one 409; non-caller 403; foreign
404 both directions; active call 409; `provider_error` call 422; Today
shows the effective outcome and membership unchanged; history fields;
log-capture still clean. Web: prompt iff an attempt exists;
pre-selection; Skip sends nothing; Save flow; error copy; history
rendering; Change-outcome visibility; dialog offers new values. Live:
one answered call corrected to Voicemail, second tab's Today updates.

## Out of scope

Ringback tone (separate tiny Lane B item); AMD; analytics; notes;
Today resurfacing; correcting manual attempts; generic fact correction;
Operator; `call_completed` corrections.

## Lanes

Lane A backend (migration `20260826000001_contact_outcome_correction.sql`,
`ContactOutcome` variants, `facts.rs` `corrects_id`, command + route +
`ApiError` variants, Today filter, history detail, `.sqlx`, tests)
≈ 1.5 d. Lane B web (`types`/`queries`, `CallPanel` prompt,
`PersonDetailView` mutation + rendering, labels, optional Change
outcome, Vitest) ≈ 1 d. Coordinator: spec, SLICE_002 §5 pointer,
SLICE_003 §2 note, `envelope.rs` doc intent, state.

## Open

BLOCKING: notes (D-032 vs. no-free-text-on-facts). SAFE DEFAULTS: the
model/vocabulary/auth above; no Today resurfacing; 006c before 006a.
CONFLICT to state in the spec: SLICE_006 §12 excluded "corrections of
auto-logged attempts"; 006c supersedes that exclusion.
