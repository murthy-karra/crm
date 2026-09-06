# Verify behavior and review the delivered change

Use for independent acceptance review of meaningful changes. Default profile:
`design` in [MODEL_ROUTING.md](MODEL_ROUTING.md). Provide the specification/brief,
the exact review target (working tree or verified base/head), and existing evidence.
Read-only review is the default; repairs belong to the assigned writer.

```text
Independently verify the assigned change against its acceptance criteria.
Read and apply docs/prompts/README.md and the verification routing in
docs/prompts/MODEL_ROUTING.md. Preserve the existing reviewer assignment.

Resolve the exact review target and inspect the real diff, untracked files,
specification, task brief, and affected code paths. Do not rely on the
implementer's changed-file list or test claims without checking their evidence.
Attribute supplied results to their runner and identify the tree they covered.

Map every applicable acceptance criterion to a test, direct observation, or an
explicit gap. Run the checks required of this review and investigate missing
or stale evidence. Scope additional checks to unresolved concerns. Coordinate
database runs with the writer; never overlap them in one checkout. If a writer
changes the target during review, re-establish the tree and evidence affected.

Examine authorization and cross-Organization attempts, contract compatibility,
error paths, idempotency/retries, concurrent transitions, historical fidelity,
and sensitive-data exposure where relevant. Review telemetry and performance
against task requirements. For UI work, inspect the required flows and states;
use screenshots only when they provide useful, non-sensitive evidence.

Return findings ordered by severity, each with file/line or spec section,
triggering scenario, evidence, impact, and suggested correction. Distinguish
regressions from pre-existing issues. Report no actionable findings when that
is the evidence; do not invent findings or replace requirements with preferences.

State which criteria pass, fail, or remain unverified, plus the exact commands
run. Approval readiness requires all required checks and blocking findings to
be resolved. Describe any user-authorized deferral honestly; never call an
unverified criterion passed or treat readiness as release approval.

Send corrections to the assigned implementer using 05-implement.md; a spec gap
returns to 02-specify.md. Do not edit the same files concurrently or repair
code during a read-only review. Once fixes land, verify affected behavior and
any required final-tree gates. Hand off the result and remaining release gate.
```
