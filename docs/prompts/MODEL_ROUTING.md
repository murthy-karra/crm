# Model routing and thinking effort

Updated **2026-09-13** for the user's documentation-efficiency request.
Use with the [shared instructions](README.md#shared-instructions-for-every-phase).
These are engineering defaults, not the CRM Operator's inference configuration.
This document does not switch a running model, configure tools, authorize new
agents/releases, or change product contracts and required verification.

## Preserve the working baseline

Explicit user/task model and effort assignments take precedence. **Terra high is
the current preference for substantive implementation**, recorded in D-076/D-078
and reaffirmed by the routing refresh. Use lighter defaults for routine work;
do not silently downgrade an assigned lane. Older Sonnet/Fable assignments apply
only to tasks that explicitly retain them, not to all future work.

Profiles below are repository recommendations, not provider guarantees or claims
of equal capability. Select only models and effort settings advertised by the
actual runner. Preserve required independent review regardless of model choice.

## Profiles and escalation ladders

| Profile | Default model / effort | Use |
|---|---|---|
| `support` | GPT-5.6 Luna / `low` | Factual status, file inventory, mechanical docs, known-signal inspection |
| `implement` | GPT-5.6 Terra / `medium` for routine fixes; **`high` for substantive work** | Bounded code changes, integration, required checks and proven release runbooks |
| `design` | GPT-5.6 Sol / `high` | Specifications, consequential contract/policy analysis, independent substantive review |
| `hard` | GPT-6 Astra / `high` | Difficult cross-layer design, concurrency failures or unresolved technical blockers |
| `interactive` | GPT-5.3-Codex-Spark / `low` or `medium`, when available and selected | Narrow edits with frequent feedback; otherwise use `implement` |

A short task can still be consequential: authorization, tenant isolation,
migration fidelity, history/erasure and concurrency need deliberate design and
review. Start their unresolved design at `design` or `hard`; an accepted, concrete
implementation brief can stay with its assigned Terra high writer. Do not promote
every phase merely because the repository contains sensitive domains.

## Phase defaults

| Phase | Default |
|---|---|
| 01 Discover | `support` for factual lookup; `implement` for bounded investigation; `design` for ambiguity |
| 02 Specify | `design`; `hard` for a named difficult cross-layer problem |
| 03 Plan | `implement` for routine decomposition; `design` for contracts or multi-lane dependencies |
| 04 Review plan | `design`; targeted `hard` escalation for unresolved consequential concerns |
| 05 Implement | Terra medium for routine fixes; Terra high for substantive implementation or an explicit high assignment |
| 06 Verify and review | `implement` for test execution; `design` for independent substantive review; `hard` for difficult findings |
| 07 Deploy | `implement` for an authorized proven runbook; `design` for release design or unfamiliar failure analysis |
| 08 Monitor | `support` for known signals; `implement` for investigation; escalate on a concrete unresolved issue |
| 09 Handoff and improve | `support`; `implement` when reconciling evidence requires investigation |

The phase table is authoritative for effort selection within these advisory
profiles; a phase is not automatically a new task, model switch or agent.

## Default workflows

- **Routine fix or docs:** one writer, relevant context, focused checks and the
  required completion gates. Start at implementation when the brief is sufficient.
  No separate specification, reviewer or retrospective merely to fill a phase;
  retain any review explicitly required by the task.
- **Substantive change:** accepted scope/contract, Terra high writer, required
  independent review and final integration gates. Pass base/head revisions, the
  acceptance criteria, applicable decisions, changed paths and evidence to review.
  Reviewers inspect the real diff and affected paths; they do not reopen unrelated
  project design. Reuse valid checks with runner/tree attribution and rerun when
  required, stale or affected by changes. Blocking gaps remain blocking.
- **Release:** use the existing release brief and attributed implementation
  evidence. Perform its backup, preservation, compatibility, artifact and smoke
  checks. Do not repeat all implementation gates unless the release requires it
  or changes/failures invalidate evidence. Publication/deployment scope still applies.

Use one writer for small work. Parallel agents are appropriate only when authorized
and substantial independent lanes justify setup/context/handoff costs; this guide
is not standing delegation authorization. Preserve AGENTS §12's three-worktree
limit, ownership boundaries and isolated build/test resources. When delegated,
prefer a compact self-contained brief over a full conversation fork where the
runner supports it; include all relevant approvals, restrictions and dependencies.

## Escalate based on evidence

1. Before escalation, check the brief, evidence and environment. Missing services,
   credentials or a human decision are not solved by more reasoning.
2. Name the missed invariant, unexplained failure or design question. Move to the
   appropriate stronger profile/effort directly; no requirement to try every rung.
3. After **two unsuccessful repairs of the same blocker**, reassess and escalate
   or report the missing decision. Carry the reproduction, exact failing check,
   attempted fixes, tree and uncertainty forward; do not reset the attempt count.
4. Reserve `xhigh`/`max` for a named difficult problem and bounded objective.
   Return routine follow-up to its appropriate default unless explicitly assigned.
5. D-050 still caps review/fix rounds at two; a third needs explicit approval.
   A model switch does not restart that limit. Preserve required trust checks and
   performance gates. An unresolved blocking finding cannot become a pass.
6. Respect supplied task-wide budgets, including workers/retries, and reserve
   room for verification. Report incomplete work honestly if a budget is exhausted.

## Thinking controls are provider-specific

Use only the current runner's advertised model/effort combinations. Effort labels
are not a hard token budget or a promise of lower total task cost. Do not copy API
parameters into desktop settings or assume that similarly named provider controls
are equivalent. Explicit assignments on another provider remain valid; verify
that provider's current official controls if a change is requested. No unrequested
provider substitution or automatic model switching follows from this guide.

## Identifiers and availability

The current Codex runner exposes `gpt-5.6-luna`, `gpt-5.6-terra`, `gpt-5.6-sol`,
`gpt-6-astra` and `gpt-5.3-codex-spark`. Recheck the target runner before dispatch;
this observation does not establish access on another account, host or API.
Record actual model/effort when observable; distinguish requested from effective
settings. If an explicit model is unavailable, report it rather than silently
substituting. A Markdown model name does not change the active task's settings.

## Measure cost per accepted task

Use Terra high as the baseline for substantive implementation. Trial lighter
settings on comparable routine work, changing one major variable at a time while
keeping acceptance criteria fixed. Use the existing [handoff record](README.md#outputs-and-handoffs):
model/effort, actual usage when available, attempts, review corrections, checks,
elapsed time and later repairs. Include coordination and workers; separate waiting
and human review time when measurable. No new telemetry service or recurring task.

Measure completed outcomes, not a single response. Tokens, subscription allowance
and billed cost are different measurements; caching and reasoning affect usage.
Do not infer per-task dollars from subscription percentages or count reasoning
again when it is already included in output usage. Mark unavailable figures as
unavailable, not zero; do not promise savings without measurements.

Official OpenAI [usage guidance](https://learn.chatgpt.com/docs/pricing#what-can-i-do-to-make-my-usage-limits-last-longer),
checked 2026-09-13, recommends relevant context, concise output and smaller models
for routine work. Recheck [current rates](https://learn.chatgpt.com/docs/pricing)
when estimating cost; this guide deliberately stores no price or savings table.
