# Slice 015 — Notes implementation brief

**Status: SPECIFICATION APPROVED by the user on 2026-09-09 ("yes,
proceed"); the lane starts the same day in `../crm-worktrees/notes-1` on
`slice-015-notes`.** The [specification](../specs/SLICE_015.md) is
authoritative for every contract; this brief sequences the work. One rung,
one lane, one writer, one short-lived branch from `main` at `510bcfa` or
later, merged through the coordinator. Model assignment follows the recorded
pattern (`docs/prompts/MODEL_ROUTING.md`): the `implement` profile writes the
lane; the coordinator (Fable) runs review, test analysis, the once-only
final-tree gates and the commit and merge gates.

## Read first

AGENTS.md (§4.3, §4.6, §4.8, §9, §11); DECISION_LOG D-015, D-021, D-023,
D-027, D-029, D-050, D-051, **D-053** and O-012 as amended; SLICE_015.md in
full; SLICE_011e.md §§2/3/5/6 (the pattern this slice copies); SLICE_002
§§2/5/6, SLICE_003 §6, SLICE_005 §5; `docs/prompts/05-implement.md`;
`docs/design/UI_STYLE.md`. Then inspect the code named below before writing.
Report a contract or decision conflict to the coordinator; never resolve it
locally.

## Outcome

Any member writes a note on a Person from the Person page; the note appears
in the timeline at its creation time with the current body; its author or an
Organization admin edits it in place or deletes it (tombstone); the Operator's
`get_person` carries the latest five notes as untrusted text and its history
view excludes note entries; nothing on People rows, Today, filters or the
D-052 columns changes.

## Key files

Backend (`backend/crates/`):

- `crm-api/migrations/20260912000001_note.sql` (new; spec §2 verbatim).
- `crm-app/src/ids.rs` (`NoteId`, the `TagId` pattern).
- New `crm-app/src/domain/note/{mod.rs,model.rs,commands.rs,queries.rs,error.rs}`
  (copy the shape of `domain/tag/`; `NoteBody::parse` in `model.rs` as a
  pure function; the `FOR SHARE` membership re-read is `tag::commands::
  lock_current_membership`, lifted to a shared place or duplicated, lane's
  choice; command structs carrying a body have no `Debug` derive).
- `crm-app/src/domain/mod.rs` (module registration).
- `crm-app/src/domain/person/queries.rs` (`lock_person` reuse; new
  `note_history`; `history_for_person` gains the eighth source; kind `note`,
  `kind_rank` 7; `detail` `{body, updated_at, edited, can_manage: false}`).
- `crm-app/src/realtime/events.rs` (`PersonChange::NoteChanged`).
- New `crm-api/src/routes/notes.rs` (three routes, `PersonNoteIdsPath`
  pair extractor on PUT/DELETE, per-route `DefaultBodyLimit`), `routes/mod.rs`,
  `lib.rs` (router merge), `routes/people.rs` (the detail handler overwrites
  `can_manage` on `note` entries from `AuthContext.role` and `actor.id`).
- `crm-operator/src/views.rs` (`NoteView`, `PersonDetail.notes`),
  `crm-operator/prompts/system.md` (the untrusted-text parenthetical gains
  "notes"), `crm-api/src/operator/backend.rs` (`latest_for_person` → `notes`;
  filter `kind == "note"` out of `history` before the `MAX_HISTORY`
  truncation; no `history_detail` arm).
- Tests: new `crm-api/tests/db_notes.rs` (registered alphabetically in
  `tests/all.rs`), extended `db_schema.rs` (table, grants incl. no `DELETE`,
  indexes), `db_admin.rs` (platform-only 401 with explicit PUT and DELETE),
  `db_people.rs` (detail shape; People rows unchanged), `db_realtime.rs`
  (payload has no `body`), `db_operator.rs` (sentinel test), and the capture
  test using the `CaptureWriter` harness from
  `db_today_source_telemetry.rs`.

Web (`web/src/`):

- `api/types.ts` (`Note`, `NoteDetail`, the `HistoryEntry` `note` arm),
  `api/queries.ts` (three mutations keyed with `personMutationKey`, settled
  through `settlePersonMutation` on `queryKeys.person(orgId, personId)`),
  `realtime/events.ts` (`'note_changed'` in the token union;
  `invalidationsFor` branch: `note_changed` → only `queryKeys.person`),
  `lib/errors.ts` (note-specific `malformed_request` copy where the composer
  renders it), `views/PersonDetailView.vue` and its test (composer, note rows
  with optional note payload on `HistoryRow`, inline edit surviving a
  refetch, delete confirm disabled while pending, code-point counter, the
  generic row for an unrecognised `kind`).

## Order of work (each step gated by its own tests before the next)

1. Migration, `NoteId`, `db_schema.rs`; `./scripts/db-migrate` on the lane
   database; spec §9.1 including the CHECK matrix, the partial unique index,
   the cascade and the 10,000 four-byte-code-point acceptance.
2. `note` module: `NoteBody::parse` with unit tests (§9.2); queries
   (`note_history`, `latest_for_person`); three commands with the spec's
   locks (person `FOR UPDATE` → note `FOR UPDATE` → membership `FOR SHARE`),
   the rule-1 permission check, `changed: false` on a byte-equal edit, the
   tombstone with every other column untouched, and publication only on a
   changed write; `db_notes.rs` for §9.3–9.6 including the same-Organization
   mismatched `(person_id, note_id)` 404s, the demoted/deactivated admin and
   deactivated author cases, the edit-racing-delete `tokio::join!` case, and
   the imported-shape row.
3. Routes and errors (§5 table and precedence), the `note` history kind in
   the detail read with `can_manage` overwritten in the route, `NoteChanged`
   publish; `./scripts/sqlx-prepare`; tests §9.7–9.8 (the 401 enumeration
   with explicit PUT and DELETE; the parsed realtime payload has no `body`).
   **Checkpoint: report to the coordinator when the routes are live, before
   Web work starts.**
4. Operator `PersonDetail.notes` as `UntrustedText`, history filter, prompt
   parenthetical; the §9.9 sentinel test and the capture test; crate fences.
5. Web types, queries, realtime branch, Person page composer and note rows;
   Vitest §9.10.
6. Walkthrough §9.11 against the lane's own API (not the shared dev
   runtime); record under `docs/design/qa/slice-015-<date>/`. LiveKit is
   down (PROJECT_STATE, Environment); nothing here touches calling, so do
   not place calls while verifying.

## Rules

Static SQL only, literal Organization predicates; the note lookup binds
`id`, `organization_id` **and** `person_id`; no fact table, no
`person.updated_at` bump, no D-052 column, no filter clause, no Today change,
no `PersonSummary` change; PUT not PATCH; tombstone not hard delete;
`note_changed` only on changed writes; **no note body in spans (all three
spans `skip_all`), logs, error envelopes, the realtime payload or the ledger**;
D-045 controls and UI_STYLE §5 (40px targets, accessible names); pessimistic
mutations. Owns `backend/**` and `web/**` in sequence (sole migration and
`.sqlx` owner); nothing under `docs/` except the QA record and this brief's
status line, which is coordinator-owned.

A checkpoint commit per step with the `Co-Authored-By: Claude Fable 5.1
<noreply@anthropic.com>` trailer. Gates per round from the worktree root under
the shared gate lock (`mkdir /private/tmp/claude-501/crm-gate.lock`, `rmdir`
after, also on failure), in the background, polled inside one call:
`source ~/.nvm/nvm.sh; ./scripts/sqlx-prepare && ./scripts/check` then
`./scripts/check-db`. Never bind ports 5173 or 3000 (the tunnel is served by
`dev-web-prod` from the main checkout; the dev API runs there too); never
`pkill -f`; kill only exact PIDs you started; never commit on `main`, push,
merge or rewrite history; never claim a check passed unless you saw it.
Report the changed-file list reconciled against `git status`.

## Coordinator

Owns this brief, the spec, PROJECT_STATE, the D-053 entry (recorded), the
amendment pointers in 002/003/005 (recorded at approval), the once-only
final-tree gates, the file-list audit against `git status` per round, the
reviewer and tester runs (two rounds maximum, D-050), and the commit and
merge gates with the user.

## Checkpoints requiring the coordinator

- Any deviation from spec §§2–5 contracts (stop and report; AGENTS §11).
- Step 3 complete: routes live, before Web work starts.
- Any need to touch a file outside the lane's scope, to add an index, or to
  change an existing statement's text.
