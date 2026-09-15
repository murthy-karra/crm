# 010e5 plan — Unblock existing People with corrected mappings

**Implementation accepted · 2026-09-15 · D-088.** The user approved execution in
Lavish and separately authorized one reviewer. Planning review precedes code;
implementation verification and release evidence are not yet complete.

## Goal and selected scope

The user selected **existing People first; recover never-imported People next**.
Repair stage/agent choices that blocked 010e2 or 010e4 refreshes. Use retained
source evidence and the existing two refresh engines. Imported workspaces remain
in admin review. The [specification](../specs/SLICE_010e5.md) owns all proposed
policy and contracts; the [brief](../tasks/SLICE_010e5_IMPL.md) owns execution order.

## Plain-language example

Alex already exists in CRM. A newer FUB snapshot assigns Alex to source agent Sam,
but the original import never approved a CRM member for Sam. The update is held.

The administrator opens the held refresh result or blocked ready preview, explicitly maps Sam to active CRM member
Jamie and previews Alex's full update. If the same FUB record also changes Alex's
phone number, that appears in the preview too. Confirmation applies only eligible
updates. If somebody edited Alex locally, Alex stays held for separate resolution.

The successful repair remembers the approved Sam → Jamie choice for Alex. Another
Person is not reassigned just because they share the source agent. A later refresh
of Alex using the same source key retains the approval. Old import records remain
an accurate record of what was originally approved and what failed.

## Proposed defaults for implementation review

- Select existing CRM stages and active members; explicit unassigned is supported.
  Creating missing stages/members inside this repair flow is deferred.
- Repair the full blocked core update after preview, preserving all existing
  local-edit protections and explicit clears/removals.
- Scope successful mapping approvals to the exact Person and source key. They
  persist into later refreshes; no Organization-wide mapping replacement.
- Reuse existing refresh workers. This slice adds no worker process or polling loop.
- New repair plans may address settled mapping holds at the current exact source
  boundary. Older-source replay and generic settled-result reopening stay forbidden.
- An all-held ready preview can enter repair without first confirming it. The
  explicit replacement action retires that unconfirmed root atomically and retains
  its sealed preview as evidence, preventing later execution of both plans.

D-088 accepts these defaults and declared contracts. Compatible review corrections
are owned implementation work; material changes retain their decision boundary.

## Delivery order

1. **Review and freeze the contract.** Check per-Person mapping provenance,
   baseline ownership, source ordering and compatibility for both refresh engines.
   Write the concrete SQL/DTO/permit inventory; obtain implementation acceptance.
2. **Backend and persistence.** Add repair mode, scoped immutable choices and
   successful binding references. Implement replay/settlement in both workers.
   Update normal refreshes so they preserve approved corrections.
3. **Admin Web flow.** Held results → mapping choices → full preview → exact
   confirmation → durable progress/results and recovery.
4. **Verification.** Prove local-edit protection, normal-refresh follow-through,
   tenant isolation, crash/retry/cancel behavior, old-workload rejection and
   bounded query plans. Finish with the actual API/Web journey and final gates.
5. **Release separately.** Record source/build/schema and fresh workload evidence,
   then deploy when requested. Production worker architecture remains separate.

## Main risks and how the plan addresses them

- **A later refresh undoes the correction:** persist successful per-Person/key
  approval and teach both normal refresh paths to use it.
- **Repair overrides someone's edits:** compare the whole current Person with the
  last migration-owned baseline; hold on any relevant difference.
- **A mapping change affects too many People:** freeze and preview only the prior
  mapping-held candidate set; activate approvals only for settled candidates.
- **Half-finished repair after a crash:** commit data, approval, baseline, result
  and progress together, then replay durable receipts.
- **Older binaries misunderstand repaired data:** require a durable compatibility
  capability across normal and repair readers/writers before confirmation.

## Remaining migration sequence

After this slice: recovery of never-imported People with explicit identity and
dependent-family handling; later notes/tasks/metadata/history updates; remaining
coverage; final reconciliation and activation. Live FUB/customer readiness and
production worker reliability keep their own gates.

## Planning evidence and limits

The source inspection is recorded in the spec against main `4a01fc0`. Read the
current architecture baseline, original/admitted refresh specs and applicable
decisions. This is a documentation-only proposal: no application changes, database
migration, live source call or runtime mutation. No independent READY review or
implementation test pass is claimed. HTML companion:
`.lavish/people-mapping-repair-plan.html` (cream explainer, same selected scope).

Planning checks: `git diff --check` and local Markdown target checks passed.
Headless Chrome rendered the HTML at 1280px light/dark and 390px; no horizontal
overflow, page errors or duplicate IDs were detected. Both page backgrounds stayed
`#F7F2E8`; the illustrative local-edit toggle produced the expected held state.
These are artifact checks, not application implementation tests.
