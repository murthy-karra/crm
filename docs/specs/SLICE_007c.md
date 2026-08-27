# Slice 007c — System actor + unattended routing

Status: APPROVED (user, 2026-08-24; planner pass + independent review
same day, reviewer verdict "ready with amendments" — no blocking
findings, all six amendments A1–A6 applied; §5 contracts and §12 safe
defaults accepted as written. Round-robin confirmed as its own
post-007d rung, not this one — see the ladder note.)
Ladder: docs/plans/SLICE_007_LADDER.md rung c. Builds on 007b (inbound
endpoint; merged `4b3462a`). Targets D-035 (unattended intake routes to
an admin-set Organization default assignee; unset → unassigned +
warning), ladder cross-rung decision 5 (system actor + new routing
strategies — SAFE_DEFAULT, declared contract change), D-021 (creation
paths), D-022 (contact attempts / Today), D-027 (membership
deactivation), SLICE_002 (intake Phase A/B, routing, facts), AGENTS.md
§4.6 (routing configuration is CRUD; routing decisions are facts), §11
(contract discipline).

## 1. User-visible outcome

An Organization admin opens **Manage → Intake** and, below the 007a
address card, finds a new **"Unattended lead routing"** card: a
dropdown of the Organization's **active** members ("Default assignee
for unattended leads") plus an explicit "Unassigned" choice. When no
default is set — or the configured default has since been deactivated —
the card shows a warning: unattended leads will be created unassigned
and appear on no one's Today.

Underneath, intake can now run with **no human actor**: a system-actor
path through the same `receive_inquiry` command, routing to the
Organization default. No email is parsed anywhere in this rung; the
system path is proven with today's `generic_v1` JSON.

Live walkthrough: (1) as an org admin, set the default assignee to bob;
(2) run `crm-admin receive-inquiry --organization <id> --source website
--payload-file lead.json` with a `generic_v1` JSON lead; (3) as bob,
open Today → the Person is there; open the Person → history shows
"Inquiry received" and the routing line attributed to **System**, not a
user; (4) clear the setting, deliver a second lead → the Person exists
in People, unassigned, on no one's Today, and the settings page shows
the warning; (5) try to set the default to a deactivated member → 422;
deactivate the member *after* setting them → the next lead routes
unassigned, nothing errors.

## 2. In / out of scope

In: one migration (org column + `routing_decision.strategy` CHECK
widening + one column-level grant), `IntakeActor` /
`FactEnvelope::for_system`, the `receive_inquiry` actor refactor, the
`organization_default` / `unassigned` routing strategies (backend enum
+ web union/labels, declared additive contract change), `GET`/`PUT
/api/organization/intake-settings`, the settings-page card, the
`crm-admin receive-inquiry` dev trigger for the walkthrough.

Out: any email/MIME parsing (007d); any change to `/inbound/email` or
`receive_inbound_email` (007b, frozen — it stays Phase-A-only and does
NOT call the system path in this rung); Person/Inquiry creation from
email (007d); Unresolved detail/retry/discard (007e); LLM extraction
(007f); round-robin or rules routing (explicitly outside the ladder);
token rotation (007g); changing `is_organization_member` semantics for
*manual* assignment (it currently permits inactive members on the
explicit-assign path — a pre-existing D-027/O-004 gap, flagged, not
fixed here); auditing the settings change itself as a typed fact
(AGENTS.md §4.6: routing configuration is CRUD); any new fact table;
any new backend dependency.

## 3. Persistence

One migration, `20260829000001_intake_settings.sql`, owned by this
lane:

```sql
ALTER TABLE organization
    ADD COLUMN intake_default_assignee_user_id UUID NULL
        REFERENCES app_user (id);
GRANT UPDATE (intake_default_assignee_user_id, updated_at)
    ON organization TO crm_app;

ALTER TABLE routing_decision
    DROP CONSTRAINT routing_decision_strategy_check;
ALTER TABLE routing_decision ADD CONSTRAINT routing_decision_strategy_check
    CHECK (strategy IN ('explicit', 'actor_default', 'kept_existing',
                        'organization_default', 'unassigned'));
```

- The current CHECK is the inline column constraint in
  `20260821000004_inquiry_and_facts.sql` (line 78): `strategy TEXT NOT
  NULL CHECK (strategy IN ('explicit', 'actor_default',
  'kept_existing'))`. Its auto-generated name should be
  `routing_decision_strategy_check`; the implementer must verify with
  `\d routing_decision` against a migrated database before relying on
  the DROP. DDL by `crm_migrator` is unaffected by the append-only
  `reject_mutation` triggers.
- `crm_app` today holds only SELECT + INSERT on `organization`; the
  column-level UPDATE grant above is the **entire** new write surface.
  `updated_at` is included (the `20260823000001` membership-grant
  precedent) so the settings PUT can maintain it — the PUT sets
  `updated_at = now()` alongside the value. `intake_slug` /
  `intake_token` stay un-updatable (007a tests already pin that;
  criterion 1 re-pins it against the new grant).
- **FK choice**: plain FK to `app_user`, deliberately *not* a composite
  FK to `organization_membership` — the same reasoned choice SLICE_002
  §6 made for `person.assigned_user_id` (a composite FK would block
  membership operations and pre-decide O-004). "Active member of this
  Organization" is a domain-layer validation at PUT time and re-checked
  at routing time.
- **Deactivated default assignee (D-027 interaction)**: the stored
  value is kept. At intake time the domain re-checks active membership
  inside the Phase-B transaction; inactive → route as `unassigned`
  (fail-safe: never assign new leads to someone whose Today is
  invisible), and the settings page shows the deactivated-warning
  state. `set_member_status` does NOT clear the column as a side effect
  (that would lose admin intent across a reactivation and couple two
  commands). The re-check is best-effort under READ COMMITTED: a
  deactivation committing between Phase B's membership read and the
  intake commit can still assign that one in-flight lead to the
  just-deactivated member — benign and self-healing (D-027 §3:
  inactive members stay visible with counts; per-Person reassignment
  exists). Do NOT add row locking (`FOR SHARE` etc.) to chase this
  window.
- No backfill: NULL is the correct initial state for every existing
  Organization.
- The fact tables need **no** migration: `actor_kind` already allows
  `'system'` with `CHECK ((actor_kind = 'user') = (actor_user_id IS NOT
  NULL))` (migration `20260821000004`), so "no actor" is
  `actor_kind='system', actor_user_id NULL` — unrepresentable any other
  way.

## 4. Domain

**`FactEnvelope::for_system`** (`crm-app/src/domain/envelope.rs`,
beside `for_command`):

```rust
pub fn for_system(
    organization_id: Uuid,
    origin: Origin,
    occurred_at: DateTime<Utc>,
    correlation_id: Uuid,
) -> Self
// actor_kind: System, actor_user_id: None,
// on_behalf_of_user_id: None, causation_id: None
```

`Origin` is a **parameter** (the ladder's "origin `webhook`" describes
007d's eventual caller, not a constraint): the CLI walkthrough passes
`Origin::Cli`; 007d will pass `Origin::Webhook`.

**`IntakeActor`** (`domain/intake/mod.rs`, beside 007a/b code):

```rust
pub enum IntakeActor {
    User(CommandContext),
    System { organization_id: Uuid, origin: Origin, correlation_id: Uuid },
}
impl IntakeActor {
    pub fn organization_id(&self) -> Uuid;
    pub fn correlation_id(&self) -> Uuid;
    pub fn envelope(&self, occurred_at: DateTime<Utc>) -> FactEnvelope;
}
```

**`receive_inquiry`** (`domain/commands/receive_inquiry.rs`): the
signature changes from `ctx: &CommandContext` to `actor: &IntakeActor`.
Only callers: `routes/intake.rs` (wraps `CommandContext::from_auth` in
`IntakeActor::User` — no behavior change) and tests. Routing matrix,
replacing today's `determine_routing`:

1. matched Person already assigned → `kept_existing` (unchanged, both
   actor kinds);
2. `assign_to_user_id` present → `explicit` (unchanged, existing
   `is_organization_member` validation — honored for either actor kind
   rather than special-cased);
3. else, User actor → `actor_default` (unchanged);
4. else, System actor → look up `intake_default_assignee_user_id`
   **joined with its membership status** inside the Phase-B
   transaction; set and active → `organization_default` with that
   assignee; unset or inactive → `unassigned` with `assignee_user_id
   NULL`.

`unassigned` mechanics: a `routing_decision` fact with NULL assignee
(the column is already nullable; the web already renders "Routed …
left unassigned"); **no** `assignment_changed` fact — the existing
"emit assignment on `current_assignee.is_none()`" block gains the
additional condition that the routing assignee is `Some` (a NULL→NULL
assignment fact is noise). A matched-but-unassigned Person + system
actor + default set produces one `assignment_changed` NULL→default with
`causation_id` = the routing decision's id — exactly today's shape.
All facts from one system intake share `actor_kind='system'`,
`actor_user_id NULL`, the supplied origin, and one correlation id.

The `receive_inquiry` tracing span's unconditional `actor_id` field
becomes conditional (recorded only for User actors); a new
`actor_kind` field is always recorded (§8).

**What triggers the system path in this rung**: nothing in the HTTP
surface. `POST /api/inquiries` stays session-actor (a session exists;
pretending otherwise would be false); `/inbound/email` stays
Phase-A-only. The callers are (a) DB-backed tests invoking
`receive_inquiry(IntakeActor::System { .. })` directly — the primary
proof — and (b) a new `crm-admin receive-inquiry` subcommand for the
live walkthrough: crm-admin already runs as `crm_app` through the same
domain functions (D-021-sanctioned; the `invite` precedent), loads
`.env` for `CRM_RAW_PAYLOAD_KEY`, and uses `Publisher::Disabled` — the
walkthrough refreshes Today rather than relying on realtime, which
SLICE_003's refetch-recovery convention blesses. 007d replaces the CLI
as the real caller.

CLI posture: the subcommand skips `resolve_actor` entirely (no user
actor is the point — it constructs `IntakeActor::System` with
`Origin::Cli`); it validates that `--organization` exists before Phase
A (a clean error, not a raw FK failure); it exposes **no**
`--assign-to` flag (the walkthrough doesn't need one; safe default (h)
covers the domain behavior). **Payload normalization**: the CLI parses
`--payload-file` as JSON and re-serializes via `serde_json::to_vec`
exactly as `routes/intake.rs` does before sealing — `content_hmac` is
computed over the normalized bytes (compact, serde key order), so
dedup between the CLI and `POST /api/inquiries` works and criterion 9
is actually exercised; sealing raw file bytes would make them never
match.

## 5. HTTP contract (frozen at approval; AGENTS.md §11)

Two new admin endpoints (extractor: the existing `OrgAdminContext`
pattern in `routes/organization.rs`; Organization solely from the
session):

`GET /api/organization/intake-settings` → 200
`{"intake_default_assignee_user_id": <uuid>|null}`. Errors: 401
`unauthenticated`, 403 `forbidden` (member, not admin), 503
`unavailable`. The page joins against the members list (any-member
`GET /api/organization/members` already returns `status`) for display
names and the deactivated warning — no member data duplicated into this
contract.

`PUT /api/organization/intake-settings` — body
`{"intake_default_assignee_user_id": <uuid>|null}`; 200 with the GET
shape (the stored result); 400 `malformed_request`; 422
`invalid_assignee` — **byte-identical** for a nonexistent user, another
Organization's member, and an inactive member (no existence leak);
401/403/503 as above. Only an explicit `null` clears the setting; a
body with the key **absent** is 400 `malformed_request` (no
`#[serde(default)]` — an omitted field must never silently clear a
routing default). Malformed UUID string → 400 per the existing
JsonRejection convention.

**Declared additive change to the frozen `POST /api/inquiries`
contract** (SLICE_002 §5): the `routing_strategy` response vocabulary
grows by `organization_default` and `unassigned`. These appear on the
user-actor endpoint **only** in `duplicate: true` replays of a
system-routed row (the duplicate path re-reads the stored strategy;
today's `RoutingStrategy::from_str` fails closed to a 500, so the Rust
enum must gain both variants). A pointer line is added to SLICE_002 §5,
007a-style. `web/src/api/types.ts`'s `RoutingStrategy` union and
`PersonDetailView.vue`'s `ROUTING_STRATEGY_LABEL` grow with it.

No other existing endpoint changes shape or behavior.

**Superseded by SLICE_008 §5 (declared breaking change, AGENTS.md §11;
D-041).** The `GET`/`PUT` bodies above — single key,
`intake_default_assignee_user_id` only — are SLICE_008's three-mode
routing picker's starting point, not its current shape. SLICE_008
replaces the PUT body with a two-key, both-required shape
(`intake_routing_mode` + `intake_default_assignee_user_id`); a PUT using
this section's single-key body now gets 400 `malformed_request` (no
mode key). The GET response gains `intake_routing_mode` alongside the
unchanged `intake_default_assignee_user_id`. This section is kept as the
historical record of what this slice shipped; SLICE_008 §5 is the
current contract for both endpoints.

## 6. Web

`IntakeSettingsView.vue` (admin-only route already exists) gains the
"Unattended lead routing" card: a PrimeVue `Select` over the members
query filtered to `status === 'active'`, plus an explicit "Unassigned"
option; saved via a `useUpdateIntakeSettings` mutation invalidating a
new `queryKeys.intakeSettings(orgId)`. Warning states: value null →
"unattended leads will be created unassigned and appear on no one's
Today"; value set but that member is inactive → "the default assignee
is deactivated; unattended leads will be created unassigned".
`types.ts`: `IntakeSettingsResponse` + the two `RoutingStrategy`
additions; `PersonDetailView.vue`: two `ROUTING_STRATEGY_LABEL`
entries (`organization_default` → "the organization default",
`unassigned` → "no default assignee"). No new routes.

## 7. Authorization & tenant isolation

The Organization comes solely from `auth.active_organization_id` on
both endpoints; the system path takes `organization_id` only from its
trusted caller (tests/CLI resolve it server-side; 007d derives it from
the recipient token). Pinned by tests: member (non-admin) 403 on GET
and PUT; org B's admin setting org A's member → 422 byte-identical to
nonexistent; org B's admin GET never sees org A's setting; a
system-actor intake into org A writes zero rows in org B; the DB CHECK
pair makes a `system` fact with a non-NULL actor (or a `user` fact with
a NULL actor) unrepresentable.

## 8. Observability

`receive_inquiry` span: new `actor_kind` field (`user` | `system`);
`actor_id` recorded only for User actors; routing strategy recorded in
the existing outcome discipline. Settings PUT span: org id and whether
the value was set or cleared (the assignee UUID is an id, not content —
fine to record). Nothing else; no secrets exist in this rung.

## 9. Acceptance criteria

1. The migration applies to a populated database; existing
   Organizations read NULL; `crm_app` can UPDATE
   `intake_default_assignee_user_id` and `updated_at`, and remains
   denied on `intake_slug`, `intake_token`, `name`, and every other
   `organization` column.
2. `routing_decision` accepts both new strategy values and still
   rejects an unknown value (CHECK test).
3. System-actor intake of valid `generic_v1` JSON with a default set
   produces Person + contact methods + Inquiry +
   `inquiry_received` / `routing_decision` (`organization_default`,
   assignee = default) / `assignment_changed` (NULL→default,
   `causation_id` = routing id) / `stage_changed` — all with
   `actor_kind='system'`, `actor_user_id NULL`, the supplied origin,
   and one shared correlation id.
4. That Person appears on the default assignee's Today and on nobody
   else's.
5. Default unset: strategy `unassigned`, `person.assigned_user_id`
   NULL, **no** `assignment_changed` fact; the Person appears in
   `GET /api/people` and on no member's Today.
6. Default set, then that member deactivated: the next system intake
   routes `unassigned` (criterion 5 shape); nothing errors.
7. System intake matching an already-assigned Person → `kept_existing`,
   no assignment fact; matching an existing unassigned Person → the
   default applied with one NULL→default `assignment_changed`.
8. User-actor intake via `POST /api/inquiries` is byte-for-byte
   unchanged (strategies, facts, response shapes) — the full existing
   `db_intake.rs` suite passes unmodified as the regression gate.
9. Re-POSTing a system-routed payload's exact bytes via
   `POST /api/inquiries` → 200 `duplicate: true` with
   `routing_strategy: "organization_default"` (pins the `from_str`
   extension; no 500).
10. PUT validation: nonexistent user, org B's member, and an inactive
    member each → 422 `invalid_assignee`, byte-identical; explicit
    `null` clears; an absent key → 400 `malformed_request`; GET
    reflects every write, and GET still returns the stored UUID after
    that member is later deactivated (deactivation is not a write to
    this setting — the UI's deactivated warning depends on this).
11. Both endpoints: non-admin member 403, no session 401, DB down 503;
    Organization from the session only (body/header/query org ids
    ignored).
12. Person-detail history renders system facts: actor "System", routing
    summaries using the new labels (Vitest-pinned).
13. Settings card: dropdown lists active members only; the unset
    warning and the deactivated-default warning both render
    (Vitest-pinned).
14. `person_changed{inquiry_received}` publishes exactly once per
    system intake (recording publisher), ids-only `v:1`, event shape
    unchanged.
15. `crm-admin receive-inquiry` performs the §1 walkthrough end-to-end
    in dev.
16. `./scripts/check`, `./scripts/check-db`, and web
    lint/typecheck/test/build all green; no new backend dependencies.

## 10. Tests

DB-backed: new `tests/db_intake_settings.rs` (endpoints, grants, CHECK
— `db_intake_address.rs` pattern); system-actor routing cases
(criteria 2–9, 14) appended to `tests/db_intake.rs` or a sibling,
reusing `db_today.rs` helpers for the Today assertions. Service-free
(`tests/intake.rs` pattern): 401/403-shape/503/malformed-body for both
endpoints. Unit: the routing matrix (all four branches ×
matched/unmatched × assigned/unassigned), `for_system` field
construction, `RoutingStrategy` round-trip including the new values.
Web Vitest: `IntakeSettingsView` card states + `PersonDetailView`
label pins.

## 11. Lane and checks

Single lane, branch `slice-007c-system-routing`, one writer, owning:
the migration (sole owner), crm-app domain (envelope, intake actor,
receive_inquiry, queries), crm-api routes + `crm-admin` subcommand, all
tests, and the small web touches (one card, one mutation, one query
key, label/type entries — too small for a second lane; splitting would
put `types.ts` on a shared boundary for no benefit). Gates:
`./scripts/check` and `./scripts/check-db` green, web
lint/typecheck/test/build green, live walkthrough per §1. Reminder:
run `./scripts/db-migrate` after checking out the branch (crm_dev never
auto-migrates).

## 12. Safe defaults adopted (reviewer/user may veto)

(a) The setting is a nullable column on `organization` — no settings
table for one value; (b) plain FK to `app_user`, active membership
enforced in the domain (SLICE_002 §6 precedent); (c) a deactivated
default assignee routes `unassigned` and warns in the UI; the setting
is retained across deactivation/reactivation; (d) `for_system` is
parameterized on `Origin` (CLI now, webhook in 007d); (e) no typed fact
for the settings change itself (AGENTS.md §4.6: routing configuration
is CRUD; every unattended intake still records its effective outcome as
a `routing_decision` fact); (f) 422 reuses the existing
`invalid_assignee` code; (g) `crm-admin receive-inquiry` with
`Publisher::Disabled` as the walkthrough trigger — a dev-only,
D-021-sanctioned tenant-data-creating subcommand at the same trust
level as the existing `invite`; (h) the system path honors an explicit
`assign_to_user_id` (existing `explicit` strategy + validation) rather
than special-casing a rejection; (i) `unassigned` routing emits no
NULL→NULL `assignment_changed` fact; (j) the settings PUT maintains
`organization.updated_at` (grant includes it, membership-grant
precedent); (k) PUT with the key absent is 400 — only explicit `null`
clears; (l) the routing-time active-membership re-check is best-effort
under READ COMMITTED (no row locking); (m) the settings change itself
is unaudited CRUD per AGENTS.md §4.6 — the effective outcome of every
unattended intake is still a `routing_decision` fact (mild tension
with "auditable administrative actions" noted by review and accepted).
