# Slice 010e5 — Repair stage and agent mappings for existing People

**IMPLEMENTATION ACCEPTED — D-088, 2026-09-15.** After D-087 planning, the user
approved execution in Lavish and authorized one reviewer. Complete independent
planning review before code. Existing People come first; never-imported People
follow separately. Compatible review corrections are owned implementation work.

[Plan](../plans/SLICE_010e5_MAPPING_REPAIR.md) ·
[Execution brief](../tasks/SLICE_010e5_IMPL.md)

## 1. Outcome

An admin opens a blocked ready preview or terminal People refresh with mapping holds,
chooses an existing CRM stage or active Organization member for the unresolved
source value, previews the complete previously blocked core update, and confirms.
The system processes eligible existing People safely and reports remaining holds.

Cover both original-import People (010e2) and successfully admitted People
(010e4), one existing refresh cohort per repair. A cohort is the fixed group
identified by an original import or one successful admission run. A cancelled
admission contributes only its committed successful People.

This is a mapping repair followed by a full core refresh: names, contacts, stage
and assignee may change when explicitly present in the preview. Preserve 010e2's
whole-Person conflict rule. No force overwrite of local edits is introduced.

First-slice mapping choices:

- Stage: explicitly select an existing same-Organization stage. Never a null stage.
- Source agent/pond: explicitly select an active same-Organization member, or
  explicitly choose unassigned with a counted confirmation acknowledgement.
- Retain a visible unresolved choice; it produces a hold, not an inferred match.
- No automatic name/email matching, stage creation, invitation or account creation.
  A missing destination remains a setup prerequisite. New catalog creation inside
  repair is a later extension; this does not revoke 010c's existing capability.

Never-imported People, arbitrary reassignment of successfully refreshed People,
local-edit resolution, other-family mapping/refresh, deletion inference, merges,
live source I/O, activation and worker-platform changes are outside this slice.

## 2. Inspected implementation and required change

Inspected main `4a01fc080d93cee959263e10f0d74fc4d64614f1`:

- `people_refresh_worker.rs` and `admitted_people_refresh_worker.rs` resolve and
  validate stage/agent choices against `migration_import_mapping` from the original
  confirmed parent plan. Both record `held_mapping_gap` when qualified resolution
  fails. Target validation includes frozen name/email and active membership.
- Original refresh seeds its baseline from successful `migration_import_result`
  and later refresh results. Admitted refresh uses successful admission identities
  and a separate baseline with result/version checks. These ownership paths differ.
- Both existing workers already prepare, claim, settle and recover refresh work.
  Routes live under `/api/migrations/fub/people-refreshes` and
  `/api/migrations/fub/admitted-people-refreshes`.
- Current specifications explicitly defer confirmed-mapping repair. Old frozen
  mappings, previews, results and raw captures must remain historical evidence.

Extend these two engines with a repair mode and narrowly shared mapping-resolution
helpers. Add no third refresh worker, new executable, supervisor or Kubernetes
workload. Do not combine the distinct original/admitted provenance chains.

## 3. Source and candidate qualification

1. Resolve actor and active Organization server-side; require current admin,
   the unchanged review workspace binding and compatible actual-workload evidence.
2. Anchor a repair to a terminal 010e2/010e4 refresh's immutable mapping-held results,
   or a sealed ready preview's mapping-held item identities. The preview path is
   necessary when every item is held and confirmation rejects zero eligible items.
   For an unconfirmed ready root, the explicit “Replace preview with mapping repair”
   command atomically cancels that old root and creates the repair draft after
   checking lifecycle and exact plan ID/revision. Preserve the sealed preview;
   release only its unused reservations under its existing cancellation rules.
   A cancelled unconfirmed root may also supply its preserved sealed held preview.
   Never replace a preparing/running/confirmed nonterminal root through this action;
   use its ordinary cancellation first. Old result rows are never reset or reopened.
   Revalidate mapping-related stale results before treating them as candidates;
   a generic stale/local-change error is not evidence of a mapping problem.
3. Resolve every candidate through its successful original import or admission
   result, source-account-qualified identity and live same-Organization Person.
   Ignore client names/contacts as identity evidence. Missing/deleted targets stay held.
4. Choose a qualified completed retained core report for this exact parent/cohort.
   It must be the latest confirmed source boundary or a qualifying later,
   non-overlapping boundary under the owning refresh rules. An older hold does
   not authorize replay of old source data after a newer boundary was confirmed.
5. A new explicit repair may use the exact current boundary for settled mapping
   holds. This is a declared extension to recovery; ordinary retry/remainder must
   not acquire authority to reopen settled records. Different same-time or
   overlapping snapshots are not interchangeable with the exact same capture.
6. Authenticate and reclassify full retained source records. Incomplete/conflicting
   source evidence, missing fields, malformed values, Trash and missing identities
   retain existing handling. Missing fields are not clears; absence is not deletion.
7. Include only the anchored mapping-held candidate set. Unselected/unresolved
   source keys and already resolved anchored held items are visible outcomes.
   This is not a lifetime exclusion of the Person/key: a fresh later mapping hold
   can authorize a newly previewed correction that supersedes an invalid binding.
   A correction never silently expands to every Person sharing a label.

The prior hold is provenance, not an authorization shortcut. Re-evaluate current
source and destination eligibility; fixing one mapping may reveal another hold.

## 4. Immutable approvals and future refresh behavior

Store new encrypted mapping choices under the repair plan. Bind each to Org,
source account, cohort, kind, exact qualified source key (or existing stage-label
identity), source evidence, destination ID/frozen target snapshot and approver.
Preserve the existing stage-label and missing-value identity rules; do not invent
a new normalization or treat labels as trusted identifiers.

Plan approval applies only to its explicitly previewed candidate set. A successful
settlement records a per-Person mapping binding for each repaired source key in
the same transaction as the result and baseline. It applies to that Person's
later refreshes when the same qualified source key is encountered. This prevents
a later normal refresh from reverting to an obsolete original mapping.

Resolution order for both engines:

1. Explicit new repair choice for this exact candidate/kind/source key.
2. This Person's last successfully settled repair binding for that exact key.
3. A qualified original approved mapping.
4. Visible mapping hold.

A selected binding that has become invalid must hold; do not fall back to an
older mapping and accidentally reassign the Person. Another source key does not
inherit the correction. Unrelated People and notes/task authorship receive no
new authority. Historical bindings remain immutable; a new successful binding
can supersede one for the same Person/key without changing old records.

Represent bindings with typed original-versus-repair references and real foreign
keys/constraints. Do not stuff repair IDs into FKs that mean original mappings.
Normal refreshes on repaired People must preserve these bindings through baseline
updates, including source omissions. Readers distinguish original approval from
later repair approval. Held, failed and unprocessed items activate no binding.

## 5. Preview, conflict and settlement

Use the existing owned baseline B, current CRM projection C, and proposed full
core projection N. A baseline is what migration last successfully wrote, including
contact row identities/order; it is not whatever happens to be in CRM today.

- If C differs from B, hold the entire Person. Mapping approval never bypasses it.
- If C equals B, validate the corrected mapping and all other source instructions,
  then show the complete proposed refresh, including explicit clears/removals.
- If C equals B equals N, an explicit repaired mapping may settle as an approval
  without a business change. Record its provenance/binding; emit no fake stage or
  assignment transition. Confirmation separately counts these approval-only items.
- Freeze baseline/result versions, current fingerprints, contact ownership,
  target snapshots, candidate set, source boundary and mapping digest in the plan.
- At execution, recheck all of them. A changed target, admin, workspace, source
  boundary, mapping head or relevant Person value produces a safe hold/pause.

Each bounded transaction holds the owning parent/cohort serialization lock and
the established root/plan/baseline/Person/contact/catalog/membership/ledger locks
in one documented order. It verifies the current lease token and authorization
while protecting those rows. Settle business writes, required IDs-only facts,
encrypted before/after evidence, result, per-Person binding, baseline, bytes and
progress atomically. A worker crash before commit rolls the unit back; a lost
response after commit replays its durable receipt without repeating transitions.

Keep history meaning: existing migration attribution and actual local transition
time, no fabricated inquiry/contact credit, no release of the admin review hold.

## 6. Lifecycle, concurrency and storage

- Repair creates a new refresh root/plan referencing the old held run. Reuse the
  existing prepare/ready/queued/running/paused/completed/cancelled lifecycle.
- A new repair root first uses bounded preparation units to derive its fixed
  candidate/key catalog from retained evidence, then pauses with the explicit reason
  `awaiting_mapping_choices`. Bounded candidate/mapping reads work in that state;
  no worker interprets half-entered choices. `POST /{id}/plans` seals the choices
  and their revision, then transitions to preparing. Editing an unconfirmed ready
  draft invalidates its ready plan and returns to awaiting choices. Reject edits
  while preparing or after confirmation; cancel first where applicable.
- Mapping-choice edits produce a new immutable plan revision. They cannot modify
  a sealed/confirmed revision. Match the existing ten-minute preview expiry.
- Initiator confirms/resumes; any current Org admin may inspect/cancel. Another
  admin cancels and prepares their own plan; do not silently adopt an executor.
- Reuse owner-specific lease and bounded transaction handling. Expiration alone
  does not stop an old transaction: fencing, row locks and query/transaction
  time limits must be covered by failure tests.
- Generic Retry in both existing command paths rejects awaiting-mapping-choices;
  explicit plan sealing is the only transition into preparation from that state.
- Serialize original/admitted normal and repair writers through their existing
  parent/cohort paths. Recheck latest source boundary at prepare, confirm and each
  commit; plans prepared before a competing repair must detect changed bindings/
  baselines. Extend old paths where necessary rather than adding a repair-only lock.
- Preserve ordinary-root same-boundary uniqueness while allowing explicitly owned
  repair roots at that boundary. Keep one active writer across both modes.
- Cancel preserves committed People and mapping approvals. Exact-boundary remainder
  handles only the original plan's unprocessed candidates. A newly corrected
  settled hold requires another explicit repair preview.
- Replay keys bind actor/Org/command/root and canonical body digest. Same request
  with a different body conflicts; duplicate completion never duplicates facts.
- Charge all new encrypted choices, selections, bindings, results and receipts to
  the correct snapshot/Org owner. Shared raw evidence retains its existing charge.
  Reserve cancellation capacity. Charge once across rollback/retry; immutable
  old bindings stay charged until an accepted deletion lifecycle removes them.
- Include these stores and their links in the erasure inventory. No new retention
  period or per-Person key policy is selected by this planning document.

## 7. Proposed shared contracts and compatibility

Current contract: refreshes use original approved mappings, immutable results and
owner-specific baseline schemas. Proposed changes are needed to apply a new
explicit approval and keep later refreshes consistent with that approval.

| Surface | Proposed contract | Affected components |
|---|---|---|
| Repair entry | Under each existing refresh base, `POST /{id}/mapping-repairs` creates a new repair draft; body `request_id`, `report_id`, expected source-run lifecycle revision and typed anchor (terminal results or exact sealed preview ID/revision). The preview-replacement action explicitly retires the unconfirmed root atomically. Server resolves cohort/candidates. | Rust commands, routes, Web client |
| Choices | `GET /{id}/repair-mappings` pages exact candidate source keys and qualified target options; `POST /{id}/repair-mappings` supplies bounded choices with `request_id` and expected draft revision. | Both query/command families, admin Web |
| Plan/confirm | Existing plan lifecycle gains an explicit mode, repair-parent reference and `awaiting_mapping_choices` pause reason. Confirmation binds exact plan/mapping digest plus selected-candidate and approval-only counts, alongside existing coverage/clear/removal acknowledgements. | DTOs, receipts, Web, tests |
| Persistence | Add exclusive original/admitted repair ownership, immutable mapping-choice/binding tables and owner-specific result/baseline references. SQL constraints enforce Org/cohort/item ownership. | Additive SQL migration, both workers/stores, SQLx |
| Future refresh | Both engines load typed successful repair bindings before original mappings for the matching Person/source key; invalid bindings hold. | Normal and repair preparation/execution, provenance readers |
| Runtime admission | New `fub-people-mapping-repair-v1` reader/writer capability and durable workspace requirement before first repair confirmation. | DB permit guards, API/admin/migrator inventory, preflight/recovery tooling |

Normal API clients retain their existing request shape and original behavior for
unrepaired People. Repair-mode confirmation requires the new fields; old clients
cannot confirm it by omission. Preserve all existing reader/writer fences.
Incompatible processes must fail closed for affected workspaces, even when an old
worker tries a normal refresh on a repaired Person. A nullable column alone is
not backward compatibility. Initial migration backfill is empty: old approvals
must not be relabelled as repair approvals. Rollback after repair confirmation
requires a compatible binary; old releases cannot simply resume these jobs.

Bound all reads/writes using existing conventions: list pages ≤20, item/mapping
pages ≤50, resource responses ≤128 KiB, item/contact/result pages ≤256 KiB,
field fragments ≤16 KiB UTF-8. A choices request is ≤128 KiB and ≤50 choices;
multiple revision-checked requests can build a draft. Freeze server-derived
candidate selections, not a browser-supplied whole-book ID array. Authenticate
cursors against actor/Org/root/endpoint/plan/revision/filter and traversal bound.
Reject unknown keys, duplicate/conflicting choices and foreign target IDs.

This spec proposes amendments to 010e2 §§1/3/4 and 010e4 §§1/3/4/5/6. It does not
change 010c, 010e3 or other-family execution authority. Concrete schema/DTO details
must be frozen in the implementation contract before code changes and reviewed
against this proposal. No shared application contract is changed by these docs.

## 8. Admin experience

1. Open a terminal refresh result or ready preview, including a preview with zero
   eligible items. Explain “This update needs a stage or agent mapping” and show
   the qualified held count. A ready preview uses the explicit action “Replace
   preview with mapping repair”; terminal results use “Repair mappings”. Preserve
   and link the retired preview, and never require an impossible zero-item confirmation.
2. Show saved-source date, cohort and original hold. Group unresolved choices by
   source key with affected People counts and paged examples. Always provide
   “Leave unresolved”; never preselect a best-guess destination.
3. Pick existing stage/member or explicit unassigned. Show that choices apply to
   selected held People, and persist for those People on future matching refreshes.
4. Prepare a full preview: current → proposed core fields, mapping approvals,
   names/contact changes and exact clears/removals. Explain remaining holds.
5. Confirm eligible updates and approval-only outcomes with exact counts. Show
   durable progress, pause reason, retry/cancel and retained partial completion.
6. Link new results to the prior hold. Original results remain unchanged and
   visibly link to their repair attempts; do not rewrite old counts as success.

Use current CRM components/tokens. Clear cached detail on actor/Org/access change;
keep no-store, CSRF and same-Org authorization in the typed routes. Native apps,
Operator, Today and ordinary member visibility remain behind the existing hold.

## 9. Acceptance evidence

- Both owner paths; original versus admitted provenance; partial cancelled
  admissions; malformed/foreign/erased identities and cross-Organization targets.
- All-held unconfirmed previews can enter repair. Preview replacement is atomic,
  revision-checked and replayable; old confirmation and replacement races cannot
  start both plans. Old preview evidence/held counts stay intact.
- Unknown stage/agent, renamed target, deactivated member, pond mapping, explicit
  unassigned, source missing/null distinctions and multiple holds on one Person.
- Whole-Person local edits protected, including edited/replaced contact IDs.
  Preview exposes full core changes rather than only the mapping choice.
- Same-value approval with no fake facts; subsequent ordinary refresh retains
  corrected mapping; unrelated People/keys/families keep their original authority.
- Fresh target/source/baseline validation at commit; competing normal/repair plans;
  invalid sticky binding never silently falls back; no older-source rollback.
- Crash before/after commit, lost HTTP response, lease takeover/stale worker,
  cancelled partial work and exact remainder, demotion and byte-budget recovery.
- Existing old-format rows/requests still work where compatible; old workload
  claim/reader/private permit rejected on repaired workspaces; additive migration
  checksum and SQLx offline coverage; no accidental increase in startup loops.
- Real API plus production Web preview/confirm/reload/cancel/recovery walkthrough
  at desktop and 390px using synthetic records. Verify tenant/access cache clearing.
- Required final-tree checks and D-050 measured hot query plans plus paired
  regression. Isolate build outputs and DB-backed verification from shared services.

Planning does not constitute passing implementation evidence. D-088 accepts
implementation; independent planning review and actual verification remain gates.

## D-090 recovery amendment

The accepted [010e6 recovery specification](../plans/SLICE_010e6_NEVER_IMPORTED_RECOVERY.md)
extends admission and follow-on qualification for explicitly anchored, successfully
created recovery People. Normal modes retain this specification's rules. Original
holds are immutable; recovery is a distinct admission mode with its own initial
approvals/provenance. Initial approval readers recognize recovery-owned evidence
before original fallback, with subsequent exact-key 010e5 repair precedence.
No fabricated original results or automatic family cascade is permitted.
