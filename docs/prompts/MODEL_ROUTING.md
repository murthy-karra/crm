# Model routing and thinking effort

Advisory engineering guidance. Provider documentation checked **2026-09-05**.
Use with the [shared instructions](README.md#shared-instructions-for-every-phase).
This file does not change the CRM's inference provider, configure a runner,
make API calls, buy services, or register background work.

## Preserve the working baseline

An explicit task/user assignment takes precedence over these defaults. Preserve
the recorded Sonnet-implementer/Fable-coordinator arrangement and independent
review where assigned; see [project state](../plans/PROJECT_STATE.md#next-recommended-action).
The phase prompts reference profiles so model updates happen in one place.
Use either provider for a complete task; cross-provider review is optional.

## Profiles and escalation ladders

These pairings describe intended work. They do not establish equal capability,
speed, tokenization, or cost. Arrows are available escalation steps, not a
requirement to try every setting. Apply the evidence rules below.

| Profile | Typical work | OpenAI starting choice → effort escalation | Anthropic starting choice → effort escalation |
|---|---|---|---|
| `support` | File inventory, factual summaries, mechanical docs, known-signal monitoring | GPT-5.6 Luna: `low` → `medium` | Claude Haiku 4.5: thinking off → bounded manual thinking; then use `implement` |
| `implement` | Discovery, bounded implementation, existing deployment runbooks | GPT-5.6 Terra: `medium` → `high` | Claude Sonnet 5: `medium` → `high` |
| `design` | Specifications, plans, independent review, complex coding | GPT-5.6 Sol: `high` → `xhigh` | Claude Opus 5: `high` → `xhigh` |
| `hard` | Difficult cross-layer design, concurrency failures, unresolved technical blockers | GPT-6 Astra: `high` → `xhigh` → `max` | Claude Fable 5.1: `high` → `xhigh` → `max` |
| `interactive` | Narrow coding edits with frequent user feedback | GPT-5.3-Codex-Spark: `low` → `medium`; then use `implement` | Claude Sonnet 5: `medium` → `high` |

Spark's Sonnet pairing is a practical coding alternative, with different latency.
Medium effort for routine coding is a proposed cost-saving baseline to validate
against this repository's acceptance checks; substantive coding can start at high.
Both providers' current guidance supports selecting effort for the workload:
[OpenAI models](https://learn.chatgpt.com/docs/models),
[Anthropic models](https://platform.claude.com/docs/en/models/overview).

## Phase defaults

| Phase | Profile and adjustment |
|---|---|
| 01 Discover | `implement`; `support` for factual extraction, `design` for ambiguous requirements/design |
| 02 Specify | `design`; `hard` for difficult cross-layer design |
| 03 Plan | `design`; `medium` is sufficient for routine decomposition, `high` for dependencies or multiple lanes |
| 04 Review plan | `design` at `high`; `hard` for unresolved consequential design concerns |
| 05 Implement | `implement`; begin at `design` for material authorization, tenant-isolation, shared-contract, migration-fidelity, or concurrency changes |
| 06 Verify and review | `design`; routine test execution can use `implement`, difficult findings can use `hard` |
| 07 Deploy | `implement` for a proven runbook; `design` for release design or unfamiliar failures |
| 08 Monitor | `support` for known signals; `implement` for investigation, `design`/`hard` for difficult incidents |
| 09 Handoff and improve | `support`; `implement` to reconcile evidence, `design` for a separately assigned architectural analysis |

Optional stages stay optional; routing neither creates new approval gates nor
waives existing ones. A stronger reviewer cannot replace required verification.

## Escalate based on evidence

1. Start at `design` or `hard` when the task materially affects consequential
   boundaries listed in phase 05. Do not spend two cheap attempts discovering
   that an obviously difficult task needs more capability.
2. Before raising effort, check the success criteria, relevant evidence, tools,
   and environment. Missing credentials/services or an unanswered product
   decision need their own remedy. Short answers alone are not a failure signal.
3. Raise effort one step for a missed invariant, incomplete failure analysis,
   or an unexplained failing check when deeper reasoning could help. Name the
   failure and what the extra analysis must resolve. Keep the existing task scope.
4. After **two unsuccessful repairs of the same blocker**, reassess and escalate
   the model/profile or return the missing decision to the coordinator. Do not
   reset the attempt count by relabeling the same failure. Typical progression:
   `support`/`interactive` → `implement` → `design` → `hard`.
5. Hand off the reproduction, exact failing check, attempted fixes, relevant
   files/tree, and remaining uncertainty. Resolve substantive reviewer disputes
   with evidence or a targeted stronger review, not majority voting.
6. Reserve `xhigh`/`max` for a named difficult problem with a bounded objective.
   Observe any supplied time/cost/token ceiling across workers and retries;
   request an extension only when genuinely needed. A ceiling or exhausted
   approach leaves an incomplete handoff, never a false completion.
7. Return to the appropriate lower profile/effort for routine follow-up work.
   Preserve useful context and avoid repeated full surveys or unnecessary
   provider switches. If the runner cannot perform an escalation, report the
   exact requested change and continue only work feasible within its capability.

## Thinking controls are provider-specific

OpenAI GPT-5.6 API effort values are `none`, `low`, `medium`, `high`, `xhigh`,
and `max`; Astra supports `low` through `max`, without `none`. A runner may
expose fewer or different controls. Select advertised settings only; labels
such as an app's Ultra mode are not interchangeable API effort values.
See [GPT-5.6 guidance](https://developers.openai.com/api/docs/guides/latest-model?model=gpt-5.6)
and [Astra](https://developers.openai.com/api/docs/models/gpt-6-astra).

Sonnet 5, Opus 5, and Fable 5.1 support `low`, `medium`, `high`, `xhigh`, and
`max`, defaulting to `high`, with adaptive thinking. Set effort explicitly for
the medium baseline here. Fable thinking is always on; Opus cannot disable it
at `xhigh`/`max`. Use adaptive thinking with effort, not manual token budgets.
See [Anthropic effort](https://platform.claude.com/docs/en/build-with-claude/effort).

Haiku 4.5 has **no effort parameter or adaptive thinking**. Start with thinking
off for mechanical work. If a small reasoning step is useful, the API supports
manual thinking (`type: "enabled"`, `budget_tokens`); a proposed starting budget
is 2,048 tokens, where the runner exposes it. The documented minimum is 1,024,
and the budget must be below `max_tokens`. Haiku has no interleaved thinking;
move substantial tool-driven reasoning to Sonnet instead of inflating the budget.
See [Haiku](https://platform.claude.com/docs/en/models/haiku-4-5/overview) and
[manual thinking](https://platform.claude.com/docs/en/build-with-claude/extended-thinking).

Effort guides behavior, not total spend. Anthropic `max_tokens` bounds one
request's thinking plus answer, not the whole agent loop. Changing top-level
effort can invalidate caching; use the supported per-message control on Opus 5
or Fable 5.1 only when the runner provides it. See
[effort changes](https://platform.claude.com/docs/en/build-with-claude/effort#change-effort-mid-conversation).

## Identifiers and availability

| Profile | OpenAI identifier | Anthropic identifier |
|---|---|---|
| `support` | `gpt-5.6-luna` | `claude-haiku-4-5-20251001` (alias: `claude-haiku-4-5`) |
| `implement` | `gpt-5.6-terra` | `claude-sonnet-5` |
| `design` | `gpt-5.6-sol` | `claude-opus-5` |
| `hard` | `gpt-6-astra` | `claude-fable-5-1` |
| `interactive` | `gpt-5.3-codex-spark` (runner research preview) | `claude-sonnet-5` |

The non-Spark IDs above are public API identifiers; verify aliases/snapshots
when reproducibility matters. Spark is not a public API choice at the check
date. Sources: [Luna](https://developers.openai.com/api/docs/models/gpt-5.6-luna),
[Terra](https://developers.openai.com/api/docs/models/gpt-5.6-terra),
[Sol](https://developers.openai.com/api/docs/models/gpt-5.6-sol), Astra above,
[Spark access](https://learn.chatgpt.com/docs/pricing), Anthropic models above.

Select from the current account/runner's real capabilities. A public catalog
does not establish access in Codex, Claude Code, or another runner. A Markdown
model name cannot switch a running agent. Record the actual model/effort when
observable and distinguish requested from effective settings otherwise. Within
authorized provider choices, use an available counterpart at the same profile
and disclose the substitution. Honor explicitly named models; report missing
access rather than silently substituting or downgrading consequential work.

## Measure cost per accepted task

Keep an explicitly assigned Fable/Sonnet workflow as the initial baseline.
Introduce cheaper profiles on mechanical chores first, then validate medium
effort on bounded implementation. Compare the first 10–20 representative tasks,
keeping acceptance criteria fixed and changing one major variable at a time.
Use the [handoff record](README.md#outputs-and-handoffs); no new telemetry service
or permanent reporting file is required.

Record actual model/effort, attempts, review corrections, acceptance results,
and elapsed time. Count coordination, workers, review, retries, and attributable
later repairs. Distinguish tool waiting and human review time when measurable.
Use supplied budgets; otherwise keep the task bounded and reassess at the repair
checkpoint. Reserve capacity for required verification.

Report billed cost only from available usage records. Label token-rate
calculations as estimates, include relevant cache/token categories and worker
usage, and avoid counting thinking twice when it is already in output totals.
Subscription percentages are not per-task dollar costs. Mark unavailable values
as unavailable, not zero. Recheck current rates before cost claims:
[OpenAI credits](https://learn.chatgpt.com/docs/pricing),
[OpenAI API pricing](https://developers.openai.com/api/docs/pricing),
[Anthropic pricing](https://platform.claude.com/docs/en/about-claude/pricing).
