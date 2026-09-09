# Slice 015 — Notes

**Status: APPROVED by the user on 2026-09-09 ("yes, proceed") after
independent review and D-053.** Approval covers the declared contracts (§7)
and the §1 safe defaults; it authorizes implementation after the Phase 6
gate (passed 2026-09-09), not commit, merge, push or deployment.
Implementation per the [implementation brief](../tasks/SLICE_015_IMPL.md).
Prepared
against local `main` at `ce15b7d` (the LATER batch merged, pushed and
serving on the shared development runtime). Independent review on
2026-09-08 returned READY WITH CORRECTIONS and the adversarial test
analysis added fourteen items; every correction and every adopted item is
applied below (the load-bearing ones: the Operator's history view excludes
note entries, `note_changed` invalidates only the Person detail, and the
note lookup binds `person_id`).

Free-text **notes** on a Person: written by any member from the Person page,
shown in the Person timeline at the moment they were written, editable and
deletable by their author or an Organization admin, visible to the AI
Operator as untrusted text. Notes are the first of the two remaining CRM-core
models (thesis §11: notes, tasks) and the destination model the parked FUB
migration's notes rung needs (010d fetched notes and preserved them because
no destination existed).

Authority: [D-004, D-005, D-007, D-015, D-021, D-023, D-027, D-029, D-045,
D-050, D-051, D-053](../decisions/DECISION_LOG.md) (D-053 is the accepted
decision that lets note bodies ship plaintext; O-012 amended accordingly),
the [architecture baseline](../architecture/ARCHITECTURE_BASELINE.md),
AGENTS §4.6 (notes are relational CRUD, not history), [002](SLICE_002.md)
§§2/5/6, [003](SLICE_003.md) §6, [005](SLICE_005.md) §5,
[011e](SLICE_011e.md) §§2/3/5 (the new-model pattern this slice copies),
[014](SLICE_014.md) §3 (optimistic mutation posture), the
[LATER batch](../tasks/LATER_BATCH_2026-09-08.md) (mutation keys and
`isMutating` guards) and [UI_STYLE](../design/UI_STYLE.md).

## 1. Scope and product behavior

In scope, one rung:

- a `note` table and `NoteId`;
- typed commands `AddNote`, `EditNote`, `DeleteNote`;
- three additive routes nested under `/api/people/{person_id}`;
- a `note` history kind in `GET /api/people/{id}` `history[]`, positioned by
  creation time, carrying the current body;
- a `note_changed` Person change on the existing realtime event;
- Operator: `PersonDetail.notes` (latest five, untrusted text); the tool
  view's `history` excludes the `note` kind;
- Web: a composer at the top of the Person page's History card, note rows in
  the timeline with inline edit and delete where permitted.

Out of scope (LATER unless stated): note revision history (edit is in place
with an "edited" marker); a "note removed" timeline row (tombstones are
invisible); a `last_note_at` derived column, any Today rule, and any
`has_note`/`note_within` filter clause (D-052 makes the column a small
additive follow-up if ever wanted; a note is not a contact, D-022);
pagination of a Person's history (the detail read is unpaginated across
seven kinds today; a 500-note Person is ~1–5 MB, inside D-050; the FUB
import measurement is the trigger); the Operator `add_note` tool (D-034
says a second mutation tool reopens the `crm-operator → crm-app` edge
question, and `operator_proposal` is PII-free by construction, so it is its
own S rung with its own decision); the FUB notes importer (this slice makes
the schema import-ready, §2, and writes no importer code); markdown, rich
text, attachments or @mentions; notes on People rows, the People preview or
the Operator's search cards; a client-minted `NoteId` making add
idempotent across a lost response (additive: `ON CONFLICT (id) DO
NOTHING`; the accepted duplicate risk is stated in §6); mobile.

Product rules (rule 1 follows D-053 §4; all are safe defaults adopted at
specification, veto-able by the user):

1. **Any active member adds a note. A note is edited or deleted by its
   author, or by an Organization admin.** Other members can neither. The
   author is `author_user_id` on the row. Imported notes with no matched
   author (§2) are admin-only. Platform admins have no tenant access and
   cannot. A deactivated author's notes remain visible and attributed
   (D-027 §2); the admin path is the correction path, so O-004 stays open.
2. **Notes are plain text**, rendered `white-space: pre-wrap`. The body is
   trimmed of leading and trailing whitespace, 1–10,000 characters after
   trimming, `\r\n` normalised to `\n` before validation, and rejected if it
   still contains any control character other than `\n` and `\t`
   (`inquiry.message` is 4 KiB; FUB notes run longer). Rust's `trim` strips
   Unicode whitespace; the §2 CHECK trims only ASCII whitespace and is
   deliberately narrower (safe in the write direction). Non-`Cc` characters
   such as zero-width or bidi marks are storable, the inquiry-message
   posture.
3. **Delete is a tombstone, not a hard delete.** The body is set to the
   empty string and `deleted_at`/`deleted_by_user_id` are stamped. Two
   reasons: the future import's idempotency key must survive so a re-run
   cannot resurrect a note a member removed, and no body lingers in the row
   (O-012, O-013). Tombstones are invisible to every read; a second delete
   or any write to a tombstone is 404, identical to a nonexistent id.
4. **Edit is in place.** The body changes, `updated_at` moves, the timeline
   position stays at `created_at`, and `edited` (`updated_at > created_at`)
   is shown. Editing to the byte-identical body is `changed: false` and
   writes nothing. Last-write-wins; `expected_revision` can be added
   additively if it is ever missed (the tags precedent).
5. **A note is not a contact.** No D-052 column moves, nothing changes on
   Today, `person.updated_at` is not bumped, and no fact row is written
   (AGENTS §4.6).
6. **Notes appear on the Person detail read, not on People rows.**
   `PersonSummary` is untouched (011e rule 6).
7. **The body is emitted at exactly three sites** (D-053 §§2/4): the
   `note` history projection, the mutation receipt, and the Operator's
   `get_person` view (clipped to 500 characters by `UntrustedText`). It
   never travels on the realtime channel, into the Operator ledger, or into
   spans, logs or error envelopes.

## 2. Persistence (one additive migration, lane-owned)

`crm-api/migrations/20260912000001_note.sql` (the 20260911 stamp is taken
by the LATER batch):

```sql
CREATE TABLE note (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    person_id UUID NOT NULL,
    -- NULL only for an imported note whose FUB author matched no member.
    author_user_id UUID REFERENCES app_user (id),
    body TEXT NOT NULL,
    -- Origin::as_str: 'web_session' | 'operator' | 'migration' | ...
    origin TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    -- Import provenance (Slice 010 notes rung); NULL for notes written here.
    source TEXT,
    source_external_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    deleted_by_user_id UUID REFERENCES app_user (id),
    CHECK (author_user_id IS NOT NULL OR origin = 'migration'),
    CHECK ((source IS NULL) = (source_external_id IS NULL)),
    CHECK (
        (deleted_at IS NULL
            AND char_length(body) BETWEEN 1 AND 10000
            AND body = btrim(body, E' \t\r\n'))
        OR (deleted_at IS NOT NULL AND body = '')
    ),
    CHECK ((deleted_at IS NULL) = (deleted_by_user_id IS NULL)),
    CHECK (updated_at >= created_at),
    -- Composite-FK anchor (stage / person / tag convention).
    UNIQUE (id, organization_id),
    -- A note for a Person of another Organization can never be persisted,
    -- even if an application check regresses. Erasure is the Person cascade.
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, author_user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, deleted_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
-- The detail read: one Person's live notes in creation order.
CREATE INDEX note_org_person_created_idx
    ON note (organization_id, person_id, created_at, id);
-- Import idempotency: a re-run of the notes rung inserts zero rows, and a
-- tombstoned imported note is never resurrected.
CREATE UNIQUE INDEX note_org_source_external_idx
    ON note (organization_id, source, source_external_id)
    WHERE source_external_id IS NOT NULL;

-- No DELETE: tombstones are UPDATEs; erasure is the Person cascade, which
-- runs as the table owner and needs no grant.
GRANT SELECT, INSERT, UPDATE ON note TO crm_app;
```

- `origin` and `correlation_id` live on the row so the timeline can fill
  `HistoryEntry.origin`/`correlation_id` without a fact row, and so a later
  Operator-originated note chains to its turn the way every command does
  (`CommandContext::for_operator`).
- **Import readiness, schema only:** a future `ImportNote` domain function
  (the Slice 010 notes rung; not written here) supplies `created_at` and
  `updated_at` explicitly (the 010c backdated-Inquiry pattern; `AddNote`'s
  API is not widened), `origin = 'migration'`, `correlation_id` = the run
  id, `author_user_id` = the member matched by email else NULL, `source =
  'fub'` and `source_external_id` = the FUB note id. Stable Person-order
  insertion is `(created_at, id)`; FUB second-precision ties break on id.
  An unmatched author's display name, and whether imported notes need a
  separate `imported_at` (the 010c backdated pattern keeps `recorded_at` =
  import time; notes are CRUD, so nothing binds), are additive nullable
  columns decided at that time, not now.
- `db_schema.rs` table, grant and index enumerations gain `note` (the
  CHECKs are pinned in `db_notes.rs`, §9.1). The schema test pins that
  `crm_app` holds no `DELETE` on `note`.
- `NoteId` newtype in `ids.rs` (the `TagId` pattern; canonical-uuid
  discipline on the wire).
- The D-015 §7 erasure runbook (deferred, O-013) gains `note`; SLICE_002 §2's
  erasable CRUD set gains it by amendment pointer.

## 3. Typed commands (new `crm-app/src/domain/note/`)

Module layout follows `domain/tag/`: `commands.rs`, `error.rs`, `model.rs`,
`queries.rs`, `mod.rs`. All commands run inside one transaction, take
`CommandContext`, use the Organization from the session only, call
`lock_person(person_id, scope.organization_id())` first exactly as the tag
commands do (absent in the Organization → `NotFound`), and follow the 011b
error conventions (DB failure → `unavailable`, never a fake 404/422).
The command structs carrying a body have **no `Debug` derive** (or a
redacting one), so no `?cmd` can ever put a body in a span or log.

| Command | Caller | Semantics |
|---|---|---|
| `AddNote { person_id, body }` | any active member | Normalise and validate the body (rule 2; violation → `MalformedRequest`); `lock_person`; insert with `author_user_id = actor`, `origin` and `correlation_id` from the context; publish `person.changed{note_changed}` after commit; return the `Note`. |
| `EditNote { person_id, note_id, body }` | author or admin (rule 1) | Validate; `lock_person`; load the note `WHERE id = $1 AND organization_id = $2 AND person_id = $3 AND deleted_at IS NULL` **`FOR UPDATE`** (→ `NotFound`); `FOR SHARE` re-read of the actor's own membership requiring `status = 'active'` (the tag `lock_current_membership` function, lifted to a shared place or duplicated, lane's choice); **permission:** `Role::Admin`, or `author_user_id = actor` → else `Forbidden`; body byte-equal to the stored body → `changed: false`, nothing written; else update `body`, `updated_at = now()`; publish only when changed; return `(Note, changed)`. |
| `DeleteNote { person_id, note_id }` | author or admin (rule 1) | Same lookups, lock, membership re-read and permission check; `UPDATE … SET body = '', deleted_at = now(), deleted_by_user_id = actor` (`updated_at` unchanged; every other column, including the import provenance, byte-identical); publish; return `deleted: true`. Not found (including an already-tombstoned id) → `NotFound`. |

Validation lives in `model.rs` as a pure function (`NoteBody::parse`)
so it is unit-tested without a database: `\r\n` → `\n`, trim, 1–10,000
chars, control characters other than `\n`/`\t` rejected. The stored body is
the normalised one; the CHECK in §2 is the belt.

Reads (`domain/note/queries.rs`): `history_for_person`'s new
`note_history(conn, org, person)` (§5) and `latest_for_person(conn, org,
person, limit)` for the Operator view (§5), both `WHERE deleted_at IS
NULL`, both ordered `(created_at, id)`; the Operator read takes the last
five of that order. The Organization boundary is the literal predicate in
every statement, never a parameter the client supplies.

`NoteError`: `NotFound`, `Forbidden`, `MalformedRequest`, `Database`,
`Corrupt` (an unparseable stored role, the tag precedent). All map to
existing `ApiError` variants (`Corrupt | Database` → 503 `unavailable`);
no new error code. The note lookup binds all three of `id`,
`organization_id` and `person_id`: a note reached through another Person's
path in the same Organization is 404 (§9.4).

## 4. Nothing changes in the filter vocabulary

Stated so the reviewer need not look: no clause kind, no
`PersonFilterParams` field, none of the fourteen filter statements, no
`.sqlx` metadata for existing statements, no Today evaluation, no D-052
trigger. `list_summaries`, `filtered_summaries` and every sort copy are
byte-identical before and after this slice.

## 5. HTTP, realtime, Operator and Web

### Routes (additive; SLICE_002 §5 gains rows by amendment pointer)

All routes require an authenticated session with an active Organization
(`AuthContext`); platform-only sessions are 401 as on every tenant route.
Edit and delete are member routes whose **command** decides permission
under rule 1; there is no route-level admin extractor. Path ids follow the
existing typed path extractors: `PersonIdPath` on POST, and a new
`PersonNoteIdsPath(PersonId, NoteId)` pair extractor (the
`PersonTagIdsPath` pattern) on PUT and DELETE. POST and PUT carry the house
128 KiB `DefaultBodyLimit` **per route** (the tags-router pattern; it is
not inherited); an oversize or malformed body is 400 `malformed_request`,
never 413 or 500. **Error precedence** (011b §5 / 011e §5 house
order): malformed path uuid 400 before authentication → 401 → 400 body →
404 → 403 (the note must exist and be visible before permission can be
judged; note ids are visible to every member through the detail read, so
404-before-403 leaks nothing) → 503.

`Note` in responses is:

```json
{"id","person_id","body","author": {"id","display_name"} | null,
 "created_at","updated_at","edited": bool,"can_manage": bool}
```

`can_manage` is the server's rule-1 verdict for the **viewer** at read time
(admin, or author). It is a display hint; the command re-decides under the
row lock.

| Route | Auth | Success | Errors |
|---|---|---|---|
| `POST /api/people/{person_id}/notes` `{"body"}` | member | 201 `{"note": Note}` (`can_manage` true for the author) | 400 `malformed_request`, 404 `not_found`, 503 |
| `PUT /api/people/{person_id}/notes/{note_id}` `{"body"}` | member; rule 1 in the command | 200 `{"note": Note, "changed": bool}` | 400, 403 `forbidden`, 404 (person, note, tombstone, or another Organization's ids, identical bodies), 503 |
| `DELETE /api/people/{person_id}/notes/{note_id}` | member; rule 1 in the command | 200 `{"deleted": true}` | 403, 404 (also on a repeat), 503 |

No `GET /api/people/{id}/notes`: the detail read's `history[]` is the read
surface (one fewer contract; pagination is LATER with its trigger stated in
§1). PUT, not PATCH (the API has no PATCH; CORS already allows PUT).

### The `note` history kind (SLICE_002 §5 pointer)

`history_for_person` gains an eighth source, `note_history`, merged and
sorted with the others (`occurred_at, recorded_at, kind_rank, id`):

| Field | Value |
|---|---|
| `kind` | `"note"`, `kind_rank` 7 (after `correspondence`, 6) |
| `id` | the note id |
| `occurred_at`, `recorded_at` | both `created_at` (an edit does not move the entry) |
| `actor` | the author's `UserRef`, or `null` for an unmatched imported author |
| `origin`, `correlation_id` | from the row |
| `detail` | `{"body","updated_at","edited","can_manage"}` |

The domain query emits `can_manage: false`; the people detail route, which
knows the viewer's role and id, overwrites it for entries of kind `note`
from `AuthContext.role` and `actor.id` (admin, or `actor.id` equal to the
viewer; `actor: null` → admins only), the tags-route pattern. The Operator
backend ignores the field. Tombstones are excluded by the query. The Web
`HistoryEntry` union gains the `note` arm, and, because a tab loaded before
a deploy refetches the detail on the first `person.changed` it receives,
the Web timeline renders **any unrecognised `kind`** as a generic
"Activity" row with a fallback icon instead of an undefined component
(today `historySummary` has no default arm); this makes every future
additive kind safe.

### Realtime (SLICE_003 §6 pointer)

`PersonChange` gains `NoteChanged` (wire `note_changed`), published on the
existing `person.changed` event after commit on add, on a changing edit and
on delete; not on `changed: false`. The Web handler's `invalidationsFor`
gains a branch: `note_changed` invalidates **only** `queryKeys.person`
(rule 5: a note changes no People row, Today queue or list count, so the
wide default would make every connected tab refetch three query families
per note). Older bundles fall through to the wide default, which is safe.
The closed `PersonChange` token union gains `'note_changed'`. Ids only; no
body on the channel (D-023, rule 7), pinned by a parsed-payload test.

### Operator (SLICE_005 §5 pointer)

- `PersonDetail` gains `notes: Vec<NoteView>` where `NoteView {
  author_display_name: Option<String>, created_at: DateTime<Utc>, body:
  UntrustedText }`, the latest five live notes in creation order (the
  `MAX_INQUIRIES` precedent), populated in the `crm-api` tool backend from
  `latest_for_person`. Bodies are user-authored text and carry the untrusted
  marker like inquiry messages, list names and tag names. This sends
  agent-authored text about a client to the model provider; it is the
  exposure class `inquiries[].message` already has, and D-053 §4 records it.
- **The Operator's `history` excludes `note` entries.** `notes` already
  represents them, and the tool view keeps only the last `MAX_HISTORY` (20)
  entries of the merged sort, so twenty recent notes would otherwise push
  every stage, assignment and call fact out of the model's view. The
  backend filters `kind == "note"` before the truncation; `history_detail`
  gains no arm and a body never reaches the model unwrapped.
- `NoteView.body` is subject to `UntrustedText`'s 500-character clip and
  whitespace flattening (`\n`/`\t` → space), the `inquiries[].message`
  precedent; the Operator can quote the first 500 characters of a note. The
  system prompt's untrusted-text parenthetical (`crm-operator/prompts/
  system.md`) gains "notes". `NoteView` derives `Serialize` and
  `Deserialize` like every view type.
- The Operator ledger (D-029) records tool name, outcome, duration and
  Person ids as today; no note text. No new tools; the crate fences pass
  unchanged.

### Web (Person page; UI_STYLE and D-045 bind)

- **Composer:** at the top of the History card, a labelled textarea
  ("Add a note") with a black primary **Add note** button, disabled while
  empty, whitespace-only or pending; Ctrl/Cmd+Enter submits; a live
  character counter appears past 9,000 characters and the button disables
  past 10,000; the counter and the disable rule count **code points**
  (`Array.from(body).length`), matching the server's `char_length`, not
  UTF-16 units. Success clears the composer and refetches the detail; a
  failed add (network error, 400, 503) keeps the draft and re-enables the
  button, with the code's message from `lib/errors.ts` inline
  (note-specific copy for `malformed_request`: "Notes are 1–10,000
  characters of plain text"); a 404 (the Person vanished) uses the page's
  existing not-found handling.
- **Timeline rows:** the `note` kind gets its own icon in `HISTORY_ICON`,
  renders the body `pre-wrap` under the standard summary line ("Note by
  <author>", "· edited" when `edited`), and, only where `detail.can_manage`
  is true, ghost **Edit** and **Delete** buttons with 40px targets and
  accessible names ("Edit note", "Delete note"). Edit swaps the body for a
  textarea with Save/Cancel (Escape cancels and returns focus to Edit);
  the editor is local state keyed by note id, so a detail refetch that
  removes the note (deleted elsewhere while the viewer types; no mutation
  is in flight, so the realtime hold does not apply) keeps the editor
  mounted with the draft and shows "This note was deleted by someone else"
  instead of vanishing. Delete opens the existing `ConfirmDialog` ("Delete
  this note? This cannot be undone.") whose confirm button disables while
  pending. A 403 (the viewer's role changed) or 404 (the note was deleted
  elsewhere) on Save or Delete refetches the detail and explains inline.
  `HistoryRow` gains an optional note payload (`id`, `body`, `edited`,
  `canManage`); the call fold is untouched.
- **Mutations are pessimistic** (no optimistic write: a note body is the
  kind of value 014 §3 declined to invent client-side) but **keyed** with
  `personMutationKey` and settled through `settlePersonMutation` with
  `queryKeys.person(orgId, personId)` (not `queryKeys.org`: rules 5–6), so
  the LATER-batch `isMutating` guards and the realtime hold apply unchanged.
  Three mutations join `queries.ts`: `useAddNoteMutation`,
  `useEditNoteMutation`, `useDeleteNoteMutation`.
- `PersonPreview.vue` keeps notes out of the preview: its activity fold
  narrows the widened `HistoryEntry` union by excluding the `note` kind (a
  compile touch recorded at implementation, no behaviour change for the
  other kinds). `OperatorPanel.vue`, the People page and Today are
  unchanged.
- **Types:** `Note`, `NoteDetail` (the history detail), the `HistoryEntry`
  `note` arm, `PersonChange` `'note_changed'`.

## 6. Authorization, tenant isolation, failure and observability

- **Authorization:** every note route needs an active membership
  (`AuthContext`). Edit/delete are decided inside the command under the
  note row's `FOR UPDATE` lock and a `FOR SHARE` membership re-read: admin,
  or author (rule 1). An admin demoted concurrently, or an author whose
  membership was deactivated inside the transaction, gets 403 and writes
  nothing.
- **Isolation:** every statement carries the literal Organization predicate;
  `lock_person` runs under the Organization visibility scope; the composite
  FKs make a cross-Organization note, or a note by a non-member, or a
  tombstone by a non-member, unpersistable. A foreign or nonexistent Person
  or note on any path is 404 `not_found`, byte-identical to nonexistent
  (SLICE_002 §6 posture). Organization B's detail read never lists
  Organization A's notes.
- **PII (D-053):** `note` is in the erasable CRUD set; the body appears in
  exactly the three emission sites of rule 7; spans record ids and sizes,
  never text; the `MalformedRequest` envelope never echoes input; the
  tombstone empties the body; the Person cascade erases it. Verified at
  review: the request trace layer logs method, URI and latency only; the
  JSON rejection message is discarded; the ledger holds no text; the Groq
  provider logs status only. The remaining exposure is the model provider
  itself, which D-053 §4 accepts.
- **Failure:** database unavailability is 503 `unavailable` on every route.
  A concurrent delete between a client's detail fetch and its PUT is a 404
  the client resolves by refetching.
- **Idempotency:** add is not idempotent. The composer's pending state
  prevents a same-tab double-submit (the mutation key serialises settle
  logic, it does not deduplicate requests), but a POST that commits while
  its response is lost over the tunnel produces a duplicate if the member
  resubmits (the realtime refetch shows the first note before they do,
  which is the practical mitigation; the additive fix is the client-minted
  `NoteId` in §1's LATER list). Import idempotency is the §2 partial unique
  index; edit-to-same is `changed: false`; a repeated delete is 404.
- **Observability:** spans `note.add` (`person_id`, `note_id`,
  `body_chars`), `note.edit` (`note_id`, `outcome` changed|unchanged),
  `note.delete` (`note_id`), all declared with `skip_all` so `instrument`'s
  default capture can never record an argument. **Note bodies are never
  logged** (AGENTS §9, D-053 §2), pinned by the §9.9 capture test.

## 7. Contract declaration and amendment ownership

Approval of this specification satisfies AGENTS §11 for:

| Previous → proposed | Reason / affected | Compatibility and required amendment |
|---|---|---|
| No note model → §2 table, §3 commands | Thesis §11 CRM core; FUB notes destination | Additive migration; no existing table changes. SLICE_002 §2 erasable-set pointer. |
| Three new routes (§5) | Person page notes | Additive; same error envelope; no new error codes; rule-1 permission decided in the command. SLICE_002 §5 rows. |
| `history[]` seven kinds → eight (`note`) | Timeline | Additive kind in a closed union; the Web adds the arm; the Operator's tool view filters the kind out of `history` (§5) and `history_detail` gains no arm. SLICE_002 §5 pointer. |
| `PersonChange` six variants → seven (`note_changed`) | Invalidate on note changes | Additive; unknown tokens already tolerated by the Web handler. SLICE_003 §6 pointer. |
| Operator `PersonDetail` → `+ notes`; its `history` excludes the `note` kind | Operator sees what the page shows without crowding out facts | Additive view field, untrusted text; the history filter is a backend projection choice. SLICE_005 §5 pointer. |
| O-012 "blocks free-text notes" → amended | D-053 | Recorded in the decision log on 2026-09-08. |

The coordinator owns the amendment pointers, PROJECT_STATE, this
specification and the brief. No Person ownership, visibility scope, history
fact, D-042 capture behaviour, filter vocabulary or Today change.

## 8. Performance (D-050)

No hot statement changes. `note_history` is one index range scan per detail
read on `note_org_person_created_idx`; the Operator read is the same scan
with a limit. Within the envelope (25,000 People, tens of notes per Person
written by hand; hundreds only after the FUB import) no gate applies beyond
the tests. Absolute latency is reported, never gated. The history-pagination
trigger is stated in §1.

## 9. Acceptance criteria and verification

1. **Schema:** migration applies on a fresh database and one with existing
   Organizations; a cross-Organization note, a non-member author and a
   non-member `deleted_by_user_id` are rejected by the composite FKs; every
   CHECK in §2 holds (empty live body, 10,001 characters, untrimmed body,
   tombstone with a body, tombstone without `deleted_by`, `source` without
   `source_external_id`, NULL author with `origin <> 'migration'`); the
   partial unique index rejects a duplicate `(org, source, external_id)`
   and allows the same external id in another Organization; grants as §2
   and `crm_app` has no `DELETE`; a Person row deletion cascades its notes;
   a body of exactly 10,000 four-byte code points is accepted (pins
   `char_length` against bytes; ~40 KB raw). (db, `db_schema.rs` and
   `db_notes.rs`)
2. **Body validation:** `\r\n` normalised; trimmed; empty, whitespace-only,
   10,001 chars and a `\u{0}`/`\u{1b}` body → 400; `\n` and `\t` kept;
   exactly 10,000 chars accepted. (unit)
3. **Add:** 201 with the `Note` shape, `can_manage` true, `edited` false;
   the detail read shows a `note` entry at `created_at` with the body,
   `actor` = author, `origin = 'web_session'`, the session's
   `correlation_id`, rank 7 (an entry created in the same instant as a
   `correspondence` row sorts after it, both inserted with explicit equal
   `occurred_at` and `recorded_at` on the owner connection so the
   `kind_rank` tie-break is actually reached); foreign or nonexistent
   Person → 404 with no row; a 200 KB body → 400 `malformed_request`, not
   413; `person.updated_at`, the four D-052 columns and Today unchanged;
   `GET /api/people` rows byte-identical. (db)
4. **Edit:** author 200 `changed:true`, body and `updated_at` updated,
   `edited` true, timeline position unchanged; admin 200 on another
   member's note; a third member 403 and no write; same body
   `changed:false` and no write; author deactivated inside the transaction
   403 and no write; admin demoted to member, and separately deactivated,
   inside the transaction (the `db_tags.rs` out-of-band UPDATE pattern) 403,
   no write, no publication; tombstone 404; another Organization's note id
   404 byte-identical to a random uuid; **the same Organization's note
   reached through another Person's path** (`PUT /people/{P2}/notes/{note
   of P1}`) 404, row unchanged, no publication; validation 400 before 404.
   (db)
5. **Delete:** author and admin 200; third member 403; the row holds body
   `''`, `deleted_at` and `deleted_by_user_id` and every other column
   (`source`, `source_external_id`, `created_at`, `updated_at`, `origin`,
   `correlation_id`, `author_user_id`) byte-identical to before; the detail
   read and the Operator read no longer list it; a repeat delete, an edit
   of the tombstone, and a delete through another Person's path 404;
   another Organization's Persons and notes untouched; an author edit
   racing an admin delete (`tokio::join!`, the `db_tags.rs` race pattern)
   ends {200, 200} or {404, 200}, never 503, with a tombstone as the final
   row. (db)
6. **Imported shape:** a row inserted directly with `origin = 'migration'`,
   NULL author, `source = 'fub'` and an external id renders with
   `actor: null` and `can_manage` true only for an admin; a member's edit
   of it is 403; a tombstoned imported note blocks a re-insert of the same
   external id (unique violation), pinning the resurrection guard. (db)
7. **Reads and precedence:** the §5 error precedence holds on each route
   (malformed uuid 400 → 401 → 400 body → 404 → 403); the three routes join
   the platform-only-session 401 enumeration in `db_admin.rs` with explicit
   PUT and DELETE calls (the existing loop is GET-only); `can_manage` in
   the detail read is true for the author and for an admin, false for
   another member; a deactivated author's note still renders with `actor`
   set (D-027 §2). (db)
8. **Realtime:** `person.changed{note_changed}` published exactly once per
   add, changing edit and delete, and not on `changed:false`; the parsed
   payload carries no `body` key; the Web handler invalidates only the
   person detail for `note_changed` (existing test extended). (db + Vitest)
9. **Operator and capture:** six notes with unique sentinels S1–S6 (S1
   oldest): the serialised `PersonDetail` contains S2–S6 exactly once each
   (in `notes`, as untrusted text, with the author's display name,
   tombstones excluded) and S1 zero times; `history` contains no `note`
   entry and no sentinel, and 25 notes plus one stage change still leave
   the stage change in `history`; a 600-character note arrives as its
   500-character clip; the `operator_tool_call` row contains no sentinel.
   **Capture test** (the `CaptureWriter` harness from
   `db_today_source_telemetry.rs` at TRACE with `FmtSpan::FULL`): an add
   with a sentinel body, an edit to a second sentinel, a delete, a rejected
   `\u{0}` body, a 403 and a 404, then the Operator tool call; neither
   sentinel appears in the captured output or in any response body other
   than the 201/200 receipts and the detail read. A foreign Person is still
   refused; crate fences pass. (db + operator tests)
10. **Web:** composer disabled states, Ctrl/Cmd+Enter, counter past 9,000
    and disable past 10,000, success clears and refetches, 400 inline;
    note rows render `pre-wrap` with the summary line and the edited
    marker; Edit and Delete only where `can_manage`; inline edit
    Save/Cancel/Escape with focus return; an inline edit survives a detail
    refetch that removes the note, keeping the draft; delete confirm with
    the button disabled while pending; a failed add keeps the draft; the
    counter counts code points (a 10,000-code-point astral body enables
    Add); 403 and 404 refetch paths; all three mutations keyed with
    `personMutationKey` and settled through `settlePersonMutation` on the
    person key; `note_changed` invalidates only the person detail; an
    unknown history `kind` renders a generic row and does not throw.
    (Vitest)
11. **Walkthrough (live, dev runtime):** alice writes a note on a Person,
    edits a typo, sees "edited"; bob sees the note after refetch and has no
    Edit/Delete; the admin deletes it and it disappears for both; a second
    Organization's identical Person shows nothing throughout; the Operator,
    asked about the Person, can quote a short note.
12. **Final gates:** on the final tree the coordinator runs
    `./scripts/sqlx-prepare`, `./scripts/check` and `./scripts/check-db`
    once; the lane runs them per round. Independent review by the reviewer
    and adversarial analysis by the tester, read-only, at most two rounds
    (D-050).

## 10. Delivery

One rung, **one lane, one writer, one short-lived branch from `main`**,
backend first, then Web against real routes (the 011e-e1 precedent: the Web
half is small and the wire contract is short). The lane is the sole owner of
the migration and `.sqlx`. Size **S–M, about 0.8× 011e-e1**: three routes
instead of six, no Manage page, one history kind and one realtime variant,
plus an inline edit UI. No shared filter statement is touched, so `.sqlx`
churn is limited to the new statements.

Expected files: `crm-api/migrations/20260912000001_note.sql`;
`crm-app/src/domain/note/` (new) and `domain/mod.rs`; `crm-app/src/ids.rs`;
`crm-app/src/domain/person/queries.rs` (`note_history`,
`history_for_person`); `crm-app/src/realtime/events.rs`;
`crm-api/src/routes/notes.rs` (new), `routes/mod.rs`, `lib.rs` (router
merge), `routes/people.rs` (`can_manage` on note entries);
`crm-operator/src/views.rs`; `crm-operator/prompts/system.md`;
`crm-api/src/operator/backend.rs`;
`crm-api/tests/db_notes.rs` (new, registered alphabetically in
`tests/all.rs`), `tests/db_schema.rs`, `tests/db_admin.rs`,
`tests/db_operator.rs`, `tests/db_realtime.rs`; `web/src/lib/errors.ts`; `web/src/api/types.ts`,
`web/src/api/queries.ts`, `web/src/realtime/events.ts`,
`web/src/views/PersonDetailView.vue` and its test.

This specification authorizes nothing until the user approves it after
independent review; approval will authorize implementation and tests, not
commit, merge, push or deployment.
