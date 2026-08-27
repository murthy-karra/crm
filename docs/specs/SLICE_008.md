# Slice 008 — Intake routing modes (round-robin)

Status: APPROVED (user, 2026-08-26). Independently reviewed
(ready-with-fixes; all applied: S1 non-default-mode assignee rule,
S2 deactivated warning kept, S3 declared PUT supersession of 007c
§5, S4 reactivation-resumes-slot stated, I1 MembershipKey composite,
I2 anchor-from-membership-row). D-041 accepted
— three-mode picker, all-active-members pool, continue-anchored
fairness. Decisions in force: D-035 (base
unattended routing), D-041 (this slice's charter), D-027 (members
deactivate, never delete). Sources: the ladder's round-robin note
(docs/plans/SLICE_007_LADDER.md) and cross-rung row 8–13's "LATER".

Targets the post-type-safety-ladder codebase (`main` ≥ `069f55a`):
`RoutingAssignees`, `UserId`/`OrganizationId`, the `Actor` envelope.

## 1. User-visible outcome

An org admin opens Intake Settings and picks how unattended leads
route: **Default assignee** (today's behavior — one picked member),
**Round-robin** (leads rotate fairly across all active members), or
**Unassigned** (leads land unassigned, with the warning banner).
Rotation is fair and predictable: join order, skip deactivated
members, newcomers join the end of the cycle, and an explicitly
assigned lead never steals anyone's turn. The Person history reads
"Routed to X (round-robin)".

## 2. In / out of scope

**In:** the `intake_routing_mode` column + three-mode CHECK; the
`intake_rotation` pointer table; `RoutingStrategy::RoundRobin` (+
fact CHECK widening, declared); `determine_routing` step-4 dispatch
by mode; the settings GET/PUT extension; the settings card mode
picker; history label; migration with the D-041 deterministic
mapping; fairness/concurrency/isolation tests; live walkthrough.

**Out (explicit):** rules-based routing (the mode CHECK widens
later); per-member opt-outs; least-loaded balancing; SMS/other
channels; any change to matrix steps 1–3 (`kept_existing` /
`explicit` / `actor_default`); rotation-state exposure in the API
("next up" — additive later); auditing mode changes as facts
(settings CRUD posture, 007c (e)/(m) precedent).

## 3. Persistence (one migration, single owner)

`20260903000001_intake_routing_mode.sql`:

1. `ALTER TABLE organization ADD COLUMN intake_routing_mode TEXT
   NOT NULL DEFAULT 'unassigned'` + named constraint
   `organization_intake_routing_mode_check IN
   ('default_assignee','round_robin','unassigned')` (named now so the
   future `rules` widening is a clean DROP/ADD).
2. Backfill (D-041 mapping): `UPDATE organization SET
   intake_routing_mode = 'default_assignee' WHERE
   intake_default_assignee_user_id IS NOT NULL`. (NULL-assignee and
   new orgs stay `unassigned` via the DEFAULT — mirrors today's
   initial state and its warning.)
3. `GRANT UPDATE (intake_routing_mode) ON organization TO crm_app`.
4. `CREATE TABLE intake_rotation (organization_id UUID PRIMARY KEY
   REFERENCES organization(id), last_assigned_user_id UUID NOT NULL
   REFERENCES app_user(id), updated_at TIMESTAMPTZ NOT NULL DEFAULT
   now())` + `GRANT SELECT, INSERT, UPDATE` to crm_app. Mutable
   operational state — no append-only triggers. Plain FKs (007c §3
   precedent).
5. `routing_decision_strategy_check` DROP/re-ADD with `round_robin`
   appended — the declared persistence-contract change (pointer
   amendments into SLICE_002 §5 and SLICE_007c §5 ride with this
   spec).

The pointer lives OFF the organization row deliberately: bumping it
per lead must not rewrite `organization.updated_at`, contend with
the admin settings PUT's row lock, or widen the organization UPDATE
grant beyond the one mode column.

## 4. Domain

New `domain/intake/rotation.rs`:

- `IntakeRoutingMode { DefaultAssignee, RoundRobin, Unassigned }` —
  `as_str()`/`parse()` house style, round-trip + fail-closed pins.
- Pure `next_in_rotation(ordered: &[(MembershipKey, UserId)], anchor:
  Option<MembershipKey>) -> Option<UserId>` — first active member
  strictly after the anchor's key, wrapping; `None` anchor or
  missing anchor row → first member; empty slice → `None`.
  `MembershipKey` = the composite `(created_at, user_id)` compared
  lexicographically — total even for same-batch joins whose
  `created_at` ties (reviewer I1; matches the SQL
  `ORDER BY m.created_at, u.id`). A REACTIVATED member resumes their
  ORIGINAL join-order slot (their membership row and key are
  unchanged — D-027; reviewer S4, stated so it is a decision, not an
  accident). Unit-tested exhaustively (wrap, skip, empty, missing
  anchor, newcomer-at-end, created_at tie, reactivation-resumes-slot).
- `take_next(tx, organization_id) -> Option<UserId>` — loads the
  active-member order (`organization_membership.created_at, user_id`
  — identical to the members list), reads the pointer row, computes
  next, upserts the pointer, returns the assignee. Documented
  precondition: only callable inside the Phase-B transaction holding
  the `intake:` advisory lock — that lock makes read+bump
  single-writer per org, and an `IntakeBusy` rollback undoes the
  bump atomically. No additional row locking (the READ COMMITTED
  membership window is the same accepted, self-healing D-027 window
  007c documents — do NOT chase it).

`determine_routing` (matrix step 4 only — steps 1–3 byte-identical):
System actor, no explicit assignee, no current assignee →
dispatch on the org's mode: `DefaultAssignee` → existing D-035 path
(active default → `organization_default`; stale/NULL → `unassigned`
+ the existing warning semantics); `RoundRobin` → `take_next` →
`RoundRobin` strategy with the member, or `unassigned` outcome on an
empty pool (config is CRUD; the fact records what happened);
`Unassigned` → `unassigned`. The pointer advances ONLY on an actual
round-robin assignment — never on kept_existing/explicit/
actor_default/unassigned/duplicate short-circuits (all of which
return before `take_next`).

`RoutingStrategy` gains `RoundRobin` ⇄ `"round_robin"` (`as_str` +
`from_str`; without the from_str arm a duplicate replay of a
round-robin-routed row would fail closed as Corrupt — same
mechanism 007c §5 handled, pinned by criterion 11).

All five entry paths (email receive, `POST /api/inquiries`,
workbench retry, extraction completion, `crm-admin
receive-inquiry`) funnel through `complete_intake` →
`determine_routing` — one change point, no per-path work.

## 5. HTTP & web

`GET /api/organization/intake-settings` (additive):
`{"intake_routing_mode": "...", "intake_default_assignee_user_id":
uuid|null}`.

`PUT` — full-state replace, BOTH keys required (absent key → 400,
extending 007c's posture; a routing mode must never flip by
omission). Validation: unknown mode → 400 `malformed_request`;
`default_assignee` mode requires a non-null assignee who is an
active member → 422 `invalid_assignee` (the dropdown no longer has
an Unassigned entry — D-041); in `round_robin`/`unassigned` modes the
assignee value must be null, an active member, or the
CURRENTLY-STORED value (stale echo allowed — so an org whose
default deactivated can still flip modes); anything else → 422
(reviewer S1: no cross-org/garbage uuid can be persisted, and no
migrated org is ever bricked). Clearing the assignee is expressed by
switching mode with `null`; clearing while REMAINING in
`default_assignee` mode is impossible by design (422). Retention
across flips otherwise per 007c §12(c). Admin-only
(`OrgAdminContext`), org from session only.

Declared breaking supersession (AGENTS §11, reviewer S3): this PUT
body supersedes SLICE_007c §5's single-key shape (old body → 400).
Sole client is the in-repo web app, shipped atomically with the API;
a pointer amendment lands in SLICE_007c §5. 007c's intake-settings
PUT test pins are amended by this slice; every other 007c pin
(facts, routing, Today) stays byte-identical as the regression gate.
(Reverse skew — new web against an old API — renders an undefined
picker briefly; harmless under atomic dev deploys, noted only.)

Web (`IntakeSettingsView.vue` + `types.ts` + queries): three-option
mode control; the assignee dropdown (active members only, no
Unassigned entry) renders only in `default_assignee` mode; the
existing "leads will be unassigned" warning becomes the `unassigned`
mode's banner; round-robin mode shows a one-line description of the
rotation ("rotates across all active members in join order").
`PersonDetailView.vue` history label: `round_robin` → "round-robin".
One mutation PUTs both fields on any change.

## 6. Authorization, isolation, observability, failure

- Both endpoints admin-only; member 403 pins extended to the new
  field. Tenant isolation: rotation into org A writes zero org-B
  rows INCLUDING `intake_rotation`; the pool query filters by
  `organization_id` (a user active in two orgs rotates independently
  in each — pinned); org-B admin GET/PUT blind to org A.
- Observability: settings PUT span gains `routing_mode` (static
  vocabulary); intake spans unchanged (strategy lives in the fact).
- Failure: empty pool → `unassigned`, never an error, no pointer
  write; `IntakeBusy` rollback leaves the pointer untouched
  (pinned); duplicates short-circuit before the lock and never
  advance the pointer; DB-down 503s unchanged.

## 7. Acceptance criteria

The planner's 14 criteria adopted verbatim, adjusted for D-041's
three-mode model — highlights: migration mapping on a populated DB
(assignee-set org → `default_assignee`, NULL org → `unassigned`,
new org DEFAULT `unassigned`); fairness a,b,c,a,b,c across 6 system
intakes with full fact/Today assertions; first-rotation start;
mid-rotation deactivation continue-not-reset (both non-pointer and
pointer member); newcomer joins at end; empty pool → `unassigned`;
kept_existing/explicit/actor_default never advance the pointer;
duplicate replay decodes `round_robin` (no 500); ~8-way concurrency
under the advisory budget with {3,3,2} distribution; PUT/GET matrix
incl. the 422 on default-mode-without-assignee and byte-identical
007c pins as the regression gate; web Vitest for the picker states AND the
deactivated-default warning state; created_at-tie and
reactivation-resumes-slot unit cases; non-default-mode PUT with
stale-echo (accepted) / cross-org / garbage assignee (422);
the §6 isolation pins carried explicitly (dual-org user rotates
independently; zero org-B `intake_rotation` rows);
live walkthrough: set round-robin in the UI, 4 real leads through
leads.elysianfeld.com + crm-admin, Today distribution 2/1/1,
deactivate a member and watch the skip, history label check.

## 8. Lane and checks

Single lane, one writer, branch `slice-008-round-robin`, sole
migration owner. Gates: `./scripts/check` + `./scripts/check-db`
(run `./scripts/db-migrate` after checkout); independent review +
adversarial testing; live walkthrough BEFORE the commit gate; Phase
9 approvals as always.

## 9. Safe defaults adopted (reviewer/user may veto)

(a) separate `intake_rotation` table (not an org column); (b)
anchor-by-membership-key, continue-never-reset (D-041); (c) advance
only on actual assignment; (d) assignee + pointer retained across
mode flips; (e) empty pool → `unassigned` fail-safe; (f) PUT
full-state both-keys-required, unknown mode 400, default-mode
assignee 422; (g) mode governs matrix step 4 only; (h) no rotation
state in GET; (i) mode changes are unaudited settings CRUD; (j) the
READ COMMITTED membership window stays accepted (no row locking);
(k) explicit assign_to never consumes a turn (strict rotation, not
least-loaded — noted as the inherent wrinkle); (l) new-org DEFAULT
`unassigned` (mirrors today's initial state).
