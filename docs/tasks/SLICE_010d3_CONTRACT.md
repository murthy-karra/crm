# Slice 010d3 concrete contract checkpoint

Frozen for the D-086 implementation lane. This admits historical event, call,
and text **metadata** from one qualified retained 010d1 capture to the exact
successful results of one terminal People-admission cohort. It extends the 010d2
timeline identity and reader seam; it does not modify a 010d1 capture, original
history plan/anchor, native truth, Today, notes/tasks, activation, or raw source
data.

`fub-admitted-history-v1` is the admitted-history compatibility capability and
engine name. The existing interpretation value remains
`fub-history-interpretation-v1`. The original-anchor reader stamp remains the
exact transaction-local `crm.history_reader=fub-history-timeline-v1`; admitted
history adds the independent exact transaction-local
`crm.admitted_history_reader=fub-admitted-history-v1`. Neither stamp is an HTTP
field or permission.

## Qualified root and fixed source boundary

`POST /api/migrations/fub/admitted-history-imports` accepts exactly:

```json
{"request_id":"uuid","admission_id":"uuid","history_capture_id":"uuid"}
```

The server resolves all other bindings. The admission must be same-Organization,
terminal (`completed` or `cancelled`), have a confirmed admission plan, and have
at least one successful settled result. Its cohort is exactly those successful
results, including settled cancelled results; it is never a query over current
People.

The capture must be `completed_with_gaps`, have the same original parent,
confirmed original People plan, Organization, account, workspace binding and
source access user as the admission's original parent. Its start is strictly
later than completion of the newer core snapshot used by the admission. Events,
calls and texts must each have successful terminal exhaustion/reconciliation.
The root freezes the capture ID/revision/final sequence/interval, account/access
user, profile/parser/schema versions, interpretation/identity-key versions, and
the admission parent/plan/report/snapshot/final sequence. Diagnostic, truncated,
identity-only, corrupt and unexhausted capture data cannot be prepared.

Preparation reads authenticated retained bytes only. It cannot request FUB, look
up credentials, invoke a downloader, map source users, name/contact-match, or
join captures. The unchanged capture linkage is evidence only: a valid source
Person can resolve through the exact admission result although its old projection
said unlinked.

The root's immutable source/cohort binding HMAC uses
`admitted-history-plan-v1`; it serializes IDs, revisions, account, interval,
sequence, versions and coverage in declared order, never customer content.
Confirmation, worker admission and every commit revalidate the exact binding.

## Classification and global identity

The sole global identity relation remains `migration_history_import_identity`.
Its stable key remains:

```text
timeline-import-identity-v1(account, family, representation, source-record-id)
```

The canonical HMAC remains the current complete lossless canonical-record HMAC
under its existing account/family/representation purpose. Capture UUID is not an
identity input. Every occurrence for a stable identity, including excluded or
other-Person observations, is considered. Equal canonical repeats become one
unique unit with occurrence provenance. Conflicting variants, ambiguous/grouped/
nested relationships, or target disagreement hold the complete identity.

The closed occurrence disposition values are `eligible`, `equal_repeat`,
`excluded`, and `held`; terminal result values are `imported`,
`already_present`, `equal_repeat`, `excluded`, and `held`. Held reasons are:

```text
invalid_identity | ambiguous_relationship | conflicting_variants |
out_of_cohort | target_missing | target_erased | target_identity_mismatch |
identity_erased | identity_conflict | fact_or_display_missing |
capture_integrity | source_binding_changed
```

`excluded` is retained coverage, not an implicit hold. It includes a valid target
outside this cohort. Invalid-ID occurrences remain individually countable; source
absence never directs deletion. An existing global identity is
`already_present` only when canonical HMAC, family, live same-Organization
Person, immutable fact and visible display provenance agree. Different target or
canonical bytes, tombstone/erasure, or missing fact/display is held. Execution
cannot adopt/repoint ownership, change native facts, move history, rebuild erased
display, or clear a tombstone.

Across an attempt chain:

```text
unique planned = imported + already_present + held + never_settled
```

Unique identities, occurrences, equal repeats, invalid occurrences, excluded
coverage and nonexclusive warnings are separate totals. Every occurrence has an
inspectable manifest/result outcome; equal repeats do not increase inserted count.

## Additive persistence and exclusive owners

The assigned additive migration creates these Organization-keyed tables:

```text
migration_admitted_history_root
migration_admitted_history_plan
migration_admitted_history_attempt
migration_admitted_history_manifest
migration_admitted_history_candidate
migration_admitted_history_result
migration_admitted_history_remainder
migration_admitted_history_receipt
migration_admitted_history_reservation
migration_admitted_history_issue
```

The root owns immutable admission/capture binding, current attempt, lifecycle,
workspace revision, executor/lease, budget and bytes. Plan owns sealed binding,
coverage/counts, classified position, readiness and expiry. Attempt owns one
plan, executor/lease, applied position and results. Manifests own occurrences;
candidates own stable-identity grouping; result/remainder/issue/receipt/
reservation rows have composite Organization-owner FKs. A remainder has exactly
the predecessor's never-settled units. All child FKs include Organization. Ready
plans/manifests are append-only; results/remainders/receipts are immutable;
root/attempt lifecycle fields are the sole mutable exception.

The existing identity relation adds nullable:

```text
admitted_root_id, admitted_plan_id, admitted_attempt_id, admitted_manifest_id
```

Its existing `owner_run_id` remains the complete original owner shape. A CHECK
accepts either original (`owner_run_id` non-null; every admitted field null) or
admitted (`owner_run_id` null; every admitted field non-null), never mixed or
ownerless. Composite Organization FKs bind the admitted tuple to one exact root,
plan, attempt and manifest. Existing rows, global
`UNIQUE(organization_id, identity_hmac)`, and append-only/tombstone behavior stay.

Each existing imported-fact table (`fub_event_record_imported`,
`fub_call_record_imported`, `fub_text_record_imported`) adds the same four
`admitted_*` fields. The existing original `plan_id/attempt_id/manifest_id`
tuple is nullable only for the complete admitted tuple. An exclusive CHECK and
composite FKs permit one whole original shape or one whole admitted shape. Fact
envelope, IDs, `identity_id` uniqueness, immutable trigger, family/time meaning
and original FKs are preserved.

`migration_history_import_display` adds admitted root/plan/attempt ownership
plus an admitted-manifest composite FK. Its original `plan_id/owner_run_id`
fields are nullable only for the complete admitted shape. Its exclusive CHECK
and FKs mirror fact/identity ownership, so one bounded encrypted display relation
works for either branch without raw-payload copying. Display encryption is
`admitted-history-v1:{root}:{plan}:{attempt}:{manifest}:display`; receipts use
`admitted-history-request-v1:{actor}:{operation}`. Keep the inherited 24-byte
AEAD nonce and 4-KiB metadata display bound.

The migration replaces identity immutability, display guard/change, Person
erasure, retained-size/measurement, fact permit/count and relevant reader joins
as one compatibility change. Owner dispatch is exclusive: original rows keep
their original run charge; admitted rows charge the admitted root. Raw capture
bytes remain charged to 010d1. Erasure inventory includes all ten tables,
admitted display/identity/fact provenance, review counters/caches and synthetic
suppression.

## Authority, ordering and lifecycle

Every mutation is a current active same-Organization admin command in a
`migration_review` workspace. Members/platform admins have no access and
foreign/missing resources are opaque. The transaction-local private permit
`crm.admitted_history_import_permit` has only the fenced lease and exact
manifest unit. The fact trigger revalidates root/plan/attempt/manifest,
admission result, identity, Person, workspace, executor and display. Neither
`origin=migration`, a client header, nor outer request context is authority.

The fixed lock order is:

```text
workspace barrier -> current membership -> Organization -> source snapshot and
Organization ledger -> root -> plan/attempt -> stable identity HMAC -> Person UUID
```

Sibling units sort identity then Person keys. Existing original history work uses
the same global identity serialization point. Bounded waits use the existing
two-second timeout; leases are database-clock, 60-second and token-fenced. No
network, inference or unbounded wait occurs in a transaction.

Root states are `preparing|ready|queued|running|paused|completed|cancelled`;
plans are `building|ready|expired`. Ready expires ten minutes after readiness
and before first confirmation. Confirm freezes exact root/plan/workspace/budget
revisions, current release readiness, and `acknowledge_held=true` plus
`acknowledge_coverage=true`; it requires at least one eligible or verified
already-present unique unit. All-held previews remain readable but cannot confirm.

First confirmation holds the exclusive workspace barrier and atomically installs
the durable admitted-history predicate before any fact writes. It is independent
of the original-anchor predicate: an original anchor continues to require exactly
`crm.history_reader=fub-history-timeline-v1`; an admitted root requires exactly
`crm.admitted_history_reader=fub-admitted-history-v1`. The shared,
`ordinary`, and `read_check` DB paths enforce the applicable predicate. Both
the original/new worker unit and recovery paths acquire the shared barrier and
stamp their required values on the actual **unit transaction connection** before
the guard, so no stamp can leak across pool reuse. A durable DB write/owner guard
also rejects an unstamped affected-row or charge commit: a permit acquired by an
original worker before first admitted confirmation cannot authorize its later
unit. Legacy complete-reader denial, permit/worker admission, recovery inventory
and preflight require the admitted capability where an admitted predicate exists.
The admitted predicate persists through zero-write confirmation, cancellation,
completion and erasure, and never changes the original history anchor. Trusted
inventory retires artifacts unable to set the server-owned transaction-local
stamp on actual pooled connections; it cannot assume old code self-detects the
schema.

Resume explicitly adopts the current admin. Replay is checked before expiry/
readiness and binds Organization, actor, action, resource and canonical body;
changed input is 409. Cancellation serializes with unit commits, fences lease,
retains settled facts and releases owned reservations only. A confirmed cancelled
attempt has one successor over exactly never-settled units and cannot reopen
settled imported/already-present/held results.

## Accounting and atomic application

Preparation/execution use one bounded retained capture walk and at most 50
manifest occurrences per unit. The inherited 32-KiB per-record reservation must
cover every new manifest/candidate/result/display/identity/provenance byte before
an executable plan is accepted. The root uses the current snapshot/Organization
ledger, initial `min(2 GiB, configured run ceiling)`, existing operator ceilings
and reserved cancellation capacity. It creates no quota, key hierarchy, retention
term or storage product.

Charge once: encrypted plan/manifest/display/result/remainder/receipt material,
nonces/ciphertexts, stable/canonical HMACs, variable source identifiers,
coverage/issue/checkpoint fields and cursor/recovery metadata. Do not recharge
fixed IDs/enums/timestamps, native fact tuples or referenced capture/admission
evidence. Cancellation, discarded preparation, erasure and remainder reconcile
their root and shared ledger without touching sibling reservations.

One commit rechecks authority, lease, workspace, binding, capture/admission
evidence, target, global identity, erasure and budget; writes one fact/display/
identity tuple or already-present/held result; advances Person review revision/
counts when visible; advances checkpoint; settles reservation; and writes a
bounded receipt. Failure leaves no partial fact/display/identity/counter/result/
charge. Source-created time is `fub_record_created` or `unknown`; sent,
updated, materialized and recorded clocks retain separate labels. A source user
is never the importer and establishes no native call/consent/delivery/contact/
Today state.

## HTTP, bounded readers and Web

All admitted-history routes are under
`/api/migrations/fub/admitted-history-imports`. Bodies cap at 8 KiB, receipts
at 4 KiB, unknown fields fail, UUIDs remain UUID strings, and vendor IDs/counts/
bytes/revisions are decimal strings. Responses and errors are `no-store`.
Pages default to 25, cap at 50, and use opaque cursors bound to endpoint,
Organization, root/plan/attempt, revision, filters, limit and total-order key.

| Route | Exact request/boundary |
| --- | --- |
| `POST /` | `{request_id,admission_id,history_capture_id}`; 201 receipt |
| `GET /` | bounded roots list, optional `admission_id`, opaque cursor |
| `GET /{root}` | frozen capture/cohort/coverage/counts/actions |
| `POST /{root}/confirm` | `{request_id,plan_id,expected_revision,acknowledge_held,acknowledge_coverage}` |
| `POST /{root}/resume`, `/cancel` | `{request_id,expected_revision}`; Resume adopts executor |
| `POST /{root}/budget` | monotonic expected-revision increase; never resumes |
| `POST /{root}/remainder` | `{request_id,attempt_id,expected_revision}`; exact successor only |
| `GET /{root}/plans/{plan}/manifests\|results\|issues` | bounded cursor/filter pages, no raw content |
| `GET /{root}/remainder` | predecessor and never-settled summary |

Errors retain 401 anonymous/platform-only, 403 member, 404 foreign/missing, 400
malformed request/cursor, 409 stale/held/conflicting/expired/incompatible/release/
budget state, and 503 corrupt/unavailable retained evidence. No source data is
placed in an error.

Existing migration-review v2/timeline endpoints remain the reader contract and
keep the actual native inventory including `person_admitted`. External reads
select exactly one original/admitted identity/fact/display owner branch, require
live non-erased identity/Person, and share one Person history revision/count
snapshot with the page. Known rows sort by source-created time; unknown dates are
separately paged by stable position. Existing limits remain: 25 default/50 max,
51 candidates/family, 4-KiB summary, 16-KiB detail, 512-KiB response. A stale
cursor returns `history_refresh_required`; suppression never resurrects data.

Web adds an admitted-history panel: terminal admissions with successful results,
qualified capture interval/source access, gaps, unique/occurrence/known/unknown/
held/excluded coverage, confirmation, progress, cancel, Resume and exact
remainder. It explains that cancellation retains committed facts. It clears on
actor/Organization/workspace/root/Person changes, ignores late results, refetches
on focus/reconnect, and stops polling terminal/paused roots. It exposes no body,
subject, phone endpoint, raw payload, URL preview or automated summary. Original
history controls remain separate.

## Required checkpoint evidence

Before admitted owner rows exist, inventory every original identity writer,
fact/display/provenance reader, charge trigger, erasure/suppression writer,
startup/recovery capability and private permit. Freeze DTO/cursor fixtures and
prove original/admitted writers/readers recognize the exclusive owner shape before
first confirmation. Prove each independent reader setting on shared, `ordinary`,
and `read_check` paths, including actual pooled connection reuse, zero-write
confirmation, cancellation and erasure. Prove an old original worker paused
after claim, then first admitted confirmation, then a unit attempt: its newly
acquired unit connection takes the shared barrier and the durable guard prevents
all affected fact/display/identity/result/revision/ledger commits until properly
stamped. Coordinator owns shared
guards/grants/registrations/preflight/navigation and serialized SQLx integration.

Focused proof covers H3-01 through H3-10: cohort/capture qualification; all
classification states; original/admitted races both orders; native/capture
preservation; replay/cancel/takeover/remainder; ledger/erasure; reader revisions/
cursors; zero-write pooled-connection fences; authority/no-store/no-PII; and
populated synthetic API/Web desktop and 390px journeys. Under D-050, changed hot
queries receive one paired p95 comparison and one realistic
`EXPLAIN (ANALYZE, BUFFERS)` plan shape; review/fix work caps at two rounds.
