# Slice 010f2 — Shared-development release

**DEPLOYED AND VERIFIED — 2026-09-11.** D-068's follow-up authorized commit,
merge, cleanup, push and deployment. The existing Mac-hosted shared-development
API and production Web bundle at [app.tarams.org](https://app.tarams.org/manage/migration)
serve source **`fc5a8757bcb2591a43976544e282722b3616039f`**, including implementation
**`9f457bc8db8707fa4ce361aac485fd6bf928bf34`**. Both are merged and pushed to main.
This replaces the prior 010f1 runtime from `e36ce36` against `crm_dev`.
No live FUB/customer processing, import, activation or production-cluster work occurred.

## Git integration and cleanup

The eight pending main planning files were preserved byte-for-byte privately;
four matched the implementation copies and four had expected implementation/status
amendments. A temporary stash allowed the merge and was dropped only after
preservation and merged-tree checks. All **1,016 verified source/configuration
hashes** match the merged source. Manifest SHA256:
`e0649d8e40cbaac17d09f0de8f2027cde61f0cb9d604cdd181a243084bdc40be`.

The clean merged `/Users/karrad/projects/crm-010f2` worktree and
`codex/slice-010f2-activity-import` branch were removed, including ignored build,
dependency and environment files. Main is the sole worktree. Before removing
`/private/tmp/crm-010f2-qa-694szdwe`, **249 original verification files** were
preserved and checked by hash in the private release directory's
`implementation-verification/`. Synthetic environment/control secrets were omitted;
the historical API's identical bytes were already preserved as `crm-api-before`.
Forty-eight external temporary logs were removed after preservation. Owned QA
services/volume/browser profiles had already been removed; all four QA ports
were absent. Shared services and earlier release recovery directories were preserved.

Sixteen published implementation logs had extra EOF blank lines removed when
staging exposed them. Original/published hashes and normalization are retained;
no product source or test result changed. Existing passing gates remain applicable:
`sqlx-prepare`, `check` (935 Rust, 1,108 Web, five doctests, 20 preflight and 11
email-worker tests), `check-db` (908/908), 92 query probes, one paired Person
comparison and 18 browser phases with exact native/business/storage reconciliation.
They were not rerun during release. See [implementation evidence](SLICE_010f2_VERIFICATION.md).

## Executed release checks

1. Preserved verified old API/admin/migrator binaries, Web bundle and configuration
   in **`/private/tmp/crm-010f2-release-guor74mb`**, mode 0700. Built ordinary
   `crm-api`, `crm-admin` and `migrate` with locked Cargo dependencies from merged
   source (36.89s); built staged production Web with pinned Node 24.16.0 and
   pnpm 11.22.0 (1.42s). Three backend and 73 Web hashes are catalogued against
   the full merged revision. Vite's existing large-chunk warning remains; build passed.
2. Recorded 45 business-table counts, 43 empty migration tables and three
   operational workspaces at revision 1. Took a custom-format `crm_dev` backup:
   **110,063,438 bytes**, SHA256
   `75438909afc6fde9e08810fe89003ebca34856e5e9a07be439fe48077d44a301`.
   `pg_restore --list` validated its 932-entry catalog. **Restoration was not exercised.**
   Normal `./scripts/db-migrate` passed, applying additive **`20260920000001`**
   and its 13 activity tables. The migrate/reconciliation helper took 6.75s;
   existing business counts, migration state and workspace states were unchanged.
3. Stopped only verified old API PID 35303 and Web PID 35323. Disabled six obsolete
   executable paths while retaining their bytes, including newly preserved old
   binaries and old dependency-build launch paths. Inventoried API/in-process
   workers, CLI/migrator launch paths, debug/release/dependency/example artifacts,
   remaining worktrees, containers and scheduled launch files/current-user cron.
   No additional CRM runtime or launcher was found. Actual database/executable-hash
   preflight passed for launch and confirmation, explicitly requiring ordinary,
   metadata **and `activity_confirmation_ready: true`**. API and worker catalog
   entries declare both import capabilities; credentials remained in environment
   variables passed to the real container `psql`, without mocked DB responses.
4. Atomically installed protected compatibility evidence, changing only the
   existing `CRM_MIGRATION_RELEASE_REPORT` environment value. Installed staged Web
   and started the existing `./scripts/dev-api` and pinned Vite preview lifecycle.
   New API PID **25757** serves port **3000**; Web PID **25780** serves **5173**.
   A fresh observed post-launch inventory and confirmation preflight passed.
5. **50 HTTP/auth/asset checks passed:** local/public health/readiness, anonymous
   denial, same-Organization and other-Organization admin empty lists, member
   denial, unknown activity/metadata resource denial, unchanged workspace session
   fields, ordinary CRM reads and exact index/entry/Migration assets. All created
   sessions were revoked. Empty other-Organization results are release smoke
   evidence; actual foreign-resource isolation retains its separate DB/HTTP proof.
6. **Eight public browser functional checks completed**, including the new activity
   panel's absent completed parent, disabled preparation, explicit refresh/reload
   and member navigation/direct-route denial. Ten desktop/390px screenshots were
   inspected; no material layout issue or horizontal overflow was found. Member
   denial has URL/DOM and HTTP evidence, not a screenshot. Both browser sessions
   were revoked and the context/browser closed. No source/business mutation occurred.
7. `./scripts/check-tunnel` passed public app/API **200/200** and realtime WebSocket
   **101**. A **214.61-second UTC observation**, ending at **22:18:29 PDT**, found
   stable listeners, matching three backend/73 Web/1,016 source hashes and zero
   API/Web WARN/ERROR lines. No recurring monitor was created.
8. Final read-only database reconciliation after browser completion found all
   45 business counts and three workspace states unchanged. All **56 migration
   tables** and operation admissions are empty. No seed/reset or binding change
   occurred. Registered FUB system name/key remain unset.

[Sanitized release evidence](../design/qa/slice-010f2-2026-09-11-release/README.md)
contains results, input inventories, hashes, counts and screenshots. Backup,
configuration, private helpers and recovery binaries remain outside Git.

An [independent read-only release audit](../design/qa/slice-010f2-2026-09-11-release/release-audit.json)
found no material release gap. It checked actual listener working directories and
executable mapping, all three backend/73 Web/1,016 source hashes, the backup
digest, protected compatibility inputs/report and all 32 recorded retired
artifacts' bytes and non-executable modes. It inspected the recorded HTTP,
browser, database, tunnel and UTC observation evidence without new DB/HTTP/UI
actions, test gates, configuration-value reads or product-source review.

## Harness qualifications and remaining scope

The first HTTP attempt incorrectly extended a new no-store assertion to an
existing metadata unknown-ID error. Its correct 404 was retained, its session
revoked, and the assertion was restricted to activity responses; the subsequent
50-check HTTP run exited zero. No product code changed.

The browser's raw result retains a failed final assertion: four GET requests to
Cloudflare Insights had been deliberately blocked by the harness, then counted
as unexpected external origins. All eight functional checks and ten screenshots
had already completed with zero page errors or attempted business mutations.
The exact known telemetry origin was qualified offline; the corrected helper
was syntax-checked and **the browser flow was not rerun**. This is a qualified
functional pass, not an untouched zero-exit browser script. Telemetry delivery
is outside this proof.

An initial observation check used process-local monotonic values across separate
shell executions and stopped before final runtime/DB checks. The completed
observation uses UTC and the original start-record modification timestamp;
its qualification and original values are retained.

The installed compatibility report expires at **2026-09-12 05:18:49 UTC**
(2026-09-11 22:18:49 PDT). Later import confirmation requires a fresh observed
compatible workload inventory and successful report. Expiry leaves ordinary CRM
access available and confirmation unavailable; no automatic renewal was performed.

Follow the [existing release runbook](SLICE_010c_RELEASE_PREPARATION.md) and
[activity compatibility contract](SLICE_010f2_CONTRACT.md). Preserve schema, source,
identities/results and workspace bindings. Any confirmed child, including a
cancelled child with no native writes, creates the durable activity boundary.
Prefer verified activity-capable forward recovery; never delete bindings, reset
workspace mode or restore the whole database merely to run old software.
The backup and retired artifacts are recovery evidence, not automatic rollback.

The [SQLx cancellation follow-up](../plans/PRODUCTION_READINESS.md#observed-transaction-cancellation-notices)
remains open before production cutover; this release does not establish the cause
or harmlessness of the earlier synthetic notices. Live FUB qualification remains
user-deferred. Remaining families, repair/deltas, customer-data readiness and
activation retain their own scope and gates.
