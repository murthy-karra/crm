# Mobile 002 — Edit notes and tasks offline

**APPROVED FOR IMPLEMENTATION — D-076, 2026-09-12.** The user requested preparation
and review of this next mobile specification, with iOS and Android development
alongside migration 010e2. Only physical-phone testing and the broader mobile
design/system-information cleanup are deferred. The user accepted the reviewed
contracts and implementation scope. See the [execution briefs](../tasks/MOBILE_002_IMPL.md),
[parallel plan](../plans/MOBILE_002_010e2_PARALLEL_LAUNCH.md) and
[review](../tasks/MOBILE_002_REVIEW.md).

Authority: [AGENTS](../../AGENTS.md), D-053/054/057/073/074/075 in the
[decision log](../decisions/DECISION_LOG.md), [Mobile 001](MOBILE_001_OFFLINE_FIELD_WORK.md)
and its [frozen contract](../tasks/MOBILE_001_CONTRACT.md),
[notes](SLICE_015.md) and [tasks](SLICE_016.md). Approval would own the declared
HTTP, persistence, command and local edit/conflict changes, plus isolated
synthetic backend/iOS/Android implementation. Release and real customer work
remain separate. The seven-day lease, server authorization and review hold stand.

## 1. User outcome and bounded scope

An agent can edit a downloaded note or task without connectivity, see that the
edit is saved on the device, and synchronize it without overwriting an intervening
change. Conflicts preserve the agent's proposal and show the authorized current
record for a deliberate decision. iOS and Android deliver equivalent behavior.

| Record | Editable here | Preserved |
|---|---|---|
| Note | Existing plain-text body | Person, author, original creation/history position, source attribution |
| Task, open or completed | Title, kind, due date/time including explicit removal of the due date | Person, creator, assignee, completion/reopen state, source attribution |

Use current native composition patterns; required edit/conflict controls belong
to this feature. A broad visual redesign and diagnostic-information cleanup do
not. New People, reassignment/member pickers, delete/reopen/snooze commands,
recurrence, attachments, messaging, calls, reminders/push and a generic history
or conflict-resolution platform are outside this rung. Existing Mobile 001
add-note/create-task/complete-task behavior remains supported.

Note edit authority stays author or current Org admin. Task edit authority stays
creator, assignee or current Org admin, including editing a completed task's
non-completion fields. `can_manage` is a cached display hint; the server rechecks
current membership and authority at execution. No offline lease grants a server
write or bypasses `migration_review`.

## 2. Product rules proposed for acceptance

1. **Compare the version the agent edited.** Submit the downloaded record's
   revision. A changed server revision conflicts even when the current text
   happens to equal the proposed text. No last-write-wins mobile edit, automatic
   merge or timestamp-based overwrite. An unchanged authorized revision with
   identical normalized fields returns a durable accepted `changed:false` receipt.
2. **One unresolved submitted action per existing record.** An immutable pending
   edit or task completion blocks another submitted action on that same target.
   Further typing is still saved as a separate follow-up draft. Other records
   continue syncing. The follow-up explicitly says **"Saved draft — waiting for
   the previous change"**; it is not claimed queued or synced.
3. **Every saved proposal survives ordinary interruption.** Keep its original
   baseline, proposed fields, target and revision in encrypted SQLite independently
   of replaceable downloaded data. Draft submission and immutable operation/local
   overlay creation commit atomically. Never mutate, coalesce or mint replacement
   identities for an already submitted operation.
4. **Resolve a conflict explicitly.** Keep the original proposal and display
   the current authorized version. The user can keep the draft for later, discard
   that proposal and use the current version, or review/edit a new proposal against
   a freshly fetched version and submit it under a new identity. There is no blind
   "force" option. A second intervening edit can conflict again.
5. **No resurrection or automatic reattachment.** Deleted/missing resources,
   lost visibility or lost authority never cause creation of a replacement note/
   task or transfer to a different Person, actor or Organization. Apply Mobile
   001's protected-work/access rules; no automatic export or copy-to-new-record.

These rules intentionally bound edit sequencing. General operation dependency
chains are later work. The accepted Mobile 001 create-task → complete dependency
continues unchanged; this slice does not invalidate queues containing it.

## 3. Shared contract declaration

| Current contract | Proposed addition and reason | Components and compatibility |
|---|---|---|
| Note has timestamps, no record revision; ordinary EditNote is last-write-wins | Add positive `note.revision` with all-writer change coverage and a revision-checked transaction-compatible edit core | PostgreSQL, typed note command/read seam, mobile pages/receipt and native baseline storage. Ordinary Web/Operator DTOs and unconditional wrapper behavior stay unchanged |
| UpdateTask owns its transaction and requires four fields including an assignee | Factor a shared transaction-compatible update core; mobile edits title/kind/due while preserving the locked current assignee, including null | Typed task commands and mobile adapter. Do not default a null imported assignee to the actor or add a reassignment path; legacy full-replace command keeps its contract |
| `mobile-v1` supports add/create/complete | Add capability-gated `edit_note` and `update_task` kinds with mandatory expected revision, using existing atomic receipts | Additive operation parser/constraints, bootstrap capabilities and native dispatch. Existing normalized canonical bytes, key domain, receipt identities and old responses stay compatible |
| Mobile note pages carry no record revision; add-note receipts have null committed revision | Add note revision to mobile note items; edit-note receipts carry the committed note revision; add-note receipts remain null | Both native parsers and cache qualification. An old cached note with no revision remains readable but is ineligible for edit until refreshed |
| Drafts compose new records, and render/receipt paths distinguish add-note versus task | Add typed edit drafts, immutable baselines, resource sequencing and explicit conflict/follow-up state | SwiftUI/Compose and protected SQLite schema upgrades. Do not route `edit_note` through a catch-all task branch |
| Reconciliation refreshes complete Person bundles | Add bounded authorized single-record reads for editing/conflict review | New mobile read routes; no legacy unbounded Person read, full-cache promotion or expanded body audience |

This spec owns these proposed amendments to Slices 015/016 and Mobile 001.
Freeze exact DTO serialization, SQL constraints/trigger inventory, lock order,
local schema upgrades and shared fixtures before native implementation begins.
Privacy, authority, conflict, dependency or protocol semantics cannot change as
an incidental implementation detail.

## 4. Additive protocol and version semantics

Keep `/api/mobile/v1` and protocol `mobile-v1`. Add bootstrap capabilities
`edit_note`, `update_task`, `note_revisions` as one available feature set when
the compatible backend/schema is installed. Existing required capabilities
remain; older clients must continue to function with additional capability names.
New clients offer edits only after observing that complete capability set under
their current authorized context. Cached capability knowledge never renews the
lease; a revoked/unsupported response still fences the operation.

Every mobile note page adds a positive canonical decimal-string `revision`.
Other existing note fields and page/cursor bounds remain unchanged. A cache
schema upgrade marks old note components without revisions unqualified for
editing and schedules a refetch even when Person revision is unchanged. Preserve
the old readable active bundle while a replacement stages; never assign a made-up
revision or discard old outbox/drafts to force a download. Generation cursors and
original envelopes are not rewritten.

Refetch alone is insufficient when local bundles are keyed by Person/revision.
Atomically replace the old-format bundle with the fully staged revision-bearing
representation even at the same Person revision, without replacing a higher
server revision or falsely completing other components. A local representation
qualification marker must distinguish the two; no fabricated server version.

`note.revision` starts at 1 for existing rows, including tombstones; this denotes
the migration baseline, not historical edit count. A before-update trigger
increments it on any real persisted note change, including body, author,
attribution and deletion changes. An identical no-op does not advance it. The
trigger must not recursively increment itself or permit revision regression.
Inventory ordinary edits/deletes, migration and privileged existing writers.
The existing Person revision continues to cover note changes and joined labels;
it is not a substitute for the note revision. Existing task revision remains the
task concurrency token, including complete/reopen/snooze/assignment changes.

Keep current shared receipt identity `(Org, actor, operation_id)`, context binding,
canonicalization/digest-key versions and consumed-marker retention. New operation
kinds have closed typed payloads under that envelope; do not recompute existing
receipts using new fields. Historical and newly accepted `add_note` receipts keep
`committed_revision:null`; `edit_note` has the accepted note revision. Task receipts
retain their current revision meaning. All receipts remain content-free and
capture `person_revision` atomically; none is a copy of current record contents.

## 5. Edit and current-record HTTP contracts

Use the existing immutable operation envelope, original device time and
`X-Mobile-Context`; no actor/Org fields, arbitrary patches or client permissions.
Both kinds accept a server resource ID from a qualified downloaded record, not
a create-operation reference or a locally guessed future revision.

| Kind | Exact payload |
|---|---|
| `edit_note` | `{person_id, note_id, expected_revision, body}` |
| `update_task` | `{person_id, task_id, expected_revision, title, kind, due_at}`; due_at is required and nullable; no assignee field |

NoteBody and TaskTitle/kind validation retain their existing normalization and
limits. Task dates are instants: preserve an untouched stored instant exactly;
an intentional date-only selection resolves to local end of day using the
chosen date's timezone rules. Persist the resolved instant when the draft is
saved so travel/timezone changes do not shift it on retry. Explicit null clears
the due date. Changing title/kind/due never changes completion state or assignee.

Two new read routes:

| Route | Response |
|---|---|
| `GET /people/{person_id}/notes/{note_id}` | `{context_id, person_id, person_revision, note}`; note is the existing public fields plus decimal revision |
| `GET /people/{person_id}/tasks/{task_id}` | `{context_id, person_id, person_revision, task}`; task is the existing mobile task fields including revision |

These are under `/api/mobile/v1`. Require existing session, matching current
context, active membership, operational workspace and normal Person visibility;
scope the resource to both trusted Org and supplied Person. Read one live record
in a short consistent snapshot, with current `can_manage`; no tombstones, bodies
in errors, foreign identifiers or read-based lease renewal. Responses are at most
128 KiB and `no-store`, including failures. Unknown query/body fields are rejected.
The read can refresh a known target for a saved edit even if it no longer appears
in the current offline selection, while current visibility still permits it.

A single-record read supplies a comparison/edit baseline only. It cannot mark a
Person component complete, advance a reconciliation cursor, authorize removal of
other cached records or override the active sealed bundle. Store it as protected
edit context with the target and local authorization lease, not as another
unbounded offline selection mechanism. Render later current values separately
from pending/accepted proposals.

Preserve existing malformed/auth/workspace/not-found/permission errors. A new
execution checks normal write authority before returning `revision_conflict` and
before the byte-equal/no-op shortcut. Conflict errors contain no record body;
fetch authorized current state separately. Missing or deleted targets return
generic 404. Existing receipt replay precedes new write authority/version checks
and still checks current visibility, as in Mobile 001. Lost current edit authority
does not re-execute an accepted old receipt.

An old server may lack the new capabilities or reject the new kind. New clients
preserve such queued edits and explain that compatible sync is unavailable; they
never downgrade them to an unconditional Web PUT or a new add/create. Missing
capability does not disable unrelated legacy operations or delete a draft.

## 6. Atomic command execution

Within the existing bounded transaction, acquire the operational workspace guard,
context/operation locks, Person then note/task row locks, and current membership
under a consistent lock inventory. Recheck resource existence and permission,
compare the expected record revision, apply the normal typed command, read final
record/Person revisions, persist the immutable receipt, then commit once.
Keep two-second lock waits, five-second statements and the existing total HTTP
deadline. Publication uses existing IDs-only note/task invalidation after commit.

Factor `EditNote` and `UpdateTask` cores, preserving old wrappers' DTOs, validation
precedence, authorization, clocks, no-op behavior and publication. Mobile cannot
call a committing wrapper then write its receipt in a separate transaction. The
preserve-assignee task mode uses the current locked value, even null/inactive,
and never treats omission as a requested reassignment. Revision check is inside
that same authorized transaction. Canonical note edit/history and task Today
behavior stays server-owned; do not stamp device time into business history.

Accepted replay cannot reapply an older edit after another edit/delete/reopen.
Concurrent duplicate operations converge on one receipt. Failures before commit
leave no business change/receipt split. A 404 receipt is not proof an uncertain
operation never executed; keep its original identity and protected input.

This applies to both POST-operation 404 and GET-receipt 404: replay checks current
resource visibility, so an accepted operation followed by deletion can return
not-found. Mark it unavailable/unconfirmable with protected original identity;
do not authorize replacement execution from either response.

## 7. Native draft, sequencing and conflict lifecycle

Persist draft ID/local CAS revision, record type, Person/resource IDs, baseline
record revision and exact baseline editable fields, proposed fields, edit mode,
and any predecessor operation reference. Account/Org/context and configured origin
remain bound by existing protected-store rules. Baseline/proposed bodies live only
in the encrypted store and authorized views, not logs, receipts or immutable
server history. Keep only bounded active edit contexts, not an automatic revision
archive. Preserve them across ordinary app and SQLite schema upgrades.

Opening Edit uses a complete qualified cached record with a record revision and
cached manage permission, or a successful authorized single-record read. Starting
and autosaving a draft does not upload. Save commits the exact latest draft
revision into a new immutable operation and a clearly pending overlay. Failed
local writes keep the previous saved state and show an error; never claim the
new text was committed. Background autosave and submit use CAS to prevent stale
UI writes from replacing newer saved input.

For an existing server resource, at most one submitted unresolved mutation is
eligible for upload. Include task completion in that per-resource check. While
one exists, another editor uses one durable follow-up draft keyed to the same
target/predecessor; Save draft is available, Queue/Complete waits. A submitted
create operation is also immutable: its follow-up draft cannot become an edit
until receipt/resource identity and a real server version are available. Existing
create → complete envelopes continue in their original dependency order; do not
attach a new edit between them.

After predecessor acceptance, commit its receipt/overlay and fetch authorized
current state. A follow-up draft is never silently rebound to that revision or
uploaded automatically: show the submitted proposal, latest server state and
saved follow-up, then require explicit review/save to create the next envelope.
After a definitive conflict/rejection, the user can resolve or abandon that local
proposal before submitting a revised action. An uncertain/in-flight request
cannot be made safe to replace by a local cancel button; recover it using its
original identity. Other resource uploads must not be starved by this target.

The local state machine distinguishes queued/sending/transport-uncertain/backoff
from accepted, definitive `revision_conflict`, unavailable/authority-held and
explicitly superseded/discarded work. Only unresolved transport states retry
automatically; a definitive conflict never retries in the background. Resolution
and any revised enqueue atomically mark the old local attempt non-uploadable.
Persist resource/predecessor relationships and query them directly; equal client
timestamps or iteration order cannot establish sequencing. Freeze concrete state
names and migration mappings in the shared implementation contract.

Before acknowledgment, validate receipt operation ID, kind's expected resource
type, exact edit target ID, accepted outcome/time and positive record/Person
revisions within signed PostgreSQL BIGINT range. Preserve legacy add-note null
revision semantics. A current edit no-op has its expected record revision; a
changed edit advances it. Invalid/mismatched receipts preserve uncertain work,
never acknowledge another record. Apply receipts, target slots and overlays in
one local transaction with identity/context fences.

Conflict presentation has **Your saved edit**, **Current version**, and access
to **Version you started from**, with full bounded note text/task fields. Keep
read-only comparison and editable revised draft distinct. Choosing current version
explicitly discards the selected local proposal/draft; label that consequence.
Preparing a revised edit retains the old proposal as protected superseded input
until explicit discard, and links it to the new operation. Never treat a local
resolution choice as a server acknowledgment. No automatic upload of the old
conflicting operation after resolution.

If current data cannot be fetched, keep the draft and comparison baseline and
explain the waiting state; do not guess a new revision. If authority/visibility
has been lost, follow existing protected lock/removal behavior and hide content
where required. Same-account reauthorization, seven-day expiry, sign-out, reboot,
clock rollback, key failure and corruption preserve Mobile 001's protections.

Gate opening/display/submission at the resource level, not merely Person presence.
An authoritative unavailable/deleted note or task inside a retained Person hides
its baseline/proposal/conflict text while retaining protected input and a
content-free unavailable item. A partial page or selection omission is not a
deletion signal; a successful current-record read can requalify a known target
within this spec's authority. For a write-only 403, recheck current authority:
full actor/Org/workspace denial locks the store, while a still-visible resource
may be shown read-only with `can_manage:false`. Late reads/autosaves are fenced
by identity, target and editor/draft revision so they cannot reopen hidden content
or overwrite a newer draft.

Accepted overlays use the original receipt's Person revision. Never replace
newer active server data with an older edit proposal, and retire an overlay only
when causally covering authorized state has been obtained under the Mobile 001
rules. A current-record comparison is not a seal for the entire cache. Later
reconciliation cannot erase drafts, uncertain envelopes or conflict baselines.

## 8. Verification and delivery

Reuse the Mobile 001 synthetic envelope: two Organizations, two actors, both
native virtual runtimes, 100 selected People, 1,000 notes, 1,000 tasks and a
representative mixed queue up to 100 actions. These are test dimensions, not new
customer quotas/support promises. Required evidence:

1. Note all-writer revisions and task revision checks, no-op/stale-equal cases,
   same-time edit/revert, author/admin and task creator/assignee/admin boundaries,
   cross-Org/Person targets, tombstones, null/inactive assignee preservation and
   edits to completed tasks without changing completion.
2. Actual atomic command/receipt commit, lost successful responses and replay
   after intervening edits/deletion/reopen; payload mismatch, two-device conflicts,
   rollback/timeout and visibility/authority changes. Receipts/logs contain no text.
3. Native offline edit/save/terminate/relaunch, CAS autosave races, storage failure,
   pending edit plus follow-up draft, completion serialization, unaffected queue
   progress, and real-API conflict review/new action on each platform.
4. Upgrade existing encrypted stores with legacy add/create/complete queues,
   legacy null note receipts, drafts and old revision-less cached notes. Retain
   readable cache while forcing qualified note refresh, preserving IDs/bytes and
   approved create/completion dependencies. Unsupported capability keeps work safe.
5. Interrupted current-record and generation reads, record removal/account/Org
   switch, seven-day locking, acknowledgment versus stale seal, follow-up versus
   remote edit, schema-upgrade failure and repeated refreshes. Single-record reads
   cannot falsely qualify complete cache coverage or remove other records.
6. Ordinary Web/Operator note/task behavior, Today ordering and migration import/
   review guards remain compatible; existing permitted note inserts and Person
   revision triggers still work alongside 010e2. No migration/native cross-tenant
   permit or competing note/task writer is introduced.

Run required backend/SQLx/DB and actual iOS/Android build, persistence and native
UI checks on isolated artifacts/runtimes. One D-050 relevant plan/paired-regression
pass and at most two bounded review/fix rounds; do not rerun prior full suites
without a changed area or concrete failure. Physical-phone/cellular tests and
distribution remain explicitly user-deferred, not silently marked passed.

No application implementation or test success is claimed by this proposal.
