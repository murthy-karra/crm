# 010e6 — Concrete implementation contract

**ACCEPTED SCOPE — D-090.** Concrete compatible encodings owned by the accepted
[specification](../plans/SLICE_010e6_NEVER_IMPORTED_RECOVERY.md). One primary writer
owns all SQL and shared readers; no independent implementation lanes.

## Modes, owners and engine

`migration_people_admission.mode` is `ordinary` by default or `mapping_recovery`.
Ordinary uses `fub-people-admission-v1`; recovery uses `fub-people-recovery-v1`.
An enforced mode/engine pair prevents accidental reinterpretation of old roots.
Recovery root has exactly one positive anchor: its parent's confirmed original
plan, or an admission ID plus exact anchor plan. An explicit remainder flag is
valid only for a confirmed cancelled recovery with that same source report.

New stores: `migration_people_recovery_candidate`, `_key`, `_choice`. Their owner
is the real admission root, never an invented refresh. Composite FKs include
Organization/root/plan as applicable. Original candidate manifest versus admission
candidate item is XOR; each must match the root's declared owner. Candidate source
IDs have existing exact positive-decimal bounds. They retain encrypted anchor
evidence and source semantic hash, not customer text in logs/history.

Choices are immutable versions per root/kind/exact source-key HMAC. Keys use the
existing `SourceKey` representation; choice target snapshots use current stage
name or active member/email and remain immutable. Dispositions: existing stage,
member, explicit unassigned, or hold. An omitted edit retains the prior choice;
no implicit target is selected. Editing takes an expected draft revision. Sealing
freezes a revision and digest before any full Person preview is prepared.

Admission items have explicit nullable recovery stage/assignee choice references.
Each mapping owner is exclusive original or recovery; a supplied recovery choice
must be owned by that root, appropriate kind and frozen revision, and match the
candidate source key. Source absence/no instruction remains distinct from a
qualified explicit unassigned approval. Original nullable references retain their
old meanings. Completed recovery approvals cannot be replaced in place.

## Lifecycle and requests

POST `/people-admissions/recoveries` has `request_id`, `report_id`, typed anchor
(`original` parent import/plan, or `admission` admission/plan), and expected anchor
revision. The server derives Organization/account and candidate selection. A ready
unconfirmed admission preview is retired atomically; retained evidence stays.
GET/POST `/{id}/recovery-mappings` use scoped paging and strict versioned choices.
POST `/{id}/plans` seals choices and prepares the complete Person preview. Generic
Retry cannot bypass the awaiting-choices pause. Existing ordinary request shapes
stay valid; recovery confirmation additionally acknowledges frozen mapping digest,
candidate/contact/explicit-unassigned counts. Missing acknowledgements fail closed.

Replay is actor/Org/action/request scoped and checks the exact request digest.
Reusing a request ID with different inputs is Conflict. Unknown fields/bad shapes
are InvalidInput; foreign resources are NotFound; wrong role is Forbidden;
stale plan/revision is Conflict; incompatible release is ReleaseNotReady. Retain
the existing route-to-HTTP mapping and CSRF/no-store/access-lost semantics.

The existing admission worker qualifies retained streams once, discovers positive
held candidates through selective paged anchors, pauses for choices, then prepares
only the materialized candidate page. No per-Person full-book scan. Remainders copy
only unfinished eligible candidates and frozen approvals. Older held evidence is
not a license to widen a remainder or reuse an obsolete source boundary.

## Proof, serialization and atomic settlement

Lock order follows admission's bounded workspace/parent/root → plan/item → exact
identity/target → membership and owned capacity reservation. Freeze source and
ordering proof, then recheck it at preview/confirm/commit. First recovery with no
previous admission uses the qualified later report relative to the original
completed import; otherwise use the current confirmed boundary or a strictly later
capture. Same-boundary recovery is explicitly anchored, never ordinary admission.

Database guards verify root/mode/workspace/admin/live lease/item/frozen choices,
candidate ownership, source IDs and target validity. The same source identity key
serializes original/admission/recovery outcomes; retained successful results without
an identity are a hold. Native creation and all result/identity/provenance/fact/ledger
writes are one transaction. A competing success is requalified, not blindly treated
as success on SQL error. Expired ownership can be reclaimed with a new token/epoch;
stale tokens cannot finish. Cancellation retains committed results.

Create an IDs-only `person_recovered` fact with admission/plan/item/result and
original hold linkage. Use the normal history envelope and append-only guard.
No `person_admitted` fact mislabels recovery as newly observed; initialization
stage/assignment facts refer to the recovered fact and a recovery reason. Operational
contact clocks remain null. Provenance contains the exact recovery mode/anchor.

## Reader and family inventory

- Admission commands/store/query list/detail/provenance and parent boundary ordering.
- Admission source classification, target recheck, settlement and database permits.
- Admitted refresh source/baseline initialization and mapping instructions.
- 010e5 `Selected`/frozen mapping evidence, initial approval fallback, DB approval
  validation, item references, successful repair persistence and retained-byte audit.
- Metadata/activity/history cohort selectors and every result/global-identity join.
- Person profile/provenance/history and bounded original/admitted-family UI labels.
- Startup workspace flags, artifact inventory, all existing schema fingerprint
  readers, release preflight and CLI/migrator compatibility.

Resolution precedence for a recovered Person's matching source key: explicit
current repair choice → successful matching 010e5 binding → successful recovery
initial approval → valid original mapping only when no recovery approval exists
for that key. A selected invalid approval cannot fall through. No recovery choice
is copied into an original or refresh approval table to satisfy a foreign key.
Follow-on families accept only a terminal cohort's successful committed recovery
results, applying their existing source/time/erasure/mapping rules unchanged.

## Bytes, limits and compatibility

Admission's variable-byte audit gains the new candidate/key/choice encrypted
nonce+ciphertext, exact source-ID/source-key hash and frozen digest bytes. Each
copy is charged to its actual owner once; replacing a checkpoint debits only its
size difference. Recovery evidence referenced by a later refresh is not recharged
as if newly created; newly serialized refresh evidence is charged to that refresh.
Reserve before insert/replace and release unused capacity atomically. Receipts
retain owned control capacity so cancellation remains possible. No quota increase.

Use specification page/body/field/item bounds. Durable workspace capability
`fub-people-recovery-v1` is installed before first recovery confirmation and survives
cancellation, zero writes and erasure. Old already-running readers/writers fail
closed through the capability fence; exact current schema/functions/FKs/grants are
inventoried. Source and history profile/version rules remain supported-only.

## Verification ownership

Primary runs isolated service-free, serial DB, real API/Web, failure/retry/identity,
ledger and all family journeys, plus D-050 plans and one paired regression. One
authorized reviewer independently reviews implementation, maximum two rounds.
No current or later release is implied by implementation acceptance. Evidence and
known limitations will be recorded in `SLICE_010e6_VERIFICATION.md`.

## Final compatible encodings

The retained stage catalog (`migration_people_recovery_catalog`) is built once per
root, and exact stage-key lookups are bounded. Native-stage-creation restrictions
(long labels or NULs) do not prohibit an explicit mapping to an existing stage;
other unsupported stage evidence remains held. Each immutable choice extends a
32-byte draft chain digest; sealing binds root, candidate count, draft revision and
chain without scanning all choice rows in a command transaction.

Mapping GET accepts `cursor` and `limit` (1–50). The authenticated encrypted cursor
uses the existing admission cursor envelope, binding actor, Organization in AAD,
root, endpoint, lifecycle/draft revision, digest and limit. A completed candidate
catalog fixes traversal membership. Mapping responses are ≤128 KiB; source keys
show a UTF-8-safe 1 KiB prefix plus decimal `source_key_bytes` and
`source_key_truncated`. GET `/{id}/recovery-mappings/{key}/field` returns at most
16 KiB of the source key with its own scoped cursor. No long key is silently lost.

Recovery detail includes `coverage.follow_on`: four cohort-qualified entries
(`core`, `metadata`, `activity`, `history`) with `status`, latest `root_id`,
`run_state`, prerequisite, and read-only handoff IDs. Status is `not_started`,
`partial`, `held` or `completed` for the **latest retained attempt**, not a claim
that all source data has migrated. Exclusions and remainder attempts remain
conservatively partial. Handoff opens the exact terminal cohort and existing root
when present; it never submits a preparation or confirmation command.

Final inventory covers exact function definitions, validated composite constraints,
owned trigger wiring/enabled states, append-only table grants and required valid
indexes. Any missing component fails the recovery release check.
