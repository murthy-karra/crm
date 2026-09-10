# Slice 017 (Inbound mail size cap: Cloudflare's 25 MiB ceiling, a streaming relay) — Verification record

Coordinator-owned evidence for [SLICE_017.md](../specs/SLICE_017.md) and
the [implementation brief](SLICE_017_IMPL.md) under the D-050 budget.
Branch `slice-017-mail-size-cap` from `main` at `59e3245` (the planning
commit), worktree `../crm-worktrees/017`, one lane (Claude Sonnet 5,
`implement` profile), coordinated by Claude Fable 5.1 on 2026-09-09.

| Commit | Step | Content |
|---|---|---|
| `422698a` | worker | `MAX_RAW_BYTES = 25 MiB` with the derivation; `base64Encode` replaced by the exported `base64Transform()` (table-driven, `Uint8Array` in and out, a 0–2 byte carry copied out of each chunk, padding only from `flush`); the request body as a `pull`-driven `ReadableStream` (prefix `{"recipient":<JSON>,"raw":"`, the piped encoder, suffix `"}`); `duplex: 'half'`; `AbortSignal.timeout(60_000)`; the response matrix untouched. Tests migrated to the stream form (chunk sizes 1–5, 8191–8193, 98303–98305 and a mixed split), a 25 MiB relay through a genuine multi-chunk `raw` (chunk 65537, not a multiple of 3) whose mocked `fetch` asserts a `ReadableStream` body and decodes it, a quoted-local-part recipient, the 34 MiB derivation pin, the matrix tests intact. |
| `3ea776f` | endpoint | `MAX_INBOUND_EMAIL_BODY_BYTES = 34 MiB` with the derivation and a unit pin (`>= 4 * ceil(25 MiB / 3) + 4096`); `InboundEmailRequest<'a>` borrows `raw: Cow<'a, str>` (`#[serde(borrow)]`); header and `error.rs` doc comments. Tests: the 413 boundary moved to 34 MiB + 1 with `Content-Length` and as a chunked unknown-length body (`Body::from_stream`), the under-limit 401 boundary at the relay's largest legitimate body, and two `#[ignore]` DB-backed tests generating a 20 MiB multipart message in-test (fixed-seed `StdRng`, a 15 MiB pseudo-random `application/pdf` part, 76-column MIME wrapping): the intake path stores once with `byte_len` and the decrypted bytes exact; the capture CC path stores one `correspondence_raw` row of the right size with the outbound/cc outcome and the history entry. |
| `1fd736b` | script | `scripts/inbound-email` writes the base64 to a temp file and builds the body with `jq --rawfile` (`rtrimstr("\n")`), posts it with `--data-binary @file`; cleanup, secret hygiene and the GNU/BSD `base64` branch unchanged. |
| `33f8284` | round 1 | The six review fixes (below). |

## Lane gates (own tree)

At `1fd736b`: `./scripts/check` all checks passed, 19 s (fmt, clippy `-D
warnings`, cargo check, crate fences, **777** Rust tests with 718
`#[ignore]` skipped, doc tests, Web lint/typecheck/test/build, email-worker
tests **10 of 10** in 230 ms). Targeted DB-backed run replicating
`check-db` step 2 with `-E 'test(/^(db_inbound_email|db_capture_receive)::/)'
--run-ignored only`: **27 of 27**, 11.5 s, on the final committed tree; the
two new tests took **8.835 s** (intake, 20 MiB) and **7.882 s** (capture,
20 MiB) in the debug profile, inside the spec's ~10 s budget. Script proof
(spec §7, the brief's lane-side form): a generated 1,048,576-byte `.eml`
posted to the running pre-017 dev API with a syntactically valid unknown
recipient → `HTTP 200 {"status":"rejected"}`, exit 0; the pre-017 script on
the same fixture → `jq: Argument list too long` (exit 126). The lane
disclosed one self-corrected mistake: a first alignment test fed a buffer
one byte at a time and took 42.6 s; rewritten to 0.2 s before commit.

Coordinator audit at `1fd736b`: the eight changed files match the lane's
list exactly (`git diff --name-only 59e3245..HEAD`; nothing untracked;
`.env`, `backend/target/`, `web/dist/`, `web/node_modules/` ignored); the
largest new blob is a 46 KB test file (no fixture committed); zero diff on
`wrangler.toml`, `Cargo.toml`, `Cargo.lock`, `.sqlx/`, `web/`, `docs/`.

## Review round 1 (of two), on `1fd736b`

Reviewer READY WITH FIXES (no code defect, no human decision); tester no
blocking item. Both verified independently, with their own probes: the
streamed body equals the former `JSON.stringify({recipient, raw})` byte for
byte (60 of 60 cases incl. `"`, `\`, a tab, non-ASCII, U+2028 and a lone
surrogate; chunks of 1 and 65,537 bytes); the carry is correct at every
alignment with padding only from `flush` and a canonical tail; the
pull-driven body reads at most two source chunks ahead (Node experiments
through the real `undici` fetch: chunked transfer encoding, 34,952,581
body bytes for 25 MiB, exact decode, peak live buffers ~1.2 MB during a
1.5 s origin stall; the 60 s abort fires as a throw with ~0.9 MiB pulled);
the handler order, every response shape, the span fields and the
no-oracle 200 are unchanged; `#[serde(borrow)]` yields `Cow::Borrowed`
for escape-free strings; the derivation is pinned from both sides with
699,048 bytes of margin; the chunked 413 test genuinely exercises the
unsized `Limited` path (545 frames); a retry after an abort or a mid-upload
reset resumes the `pending` row on both receive paths; `content_hmac` dedup
is Organization-scoped at size; nothing new reaches spans, logs, test
output or assertion messages; the script at 20 MiB keeps the message off
every argv and removes its temp files on the 200, 413 and API-down paths.

Applied in the fix round (one commit): the mocked `fetch` drains the body
so every worker test drives `pull` during `email()` as `workerd` does, plus
a source-error test (`email()` rejects, `setReject` never called: a future
catch in `pull` posting a truncated body is the silent truncation 007g §3
forbids) [CONTRACT]; the script's bearer moved off curl's argv into a 0600
temp file (`-H @file`; pre-existing, rewritten by this slice) [TRUST]; the
20 MiB and 25 MiB byte comparisons fail with lengths instead of printing
the buffers; a unit pin that `from_slice` borrows `raw`; the dead
`suffixSent` flag removed; the script's `base64` newline comment made
accurate for the BSD branch.

## Round 2 (of two): confirmation on `33f8284`, read-only by the coordinator

The fix commit touches exactly the five files the six items name and
nothing else. Verified on the diff: the mocked `fetch` drains `init.body`
before recording the call, so every matrix test now runs the relay's
`pull` during `email()`, and the new source-failure test is faithful (the
error surfaces inside the mocked fetch, so no call is recorded, `email()`
rejects and `setReject` is never called); `Buffer.compare` and the
length-only `assert!` replace the large-buffer comparisons; the
`Cow::Borrowed` unit pin parses an escape-free string through the real
request type; `suffixSent` is gone with a comment stating why it was dead;
the bearer is written by the `printf` builtin into a 0600 `mktemp` file
passed as `-H @file` and removed by the trap, so it is on no process's
argv. Lane re-verification on `33f8284`: `./scripts/check` all checks
passed, 15 s (**778** Rust tests, worker tests **11 of 11**); the targeted
`db_inbound_email` run 14 of 14 (the 20 MiB test 8.867 s); the 1 MiB
script proof `HTTP 200 {"status":"rejected"}`, exit 0. No third round
needed.

## Final-tree gates (coordinator, once)

On `33f8284` (the final tree: the three lane commits plus the round-1 fix
commit; working tree clean), 2026-09-09, from the worktree root under the
shared gate lock, in one sequence, each once. A first attempt of the same
sequence exited 127 immediately because macOS has no `timeout` command
(the wrapper, not the gates; re-run with a perl `alarm` wrapper):

| Gate on `33f8284` | Result |
|---|---|
| `./scripts/check` | all checks passed, 15 s: fmt, clippy `-D warnings`, cargo check, crate fences, **778** Rust tests (718 `#[ignore]` skipped), 5 doc tests, Web lint/typecheck/**747** Vitest (49 files)/build, email-worker tests **11 of 11** |
| `./scripts/check-db` | all checks passed, 266 s: `sqlx prepare --check` clean, **718 of 718** DB-backed tests on the first run (two more than `main`'s 716: the two 20 MiB tests, 9.691s intake and 8.252s capture under full-suite parallelism); neither known flake occurred |

Performance (D-050): no hot statement changed, so no plan-shape or paired
gate applies; the release-binary span duration and the worker CPU time
are recorded at the walkthrough.

## Recorded LATER (D-050)

Carried from the specification (§8): the per-Organization inbound byte
budget; the pre-authentication per-route concurrency limit; the
`BytesRejection` → 413 mapping for a transport-truncated chunked body; the
raw `message/rfc822` pass-through body as the fallback if the Workers plan
is Free.

From round 1:

- API peak per in-flight large message is nearer 150–180 MB than the
  spec's 110–150 (`body`, `raw` and `sealed.ciphertext` stay alive through
  Phase B, which holds the read-back ciphertext, the plaintext and
  `mail-parser`'s attachment decode); dropping `sealed` after
  `insert_pending` and decoupling `body` after decode would trim ~60 MB.
  BEYOND_ENVELOPE; watch RSS during the walkthrough's large send.
- `base64Transform` assumes `Uint8Array` chunks; an `ArrayBuffer` chunk
  would encode nothing and the message would 400 → bounce (fail closed).
  `workerd` documents `message.raw` as a byte stream. UNREACHABLE; a
  one-line coercion if ever seen.
- A sustained edge-to-origin path under ~4.7 Mbit/s aborts every attempt
  of a 25 MiB message at 60 s → MTA retries → a late bounce. Honest per
  spec rule 4. BEYOND_ENVELOPE.
- A mid-upload 413 from an old API is seen by the relay as a reset and
  retried, not bounced (spec §6 wording corrected on `main`). UNREACHABLE
  by the relay except through the reversed deploy order or `rawSize` drift
  above ~0.68 MiB.
- No `cancel()` hook on the body stream: on a fetch abort the transform
  reader stays locked and idles, bounded at two chunks until the
  invocation ends. Cosmetic.
- The two 20 MiB DB tests (8.8 s and 7.9 s in targeted debug runs) may
  exceed the spec's ~10 s under `nextest` parallelism in `check-db`; trend
  only, not a gate.
- The streamed body, `duplex` on `workerd` and chunked transfer through
  the edge and tunnel are proven only by the walkthrough: send a small
  message first after `wrangler deploy`, then the large one, so a
  relay-level failure is distinguishable from a size-level one.

## Merge readiness

Source `slice-017-mail-size-cap` at `33f8284` (four commits: the worker,
the endpoint and its tests, the script, the round-1 fixes); destination
`main` at `5d89000`, one docs-only commit past the branch base `59e3245`
(the brief amendment), so no conflict is possible: the branch touches no
file under `docs/`. Migration impact: none; `.sqlx` unchanged; no new
dependency. After the merge, with the user's approval: restart the dev
API from `main` by exact PID (the running binary still enforces 2 MiB),
then spec §6's walkthrough — the Workers plan check (a Free plan stops
before the deploy), `wrangler deploy` by the user, a small real message
first, then the 15–20 MB attachment on both receiving paths, with the
observed worker CPU time and the release-binary span duration appended
here. Unresolved risks: the LATER list above. Push, deployment and the
worker deploy are not authorized by this record.

## Post-merge (coordinator, 2026-09-09, with the user's approval)

Merged into `main` at `f06eba3` (`--no-ff`; the eight files, 699 insertions,
91 deletions; no conflict). The dev API was restarted from `main`: the
running process (pid 6939, started 17:38 on the pre-017 binary) stopped by
exact PID, `./scripts/dev-api` relaunched detached (pid 41716, binary built
21:34, `/internal/ready` 200 after ~15 s, log under
`/private/tmp/claude-501/`). Proof the new limit is live: a 3 MiB body with a
bad bearer answers 401 (the old binary answered 413) and a 35 MiB body
answers 413. `dev-web-prod` untouched (no web change). Worktree
`../crm-worktrees/017` removed and branch `slice-017-mail-size-cap` deleted.
`main` pushed to `origin/main` with the records. **Still pending, the
user's actions:** the Workers plan check and, on Paid, `wrangler deploy`,
then spec §6's real sends (a small message first, then the 15–20 MB
attachment on both receiving paths); the observed worker CPU time and the
release-binary span duration are appended here when they exist. The
worker in production is still the pre-017 relay (1.4 MiB bounce) until
that deploy.

**Walkthrough stopped at spec §6 step 2 (2026-09-09): the user confirmed the
account is on the Workers Free plan** (10 ms CPU per invocation). Per
D-056 §3 the streaming relay is not deployed; the decision between the
Workers Paid upgrade and the raw `message/rfc822` pass-through contract
change is with the user. The production relay remains the pre-017 build.
Decided the same day: the Workers Paid upgrade; the deploy and the sends
follow once the plan shows Paid.
