# Documentation Index and Precedence

This directory holds the authoritative project documentation.

## Precedence

When documents conflict, follow this order (highest authority first):

1. `docs/decisions/DECISION_LOG.md` — accepted decisions and the open-decision register
2. Accepted architecture decision records in `docs/architecture/`
3. The current vertical-slice specification in `docs/specs/`
4. The current implementation plan in `docs/plans/`
5. Current product documents in `docs/product/`
6. Research in `docs/research/`
7. Chat history or prior agent conversation

If two authoritative files still conflict, stop and report the conflict before implementation.

## Efficient reading

Follow [AGENTS.md §13](../AGENTS.md#13-required-behavior-before-coding): use the
[decision index](decisions/DECISION_INDEX.md) to locate applicable full decisions
and open items, with core invariants and full D-015/D-050 retained. Check the
log's headings for index drift and include amendments and dependencies. The index
does not change precedence or replace decision text. Reuse unchanged context
already read; load historical records only when the task needs their evidence.

## Areas

- `product/` — product thesis and product requirements
- `architecture/` — architecture baseline and accepted ADRs
- `decisions/` — accepted decisions and unresolved decision register
- `plans/` — implementation plans and `PROJECT_STATE.md` (current operational status)
- `specs/` — vertical-slice specifications
- `design/` — accepted UI style reference (`UI_STYLE.md`) and the sample
  screens it was derived from; binds the web client, subordinate to specs
- `tasks/` — bounded implementation task briefs
- [prompts/](prompts/README.md) — reusable engineering workflow prompts and
  model/effort guidance; subordinate to the authoritative documents above
- `research/` — competitor and technical research; not accepted policy

## Operational status

The current phase, active slice, pending approvals, and next action are always
recorded in `plans/PROJECT_STATE.md`. The repository, not chat memory, is
authoritative.

## Engineer handoff

- [System map](architecture/ARCHITECTURE_BASELINE.md): current components,
  boundaries and the distinction between development and planned production.
- [Current state](plans/PROJECT_STATE.md): active work, release evidence,
  residuals and the next action.
- [Released Mobile 003](specs/MOBILE_003_OFFLINE_CONTACT_LOGGING.md): offline
  manual contact logs with durable receipts and truthful Today refresh.
- [Released migration 010e3](specs/SLICE_010e3.md): core-only admission of newly
  observed People; [coordinator planning review](tasks/MOBILE_003_010e3_PLANNING_REVIEW.md)
  and [parallel execution plan](plans/MOBILE_003_010e3_PARALLEL_LAUNCH.md) cover both.
  D-078 and its follow-up own the completed implementation and release; see the
  [current release evidence](tasks/MOBILE_003_010e3_RELEASE.md).
- [Approved migration 010e2](specs/SLICE_010e2.md): previewed refresh of existing
  imported People, with [execution ownership](tasks/SLICE_010e2_IMPL.md) and
  [planning review](tasks/SLICE_010e2_REVIEW.md); implemented and released under D-076.
- [Approved Mobile 002](specs/MOBILE_002_OFFLINE_EDITS.md): offline note/task
  editing with protected drafts and explicit conflicts; [three-worktree plan](plans/MOBILE_002_010e2_PARALLEL_LAUNCH.md)
  records the completed parallel implementation. The
  [milestone release](tasks/MOBILE_002_010e2_RELEASE.md) owns publication and
  shared backend/Web deployment evidence.
- [Historical progress](plans/PROJECT_HISTORY.md): archived checkpoints, slice
  ledger and old measurements; not current implementation instructions.
- [Foundations proposal](plans/FOUNDATIONS.md): Organization portability, durable
  work and release compatibility, with ownership and capability triggers.
- [Readiness checklist](plans/PRODUCTION_READINESS.md): live validation, first
  customer data, import/cutover and production evidence gates.

The two plans contain proposals and open decisions, not accepted ADRs. Accept
new policies in the decision log and amend owning specs before implementing
contract changes. Update the system map when a boundary or operating entry point
changes; move completed progress into history while preserving live residuals.
