# Slice 010a — Review and corrections

## Round 1 — Backend, fixes required

2026-09-10, Astra review of Terra's compiling backend, before Web. Source
snapshot: `/private/tmp/crm-010a-source/review-1-tree.json` (nine implementation
files). No source calls or DB tests were run by the reviewer. The review below
is based on actual command/SQL/control flow, not a claim that the compiling
implementation meets the acceptance criteria. All findings are within scope.

1. **R1 — P1 TRUST/command boundary.** `routes/migrations.rs` performs
   credential validation/sealing and invokes public store mutations directly;
   those functions accept Organization/actor IDs without checking active admin
   authority. The worker checks membership only once before up to six requests,
   and `commit_check` never checks it. Move orchestration into typed application
   commands/queries with trusted context. Enforce membership at the command
   boundary and at the worker's pre-request/commit boundaries; do not hold a DB
   transaction across source I/O. Prove direct-domain rejection and revocation
   during an in-flight request, not only HTTP extractor denial.

2. **R2 — P1 CONTRACT/idempotency.** `migration_request_receipt` is never used.
   Create rejects an existing connection before replay lookup; replacement
   increments again or conflicts; start ignores request ID and returns an
   arbitrary active job on conflict. Implement scoped operation/request-ID
   receipts with keyed input digests and atomic mutation/receipt persistence.
   Same-input replay returns the same receipt/status even after state advances;
   changed-input reuse is 409. Prove concurrent replay and distinct competing
   starts. A duplicate response must not revalidate externally or write again.

3. **R3 — P1 BOUNDARY/recovery.** `claim_one` excludes expired `running` jobs;
   restart strands them forever. `run_once` runs all six requests under one
   60-second lease, including semaphore waits, and repeats completed checks on
   retry. `MAX_ATTEMPTS` is unused, so transient errors retry indefinitely.
   Claim/reclaim one unfinished bounded check, honor due time, fence expired
   ownership and retain successful checks. Acquire pacing permission without
   consuming a lease while queued. Three attempts per cycle then pause; explicit
   paused retry starts one new cycle while keeping cumulative evidence. Tests
   must cover crash/reclaim, completed-check skip and actual retry exhaustion.

4. **R4 — P1 TRUST/atomic transitions.** `commit_check` inserts evidence before
   checking lease/state, commits it even if the update matched zero rows, and
   the worker ignores that result and sends more GETs. Replacement updates the
   credential and cancels jobs in separate transactions; evidence commits compare
   only the assessment's revision, not the current connection/status. A stale
   worker can write after replacement; a DB failure can leave half the change.
   Lock/validate the relevant scoped rows before evidence and settle everything
   atomically. Stop when a fence rejects the result. Make disconnect idempotent
   (currently increments revision on every DELETE), reject retry of an old
   connection revision, and allow cancellation of a paused job. Preserve
   identity decryptability when revision changes and retain old report identity.

5. **R5 — P1 CONTRACT/honest coverage.** The worker marks any response without a
   continuation `complete_for_query`, including unknown totals or one item out
   of a reported 4. The parser accepts contradictory totals and arbitrary-length
   decimal strings, while persistence is NUMERIC(30,0); binding strings directly
   into a numeric column also needs an explicit checked conversion. Classify
   unknown/inconsistent metadata honestly, normalize bounded integer strings,
   and grant complete coverage only with actual bounded-query exhaustion
   evidence. Keep actual retrieved count even when total is unknown. Prove the
   absent-next/reported-total-greater-than-returned case explicitly.

6. **R6 — P1 TRUST/source evidence.** Successful bounded HTTP bytes disappear
   on malformed JSON/schema errors before the worker can encrypt them. Failure
   outcomes do not carry evidence, and `ProbeResult` derives Debug over raw
   customer bytes. Carry bounded raw response/status alongside parsing outcome,
   preserve it with the classified check result, and redact secret/content
   wrappers. Oversized/truncated bodies stay explicitly incomplete. Require
   positive account/user identity IDs. Prove malformed-200 preservation and
   cross-purpose/Organization/row/tamper decrypt rejection.

7. **R7 — P2 CONTRACT/report completeness.** DTOs omit source display name and
   access scope, destination readiness/reasons/next actions, and separately
   retrievable active versus last completed reports. Retrieved counts are numbers
   rather than the approved decimal strings. Add the approved §6 projection;
   resolve display identity from encrypted data and freeze it per assessment.
   Keep every unchecked family visible. A new running assessment must not hide
   or relabel the last completed report. Pin the exact wire envelopes before Web.

8. **R8 — P2 CONTRACT/source configuration and pacing.** `AppState::new` always
   uses `HttpFubReader::new(None,None)`; no system-name/key configuration exists,
   so registered identification cannot be sent. The code ignores successful
   rate-limit headers and converts arbitrary `Retry-After` u64 to i64 duration
   unchecked. Add redacted configured system credentials and names-only env
   inventory, fail closed for missing/incomplete configuration, and honor shared
   source pacing across validation/probes, including after 429. Bound/validate
   untrusted timing values without overflow or early retry. No test may call FUB.

9. **R9 — P2 BOUNDARY/verification and telemetry.** The worker silently drops
   all errors and has no migration spans; only a handful of parser/crypto tests
   exist at the reviewed snapshot. Add safe IDs, check kind and closed outcome
   telemetry with secret/content redaction. Add substantive DB/HTTP/worker
   tests for the cases above, hostile redirect/continuation/body-size paths,
   tenant isolation, denied/empty/partial results and no business writes. Running
   the existing full DB suite without exercising the new module is not evidence
   of this slice's correctness. Run the required final gates after fixes.

**Disposition:** apply R1–R9 before releasing Part B. This is the first of
D-050's two review/fix rounds; round two covers Web/integration and verification
of these corrections. No new product decision or human approval is needed.

## Round two — Web and integration

Backend R1–R9 corrections passed the frozen targeted checkpoint and coordinator
real-HTTP QA. Review of the initial Web implementation identified the following
in-scope corrections; Terra remains the sole Web writer. No new decision or
contract approval is required.

1. **W1 — P1 CONTRACT/progress.** The 13 unprobed persistence rows are marked
   completed to permit worker settlement. Counting all rows displays 13/19 before
   any source check. Count only the six fixed profile checks in progress/live
   announcements; unchecked rows remain explicitly not checked in the report.
2. **W2 — P1 BOUNDARY/session and disposal.** Scope changes do not reset pending
   flags; an old request's finally block can leave the next session stuck.
   sameScope alone also admits a late completion after unmount. Reset local
   operation state, fence async completions by lifetime/disposal, and ensure
   no old response invalidates or repopulates private cache after a boundary.
3. **W3 — P2 BOUNDARY/recovery and access.** Bind FormField's generated id to
   the password input. Disable assessment for disconnected connections, expose
   supported cancellation of paused work, and permit status reload after an
   initial summary failure. Verify these through accessible component tests.
4. **W4 — P2 CONTRACT/report context.** A previous report's metadata alone is
   insufficient while new work runs: keep its rows inspectable. Display state,
   destination, assessment start/completion window and row observation time so
   source counts retain their visible temporal/query scope.
5. **W5 — P2 user-facing clarity.** Translate closed reason and pause codes into
   understandable labels, preserving honest unknown/partial states. Product copy
   should not expose development terms such as future implementation “slices.”

This is the second bounded review/fix round. Remaining browser/full-gate work
verifies the corrected implementation; do not add unrelated review scope.

## Final disposition

R1–R9 and W1–W5 are verified. W4 verification also corrected the selected
previous report's title; W1 verification corrected unchecked table statuses
that exposed internal completed bookkeeping. Targeted tests, both full gates
and the final production-build synthetic browser walkthrough passed. See
[verification](SLICE_010a_VERIFICATION.md). No additional review scope or
benchmark round was added. Live FUB validation remains user-deferred.
