# Initial stateful Playwright family

Status: implemented and verified, 2026-09-17. Application baseline: `64c4671`.
Scope: user's stateful E2E implementation brief, first vertical proof (catalog
02a–02d). This does not complete the rest of the catalog.

Implementation and contributor commands: [e2e/README.md](../../e2e/README.md).
The harness owns its Docker resources and has not modified shared development
services, application behavior, persistence contracts or migrations.

## Delivered

- Docker-only Rust/API workers, production Vue build, PostgreSQL, Centrifugo,
  Chromium and controlled family-local fault proxy. No published ports.
- Versioned API/CLI-created seed with two Organizations and independent actors;
  one database for all sequential named steps, read-only SQL assertions.
- Lead creation, Today response, repeat inquiry, stage/assignment handoff,
  durable reload/fresh login, tenant denial, committed-response loss and safe
  retry, missed-event/publication-failure recovery, session revocation.
- HTTP, PostgreSQL, real WebSocket→refetch→UI evidence, sanitized traces/reports,
  failure/blocked reporting, resource ownership checks and bounded teardown.
- Family selection, concurrency limits, independent copies, whole-family retries,
  explicit retention/cleanup and deliberate-failure diagnostics mode.
- Automatic reuse of API/Web/browser images by target-specific input hashes.
  Builds happen only for changed or missing images, once before family execution;
  concurrent invocations coordinate builds. `--rebuild` is an explicit override.
  Browser recordings are retained and attached on successful and failed runs.

## Verification evidence

Evidence is local, ignored by Git, under `.e2e/runs/`. Each run records source
revision, input hashes, image IDs and resource identities. Final host-runner
verification reused immutable browser/API/Web images from `9464d3bc4a73`;
subsequent changes concerned host ownership/diagnostics and documentation.

| Check | Result / run |
|---|---|
| Clean isolated image builds, migrations and seed | Passed; `9464d3bc4a73` |
| Two concurrent copies, 11 steps each | Passed twice; final `3fbc3d57e175`, 22/22 steps |
| Pairwise network, Organization and Person isolation | Passed; final `isolation-proof.json`, overlapping execution true |
| All implemented families (`--all`) | Passed; `9fcc6e1c0e7c` (currently only leads) |
| Deliberate failure at step 2 + keep failed | Expected nonzero; `e356f97d3178`, 1 passed / 1 failed / 9 blocked |
| Explicit cleanup of retained environment | Passed; owned containers/network/volume removed |
| Retained trace/HTML secret scan | Passed; 184 artifact entries, nested ZIPs decoded |
| Timeout during setup | Expected nonzero; `cf4479b83c29`, cleanup verified empty |
| SIGTERM during setup | Expected interrupted result; `ba164ee93938`, cleanup verified empty |
| Whole-family retry after deliberate failure | Expected nonzero; `5be8a3b2cc3b`, distinct a1/a2 projects, both cleaned |
| Python runner safety tests | 6 passed: ownership, shared-name rejection, redaction, timeout/interruption, private manifest |
| Node evidence redaction test inside Docker | 1 passed |
| Node syntax checks and `git diff --check` | Passed |
| Automatic image reuse | `586b3160967f` prepared keyed images; `13e19f436055` reused all three with zero builds, 11/11 steps passed, cleanup verified empty |
| Runner tests after cache support | 8 passed, including source-change/deletion invalidation, unrelated-input exclusion, cache hits and missing-image rebuild |

Initial harness runs exposed setup assumptions (development API loopback binding,
secure-context APIs under headless-shell), a Today locator mismatch, and the
membership endpoint's PUT method. These were fixed in the harness; no production
workarounds or contract changes were made. Successful reruns cover those fixes.

## Limits

Tested on the current Docker Desktop host with Docker 29.4 / Compose 5.1.2.
Browser/seed processes use the host UID/GID so private artifacts remain readable
on bind-mount hosts; other host platforms have not been separately verified.
The secure-origin browser flag does not test production TLS. Local development
identity is used, not ZITADEL. External providers are disabled and egress blocked;
the fault proxy forwards normal publication to real Centrifugo. Provider adapters,
media and delivery are not verified. SIGKILL/host failure requires explicit cleanup.

See README for uncovered lead branches and the remaining nine catalog families.
No broader application test suite was rerun for these harness-only changes.
