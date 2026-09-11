# Slice 010b — Implementation verification

**IMPLEMENTED AND SYNTHETICALLY VERIFIED, 2026-09-11.** User approval: D-063.
All required gates passed on the final implementation. Both bounded review/fix
rounds are complete. Changes remain in the implementation worktree, uncommitted
and unmerged; no 010b runtime deployment or live-source validation is claimed.

## Baseline and ownership

Approved planning commit: `49581a0`, following published documentation `a498a2c`.
Branch: `codex/slice-010b-core-snapshot`.
Worktree: `/Users/karrad/projects/crm-worktrees/010b`.
Application baseline is the deployed 010a source `0735015`.

One primary backend writer owns persistence, the sole additive migration,
source/preview engine, API and focused tests. A bounded support writer owns the
pure source-profile parser, and another owns the compile-fenced synthetic QA
example/fixtures. The coordinator owns evidence/current-state integration and
Web after the backend checkpoint. File ownership is agreed before edits.
Agent model/effort was inherited from the runner; an exact effective value was
not exposed and is not inferred. DB-backed gates ran sequentially.

## Setup and focused evidence

- Read repository instructions, accepted decisions and the approved spec/brief.
- Approval/pointer documentation passed `git diff --check` and was committed
  locally as `49581a0`. The implementation worktree was created from that commit.
- Private `.env` copied with mode 0600. Build artifacts and Web dependencies
  cloned with APFS copy-on-write, isolated from the shared-development checkout.
- Postgres and Centrifugo containers were observed healthy. The installed pinned
  runtime is Node 24.16.0 / pnpm 11.22.0 under
  `/Users/karrad/.nvm/versions/node/v24.16.0/bin`.
- Public source-profile refresh verified all seven saved schema hashes and kept
  task pagination/terminal evidence and user/People field-scope gaps explicit.
  No authenticated FUB request or customer-data processing occurred.

- Four independent HTTP acceptance tests are written in `tests/db_snapshot_http.rs`;
  formatting passed. All four subsequently passed in the focused DB gates below.
- The QA example and synthetic JSON fixtures passed formatting/JSON/whitespace
  checks; its later startup and walkthrough are recorded below. Private QA configuration uses fresh local
  session/content keys and only loopback `crm_slice010b_qa` URLs.
- Created disposable database `crm_slice010b_qa`, owned by `crm_migrator`;
  migration and synthetic seeding subsequently succeeded. Initial catalog inspection using an
  assumed `postgres` role failed because that role does not exist; inspection
  using the container's configured role succeeded. `crm_dev` was not changed.

- `cargo check --workspace --locked` passed for the production-shape backend
  (primary runner, 12.78 seconds). The initial source-only compile found missing
  comparison-module/private-import and NormalizedEmail integration errors;
  the coordinator corrected them before that passing build.
- `SQLX_OFFLINE=true cargo test -p crm-app snapshot_ --lib` passed 18 tests
  (11 source, six comparison and one crypto), zero failures, 503 filtered out,
  0.05 seconds test runtime, reported by the source-profile writer. A subsequent combined run passed all 20 tests (12 source, seven comparison,
  one crypto), including the new lossless-reference and marker-collision cases;
  primary reported this pass before starting the exclusive DB gate.
- Pure comparisons now reuse the current destination validators and return
  separate unique candidate IDs. Date-only task deadlines remain ambiguous;
  a valid timestamp with an offset and a companion date does not create a false
  missing-time issue. Original source projections remain unchanged.
- Pinned `pnpm install --offline --frozen-lockfile` passed, with no dependency
  manifest or lockfile changes. Local Chrome and Playwright are available for
  the isolated browser walkthrough; its planned ports were unoccupied.

- First exclusive focused DB gate: `cargo test -p crm-api --test all db_snapshot_
  -- --ignored --test-threads=1` passed eight tests (four independent HTTP and
  four worker/durability tests), 14.63 seconds test runtime, primary runner.
  Tests used sqlx disposable databases, not `crm_dev`. Scale registration and
  later recovery additions were not part of these eight tests.
- A further source projection test now covers the Rust-to-JavaScript safe-integer
  boundary: integers outside ±9,007,199,254,740,991 use the existing exact-number
  wrapper even when Rust can preserve them natively. Its test passed in the subsequent 21-test library gate; raw source IDs remain decimal strings.

- First 25,000-People shared-contact run reached real preview generation and
  exposed a member-page plan regression: at cursor 12,000 the preview-record
  join scanned 2,292 earlier rows. The writer added the symmetric source-ID
  cursor predicate on that join; the subsequent full 11-test gate passed. Earlier claim/latest/stream/
  checkpoint/record/group/per-record-overlap plans in that run passed. The failed
  run took 33.73 seconds; it is not recorded as a successful scale verification.
  These directly seeded encrypted migration rows are query-plan fixtures, not
  evidence of source capture fidelity.

- Backend checkpoint: 21 focused library tests passed (13 source, seven
  comparison, one crypto), and scoped `cargo clippy -p crm-app -p crm-api
  --all-targets --locked -- -D warnings` passed in 47.19 seconds.
- The next exclusive focused DB gate passed all 11 tests in 46.56 seconds,
  including two further recovery/accounting cases and the corrected 25,000-People
  case: 502 preview batches, one group, 500 member pages, 100 duplicate source
  observations, one post-boundary record excluded and zero business People writes.
  Full evidence: `/private/tmp/crm-010b-backend-tests.log`; scoped Clippy evidence:
  `/private/tmp/crm-010b-clippy.log`. A schema-only request-fingerprint lookup
  index was added after that gate and passed the final fresh-migration gates.
- Frozen backend checkpoint SHA-256 inventory (26 backend/contract files):
  `/private/tmp/crm-010b-backend-checkpoint-hashes.json`. No credentials included.

- Synthetic QA example compile check passed unchanged:
  `cargo check -p crm-api --example snapshot_qa --features test-support --locked`
  (15.36 seconds), followed by scoped Clippy with `-D warnings` (0.21 seconds).
  These checks did not start the API or access the QA database.

## Web and synthetic API verification

- The isolated test-support API ran on 127.0.0.1:3011 with the explicitly named
  `crm_slice010b_qa` database. The ordinary seed script created synthetic admin
  and member accounts in two Organizations. No production reader or fixture
  activation switch was added.
- Normal real-API capture/preview passed: six record families (2 users, 2 stages,
  3 custom fields, 3 People, 2 notes, 3 tasks), one qualified note-detail gap,
  one task variant, email group of two and phone group of three. One-item API
  keyset pages reached every group/member/record. All 45 non-migration/non-session
  business-table counts stayed unchanged. Nine positive captures retained 5,097
  raw bytes. The private helper initially expected an unknown field in the bounded
  projection; its assertion was corrected to the omission marker and the same
  capture/report was reused, without an application or fixture change.
- Initial Web typecheck and production build passed; scoped legacy assessment
  and budget tests passed 13 cases. Core panel tests subsequently passed 28 cases.
  Final preview-panel focused tests passed 12 cases, including R2 omissions/limits.
  The budget tests include exact GiB/byte conversions above JavaScript's safe
  integer range, explicit review, frozen revisions, ceiling/conflict handling and
  uncertain-response request reuse. No dependency manifests or lockfiles changed.
- A production-build Chrome walkthrough passed retained review, overlap members,
  escaped notes, 390px layout, explicit proposal/confirmation, reload during
  progress, pause, previous report, partial preview, separate source resume,
  a fresh frozen report, cancellation, retained preview after disconnect, cross-tab
  logout, other-Organization isolation and member route/API denial. No page errors
  were observed. Document scroll width was exactly 390px at the 390px viewport.
- Browser helper failures were corrected: an immediate scripted reload could abort
  the confirm request before submission (the helper now waits for its response),
  and an empty partial-preview record container has no visible box (the helper now
  checks attachment and the actual completed report). These were test-driver
  assumptions, not claimed passes or product-code corrections. The initial login
  driver likewise needed to wait for sign-in navigation before opening a new URL.
- Safe private runtime evidence: `normal-api-result.json`, `business-before.json`,
  `business-after.json`, `browser-flow-result.json`, and screenshots under
  `/private/tmp/crm-010b-qa`. The Web runtime is Vite preview of `web/dist`, not
  a development transform server. Full-gate/final-build evidence follows below.

- The browser storage-recovery walkthrough also passed with small logical limits:
  64 MiB admitted one identity capture then paused; raising the deployment ceiling
  enabled an explicit 128 MiB approval. The UI rejected over-ceiling values and
  the server rejected a reviewed policy made stale by a restart. A fresh approval
  left capture paused until explicit source resume. Lowering the deployment ceiling
  subsequently paused a queued DB-only preview; restoring it did not resume work,
  and explicit preview resume completed the same report without another source
  capture. No page errors occurred. Private normal policy was restored afterward.
- The budget driver initially used the wrong case for the existing connection
  status label. A later driver attempt overlapped credential replacement with a
  proposal using the old revision; confirmation correctly returned 409. The
  driver was corrected to reuse the existing synthetic connection and serialize
  its dependent steps. No application fix was needed for these driver errors.
- The budget review wording also handles a deployment ceiling lowered below a
  previous allowance, and identifies the frozen policy revision alongside run
  and Organization revisions.
- Final production-build rendering passed after the full gates: six screenshots
  cover desktop and 390px capture, escaped notes, preservation notices and budget
  confirmation. All six were visually inspected; no clipping of the confirmation
  controls or page overflow was observed. Wide stream tables scroll within their
  own container. No page errors or migration mutations occurred during this render.
  These narrow screenshots exercise the Web interface, not native mobile clients.
- The temporary Vite process emitted WebSocket `EPIPE` proxy messages during
  the QA session. Snapshot workflows use HTTP polling; their browser assertions
  passed with zero page errors. This evidence does not claim new realtime or
  telephony validation.

## Final required gates

Executed sequentially on the final backend/Web tree with Node 24.16.0 and
pnpm 11.22.0. All three commands passed:

| Command | Result |
|---|---|
| `./scripts/sqlx-prepare` | Fresh throwaway database; workspace compile 36.98s; no tracked SQLx metadata changes |
| `./scripts/check` | 860 Rust tests, 5 doctests, 933 Web tests across 55 files, 11 email-worker tests; formatting, Clippy, production compile, crate boundaries, Web lint/typecheck/build; total 56s |
| `./scripts/check-db` | 809/809 DB tests passed in 330.602s; total 386s, including fresh SQLx prepare/check |

The service-free gate skipped the 809 DB cases; the DB gate skipped the 860
service-free cases. The 25,000-People test passed in the full suite (90.543s,
one informational slow-test marker). The existing LiveKit-related large Web
chunk warning remains. SQLx also reported potentially unused offline queries;
its check still passed and no tracked metadata changed. No dependency or lockfile
was added or changed.
Private full logs are `/private/tmp/crm-010b-final-sqlx-prepare.log`,
`/private/tmp/crm-010b-final-check.log` and
`/private/tmp/crm-010b-final-check-db.log`.

## Acceptance tracking

All specified implementation/synthetic checks passed, with external source
qualification explicitly deferred. Specification: [SLICE_010b](../specs/SLICE_010b.md).

| Acceptance | Required evidence | Status |
|---|---|---|
| §8.1 | Proposal/confirmation, expiry, replay, concurrent confirmation | Passed focused DB + Chrome |
| §8.2 | Admin/tenant isolation; source versus retained authority and fencing | Passed tenant/actor DB tests + Chrome session/member checks |
| §8.3 | Six families, partitions, representations, lossless values, exact encrypted bytes, no business writes | Passed parser/worker/real API checks; business counts unchanged |
| §8.4 | Pagination/exhaustion/loops, denial and qualified negative-item settlement | Passed bounded parser + worker failure/gap tests; live qualification deferred |
| §8.5 | Atomic checkpoints, retries, lease reclaim, crash/commit failure | Passed failed-settlement, lease/reclaim and preview-resume DB regressions |
| §8.6 | Shared assessment/snapshot exclusion and pacing, credential/cancel fencing | Passed existing/new cross-job and fencing tests and full regression gate |
| §8.7 | Crypto scope/purpose/tamper and log redaction | Passed crypto scope/tamper unit tests; source/credentials excluded from logs/DTOs |
| §8.8 | Frozen deterministic preview, staleness, resumability, bounded overlap groups | Passed frozen-input/retry/accounting DB cases and 25k group case |
| §8.9 | Production-build real-API synthetic browser walkthrough, narrow layout and session transitions | Passed Chrome workflow/storage/session/390px and final-build visual inspection |
| §8.10 | Representative indexed query plans within D-050 | Passed 25k EXPLAIN (ANALYZE, BUFFERS) plan assertions |
| §8.11 | Focused checks and final sequential sqlx-prepare/check/check-db | All focused and final gates passed |
| §8.12 | Durable reservations, accounting, policy ceilings, monotonic budget receipts | Passed ledger/reservation/revision DB tests + Chrome storage recovery |

## Review checkpoints

At most two implementation review/fix rounds under D-050: backend checkpoint,
then final integration. Round 1 is complete as a bounded review; its correction
pass passed all 13 focused DB tests (69.98 seconds), scoped Clippy (25.02 seconds)
and whitespace checks. Evidence: `/private/tmp/crm-010b-r1-tests.log` and
`/private/tmp/crm-010b-r1-clippy.log`. The coordinator reviewed parser, schema, source worker and
reader integration; an independent support reviewer covered authority, previews,
API and comparison behavior, excluding its authored parser. No new DB run was
performed by the reviewers. Findings on the frozen checkpoint:

1. P2 / BOUNDARY: restrict contact-overlap counts to People; colliding source IDs
   in notes/tasks/users must not inherit a Person's group.
2. P2 / TRUST: persist the actual budget-approving actor and old/new revisions in
   the immutable receipt, and bind the actor in its idempotency digest.
3. P2 / CONTRACT: persist and publish separate nonexclusive issue counts with
   preview-batch atomicity, alongside primary dispositions.
4. P2 / RECOVERY: an expired running source lease must revalidate identity even
   when reclaimed by the same process. Independently identified by the writer
   during checkpoint packaging and confirmed by the coordinator.

The primary writer completed this one correction pass and targeted regressions.
The corrected wire/schema contract was frozen before Web started.

Round 2 reviewed the R1 backend corrections and the integrated Web flow. An
independent reviewer found one P2 display-fidelity issue: omitting `_snapshot`
metadata hid omission-only records without a truncation issue. The preview writer
added concise notices for omitted fields, clipped values, limited variants and
separately preserved raw captures, with 12 focused tests passing. Backend R1
corrections had no additional actionable finding. Both allowed rounds are complete.
Earlier planning review is preserved in [the plan review record](SLICE_010b_REVIEW.md).

## Final inventory and handoff

The [sanitized evidence bundle](../design/qa/slice-010b-2026-09-11/README.md)
contains the exact 37-file implementation/configuration/contract SHA-256 inventory,
build hashes, gate results, real-API/browser results, business counts, 13 actual
query plans and six final screenshots. Private configuration, keys, passwords,
session cookies and full runtime logs are excluded.

Final handoff checks passed: 84 local Markdown target paths, 10 changed JSON
files before the handoff/integrity manifests, all 37 source hashes, Git whitespace
and a bounded credential-pattern scan with zero matches. An independent agent
confirmed the source hashes and full-gate counts without rerunning tests. Both
temporary QA ports (3011/5181) were confirmed closed.

- Backend: five migration-domain snapshot modules; shared reader, commands,
  crypto and storage integration; one additive migration; API routes, startup,
  state and budget configuration; three registered DB test files; a compile-fenced
  synthetic API example and fixture.
- Web: typed snapshot API, scoped query keys, three migration panels and their
  focused tests, shared formatting helpers and existing MigrationView integration.
- Documentation: frozen concrete contract, refreshed public source qualification,
  verification/evidence and current-state/architecture/brief pointers. Foundations
  F-01/F-02/F-03 remain proposals rather than newly accepted policy.

All 45 business-table counts were checked again after the full walkthroughs
and remained equal to the initial inventory. The temporary QA API and Vite
preview were stopped after verification. The
disposable synthetic database and private helpers remain available for reproducing
the evidence. Shared-development 010a was not restarted or migrated. Integration
and deployment are the next release work; the user-deferred authorized FUB check
remains separate. Later import behavior still needs its own approved specification.

## Deferred external validation

Live authorized FUB validation remains expressly user-deferred. Synthetic
fixtures do not establish source-account access, full endpoint coverage or
customer-data readiness. D-015/O-012/O-013 prerequisites remain open. No import,
cutover, source writes or runtime deployment is part of this implementation.
