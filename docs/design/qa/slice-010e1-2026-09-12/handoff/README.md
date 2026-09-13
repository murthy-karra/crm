# Core snapshot process handoff: independent reproduction

The suspected duplicate accepted-checkpoint/fingerprint failure **did not reproduce**. Five independent OS worker processes used the real collector and current migrations with an isolated synthetic database. The successful run finished at capture sequence 12 with five distinct accepted identity fingerprints, then removed its database. No runtime source was changed.

| Actual process | Maximum claims | Observed result |
|---|---:|---|
| 28250 | 1 | Identity checkpoint 0; queued |
| 28251 | 1 | Identity checkpoint 1; queued |
| 28252 | 1 | Identity checkpoint 2; queued |
| 28253 | 2 | Identity checkpoint 3, then users; queued |
| 28254 | 20 | Identity checkpoint 4, then remaining streams; completed |

[Full safe output](evidence.jsonl) retains exact checkpoints, states and process IDs. Fresh one-claim processes repeatedly recheck identity; a second claim in one process advances collection. This check proves checkpoint uniqueness and recovery after handoff, not fairness during uninterrupted process churn. `snapshot_worker.rs` increments all accepted stream checkpoints, including identity, and includes the identity checkpoint in its request fingerprint. No runtime fix for the original allegation is justified.

## Repeating the synthetic check

`main.rs` is the helper used for the check, with only portable explicit environment/repository arguments and stricter owned-database cleanup added for retention. It contains a synthetic `FubReader`; it cannot issue FUB HTTP calls. It requires a loopback isolated migration base named `crm_migration_010e1`, matching application/migrator roles and an explicit private env file containing `DATABASE_URL` and `MIGRATION_DATABASE_URL`. Values are never printed. It creates and drops only its own `crm_handoff_repro_<pid>` database. On failure, identify and remove that single fixture database; do not reset shared services.

Run from the repository root:

```sh
python3 docs/design/qa/slice-010e1-2026-09-12/handoff/run.py /absolute/private/fixture.env
```

The retained runner uses the existing compiled dependency artifacts listed in `artifact-inputs.json`; supply `--target /path/to/matching/target` if needed. Those are the exact artifacts used in the successful check, including `crm-app` built with `test-support`. This is a retained reproduction helper, not a new normal test target or a dependency installation path. A later build that changes artifact hashes needs its matching dependency paths recorded before reuse.

Worker source SHA-256 at the run: `5763a06317d498d11edf1a6496ed742687c08d53dde5b81cf90699b881a5da4b`.

## Harness failures and limits

- The first compile selected incompatible cached SQLx/Tokio variants: duplicate SQLx types and missing Tokio main macro. Selecting the matching already-built artifacts fixed the harness; runtime source was unchanged.
- The first database run stopped while seeding the Organization because its current schema requires `intake_slug` and `intake_token`. The synthetic seed was corrected; that failed fixture database was removed before the successful run.
- The subsequent run compiled and exited 0; all assertions passed. The retained portable runner was then recompiled and rerun successfully: processes 31222/31247/31273/31290/31404 produced the same checkpoints/states and final sequence 12; its isolated database was removed. No live source, customer data, shared-service reset, browser flow or deployment was involved.
