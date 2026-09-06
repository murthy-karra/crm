# Prepare and validate a release

Use only when release work is assigned. Default profile: `implement` for a proven
runbook, `design` for release design or unexpected failures, as defined in
[MODEL_ROUTING.md](MODEL_ROUTING.md). Provide the candidate revision/artifact,
target environment, existing runbook, and scope of deployment authorization.

```text
Prepare or execute the assigned release within its actual authorization.
Read and apply docs/prompts/README.md and the deployment routing in
docs/prompts/MODEL_ROUTING.md. Carry forward existing approval evidence.

Verify the candidate revision/artifact, target environment, runbook, accepted
infrastructure decisions, and final-tree verification. Read current project
state and inspect actual services when relevant; do not infer a production
deployment or CI pipeline from architecture plans. Use the existing deployment
mechanism. Missing infrastructure is a planning dependency, not permission to
build a new platform during release preparation.

Prepare a concrete release plan: artifact/revision, ordered commands, target
services, configuration names without values, migration/compatibility impact,
expected health checks, user-visible smoke checks, observation period, and
rollback or forward-recovery steps. State whether schema/data changes prevent
safe rollback and how the existing backup/restore procedure addresses the risk.

Distinguish development service refresh from production deployment. Follow the
documented process lifecycle; restarting containers may leave a stale local
API binary serving requests. Do not reset data or kill broad process groups
as a release shortcut. Real outbound calls/messages require their task's scope.

Complete readiness work before a missing approval is requested. If execution
is already authorized, perform the scoped runbook and record actual outcomes.
If only preparation was assigned, deliver PREPARED with the remaining gate and
its source. Never infer approval from silence, a passing review, or this prompt.

After execution, verify the running revision and required health, smoke, and
recovery signals. On failure, stop further rollout, preserve non-sensitive
evidence, and apply only recovery actions within authorization; escalate with
the concrete recovery plan when needed. A started command is not a healthy release.

Report PREPARED, DEPLOYED-AND-VERIFIED, or INCOMPLETE/FAILED with environment,
revision, command/check results, rollback/recovery status, and remaining risks.
Hand off to 08-monitor.md only for the requested observation scope. Do not
promise unattended monitoring unless a real runner/schedule has been configured.
```
