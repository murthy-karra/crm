# Task brief — Slice 004, Lane A (backend)

Parent specification: `docs/specs/SLICE_004.md` (APPROVED 2026-08-22).
Read, in order: `AGENTS.md`, `docs/decisions/DECISION_LOG.md` (D-021,
D-026, D-027 especially; O-007, O-009), the full SLICE_004 spec,
`docs/specs/SLICE_001.md` §2–§5 (sessions, identity, the schema you
alter), `docs/specs/SLICE_002.md` §2 and §4 (fact envelope, command
conventions), `docs/specs/SLICE_003.md` §6–§7 and §14a (realtime and the
hooks left for this slice), then the existing code under
`backend/crates/crm-api/` — in particular `auth/session.rs`,
`auth/context.rs`, `routes/session.rs`, `routes/organization.rs`,
`domain/envelope.rs`, `domain/facts.rs`, `domain/stage.rs`,
`domain/commands/receive_inquiry.rs` (the advisory-lock pattern) and
`assign_person.rs` (the command pattern), `realtime/publisher.rs`,
`bin/seed.rs`, `tests/common/mod.rs`, `tests/db_identity.rs`, and
`scripts/{check,check-db,dev-seed,demo-leads}`.

## Outcome

The backend half of SLICE_004 §1: the migration (roles, status,
`platform_admin`, `invitation`, four admin fact tables, grants), the
`session::verify` rewrite and three extractors, the `domain/admin`
commands and read models, the org-admin / public / platform routers,
`realtime::disconnect_user` and `Publisher::Disabled`, the `crm-admin`
CLI replacing `seed.rs`, the rewritten test fixtures, and every test in
§13.

## Ownership boundary

Owns: `backend/**`, `scripts/**`, `.env.example`, the README Development
section, and **the one migration**
(`backend/crates/crm-api/migrations/20260823000001_administration.sql`).
Does not touch `web/**` or `docs/**` (report needed doc changes to the
coordinator).

Branch: `slice-004-admin`, from `main`, in the main checkout. Lane B
works in a separate worktree and integrates against this branch once
steps 2 and 4 below land — push the branch after each green step.

## Frozen contracts

SLICE_004 §2 (DDL, facts, grants — verbatim), §3 (session predicate and
extractor semantics), §4 (command behavior, lock, error outcomes), §5
(every route, body, status, and error code, including the declared
changes to SLICE_001/002), §6 (no events; disconnect best-effort), §11
(config variable and CLI subcommands/flags). Any deviation stops work
and is reported per `AGENTS.md` §11 — do not adjust a shape to make an
implementation simpler.

## Sequence (spec §15; each step green before the next)

1. Migration exactly as §2 (including `DROP DEFAULT` after backfill,
   the partial unique index, the four fact tables with the
   `reject_mutation` triggers, and the column-level grants) → `.sqlx/`
   regenerated via `scripts/sqlx-prepare`. Config:
   `CRM_INVITATION_TTL_HOURS` (default 168, bounds 1–720; refuse to
   start otherwise). `Origin::{Platform, Cli}`.
2. `session::verify` as one statement with the §3 `WHERE` predicate
   (`(s.active_organization_id IS NULL AND pa.user_id IS NOT NULL) OR
   m.status = 'active'`), returning `organization: Option<{id, name,
   role}>` and `platform_admin`; Rust re-asserts the invariant. Login
   per §3 (earliest **active** membership; platform-admin-only →
   NULL-Organization session; else 403 `no_membership`). `/api/me` and
   the login body per §5 declared change 2. Extractors: `AuthContext`
   (+ `role`), `OrgAdminContext`, `PlatformAuthContext`. CORS gains
   `PUT`. **Push after this step — Lane B's guards depend on it.**
3. `domain/admin/*`: `CreateOrganization` (+ `stage::seed_defaults`
   moved to the app path; update its doc comment), `IssueInvitation`
   (normalize, supersede-in-transaction, `AlreadyMember` only for the
   target Organization, single retry on the partial-unique race),
   `RevokeInvitation`, `AcceptInvitation` (validate → hash → transaction
   → re-check under `FOR UPDATE`; `origin` from the caller),
   `ChangeMemberRole` and `SetMemberStatus` under
   `pg_advisory_xact_lock(hashtextextended('admin:' || org, 0))` with the
   last-active-admin check; `SetLocalPassword`; `GrantPlatformAdmin`
   (migrator connection only). Facts via `facts.rs` helpers, PII-free.
   Read models: `list_for_platform` (one statement), `members` (+ `role`,
   `status`, `joined_at`, `assigned_people_count`), `invitation::list`
   (no token; 30-day cutoff for non-pending), `invitation::preview`.
   `realtime::disconnect_user` (Centrifugo `disconnect` API; `warn` on
   failure; `Recording` captures it) and `Publisher::Disabled`.
4. Routers: org-admin (`/api/organization/members/{id}/role|status`,
   `/api/organization/invitations` GET/POST/DELETE), public
   (`POST /api/invitations/preview`, `POST /api/invitations/accept` —
   token in the body, never the path; fresh session, CSRF properties as
   login), platform (`/api/platform/organizations…`; `role: "admin"`
   only on both the role route and the invitation route; Organization id
   from the path only). New `ApiError` variants exactly as §5. **Push
   after this step.**
5. `crm-admin` binary (`bootstrap-platform-admin`, `seed-dev`,
   `create-organization`, `invite [--print-link]`, `set-password`;
   `--as <email>` actor resolution per §11; only the first subcommand
   uses `MIGRATION_DATABASE_URL`); delete `bin/seed.rs`; `scripts/dev-seed`
   → `scripts/dev-bootstrap`; rewrite `tests/common/mod.rs` fixtures
   onto the domain functions as `crm_app` (migrator connection only to
   backdate timestamps or delete rows for negative cases); point
   `db_identity.rs`'s subprocess test at `crm-admin` with both URLs.
6. Tests per §13 (service-free in `./scripts/check`; DB-backed and the
   Centrifugo-backed disconnect observation in `./scripts/check-db`),
   including: the grant negatives (`crm_app` cannot INSERT
   `platform_admin`, cannot UPDATE `invitation.token_hash`), the
   platform-only session getting 401 on every tenant route and on
   `/api/realtime/token`, the cross-Organization 404 byte-identity
   checks, the two-admins mutual-demotion race, the double-accept race,
   the Organization-state flip on expiry, and a request-log capture that
   contains no token.
7. `scripts/check-db` updates, `.env.example`
   (`CRM_INVITATION_TTL_HOURS`), README Development section (bootstrap,
   the platform-admin login, the `Manage`/`Platform` walkthrough).

## Required checks before reporting done

`./scripts/check` (fmt, clippy `-D warnings`, all service-free tests)
with no services running; `./scripts/check-db` with `dev-services up`
(sqlx `prepare --check`, DB-backed, Centrifugo-backed); a live
`./scripts/dev-bootstrap` run three times followed by
`./scripts/demo-leads`. Never report a check as passed unless it ran.

## Stop and report (do not work around)

- Any change needed to a §2/§3/§4/§5/§11 shape, or to any SLICE_001/002
  contract beyond the declared changes.
- A grant that turns out to be insufficient for a specified statement
  (report it; do not widen grants silently).
- Any need to write to the database outside a migration or the
  application path (D-021) — including in tests beyond the fixture
  rule in §11.
- Centrifugo's `disconnect` API on the pinned version behaving
  differently from §6.
- Any existing SLICE_001–003 test that must change for a reason other
  than the declared contract changes.

## Report format

Files changed (cross-checked against `git status`, not from memory);
behavior delivered per §1 step; commands run with results; contract
changes (should be none beyond §5's declared list); unresolved risks and
assumptions.
