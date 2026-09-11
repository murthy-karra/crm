# Slice 010c — Source and destination code findings

Inspected 2026-09-11 at main `c6c5930`, after the deployed 010b implementation
`89471f0`. This is evidence for the [draft spec](../specs/SLICE_010c.md), not
approval or live-source qualification. Three read-only consultations covered
source extraction, destination commands and the review-mode boundary. No test,
database, source-account or runtime operation was performed during planning.

## 1. Retained source evidence

- The [010b schema](../../backend/crates/crm-api/migrations/20260917000001_fub_core_snapshot.sql)
  stores capture sequence, representation, status/classification, accepted and
  truncated flags, encrypted raw bytes, and record ordinal/ID/semantic HMAC.
  [snapshot_source.rs](../../backend/crates/crm-app/src/domain/migration/snapshot_source.rs)
  has the duplicate-key-rejecting lossless parser, exact numeric identity and
  bounded projection. Projections clip or omit fields; they are unsuitable as
  import inputs even when a preview says reviewable.
- [snapshot_worker.rs](../../backend/crates/crm-app/src/domain/migration/snapshot_worker.rs)
  retains records from rejected pages too. The current
  [preview](../../backend/crates/crm-app/src/domain/migration/snapshot_preview.rs)
  does not require every displayed observation's capture to be accepted. Import
  therefore joins exact captures and rechecks acceptance, status, truncation,
  representation, item ordinal, exact ID and semantic HMAC against decrypted raw.
- Preview exposes only two example projections for a semantic conflict; the
  record set may contain more variants. An importer must consider every relevant
  observation, not treat those two examples as the complete set.
- A completed preview can describe an incomplete/paused source run. Its frozen
  destination input includes stages, members and definitions, but only a Boolean
  for People presence. It does not provide a destination write lock or executable
  mapping approval. Import needs its own immutable plan and atomic empty check.
- [snapshot_compare.rs](../../backend/crates/crm-app/src/domain/migration/snapshot_compare.rs)
  suggests stage matches and source-user/member email matches. These are advisory;
  pond and ambiguous mappings still need a disposition.
- [connection commands](../../backend/crates/crm-app/src/domain/migration/commands.rs)
  permit credential replacement for the same FUB account and increment revision.
  Source identity must include Organization/account/family/source ID, not credential
  revision. Retained-data reads are available to current admins after disconnect;
  a DB-only import must not inherit the old source worker's credential authority.
- Existing byte reservations distinguish source (`preview_id` null) and preview.
  Source cancellation releases source-owned reservations. Import needs its own
  ownership and cancellation scope while sharing ledger accounting and lock order.
- The same raw qualification must cover stage/user supporting records. Native
  string feasibility and per-item retained-byte bounds must be established before
  a plan can become ready, not discovered after irreversible confirmation.

Public documentation read during planning: [People GET](https://docs.followupboss.com/reference/people-get)
documents optional fields, Trash and access-dependent selection; this is not proof
of complete account visibility. [People POST](https://docs.followupboss.com/reference/people-post)
describes Person creation separately from event/lead intake. No FUB POST is part
of this project operation. The existing saved public-schema evidence is indexed
by [010b's hash manifest](SLICE_010b_SOURCE_SCHEMA_SHA256.json). Its People example
uses `emails[].isPrimary: 1`; exact recognition is proposed in 010c and must have
synthetic fixtures. Documentation examples, including ellipses, are not verified
live-response fixtures. No new source endpoint, API rate or flag meaning is assumed.

## 2. Destination fields, identity and facts

| Existing code contract | Consequence for 010c |
|---|---|
| [Person/contact schema](../../backend/crates/crm-api/migrations/20260821000002_person_contact_method.sql): names/assignee nullable; required same-Org stage; contact uniqueness only within Person/kind/normalized value | Named contactless People are representable. Household contacts can be duplicated across separate People without a schema uniqueness change. |
| [ReceiveInquiry](../../backend/crates/crm-app/src/domain/commands/receive_inquiry.rs) and [parser](../../backend/crates/crm-app/src/domain/inquiry/parse.rs) require a usable contact and create Inquiry/routing facts | Do not reuse this command for import. A FUB Person creation date is not evidence of a new Inquiry. |
| [contact.rs](../../backend/crates/crm-app/src/domain/contact.rs) normalizes email by trim/lowercase with a minimal `@` check; phone is a permissive digit-based normalization | Reuse these exact current semantics. A normalizable contact is not proof of source validity or permission to communicate. `identify` chooses among overlaps and is excluded from import. |
| [person/queries.rs](../../backend/crates/crm-app/src/domain/person/queries.rs) accepts a bare Person, but its current intake upsert takes only one email/phone | Dedicated typed import must persist all contacts, deterministic representative/order and transformations atomically. |
| Primary-contact subqueries order by `created_at` alone, while transaction timestamps tie | Importing several contacts makes the displayed primary arbitrary. Add explicit nullable order plus stable ties across every People/Today/Operator reader; this triggers D-050's paired regression check. |
| [filtered_summaries.sql](../../backend/crates/crm-app/src/domain/person/sql/filtered_summaries.sql) and Person queries derive source filtering from latest Inquiry | Store imported source attribution separately. Do not silently reinterpret filters or manufacture an Inquiry. |
| [stage schema](../../backend/crates/crm-api/migrations/20260821000001_stage.sql) has unique Org/name and Org/SMALLINT-position, and the application role has SELECT only | Explicit import-only stage creation needs a typed command, INSERT grant, append ordering and collision/capacity checks. No current public stage CRUD can be assumed. |
| [manual assignment](../../backend/crates/crm-app/src/domain/commands/assign_person.rs) checks membership existence without filtering active status | Import uses an explicit active-member check. The pre-existing manual-assignment residual is not silently repaired by this slice. |
| [envelope.rs](../../backend/crates/crm-app/src/domain/envelope.rs) already supports Migration/System/on-behalf-of; [facts.rs](../../backend/crates/crm-app/src/domain/facts.rs) has no migration initial-state reason | Add import fact/reasons explicitly, recorded at local commit time, with IDs rather than customer strings. No backdated source actor attribution. |
| [Today source candidates](../../backend/crates/crm-app/src/domain/today/source_candidates.sql) can select People without Inquiry/activity | Review mode must block Today, including custom sources. Null activity alone is not a safe operational hold. |

Contact normalized values and stage names are stored in full-value B-tree keys.
PostgreSQL documents an [index-entry size limit](https://www.postgresql.org/docs/current/btree.html),
while the current normalizers/010b can retain keys longer than destination indexes
can safely admit. The draft's conservative 2,048-byte indexed-value limit and
native NUL checks are import eligibility proposals, not global validator changes.
Unsupported values remain preserved and held before confirmation.

## 3. Review-mode inventory and entry check

Organization currently has an active/suspended administrative status; mode is
separate. [AuthContext](../../backend/crates/crm-app/src/auth/context.rs) and
[session identity](../../backend/crates/crm-api/src/auth/session.rs) provide
request-time actor/Org/role, not an authoritative workspace read/write permit.
The shared [SessionResponse](../../backend/crates/crm-api/src/routes/session.rs)
serves login, `/me` and invitation acceptance. All must carry the additive mode.

At confirmation, count with indexed `EXISTS` checks, under the exclusive guard:

- Block any `person`, `contact_method`, `inquiry`, `inquiry_received`,
  `routing_decision`, `assignment_changed`, `stage_changed`, `contact_attempted`,
  `person_tag`, `person_custom_field_value`, `note` or `task` row in the Org.
- Block `call`, `call_completed`, `raw_payload`, `intake_extraction`,
  `correspondence_raw`, `correspondence_captured` and `capture_message`, including
  unresolved/terminal/tombstoned rows. Nonterminal calls explicitly prevent entry.
- Allow terminal IDs-only `operator_turn`/`operator_tool_call` audit. Block active
  Operator admissions, unexpired `proposed` proposals and all `claimed` work;
  terminal/expired IDs-only proposals do not themselves make an Org nonempty.
  Independently block customer-content `operator_task_proposal` rows and the
  resulting business tables. No append-only audit is erased to establish emptiness.
- Block a different existing import/workspace binding. Allow only the proposed
  run's planning/evidence records until its first confirmation.
- Allow Organization/identity/session/membership/invitation and their governance
  facts, seeded stages, `tag`, `custom_field`, `custom_field_option`, saved-list
  and Today configuration/history, capture/intake addresses/tokens/rotation
  settings, and 010a/010b control/evidence/storage rows. They do not establish
  an imported customer book. Settings are allowed at entry but their ordinary
  editing is held during review as specified.

Check children through their tenant-owned parents when no direct Org column
exists. This list is derived from every current migration. Before implementation,
reconcile it with the actual head and classify any newly added table explicitly;
do not default unknown business state to allowed. Inventory corrections that
change policy require the spec's shared-contract procedure.

### Write and side-effect entry points

Paths below are relative to `backend/crates/crm-app/src` unless marked API.
The brief requires a checked test matrix against this concrete inventory.

| Area | Entry points / required enforcement |
|---|---|
| Person/history | `domain/commands/{assign_person,change_person_stage,log_contact_attempt,correct_call_outcome}.rs`; shared guard before Person/domain locks |
| Notes/tasks/tags/fields | `domain/{note,task,tag,custom_field}/commands.rs`; guard all mutation variants, including archive/restore/Undo |
| Lists/Today | `domain/saved_list/commands.rs`, `domain/today/sources.rs`, `domain/today/system_feeds/commands.rs`; no configuration bypass |
| Intake | `domain/commands/receive_inquiry.rs`, `domain/intake/{receive,workbench,rotate}.rs`, `domain/raw_payload/store.rs`; guard raw Phase A and business Phase B, retries and discard |
| Correspondence | `domain/capture/{receive,pipeline,store,commands,address}.rs`; guard capture persistence, manual matching and explicit provisioning |
| Routing settings | API `routes/organization.rs::update_intake_settings` currently autocommits through admin queries; convert to a guarded typed transaction |
| Address GET | API `routes/capture.rs` may mint an address on GET; explicitly guard this path rather than assuming GET is read-only |
| Extraction | `domain/intake/extraction/worker.rs`; discover Org without row lock, acquire workspace guard, then claim/recheck. Skip held work before inference/application without consuming attempts |
| Calls | `domain/commands/{start_call,dial_call}.rs`, `domain/telephony/dial_task.rs`; guard durable admission before provider I/O; no DB transaction across network |
| Operator | API `routes/operator.rs`, `operator/backend.rs`, `operator/mod.rs`; durable admission before spawn/inference, read permits, guarded proposal creation/confirmation/tool commands |

Namespaced shared/exclusive advisory transaction locks are a suitable concrete
mechanism. Lock order is workspace first, then existing membership/raw/Person/
run/domain locks. `ReceiveInquiry` and extraction currently take raw locks first;
adjusting only their HTTP handlers is insufficient. Protected member reads need
the same short shared permit until their complete data is loaded, so stale
authentication cannot race review entry. Size the guard/query pool usage and
exercise timeout/contention in tests; no permit may span remote I/O.

### Inbound and terminal exceptions

[Inbound email](../../backend/crates/crm-api/src/routes/inbound_email.rs) normally
returns accepted/rejected 200 envelopes for several outcomes. The
[relay](../../infra/email-worker/worker.js) treats all 2xx as finished, bounces
400/413, and throws for other statuses. Review-mode ingress therefore returns
503 before persistence; it does not pretend to own a durable replay queue.

Calls already reserve a durable `placing` row before external room creation.
The one-time empty-Org transition can reject admitted calls. Keep authenticated
owned hangup, signed terminal webhooks, expiry/failure reconciliation and no-op
terminal settlement allowed. Review these in
[hangup](../../backend/crates/crm-app/src/domain/commands/hangup_call.rs),
[webhooks](../../backend/crates/crm-api/src/routes/livekit_webhook.rs),
[sweep](../../backend/crates/crm-app/src/domain/telephony/sweep.rs) and
[settle](../../backend/crates/crm-app/src/domain/telephony/settle.rs).
Do not label dialing or ordinary outcome correction as cleanup.

The [Operator ledger](../../backend/crates/crm-api/migrations/20260824000001_operator_ledger.sql)
requires terminal fields and forbids update/delete. It cannot represent pending
admission without changing immutable-history semantics. A separate short-lived,
IDs-only admission tied to an enforced absolute turn deadline is the bounded
addition. [run_turn](../../backend/crates/crm-operator/src/service.rs) currently
starts a relative timeout when invoked; admission expiry must also cover time
waiting to run. Guard proposal confirmation before its single-use claim. Permit
terminal ledger writes/admission release after a turn ends or expires.

### Read, session and Web entry points

Guard business data queries for ordinary members, not merely routes or cached
AuthContext. Admins retain conventional read-only People/provenance inspection;
Today/Operator remain unavailable to all roles. Govern separate setup reads
explicitly; no platform-admin tenant-data bypass. Audit list/detail/search,
notes/tasks/correspondence, call state, import detail, and Operator tool response
assembly. Direct-domain access must obey the same permit boundary.

[Realtime token issuance](../../backend/crates/crm-api/src/routes/realtime.rs)
must reject held operational use. Existing connections may survive, so import
publishes no operational notifications; authorization still protects refetch.
[AppShell](../../web/src/components/AppShell.vue) owns call behavior independently
of navigation, and [router.ts](../../web/src/router.ts) gates routes. Both need
mode-aware behavior plus session-scoped request/cache invalidation. The member
waiting screen preserves `/me` and logout; it is not a failed-login screen.

## 4. Evidence limits

Pre-010c binaries ignore the proposed workspace guard. The spec therefore owns
a server/worker/CLI compatibility boundary: retire those processes before import
admission; after a review binding exists, only compatible builds may run. The
[010b rollback procedure](../tasks/SLICE_010b_RELEASE.md) predates this state and
cannot be reused unchanged for 010c. Eventual release verification must enforce
the constraint without dropping the binding or restoring away committed data.

This record shows why the draft needs new import commands, raw extraction,
explicit stage/order contracts and a cross-cutting readiness gate. It does not
prove them implemented. Live authorized FUB validation remains user-deferred;
real-data privacy/erasure and recovery gates remain in
[production readiness](../plans/PRODUCTION_READINESS.md). The review workspace
does not satisfy those gates merely by restricting normal agents.
