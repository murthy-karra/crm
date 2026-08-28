# Slice 004 — Administration

Status: APPROVED (user, 2026-08-22; planner pass, then independent
review — 19 findings, all applied as safe defaults or implementation
notes, none blocking; §14 safe defaults and §5 declared contract changes
accepted as written).
Builds on: Slice 003 (`main` at `50bcd04` + D-027 docs commit `9f3b9b4`).
Targets: D-003 (roles are application-enforced facts), D-016 §3 (local
auth is a seam, identity federation parked), D-021 (administration
scope; no direct database writes), D-026 (admin continuity), D-027
(deactivation, not removal; suspension reserved), O-007 (design defaults,
confirmed here), thesis §16 proof chain: `an Organization exists → its
people can be let in and managed → everything else still works`.

Scope decisions (user-accepted 2026-08-22, not re-litigated here):
(1) every Organization should have an admin, but members never stop
working when it does not (D-026); (2) the platform admin can always both
promote an existing member and invite a new admin (D-026 §3); (3) members
are deactivated, never removed (D-027); (4) Organization suspension is
reserved in schema only (D-027 §5, O-009).

The narrowest cut that proves the chain with real tests:

1. membership `role` and `status`; the last-active-admin invariant;
2. one invitation mechanism (issue / supersede / revoke / expire /
   accept) used by both the platform admin and Organization admins;
3. a membership-free **platform admin** principal, bootstrapped only by
   a CLI running as the migrator role;
4. a `crm-admin` CLI that calls the same domain functions as the API;
   `seed.rs` and the raw-INSERT test fixtures are deleted;
5. web: a public invitation-acceptance page, a `Manage → Members` page
   for Organization admins, and a `/platform` area for the platform
   admin;
6. tests for tenant isolation across every new endpoint, the
   last-admin invariant under a real race, invitation token handling,
   and platform-vs-org authorization.

## 1. User-visible outcome

On the D-016 Mac, through the tunnel and on loopback:

1. `./scripts/dev-services down && up` → `./scripts/db-migrate` (one new
   migration) → `./scripts/dev-bootstrap` (replaces `dev-seed`): creates
   the platform admin `owner@platform.test`, then "Acme Realty" and
   "Best Realty" with Alice/Bob as **admin** and Carol/Dave as **member**,
   every row through domain functions. Re-running is idempotent.
   `./scripts/demo-leads` still works unchanged.
2. Log in as `owner@platform.test` → you land on **Platform →
   Organizations**: a table with name, members, admins, open admin
   invitations, and state `ok` / `pending first admin` / `needs
   attention`. The sidebar shows only the Platform group; there is no
   Today, People, or Intake — the platform admin has no Organization.
3. Create Organization "Cedar Realty" → it appears as `needs attention`
   (zero admins, no invitation). Click **Invite admin**, enter
   `erin@cedar.test` → the row becomes `pending first admin` and a
   one-time accept link is shown with a copy button (no email is sent;
   the link is never shown again — re-issue to get a new one).
4. Open the link in a private window → `/invite/<token>` shows "Cedar
   Realty has invited erin@cedar.test as an admin". Enter display name
   and a password → signed in as Erin, active Organization Cedar, Today
   empty, sidebar shows `Work`, `Intake`, and **`Manage`**. The platform
   list now shows Cedar as `ok`.
5. Erin opens **Manage → Members**: herself (admin). She invites
   `frank@cedar.test` as member; Frank accepts in another private
   window; Erin promotes Frank to admin; Erin demotes herself (allowed,
   Frank remains); Frank tries to demote himself → "You are the last
   active admin. Promote someone else first." Frank tries to deactivate
   himself → same message.
6. Frank deactivates Erin. Erin's open tab goes to `/login` on its next
   request (session invalid) and her realtime connection is closed. Erin
   cannot log in ("no active membership"). Members shows Erin as
   inactive with "0 People assigned". Frank reactivates Erin; she can
   log in again.
7. Frank invites `gina@cedar.test`, then revokes the invitation before
   it is used → the link now shows "This invitation is no longer
   valid". Frank re-invites Gina → a fresh link; the old one stays
   invalid.
8. Log in as Alice (Acme admin): People/Today/Intake unchanged; Manage →
   Members shows Alice (admin) and Carol (member). Log in as Carol: no
   `Manage` in the sidebar; `GET /api/organization/invitations` is 403.
   Alice cannot see or touch anything of Best Realty by id (404).
9. The platform admin gets 401 on `/api/people`, `/api/today`,
   `/api/inquiries`, `/api/stages` (no active Organization) and 403 is
   returned to Alice on every `/api/platform/*` route.
10. `./scripts/check` is green with no services; `./scripts/check-db` is
    green; every test fixture and the seed subprocess test now create
    data through domain functions, not SQL.

## 2. Domain and schema

### Concepts

- **Role** — `organization_membership.role ∈ {admin, member}`. "Admin"
  is the only authorization fact this slice adds inside an Organization.
- **Status** — `organization_membership.status ∈ {active, inactive}`
  (D-027). Inactive = may not log in to this Organization, existing
  sessions fail on next request, realtime disconnected, data retained.
- **Active admin** — membership with `role = 'admin' AND status =
  'active'`. The invariant (D-026 §2, D-027 §4) is about active admins.
- **Platform admin** — an `app_user` with a row in `platform_admin`.
  Not a role value; not reachable through `organization_membership`; has
  no Organization of its own. Only the CLI, running as `crm_migrator`,
  writes this table. No API route can mint one.
- **Invitation** — a single-use, expiring, Organization-scoped offer to
  an email address for a role. Exactly one open (not accepted, not
  revoked) invitation per `(organization, email)`; issuing again
  supersedes the previous one. The raw token is returned once at issue
  time and never again; only its SHA-256 hash is stored.
- **Acceptance** — "authenticate as the current identity provider
  requires, then claim the token." With local auth (D-016 §3) the
  authenticate step is *set a password*. In this slice invitations are
  claimable **only by an email that has no `app_user` yet**; an existing
  user cannot accept via set-password (that would let any inviter take
  over an existing account and its other memberships). Existing-user
  acceptance and Organization switching are one later increment (§12).
- **Organization state** (platform view, computed per request, D-026 §5):
  `ok` = ≥1 active admin; `pending_first_admin` = 0 active admins and ≥1
  pending (unexpired) admin invitation; `needs_attention` = 0 active
  admins and no pending admin invitation.
- **Organization status** — `organization.status`, only `active` is
  permitted in this slice (D-027 §5). Reserved; no behavior.

### Migration (one file, Lane A): `20260823000001_administration.sql`

```sql
ALTER TABLE organization_membership
    ADD COLUMN role TEXT NOT NULL DEFAULT 'member'
        CHECK (role IN ('admin', 'member')),
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'inactive')),
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE organization_membership
    ALTER COLUMN role DROP DEFAULT, ALTER COLUMN status DROP DEFAULT;
-- Explicit writes only from here on; the defaults existed solely to
-- backfill pre-004 rows. dev-bootstrap promotes Alice/Bob afterwards.

ALTER TABLE organization
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active'));   -- D-027 §5; widened by O-009
CREATE UNIQUE INDEX organization_name_lower_idx ON organization (lower(name));

ALTER TABLE user_session ALTER COLUMN active_organization_id DROP NOT NULL;
-- NULL = platform-admin session with no active Organization (§7).

CREATE TABLE platform_admin (
    user_id UUID PRIMARY KEY REFERENCES app_user (id),
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    granted_via TEXT NOT NULL CHECK (granted_via IN ('cli'))
);

CREATE TABLE invitation (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    email TEXT NOT NULL,                 -- normalized: trimmed, lowercased
    role TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    token_hash TEXT NOT NULL,            -- SHA-256, base64url, of the raw token
    invited_by_user_id UUID NOT NULL REFERENCES app_user (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    accepted_user_id UUID REFERENCES app_user (id),
    revoked_at TIMESTAMPTZ,
    revoke_reason TEXT CHECK (revoke_reason IN ('revoked', 'superseded')),
    CHECK ((accepted_at IS NULL) = (accepted_user_id IS NULL)),
    CHECK ((revoked_at IS NULL) = (revoke_reason IS NULL)),
    CHECK (accepted_at IS NULL OR revoked_at IS NULL)
);
CREATE UNIQUE INDEX invitation_token_hash_idx ON invitation (token_hash);
CREATE UNIQUE INDEX invitation_open_per_org_email_idx
    ON invitation (organization_id, email)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;
CREATE INDEX invitation_org_created_idx ON invitation (organization_id, created_at);
```

Invitation state is derived, never stored: `accepted` if `accepted_at`,
else `revoked` if `revoked_at`, else `expired` if `expires_at <= now()`,
else `pending`.

### Facts (D-007, D-015; SLICE_002 §2 envelope; append-only triggers)

Four typed fact tables, each with the full envelope (`organization_id`,
`actor_kind`, `actor_user_id`, `on_behalf_of_user_id`, `origin`,
`occurred_at`, `recorded_at`, `correlation_id`, `causation_id`,
`corrects_id`) and the same `reject_mutation` UPDATE/DELETE row triggers
and TRUNCATE statement trigger as `contact_attempted`:

| Table | Payload columns | Written by |
|---|---|---|
| `organization_created` | — | `CreateOrganization` |
| `invitation_issued` | `invitation_id UUID NOT NULL`, `role`, `superseded_invitation_id UUID` | `IssueInvitation` |
| `invitation_resolved` | `invitation_id UUID NOT NULL`, `outcome ∈ {accepted, revoked, superseded}` | `AcceptInvitation`, `RevokeInvitation`, `IssueInvitation` (supersede) |
| `membership_changed` | `user_id UUID NOT NULL`, `from_role`, `to_role`, `from_status`, `to_status`, `reason ∈ {invitation, bootstrap, promote, demote, deactivate, reactivate}` | `AcceptInvitation`, `ChangeMemberRole`, `SetMemberStatus`, CLI bootstrap |

`membership_changed.from_role` / `from_status` are NULL only for
`reason ∈ {invitation, bootstrap}` (creation), with `CHECK ((from_role IS
NULL) = (from_status IS NULL))`; `to_role` / `to_status` are NOT NULL.
PII rule: invitation facts carry the invitation id, never the email.
Payload columns are bare UUIDs without FKs, per the SLICE_002 §2 rule.

The first platform-admin grant has no actor and no Organization, so it
is recorded in the `platform_admin` row itself (`granted_at`,
`granted_via`), not as a fact; the envelope's `organization_id NOT NULL`
stays intact. Platform-admin actions *on* an Organization (create,
invite, promote) write facts with that Organization's id and
`origin = 'platform'`.

### Grants (`crm_app`; still no DELETE anywhere)

```sql
GRANT INSERT ON organization, app_user, local_credential,
                organization_membership, invitation, stage TO crm_app;
GRANT UPDATE (role, status, updated_at) ON organization_membership TO crm_app;
GRANT UPDATE (accepted_at, accepted_user_id, revoked_at, revoke_reason)
    ON invitation TO crm_app;          -- token_hash/email/expires_at immutable
GRANT UPDATE (password_hash, updated_at) ON local_credential TO crm_app; -- accept, CLI set-password
GRANT SELECT ON platform_admin TO crm_app;   -- read-only: cannot mint
GRANT SELECT, INSERT ON organization_created, invitation_issued,
                       invitation_resolved, membership_changed TO crm_app;
```

`stage::seed_defaults` moves from "migrator only" to the application
path (O-007 §6); SLICE_002 §2's "never by the application" sentence is
superseded and the `stage.rs` doc comment changes. The coordinator adds
a pointer line to SLICE_002 §2 at approval (AGENTS.md §11 item 6).

## 3. Platform admin and sessions

- `session::verify` becomes one statement that LEFT JOINs
  `organization`, `organization_membership`, and `platform_admin`, and
  returns a row **only** when its `WHERE` predicate holds:
  `(s.active_organization_id IS NULL AND pa.user_id IS NOT NULL) OR
  (m.status = 'active')`. The rule "organization present ⇒ active
  membership present" therefore lives in SQL, preserving the
  safe-by-construction property of today's INNER JOIN; a session whose
  active Organization has no `status = 'active'` membership, or a
  NULL-Organization session whose user is not a platform admin, matches
  no row. Rust additionally asserts the invariant on the returned row
  and treats a violation as invalid — defense in depth, not the primary
  control. The row carries `organization: Option<{id, name, role}>` and
  `platform_admin: bool`.
- Login: if the user has ≥1 active membership, the earliest is the active
  Organization (unchanged). If none, and the user is a platform admin,
  a session with `active_organization_id = NULL` is created. Otherwise
  403 `no_membership` (unchanged; inactive-only memberships land here).
  A platform admin who also holds a membership logs in as that member
  *and* is `platform_admin: true` — the two capabilities compose.
- `AuthContext` (tenant extractor) is unchanged in meaning: it requires
  an active Organization and now carries `role`. A platform-only session
  gets 401 `unauthenticated` from it, so every existing tenant route
  fails closed without modification.
- `OrgAdminContext` wraps `AuthContext` and rejects 403 `forbidden` unless
  `role == admin`.
- `PlatformAuthContext` requires a valid session and a `platform_admin`
  row, ignores the session's active Organization entirely, and takes the
  target Organization id only from the path.

## 4. Commands and queries

New module `domain/admin/` (commands + read models; layout is an
implementation detail). Every command takes a `CommandContext`, runs in
one transaction, writes its fact(s), and publishes nothing (§6).
`Origin` gains `Platform` and `Cli`.

| Command | Actor paths | Behavior |
|---|---|---|
| `CreateOrganization { name }` | platform, CLI | Trim; 1–120 chars; insert `organization` (`status = 'active'`); `stage::seed_defaults`; fact `organization_created`. Name collision → `OrganizationNameTaken`. |
| `IssueInvitation { organization_id, email, role }` | org-admin, platform, CLI | Normalize email (trim + lowercase; syntactic check: one `@`, non-empty local and domain parts, ≤254 chars). In one transaction: if an open invitation exists for `(org, email)`, mark it `revoked / superseded` and write `invitation_resolved {superseded}`; insert the new row with `expires_at = now() + CRM_INVITATION_TTL_HOURS`; write `invitation_issued`. Returns the raw token **once**. If the email belongs to a current member (any status) of the **target** Organization → `AlreadyMember` (membership in the admin's own Organization is already visible on the members list, so this leaks nothing and avoids a pending-forever row). Existence anywhere else is never disclosed (201 as for any other email; O-007 §3). |
| `RevokeInvitation { organization_id, invitation_id }` | org-admin, platform | `SELECT … FOR UPDATE` the invitation; if pending or expired → set `revoked / revoked`, fact `invitation_resolved {revoked}`; if already accepted → `InvitationUsed`; if already revoked → idempotent 204; unknown or other-Organization id → `NotFound`. |
| `AcceptInvitation { token, display_name, password }` | public (superseded 2026-08-24, §11: the `CLI (seed-dev)` path no longer exists) | Hash token; read the invitation without a lock and validate state first (`NotFound` if absent or revoked, `InvitationExpired` if expired, `InvitationUsed` if accepted; `InvitationNotAcceptable` if an `app_user` with that email exists — generic) and validate display name (trim, 1–120) and password (12–256); only then Argon2id under `spawn_blocking` (so a dead token never costs a hash); then open the transaction, `SELECT … FOR UPDATE` the invitation and re-check every condition. Insert `app_user`, `local_credential`, `organization_membership {role, status: active}`; set `accepted_at/accepted_user_id`; facts `invitation_resolved {accepted}` and `membership_changed {to_role, to_status: active, reason: invitation}` with `actor_user_id` = the newly created user. `origin` comes from the caller (`WebSession`); `CommandContext::from_auth` does not apply — the command builds its context after the `app_user` insert inside the transaction. Returns `(user_id, organization_id)`; the route mints the session. |
| `ChangeMemberRole { organization_id, user_id, role }` | org-admin (any direction), platform (**promote only**), CLI | Take `SELECT pg_advisory_xact_lock(hashtextextended('admin:' \|\| $1::text, 0))` — the keyed transaction-scoped advisory lock pattern `receive_inquiry` uses, under a distinct `admin:` namespace so membership changes never contend with intake — to serialize all role/status changes per Organization (a row lock is not used: `FOR UPDATE` needs an UPDATE grant on `organization`, which `crm_app` deliberately lacks). `NotFound` if the target is not a member of this Organization; no-op 200 if already that role. Role and status are independent: promoting an inactive member is allowed and does not count toward the invariant (only active admins count); the UI disables Promote for inactive rows (§10), a UX choice, not a domain rule. If the change would leave zero active admins → `LastAdmin` (both self and other; D-026 §2 applies to the whole self-service path). Platform path with `role = member` → `MalformedRequest` (never reaches the domain; the route rejects it). Fact `membership_changed {from_role, to_role, reason: promote\|demote\|bootstrap}`. |
| `SetMemberStatus { organization_id, user_id, status }` | org-admin | Same advisory lock; `NotFound` as above; no-op if unchanged. Deactivating the last active admin → `LastAdmin`. On `inactive`: `UPDATE user_session SET revoked_at = now() WHERE user_id = $1 AND active_organization_id = $2 AND revoked_at IS NULL`, then after commit call `realtime::disconnect_user(user_id)` (best-effort, `warn` on failure — same policy as publish in SLICE_003 §6). Fact `membership_changed {from_status, to_status, reason: deactivate\|reactivate}`. Reactivation requires nothing beyond the status flip. |
| `SetLocalPassword { user_id, password }` | CLI only | Argon2id re-hash; `UPDATE local_credential`. No fact (credential material is not a business fact; D-015). No route. |
| `GrantPlatformAdmin { email, display_name, password }` | CLI only, migrator connection | Create `app_user` + `local_credential` if absent (same normalization), insert `platform_admin` if absent; idempotent. |

Widened by SLICE_009 §4 (declared additive change, AGENTS.md §11):
`AcceptInvitation` and `SetMemberStatus`'s reactivation branch (`to_status:
active`) both additionally call `mint_capture_address_if_absent
(organization_id, user_id)` inside their existing transaction — a brand-new
membership always mints; a reactivated one restores its EXISTING address
(the row survived deactivation untouched), never a fresh one. Deactivation
itself is untouched: a capture address keeps existing, only its lookup
stops resolving (an active-membership JOIN at receive time, not a row
change here).

Queries (all scoped by Organization id supplied by the extractor or the
platform path, never by the client):

- `organization::list_for_platform()` → per Organization: `id, name,
  status, created_at, member_count (active), admin_count (active),
  pending_admin_invitations, state`. One statement with subqueries; no
  N+1.
- `organization::members(org)` → existing query plus `role, status,
  joined_at (created_at), assigned_people_count` (count of `person` rows
  with `assigned_user_id = member` in this Organization — D-027 §3). The
  existing `GET /api/organization/members` gains `role`, `status`, and
  `assigned_people_count` additively.
- `invitation::list(org)` → `id, email, role, status, expires_at,
  created_at, invited_by {id, display_name}`; never `token_hash`.
- `invitation::preview(token_hash)` → `organization_name, email, role,
  expires_at, state` for the public page.

## 5. HTTP contracts

All JSON; `{"error": code}` envelope; 401 `unauthenticated` / 503
`unavailable` as before; non-UUID path → 400 `malformed_request`. New
`ApiError` variants and codes:

| Variant | Status | Code |
|---|---|---|
| `Forbidden` | 403 | `forbidden` |
| `LastAdmin` | 409 | `last_admin` |
| `InvitationUsed` | 409 | `invitation_used` |
| `InvitationExpired` | 410 | `invitation_expired` |
| `InvitationNotAcceptable` | 409 | `invitation_not_acceptable` |
| `OrganizationNameTaken` | 409 | `organization_name_taken` |
| `WeakPassword` | 422 | `weak_password` |
| `InvalidEmail` | 400 | `invalid_email` |
| `AlreadyMember` | 409 | `already_member` |

CORS method list gains `PUT`.

### Organization-admin routes (`OrgAdminContext`; Organization from session)

| Endpoint | Request | Success | Errors |
|---|---|---|---|
| `GET /api/organization/members` | — (**`AuthContext`**, any member, unchanged access) | 200 `{"members": [{"user_id","display_name","email","role","status","joined_at","assigned_people_count"}]}` — `role`/`status`/`assigned_people_count` are additive to SLICE_001 §4 | 401 |
| `PUT /api/organization/members/{user_id}/role` | `{"role": "admin"\|"member"}` | 200 `{"member": {…}}` | 400, 403, 404 (non-member; byte-identical for other Organizations' users), 409 `last_admin` |
| `PUT /api/organization/members/{user_id}/status` | `{"status": "active"\|"inactive"}` | 200 `{"member": {…}}` | 400, 403, 404, 409 `last_admin` |
| `GET /api/organization/invitations` | — | 200 `{"invitations": [{"id","email","role","status","expires_at","created_at","invited_by":{"id","display_name"}}]}`; `status ∈ pending\|expired\|accepted\|revoked`; accepted, revoked, or expired older than 30 days omitted | 401, 403 |
| `POST /api/organization/invitations` | `{"email", "role"}` | 201 `{"invitation": {…}, "accept_path": "/invite/<token>"}` — the only response that ever contains the token; the client absolutizes with its own origin (no new config) | 400 `malformed_request` / `invalid_email`, 403, 409 `already_member` |
| `DELETE /api/organization/invitations/{id}` | — | 204 (idempotent for already-revoked) | 403, 404 (also for other Organizations' ids), 409 `invitation_used` |

### Public routes (no session; the token is the credential)

| Endpoint | Request | Success | Errors |
|---|---|---|---|
| `POST /api/invitations/preview` | `{"token"}` | 200 `{"organization_name","email","role","expires_at"}` | 400 `malformed_request` (bad JSON), 404 `not_found` (unknown, malformed token, or revoked), 409 `invitation_used`, 410 `invitation_expired` |
| `POST /api/invitations/accept` | `{"token","display_name","password"}` | 200 — body identical to `POST /api/session` (below) + `Set-Cookie` | 400, 404/409/410 as above, 409 `invitation_not_acceptable`, 422 `weak_password` |

The token travels in the JSON body, never in a URL the API serves: the
request span logs `uri` for every request (`lib.rs` `DefaultMakeSpan`)
and the tunnel logs request paths, so a path-segment token would be
written to logs on every call. The SPA route `/invite/<token>` is the
only place the raw token is in a path; Lane B reads it from
`route.params` and posts it. Token format is checked before any
database access exactly as `session::is_valid_token_format` (43-char
base64url); a malformed token is 404, not 400, so the public endpoint
leaks nothing about format. `accept` ignores any presented `crm_session`
cookie and always mints a fresh token (SLICE_001 §3 fixation rule); both
routes require `Content-Type: application/json` and any JSON-extraction
failure is 400 `malformed_request` — the same CSRF properties as login.
Display name or Organization name outside 1–120 after trim → 400
`malformed_request`; `weak_password` covers both < 12 and > 256. Both
public routes are rate-limited only by the pre-existing absence of
limits (login has none either; recorded gap, §12) — mitigated by the
256-bit token.

### Platform routes (`PlatformAuthContext`; Organization from path only)

| Endpoint | Request | Success | Errors |
|---|---|---|---|
| `GET /api/platform/organizations` | — | 200 `{"organizations": [{"id","name","status","created_at","member_count","admin_count","pending_admin_invitations","state"}]}` ordered `needs_attention`, `pending_first_admin`, `ok`, then name | 401, 403 |
| `POST /api/platform/organizations` | `{"name"}` | 201 `{"organization": {…same item shape…}}` | 400, 403, 409 `organization_name_taken` |
| `GET /api/platform/organizations/{id}` | — | 200 `{"organization": {…}, "members": [...as members list...], "invitations": [...as invitations list...]}` | 403, 404 |
| `PUT /api/platform/organizations/{id}/members/{user_id}/role` | `{"role": "admin"}` — **only** `admin` is accepted (D-026 §4) | 200 `{"member"}` | 400 (`role` ≠ admin), 403, 404 |
| `POST /api/platform/organizations/{id}/invitations` | `{"email", "role": "admin"}` — **only** `admin` is accepted (D-021 §1, D-026 §4: the platform power is admin continuity) | 201 as the org-admin form | 400 (`role` ≠ admin), 403, 404, 409 `already_member` |
| `DELETE /api/platform/organizations/{id}/invitations/{inv_id}` | — | 204 | 403, 404, 409 |

No platform route reads or writes `person`, `inquiry`, `raw_payload`,
`stage`, `contact_attempted`, or any CRM fact. No platform status route
exists (O-009).

### Declared changes to existing contracts (AGENTS.md §11; approved with this spec)

1. **SLICE_001 §3 login.** A user with zero active memberships who is a
   platform admin logs in successfully with no active Organization.
   Everyone else with zero active memberships still gets 403
   `no_membership`. An inactive membership is not an active membership.
2. **SLICE_001 §4 `POST /api/session` and `GET /api/me` response.**
   ```json
   {
     "user": {"id", "email", "display_name"},
     "organization": {"id", "name", "role"} | null,
     "platform_admin": false
   }
   ```
   `organization` becomes nullable and gains `role`; `platform_admin` is
   added. `web/src/api/types.ts` `MeResponse`, `router.ts`, and
   `AppShell.vue` change accordingly (Lane B).
3. **SLICE_001 §4 `GET /api/organization/members`** gains `role`,
   `status`, `joined_at`, `assigned_people_count` (additive).
4. **SLICE_002 §2** `stage` default seeding moves to the application path
   (grant added); "never by the application" is superseded.
5. **CORS** method list gains `PUT`.
6. **SLICE_003 §7** realtime token: a platform-only session has no
   Organization channel; `POST /api/realtime/token` goes through
   `AuthContext` and therefore returns 401 for it (no change in code,
   declared for clarity).

The coordinator adds pointer lines to SLICE_001 §3/§4 and SLICE_002 §2
at approval so each contract of record points here.

## 6. Realtime contracts

**No new events.** Role and status are read per request by the
extractors; the web invalidates `me` and `['org', orgId, 'members']`
after its own mutations; other admins' changes arrive by the existing
focus/60 s refetch. `realtime::disconnect_user(user_id)` is added (the
Centrifugo `disconnect` HTTP API, keyed by the `sub` claim = user id)
and called after commit by `SetMemberStatus(inactive)`; failure is a
`warn`, never an error, because the session is already revoked and the
next refresh (≤ token TTL) cuts the connection anyway (SLICE_003 §7).
Known and accepted: Centrifugo `disconnect` is per user, so a
multi-membership user's other-Organization connection is also closed and
reconnects on its next token refresh (their other sessions are untouched;
no data exposure). `Publisher::Disabled` is added for the CLI (SLICE_003
§14a).

## 7. Authorization and tenant isolation

> Amended by SLICE_007a §5 (declared additive change): `GET
> /api/platform/organizations/{id}` gained a top-level `intake_address`
> — an onboarding-configuration value, a recorded exclusion to D-021's
> "no tenant CRM data" rule for platform admins.

- Three extractors (§3). Every admin route is 403 for a member, every
  platform route is 403 for an Organization admin without a
  `platform_admin` row, every tenant route is 401 for a platform-only
  session.
- Platform handlers never use the session's active Organization; the id
  comes from the path and is validated to exist (404 otherwise). Platform
  scope is exactly: create Organization, list, read one Organization's
  members and invitations, promote to admin, issue/revoke admin
  invitations. No member invitations, no demote, no deactivate, no
  suspend, no tenant data (D-026 §4).
- Organization-admin handlers scope every statement by
  `auth.active_organization_id`; invitation ids and member ids from
  another Organization return 404 byte-identical to nonexistent.
- The last-active-admin invariant is enforced in the domain layer under
  a per-Organization keyed advisory lock (§4; concurrent mutual demotion
  or deactivation must not reach zero). A blocking advisory lock is
  acceptable here because the critical section is one count plus one
  UPDATE. A database-level constraint is not used: the rule has a
  platform-admin exception path and depends on two columns.
- Accept path: token ⇒ invitation ⇒ Organization and role; the client
  never supplies an Organization id, role, or email.
- The `platform_admin` table is INSERT-able only by `crm_migrator`. A
  compromised API process cannot create a platform admin.
- Passwords: 12–256 chars, Argon2id as today; display name 1–120 after
  trim; email normalized on login, issue, and accept.
- Email enumeration: issuing to an email with an account returns 201
  like any other (O-007 §3); the generic `invitation_not_acceptable`
  on accept is shown only to the token holder.

## 8. Observability

Command spans as `assign_person.rs`: `organization_id`, `actor_id`,
`correlation_id`, `outcome` via `CommandError::kind()`, plus
`invitation_id` / `target_user_id` where applicable, and
`origin = platform|cli` for those paths. Never log raw tokens,
`accept_path`, passwords, or invitation emails. The CLI prints ids and,
only with `--print-link`, the accept path; never passwords.

## 9. Failure behavior

- Database down: 503 on every route; CLI exits non-zero with a generic
  "could not connect" (no `Debug`-propagated connection string).
- Token outcomes: unknown/malformed/revoked → 404; expired → 410;
  accepted → 409 `invitation_used`. Safe to distinguish: the token is
  unguessable and the holder is the invitee.
- Duplicate issue: supersedes (old row `revoked / superseded`). Two
  concurrent issues for the same `(org, email)`: the partial unique
  index fails one; the loser re-runs the supersede-and-insert
  transaction once, and if that also fails returns 503. The contract is
  "issue always succeeds with a fresh token, or 503" — no new error code.
- Accept races: two accepts of one token → the second sees `accepted_at`
  under `FOR UPDATE` → 409 `invitation_used`. The same email accepting
  two different Organizations' invitations → the second hits
  `app_user_email_lower_idx` → 409 `invitation_not_acceptable`.
- If a `platform_admin` row is ever removed (no path exists in this
  slice — a `revoke-platform-admin` subcommand or `revoked_at` column
  arrives with the second platform admin): next `/api/platform/*` → 403;
  tenant routes unaffected if a membership exists; a platform-only
  session becomes fully invalid (401) because §3's predicate no longer
  matches.
- Demoted admin mid-session: next admin request → 403; `me` refetch
  hides `Manage`. Deactivated member: session rows revoked in the same
  transaction → next request 401 → `/login`.
- Organization name collision on the new unique index during migration:
  the migration fails loudly; dev has only the two seeded names.
- `CRM_INVITATION_TTL_HOURS` out of bounds (1–720) → refuse to start.
- Centrifugo down during deactivate: `warn`; the command still succeeds.

## 10. Frontend (Lane B, `web/**`; D-017; `UI_STYLE.md` binds)

- `MeResponse`: `organization` nullable with `role`; `platform_admin`.
  Router guard: an authenticated user with `organization == null` and
  `platform_admin == true` is routed to `/platform`; tenant routes
  redirect there. `/manage/*` requires `organization.role == 'admin'`,
  otherwise redirect to `/today`. `/platform/*` requires
  `platform_admin`, otherwise `/today`. A user who is both sees a
  `Platform` link in the sidebar footer.
- `AppShell`: third nav group **`Manage`** (admins only): Members. For a
  platform-only session the shell renders the `Platform` group only and
  does not open a realtime connection (no Organization).
- `/invite/:token` is `meta: { public: true, allowAuthenticated: true }`;
  the guard's public branch redirects a signed-in visitor only when
  `allowAuthenticated` is unset. `App.vue` keeps rendering public routes
  without `AppShell`; `AppShell`'s `me.organization.id` access becomes
  null-safe for the platform-only shape. The page fetches the preview
  (`POST /api/invitations/preview` with the token from `route.params`);
  states:
  loading, invalid (404), expired (410), used (409), valid → form
  (display name, password with a 12-char minimum hint, submit). Success
  → `me` set from the response → `/today`. A signed-in visitor is not
  redirected away (they may be accepting for a second account in a
  private window — the server decides).
- `/manage/members`: table of members (name, email, role, status,
  joined, People assigned); per-row actions Promote / Demote /
  Deactivate / Reactivate with a confirm dialog; `last_admin` → inline
  message "You are the last active admin. Promote someone else first."
  Below: Invitations (email, role, status, expires, invited by; actions
  Revoke; for pending: Re-issue). **Invite** button → dialog (email,
  role) → on 201, a one-time "copy link" panel showing the absolutized
  `accept_path`; closing it is final.
- `/platform`: Organizations table with the state badge; **New
  Organization** dialog. `/platform/organizations/:id`: the
  Organization's members (Promote action only, enabled for active
  members) and invitations (Invite admin / Revoke), same one-time link
  panel. A `needs_attention` row leads with a "Restore
  admin: promote a member or invite a new admin" hint (D-026 §3 — both
  actions always available).
- Tests (Vitest): router guards for the three session shapes
  (member, admin, platform-only); `me` with `organization: null`;
  invite page state machine over mocked responses.

## 11. Development environment, transport, and configuration

> **Superseded (2026-08-24):** `crm-admin seed-dev`, described below, was
> removed. `scripts/dev-bootstrap` now wipes and recreates the local
> database on every run and seeds Organizations/admins/members via
> `scripts/seed_dev.py`, which drives the same create/invite/accept
> sequence entirely over the live HTTP API rather than in-process domain
> calls. `crm-admin bootstrap-platform-admin` is unchanged — it remains
> the one step with no HTTP route. The rest of this section is kept for
> historical context.

- New config: `CRM_INVITATION_TTL_HOURS` (default 168, bounds 1–720).
  `CRM_DEV_SEED_PASSWORD` is kept as the password for every bootstrapped
  dev user including the platform admin.
- `crm-admin` binary (replaces `seed`), subcommands:
  `bootstrap-platform-admin --email --display-name` (migrator
  connection, the only subcommand that uses `MIGRATION_DATABASE_URL`);
  `seed-dev` (creates the platform admin via the above, then the two
  Organizations, four users, two admins, through `CreateOrganization`,
  `IssueInvitation` + `AcceptInvitation`, `ChangeMemberRole`; idempotent
  by checking existence through the read models, not by `ON CONFLICT`;
  on re-run existing users are left alone except that `SetLocalPassword`
  is applied from `CRM_DEV_SEED_PASSWORD`, so rotation still works as
  SLICE_001 §3 promised; the platform admin's password also comes from
  `CRM_DEV_SEED_PASSWORD`); `create-organization --name`; `invite
  --organization --email --role [--print-link]`; `set-password --email`.
  All but the first run as `crm_app` through the same domain functions
  as the API, which doubles as a grants check.
- CLI actor identity: every `crm_app` subcommand takes `--as <email>`;
  the CLI resolves it to an `app_user` that has a `platform_admin` row
  (SELECT is granted) and uses it as `actor_user_id` with `origin = Cli`
  and `actor_kind = 'user'`. Without `--as`, the CLI uses the sole
  `platform_admin` row and refuses to run if there are zero or several.
  `seed-dev` uses `owner@platform.test`. There is no `actor_kind =
  'system'` path for administration; nobody adds a second envelope path.
- `.env.example` gains `CRM_INVITATION_TTL_HOURS` (D-016 §10).
- `scripts/dev-seed` → `scripts/dev-bootstrap`. `check-db`'s
  `sqlx prepare --check` database is migrated only (no data); the
  per-test `#[sqlx::test]` databases are populated by the
  `tests/common/mod.rs` fixtures (`create_org`, `create_user`,
  `add_membership`), rewritten to call the domain functions as `crm_app`;
  `db_identity.rs`'s subprocess test runs `crm-admin
  bootstrap-platform-admin` then `seed-dev` against its own ephemeral
  database, passing both `MIGRATION_DATABASE_URL` and a `crm_app`
  `DATABASE_URL` for that database to the child. Fixture rule: tests may
  use the migrator connection only to backdate timestamps or delete rows
  for negative cases, never to create domain rows.
- Tunnel: no new routes (all under `/api/*` and the SPA).
- README Development section: bootstrap instructions, the platform-admin
  login, the `Manage` and `Platform` walkthrough.

## 12. Explicit exclusions

Membership removal (not a concept, D-027 §1); Organization suspend /
rename / delete (O-009); existing-user invitation acceptance and
Organization switching for multi-membership users; email / Slack / push
delivery of invitations or alerts (D-026 §5); password reset or change
UI (the CLI `set-password` is the dev path); ZITADEL / `external_identity`
(parked); platform-admin impersonation or any tenant-data access; login
or public-endpoint rate limiting (pre-existing gap, recorded); bulk
reassignment of an inactive member's People (O-004); stage
administration; Operator tools (Slice 005); any realtime event for
membership changes; email ownership verification — with local auth and
no delivery channel, whoever holds an accept link creates the account
for that address (inherent to D-016 §3 dev auth; resolved under ZITADEL),
so the existing-user-acceptance increment (§16) must be designed with a
verification step rather than inheriting this; platform-admin
revocation (no second platform admin exists yet).

## 13. Checks and tests

`./scripts/check` (service-free) and `./scripts/check-db` (DB + Centrifugo)
must be green; web `lint`, `typecheck`, `test`, `build` green.

Service-free: token generation/format/hash; email normalization and
syntactic validation; invitation state derivation; Organization state
derivation from `(admin_count, pending_admin_invitations)`; password and
display-name bounds; `ApiError` → status/code mapping for every new
variant; CLI argument parsing; platform router rejects `role: member`.

DB-backed (as `crm_app`, over HTTP where a route exists):

1. Migration applies on a fresh database; grants exactly as §2 (a test
   proves `crm_app` cannot INSERT into `platform_admin` and cannot UPDATE
   `invitation.token_hash`); the four fact tables reject UPDATE/DELETE/
   TRUNCATE for both roles.
2. Superseded (2026-08-24, §11): the platform-admin-driven create/invite/
   accept flow, run over HTTP, seeds nine ordered D-019 stages and two
   members per Organization; repeating a creation step is rejected
   (`organization_name_taken` / `already_member`), never duplicated —
   see `tests/db_identity.rs::platform_bootstrap_flow_rejects_repeat_creation`
   and `tests/db_people.rs::organization_creation_seeds_nine_ordered_stages_and_two_members`.
   `demo-leads` passes unchanged.
3. Platform admin with zero memberships logs in; `me` has `organization:
   null, platform_admin: true`; 401 on `/api/people`, `/api/today`,
   `/api/inquiries`, `/api/stages`, `/api/realtime/token`; lists and
   creates Organizations.
4. Member vs admin: every org-admin route 403 for Carol; every platform
   route 403 for Alice; a user who is both admin and platform admin
   reaches both.
5. Cross-Organization: Alice cannot list, revoke, or preview Best's
   invitations or change Bob's role/status by id (404 byte-identical to
   a random UUID); platform routes on a nonexistent Organization → 404.
6. Invitation lifecycle: issue → preview → accept (new user, session
   cookie set, `me.organization.role` correct, `Manage` data reachable) →
   second accept 409 → revoke → preview 404; expired (fixture-backdated
   `expires_at` via the migrator connection) → 410; re-issue marks the
   old row `superseded` and writes both facts; issue to an email with an
   account elsewhere → 201, accept → 409 `invitation_not_acceptable`, no
   user modified; issue to a current member of the same Organization →
   409 `already_member`; malformed token → 404; a request log captured
   during the lifecycle contains no token.
7. Last-admin: sole admin self-demote 409; sole admin self-deactivate
   409; two admins concurrently demoting each other (the
   `db_intake.rs` race style) leaves ≥1 active admin; demote then
   deactivate sequences; platform promote on an admin-less Organization
   succeeds; platform role route rejects `member` with 400.
8. Deactivation: sessions revoked (next request 401), login 403
   `no_membership`, `disconnect_user` called (Recording publisher
   captures it; the Centrifugo-backed test observes the socket close),
   People still attributed and `assigned_people_count` correct;
   reactivation restores login.
9. Organization state: the three states, including the flip from
   `pending_first_admin` to `needs_attention` when the only admin
   invitation expires, and back to `ok` on acceptance.
10. Facts: each command writes exactly the expected rows with the full
    envelope; invitation facts contain no email; platform actions carry
    `origin = 'platform'`.

Browser walkthrough (§1 steps 2–9) with zero console errors, through
the tunnel.

## 14. Safe defaults adopted (overridable at approval)

1. Platform admin = membership-free `app_user` + `platform_admin`
   allowlist; writable only by the migrator via the CLI.
2. `user_session.active_organization_id` nullable; `me.organization`
   nullable with `role`; `me.platform_admin`.
3. Invitations claimable by new identities only; generic 409 otherwise.
4. Supersede on re-issue; 7-day default TTL; SHA-256 token hash
   (independent of `CRM_SESSION_SECRET` rotation); 404/409/410
   distinctions on the public token endpoints.
5. Platform powers = create, list/read, invite (admin only), promote to
   admin. No member invitations, demote, deactivate, suspend.
6. Org-admin powers = invite, revoke, promote, demote, deactivate,
   reactivate. Admins may act on themselves subject to the invariant.
7. Four typed fact tables; the bootstrap platform-admin grant is in-row,
   not a fact.
8. No realtime events; `disconnect_user` best-effort.
9. Password 12–256; display name 1–120; email trim + lowercase with a
   syntactic check only.
10. Seeded dev identities: `owner@platform.test` (platform admin),
    Alice/Bob admins, Carol/Dave members, one `CRM_DEV_SEED_PASSWORD`.
11. `accept_path` returned as a path; the client absolutizes.
12. Members list visible to every member (unchanged); role/status
    visible to all; mutations admin-only.
13. Accepted, revoked, or expired invitations older than 30 days omitted
    from the list (history remains in facts).

## 15. Lane ownership and sequencing (SLICE_003 §15 model)

Two worktrees after approval; contracts frozen by §5 of this document;
any change goes through AGENTS.md §11.

- **Lane A — backend** (owns `backend/**`, `scripts/**`, `.env.example`,
  the README Development section, and **the migration**): (1) migration
  + grants + `.sqlx`; (2) `session::verify` / login / `me` changes and
  the three extractors — **first**, since Lane B's guards depend on the
  `me` shape; (3) `domain/admin` commands, read models, facts,
  `realtime::disconnect_user`, `Publisher::Disabled`; (4) org-admin,
  public, and platform routers; (5) `crm-admin` CLI, delete `seed.rs`,
  `dev-bootstrap`, rewrite `tests/common` fixtures; (6) tests per §13;
  (7) `check-db` and README.
- **Lane B — web** (owns `web/**`): (1) types + router guards +
  `AppShell` `Manage`/`Platform` groups from the frozen contracts, with
  Vitest tests; (2) `/invite/:token`; (3) `/manage/members`; (4)
  `/platform` and the Organization detail page. Integrates against Lane
  A's branch once A(2) and A(4) land; merges after Lane A.
- **Coordinator** owns `docs/**`, `PROJECT_STATE.md`, and the pointer
  lines in SLICE_001/002.

No file overlap. Effort: Lane A 4–5 focused days (the fixture rewrite
and the CLI are the hidden cost), Lane B 3, coordinator/review 1.

Risk concentrates in: the `session::verify` rewrite (every route's
security depends on it — the test in §13 item 3 and the existing
SLICE_001 session tests must both pass); the `tests/common` fixture
rewrite touching every DB test; the last-admin race; Centrifugo's
`disconnect` API shape on the pinned version.

## 16. Next

Slice 005 is Operator retrieval (D-021). Slice 006 is calling. The
first administration follow-ups, each a small increment when needed:
existing-user acceptance + Organization switching; O-009 suspension;
O-004 departure/bulk reassignment; invitation delivery once an email
channel exists.
