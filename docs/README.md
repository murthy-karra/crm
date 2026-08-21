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

## Areas

- `product/` — product thesis and product requirements
- `architecture/` — architecture baseline and accepted ADRs
- `decisions/` — accepted decisions and unresolved decision register
- `plans/` — implementation plans and `PROJECT_STATE.md` (current operational status)
- `specs/` — vertical-slice specifications
- `design/` — accepted UI style reference (`UI_STYLE.md`) and the sample
  screens it was derived from; binds the web client, subordinate to specs
- `tasks/` — bounded implementation task briefs
- `research/` — competitor and technical research; not accepted policy

## Operational status

The current phase, active slice, pending approvals, and next action are always
recorded in `plans/PROJECT_STATE.md`. The repository, not chat memory, is
authoritative.
