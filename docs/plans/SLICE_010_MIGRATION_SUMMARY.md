# Slice 010 — FUB migration planning summary

**Status: 010a DEPLOYED AND VERIFIED (2026-09-11); live FUB validation deferred.
010b core-first capture/preview is also deployed and verified under D-063;
010c and 010f1 are also deployed and verified under D-065/D-066 and follow-ups.** The user
selected **a new, empty CRM Organization first** (D-059), then approved 010a's
API-first assessment and encrypted saved credentials (D-060). Unaccepted
later-rung recommendations remain proposals. Initial survey used main `b2fb368`, after 019b;
010c code findings use main `c6c5930`, after 010b's release.
No FUB account was connected or customer data fetched.

**010a planning follow-up (historical):** the user approved the next rung, which now
has a [010a specification](../specs/SLICE_010a.md) and
[bounded brief](../tasks/SLICE_010a_IMPL.md). D-060 accepts API-first assessment,
encrypted saved credentials and its additive contracts. Live validation is
explicitly deferred while no FUB test account is available. The first assessment
uses six bounded checks, not full source enumeration. This does not reduce the
later snapshot, preservation or reconciliation requirements.

**Current implementation:** 010b captures the six core record families and
generates retained, deterministic previews with explicit remaining coverage,
storage allowances and recovery. Both bounded reviews, all full gates and
synthetic API/browser verification passed. See the
[010b verification record](../tasks/SLICE_010b_VERIFICATION.md). Its subsequent
[shared-development release](../tasks/SLICE_010b_RELEASE.md) passed; authorized
live-source qualification remains deferred.

**Current implementation:** [010c](../specs/SLICE_010c.md) and its
[brief](../tasks/SLICE_010c_IMPL.md) define a retained-evidence People import.
D-064 accepts separate People with overlap flags, explicitly approved matching
stages and an admin review workspace until later activation. D-065 approves the
complete spec/brief after independent READY review. The implementation
does not create Inquiry history or release imported People to normal agent use.
All required implementation checks passed; [evidence](../design/qa/slice-010c-2026-09-11/README.md)
records the exact source and proof. Git integration, publication and cleanup are
complete under the D-065 follow-up: implementation `fcd2480`, merge `c3f6ca9`,
pushed to main. Its subsequent
[shared-development deployment](../tasks/SLICE_010c_RELEASE.md) is verified.
Next-import planning is now authorized; live FUB validation remains deferred.

**Latest deployed slice:** [010f1](../specs/SLICE_010f1.md), with its
[execution brief](../tasks/SLICE_010f1_IMPL.md), imports tags and custom-field
definitions/options/values for People from a completed 010c import. It reuses
the same retained snapshot and review workspace through an additive child
workflow; it does not reopen the People plan. Embedded tag membership can be
restored, while a complete standalone tag catalog remains future capture work.
Current limits, explicit mappings/creation, held incompatible data and no
replacement of differing values are accepted under D-066.
Notes and tasks follow in separately specified core-import work.
[Independent plan review](../tasks/SLICE_010f1_REVIEW.md) returned READY;
D-066 now approves the full specification, policies and shared contracts.
[Implementation and synthetic verification](../tasks/SLICE_010f1_VERIFICATION.md)
are complete: final gates, 90 measured plans / 246 checks, 51 browser checkpoints
and exact native reconciliation passed. The explicit D-066 follow-up authorized
commit, merge, push, cleanup and shared-development deployment. Implementation
`f37ddd1` and merge/runtime source `e36ce36` are published, all 975 source hashes
match, and the merged worktree/branch is removed with the 45 pending main docs
preserved. The [release record](../tasks/SLICE_010f1_RELEASE.md) and
[public evidence](../design/qa/slice-010f1-2026-09-11-release/README.md) record
43 HTTP/auth/asset checks, eight browser workflows, eight inspected desktop/390px
screenshots and tunnel 200/200/101. All 45 business counts and three operational
workspace revisions are unchanged; all 43 migration tables remain empty.
Actual-DB launch/confirmation preflight passed with metadata capability; its
five-minute report is not automatically renewed, so a later confirmation or
recovery needs fresh actual workload evidence. Backup catalog validation was
performed, not a restore exercise. Live FUB/customer-data work and activation
stay deferred; notes/tasks are the next core-import planning scope.

**010f2 implemented, release pending:** [010f2](../specs/SLICE_010f2.md) and its
[execution brief](../tasks/SLICE_010f2_IMPL.md) propose notes/open/completed tasks
from the same retained snapshot onto completed 010c People, independently of the
metadata child. D-067 accepts readable plain-text notes with exact originals
retained, and date-only tasks due at day's end in an admin-confirmed source
timezone. Explicit mappings, timestamp/unsupported-content holds, source-only
replies/settings, no overwrite/resurrection and paged admin review are specified
for approval. See the [independent review](../tasks/SLICE_010f2_REVIEW.md).
D-068 approved the full implementation. Both bounded reviews and synthetic runtime reconciliation are complete; source remains uncommitted and unreleased. See [verification](../tasks/SLICE_010f2_VERIFICATION.md).

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
idempotent rerun from an unrelated second import. D-064 now settles shared
household contacts: distinct source People stay separate with flagged overlaps.

## 2. What changed since the parked plan

Tags, notes, tasks and typed custom fields now have destination models.
Notes and tasks include source-ID uniqueness and tombstones; custom-field
definitions include `source`/`external_key`, with value origin/correlation
metadata. The notes/task models remain foundations for later import commands;
010f1 now implements the retained tags/custom-field import.
See [015](../specs/SLICE_015.md), [016](../specs/SLICE_016.md),
[019](../specs/SLICE_019.md), and [019b](../specs/SLICE_019b.md).

| Data family | Current destination and planning treatment |
|---|---|
| People, emails, phones | Core tables exist. Preserve every source identifier and contact method. Flag missing/invalid contacts and ambiguous identity matches; names alone must not merge People. |
| Stages, users, responsibility | Map source stages and users to Organization stages/members in preview. FUB users are not automatically authenticated CRM accounts. Unmatched users and pond/collaborator roles need visible dispositions. |
| Tags and custom fields | Deployed 010f1 restores embedded tags and custom definitions/options/values from the completed parent's retained source, preserving source keys and option identity. Current text/label/number limits and recurring-date semantics can prevent exact representation; preserve and report exceptions instead of truncating/coercing. Full standalone tag capture remains future work. |
| Notes | Plain-text destination exists, with a 10,000-character limit. 010f2 implements source authors/timestamps, readable HTML conversion with exact originals, and explicit holds/source-only replies and attachments. D-067 accepts the conversion choice; D-068 now approves full implementation. |
| Tasks | Destination supports call/email/text/follow-up/other. 010f2 implements explicit type/user mappings, qualified completion times, and undated/timed/date-only policies. D-067 accepts confirmed-zone end-of-day conversion; D-068 now approves full implementation. |
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
| 3 — 010c | People/contact/stage/assignment import into the new empty Organization is implemented, synthetically verified and deployed under D-065/follow-ups. Separate People, explicitly approved matching stages and admin review-only use remain D-064. Frozen-plan recovery/provenance avoids duplicates and synthetic Inquiry history; activation remains separate. |
| 4 — 010f1, then 010f2 | Deployed/verified 010f1 imports embedded tag membership and custom definitions/options/values onto completed 010c People, preserving the review hold. [010f2](../specs/SLICE_010f2.md) adds notes/tasks with authorship/timestamps, source-qualified identity, local-edit/tombstone protection and bounded admin review; D-068 approves implementation; synthetic runtime reconciliation now passes, with final closure tracked in [verification](../tasks/SLICE_010f2_VERIFICATION.md). Full standalone tag capture is still outstanding. |
| 5 — 010d | Supported historical inquiry/call/text/correspondence facts, each with verified meaning. Inaccessible content stays a disclosed gap. Resolve Today behavior before enabling imported backlog for agents. |
| 6 — 010e | Per-entity delta capture, final reconciliation and explicit cutover/activation. Resolve source privacy, enforce communication restrictions and review Today before releasing the D-064 hold; validate changes/deletions and agree a source-write cutoff/final delta. No source cancellation, phone transfer or ongoing two-way sync is implied. |

No calendar estimate is credible before source access, volume and fidelity
gaps are measured. The existing 25k-People/50-member/five-concurrent-Today
envelope and D-050 verification budget remain the baseline.

## 6. Proposed defaults and decisions still needed

| Topic | Recommendation | Alternative / consequence | Gate |
|---|---|---|---|
| Destination | **Accepted: new, empty Organization first (D-059).** | Existing-Organization merge remains a separate future scope. | Precise emptiness/rerun rules in 010c. |
| Shared contacts | **Accepted D-064: separate source People; flag overlaps.** | Automatic household merge is not selected. | Concrete normalization/provenance in 010c. |
| Unknown stages | **Accepted D-064: explicit matching stage creation.** | Mandatory mapping into current stages is not selected. | Approved manifest, collision/order checks in 010c. |
| Initial use | **Accepted D-064: admin review-only until activation.** | Immediate agent operation is not selected. | Server-enforced hold in 010c; later activation owns readiness and release. |
| Source route | **Accepted for 010a (D-060): API-first assessment.** Separately identified exports can supplement later rungs. | CSV-first was the alternative, with documented coverage limits. Do not silently narrow fidelity. | Actual source authorization before live calls. |
| Credentials | **Accepted for 010a (D-060):** Organization-scoped encrypted storage, redacted types, revocation/replacement, no key in browser storage/logs/Operator context. | Memory-only credentials were the alternative. Use existing development key conventions; do not introduce OpenBao early. | Implemented and synthetically verified; live validation remains deferred. |
| Recovery and updates | Checkpointed resume, immutable raw captures, source-ID mapping and conflict-aware updates. Keep FUB operational until reconciliation. | Automatic destructive rollback is a separate design; a new destination does not authorize wiping it after users add data. “Additive only” does not solve changes or deletions in a delta. | Before 010b/010c persistence/commit contracts. |
| Cutover readiness | Agree required entity coverage with the target team; proposed core includes notes/tasks/tags/custom fields. No silent truncation, inferred consent, or hidden gaps. | A deliberately smaller migration needs an explicit fidelity decision and visible exclusions. | Before confirming a real import/cutover. |

Additional mapping decisions belong to their rung: unmatched agents/ponds,
contacts without usable contact methods, Trash,
date-only task deadlines, unsupported field limits, communication preferences,
and source deletion versus local edits. Preserve available opt-out/suppression
information; do not enable outbound communication until it can be enforced.
Older overdue tasks can overwhelm Today; do not fabricate contact attempts or
alter the ranking silently to hide that backlog.

FUB source access and CRM visibility are different: operational Organizations
give active members Organization-wide Person visibility. D-064 explicitly accepts
an admin-only migration review hold; the implemented 010c gate enforces it and
010f1 preserves it. The
preview must still expose audience changes and restricted/private source data;
later activation requires those policies to be resolved before wider access.

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
bounded probe profile. Proposed policies beyond the approved 010b/010c/010f1
scope remain unapproved.
010a owns encrypted evidence for its bounded probes; 010b owns core snapshot
storage and pagination checkpoints. Recorded 010a synthetic acceptance proves:

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
[the approved specification](../specs/SLICE_010b.md) and
[execution brief](../tasks/SLICE_010b_IMPL.md). D-063 subsequently accepts their
reviewed contracts, storage policy, implementation and synthetic verification.
That implementation and its required gates are complete. The user's follow-up
authorized commit, merge/publication, shared-development deployment and cleanup,
now completed; live source validation remains deferred.

The reviewed 010c implementation, synthetic verification and shared-development
deployment are complete under D-065 and its follow-ups. D-066 approved
implementation and synthetic verification of the reviewed
[010f1 specification](../specs/SLICE_010f1.md) and
[brief](../tasks/SLICE_010f1_IMPL.md). Its explicit follow-up authorized Git
integration, cleanup and deployment, now completed and
[verified](../tasks/SLICE_010f1_RELEASE.md). D-067 authorizes 010f2 notes/tasks
planning and accepts its two source-interpretation choices; full shared contracts
and implementation are subsequently approved by D-068. Later families, customer-data/live-source
work and activation retain their own scope and readiness gates.
