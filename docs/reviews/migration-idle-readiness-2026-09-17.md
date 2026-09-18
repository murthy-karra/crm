# Migration worker idle readiness

User request: run release safety checks after a job is present. Implementation
base: `6c08f95`; branch `codex/migration-idle-readiness`. Local verification passed;
shared development continues running `8fac1dc`.

The scheduler previously loaded release evidence eight times per scheduling cycle
(seven one-second lanes and the two-second history-capture lane), before learning
that their queues were empty. Each load invokes current database compatibility
checks. A 30.12-second shared-development observation recorded 6,337 transactions
and 223–254 scans of several empty migration tables. This is total database
activity, not exact per-query attribution or a benchmark of the change.

The scheduler now first uses each worker's existing, shared candidate SELECT.
No candidate skips release loading and execution until the next normal tick.
A candidate still loads the complete current release evidence before dispatch.
Unavailable evidence remains distinct from an empty queue: existing worker
pause/recovery behavior still runs. Probe errors fail closed and log a bounded
worker kind. Actual workers reselect candidates and retain every transactional
workspace, actor, source, compatibility and lease check.

Combined-family preparation retains its turn without waiting for release evidence.
Its execution hint deliberately includes preparing and ready plans, preserving
executor-revocation cleanup even without a claimable execution job. Readiness is
loaded lazily and reused only for the existing finite drain, never cached across
ticks. Seven other probes share the exact candidate SQL with their workers,
including expired leases and due retry times.

Authority: D-065/010c compatibility boundaries, D-070/010d1 worker admission,
010d3's admitted-work/recovery distinction, and D-092/010g1's existing scheduling
and bounded fairness. Poll intervals, leases, drain limits, source permissions,
HTTP/persistence contracts, schema and dependencies are unchanged. No idle backoff
or release-check bypass was introduced. A new job after an idle observation waits
for the next existing tick; it cannot execute with invented readiness.

For nonempty queues the hint adds one bounded candidate read per drain. This is
an intentional tradeoff to avoid changing worker claim/settlement contracts.
Empty queues issue only the candidate probe for these release-gated lanes;
other workers and family preparation still have their existing database work.

## Verification

Production workspace compilation, formatting, diff checks and normal all-target
Clippy (`-D warnings`) passed. All 99 selected database tests passed in 170.146
seconds, including the three new scheduler regressions and existing release,
core-change, history, People refresh/admission, admitted refresh and family
execution cases. Logs: `/private/tmp/crm-idle-check.log`,
`/private/tmp/crm-idle-clippy.log`, `/private/tmp/crm-idle-db.log`. The migration
browser journey passed all 16 steps on the changed local tree, first attempt,
with `verified_empty` cleanup: [run a518666f517d](../../.e2e/runs/a518666f517d/summary.json).
Its source/build-input manifest records the uncommitted implementation atop the
base revision. It uses the existing controlled synthetic worker
harness; the new scheduler boundary itself is covered by the direct AppState
regressions, not by claiming that harness runs production scheduling verbatim.

New regressions cover the absence of a second pool
acquisition for release validation on all eight empty queues, candidate arrival
after an idle turn, unavailable evidence remaining a work case, probe errors,
and ready-plan eligibility for revocation cleanup. Existing real-database tests
cover release expiry/recovery, tenant and actor boundaries, and job execution.

Commands: isolated `cargo check --workspace --locked`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo fmt --all --check`, `git diff --check`;
`cargo nextest run -p crm-api --test all --locked --run-ignored only` with the
recorded migration-family selector; and
`./scripts/e2e --family migration --concurrency 1 --timeout 900`. No measured
post-deployment transaction-rate claim is made; shared development is unchanged.
