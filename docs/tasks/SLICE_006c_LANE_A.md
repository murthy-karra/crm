# Task brief — Slice 006c, Lane A (backend)

Parent specification: `docs/specs/SLICE_006c.md` (APPROVED). Read, in
order: `AGENTS.md` (§4, §5, §7, §9, §11), `docs/decisions/DECISION_LOG.md`
(D-015, D-021, D-022, D-023, D-031, D-032), the full SLICE_006c spec,
SLICE_003 §2/§3/§5, SLICE_006 §2/§3, then the code: `domain/envelope.rs`,
`domain/facts.rs` (`insert_contact_attempted`), `domain/commands/
log_contact_attempt.rs` (`ContactOutcome`, `ContactAttemptRef`, span
style), `domain/commands/{start_call,hangup_call}.rs` + `domain/telephony/
{queries,settle}.rs` (`lock_call`, `CallError`, envelope for call facts),
`domain/today/queries.rs` (the `last_attempt` LATERAL + doc comment),
`domain/person/queries.rs` (history UNION, `contact_attempted` detail),
`operator/backend.rs` (history text for `contact_attempted`),
`routes/calls.rs`, `error.rs`, migration `20260822000001_contact_attempted.sql`,
`tests/{db_calls,db_today,db_contact_attempts,db_operator,db_schema}.rs`.

## Outcome

SLICE_006c §2 migration `20260826000001_contact_outcome_correction.sql`;
`ContactOutcome::{Busy, WrongNumber}`; `facts::insert_contact_attempted`
with `corrects_id` and explicit `recorded_at`; command
`correct_call_outcome` (§3, exact step order, call lock first,
`clock_timestamp()` after the lock); `CallOutcomeCorrection` enum;
`CorrectedAttemptRef`; route `POST /api/calls/{id}/outcome` (§5);
`ApiError::{NoContactAttempt, CorrectionConflict}`; Today effective-row
filter (§3) + doc comment; history detail `call_id`/`corrects_id`/
`superseded`; Operator history-text fix (§4); `envelope.rs` doc line on
`causation_id = call.id` for corrections; `.sqlx` regenerated; every
test in §13 items 1–2.

## Ownership boundary

`backend/**` only (including `crates/crm-operator`? — NO: the text fix is
in `crm-api/src/operator/backend.rs`; do not touch `crates/crm-operator`).
Branch `slice-006c-outcome` from `main`, main checkout, single writer.
No `web/**`, no `docs/**`.

## Frozen contracts

§2 (DDL, row contents, vocabulary, history detail), §3 (step order),
§5 (route, body, codes, `CorrectedAttemptRef`), §6. Any deviation stops
work and is reported per AGENTS §11.

## Binding notes

- The auto-named CHECK is `contact_attempted_outcome_check`; verify on
  the live schema before relying on it.
- `recorded_at = clock_timestamp()` passed explicitly on the correction
  insert, after `lock_call`; test `correction.recorded_at >
  head.recorded_at`.
- Head lookup filters `organization_id`, `causation_id = call.id`,
  `person_id = call.person_id`, `NOT EXISTS corrector`, `ORDER BY
  recorded_at DESC LIMIT 1`.
- Equal outcome → roll back, `changed: false`, publish nothing.
- Concurrent saves serialize on the lock: `tokio::join!` test expects
  both 200 and a chain, never two corrections of one head.
- Span `call.outcome`: fields per §9 (`contact_outcome` = chosen value;
  `outcome` = result tag). Ids only.
- `ContactAttemptRef` unchanged; `TodayItem` shape unchanged.
- Manual `POST …/contact-attempts` accepts the two new values — test it.

## Required checks

`./scripts/check`; `./scripts/check-db`.

## Stop and report

Any §2/§3/§5 shape change; any need to change an existing test's
expectations (other than additive enumeration updates in
`db_schema.rs`); any Operator tool-schema change.

## Report format

Files changed (from `git status`); behaviour per §1; commands run with
results; contract changes (none expected); risks; the commit Lane B can
integrate against.
