# Independently review a specification or plan

Use for the existing spec-review step and for execution plans that benefit from
review. Default profile: `design` in [MODEL_ROUTING.md](MODEL_ROUTING.md).
Provide the target documents, outcome, and review scope. Review is read-only
unless a separate instruction authorizes edits.

```text
Independently review the assigned specification and/or execution plan.
Read and apply docs/prompts/README.md and the plan-review routing in
docs/prompts/MODEL_ROUTING.md. Honor an existing reviewer assignment, including
crm-reviewer when available; do not claim to invoke an unavailable agent.

Read the authoritative decisions, the complete target documents, and the
relevant implementation/test precedents. Evaluate the design against the
requirements rather than relying on the author's summary or confidence.

Look for missing behavior, unsupported assumptions, contract incompatibilities,
authorization/Organization-boundary gaps, migration/data-loss risks, retry or
concurrency failures, unrealistic dependencies, and unnecessary complexity.
Check that tests can demonstrate the acceptance criteria, including negative
cases. For a parallel plan, inspect ownership, database ownership, integration
order, and final-tree checks. Scale the review to the actual change.

For each actionable finding, give a stable ID, severity, file/section, concrete
failure scenario, evidence, and smallest useful correction. Separate blocking
findings, nonblocking improvements, and genuine human decisions. Do not reopen
an accepted decision as a blocker without new evidence or an actual conflict.

Return READY, READY-WITH-FIXES, or BLOCKED and explain the result. READY-WITH-FIXES
means listed blocking corrections still need resolution; it is not permission
to implement. If there are no findings, say so and identify material limits of
the review. Readiness never substitutes for the task's human approval gate.

Leave source documents untouched unless revision was explicitly assigned.
The author/coordinator resolves findings in the existing spec/plan and records
each disposition. Re-review changed assumptions and blocking corrections rather
than automatically repeating the whole review. Unresolved requirements return
to 02-specify.md; execution gaps return to 03-plan.md.
```
