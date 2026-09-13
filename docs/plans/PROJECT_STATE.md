# Project state

Updated: 2026-09-13 (documentation efficiency pass; no runtime changes).
Current status and live residuals only. [History](PROJECT_HISTORY.md#archived-project-state-snapshot--2026-09-13)
preserves the previous state verbatim, including superseded instructions.
Decisions remain authoritative; use the [reading index](../decisions/DECISION_INDEX.md).

## Current state

**Mobile 003 / 010e3 are implemented, verified, merged/pushed and deployed to
shared development under D-078 and its release follow-up.** Recorded runtime
source: `b37a480c5bc4cd5c711dceba287cc5bed70a71d0`. The
[release record](../tasks/MOBILE_003_010e3_RELEASE.md) owns artifact/schema identity,
backup/recovery paths, preservation, compatibility and HTTP/mobile/browser proof.
Dated evidence does not prove present service health; recheck before operations.

The native demo API3101 and installed stores are preserved. Native distribution,
physical-phone/cellular testing, live FUB/customer work, workspace activation and
production-cluster deployment remain separate. Shared development is Mac-hosted.

## Current slice

No active product implementation lane. Mobile 001–003 and migration assessment,
core capture, People, metadata, notes/tasks, historical capture/timeline, change
reporting, existing-People refresh and core-only new-Person admission are delivered
within their approved synthetic scopes. See [latest implementation evidence](../tasks/MOBILE_003_010e3_IMPLEMENTATION_STATUS.md).
Imported workspaces retain the administrator review hold; ordinary mutations,
Today, Operator, outbound and mobile access do not bypass it.

010e3-admitted People still lack dependent-family import and subsequent refresh
support; see [the explicit boundary](../specs/SLICE_010e3.md#1-outcome-and-deliberate-boundary).
Full migration fidelity, mapping repair, remaining deltas and activation need
separately specified scope. Older ladder/summary milestone banners may predate
this release; use the latest release record for delivered status.

## Current branch

`main`; at inspection, HEAD was documentation closeout `6e89776`, with no other
local worktrees or implementation branches remaining. This documentation pass
changes the working tree only. Verify Git state before a later task.
Keep Cargo targets and Web verification outputs separate from shared runtime
artifacts; preserve the private demo and owned recovery material.

## Last accepted decision

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
| Release/recovery | [Current release](../tasks/MOBILE_003_010e3_RELEASE.md), [compatibility runbook](../tasks/SLICE_010c_RELEASE_PREPARATION.md), [release prompt](../prompts/07-deploy.md) |
| Real data/production | [Readiness](PRODUCTION_READINESS.md); its historical release inventory predates the current release |
| Prior checkpoints | [History/ledger](PROJECT_HISTORY.md#slice-ledger); detailed proof stays in per-slice records |

Import/refresh/admission confirmation needs fresh actual-workload inventory and
`CRM_MIGRATION_RELEASE_REPORT`. The last recorded report expired at
`2026-09-13T14:43:44Z`; no automatic renewal exists. Ordinary CRM/retained reads
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

[Implementation evidence](../tasks/MOBILE_003_010e3_IMPLEMENTATION_STATUS.md) records
native offline/restart/replay and store upgrades, populated desktop/390px admission,
25k-Person query checks, paired readers and all 1,009 DB cases with passing evidence.
[Release evidence](../tasks/MOBILE_003_010e3_RELEASE.md) records 45 schema checksums,
142 prior/151 upgraded rowsets, actual artifacts and public HTTP/mobile/browser checks.
Native verification is simulator/emulator only. Release mobile checks cover reads
and compatibility; mutation/offline proof belongs to implementation verification.
Documentation pass: whitespace, local links/anchors, all 93 index IDs, unchanged
decision log, verbatim state archive and documentation-only scope checks passed.
No application tests were run; application code and runtime were unchanged.

## Next recommended action

The documentation-only efficiency pass is complete. Product follow-up
recommendation: specify dependent-family coverage and later refresh for
010e3-admitted People. Offline mobile stage changes were suggested as a possible
Mobile 004; neither new contract nor implementation is approved by this docs task.
Prepare C-gate erasure/restore work before real customer data. Keep user-deferred
work deferred and scope first production work through the readiness checklist.

## Approval currently required

Completed D-078 implementation/publication/deployment need no repeated approval.
New shared contracts, native distribution, live FUB/customer processing, activation
and production deployment retain their own scope and gates. Recovery targets,
retention/erasure, support access, outbound/recording consent and agent-departure
policy remain open where the decision log says so. This documentation assignment
authorizes reading/workflow cleanup only; it grants no product or release approval.
