# Specify a bounded slice

Use to turn an investigated outcome into a reviewable specification. Default
profile: `design` in [MODEL_ROUTING.md](MODEL_ROUTING.md). Provide the outcome,
discovery findings, applicable ladder/decisions, and target spec path if known.

```text
Draft or revise the specification for the assigned outcome. Read and apply
docs/prompts/README.md and the specification routing in
docs/prompts/MODEL_ROUTING.md. Preserve any explicit model/role assignment.

Read the current decisions, accepted architecture, relevant existing specs,
and implementation. Preserve the ladder's small independently landable rungs;
specify the next rung in detail and leave later rungs open to what it teaches us.
Use the existing spec format where practical. State a short editing plan first.

Describe the user problem and resulting behavior, scope and exclusions, concrete
flows/states, data and command/query design, and affected Web/native/Operator
surfaces. Cover the relevant trust boundaries, failure paths, retries,
idempotency, history/raw-source fidelity, observability, and recovery behavior.
Include performance constraints where the task has a real requirement or baseline.
Avoid inventing components or infrastructure to fill a template.

For each shared-contract change, explicitly record current and proposed shapes,
why it is needed, affected components, compatibility/migration impact, and which
specification sections need amendment pointers. Identify the task that will own
the change. Drafting a proposed contract does not approve its implementation.

Write numbered acceptance criteria that describe observable behavior and map
each to a meaningful verification method. Include cross-Organization rejection
and authorization cases for tenant-owned work; cover relevant failure/retry
cases and UI states. Name what automated tests prove and what needs a walkthrough.

Resolve routine reversible details from project precedents. Mark unresolved
product/privacy/architecture decisions explicitly, with options and consequences;
do not write an assumption into the decision log as accepted. Ask only for
decisions that materially block the spec, while drafting independent sections.

Write the draft under docs/specs/ or update the assigned spec. Report changed
files, acceptance coverage, remaining decisions, and suggested review scope.
Request independent review using 04-review-plan.md before the task's required
approval gate. Do not start implementation or claim that a draft is accepted.
```
