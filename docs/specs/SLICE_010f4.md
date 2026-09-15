# Slice 010f4 — Notes and tasks for admitted People

**IMPLEMENTATION AUTHORIZED — D-084, 2026-09-14.** The user requested “implement them”.
The earlier draft/planning labels below preserve the proposal history; D-084
supersedes their approval boundary. Complete independent planning review before
code work, then implement and verify without requesting repeated approval of
these contracts. Materially different policy remains outside this authorization.

**DRAFT — planning authorized 2026-09-14; numeric assignment and implementation
contracts remain proposed.** D-083 records the planning request. This is the next
activity step in the [family ladder](../plans/SLICE_010_ADMITTED_PEOPLE_LADDER.md).
[Brief](../tasks/SLICE_010f4_IMPL.md),
[paired plan](../plans/MOBILE_006_010f4_PARALLEL_LAUNCH.md),
[author findings](../tasks/MOBILE_006_010f4_PLANNING_REVIEW.md).

Read AGENTS.md, D-015/027/050/053/054/059–070/073–083 and O-012/O-013/O-015;
[architecture](../architecture/ARCHITECTURE_BASELINE.md),
[original activity import](SLICE_010f2.md), [admission](SLICE_010e3.md),
[admitted refresh](SLICE_010e4.md), [metadata](SLICE_010f3.md).
010f2 owns the unchanged note conversion, role/type/timezone, source-only component
and native equality rules. This draft extends their source/cohort/lifetime and
shared identity contracts explicitly; it does not reopen the original child.

## 1. Outcome and scope

A current Organization admin chooses a terminal admission cohort, previews notes
and open/completed tasks from one qualified retained core snapshot, confirms the
role/kind/timezone mappings and counted exclusions, and imports eligible activity
once into those exact admitted People. Results distinguish applied, already
present, held and unprocessed records, plus source-only component coverage.
Imported tasks do not reach ordinary Today while the workspace is under review.

One activity family root per admission run. Only committed successful results of
a terminal completed or cancelled admission are eligible; an admission remainder
is its own cohort. Successful metadata import/core refresh is not a technical
prerequisite. The sequence places this work after 010f3 but does not require a
Person to have a tag/custom-field value to receive its notes/tasks.

First activity coverage only: no refreshing previously imported activity, changing
confirmed mappings/source, repairing settled holds, inferring source deletion,
reopening original 010f2, new People, historical event/call/text imports, operational
timeline redesign, communications, activation or live FUB/customer processing.
No new source downloader. Missing captures stay explicit work for the existing
capture capability under separate source authorization.

## 2. Declared shared-contract changes

| Current | Proposed and why | Owner, affected components and compatibility |
|---|---|---|
| 010f2 child resolves original People/results and original snapshot only | Separate admitted-activity root/plan/attempt/manifests/results on immutable admission identities and a qualified selected snapshot | Migration lane owns additive Rust/DB/HTTP/Web; original imports/results remain unchanged |
| Global `migration_activity_identity` key already spans Org/account/kind/source ID but ownership FKs require original activity rows | Extend that same registry with exclusive admitted-owner references; retain the existing global PK, original columns/data and original FKs | Migration lane owns additive nullable new-owner columns, conditional old/new shape checks and grants; no second competing registry, repointing or synthetic original rows |
| Original private activity permit only authorizes original manifests | New exact admission/attempt/plan/unit/lease private INSERT permit | Workspace guards, command validators and worker; ordinary mobile/Web/Operator permissions gain no bypass |
| Original activity confirmation establishes a durable bounded-reader barrier | New admitted confirmation is another durable reason to require the same bounded activity review paths | Shared read guards, paged note/task/provenance joins and preflight; old original-only endpoints retain their source meaning |
| Bounded review revision sums only original activity children | Include original and admitted activity commit revisions in the same checked decimal revision, preserving opaque cursor scoping | Migration lane owns review-query/cursor fixtures; an admitted commit invalidates an in-progress traversal even without an original-child write |
| No admitted-activity routes or capability | `/api/migrations/fub/admitted-activity-imports` and `fub-admitted-activity-v1` | API/worker/admin/migrator inventory and recovery/preflight; new source-free admin pages, distinct cache keys and cursor purposes |
| Existing snapshot ledger has source, original and admitted metadata owners | New admitted-activity reservations/results charged to its selected snapshot/Org | Reuse approved allowances, ceilings and exact settlement; no duplicated charge for retained referenced source bytes |

After approval, the owned contract checkpoint freezes schema/FKs/closed enums,
wire fixtures, grants, reader fence predicates, lock order and counted columns.
Proposed amendment pointers belong to 010f2 §§2/5–7, 010e3's later-family boundary,
and the shared readiness/reader specifications. No existing accepted contract is
changed by drafting this file.

## 3. Exact source and target qualification

Select one sealed completed 010e1 report for the same trusted Org/account/original
parent. Its newer core snapshot is the only source of People, users, notes/detail
and both task streams. It may be the report used for admission, or a later report
whose source capture started strictly after that admission source completed.
Freeze report/output revision, snapshot/final sequence/capture interval, profile,
parser/HTML/time/tzdb versions and the original workspace binding before preview.
The report's changed/unchanged/new label is not activity qualification.

Independently require exhausted `people`, `users`, `notes`, `note_detail`,
`tasks_open` and `tasks_completed` streams. Both families finish enumeration
before this combined root is ready; no implicit notes-only fallback from incomplete
tasks. Settled individual note-detail failures become visible holds. Stages/custom
fields or unrelated history gaps are disclosed but not extra activity prerequisites.
Verify all retained raw identities, request/stream/representation, status, AEAD/HMAC,
source profile limits and lossless decoded source IDs. Corruption/missing keys or
broken capture links pause; valid unsupported/restricted records are held.

All observations of a source note/task in the chosen boundary participate, including
ones referring outside the cohort. Classify out-of-cohort references as excluded
coverage, not native inserts. Duplicate-key/invalid-ID occurrences stay countable.
Equal repeats collapse only with qualified same-representation equality. Conflicting
variants (including different People or task states across open/completed streams)
hold the identity; capture order/updated timestamp cannot pick a winner. A list
record and its note detail are complementary; require the matching detail for
executable body/author/time, never fill missing fields from the list. A restricted
404 is inaccessible evidence, not a deletion instruction. No cross-snapshot joins.

Resolve each target using the successful admission item/result, account-qualified
global Person identity, original parent/workspace and live same-Org Person. Preserve
exact immutable admission IDs, including committed cancelled results. No contact/
name matching, current assignment inference or fabricated original-import result.
Missing/deleted/tombstoned/mismatched targets hold related records. Freeze the cohort
before confirmation; subsequent admissions cannot enlarge it.

Use a bounded source index/preparation walk shared across the cohort, not a complete
source-book scan per Person. Qualified unknown/large/source-only content stays
inspectable through bounded exact field reads. Do not widen the existing activity
raw-reader limit merely because another retained feature accepts larger captures.
The contract checkpoint pins the current supported representations/read limits and
test fixtures; a required fidelity/parser-policy change returns for review.

## 4. Native mappings and fidelity

Reuse 010f2 §§3–4 in full, including the following consequential rules:

- Readable plain text plus the exact encrypted original; versioned non-executing
  HTML conversion, explicit subject treatment, restrictions/inaccessible content
  held, no truncation beyond the 10,000-character native note limit. Replies,
  reactions and unsupported attachment/content remain counted retained evidence.
- Task title uses the unchanged 500-character validator. Kinds require explicit
  source-to-native choices; mapping an appointment makes a task, not a calendar
  event. Recurrence/reminders/descriptions and external references remain explicitly
  source-only. Never promise uncaptured future Action Plan tasks.
- Open/completed state must agree with qualified evidence. A completed task needs
  its source completion instant; do not call the ordinary completion command or
  invent an actor/fact/contact credit. `updatedBy` is not a qualified completer.
- Date-only due values use end of day in an explicitly confirmed IANA source zone,
  freezing zone/tzdb/profile/resulting instant. Timed values require explicit
  offsets; inconsistent fields, skipped dates and inexact instants are held.
  Undated is explicit, not inferred from malformed data. No browser/server-zone
  fallback; retries reuse frozen instants.
- Confirm role mappings separately for note author, task creator and assignee.
  Parent/admission suggestions are not authorization. Historical author/creator
  may map to same-Org inactive members; assignee must be active or explicitly
  unmapped. Revalidate locked memberships. Do not substitute the importer, create
  identities or infer a user from a display name. Preserve source attribution.
- Native created/updated/completed times follow 010f2's exact rules; import time
  is separate. Notes/tasks remain erasable CRUD. Retained private/source audience
  restrictions remain visible for eventual activation review.

Plans freeze destination mappings and the exact proposed normalized native fields,
source-only acknowledgements and body conversion output. Dirty choices must be
applied or discarded before confirmation. At most 50 mapping patches per request;
ready plans expire after ten minutes, following the admitted-family precedent.
Source or mapping changes before confirmation create a new preparation/plan
revision and invalidate downstream choices/cursors; after confirmation they are
not allowed within this root's first-coverage lifetime.

## 5. Identity, atomic execution and lifetime

One unit is one source note or task. Identity remains unique by
`(organization_id, source_account_id, kind, source_id)` in the existing registry;
native source pairs remain `fub` / `v1:<account-id>:<record-id>`. Different IDs with
equal text are distinct. Existing bare legacy keys are ambiguous holds.

Extend the existing identity table with admitted root/confirmed plan/manifest
references and an exact exclusive-owner constraint: an original row has the
original non-null FK tuple and no admitted tuple; an admitted row has the admitted
tuple and no original tuple. Preserve old rows and composite FKs byte-for-byte;
make no target FK that would cascade away deletion protection. Do not backfill a
second registry or repoint an existing owner. An original/admitted unit consults
the same key and native source pair under the same existing Org serialization.
Both writers must handle the new ownership shape explicitly before release.

Matching existing source pairs require the same Person, origin, exact normalized
body/title, mapped IDs, kind, due/state/completion and created/updated values under
010f2's equality rule. Local edits, snooze/reopen/completion changes, a different
target or missing/tombstoned native row produce holds, never UPDATE/resurrection.
An exact compatible row without registry identity can establish its identity and
result atomically. An existing identity retains its original owner; another unit
can report verified already-present without adopting ownership.

Per-unit private permit validates current admin, workspace/parent, successful
admission result, root/attempt/confirmed plan/manifest, unexpired fenced lease,
exact native UUID/source pair/Person/mapped IDs and approved INSERT. No permission
for native UPDATE/DELETE, metadata/core edits, facts, ordinary commands or source
operations follows from `origin=migration`. All native data, identity/equality
result, encrypted provenance, checkpoint/counters and reservation settlement
commit together. A failure leaves none partially committed.

Lifecycle: preparing → ready → queued → running → completed; failures pause the
owned phase, Retry explicitly resumes immutable confirmed bytes, and Cancel is
terminal for that attempt while retaining committed records. Current admin authority
is checked at every read/action/claim/reclaim/commit. Explicit Retry may adopt the
currently authorized admin as executor under the existing activity rule; receipt
replay always checks current authority. Restored capacity/keys do not auto-resume.

Confirmation requires a fresh plan/revision, fresh compatible-release evidence,
budget, unchanged dependencies, explicit counted held/source-only acknowledgement
and at least one eligible native or verified already-present unit. All-held plans
remain replannable. Exact authorized receipt replay precedes fresh expiry/readiness
checks; changed input under the same request identity conflicts.

A confirmed cancelled attempt may have one successor containing only its exact
never-settled remainder: same source, mappings, cohort, normalized content and
deterministic target IDs. Settled holds/applied/already-present units are excluded.
One active attempt and one successor per predecessor are enforced transactionally.
Late workers cannot commit after cancel/lease replacement. An unconfirmed cancelled
root may be reprepared; completed first coverage does not accept a later source or
mapping repair. Zero inserts after cancellation/revalidation is an honest outcome.

Reconciliation separates the frozen cohort's primary units from out-of-cohort
coverage and nonexclusive source-only issues. For a confirmed manifest, planned
units equal applied + already-present + held + never-settled remainder across its
attempt chain. Source-only components and invalid source occurrences have separate
counts; they must not inflate native row totals or disappear on cancellation.

## 6. Bounded review, authority, accounting and recovery

Existing admitted-Person review resolution is reused, with native note/task source
provenance joins extended to the exclusive admitted identity owner. Do not duplicate
rows/counts by joining original and admitted provenance indiscriminately. Paged
review includes correct author/assignee/time and `can_manage=false`; note body
reads retain the established admin-only/no-store exposure and limits.

The current review revision queries only original activity imports. Extend it to
cover monotonic original and admitted committed activity counters, with overflow
rejection, unchanged decimal wire encoding and scope-bound opaque cursors. Native
insert/result and the admitted counter advance atomically. Validate the composite
read revision throughout paging so an admitted insert cannot slip between note/
task pages while the old original-only token remains unchanged. Never reset/drop
confirmed counters to make a stale cursor valid. Preserve original-source listing
semantics; only the combined native activity view gains this broader revision.

First admitted confirmation takes the existing exclusive workspace reader barrier
and durably installs the new confirmed-plan predicate before any native insertion.
Legacy complete Person/history/task reads holding a shared guard finish first;
later legacy reads fail before loading activity. The predicate includes confirmed
cancelled attempts with zero writes. Existing original confirmed predicates remain.
This changes neither Organization-wide PersonVisibilityScope nor the review hold.

`fub-admitted-activity-v1` covers the new identity-owner shape, original and admitted
workers, bounded readers and compatible recovery. Preparation must refuse an
unready writer/reader schema; fresh confirmation additionally requires the existing
five-minute operator report and observed API/worker/admin/migrator/container/scheduled
inventory. Durable confirmation requires capable subsequent launches. Add per-unit
readiness fences to affected already-running original workers/readers so old code
cannot bypass the new ownership/read contract. No binding/identity reset to run an
old binary. Record a compatible forward-recovery path; no rollout occurs in planning.

Follow workspace/membership/Org/snapshot-ledger/root-plan-unit/Person/native lock
order and deterministic multi-member locks, with bounded waits. Integrate existing
registry uniqueness and original worker locks rather than a generic family framework.
No network or inference inside transactions. Mobile metadata work touches separate
native tables; jointly test shared guards, broad revisions and retention bookkeeping.

New root/plan/source/mapping/manifest/attempt/result/issue/receipt/reservation state
uses composite tenant keys. Count all variable identifiers, encrypted source
derivatives, choices, target snapshots, provenance, receipts and control/remainder
rows before coding. Charge new retained bytes to the selected source snapshot and
Org, without double-charging retained references or changing old owner charges.
Use existing ceilings/allowances, unit reservation maximum 64 MiB, 60-second fenced
leases, separately reserved cancel/receipt capacity and exact atomic settlement.
Measure native note/task row/index bytes separately. Key/integrity/capacity failures
pause safely; one owner's cancellation never releases sibling reservations.
Required dependencies remain retained under existing rules; no new retention term,
customer quota, storage product or erasure policy is introduced.

## 7. HTTP and Web

Separate admitted-activity routes follow existing strict body/error/receipt shapes:

| Action | Proposed typed body / behavior |
|---|---|
| Prepare | `{request_id, admission_id, report_id}`; creates/reprepares the unique unconfirmed family root |
| Replan | `{request_id, expected_plan_id, choices, source_timezone?}`; immutable mapping revision; changing source uses explicit reprepare before confirmation |
| Confirm | `{request_id, plan_id, expected_revision, acknowledge_held, acknowledge_source_only}` |
| Retry / Cancel | `{request_id, expected_revision}`; scoped owned phase or terminal attempt |
| Remainder | `{request_id, attempt_id, expected_revision}`; one exact frozen successor |
| Read | Root/list/detail, source/mapping/result/issues and exact source-field segments; all server-scoped and paged |

Bodies ≤64 KiB; patches/pages ≤50; display pages ≤512 KiB, summaries ≤128 KiB;
full fields use 4–65,536 UTF-8-byte segments with scalar-boundary progress. Source
IDs/counters are decimal strings. Strict closed errors/DTOs and encrypted cursors
bind Org/root/plan/attempt/revision/endpoint/filter/field at the checkpoint.
Unauthenticated/platform-only 401, member 403, foreign/missing resource 404,
malformed 400, stale/incompatible 409, unavailable/corrupt evidence 503. Preserve
existing middleware precedence; all responses including outer denials are no-store.

Web presents cohort/source interval and completeness, main-note conversion output,
source-role/kind/timezone choices, source-only components, held reasons, counted
subset confirmation and native result/remainder totals. Unknown response uses the
same request identity, not a new confirmation. Retry/cancel are explicit; cancel
explains preserved data; polling stops in terminal/paused states. Preserve exact
source reads and admitted-Person provenance at desktop and 390px. Clearly state
first activity coverage, later history/delta gaps and continuing review hold.

## 8. Acceptance and evidence

| ID | Required result | Verification |
|---|---|---|
| F4-01 | Completed/cancelled cohorts include only settled successful admissions; no metadata prerequisite or synthetic original rows | DB preparation/identity tests and exact original/admission rowset comparison |
| F4-02 | Same-admission/later qualified source works; mixed snapshots, missing streams/detail, conflicting task partitions/Person variants and unknown/raw limits are honest holds or pauses | Retained synthetic capture/representation fixtures; no live source calls |
| F4-03 | Note conversion, roles, kinds, completion and explicit timezone rules match 010f2, with exact original evidence and source-only counts | Reused interpreter tests plus admitted integration fixtures/DST/invalid cases |
| F4-04 | Original/admitted/global source identities never duplicate, repoint, overwrite local edits or resurrect deleted targets | Registry/FK/permit/worker race tests and preserved native/source hashes |
| F4-05 | Confirm/retry/lost response, rollback, lease/cancel races and exact remainder settle each unit/byte once | Fault injection at native/identity/result/ledger boundaries, two-worker tests |
| F4-06 | Both ordinary typed commands and HTTP obey tenant/admin/workspace boundaries; disconnect permits retained reads; demotion pauses writes | Two Org/admin/member fixtures, direct permit negatives and outer no-store checks |
| F4-07 | First confirmation fences legacy readers even with zero writes; old identity/readiness/worker/recovery paths fail closed | Shared/exclusive reader race, complete incomplete-schema readiness matrix and old-worker unit attempt |
| F4-08 | Bounded review shows admitted provenance once, correct roles/times and receipts; admitted commits invalidate old page tokens; no content in telemetry/realtime | API/DTO tests, insert-between-pages regression and safe telemetry inspection |
| F4-09 | Admin completes preview → mappings → confirm → partial cancel → remainder → reconciliation on desktop/390px with account-switch fencing | Real browser/isolated API walkthrough, screenshots and rowset/accounting inventory |
| F4-10 | Source indexing and new/changed hot statements are bounded/indexed on a realistic 25k book; unaffected Person/Today payloads/regression satisfy D-050 | One paired benchmark plus EXPLAINs, report storage separately |

Run the brief and paired final gates. At most two review/fix rounds under D-050;
blocking gaps never count as a pass. O-012/O-013 customer readiness and live-source,
activation, distribution and deployment boundaries remain unchanged.
