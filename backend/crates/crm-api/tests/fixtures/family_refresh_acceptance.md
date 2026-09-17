# 010g1 isolated acceptance

These opt-in fixtures use synthetic data, the real application router, typed
commands and workers. They never contact FUB. Keep their database commands serial
and use isolated Cargo/Vite output directories; do not replace shared runtime
artifacts.

1. With the development services running and the gitignored development
   environment loaded, set `DATABASE_URL` to an isolated migrator master database
   and `CRM_FAMILY_BROWSER_OUTPUT` to a new absolute private directory. Run the
   ignored `family_refresh_browser_fixture` test with `test-support` and one test
   thread. It serves the API on `127.0.0.1:3118` and writes `fixture.json`.
2. Build Web to an isolated output directory, then run Vite preview on port 5198
   with `CRM_WEB_API_PROXY_TARGET=http://127.0.0.1:3118`. Run
   `node backend/crates/crm-api/tests/fixtures/family_refresh_browser.mjs OUTPUT`.
   It uses the installed Chrome and the existing Web `playwright-core` dependency.
   The only response interception deliberately loses a **real committed** Confirm
   response; the subsequent request must have the same ID and body.
3. The workflow leaves workers paused. Generate the exact history SQL inventory
   with `cargo run -p crm-api --features test-support --example
   family_refresh_query_inventory` (from `backend`), saving stdout privately.
   Run `python3 backend/crates/crm-api/tests/fixtures/family_refresh_plans.py
   OUTPUT/fixture.json PLAN_OUTPUT QUERY_INVENTORY.json`.
4. Inspect the retained JSON plans and screenshots. A successful script exit is
   not by itself a plan-shape or visual acceptance verdict. The plan fixture uses
   25,000 People, 50 memberships and dense/sparse family/history rows. It suppresses
   row/FK triggers **only during inert clone setup**, restores normal trigger
   behavior for EXPLAIN, and rolls everything back. It is index/cardinality
   evidence, never mutation, authorization, encryption or migration-fidelity proof.
5. Create `OUTPUT/stop`, await the Rust test's exit, and stop the owned preview
   server before another database gate. A rerun of the whole Web sequence needs
   a fresh fixture and output directory. Keep failures in their original evidence
   directories.

The final functional database suite uses normal guards and foreign keys. The
separate D-050 paired benchmark uses `perf-harness`, the ignored
`operational_person_detail_matches_cd3b010` test,
`CRM_010F2_PERSON_PERF_OUTPUT` and **`CRM_MOBILE006_PAIRED_TODAY=1`**. Run that pair
only in its coordinator-owned final slot; it is not part of these fixtures.
