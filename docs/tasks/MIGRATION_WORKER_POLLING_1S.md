# Migration worker polling — one second

Implemented 2026-09-15 at the user's request to reduce idle PostgreSQL CPU.

- Changed all seven 200 ms migration worker intervals in
  `backend/crates/crm-api/src/lib.rs` to one second. Bounded batch draining,
  missed-tick delay behavior, commands, authorization and persistence are unchanged.
  Other workers that already poll less often retain their intervals.
- The initial low-risk runtime correction was applied to the prior deployed
  Mobile006 source while the Mobile007 release was being prepared. The final
  Mobile007/010d3 release includes this same one-second setting in all seven
  applicable loops.
- Native test APIs on ports 3101 and 3106 retain their existing binaries and data.

## Verification

- `cargo fmt --manifest-path backend/Cargo.toml --all -- --check`: passed.
- Isolated deployed-source build, `SQLX_OFFLINE=true cargo build -p crm-api
  --bin crm-api --locked`: passed (38.12 seconds).
- Read-only migration launch and confirmation preflight: passed.
- `GET /internal/ready`: HTTP 200 after restart.
- Short local samples: `crm_dev` approximately 544 -> 138 transactions/second;
  PostgreSQL container CPU approximately 14–15% -> 5.89%. These are observations,
  not capacity guarantees. Native test databases still contribute about 79
  transactions/second combined.

The initial correction runtime evidence is retained at
`/private/tmp/crm-poll-1s-20260915/`.

Evidence: `/private/tmp/crm-poll-1s-20260915/` (`build.log`, `runtime.json`,
`preflight-launch.json`, `preflight-confirm.json`, `transaction-rates-after.json`).
No SQL or request path changed, so no new plan-shape or paired request benchmark
was needed for this scheduling-only adjustment.

Setup failures retained: the host Python lacks tar extraction's `filter` argument;
archive extraction was retried against trusted local Git content. An initial
binary guard detected that the checkout artifact differed from the still-running
process; the verified prior-release binary supplied the base and recovery copy.
The first restart script used an incorrect health URL and reported failure;
the actual `/internal/ready` endpoint and refreshed confirmation preflight then
passed. No service rollback was necessary.
