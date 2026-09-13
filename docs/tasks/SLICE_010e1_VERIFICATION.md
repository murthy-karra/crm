# Slice 010e1 — Implementation and verification

**2026-09-12: implementation locally integrated and verified.**
Coordinator runtime walkthrough and final combined implementation gates passed
as attributed below; publication and release remain separate. No live FUB call, customer processing,
canonical CRM mutation, delta application or activation is included.

D-075 approves the [specification](../specs/SLICE_010e1.md).
The [frozen contract](SLICE_010e1_CONTRACT.md) records exact DTOs and ownership.

## Implemented behavior

The admin API compares the original completed 010c capture with a later retained
same-account core capture. Its source-only worker decrypts complete raw evidence,
rechecks stored IDs/ordinals/representations/semantic HMACs and retains every
observation's effect. Unknown properties, missing/null and array order participate
in lossless comparisons. A bounded aggregate and indexed per-variant counts avoid
loading an entire high-fan-out source entity or rescanning raw pages per record.

Notes preserve list and enriched-detail components. Restricted detail responses
are linked through the original request fingerprint and remain unresolved. Tasks
combine open/completed membership and report partition changes. One-sided rows
carry access and absence warnings; absent observations do not authorize deletion.
Closed categories and representative evidence references expose no source bodies,
field values, labels, arbitrary field paths or source URLs.

Typed preparation/control commands bind request receipts to the trusted actor,
Organization, body and frozen input/engine digest. Leases use a random token and
persisted epoch; bounded commits revalidate current authority, workspace, frozen
inputs, storage and engine readiness. Statements have a 10-second timeout, units
45 seconds, and final publication atomically checks the unexpired lease. Partial
rows stay internal. Completed rows/counts share an immutable output revision.

Report-owned reservations and logical byte triggers charge variable keys/hashes,
encrypted inputs, aggregates, output, receipts and checkpoints to the newer
snapshot and existing Organization allowance. They never settle another source,
preview or import reservation. A separate cancellation reserve supports recovery
when normal admission is exhausted. No new policy ceiling is introduced.

## Executed targeted checks

Commands used the private worktree `.env` without printing credentials and
isolated SQLx databases. The actual `crm_app` database role performed operations.

- `SQLX_OFFLINE=true cargo check -p crm-api --lib`: passed.
- `DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true cargo test -p crm-api
  --test all db_core_change_reports -- --ignored --test-threads=2`: final source,
  **5 passed, 11.87 seconds**.
- Opt-in `retain_core_change_ui_fixture` under `--features perf-harness`: passed,
  **6.54 seconds**. It ran the actual collector twice and completed the real 010c
  parent before producing cancelled/completed reports in the private
  `crm_migration_010e1` database. It wrote the local synthetic login/IDs/key to
  `/private/tmp/crm-core-change-ui.json`; that credential-bearing fixture file is
  intentionally outside Git. It is not a real API/browser walkthrough by itself.
- Opt-in `independent_process_report_recovery`, executed from the just-built
  `all-4f54aadf6519a0de` test artifact: **passed, 10.84 seconds**. Four separate
  worker processes committed advancing units; an expired previous worker could
  neither commit nor pause the newer work. The report then completed without
  any source reader calls. [Process output](../design/qa/slice-010e1-2026-09-12/report-process-handoff.log).

| Required behavior | Actual proof |
|---|---|
| Lossless full evidence and exact row counts | Real dual-capture test: 4 People IDs (one unchanged, changed, newly observed, not seen again), 1 changed task, 1 unresolved note, unchanged user/stage: **8 entities, 16 observations**. Unknown-field change maps to the closed `other_properties` category. Existing lossless parser tests cover duplicate decoded keys, large numbers, invalid IDs and structure bounds. |
| Stable publication and paging | Every in-progress `/rows` request returns conflict; partial summaries remain null. Completed two-row pages enumerate all 8 IDs at one output revision. Filter-bound cursor reuse fails; row detail equals the corresponding page row. A direct attempted update after sealing is rejected. |
| No plaintext content routes | HTTP receipts/rows are constructed from closed DTOs; tests reject retained-body and arbitrary-property sentinels. Note-list content is never substituted for inaccessible enriched detail. |
| Idempotency and storage ownership | Lost-response prepare/control replays match stored receipts; changed request bodies conflict; same frozen tuple reuses its report. Independent explicit byte inventory matches report counters and the newer snapshot's exact retained-byte increase. |
| Corruption and exhausted storage | Corrupt ciphertext pauses the actual runtime role with `retained_integrity_failed`. Repairing only the synthetic fixture allows resume. Clamped storage pauses; cancellation succeeds using its own reserve and preserves rows. A new explicit request creates a new job. |
| Scope and ambiguous source evidence | Repeated equal/conflicting variants report one exact repeat and one conflicting group. A different retained source user produces a scope warning. Incomplete newer enumeration yields unresolved People rather than absence claims. |
| Tenant/account/current actor | Member access fails; a current admin of another Org receives not found. Wrong source account and same-snapshot inputs are rejected. Oversized identity history is rejected using metadata bounds before loading ciphertext. Revoked initiating admin pauses execution; another current admin can read/cancel but cannot resume it. |
| Fenced workers and expired final seal | Stale held claim cannot alter progress/state/bytes after takeover. A synthetic database trigger expires the lease during the final summary write; publication fails, rolls back, and remains unavailable. Removing the fixture fault and resuming completes successfully. Separate-process recovery is also recorded above. |
| No CRM/parent/workspace mutation | The dual-capture and failure-path tests compare hashes of every canonical table in `imports::EMPTY_TABLES`, the entire parent import row, and workspace mode/revision before/after report work. They remain identical. |

## Bounded review and corrections

Coordinator review round one identified six concrete issues. All were corrected
within this scope and targeted tests passed:

1. Pause bookkeeping needed `UPDATE(byte_count)` on its own reservation table.
   The initial real-role test reproduced PostgreSQL `42501`; the narrow grant and
   final corruption/storage/cancellation test pass.
2. A stale failure could otherwise pause a later expired lease. Persisted epochs
   distinguish a failed fresh admission from an owned in-flight failure; stale
   token/epoch tests leave the newer report unchanged.
3. Final publication needed an atomic unexpired-lease predicate. It now checks
   state/token/epoch/expiry, with bounded statement and unit execution. The
   during-final-write expiry fixture proves rollback and recovery.
4. Identity qualification now inspects aggregate metadata before ciphertext,
   admitting at most 100 responses/16 MiB and only the frozen final sequence.
5. Evidence references always remain explicitly representative; the response
   never claims an exhaustive evidence list, including small entities.
6. The frozen schema/retention inventory includes the per-variant table.

A source test expected an unknown-property-only change while its synthetic input
also added an assignee; the fixture was corrected to isolate the intended change.
An actual member-route denial exposed an upstream missing `no-store` prefix;
the coordinator corrected the shared workspace middleware and the final HTTP
assertion passed. Formatting and one `filter().next()` lint correction were also
resolved. No additional broad review was launched.

## One D-050 plan pass

[SQL](../design/qa/slice-010e1-2026-09-12/plans.sql) and
[actual plans](../design/qa/slice-010e1-2026-09-12/plans.log) cover 25,000 People,
25,000 notes and 50,000 tasks: 100,000 entity groups, 200,000 variants,
125,000 observations and 26,000 capture pages. The pass used session-local copies
of the actual table/index definitions and representative encrypted row widths;
no application rows were changed. Copied ciphertext is explicitly a SQL plan
fixture, not newly qualified comparison evidence. The transaction rolled back.

All eight hot statements used the intended indexes: capture sequence, per-capture
ordinal, exact source group, variant conflict key, note request key, comparison
keyset, filtered published page and unfiltered published page. Returned work was
bounded to 1 capture, 100 observations, or 50/51 rows. The only sort covered the
100 records of one source page (38 KiB). There is no repeated raw-page/group scan.
Displayed laptop times are observations, not production capacity guarantees.
This additive feature has no prior endpoint behavior to pair against; unchanged
client/command contracts are covered by the full regression gates.

## Existing core collector handoff concern

Independent reproduction used five distinct OS processes against the actual
collector and current migrations. Successful identity checkpoints advanced through
0–4 with distinct request fingerprints; the alleged duplicate-checkpoint failure
**did not reproduce**. A fresh process requalified identity first; a second claim
in the same process advanced to users, and a later process drained to completed
at capture sequence 12. No existing snapshot worker change was made.
[Evidence](../design/qa/slice-010e1-2026-09-12/core-snapshot-process-handoff.jsonl).
This does not claim one-unit fresh-process invocations skip identity qualification.

## Retention, compatibility and remaining gates

The additive inventory comprises `migration_core_change_report`, `group`,
`variant`, `note_key`, `receipt` and `reservation` tables (all with the
`migration_core_change_` prefix). The report has tenant-composite references to
**both** retained snapshots and the original People import. Erasure/retention must
include those dependencies and all encrypted groups/receipts/references even
though byte ownership belongs to the newer snapshot. No erasure workflow,
retention policy or customer-data approval is invented by this slice.

Coordinator-owned observed-artifact/schema/engine registration advertises
`fub-core-change-v1`; original Web/Operator/mobile command contracts stay unchanged.
The coordinator reports 34 preflight tests and the full Web gate (1,181 tests in
86 files, lint/typecheck/build) passed. Full `./scripts/check` passed in 187 seconds
before the final frozen-identity upper-bound addition, followed by the final
five focused source/DB/API tests above. The coordinator's production API rebuild
covers that final addition.

The broad DB run had 401 passing tests before two process-spawn failures caused
by a concurrent Cargo rebuild replacing its test executable, not failed product
assertions. The coordinator is continuing that gate from an immutable archive;
its final result is tracked in the combined verification record. The actual
desktop/narrow-browser walkthrough subsequently passed as recorded below. A release, live FUB qualification, customer readiness,
repair/deletion/activation and physical-device behavior are not claimed here.

## Coordinator browser and integration proof

The actual production-Web/real-API walkthrough passed cancelled-report handling,
accepted-request/lost-response retry with identical request identity, filtered
absence and inaccessible-note review, representative evidence navigation and
reload recovery. Desktop and 390px screenshots were inspected; no horizontal
overflow or browser page errors occurred. See the [sanitized evidence and exact
build attribution](../design/qa/slice-010e1-2026-09-12/README.md).

The coordinator locally integrated backend/Web at `d6e7c74`. The combined
service-free gate and live SQLx preparation subsequently passed; the complete
archived DB suite is tracked in [combined verification](MOBILE_MIGRATION_COMBINED_VERIFICATION.md).
This preserves separate source attribution for targeted worker tests, actual
browser runtime and merged-source checks. The same evidence directory records
an incidental shared-preview asset replacement and exact restoration of the
released Web files; later QA output is isolated.

The combined verification is now complete: repository checks (968 Rust and 1,181
Web tests plus supporting suites), live SQLx preparation and all 966 DB cases
have passing evidence. The full archived DB run passed964 cases; its two legacy
fixture assertions were corrected without runtime changes and passed their
focused rerun. The original failed invocation remains explicitly recorded.
The clean merged migration worktree/branch and owned QA listeners were removed.
