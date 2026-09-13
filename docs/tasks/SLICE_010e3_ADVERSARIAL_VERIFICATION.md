# Slice 010e3 adversarial verification

`db_people_admission_adversarial` is a compact real-PostgreSQL acceptance suite
for the D-078 negative paths that are not covered by the ordinary execution
fixture. Each case creates the genuine retained snapshot, completed 010c
parent, 010e1 report, admission preview, and where applicable confirmation. A
migrator connection then changes only the synthetic retained fixture or a
stored mapping, while production-path commands and the worker retain their
normal role and permit checks.

The suite proves four boundaries:

1. A retained People capture made unaccepted, an ordinal mismatch, a semantic
   HMAC mismatch, and an incomplete original People baseline each stop preview
   preparation with `source_integrity`. No result or native Person is emitted.
2. A 129-digit source ID makes report preparation fail closed before admission.
   A malformed non-string email value is held in an otherwise valid preview;
   the qualified control row is its sole eligible row.
3. A changed confirmation digest, a superseded plan, and an expired sealed plan
   cannot queue an admission, write a confirmation receipt, or create a native
   Person. Re-preview creates the only usable new plan.
4. Changing a stage mapping target after an exact plan has been confirmed makes
   the worker pause before any native Person or result write.

The focused command run after module registration was:

```sh
set -a; source /Users/karrad/projects/crm/.env; set +a
DATABASE_URL="$MIGRATION_DATABASE_URL" \
CARGO_TARGET_DIR=/private/tmp/crm-010e3-target \
cargo test -p crm-api --test all --locked db_people_admission_adversarial:: \
  -- --ignored --nocapture --test-threads=1
```

These are synthetic fault-injection tests; no production data was changed.

## Result

The focused command completed on 2026-09-13 against the final migration worktree
and isolated migrator database:

```text
running 4 tests
... corrupt_retained_capture_ordinal_hmac_or_original_baseline_fails_closed ... ok
... huge_ids_and_unsupported_contact_shapes_are_held_before_confirmation ... ok
... invalidated_stage_mapping_at_execution_pauses_without_native_write ... ok
... stale_confirmation_and_expired_preview_require_a_fresh_exact_plan ... ok

4 passed; 0 failed; 0 ignored; 1115 filtered out; finished in 52.27s
```

The oversized-ID test observed the intended earlier fail-closed boundary:
`core_change_reports::prepare` returned `SourceNotEligible`, so no admission row
was possible. The malformed contact test reached the real admission plan and
was held with the qualified control as its only eligible row. No production bug
or schema change was found.
