# Mobile 001 — Durable offline field work

**APPROVED FOR IMPLEMENTATION — 2026-09-12, D-074.** D-073 accepts planning,
SQLite, initial offline selection and seven-day local access. D-074 accepts the
reviewed contracts and isolated synthetic implementation. No native app, sync
endpoint or schema below exists yet. Authority: the
[decision log](../decisions/DECISION_LOG.md), [plan](../plans/MOBILE_OFFLINE_FIRST.md)
and [execution brief](../tasks/MOBILE_001_IMPL.md).

The [bounded review](../tasks/MOBILE_001_REVIEW.md) is **READY FOR IMPLEMENTATION
APPROVAL** after two focused corrections: upload/download promotion ordering and
server-owned generation admission. The user subsequently approved this corrected
specification under D-074; implementation evidence remains separate.

## 1. Outcome and boundaries

In a synthetic operational Organization, an iOS or Android agent signs in,
downloads People/notes/tasks, works without connectivity and later synchronizes.
New notes, new tasks and task completion are the only initial mutations. Reading
Today shows the last server-computed result and locally pending task badges;
the device does not invent Today ranking or contact-attempt credit.

The offline set is the union of People in the actor's returned Today result,
People assigned to that actor, and explicit Person pins on this installation.
Pins are availability requests, never authorization. Online lookup may reach any
Person allowed by Organization-wide PersonVisibilityScope. Downloads and uploads
require an operational workspace; imported review workspaces remain unavailable
to this native workflow even for admins (D-064/065).

Download Person identity/contact/stage/assignment summaries, retained native
notes and tasks for selected People, and their minimal display-name references.
Page notes/tasks instead of embedding complete arrays. No inquiry/call/email/
imported-history/media download is implied. Every component declares coverage
and remaining pages. A resource/storage limit leaves it visibly incomplete;
never treat a partial download as an empty or complete local record.

Existing-note/task editing, deletion, new People, reassignment, stage changes,
outbound communications and server AI actions are later native capabilities.
Existing Web/Operator actors retain their normal commands while a phone is
offline; version/conflict handling must account for those concurrent changes.

## 2. Accepted policy

| Topic | Rule / status |
|---|---|
| Accepted offline access | Seven days after successful online authorization for the same actor, Organization and installation. Local activity, connection attempts, a push or merely detecting Wi-Fi cannot renew it. |
| Accepted durability | Display saved only after local commit; survive ordinary process/device restart. Protect pending work on access expiry. Device loss, uninstall/data removal and key loss before upload are not remotely recoverable. |
| Accepted first environment | Isolated synthetic development data and existing development identity. Real customer use remains behind privacy/readiness gates. No production ZITADEL integration or distribution release. |
| Accepted conflicts | New notes/tasks are independent additions. Completing a downloaded task requires its downloaded revision. Any intervening task change produces a visible conflict unless the exact operation already has a receipt. No last-write-wins or silent new-ID retry. |
| Accepted sign-out/revocation | Immediately lock account data and stop its sync. Preserve encrypted pending work separately; reopen/retry only after the same actor and Organization are authorized. A different account never adopts it. Explain pending-count consequences before sign-out; do not require connectivity to sign out. |
| Accepted device storage | Encrypt SQLite including journals; separate actor/Organization stores with device-bound keys in platform secure storage. Exclude CRM stores, keys and credentials from automatic cloud/device-transfer backup. SQLite is not inherently encrypted. SQLCipher Community is a candidate; pin/verify its native builds before use. |
| Accepted blocked work | Conflicts/rejections remain needs-attention with protected input. Revocation/deletion may prohibit reopening it; no automatic export/reattachment. Customer retention/erasure handling remains a first-customer gate; the synthetic proof retains blocked items until deliberate fixture disposal. |
| Accepted support | `mobile-v1` is supported throughout this development slice. Unknown protocol stops sync with update-required and preserves storage. Customer support duration, minimum OS versions and deprecation commitments need a release decision before independent distribution. |

Persist server-issued authorization expiry in protected storage. During a boot,
use elapsed monotonic time anchored to trusted server time; persist the last
trusted/observed state across restart. A detected backward clock jump or
unverifiable elapsed-time boundary locks access until online authorization.
The concrete platform contract must specify/test reboot clock handling; do not
claim resistance to arbitrary compromised-device clock/storage manipulation.
Expiry locks downloaded data and associated drafts; it does not delete them or
authorize a server write. A failed authorization never renews the access window.

Seven-day access is independent of operation retention. After same-identity
reauthorization, retry older work with its original IDs. Content-free consumed-
operation markers must not expire merely because local access expired. Include
receipt lifecycle in the customer-data inventory; this spec selects no customer
retention period and does not authorize indefinite customer-content retention.

## 3. Shared-contract declaration

| Prior contract | Approved change and reason | Ownership / compatibility |
|---|---|---|
| AddNote/CreateTask can duplicate after lost responses (015/016); commands own their transactions. | Native operation identity, request binding and result stored atomically with the shared command. Extract transaction-compatible cores; never write a receipt after a committing wrapper. | Backend owns command factoring and additive receipt schema. Existing Web/Operator wrappers retain DTOs, validations, clocks, outputs and post-commit publication. |
| Complete/reopen leave `task.updated_at` unchanged; no expected revision. | Independent task revision covering every relevant writer; native completion supplies a baseline or immutable create-operation reference. | Backend owns column/trigger and native DTO; existing Task HTTP shape stays unchanged. Web/Operator/import/admin changes must advance revision. |
| Reconnect refetches; ordinary reads hide tombstones; legacy Person readers may be unbounded. | Bounded reconciliation generations, mobile Person revisions and complete paged components. Mobile is an explicitly added body-read surface with normal note/task authorization. | Backend owns new read models/routes; no unbounded legacy reader, migration bypass, separate service or generic event store. |
| Cookie sessions; authenticated commands default to WebSession. | Native adapter uses existing development identity, trusted MobileSession origin and actor/workspace-bound offline access metadata. | Additive routes/origin constraints; preserve Web cookies. No second permission database or client-trusted identity. Inventory old origin readers before release. |
| Canonical note/task clocks represent server execution. | Device-reported action time is untrusted metadata, separate from server acceptance. | Canonical creation/completion and Today clocks remain server-owned. Native UI may label device time; it must not backdate business facts. |

D-074 approves implementation of this declaration. Freeze exact Rust/SQL modules,
constraints, lock order and error mapping before parallel client coding. Any
change beyond this declaration needs its own review.

## 4. Approved HTTP contract

Routes live in the existing Axum application under `/api/mobile/v1`. Use common
session identity, current membership and explicit operational-workspace checks;
do not assume path-matching middleware recognizes a new prefix. CRM responses
are HTTP `Cache-Control: no-store`; only the managed encrypted app store persists
them offline. Never log credentials, note/task text or raw request bodies.

| Route | Request and success |
|---|---|
| `POST /bootstrap` | Protocol and installation UUID (identifier, not authentication). Returns a server-owned context ID bound to that installation and the trusted actor/Organization, workspace revision, `authorized_at`, `offline_access_expires_at`, server time and capabilities/bounds. Same-identity reauthorization renews the binding without rebinding queued work. Sign-in uses existing `POST /api/session`; protect its credential/cookie and restrict transmission to the trusted configured API origin. |
| `POST /operations` | Exactly one immutable operation envelope below. Returns accepted receipt or replay of the same accepted action. Client actor/Organization fields are forbidden. |
| `GET /operations/{operation_id}` | Authorized receipt lookup for this actor/Organization, or generic not-found; not a cross-actor search. Recheck current visibility before returning a resource reference. |
| `POST /reconciliations` | Protocol, installation UUID and explicit Person pins. Returns server generation ID, evaluation time, expiry, completeness/selection count and first manifest page. Persist bounded metadata/IDs/revisions and a Today fingerprint, not copied note/task bodies. |
| `GET /reconciliations/{id}/manifest?cursor=…` | Actor/Organization/generation-bound selected Person IDs, expected revisions and selection reasons; last page explicitly marks enumeration complete. |
| `GET /reconciliations/{id}/people/{person_id}/{section}?cursor=…` | Closed sections `summary`, `notes`, `tasks`; bounded pages fenced by expected Person revision. Notes/tasks retain existing public fields; native tasks additionally carry decimal-string `revision`. |
| `POST /reconciliations/{id}/seal` | Validate authority, selection, Today fingerprint and Person versions in one consistent snapshot; return a seal and bounded Today result only for a complete unchanged generation. |

All native sync requests carry the issued context ID. Look up its server-owned
actor/Organization binding and compare it with the current session; neither
context nor installation ID is an authentication credential. A login change
cannot silently replay an old actor's queue under a new identity. The native
store also verifies the binding before upload or response application.

An operation contains `context_id`, `operation_id` UUID, closed `kind`, `device_recorded_at`
RFC3339 or null, and typed `payload`. Store it immutably before attempting upload.
Neither operation nor installation UUID is an authentication credential.

| Kind | Payload |
|---|---|
| `add_note` | `person_id`, `body`; existing NoteBody normalization/limits. |
| `create_task` | `person_id`, `title`, `kind`, nullable `due_at`, nullable `assignee_user_id`; existing title/kind/member rules. Null assignee means authenticated actor. |
| `complete_task` | `person_id` and exactly one target: `{task_id, expected_revision}` or `{created_by_operation_id}`. A create reference must identify this actor's accepted create-task operation in this Organization and supplies its committed target/version. |

Accepted receipt example (illustrative IDs):

```json
{
  "operation_id": "11111111-1111-4111-8111-111111111111",
  "outcome": "accepted",
  "resource_type": "task",
  "resource_id": "22222222-2222-4222-8222-222222222222",
  "committed_revision": "1",
  "person_revision": "8",
  "changed": true,
  "accepted_at": "2026-09-12T23:00:00Z",
  "replayed": false
}
```

The durable server receipt contains no note body/task title. It acknowledges that
action's result, not the resource's current state. Replay cannot overwrite newer
downloaded state. A note receipt has null `committed_revision`; a task receipt
captures its version at acceptance. Every receipt also captures the enclosing
Person projection's revision after the business write, inside the same
transaction. Fetch authorized current data separately.

Use the existing error-envelope convention with closed additional codes: 401
requires reauthorization; existing workspace/permission denials still apply;
404 hides foreign/missing resources; 409 includes `operation_payload_mismatch`,
`revision_conflict`, `dependency_pending`, `generation_changed`,
`generation_expired`, `protocol_unsupported`; 422 covers invalid domain input or
an explicit over-limit request. Transient service failure retries; invalid or
rejected work enters needs-attention. Never issue a replacement operation ID
automatically. Error metadata excludes customer text and foreign identifiers.

Proposed finite work bounds: one upload at a time per installation, 128 KiB
upload maximum; up to 100 component rows and 512 KiB serialized bytes per page
(fewer rows when required); manifest pages of 250 IDs; at most 25,000 selected
People under D-050 with explicit over-envelope refusal; two concurrent downloads.
Generations expire after 30 minutes. Admission has server-enforced limits of two
live generations per registered context, four per trusted actor/Organization and
20 per Organization, with no more than 25,000 manifest entries per generation.
Lock trusted admission counters while reserving capacity, account for the exact
manifest before accepting it, and bound expired-row cleanup. Admission also
counts retained expired rows until reclaimed, so failed cleanup cannot cause
unbounded storage. Installation UUID rotation cannot increase these allowances;
new context registration is separately capped at ten per actor/Organization.
Over-capacity returns a retryable 429 with a bounded Retry-After; it never evicts
another device's active generation. Keep accepted-operation markers outside
ephemeral generation cleanup. These are computational bounds for the synthetic
slice, not customer storage quotas or permission for silent truncation.

## 5. Atomic execution and stale intent

Unique server identity is `(organization_id, actor_user_id, operation_id)`. Bind
context/protocol/kind/normalized payload/dependency and original device time with a
versioned keyed digest. Retain no raw request in the receipt; key rotation must
keep older receipts verifiable. Changed payload under an existing ID conflicts.

One transaction performs workspace guard, operation reservation, current
resource/membership authorization, shared domain command, final receipt and
commit. Identical concurrent requests converge on that receipt. Transient
failure rolls back both receipt and business change. Publish invalidation after
commit only; realtime delivery never serves as proof of command acceptance.

Check current identity, operational workspace and resource visibility before
receipt replay. An accepted task completion may replay its content-free outcome
after its assignee changes, if the actor remains an active authorized viewer;
replay performs no mutation. Every new execution checks current write permission.
Missing or now-invisible resources return generic not-found; a client treats an
unconfirmable operation as needs-attention, never as permission to recreate it.
It must not infer that a not-found receipt means the original action never ran.

Task revision advances on actual title/kind/due/assignment, completion/reopen,
snooze, attribution or deletion changes from every writer. Check the baseline
after normal write authorization and before the already-completed shortcut.
Existing receipt replay precedes new execution/version checks. A new conflicting
completion requires reviewing current state and explicitly choosing a new action.
A dependent completion waits for its create receipt; dependency failure blocks
it visibly and cannot attach it to another task. Reauthorization keeps IDs intact.

## 6. Bounded download and removals

A generation records server-chosen evaluation time, actor/Organization/workspace
and role binding, protocol, exact selected IDs/versions and a fingerprint of the
bounded Today response. Enumerate selection in one consistent snapshot using
normal authorized Today/Person logic. A partial Today source or over-limit set
makes it incomplete and ineligible for seal/removal. Factoring Today's query core
must preserve public Web ranking, clock selection, limits and failure behavior.

A mobile per-Person revision covers precisely the summary/contact/stage/
assignment, note and task DTOs. Triggers cover every relevant writer, physical
and tombstone deletion, and joined user/stage label changes. User fanout includes
assigned agent, note author, task creator/assignee/completer. Neither
`person.updated_at`, `task.updated_at` nor migration-review revisions cover this
entire inventory. Role/workspace authority is checked separately at every request.

Read each page in a short consistent transaction; mismatched Person revision
invalidates that Person's staged bundle. Cursors bind actor/Organization,
generation, Person, section, revision and stable sort position. Persist page data
and its SQLite checkpoint together. Foreign-scope cursors reveal no content.

At seal, recompute selection and Today using the original trusted evaluation
time, check all Person versions and current authority in one consistent snapshot.
Mismatch returns bounded changed/added/removed metadata and requires refreshed
reconciliation. Changes after the validation snapshot belong to the next sync.
This is a read-projection protocol, not database replication or a generic event log.

Only fully staged, sealed components replace the active local generation and
authorize removal of absent server rows. Promote active-generation pointer,
Today and checkpoint atomically in SQLite. Drafts/outbox remain separate overlays.
Upload acknowledgments create durable accepted overlays and a per-Person minimum
observed revision, rather than immediately discarding the local action. Serialize
acknowledgment application and generation promotion in SQLite. Never replace an
already active Person bundle with a lower revision. Retire an accepted overlay
only when the active complete bundle reaches at least its receipt's
`person_revision`; a later authorized deletion/removal follows its explicit
reconciliation and local-data policy, never an inferred missing row in an old
generation. Preserve a removal conflict when the generation cannot demonstrate
it postdates the accepted local action. An older sealed generation may update
unaffected cached data but cannot hide a newly accepted note/task/completion.
Old Today results remain labeled with their evaluation time; pending/accepted
local badges persist until causally covering server state replaces them.
Reuse unchanged complete bundles when restarting, never partial/error responses
as deletion. Prolonged churn retains the previous usable cache and displays
sync-incomplete; pause automatic restarts after three consecutive generation
conflicts until a foreground retry or later backoff. Pending uploads stay safe.

This first design reconciles a bounded full ID manifest and downloads only
changed Person bundles. A durable incremental feed is a later optimization if
measured churn/bandwidth justifies it. Downloading selected notes/tasks still
requires storage; low-space pauses preserve the active generation and unsynced
work with honest incomplete coverage. No database is recreated to recover space.

## 7. Native persistence and user interface

Each actor/Organization store contains active projections, staged generations,
page checkpoints, drafts, immutable pending envelopes, ID mappings, local
acknowledgments and protected access metadata. Verify durability settings on the
actual SQLite/encryption library builds. WAL with reduced sync settings cannot
silently weaken the committed-save/power-loss requirement. Upgrade schemas
transactionally; preserve the database and report recovery on upgrade failure.

Autosave acknowledges a committed draft revision. Submission converts that
revision to an immutable operation and pending local view in one transaction.
Further editing cannot mutate an already submitted payload; preserve it as a
separate draft for a deliberate future action. A locally created task completed
before upload uses an immutable create-operation dependency. Pending badges do
not claim server acceptance or grant write permission.

Screens: Sign in, Today, People, Person notes/tasks, simple note/task composers
and sync status. Show saved-on-device, synced and needs-attention on each item;
show last sync and download coverage. On conflict, preserve the proposal and
fetch authorized current data; no automatic text merge or generic conflict editor.

Sync on foreground/open and usable connectivity, with backoff for transient
failure. Attempt permitted background work, but OS scheduling is not guaranteed
and never changes an item to synced. Key/background unavailability preserves
the queue. Calling and push registration are not prerequisites for this proof.

## 8. Verification and approval handoff

Synthetic fixture: two Organizations, two actors editing one Person, 100 selected
People, 1,000 notes, 1,000 tasks and 100 queued actions. Exercise each native test
runtime, two devices for one actor, seven-day expiry, restart and schema upgrade.
These are fixtures, not customer limits. Preserve D-050's paired regression and
plan-shape checks on changed hot paths; no laptop production-capacity claim.

Required proof: local write/disk failure; app/device restart; server commit/lost
response/retry; acknowledgment racing an older sealed generation; generation
admission with rotating installation IDs and failed cleanup; duplicate concurrent
requests; payload mismatch; complete/reopen/
stale retry; create-then-complete; second-actor edit; interrupted page; generation
expiry/change/seal race; deletion; session expiry/revocation; account/Organization
switch; migration-review denial; lease expiry with pending work; old Web/Operator
compatibility. Report physical-device/cellular evidence separately from emulators.

Before a platform is done, build/launch its actual native app and run persistence,
sync and UI checks. A CLI SQLite example or 390px Web screenshot is not native
verification. See [toolchain setup](../tasks/MOBILE_NATIVE_TOOLCHAIN_SETUP.md).

Approval is recorded in D-074. Freeze the implementation contract, build the
backend foundation and follow the coordinated three-worktree sequence. Customer retention/erasure/backup
and independent release support remain gates; they do not become approved through
a synthetic proof. SDK license/setup completion is separate from code approval.

## Sources for implementation choices

- [Android offline-first guidance](https://developer.android.com/topic/architecture/data-layer/offline-first)
  supports local reads, durable writes and explicit network reconciliation.
- [SQLite atomic commit](https://sqlite.org/atomiccommit.html) and
  [durability settings](https://sqlite.org/pragma.html#pragma_synchronous) inform
  the committed-save proof; they do not guarantee recovery after device loss.
- [SQLCipher documentation](https://www.zetetic.net/sqlcipher/documentation/) and
  [Android Community integration](https://www.zetetic.net/sqlcipher/sqlcipher-for-android-community/)
  identify an encrypted-SQLite implementation candidate, not a selected binary.
- [Apple key accessibility](https://developer.apple.com/documentation/security/restricting-keychain-item-accessibility)
  and [Android backup guidance](https://developer.android.com/privacy-and-security/risks/backup-best-practices)
  inform the approved development protection rules; customer readiness remains a
  separate gate.
