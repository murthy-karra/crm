# Mobile 005 — Offline Person names and contact details

**APPROVED FOR IMPLEMENTATION — D-082, 2026-09-14.** The user accepted the reviewed
contracts after independent round 2 returned READY. Proposal wording below records
the accepted scope. [Execution briefs](../tasks/MOBILE_005_IMPL.md),
[paired plan](../plans/MOBILE_005_010f3_PARALLEL_LAUNCH.md) and
[source review](../tasks/MOBILE_005_010f3_PLANNING_REVIEW.md).

Read AGENTS.md, D-015/D-023/D-050/D-073–080 and Mobile 001–004. Keep the existing
seven-day access policy, protected local storage, immutable receipts and server
authority. This slice proposes a shared command; there is currently no ordinary
Person name/contact editor to wrap. Slice 002's normalization and distinct-Person
rules continue to apply. Calling follows the agreed progression.

## 1. Outcome and proposed product rules

An active member edits an already downloaded Person's first/last name and adds,
corrects or removes that Person's phone/email methods on iOS and Android while
offline. Saved proposals survive restart and synchronize once against the exact
downloaded profile revision. A competing profile edit preserves the proposal for
explicit review. A stage, assignment, note, task or contact-attempt change alone
does not conflict with a profile edit.

Use one revision for names and the complete contact collection. This conservative
first slice can conflict when two agents edit different profile fields; do not
silently merge them. A→B→A changes still invalidate the earlier revision. One
unresolved submitted profile operation per Person; subsequent changes stay in a
separate follow-up draft until the first result is known and reviewed.

Contact edits address stable contact UUIDs, never a displayed primary value or
array index. Retain UUID, kind, creation time and import order when correcting a
method. Adds receive server UUIDs, null import order and server creation time,
using the existing `(import_order NULLS LAST, created_at, id)` display ordering;
removal deletes only the explicitly selected UUID. No replace-all array, implicit
deletion, kind conversion, reorder or primary-selection feature. Removing the
currently displayed first method reveals the next under the existing ordering;
show that consequence before local submission. This changes future contact lookup,
not historical call/message destinations or original source evidence.

Proposed input rules: explicitly changed names trim surrounding whitespace,
allow null to clear, and accept at most 200 Unicode scalar values per name; reject
control characters. Changed contact display values trim surrounding whitespace,
are nonempty, at most 1,024 UTF-8 bytes, and reject controls. Reuse the existing
server email/phone normalizers without claiming deliverability or inventing stricter
vendor semantics. Reject duplicate normalized methods of the same kind on this
Person; shared household details on other People remain permitted. No merge,
identity adoption or change to inquiry dedup precedence follows from editing.

Require at least one nonempty name or remaining contact after the proposed edit.
Existing over-limit or unusual imported values remain readable and unchanged
unless explicitly edited. These are new-editor input bounds, not retroactive
database/import quotas. Never truncate existing data or resend untouched values
through new validation. At most 50 explicit contact mutations per submission,
within the existing 128-KiB operation envelope; large changes require later
separately reviewed submissions. The complete baseline must still be downloaded.

No new People, reassignment, tags/custom-field editor, contact consent/verification,
bulk edits, Web editor, Operator tool, messaging, calls, push, distribution or
broad mobile redesign. Ordinary mobile access remains forbidden in migration
review workspaces, including for admins.

## 2. Shared-contract declaration

| Current | Proposed contract and reason | Components / compatibility |
|---|---|---|
| No typed ordinary name/contact edit command; intake/imports own existing writes | Add typed `UpdatePersonDetails` with explicit field/contact operations and mandatory expected revision | Shared Rust command and validators, mobile adapter; future clients must use this command, no new Web/Operator route in this slice |
| Broad `mobile_revision` includes unrelated activity | Add positive `person.details_revision` covering names and contact identity/value/order changes | Additive schema and all-writer triggers; no change to `stage_revision` semantics; existing rows start at 1 |
| Existing summary pages carry contact UUID/kind/value but no ordering metadata or profile concurrency token | Add `details_revision` to summary and `import_order`/`created_at` to contact items; qualify complete summary-section traversal | Mobile generations, fixtures and native cache representation; old clients ignore extra fields and retain UUID paging |
| Receipts enumerate existing note/task/contact-attempt/stage operations | New `update_person_details` kind and `person_details` resource | Explicit backend/native dispatch and constraints; existing kinds, digests and receipt bytes retain meaning |
| `person.changed.data.change` has no details-edit variant | Add `details_changed` in the existing v1 IDs-only envelope, after a changed profile commit | Rust `PersonChange`, mobile publication dispatch, Web event type/invalidation fixtures; no-op and exact replay publish nothing |
| Conflict reads cover notes/tasks/stages | Bounded revision-pinned profile conflict read with contact pagination | New mobile read routes, no-store and context fencing; does not renew offline access or promote a partial Person generation |
| Intake lookup serializes on the Organization intake lock | Profile contact writes acquire that same lock before Person/contact locks | Prevent a contact edit from racing intake's identity decision; existing intake precedence and bounded lock budget remain unchanged |

This spec owns proposed amendments to Slice 002, [Slice 003 §6](SLICE_003.md#6-realtime-contracts)
and Mobile 001–004 only after
acceptance. Freeze DTO/schema/error/lock fixtures before native implementation.
Do not add a generic patch engine, sync service, event store or new dependency.

The proposed Slice 003 pointer amendment adds only `details_changed` to the change
vocabulary; D-023's channel, authorization, envelope and best-effort delivery stay
unchanged. Web invalidates Person detail, People, Today and saved-list counts.
Existing clients' broad `person.changed` fallback remains compatible. Verify the
new typed variant and fallback, without sending names, method IDs/values or state
over realtime. A publication failure after commit must not turn acceptance into
a command failure; reconnect/focus refetch remains recovery.

## 3. Revision, query and write semantics

`details_revision` starts at 1 on upgrade and new Person insertion. A name change
or contact insert/update/delete advances the owning Person revision in the same
transaction. Contact identity, kind, display/normalized value and ordering are in
scope; unrelated Person fields and metadata are not. Derive from the old value;
reject overflow and caller resets. Multiple contact changes may advance more than
once inside one transaction; only the final committed token is observable.
An exact no-op changes neither revision nor business rows. Compare expected
revision before evaluating no-op on a fresh operation.

Inventory intake, `capture::link_unmatched(add_contact_method)`, original import,
original refresh, admission, admitted refresh, fixtures/admin paths and Person
erasure cascades. New triggers must compose with
existing broad mobile invalidation, workspace guards and imported contact
ownership. Do not change an applied migration. Do not require an erased parent
row to survive its cascading contact deletion. All writers lock the parent before
contact mutation; document any writer requiring an owned compatibility adjustment.
No trigger may manufacture business facts or grant a review-workspace bypass.

Keep `mobile-v1`; advertise `update_person_details` and `details_revisions` only
when supported. Save requires both plus a fully sealed, matching-revision Person
summary and complete contacts. Existing old representations remain readable;
their primary email/phone summaries cannot seed edits. Upgrade must allow a fully
fetched same-`mobile_revision` bundle to replace an old representation without
inventing a token or overwriting a newer bundle. Unchanged complete caches stay
reusable under the current generation protocol.

New current-profile reads return context, Person ID, `person_revision`,
`details_revision`, names and paged contact rows including UUID/kind/value/order.
Use existing 100-row/512-KiB component limits and endpoint/context/Person/revision
bound cursors. A token supplied on subsequent pages must still match current
details state; otherwise return `revision_conflict` and retain the saved draft.
Final validation of the pinned revision is required before treating a traversal
as complete. One oversized row produces a closed over-limit result, never a
truncated editable baseline. Conflict reads are transient editor state, not
sealed cache/Today replacement. Backend checkpoint fixes exact routes/DTOs.

## 4. Command, replay and protected proposals

New operation payload contains exactly Person ID, expected details revision,
optional explicit first/last-name changes, and a bounded contact-operation list.
Missing name means unchanged; explicit null means clear. Contact entries are
typed add(kind,value), edit(id,value), or remove(id); reject repeated target IDs,
unknown fields and empty patches. Revisions are positive canonical decimal strings
within signed 64-bit range. No client normalization, actor/Org, origin, source
ownership or arbitrary contact UUID creation is trusted. Result identifies added
UUIDs by input ordinal using a bounded IDs-only receipt extension for this kind.

Resolve exact authorized receipt replay before current baseline/value checks;
changed payload under the same identity conflicts. For a fresh operation:

1. Enter the existing workspace/context/operation admission. Recheck membership,
   operational workspace and same-Org Person authority. Acquire intake lock before
   Person and deterministically ordered contact locks; bound contention using the
   existing budgets. The current adapter locks Person before dispatch: add the new
   kind's intake admission before that common lock, not only inside its dispatched
   command. Preserve exact-replay ordering and other kinds' lock behavior. Capture's
   optional contact insertion already locks Person; include it in the revision and
   concurrency inventory, and never add an intake acquisition after that lock.
   Freeze the combined lock graph before writing code.
2. Compare `details_revision`, validate every explicitly changed field and target
   against that Person, and validate the resulting identity/contact uniqueness.
   Foreign/missing Person is opaque; another Person's contact is an invalid target
   without disclosing its owner. No partial application of a multi-field edit.
3. Apply the shared command and atomically store receipt, final details revision,
   broad Person revision, changed flag and added contact IDs. If receipt admission
   or any mutation fails, roll back all changes. Server time owns `updated_at`.
4. After changed commit, publish `person.changed` with `data.change=details_changed`
   through the normal publisher, using the explicitly extended dispatch. Add no
   content history fact or contact-attempt credit.
   No-op still gets a durable receipt. Lookup checks current Person visibility,
   not continued existence of each edited/deleted contact UUID.

Keep baseline, proposal, CAS draft revision, immutable envelope and pending overlay
in encrypted SQLite. Display saved success only after a transaction commits.
Unknown outcomes require receipt lookup/exact retry before replacement. On
conflict show baseline/proposed/current names and methods; retain draft, use current,
or explicitly build a new operation against newly fetched current state. Never
force overwrite, automatically rebase or replace an in-flight envelope. Late reads
are fenced by actor/Org/context and editor epoch. Access expiry, sign-out and
revocation preserve the established protected-work rules.

Pending details are labeled separately from server data. They must not silently
change a call/message destination or intake matching before acceptance. Accepted
overlays remain until causally covering sealed reconciliation arrives; Today
ranking stays server-owned. A failed refresh after acceptance is stale display,
not a reason to resend as a new operation. No names/values in logs, metrics,
receipt audit fields or realtime; encrypted local/source content stays erasable.

## 5. Acceptance

- Both native apps: offline multi-field save → terminate/relaunch → real API sync;
  lost-response retry, duplicate submission and no-op produce one acceptance.
- Competing name/contact writes and A→B→A conflict; unrelated stage, notes/tasks,
  tag/custom-field changes do not. Two devices editing different profile fields
  get the declared conservative conflict, not silent merging.
- Explicit contact removal, primary fallback, native insertion order, stable IDs on edit,
  normalized duplicate rejection, household overlap, invalid/foreign IDs, last
  identity removal, untouched large imports and all-or-nothing validation.
- All-writer revision coverage and original/admitted refresh preservation;
  intake lookup versus edits uses one consistent lock order and bounded timeout.
  Include correspondence linking with `add_contact_method=true`, its no-op when
  the method already exists, and capture/profile overlap; real additions advance
  the revision and exact no-ops do not.
- Exact `details_changed` wire fixture, Web broad fallback and Person/People/Today/
  list-count invalidation; no publication on no-op/replay/rollback, and publish
  failure after commit preserves successful receipt/command outcome.
- Complete contact paging, changed page traversal, same-revision old-cache upgrade,
  unsupported capabilities, stale response/account change and oversized rows.
- Full/locked storage, seven-day expiry, receipt mismatch, rollback, deactivation,
  review-workspace denials at HTTP and typed command layers; no sensitive logging.
- Populated installed Mobile 004 stores upgrade in place on both platforms,
  retaining keys, mixed drafts, exact outbox bytes and old receipts. Simulator/
  emulator execution is required; reinstall/build-only evidence is insufficient.
- Relevant D-050 paired Person/Today regression and new/changed hot SQL plans;
  preserve old API/Operator/intake DTOs and unrelated ranking/history behavior.

Run lane checks and combined final gates in the paired plan. Maximum two review/
fix rounds; unresolved blocking findings are not a pass. D-082 accepts the input limits, aggregate conflict behavior, add/remove semantics
and declared contracts; new policy changes still require an accepted decision.
