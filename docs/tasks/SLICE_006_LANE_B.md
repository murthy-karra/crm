# Task brief — Slice 006, Lane B (web)

Parent specification: `docs/specs/SLICE_006.md`. Read: `AGENTS.md`
(§5, §7), `docs/decisions/DECISION_LOG.md` (D-017, D-023, D-030,
D-031), SLICE_006 §5, §6, §9, §10, §13 item 4, `docs/design/UI_STYLE.md`
(binding), then `web/src/realtime/{useRealtime,events}.ts` (+tests —
the injected-client-factory pattern), `views/PersonDetailView.vue`,
`components/LogContactDialog.vue`, `api/{client,queries,types}.ts`,
`lib/{controls,errors,format}.ts`.

## Outcome

`telephony/useCall.ts` (state machine over an injected LiveKit client
factory), `CallPanel.vue`, the **Call** button + number picker on
`PersonDetailView.vue`, `call_completed` history rendering, types and
mutations (`useStartCall`, `useDialCall`, `useHangupCall`, `useCall`),
`call.changed` invalidation mapping, §10 error copy, Vitest per §13
item 4. `livekit-client` pinned as the one new dependency.

## Ownership boundary

`web/**` only. Branch `slice-006-web` from Lane A's branch once its step
3 lands (or from `main` against the mocked §5 contract until then).

## Frozen contracts

§5 routes/bodies/codes, §6 `call.changed`, §10 states and copy. Hard
rules: the client never sends a phone number (only `contact_method_id`);
`hangup` is called exactly once per call from the client; audio
elements only for the `sip:*` participant; no call state persisted
beyond component state; one primary button per view.

## Sequence

1. Types + queries + `invalidationsFor('call.changed')` + tests.
2. `useCall` with a fake LiveKit client: every transition, remote leave
   → hangup once, mic denied → hangup, `call.changed` → refetch; tests.
3. `CallPanel.vue` + Person header integration + history kind; error
   copy; tests.
4. Integrate against Lane A on loopback (scripted provider), then the
   live walkthrough with Lane C's host.

## Required checks

`pnpm run lint`, `typecheck`, `test`, `build`; the walkthrough.

## Stop and report

Any §5/§6 change; any need for a backend change; any temptation to
drive call state from Centrifugo instead of LiveKit events (D-023 keeps
Centrifugo invalidation-only).
