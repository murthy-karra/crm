# Mobile 002 / 010e2 — Shared-development release evidence

Runtime source `9d04755e405848cb06a8e4c135793d9994176648`, released under the
explicit D-076 follow-up. The [release record](../../../tasks/MOBILE_002_010e2_RELEASE.md)
explains executed checks and limitations. Closeout changes are documentation only.

This allowlist contains source/artifact hashes, backup metadata, complete tenant
rowset digests and counts, actual workload inventory, compatibility evidence,
HTTP/mobile/browser results and cleanup attribution. Configuration values,
credentials, database dumps, raw customer records and private logs are excluded.
Private helpers and recovery artifacts remain in the directory in `release-final.json`.
The isolated build cache is removed; verified new and old binaries are retained.

All 134 prior tenant rowsets preserve every older column. Initial note revisions
are separately verified as 1. All 142 full upgraded rowsets match after smoke
cleanup. The eight new refresh tables remain empty; original workspace bindings,
100,077 existing history-review rows and all 45 business-table counts are preserved.

The 124 HTTP checks passed; 43 mobile requests verified seven-day authorization,
new capabilities, exact current-task fields/revision, actor/context denials and
complete four-Person reconciliation. This selected workspace has no notes, so
positive note editing/current-note proof remains attributed to the implementation
and native evidence. All six temporary HTTP/mobile sessions were revoked. The
browser reused an existing authenticated session only for reads and left it intact.
Two screenshots were inspected inline; no standalone screenshot files are claimed.

`build-summary.json` describes preparation, so its deployed=false field precedes
`deployed.json`. The latter captures launch time; `release-final.json` and
`closeout-preflight-confirm.json` capture later verification and report expiry.
No app distribution, physical-device/cellular test, live FUB/customer processing
or activation was performed by this release.
