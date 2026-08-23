# Task brief — Slice 006c, Lane B (web)

Parent specification: `docs/specs/SLICE_006c.md` (APPROVED). Read:
`AGENTS.md` (§5, §7), `docs/decisions/DECISION_LOG.md` (D-017, D-023,
D-031, D-032), SLICE_006c §1, §5, §6, §10, §13 item 3, SLICE_006 §10,
`docs/design/UI_STYLE.md` (binding), then `web/src/telephony/{useCall,
format}.ts`, `components/{CallPanel,LogContactDialog}.vue` + tests,
`views/PersonDetailView.vue` + test, `api/{client,queries,types}.ts`,
`lib/labels.ts`, `lib/controls.ts`.

## Outcome

§10: types (`ContactOutcome` + `busy`/`wrong_number`,
`CallOutcomeCorrection`, `ContactAttemptedDetail` fields,
`CorrectOutcomeResponse`, `CorrectedAttemptRef`), `useCorrectCallOutcome`,
labels, the `CallPanel` "How did it go?" prompt (Save enabled only when
the server `call.status ∈ {ended, failed}`; Skip ghost; one primary),
`PersonDetailView` mutation + history rendering (superseded muted,
"Outcome corrected — <label>"), optional **Change outcome** action
(last; cuttable), error copy per §10, Vitest per §13 item 3. Optional:
a local ringback tone while `ringing` if trivially small and
`UI_STYLE`-neutral — report separately if included.

## Ownership boundary

`web/**` only. Branch `slice-006c-web` from Lane A's branch once the
route lands (or from `main` against the §5 contract with mocked
responses until then).

## Frozen contracts

§5 route/body/codes/`CorrectedAttemptRef`, §2 history detail, §10 copy
and states. Hard rules: Save only when the server says terminal; the
prompt only when `attemptOutcome(call) !== null`; Skip sends nothing;
no free text anywhere; phase is never driven by Centrifugo (D-023).

## Required checks

`pnpm run lint`, `typecheck` (real project config), `test`, `build`.

## Stop and report

Any §5/§2/§10 change; any backend need.
