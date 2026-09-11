# Slice 010a — Bounded implementation brief

**Delivery: implemented, synthetically verified, merged, pushed and deployed
to shared development (2026-09-11). Live FUB validation remains user-deferred.**
See [verification](../tasks/SLICE_010a_VERIFICATION.md) and
[release](SLICE_010a_RELEASE.md).

**Status: APPROVED for implementation, 2026-09-10 (D-060).**
Specification: [SLICE_010a.md](../specs/SLICE_010a.md).
Base inspected: local main `b2fb368`, after 019b. Original implementation checkout:
`/Users/karrad/projects/crm-worktrees/010a`, branch
`codex/slice-010a-fub-assessment`. The user's baseline model split is Astra for
planning/review and Terra for implementation. After repeated incomplete repairs
of request-receipt idempotency, backend correction ownership moved to Astra
high under MODEL_ROUTING's escalation rule. Terra then completed Web; both
review/fix rounds and final synthetic verification are complete. The coordinator
owns documentation and source integration. On 2026-09-11 the user authorized
cleanup, commit, merge and push (D-060 follow-up), then separately authorized
the completed shared-development deployment.

## 1. Entry conditions

Read AGENTS, DECISION_LOG (D-012–016, D-050, D-059, O-012/O-013), the architecture
baseline, migration summary, this specification and the current implementation
prompt. The user approved the API-first connection, persistent encrypted
credentials and the additive contracts in spec §4; D-060 records that approval.
No repeated human approval is required for these in-scope implementation steps.

The user explicitly deferred live FUB validation while no test account is
available. Complete all implementation and synthetic verification; carry live
validation as pending. Published contracts have been qualified in
[the source record](../research/SLICE_010a_FUB_SOURCE_CONTRACT.md).

Source qualification must pin the official identity/collection schemas and
fixture profile. Live-account authorization/access qualification and synthetic
data availability must be recorded before live calls. No credentials go in
chat, Git, fixture files, screenshots or command arguments. Do not create or
seed a FUB account, send a vendor message, or read a real customer book merely
because the implementation uses synthetic fixtures.

## 2. Implementation sequence

The completed implementation used sequential backend and Web ownership.
Implementation branch: `codex/slice-010a-fub-assessment`. Use one isolated
checkout if active main/runtime work would otherwise conflict. No parallel
writers or new infrastructure are necessary.

1. Pin the source contract and synthetic request/response fixtures; close any
   response-shape gap before writing production parsing. Implement the typed
   GET-only reader and failure classification with the fixed probe profile.
2. Add scoped connection/assessment/check/evidence persistence, crypto wrappers,
   commands, idempotency and worker transitions. This writer exclusively owns
   the single additive DB migration; choose its version from the actual head.
3. Add the admin HTTP adapters and bounded polling projection. Prove tenant
   isolation, secret handling and cancellation/retry before connecting Web.
   Perform review/fix round one on backend/source/contract behavior.
4. Implement Manage → Migration using current UI/query/session conventions.
   Add focused component tests and the synthetic walkthrough. Round two covers
   Web/integration and verification of first-round fixes.
5. Run the final-tree gates and query plans once, complete the authorized live
   validation if access is available, and record remaining unverified evidence.
   The initial planning request did not authorize source integration or
   deployment. The 2026-09-11 follow-up authorizes cleanup, commit, merge and
   push. The later deployment follow-up is recorded in SLICE_010a_RELEASE.md.

## 3. Ownership and existing seams

Approved implementation surfaces:

- New `backend/crates/crm-app/src/domain/migration/**`; module registration,
  scoped ID types and only necessary typed crypto/config extensions.
- New `backend/crates/crm-api/src/routes/migrations.rs`; route registration,
  state/worker startup and closed error mapping. Reuse `OrgAdminContext` and
  trusted `CommandContext`; no privileged alternate command path.
- One new `backend/crates/crm-api/migrations/**` file, offline SQLx metadata,
  focused migration DB/HTTP/worker tests and their test-binary registration.
- Web migration view/components/tests, API types/queries and minimal router/
  navigation integration. Read the current UI style guide before editing UI.
- Names-only/non-secret-default configuration additions if required; no
  dependency, service or secrets-manager addition is implied.

Reference implementations inspected during planning:

- `crm-app/src/domain/raw_payload/crypto.rs`: XChaCha20-Poly1305, typed wrappers
  and Organization/row authenticated-data binding. Existing byte formats stay
  unchanged; migration uses distinct purposes and IDs.
- `crm-app/src/domain/intake/extraction/worker.rs`: short `SKIP LOCKED` claim
  transaction, lease and external call outside it. Migration adds its own
  connection-revision/cancellation fencing; do not share intake payload rows.
- `crm-api/src/routes/custom_fields.rs`: strict decoding and admin extractor
  conventions; `web/src/router.ts`: `requiresOrgAdmin` manage routes.

Unowned: Person matching, business-table mutations, Today ranking/filters,
Operator tools, email/telephony, full snapshot storage, import/delta/cutover,
credential production key hierarchy and unrelated `notes.txt`.

## 4. Required evidence

Map spec §7 to unit, DB, HTTP and browser evidence. Use synthetic fixtures for
all failure tests; explicitly distinguish them from authorized live results.
Run `./scripts/sqlx-prepare`, `./scripts/check`, `./scripts/check-db` and inspect
the new lookup/claim plans as specified. No 019b performance rerun; no claim of
full-source coverage from six bounded checks. Do not reset or bootstrap the
shared development database for migration testing.

Deliver exact changed files, contract/schema inventory, test commands/outcomes,
screenshots when useful, access limitations and live-validation status in the
verification record. Ongoing approvals already granted by the user persist;
the checkpoint between backend and Web is not another human permission gate.
