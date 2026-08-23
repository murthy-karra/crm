# Slice 006a — `crm-app` crate extraction

Status: REVIEWED (planner + reviewer 2026-08-23; no blocking decision)
Targets: D-028 §1/§5, D-030. Pure refactor: **no behaviour change.**
Prerequisite for 006b (Operator `start_call`): `crm-operator` must be
able to depend on the application layer (typed commands, queries,
`Actor`, Organization context) without a package cycle.

## 1. User-visible outcome

None. Every HTTP route, realtime event, Operator tool, migration and
`.sqlx` query is byte-for-byte the same. The evidence plan (§6) is the
deliverable, not a feature.

## 2. In scope / out of scope

In: a new workspace crate `backend/crates/crm-app` holding the
application layer; `crm-api` keeps Axum, HTTP state, extractors,
telemetry, bins, migrations and all integration tests; shims so call
sites and tests compile unchanged.

Out (explicitly): any edge from `crm-operator` to `crm-app` (006b);
moving the `ToolBackend` adapter; splitting `AppState`; moving bins or
migrations; removing shims; renaming anything; dependency cleanup
beyond deps that lose their last use site; any change to `web/`,
`crm-operator/`, `migrations/`, `.sqlx/`.

## 3. Crate layout after the slice

```
crm-api      -> crm-app, crm-operator          (Axum, routes, AppState, bins, tests)
crm-app      -> sqlx, tokio, reqwest (features = ["rustls-tls"] — declared
                explicitly; LiveKit is https and compile cannot catch its
                absence), argon2, hmac, sha2, rand, base64, chacha20poly1305,
                serde, serde_json, uuid, chrono, tracing, async-trait
                (add others only when a moved file needs them)
                dev: axum (fake LiveKit server test only)
                [lints] workspace = true  (else clippy -D warnings stops
                covering 13k moved lines)
crm-operator -> unchanged
```

No cycle. `crm-app` names neither `crm-operator` nor `crm-api` nor
`axum` in `[dependencies]` (fenced by a test, §6). The 006b edge will be
`crm-operator -> crm-app` per D-028 §1; `ToolBackend` stays as the
injection seam and is untouched here.

Naming: the crate ident is `crm_app`; the Postgres role is also
`crm_app`. Docs say "the `crm_app` DB role" where it matters.

## 4. Module moves (`crm-api/src` → `crm-app/src`)

| Moves whole (git mv, no in-file edits expected) | Stays in crm-api |
|---|---|
| `domain/**` (9.3k lines, 90 `query!` sites) | `routes/**`, `lib.rs` (`build_app`, `run`), `main.rs`, `telemetry.rs` |
| `realtime/{events,publisher,token}.rs` | `state.rs` (`AppState` whole) |
| `telephony/{mod,livekit,token,webhook,scripted}.rs` (`scripted` behind `test-support`; moves because `domain::telephony::*` and `dial_call` import `crate::telephony::{Telephony, SipFailure}`) | `error.rs` (`ApiError`, `IntoResponse`, `From<…Error>` impls) |
| `auth/password.rs` | `operator/{mod,backend,explain}.rs` (`OperatorRuntime`, `SqlxToolBackend`, ledger) |
| | `auth/session.rs` (session DB/token layer, `SessionIdentity`, `SessionOrganization`, cookies, `SessionSecret`) — D-028 §1 keeps the session layer out of what `crm-operator` will reach |
| | `bin/crm-admin.rs`, `bin/migrate.rs`, `migrations/`, `tests/**` |

Splits:

- `auth/session.rs`: the only session item `domain` uses is the pure
  format check (`domain/admin/commands/token.rs` →
  `session::is_valid_token_format`). Move **only** `TOKEN_STR_LEN` +
  `is_valid_token_format` to `crm_app::auth::token_format`; `session.rs`
  otherwise stays in crm-api unchanged and re-exports them so existing
  `session::is_valid_token_format` paths resolve. `SessionSecret` and
  `SessionContext` stay in crm-api; no `SessionSecret` constructor is
  added.
- `auth/context.rs`: only `AuthContext` moves to crm-app — it is the
  one context struct the application layer consumes (`envelope.rs`,
  `person/visibility.rs`). `OrgAdminContext`, `SessionContext` (which
  wraps `session::SessionIdentity`, staying), `PlatformAuthContext`,
  the `FromRequestParts<AppState>` impls and `resolve_session` → new
  `crm-api/src/auth/extractors.rs`. Keep the concrete
  `impl FromRequestParts<AppState> for …` form — a generic `impl<S>`
  would violate the orphan rule cross-crate.
- `domain/admin/commands/token.rs`: its `use crate::auth::session;`
  (for `is_valid_token_format`) becomes
  `use crate::auth::token_format as session;` — the one in-file edit
  inside `domain/**` (an R09x, not R100).
- `config.rs`: secret newtypes `LiveKitApiSecret`, `RawPayloadKey`,
  `CentrifugoApiKey`, `RealtimeTokenSecret` and `LiveKitConfig`,
  `TelephonyConfig` → `crm-app/src/config.rs`. Today they are built by
  tuple syntax inside `from_source`, which is also where the invariants
  live (non-empty keys, minimum secret length). The constructors must
  **carry the same invariant**: `RawPayloadKey::new([u8; 32])`
  (type-enforced); `RealtimeTokenSecret::parse(String) -> Result<Self,
  SecretError>`, `CentrifugoApiKey::parse`, `LiveKitApiSecret::parse`
  with the identical length/non-empty rules, mapped to `ConfigError` in
  crm-api. `Debug` redaction impls move with the types. `Config`, `OperatorConfig`,
  `ConfigError`, `from_source`/`from_env` and all config tests stay in
  crm-api (`Config` holds `crm_operator::GroqApiKey`; `SessionSecret`
  stays too).
  `TelephonyLimits::from_config` and `Telephony::from_config` change
  parameter from `&Config` to `&TelephonyConfig`; callers `state.rs`
  and `tests/livekit_telephony.rs` pass `&config.telephony`. The fixture
  helpers in `raw_payload/crypto.rs` (~l.130) and `realtime/token.rs`
  (~l.78) that call `Config::from_source` switch to the constructors;
  `session.rs`'s helpers are untouched (it stays in crm-api).
  Consequently the moved files that are **modified, not R100**, are
  exactly: `telephony/mod.rs` (`use crate::config`, two `from_config`
  signatures), `realtime/token.rs`, `domain/raw_payload/crypto.rs`,
  `domain/admin/commands/token.rs` (import alias above).

Shims in `crm-api/src/lib.rs`: `pub use crm_app::{domain, realtime,
telephony};`, `auth` re-exporting the crm-app items plus the local
cookie/extractor modules, `config` re-exporting the moved newtypes.
Every `crate::domain::…` in crm-api and every `crm_api::…` in
`tests/` and `bin/` compiles unchanged.

Cargo: `crm-api` adds `crm-app = { path = "../crm-app" }`; feature
`test-support = ["crm-app/test-support"]`; dev-dep on crm-app with
`test-support`. Drop crm-api deps with no remaining use site (verify
by compile, not by guess).

Scripts: `scripts/check-db` runs the ignored DB tests with
`--workspace` instead of `-p crm-api`. `sqlx-prepare` already uses
`--workspace` and the workspace-root `.sqlx/`; nothing changes.

## 5. Ordered steps (each compiles; one PR)

1. Scaffold `crm-app` (empty lib, features, deps); add to workspace.
2. Config split in place: constructors, `from_config(&TelephonyConfig)`, fix the six unit tests.
3. Auth split in place: `auth/token_format.rs` (format check out of `session.rs`), `auth/extractors.rs` (impls out of `context.rs`).
4. `git mv` the moving modules; `crm-app/src/lib.rs` declares them.
5. Shims in `crm-api/src/lib.rs`, `auth/mod.rs`, `config.rs`.
6. Cargo edits; `Cargo.lock`.
7. `scripts/check-db` → `--workspace`.
8. Fence test (§6).
9. Run the evidence plan; docs (PROJECT_STATE, SLICE_005 §3 one-line note, D-028 §5 status remark).

Expected diff: ~12.7k lines as renames (`git diff -M`), 300–500 lines
genuinely modified.

## 6. "No behaviour change" evidence (required before merge)

- `scripts/check` green; `scripts/check-db` green. Unit tests relocate
  between lib binaries, so compare **test names**, not per-binary
  counts: `cargo test --workspace -- --list 2>/dev/null | sed 's/^[^ ]*:: *//' | sort`
  on `main` vs branch must be identical (path prefixes stripped), plus
  total count; DB test counts per `tests/*` binary unchanged (e.g.
  `db_calls` 49).
- `scripts/sqlx-prepare` then `git status --porcelain backend/.sqlx`
  empty — the query set and each query's text/nullability unchanged.
- `git diff main --stat -- backend/crates/crm-api/migrations backend/.sqlx web backend/crates/crm-operator` empty.
- Route table grep on `main` vs branch identical:
  `grep -rho '\.route("[^"]*", *[a-z]*(' backend/crates/crm-api/src/routes backend/crates/crm-api/src/lib.rs | sort`
  (captures only the first method of chained `get(a).post(b)`; the
  `build_app` check below covers the rest). `build_app` diff limited to
  `use` lines; no `routes/**` file shows non-`use` changes.
- Resolved features unchanged: `cargo tree -p crm-api -e features -i reqwest`,
  `-i tokio`, `-i sqlx` identical to `main`.
- Rename purity: `git diff main -M50% --name-status` — everything under
  `crm-app/src/domain`, `realtime`, `telephony` is `R100` except the
  three files §4 names; expected `A` (not `R`) files are
  `crm-app/Cargo.toml`, `crm-app/src/lib.rs`, `crm-app/src/config.rs`,
  `crm-app/src/auth/{mod,token_format}.rs`, `crm-api/src/auth/extractors.rs`;
  each `M` reviewed by hand against §4.
- `! cargo tree -p crm-app -e normal | grep -qE 'axum|crm-operator|crm-api'`
  (negated form so it is `set -e`-safe);
  `cargo tree -p crm-operator` identical to `main`.
- `cargo build --bins` still yields exactly `crm-api`, `crm-admin`, `migrate`.
- New test in `crm-api/tests/operator_deps.rs`:
  `crm_app_has_no_axum_crm_operator_or_crm_api_dependency`, reading
  `crm-app/Cargo.toml` `[dependencies]` only (dev-deps may name axum);
  check the section split boundary rather than copying the existing
  split blindly.

## 7. Authorization, tenant isolation, realtime, Operator, observability

Unchanged by construction; the evidence in §6 is what proves it. No
contract (HTTP, realtime, Operator tool) is touched (AGENTS.md §9).

## 8. Lane

One sequential lane, one writer, branch `slice-006a-crm-app`
(`docs/tasks/SLICE_006a_LANE_A.md`). Parallel lanes would collide on
`Cargo.toml`, `lib.rs` shims and `Cargo.lock`.

## 9. Deferred (LATER, 006b or later)

`AuthContext` has all-`pub` fields and is constructible anywhere; today
only the extractor builds it. After 006a it is a cross-crate public type
that `crm-operator` will see in 006b — 006b must decide the gate
(private fields + a crm-api-only constructor, or an `Actor` newtype per
docs/design/type-safety-hardening.md). Not introduced by 006a (domain
queries already take raw org `Uuid`s), recorded so it is not forgotten.

The `crm-operator -> crm-app` edge and the `operator_deps.rs` wording
once sqlx becomes transitively reachable; moving the adapter into
crm-operator; an app-level services struct in place of `AppState`
fields; moving `crm-admin`/`migrate` bins; removing the shims.
