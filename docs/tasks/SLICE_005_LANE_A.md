# Task brief — Slice 005, Lane A (backend)

Parent specification: `docs/specs/SLICE_005.md` (APPROVED 2026-08-22).
Read, in order: `AGENTS.md` (§4, §5, §9, §11 especially),
`docs/decisions/DECISION_LOG.md` (D-008, D-010, D-015, D-028 incl. §5,
D-029), the full SLICE_005 spec, `docs/specs/SLICE_002.md` §2 (fact
envelope, append-only triggers), `docs/specs/SLICE_003.md` §3 (Today
types and ordering), then the existing code under
`backend/crates/crm-api/src/` — `domain/today`, `domain/person`
(`queries.rs`, `model.rs`, `visibility.rs`), `domain/inquiry/queries.rs`,
`auth/context.rs`, `config.rs`, `state.rs`, `error.rs`, `routes/`, the
migrations directory, `tests/common`, and `backend/Cargo.toml`.

## Outcome

The backend half of SLICE_005 §1: the `crm-operator` crate (context,
`ToolBackend`, `InferenceProvider`, view types, `tool_definitions()`,
the loop, `GroqProvider`, `ScriptedProvider`, the prompt), the
`search_summaries` query, the `crm-api` adapter / explanation builder /
ledger writer, config + `AppState.operator`, the three `ApiError`
variants, `POST /api/operator/turns`, the migration, and every test in
§13 items 1–4.

## Ownership boundary

Owns: `backend/**`, `scripts/**`, `.env.example`, the README
Development section, and **the one migration**
(`backend/crates/crm-api/migrations/20260824000001_operator_ledger.sql`).
Does not touch `web/**` or `docs/**` (report needed doc changes to the
coordinator).

Branch: `slice-005-operator`, from `main`, in the main checkout. Lane B
works in a worktree on `slice-005-web` and integrates against this
branch once step 5 lands — announce that point in your report.

## Frozen contracts

SLICE_005 §2 (DDL, grants, `search_summaries` signature — verbatim), §3
(crate public API: `OperatorContext`, `ToolBackend`, `InferenceProvider`,
`TurnOutcome`, `TurnOutput`, the view types, `UntrustedText`), §4 (loop
rules, reference precedence, adapter scoping), §5 (route, body, status,
error codes), §11 (config variables and bounds). The `tool_definitions()`
snapshot is a shared contract once committed. Any deviation stops work
and is reported per `AGENTS.md` §11 — do not adjust a shape to make an
implementation simpler.

## Sequence (spec §15; each step green before the next)

1. Workspace member `crates/crm-operator` with **no** `sqlx`, `axum`, or
   `crm-api` dependency (`reqwest` with `rustls-tls`; add `thiserror`
   and `async-trait` to the workspace). Context, both traits,
   `ToolError`, `ProviderError`, view types, `UntrustedText` (clip 500,
   strip control chars, serialize as `{"untrusted_text": ...}`),
   `tool_definitions()` with `additionalProperties: false`, bounded
   `query`/`limit`, and an `insta`-style or plain-string snapshot test.
   Add `crm-api/tests/operator_deps.rs` (the crate fence).
2. `OperatorService::run_turn` (infallible; every path yields a
   `TurnOutput`), `ScriptedProvider` behind the `test-support` feature,
   the system prompt asset, and the §13 item 1 unit tests — including
   the `FakeBackend` context-injection test and the reference-precedence
   test.
3. `GroqProvider`: OpenAI-compatible `chat/completions` with tools;
   per-call timeout from config; bearer key in a newtype with a
   redacting `Debug`; map HTTP 429 → `RateLimited`, 5xx/connection →
   `Unavailable`, body parse failure → `Malformed`.
4. `person::queries::search_summaries` (§2: `ILIKE` over first/last/
   `concat_ws`, escaped term, plus exact normalized-contact match) →
   `.sqlx/` regenerated via `scripts/sqlx-prepare`.
5. `crm-api`: config (`CRM_OPERATOR_*`, bounds validated even without
   a key; `https://` required except loopback), `AppState.operator:
   Option<Arc<OperatorRuntime>>` (service + semaphore + per-user
   in-flight set with RAII release), `src/operator/{mod,backend,
   explain}.rs`, `ApiError::{OperatorBusy, OperatorDisabled,
   OperatorUnavailable}`, `routes/operator.rs` with the turn run in a
   `tokio::spawn`ed task. **The HTTP contract is reachable here — tell
   the coordinator.**
6. Migration exactly as §2 (envelope CHECKs/FKs copied from
   `contact_attempted`, both `reject_mutation` trigger sets, grants) →
   ledger writer `record_turn` (one `operator_turn` row + N
   `operator_tool_call` rows, one transaction, after the turn; an insert
   failure is logged with `turn_id` and does not fail the response).
7. Tests per §13 items 2–4 — `explain.rs` unit tests, `tests/operator.rs`
   (401, 503 no-db), `tests/db_operator.rs` (validation, disabled, 429
   both cases, release-on-abort, tenant isolation through every tool,
   viewer-specific Today, position equality, the prompt-injection test,
   ledger rows per outcome, 429 writes no row, append-only rejection,
   unknown-field probe).
8. `scripts/check` gains `pnpm run test`; `.env.example` gains the
   `CRM_OPERATOR_*` block; README Development section gains the
   Operator subsection.

## Required checks before reporting done

`./scripts/check` (fmt, clippy `-D warnings`, service-free tests, web
lint/typecheck/test/build) with no services running and **no network**;
`./scripts/check-db` with `dev-services up` (sqlx `prepare --check`,
DB-backed, Centrifugo-backed); one manual turn against real Groq with a
key in `.env` (report the model used and the observed latency). Never
report a check as passed unless it ran.

## Stop and report (do not work around)

- Any change needed to a §2/§3/§4/§5/§11 shape, or to `GET /api/today`,
  `GET /api/people`, or any SLICE_001–004 contract.
- Any temptation to add `sqlx`/`axum` to `crm-operator`, or to put a
  trusted id in a tool input schema.
- A need to log or store message/reply/argument text "for debugging"
  (D-029 — use the ledger and spans).
- Groq tool-calling behaving in a way the loop cannot handle within the
  §4 rules (report observed behavior; do not loosen the rules).
- Any existing test that must change.

## Report format

Files changed (cross-checked against `git status`, not from memory);
behavior delivered per §1 step; commands run with results; contract
changes (should be none); unresolved risks and assumptions; the commit
at which Lane B can integrate.
