# Mobile 001 — Completed reconciliation lifecycle correction

2026-09-12. Native continuous-sync verification exposed an admission defect:
successful seals left every generation counted until its 30-minute expiry, so
two completed reconciliations exhausted one context's capacity. This correction
is within the D-074 lifecycle contract, with explicit coordinator assignment.

## Change and compatibility

`20260923000003_mobile_sealed_generation_cleanup.sql` adds nullable `sealed_at`
and a constraint requiring selection completeness when set. A successful seal
sets this marker atomically after consistent validation, with a final unexpired
row check. A retained repeated seal revalidates and preserves its original
timestamp. The marker stores no Today response or customer body.

Existing complete rows are not backfilled because complete selection does not
prove a successful seal. They retain their original expiry. Cleanup now considers
sealed or expired generations but still deletes at most two generations per unit
and counts every retained row until deletion. Unexpired pending generations,
contexts and operation receipts remain intact. Cleanup failure or a locked
terminal row never creates an admission exemption.

After cleanup reclaims a sealed generation, later reads/seals return the existing
404. Native staging recovery starts a new generation, reuses unchanged complete
bundles, and retains active cache and original outbox identities. Retained expiry
continues to return `generation_expired`. DTOs, operation semantics and receipt
retention are unchanged; the exact lifecycle is recorded in
[the frozen contract](MOBILE_001_CONTRACT.md). Native recovery/UI evidence is
owned and attributed by each platform separately.

## Source and executed checks

Base: `9483b2f99fd55fd67897b262b4087b46e17569e8`. Production SHA-256 values:

- `crm-app/src/domain/mobile/generations.rs`:
  `c5b4f2b3a8722b1c70a4bbfb41f2fdaf40f209561e545295d42902fc849ac085`
- Migration `20260923000003_mobile_sealed_generation_cleanup.sql`:
  `a0b5c4ee8e68575e5733a844f0f5e69d3a746fe08b4fe47aa55c235b87e94684`

The coordinator independently rechecked the marker placement, scoped unexpired
update, bounded cleanup and retained admission accounting with no additional
finding. No other runtime source was changed by this correction.

Executed from `backend/`, using the assigned private environment without printing
credentials and `DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo nextest run -p crm-api --test all --locked --run-ignored only \
  --no-fail-fast --test-threads 4 -E 'test(db_mobile::)'
```

Formatting: **PASS**. Clippy: **PASS**, 35.87 seconds. Mobile DB/router suite:
**10 passed**, zero failures, 32.305 seconds. This includes all eight existing
mobile DB tests and these two new tests:

- `sealed_generations_reclaim_and_recover_lost_seals_without_losing_receipts`:
  six successive create/seal cycles on each of two installations, stable complete
  bundle revisions, original seal replay, an actual successful router response
  dropped without reading its body, reclaimed-generation 404, new-generation
  recovery, identical receipt rows, one canonical note, retained contexts, and
  three terminal generations reclaimed in two bounded units while an unexpired
  pending generation survives.
- `sealed_generations_remain_counted_when_cleanup_is_locked_or_fails`:
  failed marker persistence leaves an unsealed generation; locked terminal rows
  still exhaust context admission; failed deletion preserves rows and denies new
  admission; successful later cleanup restores capacity.

Live SQLx verification: **PASS**, exit 0. The standard fresh `crm_sqlx_prepare_check`
database was owned only for this check, migrated with the complete set, checked by
`cargo sqlx prepare --check --workspace` with `SQLX_OFFLINE=false` and isolated
`backend/target/sqlx-prepare-check`, then dropped. The CLI reported a
potentially-unused-query advisory; committed SQLx metadata was not modified.
Exact script and logs are in
`/private/tmp/crm-mobile-seal-lifecycle/` (`format.log`, `clippy.log`,
`mobile-db.log`, `sqlx-check.sh`, `sqlx.log`).

The earlier [combined 966-test verification](MOBILE_MIGRATION_COMBINED_VERIFICATION.md)
belongs to its recorded prior source revision. It was not rerun or relabeled as
this new migration's full-suite evidence. This record attributes only the
affected current checks; native repeated-sync proof follows on the rebuilt
isolated API. No default root Web build or deployment was performed here.
