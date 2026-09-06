# Plan execution and ownership

Use when an accepted or review-ready specification needs an execution plan.
Default profile: `design`, with planning effort from
[MODEL_ROUTING.md](MODEL_ROUTING.md). Provide the specification, existing plan,
task constraints, and any known role/branch assignments.

```text
Create the smallest useful execution plan for the assigned specification.
Read and apply docs/prompts/README.md and the planning routing in
docs/prompts/MODEL_ROUTING.md. Preserve task-specific roles and approval gates.

Read the specification and decisions, inspect the affected implementation and
test infrastructure, and identify unresolved dependencies. State a short plan
before editing the planning documents. A draft spec can support a draft plan;
it cannot be treated as accepted implementation scope.

For each task, define its user-visible outcome, authoritative inputs, explicit
exclusions, owned files/directories, dependencies, contract ownership, concrete
acceptance criteria, and exact required checks with prerequisites. Identify
generated files such as SQLx query metadata and necessary spec amendments.
Record authorized actions and remaining gates from their real sources.

Choose a single lane when coordination would cost more than it saves. For
independent work, define disjoint ownership and explicit integration order under
AGENTS.md section 12: at most three short-lived worktrees, one primary writer
per worktree, and one named database/migration owner. Assign one coordinator
for shared documents and final-tree verification. Avoid concurrent database
test runs in a checkout. State each lane's model profile and escalation route.

Keep tasks small and independently verifiable. Explain difficult ordering or
rollback constraints. Do not create permanent component branches, speculative
services, new tooling, or infrastructure without a concrete task requirement.

Write plans under docs/plans/ and task briefs under docs/tasks/, extending the
existing documents when appropriate. A small single-lane task may need only a
brief rather than a separate plan. Link every task to its spec acceptance criteria.

Report the file list, ownership/dependency map, required checks, and remaining
decisions or approvals. Use 04-review-plan.md for a useful independent review.
Planning is complete when a fresh implementer can execute the brief without
inventing policy, contracts, or test expectations. Do not launch workers merely
because the plan describes parallel work; follow the current execution scope.
```
