# Claude Code Instructions

Read and follow `AGENTS.md` before planning, reviewing, or changing this repository.

`AGENTS.md` is the canonical repository-wide engineering policy.

Also read:

- `docs/decisions/DECISION_LOG.md`;
- the assigned task brief;
- the referenced specification; and
- relevant accepted architecture decisions.

For implementation work:

1. Inspect existing code before proposing changes.
2. State a short plan before editing.
3. Stay within the assigned ownership boundary.
4. Do not silently alter shared contracts.
5. Do not perform unrelated cleanup or architectural redesign.
6. Run the required checks before declaring completion.
7. Clearly report assumptions, blockers, requested decisions, and tests run.

For review work:

- prioritize correctness, security, tenant isolation, contract consistency, failure behavior, and missing tests;
- distinguish genuine blocking decisions from safe defaults and implementation details;
- do not report style-only findings unless they hide a material problem;
- do not edit files unless the review task explicitly grants write access.
