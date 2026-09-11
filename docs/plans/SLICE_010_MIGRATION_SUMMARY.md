# Slice 010 — FUB migration planning summary

**Status: 010a DEPLOYED AND VERIFIED (2026-09-11); live FUB validation deferred.
010b core-first planning is active; later implementation remains unapproved.** The user
selected **a new, empty CRM Organization first** (D-059), then approved 010a's
API-first assessment and encrypted saved credentials (D-060). Other
recommendations remain proposals. Inspected against main `b2fb368`, after 019b.
No FUB account was connected or customer data fetched.

**Implementation follow-up:** the user approved the next rung, which now
has a [010a specification](../specs/SLICE_010a.md) and
[bounded brief](../tasks/SLICE_010a_IMPL.md). D-060 accepts API-first assessment,
encrypted saved credentials and its additive contracts. Live validation is
explicitly deferred while no FUB test account is available. The first assessment
uses six bounded checks, not full source enumeration. This does not reduce the
later snapshot, preservation or reconciliation requirements.

## 1. Outcome and first milestone

An Organization admin should understand what their FUB account contains,
what can move faithfully, what requires a decision, and what cannot currently
be retrieved or represented. After a preview, the migration should be
resumable, repeatable and reconciled before the team switches.

The first milestone is **010a: connection and bounded access assessment**. It reads
the source and stores migration-control/report information locally; it
creates no CRM People, notes, tasks or other imported business records.
It never modifies FUB. A successful connection alone is not proof of complete
account access or import readiness.

The approved destination is new and empty at the initial import. Normal
Organization setup, default stages and invited members are compatible with
that direction. The import spec must define the precise emptiness check,
prevent accidental import into a populated destination, and distinguish an
idempotent rerun from an unrelated second import. Shared household contact
methods and duplicate source People still need explicit matching rules.

## 2. What changed since the parked plan

Tags, notes, tasks and typed custom fields now have destination models.
Notes and tasks include source-ID uniqueness and tombstones; custom-field
definitions include `source`/`external_key`, with value origin/correlation
metadata. These are useful foundations, not completed import commands.
See [015](../specs/SLICE_015.md), [016](../specs/SLICE_016.md),
[019](../specs/SLICE_019.md), and [019b](../specs/SLICE_019b.md).

| Data family | Current destination and planning treatment |
|---|---|
| People, emails, phones | Core tables exist. Preserve every source identifier and contact method. Flag missing/invalid contacts and ambiguous identity matches; names alone must not merge People. |
| Stages, users, responsibility | Map source stages and users to Organization stages/members in preview. FUB users are not automatically authenticated CRM accounts. Unmatched users and pond/collaborator roles need visible dispositions. |
| Tags and custom fields | Include in the proposed first usable migration. Keep source keys and option identity. Current text/label/number limits and recurring-date semantics can prevent exact representation; preserve and report exceptions instead of truncating/coercing. |
| Notes | Plain-text destination exists, with a 10,000-character limit. Authorship, historical timestamps, formatting, attachments and restricted notes need explicit treatment. |
| Tasks | Destination supports call/email/text/follow-up/other. Source task types, date-only deadlines, timezones, completion history and unmatched assignees need a mapping policy. |
| Inquiry and communication history | Import only facts supported by source evidence. Preserve source attribution and original timestamps separately from import time. A Person creation date is not necessarily an Inquiry; `lastCommunication` alone is not a call or text. |
| Addresses, relationships, appointments, deals | No complete corresponding destination was found in the current model. Inventory separately and retain retrievable source material; do not claim full usable import. A gap may require its own model slice before a customer's cutover. |
| Email bodies, recordings, automation/settings | Assess retrieval and representation independently. D-062 places email bulk content outside PostgreSQL, with scoped metadata and storage references in PostgreSQL; backend selection and relocation are future work. Mailbox reconstruction, recording handling, action plans and full Smart List conversion are separate work, not implied by a contact migration. |

The old survey's automatic synthetic Inquiry/contact-attempt proposals are
not adopted. They need reevaluation under `Person != Inquiry` and factual
history rules. Its notes-blocked-on-O-012 statement was superseded by D-053.

## 3. Current source facts that affect the plan

These are documentation findings, not results from a live FUB account.

- **Identity and coverage:** API keys use Basic authentication over HTTPS
  and inherit the source user's permissions. Owner/admin-level access must
  be established before presenting a whole-account assessment; restricted
  access must be labelled as such. FUB also supports OAuth, so API keys are
  a proposed first connector, not its only authentication mechanism.
  [Authentication](https://docs.followupboss.com/reference/authentication),
  [identity endpoint](https://docs.followupboss.com/reference/identity).
- **Access route:** registered system identification is required for serving
  FUB customers. Published API terms contain competing-product restrictions
  and a customer-own-data provision. Whether our planned migration access is
  permitted must be confirmed before promising an API-only product; this
  summary makes no legal determination. A customer-provided export remains
  a candidate source, with separately disclosed limits.
  [Registration](https://docs.followupboss.com/reference/identification),
  [API terms](https://docs.followupboss.com/reference/fub-api-tou).
- **People coverage:** Trash is excluded by default, and many fields require
  an explicit field selection. Inventory must account for Trash, custom
  fields, relationships and other selected source data, with Trash's import
  disposition decided separately. [People API](https://docs.followupboss.com/reference/people-get).
- **Enumeration:** use documented continuation pagination, bounded pages
  (maximum 100), stable IDs and a persisted checkpoint. Reconcile enumerated
  IDs with reported totals where available; absent totals or an early end
  must produce uncertainty, not invented completeness. A moving source is
  not an atomic snapshot. [Pagination](https://docs.followupboss.com/reference/pagination).
- **Delta capture:** the documented filter is `updatedAfter`; changes to
  related records such as notes do not necessarily advance the Person's
  `updated` value. Track each entity family independently, verify supported
  filters per endpoint, overlap checkpoints with deduplication, and reconcile
  deletions separately. [Common filters](https://docs.followupboss.com/reference/common-filters).
- **Pacing:** respect response rate-limit headers and `Retry-After` on 429.
  Do not turn the old survey's example rate into a fixed allowance. Transient
  failures get bounded backoff and a resumable paused state; credential or
  permission failures require repair rather than infinite retries.
  [Rate limiting](https://docs.followupboss.com/me/reference/rate-limiting).
- **Known source restrictions:** some calls/texts are unavailable through
  the API, and a valid note can return 404 because its originating system or
  content restricts access. Such results cannot automatically mean deletion
  or a zero count. [Calls](https://docs.followupboss.com/reference/calls-get),
  [texts](https://docs.followupboss.com/reference/textmessages-get),
  [notes](https://docs.followupboss.com/reference/notes-id-get).
- **CSV is supplementary evidence:** the all-columns contact export is
  capped at six contact methods per category, four relationships and the
  most recent 50 calls/texts/notes per contact; it does not export emails or
  recordings. It cannot establish full historical coverage by itself.
  [Export documentation](https://help.followupboss.com/hc/en-us/articles/360015269133-Export-Contacts-to-a-Spreadsheet).
- **Mapping is not type-name parity:** FUB custom fields include recurring
  dates and dropdown choices; its tasks include nine categories and may use
  date-only or timed deadlines. Preserve source semantics and expose mapping
  loss. [Custom fields](https://docs.followupboss.com/reference/customfields-get),
  [task fields](https://docs.followupboss.com/reference/tasks-post).

## 4. The migration summary the admin sees

The first page identifies the source account, source-access scope, destination,
assessment time and assessment completeness. A per-entity table shows:

| Column | Meaning |
|---|---|
| Discovered | Known source total or enumerated lower bound, with its basis. Unknown is distinct from zero. |
| Retrieved | What was actually read, including selected fields and relevant source limitations. |
| Coverage | Complete for the verified scope, partial, unavailable, or not yet checked. |
| Destination readiness | Model available, transformation/review needed, or destination missing. |
| Next action | Concrete correction, mapping choice, additional source, or deferred capability. |

010a reports potential support and access evidence. Exact record-level
importability, duplicate forecasts and reconciliation require 010b's full
snapshot; shallow probes must not claim those results. Coverage and destination
readiness are separate dimensions. “Preserved” is used only after bytes were
durably captured, never for inaccessible source data.

010b and later add mutually exclusive per-record dispositions (imported,
unchanged, needs review, preserved only, excluded by an approved rule, failed)
and separately report missing/unknown coverage. Every transformation has a
reason and provenance; a total must not double-count one item across statuses.
The report stays understandable without showing SQL, internal IDs or secrets.

## 5. Proposed delivery sequence

Keep the ladder's existing identifiers; detailed specs are written one rung
at a time. The order below deliberately puts required supporting data before
the cutover rung.

| Order / rung | User-visible result and completion condition |
|---|---|
| 1 — 010a | Secure API connection and honest bounded access report, implemented and synthetically verified. Live authorized FUB validation is user-deferred; no export upload or imported CRM business records. |
| 2 — 010b | Core-first encrypted resumable snapshot and preview (D-061): People/users/stages/custom fields/notes/tasks; mappings, overlap candidates, unsupported values and explicit remaining coverage. Raw captures carry source IDs, API/schema/profile versions, capture times and keyed hashes. Further snapshot work must address history, communications, media and other uncovered families before cutover. |
| 3 — 010c | Confirmed People/contact/stage/assignment import into the new Organization using explicit provenance and matching rules. Restart or repeat produces no duplicates. Source attribution and Person/Inquiry semantics are resolved first. |
| 4 — 010f+ core rungs | Tags, custom definitions/options/values, notes and tasks through small typed import commands. Preserve authorship/timestamps and protect locally edited or tombstoned records on rerun. Exact rung splits follow the snapshot. |
| 5 — 010d | Supported historical inquiry/call/text/correspondence facts, each with verified meaning. Inaccessible content stays a disclosed gap. Resolve Today behavior before enabling imported backlog for agents. |
| 6 — 010e | Per-entity delta capture, final reconciliation and explicit cutover. Validate changes/deletions during the snapshot, then agree a source-write cutoff and final delta. No source cancellation, phone transfer or ongoing two-way sync is implied. |

No calendar estimate is credible before source access, volume and fidelity
gaps are measured. The existing 25k-People/50-member/five-concurrent-Today
envelope and D-050 verification budget remain the baseline.

## 6. Proposed defaults and decisions still needed

| Topic | Recommendation | Alternative / consequence | Gate |
|---|---|---|---|
| Destination | **Accepted: new, empty Organization first (D-059).** | Existing-Organization merge remains a separate future scope. | Precise emptiness/rerun rules in 010c. |
| Source route | **Accepted for 010a (D-060): API-first assessment.** Separately identified exports can supplement later rungs. | CSV-first was the alternative, with documented coverage limits. Do not silently narrow fidelity. | Actual source authorization before live calls. |
| Credentials | **Accepted for 010a (D-060):** Organization-scoped encrypted storage, redacted types, revocation/replacement, no key in browser storage/logs/Operator context. | Memory-only credentials were the alternative. Use existing development key conventions; do not introduce OpenBao early. | Implemented and synthetically verified; live validation remains deferred. |
| Recovery and updates | Checkpointed resume, immutable raw captures, source-ID mapping and conflict-aware updates. Keep FUB operational until reconciliation. | Automatic destructive rollback is a separate design; a new destination does not authorize wiping it after users add data. “Additive only” does not solve changes or deletions in a delta. | Before 010b/010c persistence/commit contracts. |
| Cutover readiness | Agree required entity coverage with the target team; proposed core includes notes/tasks/tags/custom fields. No silent truncation, inferred consent, or hidden gaps. | A deliberately smaller migration needs an explicit fidelity decision and visible exclusions. | Before confirming a real import/cutover. |

Additional mapping decisions belong to their rung: duplicate source People,
unmatched agents/ponds, contacts without usable contact methods, Trash,
date-only task deadlines, unsupported field limits, communication preferences,
and source deletion versus local edits. Preserve available opt-out/suppression
information; do not enable outbound communication until it can be enforced.
Older overdue tasks can overwhelm Today; do not fabricate contact attempts or
alter the ranking silently to hide that backlog.

FUB source access and CRM visibility are different: this CRM gives active
members Organization-wide Person visibility. The preview must expose any
change in audience and resolve handling of restricted/private source content
before import, without silently inventing a new CRM authorization policy.

O-012/O-013 remain prerequisites before the first external customer's real
consumer data is held, as recorded in the [decision log](../decisions/DECISION_LOG.md).
Use synthetic test data while resolving those obligations. This summary does
not decide key hierarchy, retention, erasure or recording consent.

## 7. Engineering boundary and next deliverable

Use the existing modular Rust application, SQLx/PostgreSQL and Vue. Proposed
migration connection/run/report/raw-capture/mapping records are tenant-owned;
reliable background work should reuse current worker patterns. No separate
service or unrestricted Operator database access is warranted.
Source mappings must distinguish trusted Organization, FUB account, entity
kind and source ID; replacing a connection must not mix two source accounts.

010a owns the implemented connection, bounded assessment and report
HTTP/persistence contracts, admin authorization, error states and Web
polling/recovery in its approved specification. 010b owns raw
storage/checkpoints; later rungs own typed imports and their contract amendments.
This summary does not silently expand current Person, note, task or field APIs.

The **010a specification and bounded brief** are implemented and synthetically
verified (D-060), with API-first access, encrypted persistent credentials and a
bounded probe profile. The proposed policies for later rungs remain unapproved.
010a owns encrypted evidence for its bounded probes; 010b owns full snapshot
storage and pagination checkpoints. Recorded synthetic acceptance proves:

1. Organization admin access and cross-Organization denial, including forged
   run/connection IDs and source-account replacement.
2. No FUB mutations and no imported business records during assessment.
3. Honest partial/unknown states for denied endpoints, revoked credentials,
   absent totals, pagination failures and source limits.
4. Safe cancellation/restart/retry with rate-limit pacing and secret redaction;
   source-provided links and content cannot redirect authenticated requests
   to arbitrary hosts or act as privileged instructions.
5. Stable report recovery through reload/reconnect and an accessible Web
   walkthrough with synthetic fixtures.

Import rungs additionally prove duplicate-free reruns, historical accuracy,
tombstone/local-edit preservation, per-entity delta reconciliation and agreed
Today behavior. Use `./scripts/check`, `./scripts/sqlx-prepare` when SQL changes,
`./scripts/check-db` under the single DB gate, targeted failure/tenant tests,
and D-050's single relevant performance run when applicable. No implementation
or release action was authorized by the initial planning request. The user's
subsequent D-060 approval authorized 010a implementation and verification;
the 2026-09-11 follow-up authorized source cleanup, commit, merge and push,
now completed. A later same-day follow-up authorized 010a deployment, which
is complete and verified; live FUB validation remains user-deferred. The user
also accepted core-first 010b planning (D-061): People, users, stages, custom
fields, notes and tasks, with all remaining families explicitly tracked. See
[the draft specification](../specs/SLICE_010b.md) and
[execution brief](../tasks/SLICE_010b_IMPL.md); their contracts and implementation
are not yet approved.
