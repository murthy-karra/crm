# Hardening chunk N3 — intake id cluster (`PersonId`, `InquiryId`, `RawPayloadId`, `StageId`)

Parent plan: `docs/design/type-safety-hardening.md` (chunk 5).
Branch: `hardening-n3` (branched from `hardening-a1` — A1's Actor
enum is already in your tree). Single lane, one writer. Read
`AGENTS.md` first, then the predecessor briefs
(`docs/tasks/HARDENING_{S1,N1,N2,A1}.md`) — every invariant applies
verbatim: no SQL string changes (`.0` binds, `.sqlx` byte-untouched
with proof), no wire changes (serde-transparent), no migrations,
crm-operator untouched (bare `Uuid` at the seam, wrap/unwrap in
crm-api exactly as org/user do), no implicit `From`/`Into`, no
unrelated cleanup.

## Goal

Four new id types in `crm-app/src/ids.rs`, mirroring
`OrganizationId`/`UserId` exactly (repr/serde transparent,
`new`/`as_uuid`, Display-delegating Debug, unit tests):
`PersonId`, `InquiryId`, `RawPayloadId`, `StageId`. One additional
```compile_fail,E0308 doctest demonstrating one representative
cross-confusion (e.g. `PersonId` where `InquiryId` is expected) —
do not write one per pair, the mechanism is already proven.

This kills the survey's worst remaining adjacencies:
- `insert_person(tx, org, first, last, stage_id, assigned_user_id)` —
  stage/assignee adjacency;
- `crypto::seal/open(key, org, raw_payload_id, …)` — the payload half
  of the AAD (org half typed since N1; bytes stay identical);
- `mark_resolved(id, org, inquiry_id)` and the store fns' `(id, org)`
  pairs — payload-vs-org and payload-vs-inquiry;
- `head_attempt(org, call_id, person_id)`'s person half (call stays
  bare until N4 — if typing a call/contact-method/proposal id is ever
  needed to make something compile, STOP: leave bare, it is N4's);
- realtime `person_changed`/`intake_unresolved_changed` data structs
  and constructors (`PersonChangedData` etc.) — serde-transparent,
  exact-shape pins must pass unmodified.

## Sweep guidance

Same compile-guided passes as N1/N2. Core anchors first:
`ReceiveInquiryOutcome` variants, `CompleteIntake`/`ReceiveInquiry`
fields (`raw_payload_id`, person/inquiry outputs), person
model/queries (`PersonRef`? follow the code), inquiry queries
(`NewInquiry`), stage.rs (`stage::exists(stage_id, org)` and the
seeded-stage lookups), raw_payload store (`id` params are
`RawPayloadId`), workbench (`RawPayloadId` path extractor in crm-api
routes/intake.rs — the axum `Path` id), extraction worker
(`ClaimedRow` stays a bare private row; convert at the boundary),
facts structs' person/inquiry/stage fields, realtime data structs.

Known judgement calls, decided in advance:
- `causation_id` stays `Option<Uuid>` (a cross-fact-table union by
  design — envelope.rs docs; do NOT type it).
- `correlation_id` stays bare (N4).
- Fact-table row ids / history query rows: private rows stay bare;
  public returned structs typed — same boundary rule as always.
- Telephony: only the `person_id` fields/params this cluster owns;
  `call_id`/contact-method stay bare (N4).
- Web wire: `person_id`/`inquiry_id`/`stage_id` in JSON — transparent,
  byte-identical; route contract pins prove it.

## Checks before reporting done

`cargo fmt`, then `source ~/.nvm/nvm.sh && ./scripts/check` and
`./scripts/check-db`; `git status --short backend/.sqlx/` must be
empty. Known flake: db_calls "a_second_correction_chains…" — if it
fails once, rerun isolated then the full gate again. Same report
format as N1/N2 (files changed matching `git status` exactly — note
docs/plans/PROJECT_STATE.md and docs/tasks/HARDENING_N3.md are
COMMITTED on this branch already, so `git status` should show only
your edits; core structs typed; conversion sites; checks + results;
assumptions; surprises).
