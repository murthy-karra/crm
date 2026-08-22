# Task brief — Slice 003, Lane A (backend)

Parent specification: `docs/specs/SLICE_003.md` (APPROVED 2026-08-21).
Read, in order: `AGENTS.md`, `docs/decisions/DECISION_LOG.md` (D-022,
D-023 especially), the full SLICE_003 spec, `docs/specs/SLICE_002.md` §2,
§4, §5, §13–§15 (the substrate and conventions), then the existing code
under `backend/crates/crm-api/`.

## Outcome

The backend half of SLICE_003 §1: the `contact_attempted` fact and
command, `GET /api/today`, the Centrifugo token/publish integration, the
development transport/config changes, `scripts/demo-leads`, and the
service-free, DB-backed, and Centrifugo-backed tests of §13.

## Ownership boundary

Owns: `backend/**`, `scripts/**`, `infra/**`, `.env.example`, the README
Development section, and **the one migration**
(`backend/crates/crm-api/migrations/20260822000001_contact_attempted.sql`).
Does not touch `web/**` or `docs/**` (report needed doc changes to the
coordinator).

Branch: `slice-003-realtime`, from `main`, in the main checkout. Lane B
works in a separate worktree and integrates against this branch once
step 1 below lands — push the branch after each green step.

## Frozen contracts

SLICE_003 §5 (HTTP), §6 (realtime: channel, token claims and order,
event JSON, Centrifugo config), §3 (ranking, reason codes, tie-breaks),
§11 (config variables). Any deviation stops work and is reported per
`AGENTS.md` §11 — do not adjust a shape to make an implementation
simpler.

## Sequence (spec §15; each step green before the next)

1. Config (`CENTRIFUGO_HTTP_API_KEY`, `CENTRIFUGO_TOKEN_HMAC_SECRET` ≥ 32
   bytes, `CRM_CENTRIFUGO_API_URL` http-only no trailing slash,
   `CRM_REALTIME_TOKEN_TTL_SECONDS` 60–3600; redacted `Debug`) →
   `realtime/token.rs` (HS256, claims `sub, iat, exp, channels` in that
   order, `URL_SAFE_NO_PAD`) → `realtime/publisher.rs` (`Centrifugo` /
   `Recording`; `publish_after_commit` spawns with
   `.instrument(Span::current())`, 500 ms connect / 2 s total;
   `Recording` captures `(channel, serde_json::Value)`) → `POST
   /api/realtime/token` → `infra/development/centrifugo/config.json`
   (`log.level`, `client.allowed_origins`, `channel.namespaces: [{name:
   "org"}]`) → cloudflared path rule (§11). Every existing `test_config`
   helper gains the two required values.
2. Migration (§2 DDL verbatim) + `facts::insert_contact_attempted` +
   `commands/log_contact_attempt.rs` + history UNION arm (`kind_rank` 4)
   + `POST /api/people/{id}/contact-attempts` → re-run
   `scripts/sqlx-prepare`, commit `.sqlx/`.
3. `domain/today/*` (§3/§4: candidates with the `fresh` boolean computed
   in SQL and used in `ORDER BY … LIMIT 201`; `rank()` pure and
   order-preserving) + `GET /api/today`.
4. Thread `publisher: &Publisher` through `receive_inquiry`,
   `assign_person`, `change_person_stage` (§4 event rules; exactly one
   event per execution; `occurred_at` rules incl. unresolved).
5. Tests per §13: service-free (`./scripts/check`), DB-backed and
   Centrifugo-backed (`./scripts/check-db`; the Centrifugo tests read the
   two `CENTRIFUGO_*` values from the environment and must **fail**
   loudly when the container is unreachable). `db_schema.rs` loops gain
   `contact_attempted`.
6. `scripts/demo-leads` (§11: password via stdin, `mktemp` jar + `trap`,
   no `-v`/`set -x`, `GET /api/me` check, default `http://127.0.0.1:3000`).
7. `scripts/check` / `check-db` updates (check-db: Centrifugo
   reachability precheck), `.env.example`, README (new variables; the
   ≥ 32-byte secret note; the two troubleshooting lines in §14a).

## Dependencies to add

`reqwest = { version = "0.12", default-features = false, features =
["json"] }`; dev-only `tokio-tungstenite` (default features, no TLS). No
`jsonwebtoken`. Pin exact versions as the lockfile records them.

## Required checks before reporting done

`./scripts/check` (fmt, clippy `-D warnings`, all service-free tests)
with no services running; `./scripts/check-db` with `dev-services up`
(sqlx `prepare --check`, DB-backed, Centrifugo-backed). Never report a
check as passed unless it ran.

## Stop and report (do not work around)

- Any change needed to §5/§6/§3 shapes or to SLICE_002 §5.
- Cloudflare Access rejecting the WebSocket upgrade through the
  path-routed ingress (the fallback hostname is a user decision, §11).
- A Centrifugo v6.9.2 behavior that contradicts §6/§14a.
- Anything that would require writing to the database outside a
  migration or the application path (D-021 spirit; fixtures in tests
  are fine).

## Report format

Files changed (cross-checked against `git status`, not from memory);
behavior delivered per §1 step; commands run with results; contract
changes (should be none); unresolved risks and assumptions.
