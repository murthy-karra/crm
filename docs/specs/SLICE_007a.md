# Slice 007a — Organization intake address (no mail)

Status: REVIEWED (planner + reviewer 2026-08-23; fixes applied)
Ladder: docs/plans/SLICE_007_LADDER.md rung a. Targets O-014 (address
scheme, test domains), D-021 (creation paths), D-007 (config is CRUD
state), AGENTS.md §4.2 (Organization from the session only).

## 1. User-visible outcome

An Organization admin opens **Manage → Intake** and sees:

> Forward lead notifications to
> **leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com**  [Copy]
> Emails sent here will appear as leads once email intake is enabled.

A platform admin sees the same address on the Organization detail page.
`crm-admin create-organization` prints it. Nothing receives mail yet —
this rung only makes every Organization *addressable*, stably.

## 2. In / out of scope

In: the two columns, minting on creation, backfill, the value type +
config, one read endpoint, the settings page, platform detail field,
CLI output, seed slugs.
Out: receiving anything (007b), rotation (007f), any write endpoint,
choosing the final rendered scheme (007f — render is config here).

## 3. Persistence (migration `20260828000001_organization_intake_address.sql`, one file)

```sql
ALTER TABLE organization
    ADD COLUMN intake_slug  TEXT NOT NULL DEFAULT '',
    ADD COLUMN intake_token TEXT NOT NULL DEFAULT '';
-- backfill existing rows (migrations are the sanctioned non-application writer, D-021)
UPDATE organization
   SET intake_slug  = 'org-' || left(id::text, 8),
       intake_token = substr(md5(random()::text || id::text), 1, 8)
 WHERE intake_slug = '';
ALTER TABLE organization
    ALTER COLUMN intake_slug DROP DEFAULT,
    ALTER COLUMN intake_token DROP DEFAULT,
    ADD CONSTRAINT organization_intake_slug_format
        CHECK (intake_slug ~ '^[a-z0-9]([a-z0-9-]{0,38}[a-z0-9])?$'),
    ADD CONSTRAINT organization_intake_token_format
        CHECK (intake_token ~ '^[a-z0-9]{8}$');
CREATE UNIQUE INDEX organization_intake_slug_idx ON organization (intake_slug);
```

Slug: DNS-label-safe, ≤ 40 chars, immutable (no UPDATE grant on it).
Token: 8 chars from `[a-z2-7]` (~40 bits) minted by the application;
the backfill's md5 slice is hex — also matches the CHECK. No UPDATE
grant on either column this rung (rotation is 007f).

## 4. Domain

- `domain/admin/validation.rs`: `slugify_organization_name(&str) ->
  String` (lowercase, non-`[a-z0-9]` runs → `-`, trim `-`, clip 40,
  empty → `org`) and `mint_intake_token() -> String` (`rand`,
  `[a-z2-7]{8}`).
- `create_organization`: mint the token and the slug from the validated
  name inside the existing transaction. A unique violation aborts a
  Postgres transaction, so **no retry-after-violation**: first
  `SELECT intake_slug FROM organization WHERE intake_slug = ANY($1)`
  over the candidates `[base, base-2, …, base-9]` and take the first
  free one; the unique index stays the last-resort guard (a lost race
  → `AdminCommandError::Database`, extremely rare). The existing
  "any unique violation → `OrganizationNameTaken`" mapping must
  discriminate by constraint name (`db_err.constraint() ==
  Some("organization_name_lower_idx")`), so a slug-index violation is
  never reported as a name collision. `queries::insert_organization`
  gains the two parameters. `crm-admin create-organization` and its
  `seed-dev` subcommand need no flag: slugs derive from names
  (`acme-realty`, …); tests read the slug back rather than assuming it.
- New `domain/intake/mod.rs` + `domain/intake/address.rs`:

```rust
pub enum IntakeAddressScheme { Subdomain, LocalPart }
pub struct IntakeMailConfig { pub domain: String, pub scheme: IntakeAddressScheme }
pub struct IntakeAddress { pub slug: String, pub token: String }
impl IntakeAddress {
    /// Subdomain: `leads-<token>@<slug>.<domain>`; LocalPart: `<slug>-<token>@leads.<domain>`.
    pub fn render(&self, cfg: &IntakeMailConfig) -> String;
    /// Accepts BOTH forms regardless of `cfg.scheme` (so the rendered form can flip
    /// without touching storage), case-insensitively; wrong domain, extra labels,
    /// display names, or a malformed local part → None. Never logs its input.
    pub fn parse_recipient(addr: &str, cfg: &IntakeMailConfig) -> Option<IntakeAddress>;
}
```

  `parse_recipient` accepts tokens matching the CHECK alphabet
  `[a-z0-9]{8}` (backfilled orgs carry hex tokens), not only the mint
  alphabet. Unit tests: render/parse round-trip both schemes; `+tag`
  suffixes rejected; uppercase accepted; hex token accepted;
  `leads-abc@evil.com` rejected; `leads-abc@acme.elysianfeld.com.evil.com`
  rejected; a `tracing` capture asserts the input never reaches a span
  or log line.
- `queries::organization_intake_address(conn, organization_id) ->
  (slug, token)`. (The by-address lookup belongs to 007b, where its
  security property is tested.)
- Config: `IntakeMailConfig` + `IntakeAddressScheme` live in crm-app
  `config.rs` beside `TelephonyConfig` (`address.rs` keeps only
  render/parse). crm-api `from_source` parses `CRM_INTAKE_MAIL_DOMAIN`
  (default `elysianfeld.com`; validated as a bare hostname, no
  scheme/port/trailing dot) and `CRM_INTAKE_ADDRESS_SCHEME`
  (`subdomain` default | `local_part`) into a new `Config.intake_mail`
  field, with `ConfigError::{InvalidIntakeMailDomain,
  InvalidIntakeAddressScheme}` + Display arms in the existing style;
  `AppState.intake_mail: IntakeMailConfig`. `.env.example` documents
  both. `mint_intake_token` uses the workspace's `rand` 0.10 API
  (`rand::rng()` / `RngExt`, as `receive_inquiry.rs`).

## 5. HTTP contracts (frozen at approval; AGENTS.md §11)

- `GET /api/organization/intake-address` — `OrgAdminContext`
  (**org admins only**: the token is the one thing between "forged mail
  lands in Unresolved" and "anyone who knows the address creates rows",
  so it is held as tightly as the page that shows it; a member-visible
  variant can be added if a real need appears). 200 `{"address":
  string, "scheme": "subdomain"|"local_part"}`. Errors: 401; 403
  `forbidden` (member); 503 `unavailable`.
- `GET /api/platform/organizations/{id}` — additive **top-level**
  field `"intake_address": string` beside `organization`, `members`,
  `invitations` (not inside `PlatformOrganizationItem`, which the list
  endpoint shares — the list is untouched). Declared additive change;
  pointer line in SLICE_004 §7. Platform reading a tenant's intake
  address is an onboarding-configuration exclusion from D-021's "no
  tenant CRM data" rule, recorded here so it is not read as precedent.
- `crm-admin create-organization` loads `IntakeMailConfig` from the
  two env vars (defaults suffice; no secrets) and prints
  `intake address: …` after the existing output. The token reaching a
  local terminal is accepted for the local CLI (stated per AGENTS §9).

## 6. Web

- Route `/manage/intake` (`requiresOrgAdmin`), sidebar Manage → Intake.
- `IntakeSettingsView.vue`: heading, the address in a monospace pill,
  Copy (clipboard API; "Copied" affordance 1.5 s), the one-line
  explanation above. No editing.
- `PlatformOrganizationView.vue`: "Intake address" row.
- `queries.ts`: `useIntakeAddress(orgId)` under `queryKeys.org(...)`.

## 7. Authorization & tenant isolation

Organization from `auth.active_organization_id` only. Tests: a member
of org B calling the endpoint gets B's address, never A's; two orgs
with colliding names get distinct slugs; the platform field reads by
path id under `PlatformAuthContext` as the rest of that route.

## 8. Failure behavior

Missing/invalid `CRM_INTAKE_MAIL_DOMAIN` → startup `ConfigError`
(existing style). Slug retry exhaustion → 503 from the create route /
CLI error. Clipboard unavailable → the address is still selectable
text.

## 9. Observability

`create_organization` span gains `intake_slug` (a slug is not PII —
it is public in the address). The token is never logged or in spans.

## 10. Acceptance (live walkthrough)

1. `./scripts/db-migrate` on the dev DB → existing orgs get
   `org-xxxxxxxx` slugs and tokens.
2. `crm-admin create-organization --name "Cypress Bay Realty"` prints
   `leads-<token>@cypress-bay-realty.elysianfeld.com`.
3. Log in as that org's admin → Manage → Intake shows it; Copy works.
4. Log in as alice (Acme) → her page shows Acme's address, different.
5. Platform admin → Organizations → Cypress Bay shows the same address.
6. Flip `CRM_INTAKE_ADDRESS_SCHEME=local_part`, restart → the page
   shows `cypress-bay-realty-<token>@leads.elysianfeld.com`; the stored
   row is unchanged.

## 11. Required tests

- Unit: slugify (unicode, punctuation, length, empty), token alphabet/
  length, render/parse both schemes and the rejection list in §4,
  config validation + defaults.
- DB (`db_admin.rs` / new `db_intake_address.rs`): creation mints
  both; two names that slugify identically ("Acme Realty" /
  "Acme-Realty") → `-2` suffix **and not** `OrganizationNameTaken`;
  endpoint: admin 200 with own org's address, member 403, org B admin
  never sees A's; platform field present, non-platform user 403 (cite
  the existing platform-route tests); `create_organization` span
  carries `intake_slug` and never the token. `db_schema.rs` gains
  **new** assertions (it does not enumerate `organization` today):
  both CHECKs accept/reject, the unique index exists, and `UPDATE
  organization SET intake_slug = … WHERE false` is denied to `crm_app`.
  Backfill: `#[sqlx::test]` applies all migrations up-front, so the
  backfill is evidenced by the live walkthrough step 1 (stated) plus
  a unit test of the SQL expressions' outputs against the CHECKs.
- Vitest: the settings page renders the address and copies; route
  guard (`requiresOrgAdmin`); platform row.

## 12. Lane

One sequential lane, one writer, branch `slice-007a-intake-address`.
Migration owned by that lane. ≈ half a day to a day.
