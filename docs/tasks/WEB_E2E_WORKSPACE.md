# Stateful workspace access and membership E2E

Current status: implemented and verified. Both families passed concurrently in
run `0604a0366769`: workspace 12/12, leads 11/11. The user explicitly approved
fixing the invitation-acceptance navigation defect discovered by the new test.

Scope: second family selected by the user, catalog 01. Run it in parallel with
the existing lead family, reusing immutable images and common harness support.
The approved router fix restores the specified invitation-success navigation.
No shared contracts, database migrations or shared development resources changed.

## Implementation

`workspace-v1` starts with only the supported CLI-created platform identity.
The real Web frontend creates two workspaces, their default stages, and six
memberships through invitation acceptance. Twelve dependent steps cover opening
a workspace, role restrictions, last-admin rejection, role history, revocation
and supersession, deactivation/realtime disconnect/reactivation, cross-tenant
denial, both platform recovery entry points and logout/login as another tenant.

Read-only database checkpoints prove durable state and fact attribution.
A Person created by Alice supplies a retained-assignment and cache-isolation
canary, with a real Centrifugo → API refetch → observer UI assertion. Per
Slice 004 §6, membership edits use authoritative reads rather than expecting
nonexistent membership realtime publications.

Both families reuse actor sessions, named-step reporting, masked video capture,
traces, mutation observation and realtime assertions in `support/journey.mjs`
and `support/actions.mjs`, plus the existing evidence recorder, Docker runner,
read-only audit role, fault proxy and automatic image cache. Each family still
owns its complete runtime environment and seeds exactly once.

Invitation credentials are registered for textual artifact redaction and masked
visually before dialogs render, including video and failure screenshots. The
actual link input value is still asserted against the real response.

## Boundaries

Decisions: D-015, D-016, D-021, D-023, D-026, D-027, D-050 and resolved O-007.
Specifications: Slice 001 identity/session and Slice 004 administration, with
existing current operational-workspace semantics. No contract conflict found.

Exact invitation expiry, concurrent mutual-demotion races, existing-user
acceptance rejection, and an already-active workspace with zero admins are not
covered. Platform recovery is exercised while an admin remains, as D-026
explicitly allows; SQL does not bypass last-admin protection to manufacture an
unreachable UI state. No external provider or real email delivery is exercised.

Commands and seed details: [E2E README](../../e2e/README.md).

## Verification and discovered defect

Run `6175235ce74c` launched `workspace` and `leads` concurrently in independent
Docker stacks. Leads passed all 11 steps after shared-helper extraction.
Workspace passed its first two steps, failed step 3, and blocked steps 4–12.
Run `ca8584a7be0b` reproduced the same workspace failure using cached API and Web
images. Both runs verified complete owned-resource cleanup.

Reproduction: platform creates a workspace and invites its first admin; a new
browser context opens the real invitation and submits Create account.
`POST /api/invitations/accept` returns 200, the expected membership and
`membership_changed` fact commit, and the subsequent `GET /api/me` returns 200
with the correct Organization/admin identity. The browser stays on
`/invite/<redacted>` rather than reaching Today, contrary to Slice 004 §10.
The assertion deliberately failed; no manual navigation bypass was introduced.

Evidence is in `.e2e/runs/<run>/workspace-1-a1/`: sanitized HTTP/database ledgers,
failure screenshot, admin video, trace, steps and Playwright report. Eight
Python runner tests, the Node redaction test, Node syntax checks and diff
whitespace checks passed.

The approved fix skips session-recovery route replay for public pages that
explicitly allow authenticated visitors. Those pages need no authorization
replay; replaying the invitation URL was cancelling its concurrent navigation to
Today. Private-route guards and login redirection remain in force. The router
regression reproduced the failure before the fix and passed afterward (41 router
tests). Lint and typecheck passed, and the production Web image rebuilt while API
and browser images were reused.

Successful concurrent run `0604a0366769` records 23 passed steps, disjoint Docker
networks, Organizations and People, per-family mock/channel/data assertions,
videos, and verified cleanup. The first full Web unit run passed 1327 tests but
hit the existing wall-clock-sensitive PersonDetailView call-finishing assertion;
its isolated recheck passed all 120 tests. The full-suite recheck passed all 1328
tests across 103 files. No PersonDetailView code or test changes were made.
Final artifact checks confirmed actual browser execution overlapped for 24.2
seconds and no owned runtime containers, networks or volumes remained.
