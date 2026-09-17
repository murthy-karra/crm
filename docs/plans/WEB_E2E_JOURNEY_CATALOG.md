# Web E2E journey catalog

Status: journey grouping reviewed by the user, 2026-09-17 (Lavish feedback: “good”).
Inspected checkout: `64c4671`. Initial implementation: [lead family and runner](../../e2e/README.md).
The user's subsequent implementation brief selects the lead flow (02a–02d) first.
Workspace access (01) followed. Routing (03), relationships (05), tasks (06), and
saved lists/Today (07) now have bounded family implementations. Families 04 and
08–10 remain planned. See the harness README and family task records for verified
status, exact coverage and exclusions.
Existing decisions and slice contracts remain authoritative.

## Goal and grouping rule

Exercise complete user journeys through the real Vue Web client, Rust API, typed
commands, PostgreSQL and Centrifugo. A journey groups actions because each action
uses or changes state produced by the previous one. A family groups related
journeys and owns one isolated Docker environment per invocation.

Per the subsequent implementation brief, dependent journeys within a family run
as named sequential steps against one versioned seed with no intermediate resets.
Prerequisite failure blocks later steps; retries recreate the entire family.
Separate families never depend on one another or share mutable state. Alternative
branches use separate records within the same family environment.

## Existing foundations

- [Web package](../../web/package.json) has Vitest and `playwright-core`, but no
  unified Playwright Test configuration or journey-runner script was found.
- The [family-refresh browser acceptance](../../backend/crates/crm-api/tests/fixtures/family_refresh_browser.mjs)
  already drives real Web/API state, loses a committed response, retries the same
  confirmation, exercises partial cancellation/remainder and reloads.
  Its [fixture instructions](../../backend/crates/crm-api/tests/fixtures/family_refresh_acceptance.md)
  distinguish browser, database and plan evidence.
- Existing Rust API/database tests and synthetic migration/inference providers
  are useful starting points. They do not by themselves prove a complete browser
  journey. The guide-capture script is also not a regression runner.
- [Project state](PROJECT_STATE.md), [architecture](../architecture/ARCHITECTURE_BASELINE.md),
  [decision index](../decisions/DECISION_INDEX.md) and full applicable decisions
  were consulted. D-015, D-021, D-023, D-050 and D-064/065 constrain the test design.

## Family map

| ID | Journey family | Additional boundary |
|---|---|---|
| 01 | Workspace access and team membership | Core stack |
| 02 | Lead arrival, response, and handoff | Core stack |
| 03 | Inbound lead routing and unresolved recovery | Controlled external input |
| 04 | Captured correspondence and reply follow-up | Controlled external input |
| 05 | Relationship notes, tags, and custom fields | Core stack |
| 06 | Task follow-up from creation to completion | Core stack |
| 07 | Saved lists and configurable Today work | Core stack |
| 08 | Operator-assisted CRM work | Controlled inference |
| 09 | Browser call and required outcome | Telephony integration |
| 10 | Migration from source evidence to reviewed CRM state | Controlled FUB source |

All families retain the real core stack. “Controlled” means supplying predictable
synthetic inputs at an external boundary, not replacing CRM API responses.

## 01. Workspace access and team membership

**Why these belong together:** Membership and session state determine every later journey's authority.

**Actors:** Platform administrator, Organization admin, two members, and a second Organization.

**Starting state:** A platform identity and isolated Organizations created through application commands.

| Scenario | Stateful journey |
|---|---|
| **01a — Open a workspace** | Platform admin creates an Organization → invites its first admin → invitation is accepted → admin invites an agent → agent signs in and reaches Today. |
| **01b — Change responsibility and access** | Promote/demote a member → reject removal of the last active admin → deactivate a signed-in member → their next request loses access and realtime disconnects → reactivate → sign in again. |
| **01c — Recover and close access** | Revoke/reissue an invitation → old link stays invalid → platform admin restores admin continuity without tenant-data access → logout and sign in as another actor without stale private content. |

**Required proof:** Membership and administrative facts persist; deactivation preserves attributed records; revoked sessions cannot keep reading or writing. A platform administrator never gains CRM access merely from that role.

**Important branches:** Invitation replay, revoked/expired links, denied member-only/admin-only routes, cross-Organization resource IDs, and retained cache after identity change.

**Source anchors:** [SLICE_001.md](../../docs/specs/SLICE_001.md), [SLICE_004.md](../../docs/specs/SLICE_004.md), [MembersView.vue](../../web/src/views/MembersView.vue).

## 02. Lead arrival, response, and handoff

**Why these belong together:** An Inquiry creates work; contact resolves that work; another Inquiry and reassignment change who must act.

**Actors:** Agents A and B in one Organization; agent C in another.

**Starting state:** An operational Organization with default feeds, no saved-list sources or due tasks, and known seeded stages.

| Scenario | Stateful journey |
|---|---|
| **02a — Work a new lead** | Agent A submits New lead → Person, contact methods, Inquiry, routing, assignment and initial stage appear → Today explains the work → log contact → the inquiry reason clears. |
| **02b — Handle a returning lead** | Submit a genuinely new Inquiry with the same contact and a different source → reuse the Person → retain original source attribution → Today becomes actionable again. |
| **02c — Hand the relationship to a colleague** | Change stage → reassign to B → B's Today gains the inquiry work and A's loses it → both can still view the Person → reload and inspect history. |
| **02d — Recover an uncertain submission** | Lose the response after a real lead submission commits → retry the same form submission → exactly one Inquiry and one set of creation facts exist. |

**Required proof:** Person identity is stable, Inquiries remain distinct, and history/derived activity agree with PostgreSQL. Assignment changes Today ownership, not Organization-wide Person visibility. Another Organization sees neither records nor events.

**Important branches:** Same submission retry versus new submission, ambiguous contact matches, missing contact methods, rejected optimistic stage/assignment changes, and another remaining Today reason.

**Source anchors:** [SLICE_002.md](../../docs/specs/SLICE_002.md), [SLICE_003.md](../../docs/specs/SLICE_003.md), [NewInquiryView.vue](../../web/src/views/NewInquiryView.vue), [events.ts](../../web/src/realtime/events.ts).

## 03. Inbound lead routing and unresolved recovery

**Why these belong together:** Routing settings, incoming source data and recovery of unresolved payloads form one intake lifecycle.

**Actors:** Organization admin and the routing pool of agents.

**Starting state:** An operational Organization, an intake address, and synthetic inbound messages.

| Scenario | Stateful journey |
|---|---|
| **03a — Route unattended leads** | Admin selects default assignee → submit synthetic mail at the real inbound endpoint → routed lead appears on the agent's Today; repeat with unassigned mode → People contains the lead without inquiry work on an agent's Today. |
| **03b — Maintain round-robin continuity** | Enable round-robin → receive successive new leads → deactivate a pool member → rotation skips them → reactivate and continue from the retained position; an explicit assignment does not consume a turn. |
| **03c — Resolve failed extraction** | Receive unmatched or invalid mail → metadata appears in Unresolved → admin opens raw content and retries → qualified extraction creates the lead once; use a separate payload to exercise discard. |

**Required proof:** Raw payload is preserved encrypted; routing facts explain actual outcomes; processing retries do not duplicate business records; ordinary members cannot open raw unresolved content or retry/discard it.

**Important branches:** Direct versus forwarded mail, low-confidence/invalid extraction, inactive default assignee, invalid intake address, worker interruption, and duplicate delivery according to the intake contract.

**Source anchors:** [SLICE_007b.md](../../docs/specs/SLICE_007b.md), [SLICE_007e.md](../../docs/specs/SLICE_007e.md), [SLICE_007f.md](../../docs/specs/SLICE_007f.md), [SLICE_007h1.md](../../docs/specs/SLICE_007h1.md), [SLICE_008.md](../../docs/specs/SLICE_008.md).

## 04. Captured correspondence and reply follow-up

**Why these belong together:** Outbound capture, inbound replies and held mail change the same relationship's timeline and response state.

**Actors:** Attributed agent, another member, and a synthetic correspondent.

**Starting state:** An agent capture address and a Person with a matching email contact.

| Scenario | Stateful journey |
|---|---|
| **04a — Follow an email exchange** | Deliver synthetic outbound CC/BCC mail → capture metadata and email contact attempt appear → inquiry work clears → deliver an inbound reply → Today shows client-replied work → capture the next outbound response. |
| **04b — Recover missed correspondence** | Forward an older message → timeline places it at its original time → re-forward it → no duplicate entry and no backward movement of activity maxima. |
| **04c — Resolve held correspondence** | Deliver unmatched correspondence → attributed agent sees a held row → link to an existing Person or dismiss a separate row → rotate the capture address → old address no longer captures correspondence. |

**Required proof:** Metadata is visible Organization-wide, held rows remain attributed-agent-only, and raw correspondence has no read surface. Capture never silently creates a Person or enters the lead-extraction pipeline.

**Important branches:** Other-member/admin held-queue denial, reply-all versus mail that never reaches capture, deduplication, forwarded dates, and stale link/rotation attempts.

**Source anchors:** [SLICE_009.md](../../docs/specs/SLICE_009.md), [EmailCaptureView.vue](../../web/src/views/EmailCaptureView.vue).

## 05. Relationship notes, tags, and custom fields

**Why these belong together:** Agents build shared relationship context on one record, with different ownership rules for each edit.

**Actors:** Author/member A, member B and an Organization admin.

**Starting state:** A visible operational Person and custom-field definitions created through the admin surface or setup commands.

| Scenario | Stateful journey |
|---|---|
| **05a — Collaborate on notes** | A adds a note → B sees it on the Person timeline → A edits → admin can edit/delete → deleted text disappears after reload and in another session. |
| **05b — Organize a Person** | Create/apply a tag → another member applies the same normalized name without a duplicate definition → remove the Person link → exercise creator permissions while unused and admin-only management once used. |
| **05c — Maintain typed values** | Admin creates text, number, date and choice fields → member sets and edits values → another session reads them → clear a value → archive/restore a definition or option while preserving stored values. |

**Required proof:** Note and field content lives in erasable CRUD, not immutable facts or realtime payloads. Note tombstones clear bodies. Person tag/value changes propagate through real invalidations; catalog changes follow their existing refetch behavior.

**Important branches:** Non-author note edit denial, invalid typed values, wrong-Organization IDs, archived field versus archived option behavior, stale catalog choices, and rejected writes not leaving optimistic success.

**Source anchors:** [SLICE_011e.md](../../docs/specs/SLICE_011e.md), [SLICE_015.md](../../docs/specs/SLICE_015.md), [SLICE_019.md](../../docs/specs/SLICE_019.md), [PersonDetailView.vue](../../web/src/views/PersonDetailView.vue).

## 06. Task follow-up from creation to completion

**Why these belong together:** The task row, Person timeline and Today panel are three views of the same follow-up lifecycle.

**Actors:** Task creator, task assignee, unrelated member and admin.

**Starting state:** A Person with independently controlled inquiry/list reasons; task due times comfortably inside or outside each window.

| Scenario | Stateful journey |
|---|---|
| **06a — Complete and reopen work** | Create a task assigned to B → B sees it on Today when due → complete from Today → Person history shows completion → reopen from the Person page → work returns. |
| **06b — Reschedule and transfer work** | Snooze an open task → verify the new due instant and resulting eligibility → edit/reassign it → new assignee owns the task work → delete → no active or completed task content remains in reads. |
| **06c — Separate task ownership from Person ownership** | Assign the Person to A and a due task to B → B gets task work → complete it while another reason remains → only the task reason clears. |

**Required proof:** Task state is durable; completion history is a projection of that row, so reopening removes it. Complete/reopen/snooze obey target-state idempotency. Today task ownership follows the task assignee; authorized managers are creator, assignee or admin.

**Important branches:** No due date, date-only local end-of-day conversion, overdue versus due-soon, unrelated-member write denial, completed-task snooze, deactivated assignee, and delete tombstone. Snoozing to tomorrow need not exit the rolling 24-hour window.

**Source anchors:** [SLICE_016.md](../../docs/specs/SLICE_016.md), [TodayView.vue](../../web/src/views/TodayView.vue), [PersonDetailView.vue](../../web/src/views/PersonDetailView.vue).

## 07. Saved lists and configurable Today work

**Why these belong together:** A saved definition controls discovery and can also supply ongoing work; edits and broken references affect both surfaces.

**Actors:** Personal-list owner, another agent and an Organization admin.

**Starting state:** A small known book spanning stages, assignments, tags, activity and custom-field values.

| Scenario | Stateful journey |
|---|---|
| **07a — Build a personal work source** | Filter People → preview results → save personal list with sort → reopen → enable as a Today source → change a Person so membership changes → observe list, count and Today reason. |
| **07b — Share criteria without sharing private definitions** | Admin creates a shared list → agent opens and duplicates it → agent edits their copy → admin cannot discover the agent's personal definition; each viewer resolves 'me' to themselves. |
| **07c — Repair a broken source** | Delete a referenced tag or archive a referenced field → list remains repairable and Today reports unavailable source while other work remains → repair/restore → membership returns → delete the list and verify source preference cleanup. |
| **07d — Tune built-in work** | Admin previews a Today rule for a selected agent → saves → agent gets the revised deterministic reasons → disable/revert → default behavior returns; the fixed task axis remains outside these controls. |

**Required proof:** Saved criteria and sort persist; unsaved preview edits do not change Today. Overlapping sources yield one Person with the correct reasons. Contact clears only reasons whose predicates stop matching. Personal definitions stay private even from admins.

**Important branches:** Five-source limit, stale revisions, archived options remain valid filter choices, stale-reference versus genuine empty result, and keeping all usable work when one source fails.

**Source anchors:** [SLICE_011b.md](../../docs/specs/SLICE_011b.md), [SLICE_011b_SORT.md](../../docs/specs/SLICE_011b_SORT.md), [SLICE_011c.md](../../docs/specs/SLICE_011c.md), [SLICE_011d.md](../../docs/specs/SLICE_011d.md), [SLICE_019b.md](../../docs/specs/SLICE_019b.md).

## 08. Operator-assisted CRM work

**Why these belong together:** The conversation proposes or requests work, while normal application commands and receipts determine what actually happened.

**Actors:** Signed-in agent with server-owned permissions and a deterministic inference provider.

**Starting state:** People, saved lists and tasks created through normal commands; inference scripted at the provider boundary.

| Scenario | Stateful journey |
|---|---|
| **08a — Find and explain work** | Ask for People/list results and why a Person is on Today → approved tools retrieve current authorized data → follow a returned Person link and compare with the conventional UI. |
| **08b — Create only after confirmation** | Ask to create a task → inspect proposal → verify no task yet → dismiss; repeat and confirm → exactly one task appears on the Person and appropriate Today view. |
| **08c — Complete with Undo** | Ask to complete a task the Operator has resolved → server receipt shows completion → another view observes it → click Undo → task reopens through the normal command. |

**Required proof:** Read answers and mutations use the same application paths as the Web UI; proposal/receipt data is server supplied. Audit rows contain permitted metadata, not conversation text. Business effects persist across reload even though session-only cards do not.

**Important branches:** Ambiguous names, cross-Organization/private-list requests, malicious note content, inference timeout/failure, denied task completion, changed authorization at confirmation, and uncertain confirm response. Operator call confirmation joins family 09.

**Source anchors:** [SLICE_005.md](../../docs/specs/SLICE_005.md), [SLICE_013.md](../../docs/specs/SLICE_013.md), [SLICE_018.md](../../docs/specs/SLICE_018.md), [scripted.rs](../../backend/crates/crm-operator/src/providers/scripted.rs).

## 09. Browser call and required outcome

**Why these belong together:** Dialing, provider lifecycle, automatic contact evidence and the human outcome must remain one coherent call.

**Actors:** Caller, a different Person assignee, and a controlled call recipient/provider.

**Starting state:** Callable synthetic Person and an isolated telephony test environment.

| Scenario | Stateful journey |
|---|---|
| **09a — Finish a call** | Click Call → browser joins → call rings/answers → end call → automatic contact evidence exists → required outcome remains on caller's Today → choose outcome → timeline and Today settle. |
| **09b — Recover interrupted calling** | Exercise busy/no-answer, cancel before ringing, microphone denial or provider failure → preserve the correct contact-attempt/no-attempt distinction → reload and recover authoritative call state or outstanding outcome. |
| **09c — Confirm an Operator call** | Ask to call the Person → server-backed proposal → cancel or confirm → confirmation uses the same call lifecycle → repeat delivery/confirmation cannot create a second call where the existing contract deduplicates it. |

**Required proof:** Only the caller controls the call; org members may read it. An automatic attempt exists only for qualifying transitions. Human outcomes append correction facts; an unresolved outcome belongs to the caller, even when someone else owns the Person.

**Important branches:** One active call per user, repeated/out-of-order provider events, correction chain, session/access loss, and separate evidence for signaling/state versus actual audio transport.

**Source anchors:** [SLICE_006.md](../../docs/specs/SLICE_006.md), [SLICE_006b.md](../../docs/specs/SLICE_006b.md), [SLICE_006c.md](../../docs/specs/SLICE_006c.md), [useCall.ts](../../web/src/telephony/useCall.ts).

## 10. Migration from source evidence to reviewed CRM state

**Why these belong together:** Migration is its own long-lived state machine; imported data remains under review rather than entering normal operational work.

**Actors:** Migration admin, ordinary member and another Organization's admin.

**Starting state:** A new empty destination plus versioned synthetic FUB books; separate fixtures for original, admitted and recovered cohorts.

| Scenario | Stateful journey |
|---|---|
| **10a — Assess and retain source evidence** | Save a connection → bounded assessment → capture core/history evidence → inspect coverage and exclusions → pause/cancel/resume or retry → reload durable reports. Assessment success is not full capture or migration completeness. |
| **10b — Import core People into review** | Map stages and agents → inspect immutable preview → confirm exact plan → review imported People, provenance and reconciliation → prove overlapping contacts did not merge distinct source People and the workspace is held. |
| **10c — Add dependent families** | Import tags/fields, notes/tasks and historical metadata for eligible original/admitted/recovered People → inspect paged Person review → verify mappings, explicit date-only timezone, preserved source evidence and no native contact credit from imported history. |
| **10d — Refresh, repair and recover** | Capture a later source → compare → refresh existing People or admit genuinely new People → repair missing mappings → recover proven never-imported People once → prepare combined metadata/activity/history refresh → confirm exact families → cancel one family → prepare and finish the exact remainder. |

**Required proof:** Source identities, frozen plans, successful writes and reconciliation survive reload/retry/restart. Local changes and tombstones stay protected; source absence is not deletion. Confirmed clears/removals, task transitions and append-only history corrections follow 010g1. Every completed run retains the admin review hold.

**Important branches:** Stale preview, lost confirm response, partial cancellation, budget pause, worker restart, expired compatibility evidence, unknown versus inaccessible coverage, original/admitted/recovered identity overlap, and member/admin operational-action denial while held.

**Source anchors:** [SLICE_010c.md](../../docs/specs/SLICE_010c.md), [SLICE_010_ADMITTED_PEOPLE_LADDER.md](../../docs/plans/SLICE_010_ADMITTED_PEOPLE_LADDER.md), [SLICE_010e6_NEVER_IMPORTED_RECOVERY.md](../../docs/plans/SLICE_010e6_NEVER_IMPORTED_RECOVERY.md), [SLICE_010g1_COMBINED_FAMILY_REFRESH.md](../../docs/plans/SLICE_010g1_COMBINED_FAMILY_REFRESH.md).


## Shared verification dimensions

These belong inside selected business journeys rather than becoming a second,
duplicated suite for every entity.

1. **Two real sessions.** Use separate browser contexts for different agents,
   each with one active tab under D-050. Verify an affected observer sees committed
   state without manual refresh where that mutation promises an invalidation.
   Observe the real WebSocket publication and consequent API read, so a periodic
   poll cannot disguise broken realtime.
2. **Delivery recovery.** Interrupt only the owned test connection/service;
   mutate through another session; reconnect and refetch authoritative state.
   Also prove a command remains committed when publication fails. Catalog/list
   definition changes and migration progress use their actual refresh/poll
   contracts; do not invent realtime events for them.
3. **Durability.** Reload or use a fresh signed-in context, then inspect narrowly
   scoped read-only DB assertions for identity, counts, ownership, history,
   tombstones or import results. An API 200 or toast alone is insufficient.
4. **Tenant and role boundaries.** Exercise guessed foreign IDs through the real
   authenticated API as well as hidden/denied UI paths. Verify no foreign data,
   no mutation and no cross-Organization event. Admin and platform-admin roles
   are distinct; assignment does not narrow ordinary Person visibility.
5. **Failure and retry.** Where supported, drop a real response after commit and
   retry the same operation identity. Prove the declared idempotency behavior.
   Do not assume every create is idempotent: native task creation lacks a
   client-minted idempotent create ID. Rejected optimistic writes must settle to
   server truth rather than leaving false success.
6. **Honest state.** Distinguish empty, unavailable, held, cancelled and complete.
   A Person can retain another Today reason after contact or task completion.
   A migration can settle every planned item while still having coverage gaps.

## Test environment boundaries to carry into harness design

- Own an isolated database, API, Web server and Centrifugo instance/configuration
  or equivalently isolated owned resources. Do not reset shared development or
  use its destructive bootstrap. Isolate Rust and Web build outputs.
- Use migrations for schema; use existing typed application commands/CLI or
  supported test fixtures for business setup. Read-only SQL can verify durable
  outcomes. Direct business-table writes must not replace the journey or bypass
  workspace/authorization rules (D-021). Cleanup can drop an explicitly owned
  disposable database.
- Drive actions under test through the Web. Setup and synthetic external delivery
  may use APIs. Never fulfill CRM API requests with fabricated success responses.
  Network fault injection may forward a real request and deliberately lose its
  response, as the current 010g1 browser fixture does.
- Email inputs enter the real inbound handler; separately label a future real
  mail-relay smoke test. Script inference at the provider boundary while retaining
  the real Operator tool loop, authorization, commands and audits.
- Migration source fixtures may reuse the existing Reader seam for repeatable
  workflow tests. Those tests must be labeled as not proving the live FUB HTTP
  adapter. A local HTTP source simulator or authorized live smoke test can add
  adapter evidence later; live FUB/customer access remains deferred.
- Keep controlled call-state integration separate from proof of real LiveKit/SIP
  audio. A simulated provider result is not a full media-path pass. Live calling
  is a separate, explicitly enabled integration lane.
- Use small synthetic books and comfortable relative due times for routine
  journeys. Browser-only clock changes cannot control Rust/PostgreSQL time.
  Exact time-boundary testing needs an agreed whole-stack clock strategy.
  D-050 performance/25,000-Person gates remain separate from functional E2E.

No shared application contract change is proposed by this catalog. Concrete
test-support seams, dependency additions, runner lifecycle and any shared-contract
changes belong to the next harness design.

## Recommended implementation order

1. **First vertical proof: 02a–02c plus realtime recovery and foreign-tenant denial.**
   This covers the central product chain with a small fixture and no external
   provider. Add 02d for a real lost-response retry.
2. **Daily work: 06a–06c, 05a and 07a.** These exercise cross-screen persistence,
   permissions, overlapping Today reasons and dynamic membership.
3. **Access and organization controls: 01, remaining 05/07.** Add lifecycle
   revocation, private lists, catalog invalidation and feed administration.
4. **Assisted/inbound work: 08, 03 and 04.** Keep external input deterministic
   while proving the full application path.
5. **Migration: 10b first, then 10a/10c/10d as separate subfamilies.** Reuse
   existing synthetic/browser evidence, but preserve standalone scenario setup.
   Include the review hold early; it is a security boundary, not final polish.
6. **Telephony: 09 in its separate integration lane.** Define which tests prove
   call state and which prove real media before reporting coverage.

This order is a proposed delivery sequence, not a decision to defer security
checks from any family. Every implemented journey includes its relevant boundaries.

## Explicitly outside the current journey catalog

Native iOS/Android offline behavior requires native automation, not Playwright
Web evidence. Future migration activation/cutover, SMS and outbound email,
inbound/multi-device calling, recording/transcription, chat, Deals, calendar
sync, autonomous AI outreach and Person erasure are not current Web journeys
to invent. Add them when their owning specifications and implementations exist.

## Review and validation of this catalog

This pass inspected source/specifications; it did not execute application tests,
access live FUB, place calls, or change application code. The companion HTML is
a review aid generated from the same journey entries. Existing residual risks
in PROJECT_STATE remain unchanged.

The user ended the visual review with “good”; no revisions were requested.
This records review of the grouping, not a new shared-contract or product-policy decision.
