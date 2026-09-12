# Slice 010c — People import into a migration-review workspace

**Approved activity/read amendment — 010f2 (D-068, 2026-09-11):**
[010f2](SLICE_010f2.md) owns one independent activity child for a completed
People parent. Parent plans/results/identities and the original review binding
remain immutable. First activity confirmation takes the exclusive workspace
barrier and permanently switches complete activity reads to a distinct bounded
admin representation, including after cancellation. The new core-review routes
also support already settled People before activity confirmation and while a
People parent is running/paused/cancelled. Ordinary mutations, Today, Operator
and outbound actions remain held; no activation is added.

**APPROVED FOR IMPLEMENTATION, 2026-09-11 (D-065).** The user approved the
reviewed specification and execution brief. D-064 accepts separate source People,
explicitly approved stage creation and review-only operation until later activation.
Implementation and synthetic verification are authorized; source operations,
customer-data processing and release remain outside this approval.
Baseline: main `c6c5930`, deployed 010b source `89471f0`.

Inputs: [decisions](../decisions/DECISION_LOG.md) D-004/005/007/012/015/019,
D-050 and D-059–065; [architecture](../architecture/ARCHITECTURE_BASELINE.md);
[010b](SLICE_010b.md) and [its concrete contract](SLICE_010b_CONTRACT.md);
[current-code findings](../research/SLICE_010c_CODE_CONTRACTS.md);
[execution brief](../tasks/SLICE_010c_IMPL.md).
Independent [plan review](../tasks/SLICE_010c_REVIEW.md) returned READY after
five corrections and targeted confirmation. Implementation is now authorized.

## 1. Outcome and scope

An Organization admin turns a retained core FUB snapshot into actual CRM People,
contact methods, stages and assignments in a new, empty Organization. They first
review exact proposed writes and gaps, confirm, then inspect resumable progress
and per-Person reconciliation. Imported records remain in an admin review
workspace; normal agent use, Today and outbound actions stay unavailable until
a separately specified activation capability is approved and implemented.

010c is a database-only consumer of 010b evidence. It does not download from FUB,
import into another Organization, refresh source data, merge existing People,
apply source deltas or alter FUB. No Inquiry, contact attempt, correspondence,
note, task, tag/value, custom-field value, file, authenticated user or invitation
is synthesized from a Person. No native client, new Operator tool, broker,
service, storage vendor, erasure policy or cutover is included.

### Accepted choices and defaults

- **Accepted D-064:** distinct source People stay distinct even when normalized
  contacts overlap. Overlaps remain visible; 010c never calls intake `identify`.
- **Accepted D-064:** admins may explicitly approve creation of matching source
  stages. Suggested mappings and creations are never executed merely by previewing.
- **Accepted D-064:** imported records are for admin review until later activation.
- **Accepted with this spec (D-065):** local Person creation time is import time; source
  creation/update/source attribution are separately preserved provenance. No
  synthetic Inquiry or last-contact fact is used to affect Today or source filters.
- **Accepted with this spec (D-065):** a single retained snapshot/boundary and frozen plan drive one
  first-import run. Paused execution resumes that plan. Updating already imported
  records, repairing a confirmed mapping and importing another snapshot belong
  to a later, explicitly specified revision/delta workflow.

## 2. Admin flow and states

1. An admin opens a completed 010b snapshot and selects **Plan People import**.
   The server checks retained authority and source eligibility. This writes only
   migration planning records and performs no source request or business mutation.
2. A bounded worker prepares a versioned, encrypted manifest from accepted raw
   captures. The screen reports preparation progress and per-Person outcomes.
   Completed 010b preview candidates help navigation but never authorize a write.
3. Admins review stage mappings/creations and source-user/pond assignments. Every
   source stage is mapped to an existing same-Organization stage, explicitly
   approved for creation, or left held. Every non-null source assignment is
   explicitly mapped to an active member, explicitly made unassigned, or held.
   Exact-label/unique-email matches are suggestions included in the final review.
4. Changing mappings creates a new immutable plan revision and regenerates its
   manifest. Prior revisions remain historical. No revision can be edited after
   it becomes ready; no mapping changes are allowed after confirmation in 010c.
5. The ready plan shows: People to create, People held with reasons, normalized
   contact counts, overlap counts, proposed stages, assignment/unassigned counts,
   every remaining unimported family/field, source window and review-only effects.
   Held records cannot be hidden by a successful total. Zero eligible People
   cannot be confirmed. Admins may confirm the eligible subset with held counts
   explicitly acknowledged; this is not complete migration or cutover approval.
6. **Confirm People import** binds the exact plan ID/revision/digest, review
   acknowledgments and a request ID. The server atomically rechecks eligibility,
   enters migration-review mode and queues the frozen run. An uncertain response
   is retried with the same request ID, never a newly prepared plan.
7. Progress reports committed People and contacts, held records, remaining work
   and a concrete pause/failure reason. Reload uses durable state. **Resume** is
   explicit; **Cancel remaining import** fences future work and retains all
   committed records/stages/evidence. Neither action activates the Organization.
8. After completion, admins follow imported Person links and reconcile outcomes.
   The workspace remains in review mode, including after cancellation/failure.
   There is no Activate, Undo import, Delete imported book or destructive reset.

Plan revision states: `building`, `ready`, `paused`, `failed`, `superseded`.
Run states: `proposed`, `queued`, `running`, `paused`, `completed`, `cancelled`,
`expired`. Ready confirmation expires ten minutes after publication. Expiry
requires a fresh plan; it does not delete evidence. Runtime/transient/storage
errors pause with a stable code; terminal completion means every planned item
has a committed disposition, not that all source data was imported.

## 3. Source eligibility and exact extraction

The source must belong to the trusted active Organization and bound FUB account.
Require a supported `fub-core-v1` snapshot in `completed` or `completed_with_gaps`,
and a completed preview whose sequence boundary equals the final snapshot
sequence. People, users and stages streams must have proven exhaustion. Notes,
tasks or custom-field gaps may remain with explicit disclosure because they are
not written by 010c. Paused/cancelled source runs and earlier partial previews
remain reviewable in 010b but cannot be executed by this first import.

For each Person, stage and source-user ID used by the plan, join observations
to their exact captures. Supporting mapping evidence has the same eligibility
requirements as Person fields; projections never supply executable stage labels,
user identities or assignment semantics. An importable
observation requires matching Organization/run/family/representation, successful
2xx status, accepted successful classification and nontruncated raw bytes.
Decode the original collection item at its stored ordinal with the existing
duplicate-key-rejecting lossless parser; verify exact source ID and semantic
HMAC. Never use a bounded display projection, selected HTML, client JSON or a
preview disposition as the source of a destination field. Existing raw rows are
not rewritten. IDs remain positive decimal strings, including values up to
128 digits; no JavaScript Number or fixed-width source-ID conversion.

Repeated identical accepted observations produce one candidate for that source
ID. Comparable semantic variants, rejected observations that disagree with the
accepted value, inaccessible content or unsupported profile/encoding produce a
held record. The first version does not select a variant or silently choose the
latest observation. Rejected-page records are never promoted just because 010b
included them in a preview. Invalid-ID observations remain separately counted
evidence and cannot receive a fabricated import identity.
Unqualified/conflicting stage or user evidence holds the affected mapping and
dependent People. An explicit choice does not cure unqualified source evidence;
pre-confirmation requalification or later source repair is required.

Trash People are held for later archive/erasure semantics. Restricted/private
content and unqualified source flags remain explicit review evidence. All
returned communication flags/statuses are preserved; absent flags are not
permission to communicate. The entire review workspace remains unavailable for
operational use regardless of how a particular flag is classified. This hold
does not waive the separate real-customer-data prerequisites.

## 4. Mapping, fields and attribution

### People and contacts

- Import `firstName`/`lastName` as source text, mapping whitespace-only values
  to null with an explicit transformation receipt. Do not split `name` to invent
  first/last names. At least one nonempty name or normalizable contact is required
  to avoid an empty display name; contactless named People are allowed. Other
  source shapes remain held. No unrelated change to intake's contact requirement.
- Read every source email/phone entry. Reuse the current typed normalizers;
  do not silently strengthen ordinary CRM normalization in this slice. If any
  nonempty contact value cannot be represented losslessly in its native string
  shape or normalized, hold that Person for correction in a later source/plan
  workflow. Null/empty entries are counted separately, not fabricated contacts.
- Before readiness, check every native destination string for PostgreSQL text
  representability, including rejection of embedded NUL. For this import version,
  normalized contact values and newly created stage names must be at most 2,048
  UTF-8 bytes, a conservative admission limit for the existing full-value indexes.
  Preserve unsupported source text and hold affected People/creation choices;
  never truncate or strip characters to pass. A source stage too large to create
  may still be explicitly mapped to a valid existing stage. These are disclosed
  import-version limits, not changes to ordinary CRM normalization or global CRUD.
  Pin actual indexed inserts at the boundary on the supported test database.
- Within one Person and kind, equal normalized values produce one contact row;
  preserve all source spellings/labels/status/preference metadata in source
  provenance, and disclose this normalization. Across different People they
  remain separate contact rows, even when values match exactly (D-064).
- Preserve a deterministic per-kind source order. A single recognized primary
  marker takes precedence; otherwise first source-array occurrence determines
  presentation, with ambiguous/unknown preference explicitly flagged. Freeze
  `isPrimary` JSON number `1` as primary and `0` as non-primary for this profile;
  absent/null is unspecified and other shapes are unknown, not truthy coercions.
  A single primary is selected before within-Person normalized deduplication,
  so its spelling remains the representative even if repeated earlier.
  Pin these rules in extractor fixtures; unqualified
  metadata is never interpreted as communication permission.
- Add nullable `contact_method.import_order` with nonnegative values, unique per
  Person/kind when present. Imported contacts use that order; normal inserts leave
  it null. Primary-contact readers order by import order (nulls last), then
  `created_at`, then ID. Existing rows need no backfill; their existing chronological
  order remains, with stable tie-breaking. Audit every shared primary-contact
  read, including People, Today and Operator. Source labels/statuses remain
  provenance, not an invented native contact-type or suppression model.

### Stages and assignments

- The source stage label must resolve to one captured stage. Exact trimmed-label
  equality can propose an existing stage; ambiguous/missing source stages hold
  the Person until the pre-confirmation mapping is resolved. Missing stage values
  require an explicit existing-stage choice, never an implicit first-stage default.
- A create choice preserves the trimmed nonempty source label. Source label
  collisions require explicit mapping; do not invent suffixes or case folding.
  Append new stages after existing positions in stable source order, with source
  ID as tie-breaker. Detect SMALLINT position exhaustion before confirmation.
  Protected-source flags do not create special CRM behavior; Trash is held.
- The typed import-stage command is scoped to an approved manifest and same
  Organization. It may INSERT only the approved new stage; it cannot rename,
  reorder or delete existing stages. Add only the required application INSERT
  grant. Mapping/receipt and stage creation commit atomically and are replay-safe.
- Explicit source user/pond mappings target active memberships or `unassigned`.
  A source name is not a CRM identity. No account/invitation or guessed assignee
  is created. Freeze active-member identity and revalidate at commit; a changed
  reference pauses remaining work instead of falling back. Already imported
  People are not silently reassigned during resume.

### Provenance and history

The identity key is `(Organization, FUB account, family, source ID)`, independent
of credential revision. Person/contact source mappings reference exact capture,
ordinal, representation, semantic HMAC, snapshot boundary, plan/engine version
and import run. Preserve original source label/URL, source creation/update
values and contact metadata in encrypted erasable provenance, without copying
whole raw pages again. Missing/ambiguous dates remain unknown; `person.created_at`
is the actual local import time and activity columns start null.

Write a PII-free `person_imported` receipt/fact with local IDs and the standard
envelope, plus initial stage/assignment facts with an explicit migration reason.
Use `Actor::System`, `Origin::Migration`, actual committing time, run correlation
and the current authorized confirmer/resumer as on-behalf-of actor. Do not
attribute source historical actions to that admin or a fabricated source user.
Source strings, contact values and payloads never enter immutable facts/logs.

Add a bounded read-only Person import-provenance endpoint and display section.
Source URLs are escaped text, not automatically fetched. Existing Inquiry lists,
latest-Inquiry-source filters and history meaning remain unchanged: a Person
with imported source attribution still has zero Inquiries until real Inquiry
facts exist. Later history work owns any new source-filter semantics.

## 5. Durable migration-review boundary

Add an Organization mode, default `operational`; the new value is
`migration_review`. The mode is server-owned, not a client or model permission.
It is a workspace readiness gate before normal Person visibility, not a new
Team/AssignedUser visibility scope. D-064 specifically restricts normal agent
use during review; operational Organizations retain D-004/005 unchanged.

At first confirmation require no customer/business state: no People/contacts,
Inquiries, notes/tasks, calls or call facts, correspondence/raw intake payloads,
or pending business/Operator work. Settings, memberships/invitations, seeded
stages, tag/custom-field definitions, saved filters/Today configuration and
010a/010b evidence are allowed. An existing import binding rejects another run.
Do not erase rows to make a destination qualify. Freeze the concrete table and
entry-point inventory from [the code record](../research/SLICE_010c_CODE_CONTRACTS.md)
before implementing the gate; new/unknown business writers must fail closed.
Existing terminal IDs-only Operator audit does not disqualify an otherwise empty
Organization. Active admissions, actionable proposals and claimed/in-flight work
do; independently check all resulting customer/business tables. Retain past audit
without erasing it to qualify. This eligibility clarification is accepted with
the full spec under D-065, in addition to D-064's three earlier choices.

The emptiness check and transition must serialize with **all** ordinary business
writes, including unattended intake and CLI/domain paths. Use one Organization
transaction barrier: ordinary writes acquire a shared guard and verify operational
mode before mutations/side effects; confirmation acquires the exclusive guard,
checks emptiness and persists review mode plus the run binding atomically.
This drains prior writes and prevents writes entering between import batches.
An HTTP-only check, a one-time SELECT, or an import worker's lease is insufficient.

Protected business reads also take a short shared workspace permit and check
current mode/role while assembling their complete response. This prevents a
member authenticated before entry from loading newly imported rows afterward.
The permit lasts until all DB-backed response data is loaded, then is released;
it never spans a provider request, streaming response or browser session. A
single guard connection may protect existing pool-based queries, but acquisition
must reserve sufficient pool capacity and have a bounded timeout. AuthContext
and `/me` provide presentation context, not the authoritative enforcement check.

Register an IDs-only durable Operator admission under the shared guard before
scheduling/inference. Bind its expiry to one absolute deadline enforced around
the entire local turn, including scheduling and tool execution; refuse review
entry while an admission is active. A relative timeout that starts only after
spawning is insufficient. Cancel local work at that deadline and recheck mode
for tool reads, proposal writes and commands. Release admission after the local
task ends; terminal audit/release remains allowed. Expired/crashed admission
recovery cannot authorize new work. No DB transaction spans inference, and no
claim is made that cancelling a request stops computation at a remote provider.
Use a small separate admission table: the existing terminal Operator ledger
stays append-only. Guard proposal confirmation before consuming its single-use
claim, so a review-mode rejection does not consume the proposal.

Import transactions use a private typed review-write permit issued only after
locking the bound workspace/run/plan and rechecking the current admin and lease.
It is not an environment variable, client field or privileged generic SQL tool.
Ordinary writes cannot obtain it. Acquire the workspace barrier before existing
membership/run/Person locks; preserve one documented lock order and bounded
lock timeout. Mode/authority is rechecked inside the committing transaction.
Before an external call can occur, durable call reservation and the operational
guard must prevent review entry from racing that side effect.
Enter-review refuses nonterminal calls and existing raw intake/capture work.
Signed terminal call events, authorized hangup, expiry/failure reconciliation
and already-started terminal audit remain narrowly allowed cleanup. Active
call transitions and ordinary outcome correction are not cleanup exceptions.

| Surface in review mode | Behavior |
|---|---|
| Current Organization admins | Migration controls and ordinary read-only People/detail/provenance review; setup membership/stage-list/definition reads remain available |
| Ordinary members | Session/status and logout remain available; business reads and writes denied with `workspace_in_migration_review`; status-only Web screen |
| Today and operational Operator | Unavailable for all roles; no derived queue or inference side effect. Do not rewrite ranking or manufacture history to hide imported backlog |
| Ordinary business mutations/outbound calls | Typed domain guard rejects before mutation/provider effect; UI controls disabled; same enforcement for stale clients and existing Operator commands |
| Unattended intake/correspondence | Return retryable 503 before raw persistence or a success acknowledgment; relay retries are external and finite, not a durable application queue. Existing workers skip held Organizations before claim/inference/application without consuming attempts or hot looping |
| Migration source/preview/import/budget controls | Current-admin/010a/010b-specific authorization still applies; review mode grants no new source authority |
| Identity/setup administration | Login/logout, membership/invitation lifecycle, password/security administration and associated address provisioning remain allowed under existing rules. No invitation is sent by import. Stage/definition reads remain available; settings/definition/saved-filter/Today writes and explicit intake/capture provisioning are blocked during review |
| Realtime/push | Import emits no operational notifications in review mode. Existing IDs-only messages cannot grant reads; no new operational realtime token in review mode |

An admin downgrade fences future import commits; a current admin may explicitly
resume the same run, atomically replacing its executor/lease. Credential changes,
disconnect or departure of the original source initiator do not revoke retained
import authority. Losing retained data/key access pauses; no new source fetch is
used to repair it. Activation/release from a nonempty review workspace is absent
from 010c, including through generic admin/CLI paths.

## 6. Persistence, atomicity, budgets and recovery

Use ordinary migration control tables, not a new job service. Concrete logical
records are workspace binding, import run, immutable plan revision, per-source
manifest item, stage/source-Person/contact mapping, per-item outcome/fact and
import-owned byte reservation. Every child has composite Organization/run/plan
references and appropriate unique source identity. Mapping existence is durable
even when later erasure makes a Person unavailable; never recreate a missing
mapped Person as a retry shortcut. Do not cascade-delete the source identity
mapping when a target disappears; report the unavailable-target conflict.
Future erasure owns the tombstone/redaction policy.

Prepare at most one bounded raw capture per transaction/work unit; use existing
64 MiB retained-byte admission while materializing its encrypted manifest.
Do not load an entire book or group into memory. Execution iterates keyset pages
of at most 50 IDs/size descriptors, loading one payload at a time; each Person
commits independently and atomically with all
its contacts, mapping, facts, per-item result, cursor advancement and exact
retained-byte settlement. Preparation freezes a proven upper bound for each
item's added retained bytes, including provenance, receipt, encryption overhead
and source mappings. Reserve that amount immediately before the Person's commit,
then settle exact bytes under the same lock/fence. Stage creation has the same
bounded accounting rule. The work-unit ceiling remains 64 MiB; items exceeding
it are held before readiness, with a specific version-limit reason. A >2 MiB
item is executable when its proven bound and current allowance permit it.
Insufficient remaining run/Organization allowance pauses before business writes;
an allowance increase cannot fix an intrinsic work-unit limit. Business rows
are not counted as raw snapshot payload allowances.

Plan/provenance/receipt variable bytes share the existing run/Organization
logical allowances. Add an explicit import reservation owner/table; source
cancellation must not release an import's reservation or fences. Count ciphertext,
nonces, source identifiers, indexed keys and variable metadata using a documented
column inventory; fixed row/index/WAL/replica overhead remains physical storage.
Do not increase ceilings automatically or invent production quotas. Existing
admin budget approval stays revisioned, and a budget increase never resumes work.

Claim/settle with durable leases and fresh fencing tokens. No DB/network wait
while holding unrelated long transactions. A crash before commit rolls back the
Person and its ledger/receipt; a crash after commit finds the mapping and reports
the previous outcome without duplicate rows or facts. Do not use updated names,
email/phone or source payload hash as the import identity key.

Stages already created and People already committed survive pause/cancel. A
source mismatch, deleted mapping target, different confirmed plan, invalid member
or changed approved stage is an explicit conflict; no silent overwrite, merge,
replacement Person or destructive rollback. Resume may continue only the same
frozen plan after the actual cause is resolved. Requests are idempotent by
Organization/actor/action/request ID plus exact input digest; altered replay
conflicts, and duplicate confirmation cannot establish another binding.

## 7. Shared contracts and owned amendments

These changes are approved under D-065 and owned by 010c.
Existing 010a/010b response shapes and stored source bytes remain compatible.

| Current contract | Proposed change / reason | Affected components and compatibility |
|---|---|---|
| No workspace review mode; normal members can use all Organization People | Server-owned Organization mode/revision on the shared session Organization payload, read/write permits, Operator admission and `workspace_in_migration_review` errors | Auth/login/me/invitation acceptance, existing business commands/workers, Web shell/route/query caches, CLI/Operator; existing Organizations default operational; old clients fail safely in review mode |
| 010b has capture/preview only | Add DB-only import routes below, manifest/identity mappings, receipts and import reservation ownership | Migration domain/API/Web, crypto purposes, schema/state/worker composition; additive, no source profile rewrite |
| Stages are seeded/read-only for `crm_app` | Scoped typed stage INSERT from approved import manifest and provenance | Stage schema/grant/domain; no general stage management API or existing-stage rewrite |
| Person/contact creation comes through intake; primary order is timestamp-only | Dedicated typed import/multi-contact path; nullable deterministic import order and consistently ordered reads | People/contact persistence, all primary-contact projections, Today/Operator readers; no change to intake identity matching |
| History has no import fact/reason; source means Inquiry source | Add IDs-only import fact, migration initial-state reasons and separate erasable provenance query | Timeline/Person Web/API and fact DTO vocabulary; no synthetic Inquiry or source-filter reinterpretation |
| Snapshot reservations belong only to source/preview | Separate import-owned reservation records sharing the same ledger with matching lock order | Budget/recovery code; preserve original budgets and existing cancellation semantics |

### Server compatibility and release constraint

Old browser clients fail safely against the new server, but pre-010c server,
worker and CLI executables do not enforce review mode. All such executables
must be retired before enabling first confirmation; mixed pre-gate/compatible
runtime operation is prohibited. The eventual release must inventory active
processes/workloads, verify the deployed source includes the gate, and verify
that launch/admin paths use compatible binaries before permitting import.

Once any persistent review binding exists, deploy/rollback preflight must inspect
that database state and select only an artifact known to enforce the review
boundary. Unknown compatibility fails closed. No rollback to a pre-gate binary,
including 010b's saved application-rollback recipe, is allowed even if the additive
schema remains. Recover with a compatible build or fix forward while preserving
review mode and committed data. Do not use a whole-database restore, mode toggle
or removed binding as an application-rollback shortcut. This constraint belongs
in release tooling/procedure and its verification; no deployment is authorized
by this planning document.

### Import HTTP and Web

Proposed endpoints (strict unknown-field rejection, `no-store`, trusted
Organization from session; IDs are UUIDs except exact source IDs/counts/bytes):

| Endpoint | Request / result |
|---|---|
| `POST /api/migrations/fub/imports` | `{request_id,snapshot_id,preview_id}` → 201 `{import_id,plan_id,state}`; planning only |
| `GET /api/migrations/fub/imports` and `/{id}` | Scoped keyset list/detail, latest plan/run states, counts, policy/coverage and server-derived permitted actions |
| `POST /api/migrations/fub/imports/{id}/plans` | `{request_id,expected_plan_revision,stage_mappings,assignee_mappings}` → 202 immutable replacement plan; arrays are patches with at most 50 combined choices, inheriting untouched choices from the named revision; prohibited after confirmation |
| `GET /api/migrations/fub/imports/{id}/plans/{plan}/records` | Keyset cursor, disposition, limit 1–50 → bounded proposed core values, transformations, held reasons and next cursor |
| `POST /api/migrations/fub/imports/{id}/confirm` | `{request_id,plan_id,plan_revision,confirmation_digest,acknowledgments}` → 202 durable queued run/mode; independent under-lock validation |
| `POST /api/migrations/fub/imports/{id}/retry` | `{request_id}` → 202 resume paused preparation/execution at the existing committed checkpoint, with phase/action explicit in detail |
| `POST /api/migrations/fub/imports/{id}/cancel` | `{request_id}` → 200 fenced cancellation; no rollback or activation |
| `GET /api/migrations/fub/imports/{id}/results` | Keyset pages of imported/already-imported/held/pending outcomes, local Person links and reasons |
| `GET /api/people/{id}/import-provenance` | Bounded escaped source attribution/contact metadata and migration links under current workspace/Person authority |

Mappings must use tagged choices (existing stage/create captured stage/hold;
active member/unassigned/hold), never client-supplied trusted labels or source
payloads. Counts are nonnegative decimal strings, not mixed-family totals.
All mutation bodies are limited to 64 KiB; oversize returns 413 before work.
Reject duplicate choice keys. Plan creation/copy/rebuild is asynchronously
checkpointed and paged, not one full-book transaction. Only one revision builds
at a time; patching a building revision returns busy. Before confirmation, a
patch may replace a ready, paused or failed revision, explicitly superseding and
fencing its predecessor; an expired confirmation may receive a fresh revision
on the same unbound run. Cancel stops the entire run, not just its current plan;
starting over after cancellation requires a new planning run. Mapping lists are
paged independently, never embedded unbounded in
the import detail. Add `GET /api/migrations/fub/imports/{id}/plans/{plan}/mappings`
with `kind=stage|assignee`, scoped keyset cursor and limit 1–50 for these choices.
Supporting mapping fields use
`GET /api/migrations/fub/imports/{id}/plans/{plan}/mappings/{mapping}/fields/{field}`
for exact bounded inspection of the retained stage/user evidence. Enforce current
admin and Organization/import/plan/mapping ownership; mapping field cursors have
a distinct scope from record field cursors. This additive concrete endpoint is
within D-065's owned review contract and preserves existing record-field routes.
Record/provenance pages have a 512 KiB serialized ceiling. Long field display is
explicitly labelled abbreviated with its full length; display abbreviation never
changes the stored plan or imported value. A scoped field-detail read supports
bounded text segments for exact inspection, with a cursor bound to plan/item/field
and revision (or Person/provenance revision). It serves escaped text, not a raw
capture download. Never truncate values in the manifest to fit a response.
Freeze exact DTOs, state transitions, error/status mapping and cursor AEAD scope
before coding; routes above and field semantics cannot change silently.
Use 401 for no session, 403 for forbidden actor, 404 for out-of-scope/missing
resource, 409 for stale/changed/busy/workspace state, 422 for malformed choices,
and retryable 503 for unavailable storage/locks or deferred unattended intake.

Migration Web stays on Manage → Migration. Current admins see plan review,
source/destination counts, stage/user choices, explicit confirmation, pause/
cancel/resume and previous results. Organization mode gates normal navigation;
on mode/identity change abort requests, clear scoped caches and refetch trusted
session context. Late responses must never repopulate another actor's screen.
Use existing bounded polling/backoff and terminal stop behavior. Escape source
content; no HTML execution, credential echo or raw download. Test desktop and
390px Web. Native SwiftUI/Kotlin clients remain future work.

## 8. Acceptance and evidence

1. **Source boundary:** accepted raw 2xx observations reconstruct matching exact
   IDs/HMACs/ordinals; rejected, truncated, foreign, mismatched, invalid-ID and
   multi-variant evidence cannot execute, including stage/user supporting inputs.
   No projection-derived destination value.
2. **Eligibility/coverage:** require final People/users/stages completion and
   final matching preview boundary; allow disclosed unrelated gaps; partial runs,
   Trash and unresolved fields remain held with inspectable reasons.
3. **Identity/fidelity:** two People sharing contacts stay separate; duplicate
   normalized values within one Person are disclosed; all valid contacts and
   deterministic primary order survive, including reload. Test named/contactless
   and undisplayable records, native NUL/index byte limits with poorly compressible
   near-4 KiB values, original attribution and exact-number boundaries.
4. **Mapping:** reviewed stage creation runs once, preserves stable append order
   and detects name/position conflicts; active-member/unassigned choices are
   explicit and revalidated. No invitations or guessed identities.
5. **Atomic new destination:** dirty Organization, competing confirmation and
   intake racing entry cannot partially enter or contaminate the workspace.
   Terminal IDs-only Operator audit is allowed; active/actionable work blocks entry.
   Exercise every inventoried ordinary command/background writer, not HTTP only;
   member read/entry races, Operator scheduling/deadline/crash and call admission
   must prove both ordering and bounded pool/lock behavior.
6. **Review hold:** admin read-only review works; members/Today/Operator and all
   ordinary mutations/outbound paths fail safely, including stale UI, direct API,
   CLI/domain calls and worker paths. No provider call precedes the guard. Fresh
   and existing operational Organizations retain normal behavior.
7. **Authority:** foreign IDs/forged choices/role changes fail; admin revocation
   fences commit; current-admin explicit resume works after lease replacement.
   Disconnect or source-initiator departure does not imply retained-data loss.
8. **Durability:** per-Person failure rolls back contacts/mappings/facts/accounting;
   post-commit lost response, process restart, retry and duplicate confirmation
   never duplicate writes. Cancel retains committed data and review mode.
9. **Storage:** preparation/execution admission, ceilings, failed reservations,
   source-cancel independence and explicit post-increase resume use small fixtures.
   Include >2 MiB provenance that imports within allowance, and an intrinsically
   over-unit item held before confirmation. Business bytes and retained evidence
   bytes are reported without claiming disk quotas.
10. **History/read behavior:** imported People have zero Inquiries and null
    activity columns; no fabricated contact attempts/Today suppression facts.
    Provenance is visible separately, source filters retain their old meaning,
    and no PII appears in audit/realtime/logs. Operational primary-contact reads
    remain correct after the declared ordering amendment.
11. **UI/session:** production Web against the real API plus injected source
    fixtures proves plan revision, confirm/replay, progress, pause/resume/cancel,
    review-mode navigation, role/mode transitions, held rows, reload and 390px.
12. **Performance/gates:** D-050 25k People/50 members with dense shared contacts;
    indexed mapping/claim/manifest/result pages, no per-Person full-book scans or
    pair materialization. One required paired regression for changed primary-
    contact readers; relevant plan evidence. Sequential `sqlx-prepare`, `check`,
    `check-db` on final code; at most two implementation review/fix rounds.
13. **Runtime compatibility:** eventual release evidence shows no pre-gate
    server/worker/CLI remains, and rollback preflight rejects an old/unknown
    artifact once a review binding exists. Recovery preserves the hold. During
    implementation, exercise this with synthetic manifests/state; no live rollout
    is required to complete the implementation checks.

At planning approval, no acceptance test above had run. Actual implementation
results now belong to the [verification record](../tasks/SLICE_010c_VERIFICATION.md);
the brief defines execution ownership. Live FUB validation, customer privacy
readiness, deployed recovery proof and later activation are separate gates; an
admin-only review workspace does not by itself make real customer data permissible.
