# Stateful E2E expansion verification

Status: PASS, 2026-09-17. Four new families were authored in parallel using
subagents with separate file ownership. Existing application behavior and
contracts were not changed by this expansion.

## Final verification

`./scripts/e2e --all --concurrency 3` exited 0 in run `949f3e952fb3`.
All six families passed all 61 sequential steps. Three browser-family execution
intervals overlapped simultaneously. No family shared mutable state with another.

| Family | Catalog | Passed steps |
|---|---|---:|
| Workspace access | 01 | 12 |
| Lead handoff | 02 | 11 |
| Inbound routing | 03 | 9 |
| Relationship context | 05 | 10 |
| Task follow-up | 06 | 10 |
| Saved lists and Today sources | 07 | 9 |

The final run reused API, Web and browser images without invoking a build. Source
fingerprints matched the tested images. All 15 family pairs have disjoint
networks, Organization IDs and Person IDs. Every environment reports
`verified_empty` after teardown. External-service request counts were zero.

Evidence under `.e2e/runs/949f3e952fb3/`:

- `summary.json`, `isolation-proof.json`, `browser-overlap.json`, `build-cache.json`.
- Each family directory: `steps.json`, `run.json`, `html/index.html`, actor traces,
  WebM videos, HTTP/realtime/database evidence and service/cleanup logs.
- `runner-tests.log`: 9 runner safety/cache tests passed.

The same final browser image passed 5 diagnostic regression tests in
`.e2e/runs/fc602c15f396/support-tests.log`. That run also passed two simultaneous
independent copies of the lists family, 18/18 steps, with verified teardown.
`git diff --check` passed. No production application source changed in this
expansion, so existing application checks were not redundantly rerun.

## Changes and reuse

Added family specs, versioned seeds and coverage records for routing,
relationships, tasks and lists. Registration and seed dispatch now cover six
families. `support/operational-seed.mjs` extracts the common typed-API setup from
leads; each operational family adds only its necessary prerequisites. Workspace
continues to exercise onboarding itself. Actor contexts, mutation/realtime
assertions, database audit access, recordings and cleanup remain shared.

Only routing receives an ephemeral inbound-email credential. It supplies synthetic
RFC822 input directly to the real private API; it does not deploy or contact a
mail provider. Intake addresses are masked before rendering and registered for
artifact redaction.

Parallel verification exposed a shared recorder hang when navigation interrupts
Playwright response inspection. Actual aborts now settle pending capture. Navigation
interruption records the actual status plus an explicit unavailable-body marker,
never an invented response. Unexpected capture deadlines fail final flush.
Actor cleanup saves traces/videos even when recording fails. Regression tests
cover redaction, abort settlement, deadline failure, navigation interruption and
retaining completed response bodies. Business assertions were not weakened.

A bounded independent review of the shared seed and tasks found no concrete
issues. Recorder review identified a possible final-flush false pass; the fix and
fake-timer regression enforce failure on incomplete unexpected diagnostics.

## Failed runs and limitations

Initial authoring runs found seed/selector/status handling mistakes and a routing
observer subscription-readiness race; these were corrected in tests. Runs
`884311e89752` and `db8422e87988` exposed recorder failures and are not suite passes.
Run `ae66c40bf193` was deliberately interrupted to replace a superseded recorder
build. Every owned environment was removed. Timeout/interruption left incomplete
archives in two prior attempts; their sanitization failure is explicitly marked
and those artifacts remain private, not deliverable reports. Final evidence above
sanitized successfully.

Coverage is bounded. At this verification milestone correspondence, Operator,
calls and migration were still planned; their later implementation is recorded
in the individual family documents.
Routing does not yet cover successful inference extraction/retry or deactivated
round-robin members. Lists does not yet cover built-in Today rule tuning. Other
residual branches are listed in the individual family records. The existing
new-custom-field input lacks an associated accessible Label; the test uses its
existing test ID and records that limitation without changing the application.

Family records: [routing](WEB_E2E_ROUTING.md),
[relationships](WEB_E2E_RELATIONSHIPS.md), [tasks](WEB_E2E_TASKS.md),
[lists](WEB_E2E_LISTS.md). Run instructions: [harness README](../../e2e/README.md).
