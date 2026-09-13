# Slice 010e3 verification evidence

## Isolated PostgreSQL lifecycle

`db_people_admission_execution::retained_new_people_admit_with_native_identity_provenance_and_facts` creates a real synthetic 010c review parent, captures a newer People stream, builds a real 010e1 report, then uses the admission command and worker. The fixture includes two original imported People, one original held Person, and one new Person with an email and phone.

The test verifies the sealed preview has exactly one eligible new source ID; exact confirmation settles it into one new native Person with two contacts, one global admission-origin identity, encrypted admission provenance, and one `person_admitted` fact. It also verifies the admitted Person has no Inquiry. The worker is driven through all four retained-source qualification phases before it builds the plan, so it does not fabricate a plan or result.

Passed 2026-09-13 against the disposable synthetic migrator database:

```text
DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true \
CARGO_TARGET_DIR=/private/tmp/crm-010e3-target \
cargo test -p crm-api --test all \
retained_new_people_admit_with_native_identity_provenance_and_facts \
-- --ignored --nocapture
```

This run exposed and corrected live errors in the original snapshot-boundary join, admission ledger numeric cast/release bind, planned contact permit binding, and original mapping revalidation.

The expanded focused suite passed on 2026-09-13 with five real PostgreSQL tests. It covers retained-source qualification and native settlement, original/identity/mapping holds, same-report cancellation and remainder preparation, a lowered live policy that pauses before any native write while preserving the cancel reserve, expired-lease fencing, and initiator-demotion fencing. The settlement test compares the live retained-byte ledger with the independent persisted-byte audit.

```text
DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true \
CARGO_TARGET_DIR=/private/tmp/crm-010e3-target \
cargo test -p crm-api --test all 'db_people_admission_execution::' \
-- --ignored --nocapture

5 passed; 0 failed
```
