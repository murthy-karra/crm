# Slice 015 (Notes) — live walkthrough (spec §9.11)

Recorded 2026-09-09 by the implementation lane against its own API
(`127.0.0.1:31015`) and Web dev server (`127.0.0.1:51015`), driven live with
the Playwright MCP browser tools (`browser_navigate`, `browser_snapshot`,
`browser_click`, `browser_type`, `browser_take_screenshot`) — never the
shared dev runtime, never ports 3000/5173. Database: a fresh
`crm_slice015_qa` on the same Postgres server as `MIGRATION_DATABASE_URL`,
migrated with the worktree's own `./scripts/db-migrate` under env overrides,
bootstrapped with `crm-admin bootstrap-platform-admin` and seeded with two
Organizations (Acme Realty: alice admin, carol member; Best Realty: bob
admin, dave member) via `scripts/seed_dev.py` against the QA API — no direct
database writes (D-021). LiveKit is down (PROJECT_STATE); the Call button was
never clicked.

Person used: **Grace Hopper** (`grace.hopper@slice015-qa.test`), created in
Acme Realty via the Web "New lead" form during the walkthrough itself.

| # | Scenario (§9.11) | Result | Evidence |
|---|---|---|---|
| 1 | Alice writes a note on the Person | PASS | composer clears; "Note by Alice Anderson" row appears with the pre-wrap body |
| 2 | Alice edits a typo, sees "edited" | PASS | row reads "Note by Alice Anderson · edited" with the corrected body |
| 3 | Carol (a plain Acme member) sees the note after a refetch, with no Edit/Delete | PASS | note row renders for Carol; no `Edit note`/`Delete note` buttons present |
| 4 | The admin (Alice) deletes it and it disappears | PASS (observed for Alice only) | ConfirmDialog reads exactly "Delete this note? This cannot be undone."; row gone for Alice after confirm. No screenshot of Carol's view after the delete was taken; her side of "disappears for both" is not directly evidenced by this walkthrough |
| 5 | A second Organization's identical Person shows nothing throughout | Not run as specified | This walkthrough did not create a second, identically-named Person in Best Realty. What was observed instead: Bob (Best Realty) navigating directly to Grace Hopper's Person id got "Person not found." — this demonstrates id-level tenant isolation (a foreign Person id is invisible), not the spec's own scenario of a second Organization's own, separately-created identical-looking Person showing no notes |
| 6 | The Operator, asked about the Person, can quote the note | PASS | asked "What does the latest note on Grace Hopper say?" before the delete step; replied with the note's exact text, quoted, plus the Person reference card |

Screenshots (`screenshots/`): `01-person-before-note.png` (composer at the
top of the History card, before any note exists), `02-note-added.png`,
`03-note-edited.png` (the "edited" marker), `04-operator-quotes-note.png`,
`05-carol-view-only.png` (no Edit/Delete for a non-author member),
`06-delete-confirm-dialog.png` (the exact confirm copy), `07-note-deleted-alice.png`,
`08-cross-org-not-found.png` (Bob's cross-Organization 404).

## Environment notes

- Realtime (Centrifugo) was not wired up for this ad hoc QA environment — the
  Web console logged repeated WebSocket handshake 403s against the shared
  dev Centrifugo instance, which does not recognize this QA Organization's
  connection token. This is an environment-configuration gap, not a product
  defect: every scenario above used a normal page load/refetch to observe
  state, which is the walkthrough's own bar ("bob sees the note after
  refetch"), and Slice 015's `note_changed` realtime contract is already
  covered by `db_realtime::note_changed_payload_carries_no_body_key` and the
  Vitest `invalidationsFor` unit test at the code level.
- The QA API (`127.0.0.1:31015`) reported `telephony enabled provider=livekit`
  at startup (from `.env`'s real `LIVEKIT_API_KEY`) even though the LiveKit
  server itself is down; the Call button was never exercised, per the
  standing instruction.
- `crm_slice015_qa` is left in place for the coordinator, per instruction;
  the two processes started for this walkthrough (the QA API and the QA Web
  dev server) were stopped by their exact PIDs once the walkthrough
  finished.

## Addendum: realtime observed on the shared development runtime (coordinator, 2026-09-09)

After the merge (`fd5a184`), `crm_dev` migration, dev API relaunch and
`dev-web-prod` rebuild, the coordinator opened the same Person (Skip
Carolson, a seeded fixture) as Alice in two tabs at `127.0.0.1:5173`,
added a note in tab two and watched it appear in tab one without a reload
(`screenshots/09-realtime-tab-one-dev-runtime.png`), then deleted it from
tab one and watched it vanish from tab two without a reload. This closes
the one §9.11 item the QA environment could not exercise (Centrifugo was
not wired there). The test note was deleted; `crm_dev` holds no notes.
