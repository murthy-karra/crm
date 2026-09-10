# Slice 017 — Inbound mail size cap implementation brief

**Status: APPROVED by the user on 2026-09-09; the lane starts the same day
in `../crm-worktrees/017` on `slice-017-mail-size-cap`.** The
[specification](../specs/SLICE_017.md) is authoritative for every
contract; this brief sequences the work. One rung, one lane, one writer,
one short-lived branch `slice-017-mail-size-cap` from `main` in
`../crm-worktrees/017`. Model assignment follows the recorded pattern
(`docs/prompts/MODEL_ROUTING.md`): the `implement` profile writes the
lane; the coordinator (Fable) runs review, test analysis, the once-only
final-tree gates and the commit and merge gates. **Only the coordinator
runs `./scripts/check-db`.**

## Read first

AGENTS.md (§8, §9, §11, §14); DECISION_LOG D-012, D-015 §4, D-039, D-042,
D-050, **D-056**, O-015; SLICE_017.md in full; SLICE_007b.md §5 and §11;
SLICE_007g.md §3 and §10; SLICE_009.md §1; `docs/prompts/05-implement.md`.
Then inspect the code named below before writing. Report a contract or
decision conflict to the coordinator; never resolve it locally.

## Outcome

A message of up to 25 MiB (Cloudflare's own inbound ceiling) reaches the
CRM through the relay and is stored whole; the relay streams instead of
buffering; the endpoint accepts bodies to 34 MiB; the frozen JSON envelope
is untouched.

## Ownership

Owns, and nothing else:

- `infra/email-worker/worker.js`, `infra/email-worker/worker.test.mjs`
  (`wrangler.toml` only if `duplex`/limits genuinely need it — report
  first).
- `backend/crates/crm-api/src/routes/inbound_email.rs`;
  `backend/crates/crm-api/src/error.rs` (its `PayloadTooLarge` doc comment
  says 2 MiB; comment only).
- `backend/crates/crm-api/tests/inbound_email.rs`,
  `tests/db_inbound_email.rs`, `tests/db_capture_receive.rs` (additions and
  the moved boundary only).
- `scripts/inbound-email` (spec §7: it passes the whole base64 to `jq` on
  argv and already fails above ~0.75 MiB raw on macOS).

Not owned: `Cargo.toml`, `Cargo.lock` (no new dependency is needed —
`futures-util` is already a crm-api dev-dependency), `.sqlx/`, anything
under `web/`.

No migration exists to own. Nothing under `docs/` except this brief's
status line, which is coordinator-owned. No web change.

## Sequence

1. **Worker.** Replace `base64Encode` with the exported `base64Transform()`
   `TransformStream` factory (spec §2: byte chunks in, ASCII `Uint8Array`
   out, a 0–2 byte carry, padding only at the end; a table-driven encoder
   is the recommended shape). Build the request body as a `ReadableStream`
   yielding the prefix `{"recipient":` + `JSON.stringify(message.to)` +
   `,"raw":"`, the encoded stream, then `"}`. `MAX_RAW_BYTES = 25 MiB` with
   the derivation comment; `AbortSignal.timeout(60_000)`; `duplex: 'half'`
   on the fetch init. Keep the response matrix exactly as it is. Update the
   header comment's derivation.
2. **Worker tests** (spec §5.1–5.5): migrate the alignment tests to the
   stream form; the 25 MiB relay test whose mocked `fetch` asserts the body
   is a `ReadableStream` and consumes it with `new Response(body).text()`,
   with the stub's `raw` delivered as a multi-chunk stream whose chunk size
   is not a multiple of 3; the quoted-local-part recipient; the derivation
   pin against 34 MiB; the existing matrix tests unchanged.
3. **Endpoint.** `MAX_INBOUND_EMAIL_BODY_BYTES = 34 * 1024 * 1024` with the
   derivation comment; borrow `raw` (`#[serde(borrow)]`, `Cow<'_, str>`)
   so the body text is not copied twice. No other handler change. Update
   the module header comment (it says 2 MiB) and the `error.rs` doc comment.
4. **Endpoint tests** (spec §5.6–5.9): move the 413 test to 34 MiB + 1;
   add the chunked-body 413 with
   `Body::from_stream(futures_util::stream::iter(...))` (no Cargo change);
   add the under-limit 401 boundary and the unit derivation assertion; the
   two DB-backed ~20 MiB tests, one per receiving path, generated in-test (a
   fixed-seed pseudo-random attachment; never a committed large file), each
   under about 10 s in the debug profile, no redelivery half. Reuse the
   files' own helpers (`post_inbound_email_raw_body`, `raw_payload_row`;
   `fixture`, `capture_recipient`, `capture_token_for`, `person_history`).
5. **Script.** `scripts/inbound-email`: write the base64 to a temp file and
   build the body with `jq --rawfile raw <file>` (trim the trailing
   newline), write the body to a temp file and post it with
   `--data-binary @file`; verify once with a generated 20 MiB `.eml`
   against the local API (spec §7). The trend timing comes from the
   walkthrough's release binary, not from the debug-profile tests.

## Rules for the lane

- State a short plan before editing; stay inside the ownership list; no
  unrelated refactors or cleanup.
- Checkpoint-commit on the branch as steps land (worker, endpoint, tests)
  so a stalled session loses nothing; use absolute paths.
- Run `./scripts/check` yourself (it runs the worker tests and the Rust
  and Web checks) in the background with a timeout and report its result
  verbatim; **do not run `./scripts/check-db`** — the coordinator runs it
  once on the final tree, and two database-backed runs must never overlap
  in one checkout.
- Never log or print message content, recipients, tokens or the bearer in
  tests, fixtures, spans or the handoff; the generated attachment is
  pseudo-random bytes.
- Report the exact list of changed files at the end; the coordinator diffs
  it against `git status`.
- `duplex: 'half'` is a `worker.js` fetch option and is passed
  unconditionally. If a `[limits]` entry in `wrangler.toml` seems needed,
  or if the frozen envelope would have to change for any reason, stop and
  report (AGENTS §11); do not change `wrangler.toml` vars or the endpoint's
  response shapes.

## Gates

Lane: `./scripts/check` green. Coordinator, once on the final tree:
`./scripts/check` and `./scripts/check-db`; reviewer and tester passes (at
most two rounds, D-050); the file-list audit; then the commit, merge and
push gates with the user; then spec §6's walkthrough (API restart, the
Workers plan check — a Free plan stops before the deploy — `wrangler
deploy` by the user, the real large send) recorded in
`docs/tasks/SLICE_017_VERIFICATION.md`.

## Handoff record

Use the compact record in `docs/prompts/README.md` ("Outputs and
handoffs"): branch and commits, changed files, checks run with results and
the tree they covered, the trend timing, anything assumed, and the next
action.
