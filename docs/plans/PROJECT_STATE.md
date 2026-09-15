# Project state

Updated: 2026-09-14 (Mobile005 / 010f3 implemented and independently READY under D-082; prior release unchanged).
Current status and live residuals only. [History](PROJECT_HISTORY.md#archived-project-state-snapshot--2026-09-13)
preserves the previous state verbatim, including superseded instructions.
Decisions remain authoritative; use the [reading index](../decisions/DECISION_INDEX.md).

## Current state

**Mobile004 / 010e4 are implemented, verified, published and deployed to shared
development under D-080 and its release follow-up.** Runtime backend source:
`ffbc9fd69871bb6a3dece88dfa008228dc30cb47`; unchanged Web source `26657c8`. The
[release record](../tasks/MOBILE_004_010e4_RELEASE.md) owns artifact/schema identity,
backup/recovery paths, preservation, compatibility and HTTP/mobile/browser proof.
Dated evidence does not prove present service health; recheck before operations.

The native demo API3101 and installed stores are preserved. Native distribution,
physical-phone/cellular testing, live FUB/customer work, workspace activation and
production-cluster deployment remain separate. Shared development is Mac-hosted.

## Current slice

**Implemented and verified locally: Mobile005 / 010f3.** Offline Person name/contact
editing on iOS/Android and admitted-People tags/custom-field imports are complete
under D-082. Both independent implementation reviews are READY.
The [coordinated plan](MOBILE_005_010f3_PARALLEL_LAUNCH.md) links both specifications,
execution briefs and review evidence. [Implementation status](../tasks/MOBILE_005_010f3_IMPLEMENTATION_STATUS.md)
owns delivered behavior, retained resources and passing checks. Final tested code
is `ab4a362`; later completion changes are documentation only. Publication and
deployment remain separate. Shared development is preserved.

**Mobile004 / 010e4 delivered within their approved synthetic scope.**
[Mobile stage specification](../specs/MOBILE_004_OFFLINE_STAGE_CHANGES.md),
[admitted-core refresh specification](../specs/SLICE_010e4.md) and
[coordinated plan](MOBILE_004_010e4_PARALLEL_LAUNCH.md) are complete. The
[implementation record](../tasks/MOBILE_004_010e4_IMPLEMENTATION_STATUS.md) owns
acceptance and final gates; the [release record](../tasks/MOBILE_004_010e4_RELEASE.md)
owns deployed artifact and preservation proof.
Mobile 001–003 and migration assessment,
core capture, People, metadata, notes/tasks, historical capture/timeline, change
reporting, existing-People refresh and core-only new-Person admission are delivered
within their approved synthetic scopes. See [prior implementation evidence](../tasks/MOBILE_003_010e3_IMPLEMENTATION_STATUS.md).
Imported workspaces retain the administrator review hold; ordinary mutations,
Today, Operator, outbound and mobile access do not bypass it.

010e4 adds subsequent core refresh for admitted People in shared development.
010f3 adds tags/custom-field imports locally; notes/tasks and history remain
sequential follow-ups in the [family ladder](SLICE_010_ADMITTED_PEOPLE_LADDER.md).
Full migration fidelity, mapping repair, remaining deltas and activation need
separately specified scope. Older ladder/summary milestone banners may predate
this release; use the latest release record for delivered status.

## Current branch

`codex/mobile005-010f3-integration`, final tested code `ab4a362`, followed by
completion documentation. The three completed writer worktrees and owned QA
APIs/Web preview are retained; no active writer remains. Keep verification and
release recovery material, QA databases and native demo/QA stores. Only inactive
private compiler caches were removed when needed. Shared runtime remains `ffbc9fd`.

## Last accepted decision

**D-082:** implement both reviewed plans and isolated synthetic verification,
with Terra high writers and the coordinated three-worktree sequence. Local
integration checkpoints are allowed. Implementation and required verification are
complete; publication/deployment remains separate.

**D-081:** draft Mobile005 and 010f3 together, preserving parallel mobile/migration
development; 2026-09-14 follow-up authorized independent review, now READY.
D-082 subsequently accepted the reviewed contracts and implementation. D-081's
earlier planning-only boundary is superseded for this accepted implementation.

**D-080 plus its release follow-up:** accepted both implementations, then authorized
commit/push, cleanup and shared-development deployment. Those actions are complete.
Live customer/source work, activation and native distribution remain separate.

**D-079:** plan Mobile004 and 010e4; user selected smaller sequential migration
slices and kept calling afterwards. Proposed implementation contracts are not
accepted by planning authorization.

**D-078 plus its 2026-09-13 release follow-up:** accepted Mobile 003's reported
contact occurrence time and 010e3's core-only admission contracts, then authorized
the completed main merge, push and shared-development release. Full text and
prior approvals: [decision index](../decisions/DECISION_INDEX.md#accepted-decision-locations).
No repeat approval is pending for those completed actions.

## Operational entry points

| Need | Entry point / limit |
|---|---|
| Development | [README](../../README.md#development); Docker restart does not restart the API; dev-bootstrap wipes data |
| Boundaries | [Architecture map](../architecture/ARCHITECTURE_BASELINE.md); planned production differs from development |
| Release/recovery | [Current release](../tasks/MOBILE_004_010e4_RELEASE.md), [compatibility runbook](../tasks/SLICE_010c_RELEASE_PREPARATION.md), [release prompt](../prompts/07-deploy.md) |
| Real data/production | [Readiness](PRODUCTION_READINESS.md); its historical release inventory predates the current release |
| Prior checkpoints | [History/ledger](PROJECT_HISTORY.md#slice-ledger); detailed proof stays in per-slice records |

Import/refresh/admission confirmation needs fresh actual-workload inventory and
`CRM_MIGRATION_RELEASE_REPORT`. The last recorded report expired at
`2026-09-13T19:45:18Z`; no automatic renewal exists. Ordinary CRM/retained reads
remain available after expiry. Registered FUB system configuration was unset at
release; no live source operation or customer activation was performed.

## Parked / queued tracks

- **Mobile design/system information and physical-phone/cellular testing:**
  explicitly user-deferred. Preserve essential saved-work/access/sync feedback.
  [Device follow-up](../tasks/MOBILE_001_PHYSICAL_DEVICE_FOLLOWUP.md) owns prerequisites.
- **Live FUB validation/customer processing:** user-deferred; dataset and V/C
  readiness gates apply before source operations. Synthetic proof is not live qualification.
- **Remote gates:** deferred; local gates were about three minutes at the recorded
  checkpoint. Preserve the prior vendor/runner survey in history; recheck before selection.
- **Deferred walkthroughs:** Slice 017 live sends; 009 steps 3–5 (reply-all,
  retroactive forwards, rotation). A held capture row was deliberately retained
  for the user's dismiss-path exercise. Earlier 011a filter walkthrough is complete.

## Live residuals and follow-ups

- **Customer readiness:** erasure runbook, key/retention/deletion policy, synthetic
  restore proof and responsible owners remain open. [Readiness](PRODUCTION_READINESS.md)
  also tracks core snapshot multi-worker handoff and SQLx cancellation notices;
  neither has a proven benign explanation or completed correction.
- **Telephony:** Telnyx SIP password/trunk rotation remains a user action;
  busy/ring-out live proof, placing-sweep versus slow microphone prompts and
  deactivated-call-owner handling remain open. Host: `livekit1.tarams.org` (D-055).
  Egress is dormant. Check instructions before live telephony operations.
- **Call-history ordering:** the 2026-09-12 audit reproduced timeline placement
  failure around `call_completed`, including in isolation; later complete run
  passed. Host/DB clock comparison is a supported mechanism, not proven ties.
  [Audit](../tasks/SLICE_010_COMPLETION_AUDIT_2026-09-12.md) preserves limits and
  the separate sleep/wake-correlated database warning episode.
- **Known product edges:** 007h1 plain-text forwarded signatures remain accepted;
  HTML gmail_quote separation is later. Manual explicit assignment lacks a
  membership-status filter (D-027/O-004); `set_local_password` and the actual
  `crm-admin` binary lack recorded test coverage. Revisit early-slice cookie,
  bind, request-ID, configuration and error-output minors when affected code changes.
- **Tooling:** fourth 501-INSERT fixture remains unbatched; killed runs may leave
  SQLx test databases; doctests previously dominated service-free checks. The lld
  experiment was reverted; retry only for a new toolchain reason. Never overlap
  DB-backed runs against shared test resources.
- **Environment:** dev ingress lives in the Cloudflare dashboard (D-025).
  Verify process identity and additive schema before release; old API processes
  can survive Docker restarts. Temporary backup artifacts are not a durable backup policy.
- **Standing review discipline:** verify reported changed files against actual
  tracked/untracked diffs. Prior unexplained document additions were retained by
  user choice; preserve attribution rather than guessing authorship.
- **Minor evidence pin:** 011a M7 positive `filter_kinds` span pin remains skipped;
  URL/filter regressions were resolved, and the unconstructible 20-clause ceiling
  was closed. Full residual wording remains in the archived snapshot.

## Backlog (deferred product tracks — full notes in the decision log)

Email sending/transactional/mailbox reconstruction (O-014/O-006), per-Person
crypto-shred and erasure (O-012/O-013), bulk-content storage/retention (D-062/O-015),
AI next-step suggestions (O-008), SMS consent (O-006) and recording consent (O-002)
retain their owning decisions. D-056's size cap is already implemented; raw MIME
still uses encrypted PostgreSQL storage. No new policy is chosen by this list.
Foundations F-01/F-02/F-03 remain proposals except where separately accepted.

## Latest verification

[Mobile005 / 010f3 final evidence](../tasks/MOBILE_005_010f3_FINAL_VERIFICATION.md)
records all required gates passing at `ab4a362`: 992 ordinary Rust, 1,076 database,
1,264 Web, 51 preflight, five documentation and 11 email-worker tests. Native proof
includes 47 iOS and 42 Android tests, installed upgrades and real simulator/emulator
journeys. Readiness covers 184 incomplete-schema variants. Browser, fidelity,
performance and preservation checks pass; both final independent reviews are READY.
All 74 protected artifacts and four shared listeners are preserved. Failed attempts
and corrections remain in the owning records. No physical-phone or release claim
is made for this local implementation.

[Mobile004 / 010e4 evidence](../tasks/MOBILE_004_010e4_IMPLEMENTATION_STATUS.md)
records installed native store upgrades, offline/replay/conflict flows, desktop/
390px refresh cancellation/remainder, exact source/byte preservation, both 25k
performance gates and all final checks: 989 ordinary Rust, 1,034 DB, 1,238 Web,
48 preflight, five documentation and 11 email-worker tests. Native acceptance is
simulator/emulator only. Failed attempts and corrections remain in the owning
verification records. The [current release](../tasks/MOBILE_004_010e4_RELEASE.md)
adds exact 151→161 rowset preservation, 56 schema checksums, HTTP/browser/mobile
checks and 188s observation. Release fixed one missing no-store header on early
member denials; all nine affected refresh/workspace tests passed. The original
full-suite evidence was reused with its source attribution.

## Next recommended action

The [Mobile005 / 010f3 pair](MOBILE_005_010f3_PARALLEL_LAUNCH.md) is ready for
publication and shared-development release once requested. Preserve retained QA
and installed stores through that release. Subsequent planning covers admitted
notes/tasks, then history, in the [family ladder](SLICE_010_ADMITTED_PEOPLE_LADDER.md)
and the next mobile slice; native calling follows the agreed progression.
Physical-phone/design/live-source work remains deferred; complete C-gate
erasure/restore work before real customer data. No repeat release action is needed
for the completed Mobile004 / 010e4 milestone.

## Approval currently required

D-082 accepts Mobile005's input/revision/contact-operation/realtime and 010f3's
source/catalog-handover/lifetime contracts. No repeat implementation approval is
needed; materially different scope/authority/policy requires its own decision.
Publication/deployment and native distribution remain separate.

D-080 implementation, publication, cleanup and shared-development release are
complete; no repeat approval is pending. Completed D-078 release also needs no
repeated approval.
New shared contracts, native distribution, live FUB/customer processing, activation
and production deployment retain their own scope and gates. Recovery targets,
retention/erasure, support access, outbound/recording consent and agent-departure
policy remain open where the decision log says so.
