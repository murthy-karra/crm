# Slice 006b — Lane B (web): call host + proposal card

Read first: `AGENTS.md`, `docs/specs/SLICE_006b.md` (whole, esp. §1,
§4, §6, §13), SLICE_006c §5a (forced outcome — must survive verbatim),
docs/design/UI_STYLE.md.

Ownership: `web/**` only. Do NOT touch `backend/**` or `docs/**`.

Deliverables:
1. **Call host lift** (§6): `web/src/telephony/callHost.ts` +
   `<CallPanel>` mounted in `AppShell.vue`; PersonDetailView's Call
   button and the drawer Confirm both feed it; `useCall.adopt({call,
   join})`. The D-033 forced-outcome prompt + outcome mutations move
   with the panel. Behavior preserved verbatim — the 006c Vitest
   regressions are the gate; extend them to the host, do not weaken.
2. **Drawer proposal card** (§1, §6): render only from the turn
   response `proposal` object; Confirm primary / Dismiss ghost (local);
   expiry disable + "This suggestion expired — ask again."
3. **Confirm flow**: mic permission FIRST, then
   `POST /api/operator/proposals/{id}/confirm`; on 200 adopt into the
   call host; map §4 errors to copy (consumed/expired/call_in_progress
   reuse existing patterns).
4. Types in `web/src/api/types.ts` + query/mutation in `queries.ts`.
5. Vitest per §13.

Frozen contract: §4. Mock it until Lane A's route lands; integrate
after. If the contract doesn't fit, STOP and report.

Checks: `pnpm lint`, `typecheck`, `test`, `build` (source
`~/.nvm/nvm.sh` first). Report: files, behavior, checks, deviations.
