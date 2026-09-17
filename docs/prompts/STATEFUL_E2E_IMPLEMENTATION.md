# Stateful E2E implementation prompt

Build the Playwright E2E system using the reviewed journey catalog at:

`docs/plans/WEB_E2E_JOURNEY_CATALOG.md`

Follow AGENTS.md, the applicable accepted decisions, and the current specifications. This task owns the E2E harness and tests. Do not silently change application behavior or shared contracts to make tests pass.

The following requirements define the test architecture.

## 1. Isolation boundary

Each journey family, for each invocation, must receive its own completely isolated Docker environment containing:

- Playwright with a real headless Chromium browser
- The real Web frontend
- The real Rust API and application workers
- A real Centrifugo instance
- A real PostgreSQL instance
- Family-local external-service mocks

Isolation is per family, not per individual step.

Each family owns its containers, Docker network, database, writable volumes, browser state, mock state and artifact directory. No mutable resource may be shared between families or concurrent runs.

Immutable application images may be built once and reused.

Do not use or reset the shared development environment. Do not connect tests to its API, PostgreSQL, Centrifugo, credentials or external accounts.

Use unique run/family identifiers. Avoid fixed host ports; prefer communication inside each family's Docker network. Any necessary published ports must be assigned without collisions.

## 2. Stateful execution within a family

Run dependent journeys within a family in a defined sequence.

Create and seed the family database once. Do not truncate, reset, restore or recreate it between the family's journeys or steps.

The state accumulated by preceding steps must remain available for later steps to inspect and use.

Use separate records within the same family when alternative or destructive branches need different starting conditions.

Prefer an execution structure with clearly named steps and useful reporting. Do not spread required state dependencies across tests that the runner may reorder or independently retry.

If a prerequisite step fails, stop dependent steps and mark them as blocked rather than producing misleading cascading failures.

An infrastructure or test retry must restart the whole family with a fresh environment and the same deterministic seed. Intentional application retry/recovery tests must happen inside the current family environment.

## 3. Appropriate starting state

Define a versioned, deterministic seed for each family.

Start each family at the earliest meaningful prerequisite state for the behavior it tests. Do not force every family to repeat workspace onboarding or the entire migration ladder.

Examples:

- Onboarding starts with the minimum platform identity required to create an Organization.
- Lead workflows start with an operational Organization, members and default stages.
- Tasks and relationship editing start with suitable People and permissions.
- Saved lists start with a small book spanning relevant assignments, stages, tags, fields and activity.
- Operator tests start with resolvable People, tasks and lists.
- Migration tests start with the appropriate qualified source evidence, import state or cohort for their specific workflow.

Every family must document its seed, actors, expected initial state and why that state is appropriate.

Create business data through existing typed application commands, supported APIs, CLI paths or legitimate test-support fixtures that preserve the same rules. Do not seed by bypassing business authorization, migration review holds, history requirements or database constraints.

Run the real database migrations. Verify the seed before starting browser steps.

Provide multiple actors and a second Organization wherever needed to verify role boundaries and tenant isolation.

## 4. Real application stack and controlled external boundaries

The browser must exercise the real frontend, which must call the real API, typed command layer and PostgreSQL.

Use a production Web build served inside the family environment, including correct API and WebSocket routing. Isolate build outputs from shared development artifacts.

Do not fabricate CRM API responses or replace application persistence/realtime behavior with browser mocks.

LiveKit, Telnyx and FUB may be mocked at clearly documented external integration boundaries.

Also make inference/Groq and external email delivery deterministic. No real customer data, paid inference, outbound telephone calls, external messages or live FUB requests are permitted.

Keep mocks family-local, reset only during family startup, and give them explicit scenario controls and request recording.

Exercise real application parsing, validation, authorization, command execution and workers behind those boundaries.

Document exactly what each mock bypasses. Mocked call-state tests must not be described as verification of real WebRTC, SIP, audio or carrier behavior. Likewise, migration Reader fixtures do not prove the live FUB HTTP adapter.

## 5. Assertions at meaningful checkpoints

Each journey must verify all applicable layers at the points where state changes occur.

Browser:

- Visible state, navigation and feedback
- Loading, validation, denied, empty, partial and failure states
- Another actor's view where relevant
- Persistence after reload or a fresh signed-in browser context

HTTP:

- Which requests actually fired
- Method, path, meaningful headers and request payload
- Response status and relevant response payload
- Resource IDs, revisions and idempotency identifiers
- Required ordering where there is a causal dependency
- Absence of unauthorized, premature or duplicate requests

Register request observation before triggering the action. Assert meaningful contract fields; avoid brittle snapshots of volatile IDs, timestamps or unrelated request ordering.

Database:

- Read-only assertions against the family's actual PostgreSQL instance
- Expected row creation or mutation, counts and relationships
- Organization and actor attribution
- History, provenance, revisions, tombstones and reconciliation where relevant
- Absence of duplicate or unauthorized writes

Check database state during the journey at appropriate commit boundaries, not only at the end. Do not use SQL assertions to perform the application work being tested.

A successful response or browser toast alone is not sufficient evidence.

For forbidden actions, prove there were no unauthorized durable effects. Where appropriate, also verify that no external mock received a request.

## 6. Realtime and recovery

Use actual Centrifugo connections.

For mutations that promise realtime invalidation, verify:

- The committed mutation
- The relevant real WebSocket event
- The resulting API refetch
- The observer browser's updated state

Do not allow periodic polling to disguise a broken realtime path.

Use separate browser contexts for distinct actors, normally one active tab per actor, consistent with D-050.

Include representative scenarios for:

- Missed events and reconnect/refetch recovery
- Successful database commit despite realtime publication failure
- Session revocation and disconnect
- Cross-Organization channel isolation
- Lost HTTP responses after real commits
- Safe retry using the same operation identity where supported
- Worker interruption and durable resumption for migration workflows

Respect existing contracts: not every mutation publishes an event, and not every create operation is idempotent.

## 7. Determinism and waiting

Wait for observable readiness and state transitions, not arbitrary sleeps.

Health checks must establish that PostgreSQL is migrated, the seed is complete, the API is ready, the Web build is served and Centrifugo is available before browser execution begins.

Use bounded condition-based waits for eventual state.

Define a consistent approach to dates, timezones and due-time windows. Browser clock manipulation alone does not control Rust or PostgreSQL time.

Use comfortable time margins for ordinary journeys. If exact clock-boundary scenarios require new test seams, document the proposal before changing shared contracts.

## 8. Full parallel execution

All families must be capable of running concurrently without resource collisions, shared mutable fixtures, global resets or ordering dependencies.

Provide commands to run:

- One family
- A selected set of families
- All families in parallel

Expose a concurrency limit for available machine resources without imposing logical serialization between families.

Separate distinct actors inside a family from separate family execution environments.

Verify concurrency with an actual concurrent run, not merely a configuration setting. Include evidence that family data, events and mock requests remain isolated.

Respect existing restrictions on overlapping database gates or shared verification resources; this new runner must own its resources independently.

## 9. Diagnostics and cleanup

Produce a per-run/per-family report containing:

- Source revision and image identities
- Seed/scenario identity
- Passed, failed and blocked steps
- Relevant HTTP request/response evidence
- Realtime evidence
- Database assertion results
- Browser console errors
- Playwright traces and failure screenshots
- API, worker, Centrifugo, PostgreSQL and mock logs

Use synthetic data and redact credentials, cookies, tokens and other secrets from retained artifacts.

Collect diagnostics before teardown.

Clean up the family's owned containers, networks and volumes on success, failure, setup error, interruption and timeout.

Allow an explicit local debugging option to retain a failed environment. Print its resource identifiers and exact cleanup command. Never delete resources owned by another run or shared development.

## 10. Delivery sequence and acceptance

First inspect the existing Playwright migration acceptance script, Rust fixtures and application seams. Reuse suitable foundations without carrying over shared-resource assumptions.

Implement one complete vertical proof first:

New lead → Today → log contact → repeat inquiry → reassign to another agent.

Include browser, HTTP, PostgreSQL and real Centrifugo assertions, plus persistence, tenant isolation and a representative recovery path.

Then extend the same architecture to the remaining catalog families.

Document coverage honestly, including mocked boundaries, unimplemented scenarios and known failures. Do not report the entire catalog as complete when only the initial family is implemented.

Completion requires:

- Reproducible setup from a clean checkout
- Isolated family environments
- Stateful family execution without database resets
- Real concurrent-family execution
- Meaningful assertions across the required layers
- Reliable failure diagnostics and cleanup
- Relevant repository checks passing
- Clear commands and documentation for future contributors
