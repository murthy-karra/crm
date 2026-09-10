# Slice 017 — Inbound mail size cap: Cloudflare's 25 MiB ceiling, a streaming relay

**Status: APPROVED by the user on 2026-09-09 after independent review
(READY WITH CORRECTIONS; all ten findings applied, none a human decision).
Approval authorizes committing the planning documents and implementing in
the Slice 017 lane; the code commit, merge, push, deployment and the worker
deploy are asked for separately.** Prepared against `main` at
`7a9f9f8` (Slice 016 merged and pushed, D-055 recorded). One S rung, one
lane. Brief: [SLICE_017_IMPL.md](../tasks/SLICE_017_IMPL.md). Decision:
[D-056](../decisions/DECISION_LOG.md) (resolves O-015 question 1; questions
2 and 3 stay open).

Today the Email Worker relay bounces any message over ~1.4 MiB and the
endpoint caps its HTTP body at 2 MiB. Real-estate mail routinely carries
5–20 MB disclosure packets and signed contracts, so a client who emails a
signed PDF to an agent's capture address gets a bounce from our domain and
the thread never reaches the timeline (O-015 §1; SLICE_009's stated
limitation). Cloudflare Email Routing itself rejects inbound messages over
25 MiB before any worker runs, so 25 MiB is the ceiling at which our own
bounce disappears entirely. This slice moves both constants to that
ceiling and makes the relay stream instead of buffer, because buffering a
25 MiB message plus its base64 plus the JSON copy would need most of a
Worker's 128 MB isolate.

Authority: [D-012, D-015 §4, D-039, D-042, D-050, D-056](../decisions/DECISION_LOG.md);
[007b](SLICE_007b.md) §5 (the frozen endpoint contract, amended here by
pointer), [007g](SLICE_007g.md) §3 (the relay), [009](SLICE_009.md) §1
(capture reuses the frozen contract byte for byte). Cloudflare facts
checked 2026-09-09 against developers.cloudflare.com: Email Routing inbound
message limit 25 MiB; Workers memory 128 MB per isolate; Workers CPU time
10 ms per invocation on the Free plan, 30 s by default on Paid (up to 5
min via `limits.cpu_ms`); request bodies to a Cloudflare-proxied origin up
to 100 MB on the Free and Pro zone plans; a `ReadableStream` request body
is sent with chunked transfer encoding.

## 1. Scope and product behavior

In scope:

- The relay's raw-size threshold becomes 25 MiB and the relay streams the
  message through a chunked base64 encoder into the frozen JSON envelope
  (§2).
- The endpoint's per-route body limit becomes 34 MiB, derived from the
  25 MiB raw ceiling (§3).
- Tests that pin both constants from both sides, the streamed body's
  correctness at every chunk alignment, the moved 413 boundary, and one
  generated ~20 MiB message end to end on each of the two receiving paths
  (§5).
- A live walkthrough with a real large attachment after the API and the
  worker are updated, in that order (§6).
- Pointer amendments to 007b §5, 007g §3 and 009 §1 (§7).

Out of scope (LATER unless stated): moving blobs to object storage and
whole-message relocation (O-015 question 2, forced by the recordings
slice); retention (O-015 question 3, sequenced with O-013); per-attachment
extraction or an attachment read endpoint (nothing serves attachments
today, D-042.6); junk stripping (REJECTED by O-015, not deferred); any
change to the frozen `{"recipient","raw"}` envelope — in particular a raw
`message/rfc822` pass-through body, considered and set aside (§8); the
generic intake JSON cap (256 KiB), the LLM extraction input cap (16 KiB) and
the workbench detail cap (64 KiB), all unrelated to attachments; a
per-Organization inbound byte budget (§8); Cloudflare plan or dashboard
changes (user actions, §8).

Product rules (safe defaults adopted at specification, veto-able):

1. **No message Cloudflare accepts is bounced by us for size.** The relay's
   threshold equals Cloudflare's inbound ceiling. The reject branch stays
   as defence in depth so an upstream limit change can never push an
   unbounded body at the endpoint.
2. **Over the cap the behaviour is unchanged:** an honest `setReject`
   bounce from the relay, and a 413 from the endpoint that the relay turns
   into the same bounce (007g §3's 4xx matrix stands).
3. **Stored bytes do not change shape.** The whole raw MIME, attachments
   included, is stored encrypted as before (D-012 raw preservation);
   `content_hmac` and the `(organization_id, source, content_hmac)`
   idempotency are untouched, so redelivery dedup is unaffected. A message
   that bounced under the old cap and is re-sent now stores normally; there
   is no memory of earlier bounces.
4. **The relay's wall-clock budget becomes 60 s** (was 30 s): a 34 MiB body
   at a conservative 10 Mbit/s takes about 28 s to reach the origin. Email
   Workers have no wall-clock limit, only CPU. An abort still throws, so
   the mail layer temp-fails and the sending MTA retries, as today.

## 2. The relay (`infra/email-worker/worker.js`)

- `MAX_RAW_BYTES = 25 * 1024 * 1024`, with the derivation in the comment:
  Cloudflare's Email Routing inbound limit; `4 * ceil(26_214_400 / 3) =
  34_952_536` encoded bytes plus the JSON envelope fit under the endpoint's
  34 MiB. `message.rawSize > MAX_RAW_BYTES` still rejects before any fetch.
- **Streaming.** The request body passed to `fetch` is a `ReadableStream`
  of bytes that yields, in order: the envelope prefix
  `{"recipient":<JSON.stringify(message.to)>,"raw":"`, the base64 of
  `message.raw` produced chunk by chunk, and the suffix `"}`. The encoder is
  an exported pure `TransformStream` factory (`base64Transform()`), carrying
  a 0–2 byte remainder between chunks so padding appears only at the very
  end, and emitting ASCII bytes (`Uint8Array`), never JavaScript strings, so
  the body needs no `TextEncoder` pass over megabytes. The relay never calls
  `arrayBuffer()`/`text()` on `message.raw` and never concatenates the
  encoded message into one string; peak memory is a few chunks.
- **Chunked transfer encoding is accepted**, not avoided: the runtime sets
  it for any non-fixed-length stream body; hyper and Axum read chunked
  request bodies and `DefaultBodyLimit` enforces the cap while reading. A
  `FixedLengthStream` (exact `Content-Length`) was considered and set
  aside: it couples the relay to `message.rawSize` matching the streamed
  byte count exactly (a mismatch becomes a stream error, that is, a retry
  loop), and Node's `node --test` harness has no `FixedLengthStream`, so
  the encoder would lose its existing unit-test seat.
- The recipient is JSON-escaped with `JSON.stringify` (RFC 5321 quoted
  local parts may contain `"` and `\`).
- Response handling, the 4xx matrix, the bearer, and the no-content logging
  rule are unchanged (007g §3). `AbortSignal.timeout(60_000)`.
- Implementation notes for the lane, not contract: pass `duplex: 'half'`
  unconditionally on the `fetch` init (WHATWG fetch and Node's undici
  require it for a stream body; `workerd` ignores init members it does not
  know; it is a `worker.js` option, not a wire or `wrangler.toml` change).
  A table-driven encoder writing the base64 alphabet straight into a
  `Uint8Array` is simpler and cheaper than the `String.fromCharCode` +
  `btoa` detour and is the recommended shape. Optional, zero-cost proof
  before anything is deployed: `wrangler dev` with `CRM_INBOUND_API_URL`
  pointed at the local API and a generated 20 MiB `.eml` posted to the
  local Email Worker test endpoint, if the installed wrangler exposes it;
  that exercises `workerd`'s stream body, chunked encoding and the CPU cost.

## 3. The endpoint (`crm-api/src/routes/inbound_email.rs`)

- `MAX_INBOUND_EMAIL_BODY_BYTES = 34 * 1024 * 1024` with the derivation in
  the comment (§2's number plus a 4 KiB envelope allowance, rounded up to a
  clean value). This is a value change on one declared row of the frozen
  007b §5 table — "Body over 2 MiB → 413" becomes "Body over 34 MiB →
  413" — reported per AGENTS §11 in §7. The envelope, the handler order
  (413 before bearer, then JSON, then base64), every response shape, the
  span fields (`byte_len` is the decoded size, never content) and the
  no-oracle 200 `{"status":"rejected"}` are unchanged.
- Chunked (unknown-length) request bodies need no code: they already flow
  through `Bytes` extraction under the limit. §5 pins it.
- Memory, an implementation detail the lane should take: borrow `raw` from
  the buffered body (`#[serde(borrow)]` into a `Cow<'_, str>`; serde_json
  borrows when the string has no escapes, and base64 has none) so the
  34 MiB text is not copied a second time. Peak per in-flight large message
  is then roughly 110–150 MB: the 34 MiB body (transiently twice while
  Axum concatenates a multi-frame body), the 25 MiB decoded bytes, the
  attachment `mail-parser` decodes into its own buffer, and the 25 MiB
  ciphertext. A per-route concurrency limit is BEYOND_ENVELOPE:
  capture traffic for a 50-member Organization is a handful of messages a
  minute.

## 4. Persistence and downstream (no change, stated)

No migration. `raw_payload.ciphertext` and `correspondence_raw.ciphertext`
are `BYTEA` with no length CHECK; Postgres stores values to 1 GB and TOASTs
them out of line. WAL amplification of encrypted, incompressible blobs is
the cost O-015 accepts at design-partner scale. `mail-parser` borrows from
its input; extraction still sees at most 16 KiB of text and the workbench
at most 64 KiB; realtime events carry ids only. The capture path (009)
stores the same whole message in `correspondence_raw` and is exercised at
size by §5.

## 5. Tests (calibrated per D-050 and `docs/prompts/06-verify-and-review.md`)

Worker, `node --test infra/email-worker/worker.test.mjs` (already in
`./scripts/check`):

1. `base64Transform()` round-trips exact bytes when the input arrives in
   chunks of 1, 2, 3, 4, 5, 8191, 8192, 8193, 98303, 98304 and 98305 bytes
   and in a mixed split; padding only at the very end. (Migrates the
   existing alignment tests to the stream form.)
2. A 25 MiB message relays: the mocked `fetch` receives a `ReadableStream`
   body (not a string), consumes it, and the parsed JSON carries the
   envelope recipient and decodes to the exact bytes; the stub's `raw` is
   a multi-chunk stream whose chunk size is not a multiple of 3, so the
   carry crosses chunks through `email()` itself, not only at the transform
   level. 25 MiB + 1 is rejected with zero fetch calls.
3. A recipient containing `"` and `\` in a quoted local part produces
   valid JSON with the exact recipient.
4. Derivation pin: `4 * ceil(MAX_RAW_BYTES / 3) + 4096 <= 34 MiB` (the
   endpoint's constant, mirrored).
5. The response matrix tests (200, 413/400 → reject, 401/403/5xx/network →
   throw) pass unchanged against the streaming relay.

Endpoint, service-free `tests/inbound_email.rs`:

6. `oversize_body_with_bad_bearer_is_413_with_the_envelope` moves to
   `34 MiB + 1`; a new sibling sends `34 MiB + 1` as a chunked
   unknown-length body (`Body::from_stream`) and also gets the 413
   envelope.
7. Boundary from the other side: a body of exactly
   `4 * ceil(25 MiB / 3) + 64` bytes with a bad bearer is 401, not 413
   (pins that the endpoint never rejects what the relay can legitimately
   send). A unit assertion pins `MAX_INBOUND_EMAIL_BODY_BYTES >=
   4 * (25 MiB).div_ceil(3) + 4096`.

Endpoint, DB-backed (two tests, each generated in-test, nothing large
committed, each under about 10 s in the debug profile `check-db` runs —
XChaCha20-Poly1305 over 20 MiB costs ~1.8 s per seal or open there, ~55 ms
in release — timed once):

8. `db_inbound_email.rs`: a ~20 MiB multipart message (a short text part
   plus a base64 `application/pdf` part of pseudo-random bytes) delivered
   to an intake address is stored once, `byte_len` equals the raw size and
   the row decrypts to the exact bytes (the existing
   `valid_delivery_stores_one_row_that_decrypts_to_the_exact_bytes`
   pattern). No redelivery half: dedup at size restates
   `byte_identical_redelivery_is_a_noop` (`content_hmac` is
   size-independent in kind) and would cost another ~2.7 s of debug-profile
   crypto.
9. `db_capture_receive.rs`: the same size sent from an agent's mailbox to a
   capture address (the CC path of
   `cc_from_agent_login_creates_outbound_row_...`, using the file's
   `fixture()`, `capture_token_for()` and `person_history()` helpers)
   stores one `correspondence_raw` row and follows the existing capture
   outcome; the Person's history shows the correspondence entry.
10. No new tracing test: the existing no-content capture tests cover the
    span fields, which do not change.

Performance (D-050): no hot statement changes, so no plan-shape gate and no
paired regression. Report once, trend only, from the release binary during the walkthrough:
the `intake.inbound_email` span duration for the large send (a debug-profile
test timing is not a trend figure), and the worker's CPU time as the
dashboard shows it (§6).

## 6. Walkthrough (coordinator and user, after merge)

Order matters: **update the API first, then deploy the worker.** A new
worker in front of an old API would relay up to 34 MiB into a 2 MiB
endpoint; the old API answers 413 mid-upload and closes, which the relay
sees as a connection reset, so it throws and the MTA retries until the API
is updated (verified in review: a retry loop, not a bounce, and not worse
than today), but the reverse order proves nothing.

1. Restart the dev API from `main` (kill by exact PID; the binary runs
   orphaned).
2. Verify the Workers plan on the account (dashboard → Workers & Pages →
   Plans); §8 states why. **If the plan is Free, stop here and do not
   deploy:** the merged 34 MiB API is harmless behind the old relay, whereas
   the new relay on Free would turn today's immediate honest bounce into
   `EXCEEDED_CPU` temp-fails, days of MTA retries and a late bounce. On
   Paid: `cd infra/email-worker && wrangler deploy` (the secret and the
   route survive a deploy; 007g §10).
3. From a real mailbox, send a message with a 15–20 MB PDF attachment to
   an agent's capture address (CC) → accepted; the correspondence entry
   renders on the Person's timeline. Send the same size to the
   Organization's intake address → an Unresolved row showing the size.
4. Record the worker's CPU time for the large send (Workers → the worker →
   Metrics) in the verification record.
5. Optional: a message over 25 MiB is rejected by Cloudflare itself; the
   sender sees Cloudflare's bounce and nothing reaches the worker.

## 7. Contract declaration (AGENTS §11)

| | Current | Proposed |
|---|---|---|
| 007b §5 | per-route `DefaultBodyLimit::max(2 MiB)`; row "Body over 2 MiB → 413 `{"error":"payload_too_large"}`" | `DefaultBodyLimit::max(34 MiB)`; row "Body over 34 MiB → 413", same envelope |
| 007g §3 | relay threshold ~1.4 MiB, derived from the 2 MiB body cap; chunked base64 into one buffered body | threshold 25 MiB (Cloudflare's ceiling); the body is streamed |
| 009 §1 | "Known limitation: the 2 MiB endpoint cap bounces attachment-heavy mail" | closed |

Why: D-056. Affected components: the relay, the endpoint constant, the
tests above, and `scripts/inbound-email`, which today passes the whole
base64 to `jq` on argv (macOS `kern.argmax` is 1 MiB, so it already fails
above ~0.75 MiB raw, under even the old cap): it switches to `jq --rawfile`
reading the base64 from a temp file (trailing newline trimmed) and posts
the body from a temp file. Compatibility: a pure relaxation —
every previously accepted request is still accepted; the only callers are
the relay and that script. Specification updates: 007b §5, 007g §3 and
009 §1 carry pointer amendments to this document; the 007 ladder table and
007e/007h1 mentions of the old numbers are historical and stay.

## 8. Preconditions, risks and the recorded fallback

- **Workers plan (verify before the walkthrough).** On the Free plan a
  Worker invocation gets 10 ms of CPU and Cloudflare's Email Routing docs
  warn that Email Workers exceed it (`EXCEEDED_CPU`); encoding megabytes is
  CPU-bound (the current 1.4 MiB relay has never been exercised at its own
  cap live). A failed invocation temp-fails the mail, the MTA retries, and
  the sender eventually gets a bounce — the outcome this slice exists to
  remove. Workers Paid gives 30 s by default. The plan is not recorded in
  the repository; if the account is on Free, the upgrade is the user's
  decision, and the fallback design is a raw `message/rfc822` pass-through
  body (no base64, near-zero CPU, 25 MiB instead of 34 MiB on the wire) —
  an additive second body form on the frozen route, which is a contract
  change needing its own approval and is not part of this slice.
- **Zone upload limit.** 34 MiB is within the 100 MB request-body limit of
  the Free and Pro zone plans.
- **Storage growth from a hostile sender.** Anyone holding an intake
  address can send distinct 25 MiB messages; byte-identical repeats cost
  nothing (dedup) but distinct ones each cost one encrypted row and its
  WAL. The class of risk exists at 1.4 MiB; the cap changes the multiplier.
  Recorded LATER with the production-deployment trigger: a per-Organization
  inbound byte budget that fails closed with an honest bounce is the D-050
  control if it is ever observed. Not built here.
- **Pre-authentication memory (LATER, same trigger).** 007b §5's accepted
  deviation buffers the whole body before the bearer check, so an
  unauthenticated internet sender can hold N × 34 MiB in API memory (was
  N × 2 MiB); chunked bodies make a `Content-Length` pre-check impossible,
  and "bearer first" would break the frozen 413-before-401 row. The
  fail-closed control is a small per-route concurrency limit (503 → the
  relay throws → MTA retry, honest) or a Cloudflare rate or size rule on
  the path. Not built here.
- **API memory.** Roughly 110–150 MB per in-flight large message (§3);
  fine on the development laptop and a production replica for a handful of
  concurrent deliveries. BEYOND_ENVELOPE beyond that.
- **Truncated streamed uploads (LATER, unreachable by the relay).** The
  handler maps every `BytesRejection` to 413, including a
  transport-truncated chunked body; a 30–60 s upload makes mid-stream
  failure likelier, but the relay's own connection is the one that broke,
  so it throws and the MTA retries; the 413 is never observed. Split
  `LengthLimitError` (413) from other buffering errors (5xx) only if a
  truncated stream is ever seen reaching `setReject`.
- **`rawSize` drift.** If the streamed bytes exceeded `rawSize`, the
  endpoint's 34 MiB limit is the backstop (413 → bounce).

## 9. Acceptance criteria

1. `MAX_RAW_BYTES` is 25 MiB; a 25 MiB message relays and 25 MiB + 1 is
   rejected without a fetch (§5.2).
2. The relay never buffers the message: the body handed to `fetch` is a
   `ReadableStream`; no `arrayBuffer()`, `text()` or whole-message string
   on the raw path (§5.2 plus review).
3. The streamed body decodes to the exact bytes at every chunk alignment,
   padding only at the end (§5.1).
4. Quoted-local-part recipients are JSON-escaped (§5.3).
5. `MAX_INBOUND_EMAIL_BODY_BYTES` is 34 MiB and the derivation is pinned on
   both sides (§5.4, §5.7).
6. Over the limit, with `Content-Length` or chunked, and with a bad bearer:
   413 with the envelope; under the limit with a bad bearer: 401 (§5.6,
   §5.7).
7. A ~20 MiB message stores and decrypts exactly on the intake path and
   stores one `correspondence_raw` row on the capture path (§5.8, §5.9);
   `scripts/inbound-email` posts a generated 20 MiB fixture to the local
   API without an argument-length failure (§7).
8. Every existing `inbound_email.rs`, `db_inbound_email.rs`,
   `db_capture_receive.rs` and worker test passes unchanged apart from the
   moved boundary.
9. The relay's timeout is 60 s.
10. 007b §5, 007g §3 and 009 §1 carry the pointer amendments; the worker's
    header comment states the new derivation.
11. Walkthrough §6 recorded, including the Workers plan (and, if Free, the
    stop before deploy) and, on Paid, the observed worker CPU time and the
    release-binary span duration.
12. `./scripts/check` (Rust, Web and worker tests) and `./scripts/check-db`
    green once on the final tree, run by the coordinator.

## 10. Safe defaults adopted at specification (veto-able)

(a) The threshold sits exactly at Cloudflare's ceiling, not below it; (b)
chunked transfer encoding over a fixed-length body; (c) a 60 s abort; (d)
34 MiB as the clean endpoint constant; (e) borrowing `raw` in the handler;
(f) two DB-backed tests at ~20 MiB rather than 25 MiB, without a redelivery
half, the boundary being exercised service-free; (g) the storage-flood
budget and the pre-authentication concurrency limit recorded LATER rather
than built; (h) the raw pass-through body recorded as the fallback, not
built; (i) `duplex: 'half'` passed unconditionally; (j) a Free plan stops
the walkthrough before the worker deploy.

## 11. Delivery

One lane, branch `slice-017-mail-size-cap` in `../crm-worktrees/017`, the
`implement` profile; the coordinator runs review, test analysis, the
once-only final-tree gates and the commit, merge and push gates. After the
merge: restart the dev API, the user deploys the worker, the walkthrough
runs, the verification record lands, O-015 question 1 is marked resolved
in the log and PROJECT_STATE is updated.
