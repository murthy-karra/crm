# Mobile 004 / 010e4 — Implementation status

**IN PROGRESS — 2026-09-13, D-080.** User accepted both specifications, shared
contracts and isolated implementation. No completion or release claim.

## Ownership and resources

Coordinator: integration branch `codex/mobile004-010e4-integration` from `44dcf52`
plus approved planning/acceptance documents. Independent planning review runs
read-only before the writers start. Backend/migration/iOS/Android writers use
Terra high. Shared changes and database gates are serialized by the coordinator.

Reserved schema versions: Mobile004 `20260929000001`; 010e4 `20260930000001`.
Proposed isolated APIs: mobile3102 and migration3103/Web5174, verified free at
launch preparation; isolated empty databases `crm_mobile_004` and `crm_010e4_qa`
were created and migrated through the published baseline. Mobile backend
`6992aa7` integrated at `04b5f35`; 010e4 checkpoint `d52c02a` integrated at
`ac7fa5a` for combined verification. These checkpoints are not completion.
Schema corrections `20260930000002`/`00003` preserve the applied first migration.
Native QA API3102 PID33892 uses a copied isolated binary; health/readiness pass.
Its database has synthetic People001–100 and all current additive migrations.
Private evidence root `/private/tmp/crm-mobile004-010e4/`; per-lane
Cargo targets under this directory, Web/native outputs isolated from shared dev.

Preserved runtime listeners observed: shared API PID93752/3000, Web PID93773/5173,
private native demo API PID71121/3101. Existing Mobile003/010e3 fixture evidence
and installed stores remain untouched.

## Gates

Independent planning review: Sol high, read-only against `c581e1e`, READY.
Required owner checkpoints are narrow catalog-revision maintenance during review
imports and exact OLD→NEW field validation for the admitted-refresh permit.
Both remain required implementation/tests, not unresolved product decisions.

Native iOS and Android implementations are integrated, including actual installed
Mobile003→004 encrypted-store upgrades and isolated real-API stage replay,
conflict and explicit replacement. Evidence is recorded in
[MOBILE_004_IOS_VERIFICATION.md](MOBILE_004_IOS_VERIFICATION.md),
[MOBILE_004_ANDROID_VERIFICATION.md](MOBILE_004_ANDROID_VERIFICATION.md) and
[MOBILE_004_BACKEND_VERIFICATION.md](MOBILE_004_BACKEND_VERIFICATION.md).
The mobile paired Today gate passed; its source attribution and contact-plan
fixture correction are recorded in the backend verification.

Mobile004 independent review used both allowed rounds. Final round found one
remaining Android reverse-follow-up composer defect: initially selected stage A
was not persisted while A→B remained unresolved. Commit `5a683d5` corrects that path; its actual emulator composer regression
passed in 36.858 seconds, preserving the baseline and single outbox envelope.
Both review findings and their targeted corrections are accounted for; no third
review is planned. Final combined repository gates remain.

010e4 first independent review identified lifecycle/read, closed settlement,
physical-byte accounting and source-availability gaps. Root integrated read,
re-preview/retry/cancel and exact eligible-count fixes, then truthful settled and
cancelled item filters. The migration owner is finishing retained-source,
cancelled-cohort, permit, atomic rollback and ledger regressions. A separate
bounded owner implements server-qualified selected-report availability and its
Web gating. Source population remains exactly the successful terminal admission
cohort; unreachable copied traversal of unrelated report groups is being removed.

API3103/Web5174 use isolated retained synthetic records. The first browser
prepare created a real ready plan, but exposed list/overview contract defects;
those source fixes are integrated. No browser confirmation/cancellation/remainder
acceptance is claimed yet. The same retained QA database will be reused after
integrating and rebuilding the corrected API/Web.

Combined focused evidence includes 18 mobile DB tests, 3 ordinary stage/tenant/
realtime regressions and 48 preflight tests. Migration lifecycle and valid-lease
negative tests passed at their recorded checkpoints; expanded regressions and
final-tree gates remain. One early migration test overlapped the mobile gate,
failed early in its disposable database and is not performance evidence.
Subsequent DB gates are serialized explicitly.

Remaining: integrated migration recovery,
source/permit/byte tests; desktop and 390px browser cancellation/remainder,
preservation and provenance evidence; one migration 25k hot-plan and paired
Person read pass; second/final migration review; final repository, SQLx and DB
gates. Physical phones/cellular, later family imports, calling, publication and
deployment remain separate.
