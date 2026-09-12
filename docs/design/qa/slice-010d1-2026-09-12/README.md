# Slice 010d1 implementation evidence — 2026-09-12

Historical source capture and bounded admin coverage are implemented and verified
in the uncommitted `codex/slice-010d1-history-capture` worktree. The authoritative
[verification record](../../../tasks/SLICE_010d1_VERIFICATION.md) maps A1–A12 to
actual checks and states remaining limits. The two bounded
[implementation reviews](../../../tasks/SLICE_010d1_IMPLEMENTATION_REVIEW.md) are
READY. No native timeline, live FUB/customer-data, Git publication or deployment
completion is claimed.

| Package | Evidence |
|---|---|
| [Backend](backend/README.md) | Focused parser/DB/transport/lint evidence, additive-upgrade proof, 75,000 observations, 1,500 retained pages and 14 actual SQL plans; interim failures retained |
| [Repository gates](gates/README.md) | SQLx preparation, full check, 934-test DB gate, session follow-up and post-visual full check; exact source manifests and logs |
| [Runtime reproduction](runtime/REPRODUCTION.md) | Production Web with real typed API/DB operations and constructed source/readiness/publisher; 19 browser attempts, including 3 preserved harness failures |
| [Final reconciliation](runtime/reconciliation-final.json) | 108 tenant tables checked; exact allowed changes, unchanged native/parent/sibling/workspace data and shared byte ledger |
| [Source/log audit](runtime/runtime-source-audit.md) | 74 synthetic GETs; 39 history-worker reads reconcile to 38 retained captures after cancellation discard; retries and 23 transaction notices qualified |
| [Visual inspection](runtime/visual-inspection-agent.json) | 22 independently viewed desktop/390px images, including the before/fixed allowance dialog and actual disconnect state |
| [Cleanup receipt](runtime/cleanup-receipt.json) | Exact owned service/process/port cleanup; shared development preserved; final private-file disposition |

The final full check passed 949 Rust tests, 1,131 Web tests, five doctests,
25 preflight tests and 11 email-worker tests. The separate DB gate passed 934/934.
The populated collector is constructed correctness/query-plan evidence, not
production latency or capacity qualification; sparse filters can scan many index
entries despite bounded fetch/decryption/response size.

All browser attempts and 75 screenshots are retained. A completed run needed
owned-browser shutdown to release the harness's pending response-body cleanup;
a source-free retained repeat then exited cleanly. Two access attempts needed
login synchronization, and the first layout attempt targeted a terminal run
whose allowance controls are correctly absent. The corrected layout proof used
an unconfirmed proposal and cancelled it with zero source reads. The actual
390px clipped Cancel button was fixed by shortening the confirmation CTA; the
final image and button bounds verify the correction.

The original backend/schema gate manifest is `f559c658…`. The final Web gate is
`2af03113…`; all 1,037 entries were compared and only the CTA and its test locator
changed. Original DB/collector evidence was not relabeled as a later rerun.
Package manifests record exact file bytes and original-to-archive transformations.
The root `SHA256SUMS` inventories this complete artifact tree and excludes itself.

Environment files, keys, browser cookies/profiles, source response bodies and
executable binaries are excluded. Scans compare configured private credentials
and encoded variants; they do not imply an unrestricted secret-detection or
host-wide network audit. API logs have no ERROR/HTTP 5xx or scanned credential/
payload matches, but preserve 23 transaction-state notices. Their cause remains
a bounded platform follow-up before production cutover.
