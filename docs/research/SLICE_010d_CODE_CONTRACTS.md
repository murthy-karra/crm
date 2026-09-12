# Slice 010d — Capture seams and future native-history contracts

Inspected 2026-09-12 at main
`028d6133e1b7c3f81642275e030f63e98b2cca49`. Planning evidence only: no new
policy, source access, tests, database activity or runtime changes are implied.
This file owns destination-code discovery; the 010d specification and execution
brief must separately freeze any proposed contracts before implementation.

The user accepted the execution split while this discovery was underway:
**010d1 is retained historical capture and coverage; 010d2 is later timeline
interpretation.** Section 2 records the immediate capture seams. Sections 3–8
are destination evidence and decisions for 010d2, not 010d1 implementation scope.
010d1 creates no native Inquiries, calls, contact attempts, correspondence,
history facts, timeline/read-model changes, Person activity maxima or Today
behavior. Coverage describes source observations, not interpreted CRM events.

Authority read: AGENTS; DECISION_LOG D-006–015, D-022, D-027, D-031–033,
D-042, D-050, D-052–054 and D-059–068; the architecture baseline; relevant
002, 006c, 007d/009 and 010b/c/f1/f2 contracts. The architecture baseline is a
derived map, not an accepted ADR that overrides decisions. Its 010f2 deployment
paragraph is stale at this baseline; use D-068's follow-up and the release record
for delivery status. Source references below were inspected at this SHA; line
numbers are retrieval aids, not proposed implementation locations.

## 1. Findings that determine scope

1. **There is no retained historical source collection to import yet.**
   [`snapshot_source.rs:26`](../../backend/crates/crm-app/src/domain/migration/snapshot_source.rs)
   defines only users, stages, custom fields, People, notes and tasks;
   its `Stream` at line 62 adds note detail and the two task partitions, but no
   inquiry/event, call, text or email stream. D-061 and
   [010b §3/§5](../specs/SLICE_010b.md) explicitly leave those families
   `not_captured`/`embedded_only`. Person creation, source labels and
   `lastCommunication` summaries cannot stand in for missing records. A new
   separately confirmed source run/extension is necessary; do not append to the
   completed core snapshot or change the capture boundary used by 010c/f1/f2.
2. **Existing live commands are unsuitable historical import commands.**
   `ReceiveInquiry` identifies and may create/assign a Person; `StartCall`
   creates a provider room; correspondence capture infers direction and writes
   response facts. None is a harmless persistence adapter. Reuse validators,
   envelope and bounded transaction patterns behind new private typed commands.
3. **A write to an existing native activity table can change Today without
   publishing anything.** D-052 triggers update Person maxima in the same
   transaction. The review hold prevents viewing/acting on Today, but does not
   mean the underlying data has no future ranking effect. These trigger-driven
   Person UPDATEs also require a narrowly approved migration permit.
4. **The current review core is not a bounded history reader.** 010f2 bounds
   note/task pages, while core detail still fetches all Inquiries and other
   history. Historical density requires its own paged history contract and
   compatibility boundary before the first historical write.
5. **Email content is a separate storage dependency.** D-062 requires future
   email bodies/raw messages/embedded attachments outside PostgreSQL, with a
   separately specified relocation and durable-write/read/recovery contract.
   Existing `correspondence_raw` BYTEA is an explicitly grandfathered system,
   not permission to build a new email importer on the same placement.

## 2. Immediate 010d1 capture seams

Public-source qualification is owned by the companion source-discovery work;
this section describes existing local extension points, not endpoint approval.

| Verified seam | 010d1 consequence |
|---|---|
| [`snapshot.rs:9`](../../backend/crates/crm-app/src/domain/migration/snapshot.rs) freezes `PROFILE=fub-core-v1`; proposal at line 146 stores connection revision/account, schema version and byte budgets and declares exactly six families. | Introduce a separately named, explicitly confirmed history capture profile/run and coverage contract. Do not relabel the completed core run or reuse its confirmation. Bind same Org/account and the completed People parent as source context without expanding that parent's frozen capture sequence. |
| [`20260917000001:2`](../../backend/crates/crm-api/migrations/20260917000001_fub_core_snapshot.sql) provides the Org ledger; run at line 9, streams at 34, encrypted capture at 43 and encrypted record projection at 55 retain independent provenance/checkpoints. | These are reusable persistence patterns, not proof that existing code supports arbitrary profiles. Specify history run ownership, raw references, lossless identity rules and new per-Person/stream fan-out checkpoints where needed. Exact source records stay encrypted and erasable; coverage output remains bounded metadata. |
| [`snapshot_worker.rs:104`](../../backend/crates/crm-app/src/domain/migration/snapshot_worker.rs) claims every runnable `migration_snapshot` without filtering profile. It then decodes the core stream/request and invokes the core parser at line 291. The one-active-run index at migration line 30 is per Org across that table. | A new profile in this table is unsafe until claim/dispatch and admission are explicitly profile-aware. Choose profile-aware reuse with old-artifact retirement, or separately owned history job tables sharing accounting. Do not let an old core worker claim a new history run, or silently change whether concurrent source runs are allowed. |
| [`reader.rs:142`](../../backend/crates/crm-app/src/domain/migration/reader.rs) exposes the typed `FubReader::snapshot` request; line 16 owns the shared source-read permit; the HTTP reader at 185 refuses redirects and bounds request time. [`snapshot_source.rs:14`](../../backend/crates/crm-app/src/domain/migration/snapshot_source.rs) freezes body/parser/projection limits. | Add approved GET request variants and bounded response parsing explicitly. Preserve destination-host/continuation validation, identity rechecks, credential revision, source budget and denied/unavailable/retry classifications. A new source family must not acquire arbitrary URL or write access. |
| [`snapshot.rs:584`](../../backend/crates/crm-app/src/domain/migration/snapshot.rs) locks run plus Org ledger before reserve; line 558 consumes an owned reservation and charges actual retained bytes to both. [`snapshot_worker.rs:304`](../../backend/crates/crm-app/src/domain/migration/snapshot_worker.rs) rechecks lease/state, connection and admin before settlement; lines 500–503 charge raw length and validate reservation expiry. | Freeze which history-run ledger owns every raw/projection/cursor/coverage byte. Preserve exact reservation settlement and shared Org admission alongside 010c/f1/f2. A new run has its own ceiling; it cannot charge its bytes to or release reservations belonging to the completed core run or its children. External content placement, if needed, requires an equally explicit durable-write/accounting/recovery contract. |
| [`snapshot.rs:354`](../../backend/crates/crm-app/src/domain/migration/snapshot.rs) returns decimal byte/count values, profile/state/actions and stream coverage; detail/list at 437/451 are existing admin capture read seams. [`snapshot_preview.rs:524`](../../backend/crates/crm-app/src/domain/migration/snapshot_preview.rs) is a core interpretation preview, not a generic history coverage reader. | Add bounded capture/coverage DTOs and routes under their own profile semantics. Do not offer the People/core reconciliation preview for a history profile. Define counts as observed/accepted/unsupported/inaccessible/truncated and preserve unknown completeness, with paged details if necessary. No Person timeline/read-model extension belongs here. |
| [`crm-api/src/lib.rs:273`](../../backend/crates/crm-api/src/lib.rs) starts the snapshot worker in the API runtime. [`snapshot_worker.rs:24`](../../backend/crates/crm-app/src/domain/migration/snapshot_worker.rs) interleaves bounded source and preview work. [`migration-release-preflight:48`](../../scripts/migration-release-preflight) currently recognizes only workspace, metadata and activity capabilities. | Keep one modular runtime and bounded work; no new service is implied. Declare history-capture compatibility separately if sharing claimable storage or runtime dispatch. A capture capability proves capture safety only; it must not satisfy future history-native/read capability requirements. Existing review hold remains unchanged. |

010d1 amendments are therefore source profile and proposal/confirmation,
retention/coverage and privacy contracts, bounded admin capture reads, exact
ledger ownership, worker dispatch/admission and release compatibility. The
native destination options below are deliberately deferred to 010d2. D-062
still constrains any newly captured email bodies/raw messages even when the
native timeline is deferred; a capture-only label does not exempt storage.

## 3. Inquiry, Person attribution and intake — 010d2 evidence

| Current source | Verified invariant | Consequence for historical imports |
|---|---|---|
| [`010c §4:206`](../specs/SLICE_010c.md) | Imported Person provenance retains original labels/URLs/dates encrypted. Local `person.created_at` is import time; activity maxima start null. Person import creates zero Inquiries. | Preserve original Person attribution and parent results. A source event must independently qualify as an Inquiry; do not infer one from Person creation/source fields. |
| [`inquiry/queries.rs:10`](../../backend/crates/crm-app/src/domain/inquiry/queries.rs) | `NewInquiry` requires Org, Person, `RawPayloadId`, source token, optional source external ID/message, and `received_at`; insert generates the Inquiry UUID. | A migration insertion needs deterministic destination identity and a truthful raw-evidence reference. Casting a migration capture UUID into `RawPayloadId` merely because both are UUIDs misstates ownership. Explicitly own an additive content-reference seam or keep external inquiry facts distinct. |
| [`inquiry/parse.rs:10`](../../backend/crates/crm-app/src/domain/inquiry/parse.rs) | Current message preview limit is 4,096 bytes; `Source` is trimmed/lowercased ASCII `[a-z0-9_]{1,64}`. | Do not silently truncate historical content or squash arbitrary source labels into this token vocabulary. Retain full source values separately; decide source classification/filter semantics explicitly. |
| [`receive_inquiry.rs:465`](../../backend/crates/crm-app/src/domain/commands/receive_inquiry.rs) | Intake identifies by contact, creates or selects Person, applies routing, upserts contacts, inserts Inquiry and receipt/routing/stage/assignment facts, marks raw resolved and publishes (lines 465–647). | Do not call `ReceiveInquiry` or `complete_intake` for existing imported People. Historical import must resolve only the completed parent's source identity/result/live Person, with zero routing/contact/assignment changes. |
| [`facts.rs:18`](../../backend/crates/crm-app/src/domain/facts.rs) | `InquiryReceivedFact` carries Inquiry/Person/raw IDs, HMAC, source, `person_created` and `matched_by`, using a supplied envelope. | Low-level insertion can be reused only if each field's historical meaning is specified. No fake contact match, local Person creation or source receipt event. |
| [`20260911000001:71`](../../backend/crates/crm-api/migrations/20260911000001_inquiry_append_only.sql) | Inquiry rejects UPDATE/direct DELETE/TRUNCATE; actual Person cascade may erase it. History facts remain append-only with bare erasable-resource IDs. | Equal-source replay needs separate durable source identity/results. An existing conflicting Inquiry cannot be updated in place, and deletion must not authorize resurrection. |
| [`inquiry/queries.rs:86`](../../backend/crates/crm-app/src/domain/inquiry/queries.rs) and [`person/queries.rs:1039`](../../backend/crates/crm-app/src/domain/person/queries.rs) | Inquiry summaries and Inquiry history fetch all rows for one Person. Source dropdown/filter semantics derive from native Inquiry rows. | Native Inquiry import is a visible count/source/filter change as well as storage. Page both read surfaces before importing large historical sets. |

`contact::identify` intentionally picks the earliest matching Person across
overlapping normalized contacts (`contact.rs:132`); D-064 intentionally preserved
separate People. That intake resolver is therefore specifically excluded.

## 4. Calls, contact attempts and outcomes — 010d2 evidence

The current [`call` schema:6](../../backend/crates/crm-api/migrations/20260825000001_calls.sql)
requires `contact_method_id`, `caller_user_id`, provider, provider room,
placement/status and lifecycle timestamps. Its terminal states encode the local
LiveKit workflow. The matching `call_completed` fact at line 40 requires a local
call ID, contact-method ID, explicit closed outcome and ended time; answered time
and talk duration may be null. These are not generic vendor call-log rows.

[`StartCall:77`](../../backend/crates/crm-app/src/domain/commands/start_call.rs)
reserves a real call and [`StartCall:154`](../../backend/crates/crm-app/src/domain/commands/start_call.rs)
creates the provider room. [`telephony/settle.rs:41`](../../backend/crates/crm-app/src/domain/telephony/settle.rs)
applies live signals and creates automatic contact attempts/completed facts.
Neither path belongs in historical ingestion, even with `Origin::Migration`.

[`LogContactAttempt:105`](../../backend/crates/crm-app/src/domain/commands/log_contact_attempt.rs)
accepts Person/channel/outcome, supplies current time/current actor, is
deliberately non-idempotent and publishes after commit. The lower-level
[`ContactAttemptedFact:166`](../../backend/crates/crm-app/src/domain/facts.rs)
supports the standard envelope, optional correction and recorded time; its closed
channels/outcomes are useful only when source evidence proves those semantics.
An inbound message is not an outbound `sent` attempt; a duration or recording URL
does not prove an answered/reached call. An unknown outcome cannot be replaced
with `no_answer`, `reached`, `sent` or the importing admin's judgment.

D-033's unfinished-outcome workflow is specifically a local call obligation.
[`correct_call_outcome.rs:204`](../../backend/crates/crm-app/src/domain/commands/correct_call_outcome.rs)
requires the actual caller, a terminal `call`, and its head contact attempt.
[`call_only.sql:26`](../../backend/crates/crm-app/src/domain/today/system_feeds/sql/call_only.sql)
selects caller-owned terminal calls with an uncorrected root. Inventing a local
call/root to render an external record would create an artificial “set outcome”
obligation. Historical source outcomes must be labeled as source evidence and
must not mint synthetic correction chains to suppress that obligation.

There is no native persisted SMS-conversation model in the inspected domain
module/command registrations. `ContactChannel::Text` represents a classified
attempt, not a text body, recipient set, inbound message or delivery receipt.

## 5. Correspondence and content — 010d2 evidence

[`correspondence_captured:79`](../../backend/crates/crm-api/migrations/20260904000001_correspondence_capture.sql)
requires a CRM `agent_user_id`, inbound/outbound direction, `via=cc|forward`, an
actual `correspondence_raw_id` FK and `backdated`. It also stores normalized
Message-ID/thread identifiers with per-Person Message-ID deduplication. These
fields describe the token-attributed CC/forward pipeline, not FUB history.

[`capture/pipeline.rs:216`](../../backend/crates/crm-app/src/domain/capture/pipeline.rs)
forces `Origin::Webhook`, System-on-behalf-of-agent, and automatically adds an
`email/sent` attempt for an outbound row. Its parser at line 80 clamps message
time to the receipt window and may fall back to receipt time. Importing through
it would change historical time/origin/attribution and infer an operational
attempt. Do not wrap vendor JSON in fabricated MIME or manufacture `via=cc`.

The current [`correspondence_history:1435`](../../backend/crates/crm-app/src/domain/person/queries.rs)
fails closed if the attributed CRM user cannot be rendered. Unknown/deleted
source users therefore cannot be represented by an absent actor in this old
shape, nor substituted with the importer. Its DTO deliberately exposes only
direction, agent, capture time, via and backdated; D-042 prohibits readable
subjects/addresses/Message-ID/body there. Extending migration review does not
silently authorize a new ordinary correspondence-body surface.

[`correspondence_raw:55`](../../backend/crates/crm-api/migrations/20260904000001_correspondence_capture.sql)
stores encrypted whole messages in PostgreSQL with Org/HMAC deduplication and
no public read endpoint. Its fact FK blocks simple deletion of that raw row,
which is a lifecycle concern for the future relocation/erasure design. D-062
explicitly leaves that old implementation until a separately specified step;
do not use this debt as a new importer template. O-012 also keeps call
summaries/transcripts/recordings behind unresolved key/content prerequisites.

For a first history rung, prefer metadata plus honest content-availability
states. Exact fetched source payloads still require encrypted, deletable
preservation; “metadata only” cannot justify discarding bodies returned by a
source response. If a proposed email endpoint unavoidably returns full content,
its capture must wait for the D-062 placement contract or use an independently
qualified metadata-only request. Do not fetch attachment/recording/source URLs.

## 6. Read models, workspace permits and compatibility — 010d2 only

[`20260910000001:35`](../../backend/crates/crm-api/migrations/20260910000001_person_last_activity.sql)
defines the transactionally maintained maxima: Inquiry→`last_inquiry_at`, contact
attempt→`last_contact_at`, correspondence direction→`last_inbound_at` or
`last_outbound_at`. D-052's invariant is equality with canonical maxima, not a
hint an importer may hand-set. Backdated inserts advance only when newer than
the current maximum. A future erasure/correction policy must include rebuilds.

[`person_state.sql:200`](../../backend/crates/crm-app/src/domain/today/system_feeds/sql/person_state.sql)
tests unanswered inquiry timing against contact maxima; line 207 requires a
native Inquiry. Reply eligibility compares inbound against both contact and
outbound maxima. Saved lists can select People without Inquiry and use these
same activity fields. Never create an Inquiry merely to make an imported call,
text or reply appear on Today. [`today/mod.rs:303`](../../backend/crates/crm-app/src/domain/today/mod.rs)
still requires operational workspace; this gate must continue for every role.

The latest [`workspace mutation guard:227`](../../backend/crates/crm-api/migrations/20260920000001_fub_activity_import.sql)
permits only existing People/metadata/activity units and constrained terminal
call cleanup under review. New history INSERTs and any trigger-produced Person
maxima UPDATE require an exact new permit. If native projection is approved,
authorize only the expected derived columns/values in that unit; never grant
general Person UPDATE or let `origin=migration` bypass the hold.

[`activity_review.rs:153`](../../backend/crates/crm-app/src/domain/migration/activity_review.rs)
assembles core detail from unbounded Inquiry and `core_history` queries before
its 512-KiB serialized-size check. [`core_history:1582`](../../backend/crates/crm-app/src/domain/person/queries.rs)
includes import, inquiry, routing, assignment, stage, attempt, call and
correspondence facts. A size rejection after loading them is not sufficient
history paging. Existing native cursors bind only the activity-child revision
(`activity_review.rs:147`); history commits would otherwise be invisible to
those cursors.

Introduce a distinct history-aware review core/page DTO with explicit scope,
decimal counts, stable keyset order, bounded per-item metadata and byte bounds,
on-demand authorized content/provenance, and a history revision. Preserve 010f2
note/task paging. A combined timeline needs deterministic cross-kind keysets and
must not expose a falsely complete first-N array. Separate paged cards are the
smaller initial contract if cross-family sorting is not required.

The first confirmed history child must take the exclusive workspace barrier and
durably require history-capable readers, independently of whether a notes/tasks
child exists. The old [`crm_activity_complete_read:188`](../../backend/crates/crm-api/migrations/20260920000001_fub_activity_import.sql)
checks only confirmed activity children. Extend/freeze release/startup/preflight
bound-state detection and retain the new boundary after completion or zero-write
cancellation. Older clients should receive an explicit refresh/upgrade error;
old 010f2 core readers must not fetch the new bulk history unboundedly.

## 7. Destination choices for the later 010d2 specification

These are proposals requiring approval, not a discovered decision.

| Option | Scope and cost | Assessment |
|---|---|---|
| Expand existing native Inquiry/call/correspondence models | Add source identity and content references; make previously mandatory historical actors/lifecycle fields optional or tagged; amend every reader, outcome workflow, trigger and Today rule. | Broadest compatibility change. Reusing live call/capture representations before defining their new meaning risks fabricated facts. |
| Separate, typed external-history facts and bounded review | Explicit imported inquiry/call/text/correspondence fact types preserve source-qualified meaning and nullable unknown attributes; separate encrypted erasable provenance/content; no synthetic live aggregates or automatic canonical response projection. | **Recommended first scope.** This is a small domain inside the existing Rust application, not a service or generic JSON event store. It preserves source history while keeping operational effects a visible later decision. |
| Capture/preview only, then native-history import in a subsequent step | Complete source/semantics/content qualification first; no business facts in this first step. | Prefer this staged execution if public evidence cannot freeze fidelity/availability rules. It is capture completion, not completed historical import or cutover. |

For the recommended option, use ordinary D-015 envelope/append-only disciplines
and distinct closed fact types such as `ImportedInquiryReceived`,
`ImportedCallRecorded` and `ImportedTextRecorded`; email correspondence joins
only once its source and D-062 prerequisites are satisfied. Do not create a
generic `event_type + unrestricted payload JSON` store. Keep source labels,
numbers, recipient addresses, bodies, subjects and transcripts out of immutable
facts. Facts carry local IDs, exact qualified timestamps, closed semantic codes,
integrity references and source-identity references. Unknown outcome/direction or
time must be represented explicitly or held under a reviewed policy.

The standard envelope distinguishes the import action from the historical
source actor: [`FactEnvelope:164`](../../backend/crates/crm-app/src/domain/envelope.rs)
supports System/Migration and on-behalf-of executor. Preserve the source actor
separately, optionally explicitly linked to an existing same-Org active/inactive
member, with no invented account or edit authority. `occurred_at` is the qualified
source instant; `recorded_at` is local commit time. A source correction is not
the import operation's retry, and a changed duplicate is not last-wins evidence.

Preserve identity `(Org, source account, entity kind, vendor ID)` and immutable
parent Person identity/result binding, one record per atomic unit, explicit
held/source-only reconciliation, account-safe equality, no overwrite and
content-free tombstones. Reuse 010f2's retained validation, private manifests,
receipts, worker fencing, reservation ownership and byte-accounting patterns;
do not inherit its completed core-source boundary as proof of historical capture.

## 8. Later 010d2 decisions and owned amendments still needed

- **Operational meaning:** are imported facts review-only history, or do qualified
  facts immediately populate native Inquiry/contact/correspondence truth and
  maxima? Recommend review-only first, with an explicit 010e projection/Today
  decision before activation. Display historical Inquiry counts separately from
  native counts if those native rows are not written; do not claim the native
  Inquiry model has been populated. Import is not operational activation.
- **Qualification policy:** which source event types prove a genuine inquiry,
  call, sent text, received text or email? What remains unknown/held? Do not infer
  an outcome from duration, an author from a label, or an exact time from a date.
  Preserve original source labels independently of latest-native-source filters.
- **Content:** metadata-only first is the bounded recommendation. Any readable
  SMS/email body or subject and any new bulk-email capture require explicit
  exposure/placement/lifecycle contracts; summaries/transcripts/recordings keep
  their existing O-012/O-002 gates. Hidden/inaccessible source content stays a
  counted gap, never “preserved.”
- **Source lifetime:** independently confirmed same-account history source with
  its own window, profile/version, cancellation/retry authority and storage
  admissions. Binding to the completed People parent must not imply contemporaneous
  source snapshots; source-only People absent from that parent stay unresolved.
- **Shared contracts to amend:** 010b source profiles/coverage, 010c parent and
  review-reader contract, 010f1/f2 sibling accounting/compatibility pointers,
  002 timeline/Inquiry source/count semantics if affected, 003/006c/009/012 Today
  and history semantics if native projection is chosen, release preflight and
  capability types, and Web review DTO/query keys. Ordinary live commands stay
  unchanged unless a separately declared contract explicitly owns their change.
- **Erasure inventory:** include every new raw/source observation, mapping,
  manifest, encrypted body/subject/attribution, content-object reference and key,
  result/receipt, native fact/read model, cursor/cache and backup. Shared raw pages
  and multi-Person messages need actual Person linkage and deletion accounting.
  Content-free source identities must survive to prevent reimport resurrection.
  This extends [readiness C1/C2](../plans/PRODUCTION_READINESS.md), not completion
  of those gates or a new retention-policy decision.

Required later verification should be driven by the approved scope: source
fidelity/variant holds, genuine parent identity and foreign-Org negatives,
immutable native equality/tombstones, no routing/provider/outbound/notification
side effects, exact maxima policy, partial cancellation and sibling ledgers,
bounded history traversal/cursor invalidation, old-reader/confirm races and
activity-independent release gating. Use D-050's existing envelope and two
review rounds; add only the paired reader checks for readers actually changed.
No such implementation or execution evidence is claimed by this discovery file.
