# Implement the assigned task

Use for a sufficiently specified, authorized change. Default profile: `implement`
in [MODEL_ROUTING.md](MODEL_ROUTING.md); consequential work starts at `design`.
Provide the task brief or equivalent concrete scope, branch, and existing gates.

```text
Implement the assigned task through its required verification and handoff.
Read and apply docs/prompts/README.md and the implementation routing in
docs/prompts/MODEL_ROUTING.md. Preserve explicit implementer/coordinator roles.

Read the brief, specification, decisions, referenced architecture, and existing
implementation. Check the actual branch and working tree. State a short plan,
identify owned files and checks, and surface any contract/decision conflict
before editing. Do not manufacture a new spec when the existing brief is adequate.

Make the in-scope changes through established patterns and the shared typed
command/query layer. Preserve tenant isolation, authorization, history/raw-data
semantics, and sensitive-data handling. Apply the relevant specification and
UI_STYLE.md for visual work. Keep edits within the lane's ownership; coordinate
shared-file or database changes before writing them.

Add or adjust meaningful tests for changed behavior and failure cases. Run the
task's exact required checks, including final-tree gates, and map acceptance
criteria to actual results. Regenerate required artifacts, including SQLx
metadata when queries change, using the documented workflow. Inspect relevant
UI states when the task changes visual behavior. Do not weaken tests to pass.

Diagnose failures before retrying. Distinguish code defects, incomplete
requirements, environment problems, and inadequate reasoning; use MODEL_ROUTING
for effort/model escalation and preserve attempted-fix evidence. Continue
authorized repairs until the task's completion bar is met. A blocker leaves
the task incomplete and gets a precise handoff, not a fabricated success.

Reconcile the changed-file report against git status and the actual diff,
including untracked/generated files. Report outcome, criterion-to-test mapping,
commands/results, relevant telemetry/error handling, unresolved risks, and
the shared handoff record. Label checks that were not run and explain why.

Follow the task's commit/merge/push/deployment scope and gates. If review is the
next step, hand the final tree and evidence to 06-verify-and-review.md. Do not
infer release authorization from implementation completion.
```
