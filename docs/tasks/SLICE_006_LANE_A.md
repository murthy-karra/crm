# Task brief — Slice 006, Lane A (backend)

Parent specification: `docs/specs/SLICE_006.md`. Read, in order:
`AGENTS.md` (§4, §5, §7, §9, §11), `docs/decisions/DECISION_LOG.md`
(D-002, D-007, D-008, D-013, D-015, D-016 §5, D-021, D-022, D-023,
D-030, D-031, O-002, O-011), the full SLICE_006 spec,
`docs/specs/SLICE_002.md` §2/§5 (fact envelope, append-only triggers,
history entries), `docs/specs/SLICE_003.md` §2/§6 (contact_attempted,
realtime events), then the existing code: `domain/commands/
log_contact_attempt.rs`, `domain/envelope.rs`, `domain/facts.rs`,
`domain/person/queries.rs` (history sources), `realtime/{events,
publisher,token}.rs` (HS256 pattern), `operator/mod.rs` (runtime +
test-support provider pattern), `config.rs`, `state.rs`, `error.rs`,
`routes/`, `tests/common`, migrations.

## Outcome

SLICE_006 §1 backend: `telephony/` (trait, `LiveKitProvider`,
`ScriptedProvider`, `JoinTokenSigner`, `WebhookVerifier`, config),
migration `20260825000001_calls.sql`, `domain/telephony/{transitions,
settle,queries}.rs`, commands `start_call`/`dial_call`/`hangup_call`,
routes + the webhook, the dial task, the sweep, the `call_completed`
history kind, `call.changed` event, `scripts/telephony-trunk`, every
test in §13 items 1–3, `.env.example`, README Development additions.

## Ownership boundary

Owns `backend/**`, `scripts/**`, `.env.example`, README Development
section, **the one migration**. Does not touch `web/**`, `infra/**`, or
`docs/**`. Branch `slice-006-calling` from `main`, main checkout.

## Frozen contracts

§2 (DDL, state machine, D-031 mapping, history kind), §3 (trait, token
claims, command signatures), §5 (routes, bodies, codes), §6 (event
shape), §11 (config names/bounds). Any deviation stops work and is
reported per AGENTS §11.

## Sequence (each step green before the next)

1. Config (`LIVEKIT_*`, `CRM_TELEPHONY_*`; disabled without key; bounds
   and URL rules; redacting newtypes) → `telephony/` trait, errors,
   `ScriptedProvider` (test-support), `JoinTokenSigner` + tests,
   `WebhookVerifier` + tests, `AppState.telephony: Option<Arc<Telephony>>`.
2. Migration exactly as §2 → `domain/telephony/transitions.rs` (pure,
   every state × signal tested) → `settle` → `.sqlx` regenerated.
3. Commands + routes + `POST /webhooks/livekit` (outside session/CORS
   layers) + `ApiError` variants. **HTTP contract reachable — tell the
   coordinator.**
4. `LiveKitProvider` (Twirp JSON; HS256 JWT with `video` grants) → dial
   task → sweep.
5. `call_completed` history source (`kind_rank` 5) + `call.changed`
   publication.
6. Tests per §13 items 1–3 (`tests/telephony.rs`, `tests/db_calls.rs`,
   optional `tests/livekit_telephony.rs` gated on `LIVEKIT_API_URL`,
   fails loudly never skips).
7. `scripts/telephony-trunk`, `.env.example`, README.

## Required checks

`./scripts/check` (no services, no network); `./scripts/check-db`; with
Lane C's host up: the LiveKit-backed test and one real call (report the
SIP status flow and timings, never the number).

## Stop and report

Any §2/§3/§5/§6/§11 shape change; any Telnyx credential or phone number
reaching the application's config, logs, or tables; any need to widen
`ToolBackend` or touch the Operator; any existing test that must change;
LiveKit SIP behaving in a way the state machine cannot absorb.

## Report format

Files changed (from `git status`); behaviour per §1 step; commands run
with results; contract changes (none expected); risks; the commit Lane B
can integrate against.
