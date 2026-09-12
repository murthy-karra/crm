# Slice 010d1 backend evidence

This package records the backend handoff after HC-R1-01/02/04 and
HC-R2-01/02. It is separate from the coordinator's final repository gates,
release preflight and integrated Web/runtime verification. History capture left
the prepared native baseline unchanged; synthetic fixture setup used existing
native commands and rows as documented. No live Follow Up Boss requests,
deployment, commit or merge occurred in this lane.

## Final results

| Evidence | Actual result |
|---|---|
| [Source parser](final/history-source-tests.log) | 11 tests passed; parser source unchanged afterward |
| [Production transport excerpt](final/history-transport-excerpt.log) | PASS in the coordinator's completed final gate; exact result line preserved |
| [Focused DB batch and populated collector](final/history-db-final-with-plans.log) | 26 tests passed in 326.32s: 25 focused tests plus the collector |
| [Incomplete-parent lifecycle](final/history-incomplete-parent.log) | 1 additional focused test passed in 2.98s; actual proposed and queued parents rejected, same completed parent accepted |
| [Populated additive upgrade](final/history-additive-upgrade.log) | 1 test passed in 3.85s |
| [Formatting](final/history-fmt-final.log) | Passed; successful command emitted no output |
| [Standard all-target lint](final/history-clippy-final.log) | Passed with warnings denied, 39.27s |
| [Opt-in test-target lint](final/history-clippy-optin-final.log) | Passed with warnings denied, 30.65s |
| [QA example build](final/history-qa-build-final.log) | Passed, 6.82s; executable was not started by this lane |

There are **26 focused history DB tests**, one populated collector and one
additive-upgrade test across the final records. The incomplete-parent test was
added after the combined batch and passed separately; it changed no production
code or measured SQL. The combined command now also selects that added test.

The focused cases cover current admin/tenant/actor boundaries, exact receipt
replay, incomplete parents, symmetric source admission, cancellation and late
responses, disconnect/demotion, identity/key/checkpoint corruption, strict
duplicate-key identity parsing, retries, overlap reconciliation, byte accounting,
sibling reservations, budget races, erasure references, cursor scopes and actual
artifact readiness. Independent worker sessions prove collection progress,
restart revalidation, and each worker's own initial readiness admission.

The older upgrade test keeps its original activity-only preservation assertion.
A temporary empty SELECT-only compatibility view supports current core helper
lookup during fixture construction; it is dropped and asserted absent before
freezing rows and applying the real migrations. Later current schema is applied
before testing startup of the current executable, with another unchanged-row
assertion. No production guard was weakened.

## Commands

Commands ran sequentially from `backend/`, with `SQLX_OFFLINE=true`. DB tests
used the explicitly provisioned disposable database via `DATABASE_URL`, never
the shared development database. Credential values are intentionally omitted.
The collector used `CRM_010D1_PLAN_OUTPUT` to write private JSON instead of
printing it into the log.

```sh
cargo test -p crm-app --lib history_capture_source::tests --locked
cargo test -p crm-api --test all --features perf-harness --locked db_history_capture -- --ignored --nocapture --test-threads=1
cargo test -p crm-api --test all --features perf-harness --locked history_proposal_rejects_real_proposed -- --ignored --nocapture --test-threads=1
cargo test -p crm-api --test all --features perf-harness --locked populated_010f1_upgrade -- --ignored --nocapture --test-threads=1
cargo fmt --all -- --check
cargo clippy -p crm-app -p crm-api --all-targets --locked -- -D warnings
cargo clippy -p crm-api --test all --features perf-harness --locked -- -D warnings
cargo build -p crm-api --example history_capture_qa --features test-support --locked
```

The production loopback transport result is extracted from line 566 of the
coordinator's [completed full-gate log](../gates/final-check-1.log). It is one
exact PASS line, not a reconstructed standalone run. The originating log's
SHA256 and extraction line are recorded in the evidence manifest. The earlier
standalone transport pass had no retained log and is not used as archive proof.

## Populated evidence and limits

[Full collector JSON](final/history-plans.json) and its
[summary](final/history-plans-summary.json) contain 14 actual SQL EXPLAIN plans,
bindings, scan work, reader measurements, byte reconciliation and source hashes.
The constructed fixture has 25,000 People and 50 members. The real worker retained
25,000 records per family through 750 collection pages and one identity request:
75,000 valid encrypted observations, traversed through 1,500 authorized pages.
One Person has 501 records per family. Fixed-sequence reads, variants, rare/empty
filters and source-free retained reads have positive controls.

The maximum measured response was 51,965 bytes and record 1,001 bytes. Native,
parent and sibling row hashes stayed unchanged; retained byte totals reconciled
with stored octet lengths. The plans show no sequential scan of the observation
relation or temporary spill. These are synthetic correctness observations, not
vendor completeness, operational capacity or latency guarantees.

Physical scan work is **not** bounded by page size: the rare excluded filter
scanned 25,000 indexed family entries, and an empty conflicting-reference filter
scanned 75,000 indexed entries with same-identity probes. SQL fetches at most 51
observations and decrypts at most 50. The result does not establish constant scan
work or qualify public-source pagination behavior.

All six production/schema SHA256 values embedded in the final collector matched
the files at packaging. The parser/schema qualification manifest is copied
unchanged as [source-schema-manifest.json](final/source-schema-manifest.json),
SHA256 `d20b212ecf0db3e6fbbf467b272501dd0796d9dc4af7e58fd50a075ae4a8841b`.
The [QA build identity](final/history-qa-build-artifact.json) records executable
SHA256 `fc82cf60568a2f1f1b39f9db30406ede4c3a39b02e36ec584ff5ac3c76b075b2`
at backend handoff; the binary itself is excluded.

## Interim attempts and sanitization

`interim/` preserves earlier passes, failed attempts and the superseded collector.
It is not evidence for the final artifact. The meaningful failures were helper
argument-count lints, five release-test auto-deref lints, and the old-schema
fixture's missing admission relation; all were corrected. The broad
`--all-targets --features perf-harness` attempt also hit inherited unused Today
HTTP performance fixtures, which were not changed. The standard all-target and
explicit opt-in history target checks both passed. Earlier QA compile errors
were fixed before a retained successful build; no standalone failed-build log
exists in this package.

[evidence-manifest.json](evidence-manifest.json) records original and archived
file hashes, classification and byte counts. Apart from the explicitly labeled
transport excerpt, only absolute machine/private directory prefixes were
replaced with `<WORKTREE>` and `<PRIVATE_QA>`; source,
schema, executable and query identity hashes remain unchanged. Exact,
URL-encoded, JSON-escaped and base64 comparisons against ten private `qa.env`
credential values found no matches before copying. The archive also excludes
environment files, credentials, keys, cookies, raw source bodies, executable
binaries, runtime state and coordinator-owned gate/preflight files. Collector
bindings contain synthetic local IDs or purpose HMACs; baseline evidence is row
counts and hashes, not row payloads.

The inherited core snapshot worker's UUID-based ownership behavior is an
unproven follow-up, not a fixed or reproduced result of this slice.
