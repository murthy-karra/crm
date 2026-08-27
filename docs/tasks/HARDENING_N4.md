# Hardening chunk N4 — call cluster (`CallId`, `ContactMethodId`, `ProposalId`, `CorrelationId`, `InvitationId`, `TurnId`)

Parent plan: `docs/design/type-safety-hardening.md` (chunk 6).
Branch: `hardening-n4`, worktree `/Users/karrad/projects/crm-wt-n4`
— a PARALLEL LANE: chunk V1 runs simultaneously in another worktree.
Single writer within this lane. Read `AGENTS.md` first, then the
predecessor briefs (`docs/tasks/HARDENING_{S1,N1,N2,A1,N3}.md`) —
every invariant applies verbatim: no SQL string changes (`.0` binds,
`.sqlx` byte-untouched with proof), no wire changes
(serde-transparent), no migrations, crm-operator untouched, no
implicit conversions, no unrelated cleanup.

## LANE OWNERSHIP (hard boundary — parallel-lane safety)

You may edit: `ids.rs`, telephony (queries, settle, sweep, dial_task,
webhook), `facts.rs`, `envelope.rs`, realtime `events.rs`/
`publisher.rs`, commands (`start_call`, `dial_call`, `hangup_call`,
`correct_call_outcome`, `log_contact_attempt`, `receive_inquiry` —
correlation only), admin invitation files, crm-api routes
(`calls.rs`, `operator.rs`, `livekit_webhook.rs`, `invitations.rs`),
`operator/backend.rs`, `operator/mod.rs`, tests.

You must NOT edit: `domain/contact.rs`, `domain/inquiry/parse.rs`,
`domain/intake/email/**`, `domain/intake/extraction/mod.rs`,
`domain/person/queries.rs`'s contact-method fns — those belong to the
V1 lane. If a change you need forces an edit there, STOP that edit
and report it as a shared-contract conflict instead of making it.

## Goal

Six types joining `ids.rs`, mirroring the existing ones exactly:
`CallId`, `ContactMethodId`, `ProposalId`, `CorrelationId`,
`InvitationId`, `TurnId`. No new compile_fail doctests (mechanism
proven; the existing ones cover the class).

Kills the remaining survey adjacencies:
- `RealtimeEvent::call_changed(org, occurred_at, correlation_id,
  call_id, person_id)` — every param now distinct;
- `propose_start_call(ctx, person_id, contact_method_id)` seam sites
  (crm-operator TRAIT stays bare `Uuid` — wrap at crm-api entry like
  N3 did for person_id);
- `finalize_failed(proposal_id, call_id)` and the proposal/turn
  routes;
- `mark_invitation_accepted(invitation_id, accepted_user_id)`;
- `FactEnvelope.correlation_id: CorrelationId` (`causation_id` STAYS
  `Option<Uuid>` — the cross-fact union, per envelope.rs docs; do not
  type it);
- `CallCompletedFact.call_id`/`contact_method_id`,
  `ContactAttemptedFact.contact_method_id`;
- telephony `settle(org, call_id)`, `head_attempt`'s call half,
  `CallRow.id`, `NewCall`, `DialTask.call_id`;
- `routes/calls.rs`: the generic bare `PathId` extractor serves BOTH
  a person id and a call id — split it (e.g. `CallIdPath(CallId)`
  beside N3's typed-extractor pattern; the person route keeps/gains
  its own). Orphan-rule rename rationale as N3.

Turn ids: `operator_turn`/`TurnSlot`-adjacent ids in crm-api that are
genuinely turn ids get `TurnId`; the turn-id-as-correlation chain
(`correlation_id = turn id` on operator-driven facts) converts
explicitly (`CorrelationId::new(turn_id.as_uuid())` at the one
crossing) — a visible, commented conversion, not an implicit one.

## Checks before reporting done

Worktree specifics: run `pnpm install --frozen-lockfile` in `web/`
first if `./scripts/check`'s web steps fail on missing node_modules
(fresh worktree). Then `cargo fmt`, `source ~/.nvm/nvm.sh &&
./scripts/check`. Do NOT run `./scripts/check-db` (the parallel lane
shares the Postgres server; the coordinator runs the db gate serially
per lane). Instead run the affected suites directly:
`source .env && DATABASE_URL="$MIGRATION_DATABASE_URL" cargo test
-p crm-api --test db_calls --test db_operator_call --test db_operator
--test db_admin --test db_realtime -- --ignored` (plus any suite your
diff touches). `git status --short backend/.sqlx/` must be empty.

Report format as predecessors: files changed matching `git status`
exactly, anchors typed, seam conversions, the PathId split, checks +
results, assumptions, surprises — and explicitly confirm you never
edited the V1-owned files.
