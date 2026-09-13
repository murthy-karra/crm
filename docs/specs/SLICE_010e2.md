# Slice 010e2 — Refresh already imported People

**APPROVED FOR IMPLEMENTATION — D-076, 2026-09-12.** The user requested continuation
after the released Mobile 001 / 010e1 milestone and deferred mobile design and
physical-phone testing. D-076 accepts this reviewed migration write scope;
D-075 owns the completed 010e1 read-only reports. Read
[AGENTS](../../AGENTS.md), D-050/D-059/D-064/D-065/D-075,
[010c](SLICE_010c.md), [010e1](SLICE_010e1.md) and the
[execution brief](../tasks/SLICE_010e2_IMPL.md). Approval of this spec would own
the contracts below and isolated synthetic implementation, not a live import or
release. [Planning review](../tasks/SLICE_010e2_REVIEW.md) records readiness.

## 1. Outcome and scope

An Organization admin selects a completed 010e1 report, previews how its newer
retained capture would change People already successfully imported by the bound
010c parent, and confirms that exact plan. Eligible updates apply once; conflicts
and gaps remain visible. The workspace stays in `migration_review`.

Included: first/last name, email/phone collections and their primary order, stage,
and assignee. Stage/assignee resolution reuses the parent's original confirmed
choices and currently valid destination targets. This is a refresh of that
mapping, not a mapping editor. No new stage/member or Person is created.

Newly observed or previously held People, new mapping choices, notes, tasks,
tags/custom fields, timeline facts, source deletion, Person merging/erasure,
activation, outbound actions and live FUB work remain separate. Show these
exclusions and their counts; this rung does not complete delta fidelity or cutover.
Read retained evidence only; the worker has no source reader or credentials.

## 2. Declared contract changes

| Current contract | Proposed contract and reason | Affected components and compatibility |
|---|---|---|
| 010e1 compares two source boundaries and writes no CRM data | Separate immutable People-refresh preview, confirmation and bounded execution | Additive Rust domain, HTTP, worker, PostgreSQL and admin Web feature; existing report routes and meanings unchanged |
| 010c is a terminal INSERT-only import with immutable plan/identities | New private typed permit updates only verified existing People and their owned contacts | Workspace DB guard and permission inventory must explicitly cover exact tables/operations; no generic review-mode write bypass and no reopen of 010c |
| Original import provenance is the sole imported baseline | Append-only refresh results plus a per-Person last-settled baseline and contact ownership | New tenant/account/parent-scoped state; original plans, results and identity tombstones remain unchanged |
| Report lists expose metadata only | Separate bounded current/before/proposed field preview to current Org admins | New authorized value read, encrypted retained preview, no raw-payload viewer or new member/Operator exposure |
| Existing workload capability inventory has no refresh writer | Add independent `fub-people-refresh-v1` capability covering execution and readers | API/admin/worker/migrator preflight and recovery inventory; old report/mobile clients keep their contracts; incompatible writers cannot recover this work |

This spec and its brief own the new HTTP/persistence/private-write contracts.
Concrete DTO names, schema constraints, digest encoding and lock tokens are
implementation details to freeze before parallel Web work. A changed ownership,
clear/removal, conflict or audience policy requires an amendment, not silent code.

## 3. Frozen inputs and eligibility

Require a currently active Org admin, the current review workspace's completed
010c import and exact confirmed parent plan, and a sealed completed 010e1 report
for that parent/account. Freeze the report output revision, both source boundaries,
newer capture interval/representation, import engine, inherited mapping choices
and refresh engine `fub-people-refresh-v1`. Original workspace binding is immutable.

Requalify exact retained raw evidence using the approved 010c/010e1 lossless rules:
successful nontruncated captures, exact IDs/ordinals/representations, all relevant
observations, semantic HMACs, and qualified supporting user/stage evidence. No
clipped display projection, representative evidence subset, name/contact match,
timestamp-only shortcut or last-variant winner may authorize a write. Corrupt
evidence/missing keys pauses work; unsupported or ambiguous records are held.
Preserve report coverage/access warnings. Missing/partial evidence is not deletion.

Only a successful original `imported` result with a matching immutable source
identity and existing same-Org Person can seed refresh ownership. Missing targets,
identity disagreement, erased/unprovable provenance or ambiguous contact ownership
are held. Never recreate a target or adopt a lookalike. People held by the original
import remain held even if the newer source now looks valid.

Source stage/assignee keys must resolve through a qualified original confirmed
choice. An original approved stage creation resolves to its actual created target;
it does not authorize another creation. Recheck stage identity/name and active
member identity/email against the original confirmed target snapshot when planning
and committing, preserving 010c's existing mapping checks. Unknown/new keys or invalid targets hold
the whole Person. Explicit originally approved unassigned/missing-stage choices
remain usable; arbitrary source labels or matching member email confer no mapping.

## 4. Last-settled baseline and conflicts

010e1 always compares against the original capture. Therefore examine **all People
groups, including `unchanged`**, and compare the newer desired native projection
with the last settled refresh baseline. A source change back to the original value
must be detected after an intervening refresh.

The initial baseline comes from the original successful executable projection,
resolved stage/assignee targets and exact contact IDs/order in committed 010c
provenance. Later baselines and ownership come only from atomic settled 010e2
results. Store a durable per-Person revision pointing to those immutable results;
do not rewrite the original baseline or relabel old reports.

Let B be that owned baseline, N the newly qualified instructions overlaid on B,
and C the current native projection. Compare complete owned names, stage, assignee and
contacts with row identity, value/normalization and order. Ignore unrelated
notes/tasks/history when determining equality.

| Case | Outcome |
|---|---|
| C equals B and N differs | Eligible update, including explicitly previewed source clears/removals below |
| C equals B and N equals B with ownership intact | `already_current`; no business mutation or new stage/assignment fact |
| C differs from B, even if C equals N, or owned contact identity is missing/replaced | `held_local_change`; no field of this Person changes; do not adopt a local edit because its value matches the source |
| Unsupported, ambiguous, missing or out-of-scope input | A closed held/excluded reason; no target mutation or baseline advancement |

Any native contact without proven import/refresh ownership holds the Person;
do not merge, adopt or delete it even if its value matches N. The initial version
uses an atomic Person unit rather than partial per-field conflict resolution.
There is no force-overwrite or conflict-resolution button in this slice.
This protects observable current divergence. Existing data cannot prove that a
value was never edited and then reverted; do not fabricate that history from
the broad mobile revision, which also changes for notes/tasks.

At execution, under locks, recheck both the frozen baseline revision and the
previewed destination fingerprint, including contact IDs/order and mapping target
eligibility. A changed destination or predecessor since preview yields a held
stale/conflict result rather than silently regenerating the confirmed plan.
Unchanged/`already_current` results may advance only their exact qualified source
baseline/ownership; held/excluded results never do so. No-op receipts record this
distinction without inventing business changes.

Serialize one nonterminal refresh per parent. Freeze a monotonic confirmed source
boundary for the parent; a different capture must begin after the last confirmed
capture completed. Reject older/overlapping inputs. The same frozen input can be
retried after cancellation through a new explicit preview using surviving
per-Person checkpoints. Exact request replay returns its old receipt. Partial
cancellation cannot reset the boundary or roll already updated People backward.

## 5. Field application and provenance

Names/contact normalization and source flags retain 010c interpretation. The
following presence rules are specific to this refresh engine and do not change
old 010c extraction. Every `no_instruction` component preserves its B value and
prior field provenance; show/count it without claiming it was reconciled from
the newer source. Other qualified instructions may proceed. An invalid supplied
component, Trash, or an unqualified Person holds the whole Person.

| Component | Instruction and explicit clear rule |
|---|---|
| `firstName` / `lastName` | Present valid string replaces the name; present null or whitespace-only string proposes a visible clear under 010c normalization. Missing key is `no_instruction`; invalid type/NUL holds. |
| `emails` / `phones` | A present qualified complete array replaces that owned kind; explicit `[]` may remove that kind. Missing key or null collection is `no_instruction`. Any invalid nonempty element holds the Person; never execute a filtered partial array. |
| `stage` | Present valid source stage uses its original confirmed mapping. Present null/blank may use the original approved `missing` mapping, otherwise hold. An absent key is `no_instruction`; stage never becomes null. |
| Assignment | A qualified positive `assignedUserId` or `assignedPondId` uses the original choice; conflicting/invalid references hold. Explicit clear requires both reference keys present null and `assignedTo` absent/null. With no positive reference, missing either key is `no_instruction`; a conflicting `assignedTo` shape holds. |

Require the resulting Person to retain a displayable name or usable contact,
as in original import qualification. Clears/contact removals are separately
visible/countable in the exact confirmation. An absent Person in the newer
capture never deletes a Person or its contacts. These conservative interpretation
rules are a proposal over retained evidence, not a claim about live FUB responses.

Contact refresh is replacement of the **proven owned collection**, not a
whole-Organization match. Retain existing IDs for contacts with the same kind and
normalized value; update representation/order only when the complete old row
matches the baseline. Allocate deterministic-in-plan IDs for additions. Remove
only obsolete rows named in that owned baseline and exact preview. Do not reuse
original IDs after removal. Preserve original and each applied collection in
encrypted provenance; old source contact maps remain historical rather than
being changed to point at replacement rows. No generic contact DELETE/UPDATE
authority is added for ordinary application callers merely to support refresh.

One short transaction atomically applies all eligible Person/contact changes,
updates ownership/baseline, appends result/receipt and necessary encrypted
before/after provenance, advances progress and settles reserved bytes. Rollback
leaves none of these half-applied. Use existing typed application/domain seams
with a private refresh permit; HTTP/Web never receive the permit.

Real stage/assignment transitions append existing IDs-only facts with
System/Migration attribution, initiating admin, actual local commit time and
refresh correlation. Preserve original `person_imported`, creation time/source
attribution, notes/tasks and historical facts. Do not invent inquiry/contact
attempt credit, source event times or operational notifications. Existing Person
revision triggers continue to invalidate mobile projections; no native wire change
or separate business command implementation is introduced.

## 6. Preview, confirmation and recovery

Use durable states `preparing`, `ready`, `queued`, `running`, `paused`, `completed`,
`cancelled`. Preparation freezes one immutable plan and reconciliation counts.
A ready plan expires after ten minutes; explicit re-preview creates a new revision
without editing the prior plan. Confirm binds actor/Org, exact plan/revision/digest,
coverage/exclusion acknowledgments and exact counts of clears/contact removals.
All-held/no-native-change plans remain inspectable but cannot queue a write run.

Report separately: eligible updates, already current, held conflicts, evidence/
mapping gaps, source-only/excluded changes, and not-seen-again People. Show every
clear/removal before confirmation. The user can cancel and re-preview; no hidden
auto-apply. A completed refresh means its planned units settled, not that every
record was updated or the whole account is reconciled.

Worker leases use existing fencing/lock ordering and current-authority checks on
each bounded commit. The initiating admin owns confirmation/resume; any current
Org admin can read/cancel. If the initiator loses access, pause; another admin
can cancel and explicitly prepare a new run, not adopt its old authority.
Cancellation fences future writes and retains committed units, receipts, raw
evidence, baselines and the review hold. Resume a paused job only after explicit
action and current capacity/capability checks. Stale workers cannot settle work
or release another lease's reservation.

Actor/Org/action-bound request IDs and normalized input digests cover prepare,
re-preview, confirm, retry and cancel. Exact replay returns the same resource or
receipt; altered actor/action/body conflicts. Every settled Person result has a
unique logical identity so server acceptance followed by response loss cannot
reapply a write or duplicate a fact.

Preparation uses keyset chunks of at most 50 descriptors and one qualified raw
capture at a time, with at most 16 MiB input per unit. Oversized captures/items
must pause or be explicitly held with retained originals, never truncated into
an executable subset. Execute one Person per transaction with a proven retained
byte bound no larger than the existing 64 MiB unit ceiling. Reserve before
work; charge preview/provenance/results/checkpoints/cursors/receipts exactly once
to the newer snapshot and Org ledger. Use refresh-owned reservations and a bounded
cancel reserve. Lower policy ceilings still clamp admission; storage shortage
pauses before native writes. No parent/sibling/source reservation is released.

## 7. HTTP, Web and compatibility

New routes live under `/api/migrations/fub/people-refreshes`. All reads/writes
require current admin, trusted Org and matching workspace/parent; enforce normal
session/CSRF rules, non-disclosing foreign-resource errors and `no-store` on
success and failure. Plaintext previews contain only the owned fields; no bodies,
arbitrary source paths or raw payloads. Escape all displayed content.

| Route | Proposed bounded contract |
|---|---|
| `POST /` | `{request_id, report_id}` prepares one frozen plan |
| `GET /` | Required `parent_import_id`, opaque cursor; at most 20 runs / 128 KiB |
| `GET /{id}` | Input/plan revisions, expiry, progress, counts/reasons and available actions |
| `GET /{id}/items` | Closed disposition filter and opaque cursor; at most 50 summaries / 256 KiB |
| `GET /{id}/items/{item_id}` | Current/baseline/proposed scalar fields, outcomes and contact counts; bounded 128 KiB |
| `GET /{id}/items/{item_id}/contacts` | Paged before/current/proposed contact diff; at most 50 rows / 256 KiB |
| `POST /{id}/plans` | `{request_id, expected_plan_revision}` explicitly re-previews unconfirmed work |
| `POST /{id}/confirm` | Request ID, exact plan/revision/digest and counted acknowledgments; queues eligible changes |
| `POST /{id}/retry`, `POST /{id}/cancel` | Request ID and expected lifecycle revision; preserve settled units |
| `GET /{id}/results` | At most 50 immutable settled results / 256 KiB with Person links and closed reasons |

Use scalar/provenance fragments if required to stay within existing authorized
text bounds; a clipped display value cannot be submitted as an executable value.
Cursor binding includes trusted scope, run/plan/output revision, endpoint and
filters. Publish immutable preview pages only when preparation seals. Results
use a fixed upper settlement key per traversal; refresh begins a new traversal
for newly committed results. Org/actor/role changes fence late Web responses and
clear cached values. No unbounded contact or report array may be loaded for UI.

Capability preflight covers exact running API/admin/worker artifacts, additive
schema and the refresh-aware private guard, retained reads, byte inventory and
recovery. Confirmation requires fresh existing operator-owned release evidence.
Original imported-People and report/child readers remain compatible and bounded;
hold the workspace barrier through protected response assembly. Contact/stage/
assignment mutation must not mix a response from before/after a transaction.
Retire incapable writers before activating this capability; never drop the hold
to permit an old binary. No rollout or schema change is performed during planning.

## 8. Required proof

Use isolated synthetic retained captures and the actual PostgreSQL/API/Web paths:

1. Names, reordered/added/removed contacts, explicit name/assignee clears and
   mapped stage/assignee changes; exact before/after native and provenance audit.
   Missing fields/Person and restricted/partial source coverage never cause clears.
2. Untouched baseline, local edits, unowned/equal-looking contacts, missing owned
   IDs/targets, tombstones and invalidated mappings; whole-Person atomic holds.
3. First refresh, replay, interrupted partial refresh, same-source re-preview,
   newer refresh and source reversion to the original value; older input rejection
   and no accidental reliance on the report's `changed` filter.
4. Current actor/Org/account/workspace guards, demotion races, forged IDs/permits,
   direct review-mode write denial and cross-tenant targets. Original parent,
   siblings, tombstones, native note/task state and review binding stay unchanged.
5. Lost response, request-body mismatch, crash before/after atomic settlement,
   stale lease takeover, cancel/resume, storage exhaustion and byte ledger recovery.
6. Preview/confirmation race and expiry, stable bounded pages, complete contact
   traversal and no stale value leaks after context changes. Actual desktop/390px
   Web preview/confirm/reload/conflict/recovery with visibly synthetic data.
7. Existing Person/mobile revision behavior and ordinary note/task/Today/report
   contracts remain intact; changed hot-query plans and paired reader regressions
   stay within D-050. One representative 25k-People plan pass and appropriate
   final repository/SQLx/DB gates; at most two bounded review/fix rounds.

No application test, migration or runtime success is claimed by this draft.
