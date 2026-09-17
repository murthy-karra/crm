# Web E2E — AI Operator (catalog 08)

Status: passed 8/8 steps in isolated run `18502a4cca9c`; cleanup verified empty.

Authority: AGENTS.md, D-015/D-023/D-028/D-029/D-034/D-050/D-057, SLICE_005/013/018, and journey catalog08. No application changes or shared contract changes.

Owned files: `e2e/families/operator.spec.mjs`, `e2e/support/operator-seed.mjs`, `e2e/support/operator-provider.mjs`, this record. Seed `operator-v1` reuses operational setup, then typed APIs create two Persons in separate Organizations and an admin-created task assigned to Blair. Alice is neither creator nor assignee of that protected task.

Eight dependent browser steps exercise:

1. Search through real Operator tool dispatch and follow a server-returned Person card to the conventional Person UI.
2. Read Person and explain Today through the real tools; compare the result with the authoritative Today API.
3. Propose task creation, inspect the server proposal, verify no task exists, dismiss locally and verify the inert server proposal remains unconfirmed.
4. Repeat and confirm through the browser; exactly one new task persists, observer receives task invalidation/refetch, Today references it, repeated confirmation returns409 without duplication.
5. Complete through the approved Operator tool, inspect server receipt, observe durable completion elsewhere, Undo through the normal reopen command and verify persistence/realtime.
6. Attempt a task completion Alice cannot manage and a foreign Person read; no receipt/no unauthorized mutation, foreign read returns not-found with no references.
7. Controlled inference429 surfaces as Operator503, leaves task state unchanged, and records provider failure in the metadata-only audit.
8. Reload preserves business effects while ephemeral cards disappear; exact book, actor/Organization-scoped audit, no transcript content in audit rows, channel scope, and no unplanned external calls.

The production Groq adapter still performs real HTTP serialization/deserialization and the application still executes its full tool loop and typed queries/commands. Only inference is deterministic: `/operator-control` installs bounded tool-call responses; `/openai/v1/chat/completions` returns OpenAI-compatible messages. It never returns fabricated CRM endpoint responses. Mock evidence records scenario, tool names, counts and status only, never prompts, tool arguments, tool-result bodies or credentials. A generic scripted final sentence is not a language-quality assertion.

Configuration required from the parent harness: synthetic `GROQ_API_KEY`, `CRM_OPERATOR_BASE_URL=http://127.0.0.1:9001/openai/v1`, and a container-local proxy from9001 to family-local mocks9000 because the application admits plain HTTP only on loopback. Provider module exports `handleProvider(req,res,body,requests)` where body is parsedJSON or UTF8/stringBuffer, returns true when handled. The module also supports `{scenario:'call',personId,contactMethodId?}` for the call family's server-owned proposal check. No real provider/inference access.

Limits: natural-language quality, prompt-injection resistance of a real model, provider streaming, ambiguous name/private saved-list resolution, authorization change during confirmation, uncertain confirmation delivery and provider timeout are not claimed. The representative provider error uses rate limiting deliberately to avoid retries. Saved-list browser filtering is independently covered by its family; calling proposals join the calls family. The permanent audit is checked for content absence but this is not a general production log-retention audit.

Checks: all three new `.mjs` files pass `node --check`. Coordinator owns Docker execution and final evidence.

Runtime evidence: [report](../../.e2e/runs/18502a4cca9c/operator-1-a1/html/index.html), [step outcomes](../../.e2e/runs/18502a4cca9c/operator-1-a1/steps.json), [run/image/cleanup metadata](../../.e2e/runs/18502a4cca9c/operator-1-a1/run.json). The coordinated batch exercised both families concurrently with other families.
