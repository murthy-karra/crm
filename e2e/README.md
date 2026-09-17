# Stateful Web journeys

Implemented families: `workspace` (01), `leads` (02), `routing` (03),
`correspondence` (04), `relationships` (05), `tasks` (06), `lists` (07),
`operator` (08), `calls` (09), and `migration` (10). Each implements a bounded
stateful path with tenant isolation and persistence checks; see the family records
in `docs/tasks/WEB_E2E_*.md` for exact coverage and residual scenarios.

Latest combined verification: **10 families, 103 steps passed**, concurrency 4;
all five images reused and cleanup verified. The attempt window was 160.8 seconds.
See the [verification record](../docs/tasks/WEB_E2E_REMAINING.md) for evidence and
remaining branches. Coverage is representative, not every catalog scenario.

## Run

`./scripts/check-db` automatically runs every registered family after the database
integration suite succeeds, using four concurrent environments and a 900-second
per-family deadline. A failed journey fails the testing gate. Override parallelism
with `CRM_E2E_CONCURRENCY`; image caching and owned cleanup remain automatic.
`./scripts/check` remains service-free and includes the runner safety/cache tests.
The commands below are available for focused E2E work independently of that gate.

Prerequisites: Docker with Compose v2 or newer, Python 3.10+, and enough Docker
memory/disk for the Rust build and concurrent Chromium stacks. Host Node, Rust,
PostgreSQL and application credentials are not needed. Initial builds download
pinned toolchain images and locked dependencies; subsequent builds use Docker's
immutable layer cache. Application processes run without internet egress.

```sh
./scripts/e2e --family leads
./scripts/e2e --family workspace
./scripts/e2e --family leads --family workspace --concurrency 2
./scripts/e2e --all --concurrency 3
# Independent copies can also exercise runner isolation:
./scripts/e2e --family leads --copies 2 --concurrency 2
python3 -m unittest e2e.test_runner -v
```

The runner builds the real Rust binaries and production Vue bundle entirely in
Docker. It never loads the checkout `.env`, uses shared service ports, or writes
to host build outputs. Each family attempt receives a unique Compose project,
internal network, PostgreSQL volume, random credentials, mock process, browser
state and artifact directory. No host ports are published. Only immutable images
are reused. The runner automatically reuses existing API, Web and browser images
whose input-content hashes match the checkout, skipping Docker builds entirely.
An API source change rebuilds the API image; a Web change rebuilds Web; a journey
or browser dependency change rebuilds the browser image. Host runner and README
edits require no image rebuild. Shared Dockerfile/ignore changes invalidate all
targets conservatively. Deleted images are rebuilt using Docker's cached layers.
All selected families share these immutable images, prepared once before launch.
Calls additionally uses a Web image that substitutes the external LiveKit SDK;
migration uses an API example with the existing test-support Reader seam. These
optional images are built only when their families are selected.
Concurrent runner invocations coordinate builds with process locks.

`build-cache.json` records each target's input hash, immutable ID and reuse result.
`--rebuild` explicitly invokes the builds again while retaining Docker layer
caching. `--images .e2e/runs/<run>/images.json` explicitly selects an earlier build,
bypassing automatic source matching; use it only when that older build is intended.
Database migrations and API seed creation still run once per fresh family because
databases, sessions, containers and writable state must remain isolated. No rebuild
or reset happens between steps in a family.

One Playwright test contains ordered, named steps. A failure stops dependencies;
`steps.json` distinguishes failed and blocked steps. There are no independent
Playwright retries or database resets inside a family. `--retries 1` retries the
whole family in a new environment only after verified cleanup. Application retry
tests deliberately reuse the same operation identity in the original environment.

`--timeout 420` bounds each attempt after builds. SIGINT, SIGTERM, test failure,
setup failure and timeout collect diagnostics and remove owned resources. A hard
host/daemon crash or SIGKILL cannot execute cleanup; use the printed project ID
with `./scripts/e2e --cleanup <project>` after recovery. Ownership labels are
checked on the configuration and live containers, networks and volumes. Cleanup
never uses global prune. Failed environments are retained only with
`--keep-failed`; the exact cleanup command is printed. Private credentials remain
under `.e2e/private/<project>` until cleanup and must not be shared.

For a deliberate diagnostics failure, use `--fail-step 2`. This should return
nonzero, report step 1 passed / step 2 failed / later steps blocked, retain traces
and screenshots, and verify resources removed. Combine with `--keep-failed` to
exercise explicit retention and subsequent cleanup.

## Seed: leads-v1

The family starts with an operational empty book, because onboarding is a
different family. Run every real database migration. Database initialization
creates only roles and grants. The supported `crm-admin` bootstrap command creates
the platform identity. Real authenticated HTTP APIs then create two Organizations,
invite and accept their members, and create the normal default stages:

| Actor | Organization | Role / purpose |
|---|---|---|
| Platform | None | Create workspaces and initial invitations |
| Admin | Journey Realty | Membership revocation |
| Alice | Journey Realty | Lead creation, contact, repeat inquiry and handoff |
| Blair | Journey Realty | Realtime observer and new assignee |
| Casey | Other Realty | Foreign-tenant denial and positive channel canary |

Seed verification requires two Organizations, four memberships, eighteen default
stages, all expected migration records, and zero People. No tasks, saved-list
Today sources or customer records are inserted. IDs/passwords are intentionally
fresh; actor roles, stages and logical seed content are deterministic. Lead last
names carry the unique family project marker. SQL assertion access uses a separate
read-only role and `BEGIN READ ONLY`; business state changes exclusively through
the application.

## Reusable operational setup

`support/operational-seed.mjs` creates and verifies the common Organizations,
identities, memberships and stages once per family. Leads and routing begin with
an empty book. Tasks and relationships add one Person, and lists add a small
cross-tenant book and tag through typed APIs. Each family owns only its additional
prerequisites. Workspace continues to test onboarding itself from a platform-only
seed. All families share actor/session, mutation, realtime, video and diagnostic
helpers; none depends on another family's execution or database.

## Evidence and boundaries

The scenario observes HTTP and WebSockets before actions. Navigation-aborted requests retain explicit failure evidence; response inspection
settles on the browser’s failure event. Navigation that interrupts response
inspection retains the real HTTP status and an explicit `responseUnavailable`
marker; the recorder never invents a missing body. Business checkpoints still
assert actual responses, database state and visible results. A fifteen-second
diagnostics deadline fails
the test if evidence cannot be collected, including during final actor cleanup.
Mutation checkpoints assert response contracts and PostgreSQL facts/counts/actor attribution. For live
updates, another actor must receive a real Centrifugo publication, issue the
authoritative GET after that frame, and display the change within eight seconds
(before the application's sixty-second periodic refresh). Reload and a new login
verify durable state. Cross-tenant reads/writes fail without durable effects, and
every observed subscription/publication belongs to the actor's Organization.

Recovery deliberately drops one **real committed** HTTP response, then retries the
same submission ID. It takes Blair offline, commits another stage, injects a
Centrifugo publication failure through a transparent family-local HTTP proxy,
then proves reconnect/refetch recovers PostgreSQL state without replayed events.
Admin deactivation verifies real disconnect, session rejection and retained history.

The mock's `/control` toggles publication failure; `/evidence` records method,
path, outcome and family identity without credentials/content. Normal realtime
requests are forwarded to real Centrifugo. Unexpected external requests fail
closed. Network egress is denied. Operator and calls use fresh synthetic credentials
and family-local provider adapters. Migration uses an injected source Reader over
a private HTTP fixture. Routing and correspondence enable the inbound adapter
with a fresh synthetic bearer, posting RFC 822 bytes on their private networks.
The calls fixture exercises signaling and application state, including controlled
SDK microphone failure; it does not capture audio or use an actual media room.
Migration uses synthetic release readiness and does not prove deployed fleet
compatibility. No live provider operation is part of these families: this does not
verify email delivery, inference, FUB HTTP, WebRTC, SIP, audio or carrier behavior.

The development API retains its loopback bind restriction: a container-local
TCP relay exposes it only on the private family network. Full headless Chromium
uses a secure-origin flag for `http://web:8080` so `crypto.randomUUID` behaves as on
localhost. This is not TLS/certificate or production identity-provider evidence.
Browser timezone is America/Los_Angeles; Rust/PostgreSQL use actual time. This
family asserts relative event ordering and has no exact due-time boundary cases.

`.e2e/runs/<run>/` contains source revision/input hashes, immutable image IDs,
build logs, summary and pairwise isolation proof. Each attempt includes:

- `run.json`, verified seed, step status, Playwright JSON/HTML report;
- sanitized HTTP, realtime, database, console and mock evidence;
- per-actor traces, videos on every run, and failure screenshots;
- timestamped API/worker, PostgreSQL, Centrifugo, Web and mock logs;
- container/network identities and verified teardown result.

Secrets are redacted after writers stop, including nested trace ZIPs and the
embedded ZIP in HTML reports. Only synthetic data is used. Reports may lose
credential-bearing replay details intentionally. Build/package images remain
cached; owned runtime containers/networks/volumes are removed. Concurrent proof
compares actual overlapping execution, disjoint networks, Organization/Person
IDs, exact family data canaries, channel scopes and mock identities.

Videos are saved under each attempt's `videos/` directory and attached to the
Playwright HTML report. Actor names identify each recording; a fresh login uses a
new recording. They capture the actual test speed, without narration or audio.

## Seed and coverage: workspace-v1

This family begins with only the CLI-created platform identity: one user, no
Organizations, memberships, invitations or People. A real login verifies that
identity has no tenant context; SQL verifies the empty starting state and all
migrations. Workspaces, default stages, invitations and memberships are then
created through the real Web frontend, not pre-seeded.

The platform actor creates two uniquely named Organizations. The primary admin
invites Alice and Blair; Charlie joins through a replacement invitation. Casey
administers the foreign Organization. A recovery admin joins through a platform
invitation. Each actor has its own browser context and a maximum of one tab.

The stateful sequence covers creation, first-admin acceptance and replay denial,
member invitations, forbidden admin/platform actions, last-admin protection,
promotion/demotion, invitation revocation/supersession, deactivation and real
Centrifugo disconnect, reactivation with a new login, tenant-scoped denial,
platform promotion/invitation recovery, and logout/login as a different tenant in
the same browser context. A single Person created by Alice proves retained
attribution and provides a visible cross-tenant cache canary. Both families reuse
`support/journey.mjs`, `actions.mjs`, and `evidence.mjs`.

Membership changes publish no application event under Slice 004 §6; the tests
use explicit refresh and real authorization checks for those transitions.
Deactivation must disconnect Centrifugo. The canary Person creation also checks
a real publication → authoritative refetch → another actor's updated view.
Read-only database checkpoints verify memberships, sessions, default stages,
invitation resolution and immutable fact attribution, including rejected writes.

Invitation link fields and routing intake addresses are visually masked by test-only CSS installed before the
application renders, while their actual values remain asserted. Raw tokens are
registered with the artifact sanitizer. This prevents credentials appearing in
videos and screenshots without replacing responses or changing application code.

Not covered here: exact invitation-expiry boundaries, simultaneous last-admin
races, existing-user invitation rejection, and recovery from a genuinely
admin-less previously active workspace. The supported UI prevents removing its
last admin; the family verifies the always-available platform recovery paths
without bypassing that invariant through SQL. No outbound invitation email exists
in the Slice 004 contract, and no real email is sent.

## Extend

Add a registry entry with a versioned seed and ordered step names, a family spec,
and supported API/CLI seed implementation registered in `support/seed.mjs`.
Use `createJourney` for actor contexts, named-step failure/blocked reporting,
traces, videos and cleanup; shared actions cover login, mutation observation and
realtime assertions. Keep dependent actions in one test and
keep alternative records in that same database. Add family-local deterministic
provider adapters only when the scenario needs them. Respect D-015, D-021, D-023,
D-027, D-050 and the current slice contracts; new shared seams require a separate
contract proposal. Do not skip a failing assertion or change product behavior to
make the harness green.

Not yet covered: correspondence, Operator, calls, migration/worker resumption,
successful unresolved-to-Person extraction, forwarded mail, built-in Today rule
tuning, ambiguous contact matches, and the per-family edge cases documented above
and in the task records. Task/inquiry and overlapping saved-list Today reasons
are covered by their respective families. This is bounded journey coverage, not
a claim that every catalog branch is implemented.

References: [catalog](../docs/plans/WEB_E2E_JOURNEY_CATALOG.md),
[Playwright Docker](https://playwright.dev/docs/docker),
[Compose networking](https://docs.docker.com/compose/how-tos/networking/).
