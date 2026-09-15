# Mobile007 / Slice 010d3 — D-050 performance evidence

**Verdict: PASS.** The final integrated runner passed both exact-query plan
checks and one complete same-build paired Person/Today comparison. The retained
artifacts satisfy the accepted D-050 development-machine gate for this slice.

## Gate and scope

The accepted D-050 envelope is one realistic 25,000-Person, 50-active-member
Organization; at most five simultaneous Today loads; and one same-build paired
Person/Today comparison. The paired current p95 must be no greater than baseline
p95 plus `max(25ms, 10%)`, with complete payload parity apart from fields declared
by the existing frozen protocol. Each new or changed hot statement is captured
once with `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` and must remain bounded and
indexed at the realistic cardinality.

These are development-machine regression and plan-shape checks. They do not
claim worker throughput, provider behavior, production-host latency or capacity.

## Exact-query harnesses

`mobile007_plans::mobile007_discovery_25k_people_50_members` creates the required
book with 50,000 contact methods and explains the exact exported production
discovery statement for a broad name match and an exact email match. The query's
26-row lookahead is enforced. Contact access must remain indexed: primary-contact
work is bounded for the name case, and exact-contact work is constrained to at
most linear work across the 25,000-Person Organization.

`db_admitted_history_performance::admitted_history_hot_queries_25k_people_50_members`
starts from the real qualified admitted-history fixture and completes it through
the typed API. It then adds inert relational volume with production evidence,
immutability, result-permit, retained-byte and owner triggers enabled. Every
synthetic manifest preserves the exact retained advancing capture, observation,
ordinal and family tuple accepted by the evidence guard. Synthetic HMACs and
People bindings supply cardinality only; no synthetic row is decrypted or run as
a successful import.

The H3 fixture contains 25,000 manifests and candidates plus a dense 24,950-row
immutable held-result prefix. The stored attempt cursor is at that settled tail,
so the exact apply query proves its remaining 50-row unit without walking the
prior prefix. The evidence also hashes the command source that carries the
predecessor's durable cursor into a remainder attempt. A current work reservation
remains for that next unit, and the
fixture asserts its exact reservation bytes against the root counter and the
Organization counter delta. It also requires a positive root retained-byte
charge from the production measurement triggers. The output records source and statement hashes, bindings,
complete JSON plans, scan summaries, row bounds and the charging snapshot.

The H3 plans cover worker claim; observation, classification and application
pages; the global identity lock; root/manifest/result/issue readers; and the
exact generated admitted Person-timeline candidate. Indexed assertions are
applied to the 25,000-row manifest/result relations. Small fixture-owned roots,
issues, observations, identities and facts retain their complete plans without
mislabeling a cheap small-relation sequential scan as a regression.

## Paired reader protocol

The existing accepted Person/Today harness is reused without changing its
frozen reader bodies, fixture, alternating order, samples, warmups, parity checks
or p95 rule. Both arms intentionally share the current HTTP/authentication stack.
The source manifest updates only the two shared workspace-helper hashes and says
that Slice 010d3 adds its independent admitted-history transaction stamp. This
is a reader-body comparison, not a historical authentication-stack comparison.

## Commands and retained evidence

The serialized runs used the opt-in test binary at
`/private/tmp/crm-mobile007-010d3-yyoxjx50/acceptance-final-runner` with SHA-256
`0aed8d566e3a4864e0df8f1f1076a75558a68949d8655b70eda762b95ef9222b`.
The Git base was `edac8c3975bd457c436171ce47075689cfcc14ef`; the generated
artifacts bind the exact uncommitted final sources by the hashes below.

The three opt-in invocations were equivalent to:

```text
CRM_MOBILE007_PLANS_OUTPUT=<absolute new file> acceptance-final-runner \
  mobile007_plans::mobile007_discovery_25k_people_50_members --ignored --exact
CRM_ADMITTED_HISTORY_PERF_OUTPUT=<absolute new file> acceptance-final-runner \
  db_admitted_history_performance::admitted_history_hot_queries_25k_people_50_members --ignored --exact
CRM_010F2_PERSON_PERF_OUTPUT=<absolute new directory> \
CRM_MOBILE006_PAIRED_TODAY=1 acceptance-final-runner \
  db_activity_person_detail_perf::operational_person_detail_matches_cd3b010 --ignored --exact
```

The source and protocol fingerprints were:

- Mobile007 discovery production SQL source:
  `fdc84bdd0fdfdeaa205ceec2ca371d1ef2d981898dbc666574ef0ad9d81d982b`;
  SQL: `a39ac4f9db038136603c2ff0352d53bc14143b1a6c9078b02b322c5e6b1196a3`.
- H3 commands: `a799dd2e7d0ef18bb826af6e3f2ed495fe6f1fb45997bcc3120318c4f632e514`;
  worker: `b1762eadd3b25edc405ab9891af7cd4a9070c1774db8a3fb7450564f5e0d6a09`;
  queries: `45724f0478f479c814ac8c3979ffe45b3240b31904ecaa1b428dbdc77f764d9d`;
  history reader: `e1226f5eb82d8e3b780ff5423e904ab6cb03b81e54eed539218cccfbe079d8b0`.
- Paired source manifest:
  `e533c0df6cf634b53258c021cb9a520b97d36b911fb67d3ba81497f3d36a975a`.
  Its three frozen reader bodies remain byte-exact to `cd3b010`; the only
  shared-helper revisions are the two declared 010d3 transaction stamps.
- Harness sources: Mobile007
  `304aba9a3be9736c994bcd3fa8a1bc3b9eed19627f38621999581f9c250450e8`;
  H3 `1bde97ba2ee090fcf5643b0bf4f3124bb7ae0e5b5fab56fac42917da2e39b662`.

Retained final evidence:

- Mobile007 plan run: 18.36 seconds, 1 passed / 0 failed;
  [log](/private/tmp/crm-mobile007-010d3-yyoxjx50/logs/perf-mobile007-final.log)
  SHA-256 `321f116b1122b1b61fedce4db21b4ca55722a8fc9ebc0b70a60ad96ecc94a680`;
  [artifact](/private/tmp/crm-mobile007-010d3-yyoxjx50/performance-final/mobile007-plans.json)
  SHA-256 `1dce440f52fed264c11749fb614e7d2b4780e3ece5c28e1027914fef4fff18db`.
- H3 plan run: 40.03 seconds, 1 passed / 0 failed;
  [log](/private/tmp/crm-mobile007-010d3-yyoxjx50/logs/perf-admitted-history-final.log)
  SHA-256 `93e6eec7eb6c173a86db41387f43af76c9ea336312822ff10c3086a704e6ce28`;
  [artifact](/private/tmp/crm-mobile007-010d3-yyoxjx50/performance-final/admitted-history-plans.json)
  SHA-256 `30dc43a87617350da00c814a7b627d9afba27b09fa2c62125e368b8ed6548f55`.
- Complete paired run: 123.66 seconds, 1 passed / 0 failed;
  [log](/private/tmp/crm-mobile007-010d3-yyoxjx50/logs/perf-person-today-complete.log)
  SHA-256 `9ce864e4275655536abb77b64845993fd0ad1ac2e2c031f8c7aee373ef9bdb42`;
  [Person artifact](/private/tmp/crm-mobile007-010d3-yyoxjx50/performance-final/person-today-complete/person-detail-paired.json)
  SHA-256 `25e5de7d694604e3df61febbd06d63df0c86ffad6f09a402501cdd60ef4dcf55`;
  [Today artifact](/private/tmp/crm-mobile007-010d3-yyoxjx50/performance-final/person-today-complete/mobile006-today-paired.json)
  SHA-256 `ea8b94bb7c885d04ecc65dcf771d3b8ffb607b9bd0c82206717e9cd65c35f179`.

One earlier 135.80-second invocation passed its Person test but omitted
`CRM_MOBILE006_PAIRED_TODAY=1`, so it emitted no Today artifact and did not
complete the accepted paired protocol. It is retained as an incomplete attempt
at [log](/private/tmp/crm-mobile007-010d3-yyoxjx50/logs/perf-person-today-final.log)
and
[Person-only artifact](/private/tmp/crm-mobile007-010d3-yyoxjx50/performance-final/person-today-paired/person-detail-paired.json).
No result from that attempt is used for the verdict.

## Results

The Mobile007 fixture contained 25,000 People, 50 active members and 50,000
contact methods. Both discovery cases respected the 26-row lookahead and did
not spill. The name-fragment case examined the Organization's 25,000 People and
52 contact rows. The exact-email case examined the Organization's 25,000 People
and only three contact rows through `contact_method_lookup_idx`; returned
primary contact lookups used `contact_method_history_review_primary`.

The H3 fixture contained 25,000 People, 50 active members, 25,000 manifests and
a dense 24,950-result settled prefix. All ten exact production statements met
their row bounds. In the scaled relations:

- classification returned 50 manifests through
  `migration_admitted_history_manifest_page`, with one indexed candidate lookup
  per row;
- application began at cursor 24,950, returned the remaining 50 manifests
  through the same page index, and performed 50 indexed result-existence and
  candidate lookups without walking the settled prefix;
- the manifest reader returned the final 50 rows from cursor 24,950 through the
  manifest page index;
- the result reader returned 50 rows from cursor 24,900 through
  `migration_admitted_history_results_plan_page`;
- worker claim used `migration_admitted_history_one_active`; the other small
  fixture-owned root, observation, identity, issue and fact relations stayed
  within their declared 1- or 51-row result limits.

The scaled charge snapshot retained one exact 1,654,784-byte work reservation;
the root and Organization counters matched it, and production measurement
triggers recorded 21,846,076 retained bytes for the synthetic root.

The complete paired run used one concurrent request, five Person warmups per
arm, ten Today warmups per arm and 40 measured samples per arm in alternating
AB/BA order. Person responses had complete byte/JSON parity and verified both
entrypoints. Baseline p95 was **20.3765 ms**, current p95 was **22.8405 ms**, and
the limit was **45.3765 ms**. Today returned equal 200-item DTOs from the same
build. Baseline p95 was **71.537125 ms**, current p95 was **72.272875 ms**, and
the limit was **96.537125 ms**. Both latency checks passed, fixture row counts
were unchanged, and the paired protocol reported no unavailable or upper-bound
timing observations.
