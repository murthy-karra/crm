# Discover the next useful change

Use when the desired outcome needs investigation. Default profile: `implement`
in [MODEL_ROUTING.md](MODEL_ROUTING.md); `design` for ambiguous requirements.
Provide an outcome, relevant constraints, and optional output path. Copy the
prompt below with your task notes, or ask the agent to read this file.

```text
Investigate the requested outcome for this CRM. Read and apply the shared
instructions in docs/prompts/README.md and the discovery routing in
docs/prompts/MODEL_ROUTING.md. Preserve any explicit model/role assignment.

Inspect the authoritative decisions, current project state, relevant specs,
and the existing implementation. Explain what works today, the user problem,
and the smallest useful change. Cite the files and behavior that support this.
Use focused searches; a whole-repository audit is unnecessary unless requested.

Identify the uncertainties that could change the design. Research primary
sources or run a small, non-destructive experiment when it answers one of them.
Separate verified facts, reasonable reversible assumptions, and human decisions.
Do not adopt a product/privacy/architecture decision merely because a source
or another agent recommends it.

Compare plausible options only where there is a meaningful tradeoff. Recommend
one, with its effects on user behavior, existing contracts, testing, and scope.
Flag dependencies and open decisions that prevent a sufficiently specified task.
For a genuine decision, ask one focused question with context, options,
recommendation, and consequences; continue independent investigation meanwhile.

Return the findings and next bounded slice or fix. Include relevant files and
their functions, exclusions, proposed acceptance examples, and any source dates.
Write a research note only if requested or useful to preserve material findings;
otherwise keep the result in the conversation. Do not modify application code
or launch implementation from this discovery request alone.

Finish when the next step is supported by evidence or you have isolated the
specific missing decision. Route a new behavior to 02-specify.md; route an
already specified authorized fix to 05-implement.md. Include the shared handoff
record only to the extent needed to continue without repeating the survey.
```
