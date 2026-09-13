# Slice 010e1 — Core change assessment

**APPROVED FOR IMPLEMENTATION — D-075, 2026-09-12.**
D-075 accepts the reviewed contracts and isolated synthetic implementation.
D-074 already approves Mobile 001 and the coordinated parallel development. Read the [execution brief](../tasks/SLICE_010e1_IMPL.md),
[migration ladder](../plans/SLICE_010_LADDER.md) and D-050/D-063–074 before implementation.

The [bounded review](../tasks/SLICE_010e1_REVIEW.md) is READY FOR MIGRATION
CONTRACT APPROVAL after the publication/pagination correction and targeted recheck.
The user subsequently approved it under D-075. Implementation verification and
the authorized [milestone release](../tasks/MOBILE_001_010e1_RELEASE.md) are recorded
separately from this frozen specification.

## 1. Outcome and boundary

An Organization admin can see what differs between the core capture behind its
completed 010c People import and a newer retained core capture. A durable, paged
report identifies unchanged, newly observed, changed, not-seen-again and unresolved
source records across People, users, stages, custom fields, notes and tasks.
This supplies evidence for later repair and final reconciliation; it does not
apply a delta or establish that the destination is ready for cutover.

Use the existing 010b proposal/confirmation/capture workflow for a new full core
capture. Additional completed snapshots are already supported; only active source
runs are exclusive. This slice adds no FUB request or incremental filter. Its
report worker consumes retained evidence only and cannot obtain a source reader
or credentials. Synthetic capture/report verification is in scope; live FUB and
customer-data work remain user-deferred under their existing readiness gates.

The baseline People plan, sibling imports, identities and review binding remain
immutable. No business records, assignments, source mappings, history facts or
Today projections change. Deletion, overwriting local edits, repair execution,
history comparison, new source families and activation require later scopes.

## 2. Declared shared contracts

Under D-075, the assigned owner freezes exact names, DTO serialization,
indexes, lock order and error codes within these policies before coding. A policy
change requires a reported amendment. This table covers current/proposed contracts,
reason, affected components, compatibility and the owning specification/brief.

| Current contract | Proposed contract and reason | Components, compatibility and owning scope |
|---|---|---|
| 010b exposes independent captures and destination previews | Source-to-source report freezes two retained boundaries and a comparison engine; destination preview is not a delta | Additive Rust report modules, tables and admin Web panel; preserve all 010b bodies/profiles. This spec §§3–7 and its brief own the addition |
| 010c binds one completed People import and original snapshot | Report references that unchanged binding plus a newer same-account core capture | Composite tenant references; no parent plan/state rewrite or new import eligibility. §§3–4 |
| Stored projections are clipped; semantic hashes are representation-scoped | Requalify raw evidence and compare complete observations with explicit uncertainty | New retained-only interpreter; existing import extractors remain compatible. §§3–4 |
| Retained-byte allowances/reservations have separate owners | Report-owned reservations and exact variable-byte charges use the newer snapshot and existing Org allowance | Additive owner accounting; cannot release source/preview/import reservations or raise policy ceilings. §5 |
| Migration routes and workers have current-role and recovery guards | Add report lifecycle/read routes, engine admission and observed-artifact capability `fub-core-change-v1` | API, worker registration and release inventory/preflight; no mobile/native command changes. §§6–7 |

## 3. Frozen inputs and source qualification

Require a completed 010c parent in the current `migration_review` workspace and
its exact original snapshot/final sequence. The comparison capture must be a
different completed or completed-with-gaps `fub-core-v1` run in the same trusted
Organization/source account, with the same schema/profile/representation contract.
Its first source capture must follow the baseline's completion; overlapping runs
are ineligible for this first "since the original import" report. Freeze both
final sequences, capture intervals, stream states/totals and comparison engine.

Decrypt and qualify retained identity responses from both runs. Record whether
source-user identity/access evidence is consistent, changed or unknown. Account
mismatch or contradictory identity evidence blocks preparation; a different source
user or incomplete permission evidence yields an explicit scope warning. Even
consistent identities do not prove unchanged effective permissions or complete
account access. Current connection credentials are irrelevant to retained reads.

Use exact successful nontruncated raw captures, representation, ordinal, source
ID and recomputed semantic HMAC. Lossless parsing must reject duplicate decoded
keys, invalid encodings and excessive structure. Do not use display projections,
source timestamps alone, names, email or phone as equality/identity. Source IDs
remain exact positive decimal strings within the existing 128-digit bound.

Every relevant observation at each frozen boundary participates. Invalid IDs are
reported as unresolved observations keyed by capture/ordinal, not fabricated IDs.
Rejected/corrupt evidence cannot silently select an accepted winner. Missing keys
or corrupt ciphertext pause the job; valid unsupported evidence is a reported
uncertainty. Preserve missing/null, array order and unknown properties. Reuse
existing lossless canonical semantics and identify their engine version.

## 4. Comparison meaning and counts

Group by source family/ID and comparable representation. Equal repeated
observations within one capture collapse with their count/evidence retained;
disagreeing variants are unresolved. Never select the last observed variant.

| Disposition | Exact meaning |
|---|---|
| `unchanged` | Both sides have qualified comparable evidence with identical canonical semantics |
| `changed` | Both sides have qualified comparable evidence with different canonical semantics |
| `newly_observed` | Qualified evidence is present only in the newer capture; creation time is not inferred |
| `not_seen_again` | Qualified baseline evidence is absent from the newer enumeration; deletion is not inferred |
| `unresolved` | Ambiguous, invalid, incompatible or incomplete evidence prevents the comparison |

One primary disposition per qualified family/ID prevents double counting. Missing
or incomplete relevant enumeration produces `unresolved`, not one-sided presence.
Scope changes/unknown access remain attached to each one-sided row and the report.
Source-only absence is never a deletion instruction, even with exhausted streams.

Notes keep list and enriched detail as separate components. Do not fill missing
detail from list content; a restricted detail 404 is inaccessible, not deletion.
Either component's uncertainty makes the entity unresolved, while known component
observations remain inspectable. Tasks combine open/completed partition membership
under the shared task representation: movement between partitions is a task
change; inconsistent duplicate variants within a run remain unresolved.

Report closed changed-field categories and counts, such as contact information,
assignment/stage, embedded tags/custom fields, note content, task status/due date
and other preserved properties. Do not return bodies, values, names, labels,
source URLs or arbitrary field paths through these new routes. Opaque report-row
IDs, source IDs, capture references and closed reasons support later repair.
No new plaintext evidence viewer or export is introduced.
Provide links into existing authorized snapshot/import review so an admin can
identify the affected Person or source record. Those destinations recheck their
own read authorization; report references never grant access or expose a raw dump.

Summary counts reconcile source-ID dispositions, invalid observations, duplicates
and per-stream coverage separately. `completed` means report computation finished;
it never means complete account coverage, completed repair or cutover readiness.

## 5. Durable work, storage and recovery

Create a report through a typed command with actor/Org-bound request-ID receipt;
digest the parent, both frozen boundaries and engine. Exact retries return the
same job; changed inputs under the same ID conflict. Reuse an existing active or
completed report for the same tuple. After cancellation, a new explicit request
may create a fresh report; it does not revive the cancelled job or erase evidence.

States are `queued`, `running`, `paused`, `completed`, `cancelled`. A worker claims
a fenced lease and revalidates current active admin, workspace binding, inputs,
capability and storage before each bounded commit. Expired leases are reclaimable;
stale workers cannot commit or release another lease's reservations. Resume only
paused work, retaining checkpoints. Cancellation fences in-flight work, preserves
committed rows and is idempotent; completed/cancelled states remain terminal.
Keep partial comparison rows internal. After all input/group work finishes,
atomically seal the final rows and reconciled counts with an immutable output
revision and transition to completed; no comparison row may change after sealing.
The initiating admin owns execution/resume authority; any current Org admin may
cancel or read a report. If that initiator loses access, another admin can cancel
the paused job and explicitly request a new one without adopting its authority.

Admit at most one queued/running/paused report per Org. Use keyset traversal and bounded staged
comparison, not a whole-account in-memory join or repeated raw-page rescans per
record. Freeze units at no more than 100 observations and 16 MiB raw input; rows
with greater grouping fan-out must use resumable aggregation or become explicitly
unresolved with all originals retained, never a silently selected subset.

Charge encrypted report projections/checkpoints/cursors/receipts and variable key
bytes exactly once to the newer snapshot and existing Org allowance. Account for
references to both retained inputs in erasure/retention inventory. Reserve before
work under existing Org/snapshot admission locks, with a separate bounded cancel
reserve; no parent reservation may be released. Storage exhaustion pauses work
and allows cancellation. Current policy ceilings clamp all admissions; the
existing explicit budget workflow may increase an approved allowance, not silently
raise a ceiling. No report/body duplication bypasses accounting.

## 6. HTTP and admin workflow

All routes are under `/api/migrations/fub/core-change-reports`, require current
active Org admin, enforce trusted Org/workspace context and return `no-store`,
including errors. Use existing same-origin/session/CSRF rules. Cross-tenant or
unowned IDs return the established non-disclosing response.

| Route | Contract |
|---|---|
| `POST /` | `{request_id, parent_import_id, newer_snapshot_id}`; server resolves/fixes baseline and returns report ID/state and immutable input summary |
| `GET /` | Required `parent_import_id`, optional cursor/limit; up to 20 reports and 128 KiB, newest first, including prior cancelled/completed jobs for reload/navigation recovery |
| `GET /{id}` | State, input intervals/profile/scope, coverage, reconciled counts, progress, pause reason and available actions |
| `GET /{id}/rows` | Optional closed family/disposition and opaque cursor; up to 50 rows and 256 KiB with `next_cursor`; no complete collection response |
| `GET /{id}/rows/{row_id}` | Bounded component dispositions, closed change categories/reasons, up to 16 representative evidence references and total observation counts; never claim those references are exhaustive |
| `POST /{id}/resume`, `POST /{id}/cancel` | `{request_id}` with actor/body-bound idempotent control receipt; current state/authority rechecked |

Cursors bind report/engine/frozen inputs/published output revision/filter and reject cross-report reuse or
tampering. Report-list cursors instead bind the parent and first-page upper key;
new jobs appear on refresh without destabilizing an existing traversal.
Row/detail reads require the sealed completed output and reject unavailable/stale
output, avoiding unstable paging through computation. Other states expose bounded
committed progress counts only, including cancelled jobs with preserved partial rows.
UI offers the existing recapture workflow separately, eligible retained
capture selection, prior-report list, "Assess changes", progress/recovery and bounded filtered rows.
Scope/absence warnings remain next to the affected counts; no "apply" or activation
button exists. Role/Org/workspace changes fence late responses and clear cached
report data using existing migration UI patterns.

## 7. Compatibility and acceptance

Add schema/engine capability and additive report inventories to existing observed-
artifact preflight. Report creation requires capable API/worker artifacts; retained
reports require compatible recovery. Old clients and original import commands keep
their contracts. No capability or namespace grants a review-mode business write.
Telemetry contains only safe IDs, closed state/reason and numeric counts/bytes.

Required proof: isolated synthetic dual captures through the real existing
collector plus report worker/API/UI; exact disposition/count reconciliation;
cross-Org/account/actor denials; changed access/partial stream/no-false-deletion;
unknown-field changes, duplicate/conflicting variants, inaccessible note detail,
task partition transitions; corruption, lost response, cancel/resume, independent
worker handoff and stale lease fencing; budget races and paged bounded reads.
Prove that paging during worker progress cannot expose, skip or reclassify rows,
and that final summary counts and all published pages share the sealed revision.
Assert no canonical CRM writes, changed parent plans or workspace activation.
Use one D-050 plan pass at 25k People with representative notes/tasks, relevant
regressions, required repository gates and at most two bounded review/fix rounds.

The inherited core snapshot process-UUID handoff concern gets one independent-
process reproduction during the recapture check. If demonstrated, make only the
narrow compatible progress/fencing fix needed here; if not, record the actual
evidence. Do not claim that the earlier concern was already reproduced or fixed.

Public source evidence: [pagination](https://docs.followupboss.com/reference/pagination)
and [common filters](https://docs.followupboss.com/reference/common-filters) describe
bounds and why related changes cannot be derived from Person `updated` alone.
The [core source record](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md) retains
endpoint qualification gaps; [note restrictions](https://docs.followupboss.com/reference/notes-id-get)
explain why 404 does not prove deletion. These are documentation findings, not
live-account verification. Standalone tags and other uncovered families remain
separate; no qualified standalone tags endpoint is assumed.
