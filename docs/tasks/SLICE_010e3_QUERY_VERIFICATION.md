# Slice 010e3 query-plan verification

The perf-harness test `hot_plan_25k_people_has_sparse_pages_and_dense_shared_contacts` is a synthetic relational query-plan fixture. It does not claim retained-source qualification, a full admission-worker throughput result, or production capacity.

It creates 25,000 native People, 50 active members, and 25,000 native contacts with one shared synthetic email. The admission-side fixture has 25,000 People groups, 25,000 plan items with 50 eligible and 24,950 held items, 25,049 encrypted admission contacts including one 50-contact page, 25,000 exact baseline-record index rows, and 25,000 global-identity rows. Retained core-group rows are inserted in 250-row committed batches because their retained-byte trigger updates three ledgers per inserted row.

The test extracts the SQL strings from the production worker, source, and query modules before applying `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)`. It verifies these bounds:

- preparation keyset uses `migration_core_change_group_admission_people_keyset` and examines at most one group;
- global identity and original-presence lookups use their exact indexes and examine at most one row;
- sparse eligible item paging uses `migration_people_admission_item_page` and examines at most 51 rows;
- the normal all-items page uses an indexed path (primary key or the plan-scoped all-items index) and examines at most 51 rows;
- both physical predicates used to merge a cancelled page are separately bounded to 51 item rows;
- the 50-contact page uses an indexed contact path and examines at most 50 rows; and
- the worker's sparse eligible claim uses `migration_people_admission_item_eligible_claim` and examines at most one item.

The first run exposed a real sparse-page regression: a display `CASE` predicate caused the item endpoint to scan all 25,000 rows and discard 24,950. The query now resolves cancellation before SQL and merges two bounded direct predicates. The migration also scopes item page indexes by plan.

Passed on 2026-09-13 with the current compiled `perf-harness` test binary, after the binary was rebuilt from the final assertion source:

```text
DATABASE_URL="$MIGRATION_DATABASE_URL" \
/private/tmp/crm-010e3-target/debug/deps/all-4f54aadf6519a0de \
hot_plan_25k_people_has_sparse_pages_and_dense_shared_contacts \
--ignored --nocapture --test-threads=1

1 passed; 0 failed
```

The final run log is `/private/tmp/crm-010e3-qa/hotplan-010e3-awake.log`.
