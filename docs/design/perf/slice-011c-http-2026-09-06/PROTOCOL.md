# Slice 011c — authenticated HTTP verification protocol

Status: **executed 2026-09-06 (two retained runs); results and limits in [README.md](README.md).**
Declared before execution as follows.
This implements [the approved specification §8](../../../specs/SLICE_011c.md#8-performance-checkpoint-and-observability),
using the same workload and sampling shape as the
[Phase A archive](../slice-011c-2026-09-06/README.md). Phase A remains unchanged.
The primary Terra lane owns every database and runtime operation. The
coordinator owns this protocol and the final evidence report.
The [pre-execution host/toolchain inventory](ENVIRONMENT.json) records the
coordinator's read-only environment checks. Actual build, database, runtime
and measurement facts still require the execution manifest.

## Path and environment

Use a generated isolated database with current migrations and the history-rich
50,000-Person fixture: 46,064 inquiries, 52,306 contact facts including 2,906
corrections, and 2,843 inbound records. Preserve the four viewer books and
five filter definitions from Phase A. Record actual counts before execution.
Use ten application pool connections and the approved two-second acquisition
timeout. Do not overlap load measurement with the live browser walkthrough,
other test suites, builds or another benchmark.

Authenticate through the real session endpoint before each measured series.
Measure real loopback HTTP GET requests through the actual authentication,
Today handler, shared application query, merge and serialization. Request
time runs from dispatch through consumption of the complete response body.
Capture both authentication and feed pool acquisitions, including failures;
the feed's acquisition alone is not the complete request's pool wait.
Source setup uses the approved configuration command/HTTP path, with current
saved definitions rather than the prototype preference table.

The original arm uses the frozen `9d62e86` query/model/ranking and original
planning/JIT policy behind otherwise equivalent authentication and HTTP
middleware in the same build. A test-only fixed-clock seam may select the
same evaluation instant for original and final arms, while preserving the
production transaction-clock query cost. No public benchmark endpoint,
caller-selected clock, new public contract or production tuning is authorized.
Run original and final arms sequentially; do not combine their pool capacity.

## Matrix and sample counts

| Series | Cases | Measured samples per case |
|---|---|---|
| Concentrated book, 100 built-in items | Zero sources, one dense source, one absence source, five overlapping sources | 8 serial; two waves of 10; two waves of 20: 68 total |
| Typical 2,000-Person book, full/truncated built-ins | Zero sources and five sources | 5 serial; one wave of 10; one wave of 20: 35 total |
| 100-Person book, partially filled built-ins | Zero sources and five sources | 5 serial; one wave of 10; one wave of 20: 35 total |
| Empty built-in queue | Zero sources and five sources | 5 serial; one wave of 10; one wave of 20: 35 total |
| Independent concentrated five-source repeat | Same critical fixture and configuration | Two further waves of 20: 40 requests and 200 expected whole-source evaluations |
| Original zero-source comparisons | Each of the four corresponding books, same clock/build/concurrency | Match its final arm's serial/10/20 sample counts |

Six warm-up waves of ten precede each case, matching Phase A. Retain warm-ups
separately from measured percentiles, including their outcomes; they must not
silently disappear. Preassign every attempt an ID and account for transport
errors, incomplete bodies, unexpected status/envelopes, timeouts and task
panics. Never silently omit a failed join or automatically retry an attempt.
The final matrix and independent repeat contain 522 measured final-arm
requests before the separate paired original-arm requests and warm-ups.
For explicit accounting, the independent repeat receives its own six warm-up
waves. Ten final matrix cases, one independent repeat and four original cases
therefore produce **900 retained warm-up attempts**, separate from **522 final
and 173 original measured attempts**. The final measured cases expect **1,201
whole-source evaluations** (476 critical, 525 supplemental and 200 repeat);
these expected counts must be checked against actual telemetry, never used as
a substitute for observations.

The same matrix totals **1,595 GET attempts in 215 waves** including warm-ups.
It expects 1,182 final-arm enumeration events and 2,821 source evaluations
including warm-ups, plus one authentication acquisition and one feed
acquisition per GET. These are arithmetic cross-checks of the matrix, not
additional samples or observed results. Series-login setup is separate.

## Acceptance and retained evidence

Each normal final request must return HTTP 200 with complete source status;
unexpected partial/unavailable, source or pool timeout, HTTP 503 and transport
failure fail the case. Preserve every attempt and earlier failed run. Treat
intentional fault-injection tests as separate recovery evidence, never as
normal successful benchmark cases.

Measured final-arm request p95 must be at most 1,250/2,500/4,500 ms at concurrency
1/10/20. Whole-source p95 in every case must remain below 450 ms. Keep the
250 ms enumeration, 500 ms whole-source and combined 100 ms recovery budgets.
Report nearest-rank percentiles from the retained raw samples, counts, maxima,
authentication/feed acquisition p95 and maximum, and remaining headroom to
the two-second acquisition timeout. Do not omit failures from request timing
summaries or substitute successful-only percentiles.

For paired zero-source cases, compare complete item payloads, reasons, order
and truncation at the common clock, excluding only the declared source
envelope. Final p95 regression may not exceed max(25 ms, 10% of original
p95). A failed or missing original comparison cannot be labeled a passed
regression check; retain it and report the limitation.
The absolute latency caps apply to the final arm. A complete, valid original
series that exceeds those caps remains usable as the measured baseline;
its slowness must be reported without making it a final-arm cap failure.

Retain exact source/build hashes, fixture counts and clock, warm-up and raw
attempt files, safe per-source and acquisition timing, complete outcomes,
query plans and JIT restoration evidence, commands, and cleanup verification.
Trace sentinel checks must establish that names, filter values/JSON and Person
content are absent while static kinds, safe counts and outcome/timing fields
are present. No passwords, cookies, database URLs or debug database errors
belong in the committed evidence archive.

The coordinator's [independent request recount](recompute_requests.py) reads
saved driver waves, checks attempt IDs/context/slots and recomputes outcome
counts and nearest-rank request percentiles, retaining failed joins and missing
timings. It separately groups each case and series, summarizes actual source
and authentication/feed acquisition events, includes client-failure telemetry,
and reports missing metadata explicitly. It associates separately retained late
join telemetry with the original failed attempt, rejects duplicate or mismatched
associations, and reports initial/final capture terminals without reclassifying
a failed join as success. Synthetic checks passed for these paths. Run the
recount once per complete run; incremental checkpoints duplicate earlier samples
and must not be combined with that run's final artifact. It performs no
HTTP/database work and does not by itself establish
matrix completeness, body parity, trace sentinel safety or final acceptance;
those need the complete measured evidence.

These are fixture acceptance limits, not a production capacity promise. If
the bounded implementation fails, preserve the failure and follow the
specification's stop/split rule. Do not increase budgets or pool size, shrink
the workload, drop sources or add unapproved SQL/index/read-model changes.
