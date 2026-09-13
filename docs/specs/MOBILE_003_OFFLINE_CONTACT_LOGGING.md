# Mobile 003 — Offline contact logging

**APPROVED FOR IMPLEMENTATION — D-078, 2026-09-13.** The user accepted this
specification and the recommended occurrence-time policy after D-077 planning.
Mobile 001/002 are released; retain their protected-storage, seven-day access
and receipt policies. The time exception below applies only to the new contact kind.

Read [AGENTS](../../AGENTS.md), decisions D-022/D-031/D-032/D-052/D-073–078,
[Mobile 001](MOBILE_001_OFFLINE_FIELD_WORK.md),
[Mobile 002](MOBILE_002_OFFLINE_EDITS.md), the frozen
[001 contract](../tasks/MOBILE_001_CONTRACT.md) and
[002 contract](../tasks/MOBILE_002_CONTRACT.md). The
[brief](../tasks/MOBILE_003_IMPL.md), [review](../tasks/MOBILE_003_010e3_PLANNING_REVIEW.md)
and [parallel plan](../plans/MOBILE_003_010e3_PARALLEL_LAUNCH.md) own execution and readiness.

## 1. Field workflow and scope

On an already downloaded Person, an active member records a manual contact
attempt: channel, outcome and when it happened. Save commits the contact draft
and immutable upload operation together in protected SQLite. The app distinguishes
saved on this device, waiting to sync, accepted and needs attention. It preserves
saved work through interruption, upgrade, lost responses and access locking.

This records an interaction that already happened; it does not place a call,
send a text/email, create an Inquiry, change assignment/stage, or correct a
LiveKit/Telnyx call. Use the existing vocabulary: channels `call`, `text`, `email`,
`other`; outcomes `reached`, `no_answer`, `left_message`, `sent`, `busy`,
`wrong_number`. Preserve current manual-command validation; do not introduce a
new outcome matrix or invent `meeting` as a wire channel. A meeting may use Other.
No free text belongs to the immutable contact fact. Notes and follow-up tasks use
the already implemented workflows, each with its own save and receipt; no new
compound contact+note+task transaction or dependency chain.

Do not offer this as an automatic post-call action. The existing call settlement
already records its own contact attempt, and call corrections use their existing
command. A manually logged interaction cannot be matched or merged with a call
by phone number, timestamp or similar-looking fields. Explicitly warn in the
manual form that calls made through the CRM already have a contact record.

## 2. Declared contract changes

| Current | Proposed change and reason | Compatibility / owner |
|---|---|---|
| Manual LogContactAttempt is not idempotent and owns its transaction | Factor a typed transaction-compatible core; mobile operation and contact fact commit atomically | Mobile backend owns command/core and tests; ordinary Web/Operator wrappers keep their request/response, timestamp and repeated-submit behavior |
| mobile-v1 supports note/task operations only | Add `log_contact_attempt` capability/kind and `contact_attempt` receipt resource type | Additive operation parser, receipt CHECK/visibility handling and both native dispatchers; old canonical bytes and old receipts remain unchanged |
| Mobile 001/002 keep business timestamps server-owned | For this new manual contact kind only, use accepted user-reported occurrence time; record server receipt time separately | This is the explicit policy exception accepted under D-078; note/task clocks and automatic call clocks do not change |
| Native drafts/receipts branch among note/task types | Add a dedicated contact draft, immutable operation and saved-work rendering | Non-destructive protected SQLite upgrades; never treat an unknown kind as a task |
| Contact facts affect Today without changing the downloaded Person fields | Schedule a fresh reconciliation after acceptance; keep local pending state separate from server Today | No fabricated Person revision increment, new history component or locally recomputed team-wide ranking |

The migration lane never edits these contracts. No mobile-specific service or
second authorization path is created. Rust/Axum and the current typed command
layer remain the only business-write path.

## 3. Time and Today — accepted occurrence-time policy

**Use when the contact happened (D-078).** A Monday contact uploaded on
Wednesday must not satisfy a Tuesday Inquiry. D-022's existing occurrence-time
comparison and D-052's maximum-derived dates remain unchanged. Every currently
valid contact outcome continues to count as an attempt; this does not redefine
Today to count only successful conversations.

The new payload carries mandatory `occurred_at`, a finite RFC3339 instant,
normalized to UTC. The native draft defaults it to the time the agent opens the
contact log and permits explicit date/time correction before submission. Preserve
the resolved instant through timezone/DST changes and relaunch. Ambiguous local
times need an explicit offset choice; nonexistent local times cannot be saved as
a silently shifted instant. The form says the time is reported by the agent.

The outer `device_recorded_at` remains its existing untrusted metadata and part
of the original digest. Never generally promote that field into business clocks.
The server validates the explicit payload time using a server clock sampled after
required locks. A future occurrence returns `422 contact_time_in_future` without
writing a fact/accepted receipt. Preserve the original local operation and show
that the phone's time may need correction; allow an explicit retry later or a
revised operation after a definitive rejection. Never silently clamp a time.

There is no receipt-age cutoff derived from the seven-day access window. Work
saved while authorized can remain protected beyond that window and submit after
current online reauthorization. A past occurrence is not itself a permission or
proof of past membership. All executions require live authorization and an
operational workspace now. No automatic history/time inference from phone logs.

`contact_attempted.occurred_at` is the accepted reported time; `recorded_at` is a
server clock sampled at acceptance after locks. Preserve the original accepted
values on replay. The fact uses the trusted actor, `Origin::MobileSession`,
server-owned correlation and no call causation/correction ID. Receipt accepted_at
retains its existing server-owned meaning. Existing Web/Operator manual logs and
automatic call settlement/correction keep their clocks and semantics.

This explicit reported-time policy is accepted under D-078. Implementers do not
substitute upload time or extend this exception to existing operation kinds.

## 4. HTTP, typed execution and retry

Use existing `POST /api/mobile/v1/operations` and `GET /operations/{id}`,
bootstrap/session/context checks, no-store responses, request deadlines and
canonical digest domain `crm-mobile-operation-v1\0`. Proposed new payload:

```json
{"person_id":"uuid","channel":"other","outcome":"reached","occurred_at":"2026-09-13T12:00:00Z"}
```

Unknown fields/enums fail closed. Do not accept actor, Organization, permission,
assignment, call ID, correction ID, or expected Person revision. This is an append,
not a last-write-wins edit: another actor logging another contact is not a version
conflict. The same fields with a genuinely new operation ID describe a second
manual log; no cross-device heuristic deduplication is promised.

A dedicated typed Rust transaction core must accept only already validated manual
contact data and server-derived context. It locks the Person through the trusted
Organization, inserts exactly one fact, and returns its ID/time without committing
or publishing. Ordinary wrappers choose server execution time and retain their
existing behavior. The mobile adapter reuses this core inside the current
workspace → context → operation → Person → membership ordering. Replay rechecks
current identity, workspace and resource visibility before returning the stored
result, and does not run new mutation/time validation on an accepted operation.
The same ID with different normalized content/context/time conflicts.

Receipt shape remains unchanged, with `resource_type:"contact_attempt"`, fact ID
as resource_id, `committed_revision:null`, `changed:true`, and current positive
person_revision. All legacy note/task receipt rules remain strict. Extend receipt
lookup to prove the original fact belongs to that exact Person/Organization under
current visibility. Never return a receipt for another context, target or actor.
Deleting/erasing a target must not erase the consumed operation marker or permit
re-execution. A 404 receipt still does not prove an uncertain upload failed.

Fact, D-052 trigger-maintained activity columns and durable receipt commit once.
Rollback yields neither fact nor receipt. Publish exactly one existing
`person.changed{contact_attempted}` only after a new commit, never on replay or
rejection; no contact data appears in that event. Selection/sealing must see the
accepted Today state through the existing consistent query path. Audit all
receipt resource-type, revision and PersonChange branches; none may fall through
to note/task logic. Logs contain IDs and closed outcomes, never contact values.

## 5. Native local state and access

Add explicit contact draft/operation variants in SwiftUI and Compose. Store
Person/context binding, draft ID/CAS revision, channel/outcome, resolved occurrence
instant, original device save time and immutable serialized operation. Save success
requires the SQLite transaction. A failed/full/locked store preserves the editor
and never displays a successful save. Upgrade existing populated Mobile 002 stores
without changing any prior queued bytes/IDs, drafts, receipt types or cache state.

Two taps or two editors submitting the same draft converge on one local operation.
Submitted bytes are immutable. A deliberate new contact uses a new draft identity,
even on the same Person while an earlier contact is pending. This must not inherit
the single-target edit slot or collapse two real contacts. Subsequent editing of
an unsent draft is local CAS; an uncertain upload cannot be replaced by a new ID.
Existing note/task queues continue independently.

Keep encrypted saved-work metadata sufficient to display channel/outcome/reported
time after a receipt; no unbounded contact-history replication is added. Validate
receipt operation ID, contact resource type, null committed revision and target
binding before acknowledging locally. Store acceptance atomically. Contact facts
may leave person_revision unchanged because summary/notes/tasks did not change;
that equality is not evidence that the displayed Today snapshot is fresh.

While offline, keep server Today ordering/membership and show a pending contact
badge. Do not remove a Person merely because a local contact is queued. After an
accepted receipt, mark the saved contact synced and request normal bounded
reconciliation; only a successful seal replaces Today. If refresh fails, retain
accepted status plus clearly stale Today. No lost invalidation can strand this
refresh, and a failed refresh must not cause another contact upload.

Keep Mobile 001's seven-day authorization, account/Organization epochs, protected
sign-out/revocation retention and response fencing. Expiry locks CRM content and
preserves committed work. Missing capability disables only the new submission,
never deletes a draft or alters old queued operations. Unknown receipt variants
fail closed and remain recoverable. No endpoint chooser, relaxed device keys,
new background-execution guarantee, new push permission or phone setup is included.

## 6. Verification and completion

Use the existing isolated native envelope: two Organizations/actors, both native
virtual devices, 100 selected People, 1,000 notes, 1,000 tasks and up to 100 mixed
queued actions. These are fixtures, not new product limits. Preserve actual shared
services and installed demo stores. Physical-phone/cellular and broad design work
remain user-deferred.

1. Typed core/receipt: one fact under concurrent replay, lost accepted response,
   changed body/time mismatch, rollback/timeouts and key rotation; old operation
   bytes, add-note null revisions and note/task edit receipts remain unchanged.
2. Authority: same-Org non-assignee allowed, foreign/missing targets denied,
   revoked membership/context, review workspace, delayed responses, missing fact
   and replay after target removal cannot disclose or recreate anything.
3. Time: Monday occurrence/Wednesday upload cannot answer Tuesday Inquiry; later
   existing contact maximum never regresses; equal-instant ordering; future clock
   rejection/retry, timezone/DST correction and delayed upload after reauthorization.
   Pin unchanged Web/Operator/call clock and correction behavior.
4. Today: pending contact does not hide work; accepted contact triggers reconciliation
   even with unchanged Person revision; missed realtime, failed seal and cached
   old Today never create a duplicate. Existing generation selection stays bounded.
5. iOS and Android: actual native offline Save → terminate/relaunch → upload →
   exact receipt; two rapid Save taps; two intentional contacts; mixed queues;
   storage/CAS failure; lost response; time rejection and explicit revised work;
   seven-day lock/reauthorization; installed populated-store upgrade.
6. One relevant D-050 hot-plan/paired Today regression, focused command/mobile/
   call/Today/tenant tests, final repository/live SQLx/DB checks and native builds.
   One bounded review plus fix verification, at most two rounds. Report actual
   runtime/fixture/log evidence; no completion claim from compilation alone.

No implementation or acceptance test is claimed by this plan.
