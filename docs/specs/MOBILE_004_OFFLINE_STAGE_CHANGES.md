# Mobile 004 — Offline Person stage changes

**APPROVED FOR IMPLEMENTATION — D-080, 2026-09-13.** The user accepted the
reviewed draft contracts and isolated synthetic implementation. Proposal wording
below records the accepted design; release remains separate. [Brief](../tasks/MOBILE_004_IMPL.md),
[coordinated plan](../plans/MOBILE_004_010e4_PARALLEL_LAUNCH.md) and
[planning review](../tasks/MOBILE_004_010e4_PLANNING_REVIEW.md) own execution preparation.
Read AGENTS §13, full D-015/D-019/D-020/D-050/D-064/D-065/D-073–079,
[Mobile 002](MOBILE_002_OFFLINE_EDITS.md), [Mobile 003](MOBILE_003_OFFLINE_CONTACT_LOGGING.md),
[Slice 002](SLICE_002.md), and the [architecture baseline](../architecture/ARCHITECTURE_BASELINE.md).
Existing identity, seven-day offline access, protected storage and migration hold remain.

## 1. Outcome and scope

An active member changes an already downloaded Person's stage on iOS or Android
while offline. The saved proposal survives restart. When authorized connectivity
returns, it applies once only if that Person's stage has not changed since the
agent's baseline. An intervening stage change preserves the proposal for review.

Use the Organization's actual stage IDs and labels, including custom stages.
No hardcoded nine-stage vocabulary, guessed ID or label-based matching. Assignment
and stage-catalog administration remain separate. No new People, contact edits,
reassignment, bulk changes, Operator tool, calls, messaging, push or broad redesign.
Native calling follows the existing progression per the user's sequencing choice.

## 2. Proposed product rules

1. **Stage-specific optimistic concurrency.** Compare a positive stage revision,
   not the broad Person mobile revision. An unrelated note, task or contact edit
   must not block a stage change. A stage changed A→B→A still conflicts with an
   older revision even if the current stage equals the proposed target.
2. **One unresolved submitted stage change per Person.** Further changes may be
   saved as a separate follow-up draft, visibly waiting on the previous operation.
   No immutable upload is edited, coalesced or automatically rebased. Notes,
   tasks and manual contact logs on that Person keep their own sequencing rules.
3. **Explicit conflict recovery.** Show the saved baseline, proposed stage and
   freshly authorized current stage. Keep the draft, discard it/use current, or
   review a new proposal against the fetched revision and submit under a new ID.
   No force-overwrite button. Lost responses require receipt lookup/exact retry
   before replacement; an unknown outcome is not a rejection.
4. **Server business time.** StageChanged uses the existing server execution
   clock, manual reason and MobileSession origin. Offline save time is diagnostic
   only. Mobile 003's contact-occurrence exception does not extend to stages.
5. **Truthful Today.** A pending proposal is shown separately from the downloaded
   server stage. Do not optimistically recompute Today membership/ranking. After
   acceptance, request normal sealed reconciliation; failed refresh leaves a
   synced receipt plus stale data, not another mutation or false freshness.
6. **No-op.** With matching revision and valid target, selecting the current
   stage accepts `changed:false` with a durable receipt and no fact/publication.
   Revision comparison precedes no-op detection on fresh operations.

These are proposed customer-visible/conflict contracts for approval with this
spec. They preserve member-wide Person visibility and all server authority gates.

## 3. Shared-contract declaration

| Current | Proposed change and reason | Affected components / compatibility |
|---|---|---|
| Person has broad `mobile_revision`; stage command has no expected version | Add `person.stage_revision`, incremented by every real stage-ID transition; mobile passes the downloaded expected value | Additive PostgreSQL trigger/column, stage command core, mobile reads and native storage; ordinary Web/Operator DTOs stay unconditional |
| Stage command opens/commits its own transaction | Factor a typed transaction-compatible core accepting an optional expected stage revision | Existing wrapper preserves validation, no-op and post-commit publication; mobile shares the core within its receipt transaction |
| Mobile summary contains stage ID/name only | Add summary `stage_revision` as a canonical decimal string | Older clients ignore the new field; new clients cannot edit an unqualified old cache until refreshed, even at the same broad revision |
| Reconciliation has only Person bundles and Today | Add opt-in bounded stage-catalog download with a pinned catalog revision | New request flag, generation binding, catalog pages and conditional seal validation; absent flag keeps old generation behavior |
| Receipt kinds cover notes/tasks/contact attempts | Add `change_person_stage`, resource type `person_stage`, mandatory committed stage revision and Person resource ID | Explicit server/native parsing, visibility and CHECK constraints; existing envelope digests, receipt bytes and kind meanings stay unchanged |
| Conflict reads cover notes/tasks | Add a small authorized current-stage read | New mobile route only; no unbounded Person/history reader or broader audience |

Owned amendments would apply to Slice 002 and Mobile 001–003 on approval. Exact
schema/DTO fixtures and lock inventory are frozen by the backend owner before
native work; materially different conflict, time, authority or lifecycle policy
returns to this spec. No new dependency, sync service or generic conflict engine.

## 4. Revision and catalogue contracts

`person.stage_revision` starts at 1 for existing/new rows. This is a migration
baseline, not a reconstructed historical count. A BEFORE UPDATE trigger derives
it from OLD: +1 only when stage_id differs, otherwise retain OLD's value. Do not
allow a caller-supplied reset or overflow/wrap. Inventory all stage writers,
including original import, admission, refresh, ordinary command and test/admin
paths; no manual increment in one client-specific path. Existing broad Person
revision continues to invalidate the downloaded summary normally.

Add positive `organization.stage_catalog_revision`, advanced transactionally on
real stage insert/delete or name/position changes. It versions IDs, names and
ordering; it does not increment every Person's stage revision. Existing Person
label invalidation stays intact. No stage administration capability is added.

Keep protocol `mobile-v1`. Advertise capabilities `change_person_stage`,
`stage_revisions`, `stage_catalog` only when the compatible schema/backend is
ready. New clients require all three under their bound context to enable Save.
Unknown extra capabilities do not break old clients. A capability is no authority.

Add optional `include_stage_catalog: true` to POST `/api/mobile/v1/reconciliations`.
Missing/false retains the existing request, response and seal semantics. An
opted-in generation pins the current catalog revision and returns that revision
and a stage-page URL. GET `/api/mobile/v1/reconciliations/{id}/stages` pages
`{id,name,position}` ordered `(position,id)`, at most 100 rows and the existing
512-KiB component budget. It uses the existing context/download admission limits,
30-minute generation lifetime and authenticated endpoint-bound cursor discipline.
A single over-budget record returns a closed over-limit error; never truncate a
stage label and call the catalog complete. This is a download safety bound, not
a new Organization stage quota.

Read revision and rows consistently in one transaction. Every page and opted-in
seal revalidates the pinned catalog revision; concurrent catalogue mutation
invalidates staging with `generation_changed`. The catalog revision and stage
rows must share a proven locking/isolation order, including the all-writer
trigger. New clients promote catalog pages and Person generation atomically only
after complete traversal and successful seal. Partial/expired staging cannot
replace the last good catalog or erase saved work. Existing cleanup reclaims
new generation metadata; no permanently retained catalog history.

Old-format cached summaries/catalogs remain readable, but cannot seed a new stage
submission until qualified. Store a local representation marker and permit a
fully fetched same-mobile-revision bundle to replace the old representation;
do not invent a revision or overwrite a higher downloaded version. Preserve
old outbox envelopes, keys, receipts and all drafts on upgrade.

Stage rename/reorder does not represent a Person stage transition; selection
continues by the same valid ID. A stale catalog can propose that ID within the
access window, but the server checks current target existence/Organization.
Deleted/foreign targets are `invalid_stage`; never recreate or map by label.

## 5. Operation, receipt and current-stage read

POST `/api/mobile/v1/operations` keeps the existing immutable envelope and
`X-Mobile-Context`. New kind `change_person_stage` has exactly
`{person_id, stage_id, expected_stage_revision}`; IDs are UUID strings, revision
is positive canonical decimal within signed 64-bit range. No actor, Organization,
source clock, arbitrary patch or authority fields. Keep old canonical bytes and
HMAC domains unchanged; include the new payload only in this new kind's digest.

Under existing workspace/context and operation-identity locks:

1. Validate current authentication/context and replay visibility. An existing
   matching receipt returns its original result before new baseline/target checks;
   it does not reapply after a later stage transition. Changed content using the
   same operation identity returns the existing payload-mismatch conflict.
2. For a fresh operation, lock the same-Org Person and recheck active membership,
   operational workspace and normal stage-change authority. Missing/foreign
   Person remains opaque. Target stage must belong to that Organization.
3. Compare locked stage revision; mismatch is `409 revision_conflict`, even when
   the target is already current. Apply the typed command core on a matching
   baseline, preserving ordinary command error precedence outside this adapter.
4. Atomically commit stage update, optional single StageChanged fact and receipt.
   Rollback leaves none applied. Publish one stage invalidation after a changed
   commit only. Receipt `resource_type=person_stage`, `resource_id=person_id`,
   `committed_revision=stage_revision`, and `person_revision=mobile_revision` are
   captured in that transaction. `changed:false` still has a nonnull revision.

Receipt lookup for person_stage checks the current same-Org Person plus existing
session/context/workspace rules. It returns historical acceptance, not a current
stage snapshot. It must not depend on the original target label still existing.
Unknown receipt kinds fail closed rather than falling through a task branch.

GET `/api/mobile/v1/people/{person_id}/stage` returns bound `context_id`,
`person_id`, `person_revision`, `stage_revision` and `{id,name}` current stage.
Use normal read authority, no-store and bounded output; do not renew the offline
lease. Its single-record result supports conflict review only, not promotion of
an incomplete full Person bundle/Today generation. Late responses are fenced by
actor/Org/context and local editor epoch. Error disclosure matches existing
mobile current-note/task endpoints; no customer content in receipts or logs.

## 6. Native durability and acceptance

Both apps use typed stage drafts, CAS draft revisions and immutable outbox rows.
Saving baseline + proposal + operation + pending overlay is one protected SQLite
transaction. Display saved success only after commit; disk-full/locked/key errors
leave the editor recoverable. A follow-up is a draft, not silently queued work.
Keep normal seven-day locking, reauthorization, sign-out/account changes and
revoked/deleted resource behavior; never transfer work between identities.

Required evidence (synthetic operational workspace plus another Org/actor):

- Offline Save → terminate/relaunch → real API sync: one stage change/fact/receipt;
  repeat after a lost response and concurrent duplicate submission.
- Competing Web/native stage edits, A→B→A, same-target changed-revision conflict;
  unrelated note/task edits do not conflict. Same-revision no-op creates no fact.
- Current-stage conflict review → explicit revised proposal → fresh operation;
  follow-up drafts and mixed existing queues survive restart and retry.
- Catalog paging, custom labels, rename/reorder during traversal, old-server
  capability absence, missing old-cache token, same-revision representation
  upgrade and late response/account switches; no false complete catalog/Today.
- Foreign/missing Person and stage, member deactivation, review hold, expired
  context, receipt payload mismatch, transaction rollback and no secret/content logs.
- Installed populated Mobile 003 store → Mobile 004 upgrade on each platform,
  preserving old drafts/queued bytes/receipts/keys; full/locked storage and access
  expiry tests. Build-only or reinstall proof is insufficient.
- Existing Web/Operator stage command semantics, old mobile fixtures, migration
  stage writers, and one relevant D-050 paired Today/Person read and changed hot
  query plans. Test catalogs beyond one page without imposing a new product quota.

Run required backend/native checks and combined final gates per the briefs;
maximum two review/fix rounds. Physical phones, cellular, signing/distribution,
live customer data, calling and deployment remain separate, deferred work.
