# Hardening chunk N1 — `OrganizationId`

Parent plan: `docs/design/type-safety-hardening.md` (chunk 2; read its
"sqlx strategy" section — it is binding). Branch: `hardening-n1`.
Single lane, one writer. Read `AGENTS.md` first. S1
(`docs/tasks/HARDENING_S1.md`, merged) established the row-boundary
pattern; this chunk establishes the id-newtype pattern every later id
chunk reuses.

## Goal

The tenant key gets its own type. Every crm-app and crm-api site that
today passes `organization_id: Uuid` takes `OrganizationId`, so that
swapping the org id with ANY other id (user, person, payload, call, …)
becomes a compile error. This is the single highest-value chunk in the
ladder (tenant isolation).

## Hard invariants (violating any fails review)

1. **No SQL string changes**; bind `.0` (or `.as_uuid()`) at
   `query!`/`query_as!` sites. `backend/.sqlx/` must stay byte-
   untouched (`git status` proof).
2. **No wire changes.** The newtype is `#[serde(transparent)]`; every
   JSON payload and realtime event serializes byte-identically (the
   exact-shape pins in `realtime/events.rs` tests must pass
   unmodified — if one fails, your change is wrong, not the test).
3. **No DB changes, no migrations.**
4. **Only `OrganizationId` this chunk.** No `UserId`, `PersonId`, etc.
   (later chunks). No unrelated cleanup.
5. **The crm-operator crate is NOT touched.** Its tool seam stays bare
   `Uuid` (D-028 §5 crate fence); `crm-api/src/operator/backend.rs`
   converts at the boundary. The fence tests in `./scripts/check` and
   `crm-api/tests/operator_deps.rs` must stay green.

## The type

New module `backend/crates/crm-app/src/ids.rs` (wired into lib.rs):

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct OrganizationId(pub Uuid);
```

plus: `Display` delegating to the inner Uuid (span fields use `%org`),
a `Debug` matching Display (ids are not PII), `fn new(Uuid) -> Self`
and `fn as_uuid(self) -> Uuid` conveniences. Doc comment explaining
the chunk's purpose with a ```compile_fail doctest showing that a
bare `Uuid` (or any future other id) no longer compiles where
`OrganizationId` is expected. House-style doc tone (see `SenderTrust`
in `domain/intake/email/forward.rs`).

Do NOT add `From<Uuid>`/`Into<Uuid>` implicit conversions beyond
new/as_uuid — the friction is the feature; a conversion must be
visible at the call site.

## The work (compile-guided; let rustc drive the sweep)

1. Add the type; change the **core structs** first:
   `CommandContext.organization_id`, `FactEnvelope.organization_id`,
   `IntakeActor::System { organization_id }`, crm-api's
   `AuthContext`/session identity, `OrganizationScope` if present —
   then follow compile errors outward through crm-app commands,
   queries, store fns, realtime (`channel_for`, `RealtimeEvent`
   constructors and data structs), crypto
   (`seal`/`open`/`associated_data`'s org param — the AAD tenant
   binding; the BYTES it produces must be identical, i.e. still the
   inner uuid's bytes), telephony, admin, intake, extraction worker.
2. crm-api edges: session extraction constructs `OrganizationId` once
   where the org id enters (auth/session read), routes pass it
   through; `operator/backend.rs` converts to bare `Uuid` where it
   calls into crm-operator types and wraps again on the way back.
3. Row/DTO structs: per the parent doc, sqlx row structs KEEP bare
   `Uuid` fields where they are private mapping intermediaries;
   public query-fn signatures and returned domain structs carry
   `OrganizationId`. Judgement call per site; bias to typing public
   boundaries, not private plumbing.
4. Tests in the workspace that construct org ids adapt mechanically
   (`OrganizationId(x)` / `.0`); do not weaken any assertion.
5. New tests: the compile_fail doctest(s) in ids.rs; a
   Display/serde-transparency unit test (uuid string in, same string
   out through serde_json). Everything else is covered by existing
   gates — notably the realtime exact-shape pins and the db suites.

## Watch-outs (from the survey; verify each ends up typed)

- `crypto::seal/open(key, organization_id, raw_payload_id, …)` — org
  half of the AAD. After this chunk the org/payload swap must not
  compile. (payload id stays `Uuid` until N3 — that's fine, the org
  side alone kills the swap.)
- `realtime::token::mint(secret, user_id, organization_id, …)` — the
  user-first argument order that invites the swap; org side typed now.
- `channel_for(organization_id)` — the realtime channel derivation.
- The ~96 `organization_id` bind sites — all become `.0` with SQL
  text untouched.

## Checks before reporting done

`cargo fmt`, then `source ~/.nvm/nvm.sh && ./scripts/check` and
`./scripts/check-db`. Also `git status --short backend/.sqlx/` (must
be empty) — include that proof in the report.

## Report

Files changed (must match `git status` exactly), the core-struct list
you typed, conversion sites at the crm-api/crm-operator seam, checks
run + results, assumptions, surprises. Raw data, no pleasantries.
