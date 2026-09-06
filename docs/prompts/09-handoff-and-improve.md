# Preserve progress and improve the next run

Use at a phase boundary, session handoff, or completed task. Default profile:
`support` in [MODEL_ROUTING.md](MODEL_ROUTING.md); use `implement` to reconcile
conflicting evidence. Provide the task and artifacts; no separate retrospective
file is required for routine work.

```text
Prepare a concise, evidence-backed handoff for the assigned task. Read and apply
docs/prompts/README.md and the handoff routing in docs/prompts/MODEL_ROUTING.md.

Inspect the actual branch, working tree, relevant artifacts, review findings,
and recorded check results. Reconcile them with the task's completion criteria
and authorization. Do not substitute an agent's confidence for evidence, or
repeat expensive tests just to write a summary when applicable evidence exists.

Record the shared handoff fields: outcome/phase, branch and tree, authoritative
paths, completed work and files, remaining ownership, criteria/check evidence,
failed attempts, changed assumptions, unresolved decisions, approvals already
given, pending gates and their sources, and one concrete next action.

Update the existing assigned brief or relevant plan rather than creating a
second source of truth. The coordinator alone updates PROJECT_STATE.md for
the work it owns; other lanes return a proposed state update. Preserve unrelated
active tracks and distinguish historical status from the actual checkout.
Record accepted decisions only with actual user acceptance and the right
decision-log/spec amendments. Completion cannot erase unresolved residuals.

Capture only lessons supported by a concrete failure or successful technique.
Separate a task-specific observation from a proposed standing rule. Suggest a
small prompt/environment improvement when evidence warrants it; edit AGENTS,
skills, hooks, runner configuration, or tool permissions only when that work
is assigned. Do not introduce dependencies or automation to complete a recap.

Include actual model/effort and available elapsed time, usage, retries, review
repairs, and human-intervention observations. Mark unavailable values explicitly.
Compare cost per accepted task, including workers and review, rather than a
single call's token price. Use the rollout guidance in MODEL_ROUTING.md; avoid
changing model and prompt defaults together without a way to distinguish effects.

Return a resumable handoff with changed documentation paths and the exact next
action. Do not mark the task complete, approved, committed, deployed, or monitored
beyond what the evidence establishes. No new work starts just because it is
listed as a follow-up.
```
