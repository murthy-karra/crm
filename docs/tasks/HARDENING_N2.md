# Hardening chunk N2 — `UserId` (+ N1 carry-overs)

Parent plan: `docs/design/type-safety-hardening.md` (chunk 3; the
"sqlx strategy" section is binding). Branch: `hardening-n2`. Single
lane, one writer. Read `AGENTS.md` first. Predecessors S1
(`docs/tasks/HARDENING_S1.md`) and N1 (`docs/tasks/HARDENING_N1.md`,
both merged) established every pattern this chunk reuses — read N1's
brief; its invariants and mechanics apply verbatim unless restated.

## Goal

1. `UserId` joins `OrganizationId` in `crm-app/src/ids.rs` — the
   user/org adjacency is the most frequent swap surface (~22 bare
   `user_id` params, plus `actor_user_id` / `assigned_user_id` /
   `on_behalf_of_user_id` fields).
2. Close N1's two recorded carry-overs (ladder doc, N2 row):
   a. The session layer below `AuthContext` — `session::create(...,
      user_id, active_organization_id, ...)` and
      `SessionOrganization.id` — today the login-path user/org swap
      still compiles (mitigated only by the membership join → 401).
   b. Type `OrganizationRef.id` and `PlatformOrganizationItem.id` as
      `OrganizationId`, removing the typed→bare→typed chains N1 left.

## Hard invariants (same as N1)

No SQL string changes (`.0` binds; `.sqlx` byte-untouched — include
the `git status --short backend/.sqlx/` proof). No wire changes
(serde-transparent; existing exact-shape and route contract pins must
pass unmodified). No migrations. crm-operator untouched (D-028 fence;
convert at crm-api's backend/route seam exactly as N1 did). Only
`UserId` + the two named carry-overs — nothing else. No implicit
`From`/`Into` conversions.

## The type

`UserId` in `ids.rs`, exactly `OrganizationId`'s shape: repr/serde
transparent, `new`/`as_uuid`, Display-delegating Debug, doc comment,
one ```compile_fail,E0308 doctest showing the user/org transposition
failing (both directions now typed — the doctest should demonstrate
`f(org, user)` rejecting `f(user, org)`). Unit tests mirroring
OrganizationId's (Display/Debug/serde-transparency/new-as_uuid).

## The sweep (compile-guided, same passes as N1)

Core fields first, then follow rustc outward:

- `FactEnvelope.actor_user_id: Option<UserId>` and
  `on_behalf_of_user_id: Option<UserId>` (+ `for_system`);
  `IntakeActor::System { on_behalf_of_user_id }`;
  `CommandContext`'s user field; `AuthContext.user_id`.
- `assigned_user_id` across person/routing/today/realtime data
  structs; `intake_default_assignee_user_id` (admin settings, D-035
  routing); `determine_routing`'s two adjacent `Option<Uuid>` user
  params (`assign_to_user_id`, current assignee) — after this chunk
  that swap must not compile.
- `realtime::token::mint`'s user param (org side already typed — the
  full signature is now swap-proof).
- Admin: membership queries (`member_view(org, user)` etc.),
  invitation accepted_user/inviter fields, set_member_status /
  change_member_role targets.
- Session layer (carry-over a): `session::create` params typed
  (`UserId`, `Option<OrganizationId>`), `SessionIdentityRow` may stay
  a bare private row per the contract, but `SessionOrganization.id`
  and the public session API get typed. The 4 runtime (non-macro)
  `.bind()` sites in session.rs: bind `.0` — do NOT add
  `#[sqlx(transparent)]`/`sqlx::Type` derives; `.0` keeps one
  consistent mechanism everywhere.
- Org DTOs (carry-over b): `OrganizationRef.id`,
  `PlatformOrganizationItem.id` → `OrganizationId`; delete the
  now-redundant re-wrap sites N1 added (platform.rs, crm-admin.rs,
  create_organization outcome path).

## Watch-outs

- HTTP DTOs that serialize `assigned_user_id`/user ids to the web
  (people routes, today, session/me responses): serde transparency
  keeps JSON identical — verify against existing route pins, change
  no response shape.
- The operator seam: `on_behalf_of` / user ids crossing into
  crm-operator stay bare `Uuid` there; wrap/unwrap at the same seam
  files N1 used (routes/operator.rs, operator/backend.rs).
- Web (Vue) is out of scope entirely — transparent serde means no
  TS changes.
- If some third id type (person, call) is required mid-sweep to make
  something compile, STOP that edit — leave that param bare `Uuid`;
  it belongs to N3/N4.

## Checks before reporting done

`cargo fmt`, then `source ~/.nvm/nvm.sh && ./scripts/check` and
`./scripts/check-db`; `git status --short backend/.sqlx/` must be
empty. Same report format as N1 (files changed matching `git status`
exactly, core structs typed, conversion sites, checks + results,
assumptions, surprises).
