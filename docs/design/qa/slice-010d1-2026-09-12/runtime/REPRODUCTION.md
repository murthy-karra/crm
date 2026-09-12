# Slice 010d1 isolated runtime reproduction

**Browser phases passed through source disconnect.** This reproduction record
was updated from the coordinator's completed checkpoints through
2026-09-12T17:59:54.021Z, final reconciliation and the owned-service cleanup
receipt at 18:03:25.951325Z. Failed attempts and the cleanup recovery remain
recorded below. The coordinator subsequently removed the private scratch,
credentials and worktree `.env`; the final cleanup receipt records that time. D-070 authorizes isolated synthetic verification; this procedure does
not deploy the product or qualify a live Follow Up Boss account.

The recorded inputs were the private scripts under
`/private/tmp/crm-010d1-qa-5hqgjwkd`. Sanitized copies of the browser, runtime,
preparation, audit, gate and reconciliation helpers are archived in
`reproduction/`; API logs are in `logs/`. `runtime-manifest.json` maps original
and archived hashes and paths. Copies change only the documented path prefixes,
except the explicitly allowlisted People-ID setup summary. Restore the prefix
placeholders to an owned private fixture before executing a helper; the commands
below describe the recorded local layout. Do not execute the archived helpers
against this evidence directory. Preserve failed attempts and their original
request/checkpoint evidence when adapting the driver. Embedded source/build/input
hashes describe the original inputs; use the manifest when a path replacement
changed an archived file's hash.

## Fixed scope and prerequisites

| Resource | Exact scope |
|---|---|
| Checkout | `/Users/karrad/projects/crm-worktrees/slice-010d1` |
| Private directory | `/private/tmp/crm-010d1-qa-5hqgjwkd`, mode `0700`, not a symlink |
| Compose project / file | `crm-010d1-qa` / private `compose.json` |
| PostgreSQL | `postgres:18.6`, `127.0.0.1:55432`, database `crm_010d1_qa` |
| Database roles | bootstrap `crm_010d1_admin`; migration `crm_migrator`; application `crm_app` |
| PostgreSQL volume | Compose volume key `crm_010d1_postgres`, owned by this project |
| API | `http://127.0.0.1:13010` |
| Production Web preview | `http://127.0.0.1:15173` |
| Centrifugo fixture service | `centrifugo/centrifugo:v6.9.2`, `127.0.0.1:18082`; API URL `http://127.0.0.1:18082/api` |
| Browser | `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` |
| Node / pnpm | Node `24.16.0`; pnpm `11.22.0`; lockfile-installed Playwright Core `1.63.0` |

Use the repository's pinned Rust toolchain and installed Python `requests`
dependency (`scripts/requirements.txt`). Obtain the coordinator's DB/cargo slot
before building or operating services. The existing shared-development checkout,
runtime, databases and release evidence remain outside this scope. Do not use
`scripts/dev-bootstrap` or default `scripts/dev-services` for this fixture: those
target the ordinary development configuration.

Create the private directory and configuration only for an owned, unused
fixture; do not overwrite an existing `qa.env`, volume or checkpoint set. Generate
independent secrets with a cryptographic generator such as
`secrets.token_hex(32)` directly into a mode-`0600` file. Generate new values for
`POSTGRES_PASSWORD`, `CRM_DB_APP_PASSWORD`, `CRM_DB_MIGRATOR_PASSWORD`,
`CRM_SESSION_SECRET`, `CRM_RAW_PAYLOAD_KEY`, `CRM_DEV_SEED_PASSWORD`,
`CENTRIFUGO_HTTP_API_KEY` and `CENTRIFUGO_TOKEN_HMAC_SECRET`. The raw-payload key
must be exactly 64 hex characters. No credential value belongs in this document,
console output, shell history or command arguments.

`qa.env` contains explicit literals, with no `$` interpolation:

- `POSTGRES_USER=crm_010d1_admin`, `POSTGRES_DB=crm_010d1_qa`.
- `DATABASE_URL` names `crm_app` and `MIGRATION_DATABASE_URL` names
  `crm_migrator`; both use their generated passwords and
  `127.0.0.1:55432/crm_010d1_qa`.
- The generated application/session/realtime values above;
  `CRM_CENTRIFUGO_API_URL=http://127.0.0.1:18082/api`.
- Initial policy `CRM_FUB_SNAPSHOT_RUN_CEILING_BYTES=2147483648` and
  `CRM_FUB_SNAPSHOT_ORG_CEILING_BYTES=4294967296` (2/4 GiB).

The private Compose file passes database passwords from its environment to the
PostgreSQL container, mounts the owned PostgreSQL volume and private
`centrifugo.json`, and publishes only the loopback ports in the table. Realtime
keys are environment inputs, not literal values in the Compose file. Do not print
resolved `docker compose config` output.

The following shell variables are local conveniences; they do not load secrets:

```sh
qa_root=/private/tmp/crm-010d1-qa-5hqgjwkd
qa_repo=/Users/karrad/projects/crm-worktrees/slice-010d1
qa_node=/Users/karrad/.nvm/versions/node/v24.16.0/bin/node
qa_pnpm_bin=/Users/karrad/Library/pnpm/store/v11/links/@/pnpm/11.22.0/eeb737e15b4ed7190c895e85812ddc0832a617564aa8721c8139de5d87d3a2b4/bin
```

With the owned private configuration already provisioned, start only this
Compose project and provision its database roles using the existing role script.
The role passwords are read inside the container through `psql`'s `\getenv`:

```sh
docker compose -p crm-010d1-qa -f "$qa_root/compose.json" --env-file "$qa_root/qa.env" up -d --wait
docker compose -p crm-010d1-qa -f "$qa_root/compose.json" --env-file "$qa_root/qa.env" exec -T postgres sh -c 'psql -X -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB"' < "$qa_repo/infra/development/postgres/provision-roles.sql"
```

## API example and source controls

Build the explicit test-support example, then start it through the owned wrapper:

```sh
cd "$qa_repo"
cargo build --manifest-path backend/Cargo.toml -p crm-api --example history_capture_qa --features test-support --locked
python3 "$qa_root/runtime.py" start --epoch browser-initial
```

Each start requires a previously unused epoch. The wrapper refuses an existing
`api-process.json` or any listener on API13010 before changing controls. It
records the executable SHA-256, PID, epoch and private log path. A process being
alive after the wrapper's brief launch check is not proof of readiness; wait for
`/api/health` and `/internal/ready` on API13010 before preparation.

The example accepts **only** the two absolute arguments `qa.env` and
`control.json` in the fixed private directory. It rejects symlinks/nonprivate
files, an env file over 64 KiB or containing interpolation, a control file over
4 KiB, a nonzero startup grant, and an existing recorder epoch. It verifies
loopback PG55432/database/role both before connecting and through
`current_database()`/`current_user`. API binding precedes recorder initialization,
migrations and platform bootstrap. The executable neither creates nor drops a
database. It applies migrations with `crm_migrator`, bootstraps
`owner@platform.test` through the typed platform command, and serves with
`crm_app`. Web origin and API binding are fixed internally.

The initial private control object is:

```json
{"epoch":"browser-initial","scenario":"happy","mode":"normal","delay_ms":0,"history_units":0,"note_detail_gap":true,"failure_stream":"events","retry_failures":2,"retry_after_seconds":2}
```

Unknown fields fail closed. Epoch/scenario labels are ASCII letters, digits,
hyphens or underscores, at most 64 characters. A request freezes its control
snapshot before delay; changing the file does not change an in-flight response.

| Mode | Constructed behavior |
|---|---|
| `normal` | 201 events, 101 calls, 101 texts: seven collection pages; events use offsets then a token, calls offsets, texts a token |
| `overlap` | Events total 200; pages IDs 1–100 and 100–199; ID 100 has changed content and Person reference; terminal reconciliation must pause at 200 occurrences / 199 distinct IDs |
| `denied` | Collection 403 on `failure_stream`; this is inaccessible/unknown, not an empty enumerated stream |
| `retry` | First `retry_failures` attempts for **each exact scenario/mode/path** return 429; default two failures for each of three event paths means six failures total |
| `unavailable` | Source method returns unavailable |
| `invalid_identity` | Identity returns constructed 401 |
| `changed_account` / `changed_user` | Identity account changes 101→202 or source user 7→8; collections do not choose a new baseline |
| `delayed` | Delays the requested source response by `delay_ms`, maximum 5,000 ms |
| `empty` | Empty history collections with coherent zero totals |
| `pause_people` | Core People fixture failure for prerequisite/admission checks; not a normal history phase |

`failure_stream` is `events`, `calls` or `text_messages`. Retry failures are
bounded to three and Retry-After to 1–30 seconds. Use a new scenario for a new
retry experiment. Synthetic source credentials use the fixture marker
`synthetic-snapshot`, which is not a vendor key.

`history_units` is a **cumulative count of successful real-worker units**, not
rows or source requests. `0` grants no new work; a numeric target at most 100,000
allows at most one unit per two-second tick until that count is reached. `null`
uses the ordinary bounded batch, at most 32 units per tick. A request already
started can finish after the grant changes. A worker call that pauses before
source I/O can return false without incrementing completed units. Control edits
never resume a paused run: Resume remains an explicit API/UI action.

Normal identity and core prerequisite calls are separate from the seven history
collection calls. `control.stats.json` and the matching `source-<epoch>.json`
record bounded request starts/ends, operation, GET path, status, byte length and
response SHA-256, with no bodies or credentials. Capacity is 2,048 requests per
epoch. `control.history-stats.json` records units/errors;
`control.delivery-stats.json` records publication/disconnect counts. Counters
restart at zero for a new process/epoch; do not subtract baselines across epochs.

## People prerequisites and production Web

With history grants at zero, prepare three synthetic Organizations through the
real API, then take the baseline audit:

```sh
python3 "$qa_root/prepare_people.py"
python3 "$qa_root/audit_retained.py" baseline
```

Preparation creates `complete`, `cancel` and `budget` cases, each with an admin,
member, inactive former member and separate helper admin. It uses normal
create/invite/accept APIs, connects the injected fixture, captures a core
snapshot, builds a preview, maps Lead/source user 7 explicitly, and confirms a
People import into administrator review. Source People 101 and 102 become native
People; the trash/unavailable Person remains held. It saves IDs and completed
parent baselines in private `people-context.json`; it does not prepare history.
Do not also run the overlapping ordinary development seed script.

Serve a freshly built production Web bundle in a separate owned terminal:

```sh
cd "$qa_repo"
PATH="$(dirname "$qa_node"):$qa_pnpm_bin:$PATH" \
CRM_WEB_BIND_ADDR=127.0.0.1 CRM_WEB_PORT=15173 \
CRM_WEB_API_PROXY_TARGET=http://127.0.0.1:13010 \
CRM_WEB_REALTIME_PROXY_TARGET=http://127.0.0.1:18082 \
CRM_WEB_ALLOWED_HOSTS=127.0.0.1 VITE_API_BASE_URL=/api \
./scripts/dev-web-prod
```

This script builds `web/dist` before Vite preview; explicit environment values
pin the local proxy/origin. The browser driver opens `/manage/migration` in a
private persistent Chrome profile, uses the synthetic login password only in
process memory, blocks nonlocal page requests, and records executable/build
hashes. Its desktop viewport is 1440×1000 and narrow viewport 390×844. A viewport
screenshot/overflow assertion still needs human inspection of content, loading
state and dialog reachability before a visual pass is claimed.

## Browser phase order and recorded outcomes

Invoke one phase at a time; the explicit flag acknowledges runtime operation:

```sh
"$qa_node" "$qa_root/browser_driver.mjs" --authorize-runtime --case=complete --phase=prepare
```

Use that same command with the case/phase pairs in this order. A completed phase
is checkpointed and skipped on a later invocation unless `--repeat` is supplied.
Do not repeat a consequential phase to recover an uncertain result: reconcile
its existing request/receipt/checkpoint first. `--fresh-profile` makes a fresh
owned browser profile; it does not erase server state or request evidence.

The first `complete` attempt, `1789234079916-complete-complete`, passed its
product assertions but stalled during driver cleanup: it awaited response-body
tasks while the browser context was still open. The coordinator recorded the
recovery in `browser-cleanup-recovery.json` and sent SIGTERM only to the exact
owned Chrome parent (PID 3476), releasing those pending reads. Preserve that
attempt and recovery note; its eventual `passed` checkpoint is not evidence of
an unassisted clean browser exit. The corrected driver closes its browser
context before draining response tasks, fails the phase on close errors, and
rechecks late browser errors before recording final counters and `finished_at`.

The coordinator then used `--case=complete --phase=complete --repeat` for
`1789234398379-complete-complete`. This finished normally and rechecked the
existing retained capture: 403 observations across nine API read pages at
sequence 8, with the original cumulative seven history collection calls and no
history business publications. It was a retained-read/visual completion repeat,
not a second capture or a repeat of confirmation. Its screenshots are the later
complete-phase visual evidence; both attempts remain in the checkpoint file.

| Order | Case / phase | Recorded evidence |
|---|---|---|
| 1–4 | `complete`: `prepare`, `confirm-lost`, `progress`, `complete` | Passed: DB-only proposal; exact confirmation replay after deliberate response loss; old cursor boundary survives a later commit; 403 retained rows / seven collection pages with gaps and zero history business publications. The retained completion repeat is described above. |
| 5 | `cancel`: `cancel` | Passed: delayed collection was in flight; cancellation fenced its late settlement, retaining the first 100 events and releasing run reservations. |
| 6–8 | `cancel`: `overlap`, `denied`, `retry` | Passed: named independent attempts; 200/199 reconciliation uncertainty and two variants; denied scope stayed unknown; six 429s across three event paths honored delay before completion. |
| 9 | `budget`: `budget-queue` | Passed: independently named capture confirmed with zero grants and original 2/4-GiB allowances saved. |
| 10 | `budget`: `budget-pause`, after lowered-policy restart below | Passed: paused at sequence 0, raw bytes 0 and source calls 0 under reduced deployment headroom; no ledger/allowance rows were edited. |
| 11 | `budget`: `budget-resume`, after raised-policy restart below | Passed: UI explicitly increased run/Org allowances to 3/5 GiB, remained paused, then separate Resume completed the 403-observation capture. The original 390px dialog defect and targeted correction are recorded below. |
| Audit boundary | `audit_retained.py before-authority` | Coordinator reconciliation records unchanged native/parent/sibling/workspace data and exact history-only retained/reserved ledger deltas against `audit-baseline.json`. This precedes intentional authority changes. |
| 12 | `cancel`: `replacement` | Passed `1789235318692-cancel-replacement`: credential revision/source-user change fenced the old run; a new named proposal displayed user 8 versus parent user 7; identity-only admission then cancellation, no collection calls; fixture credential restored through the API. |
| 13 | `complete`: `access` | Passed `1789235810131-complete-access`: current admin demotion/restoration and foreign-Organization denial/option exclusion. Two earlier harness failures remain recorded below. |
| 14 | `budget`: `budget-layout` | Passed `1789235967633-budget-budget-layout` on the corrected production Web build: both enabled dialog actions fit 390px; Cancel preserved allowances/revision; the separate unconfirmed proposal was cancelled with zero source calls. |
| 15 | `complete`: `disconnect` | Passed `1789235990574-complete-disconnect`: all 403 retained observations remained readable after disconnect, with zero new source calls and source actions unavailable. |
| Final audit | `audit_retained.py final` | Passed coordinator reconciliation: 9 runs, 38 retained captures, 1,509 observations and 1,508 identities; 94 unchanged tables plus four intentional authority tables, against 98 unchanged tables before authority changes. Native/parent/sibling/workspace invariants and exact history-only ledger deltas reconcile. |

Complete steps 1–4 without a process restart or interleaved source grants in
another case. This preserves the meaningful seven-request measurement. Before
each policy restart, finish or pause all other source jobs, set grants zero and
wait for recorded in-flight requests to settle.

After `budget-queue`, stop only the recorded API, lower the run ceiling to 8 MiB
(below the 16-MiB request reservation), then start a new epoch:

```sh
python3 "$qa_root/runtime.py" stop
python3 "$qa_root/runtime.py" policy --run-ceiling 8388608 --org-ceiling 4294967296
python3 "$qa_root/runtime.py" start --epoch browser-budget-low
"$qa_node" "$qa_root/browser_driver.mjs" --authorize-runtime --case=budget --phase=budget-pause
```

Wait for API readiness after each start. A startup at new ceilings changes
policy, not existing run/Org allowances and not the paused state. Raise ceilings
to 4/8 GiB for the explicit UI increase, using another fresh epoch:

```sh
python3 "$qa_root/runtime.py" stop
python3 "$qa_root/runtime.py" policy --run-ceiling 4294967296 --org-ceiling 8589934592
python3 "$qa_root/runtime.py" start --epoch browser-budget-restored
"$qa_node" "$qa_root/browser_driver.mjs" --authorize-runtime --case=budget --phase=budget-resume
```

This tests exhaustion of deliberately reduced **deployment headroom**, not
natural consumption of the original 2-GiB allowance. The recorded epochs were
`initial`, `low-budget` and `restored-budget`; the `browser-*` names above are
fresh reproduction examples, not the names of the executed epochs. Passed
budget checkpoints record the observed pause and allowance increase. Preserve
their actual policy revisions and ledger/reservation totals with those epochs.

The first access attempt, `1789235619122-complete-access`, passed its current-role
revocation assertion and then failed because the harness read the foreign
session before sign-in had completed (401). The second,
`1789235732667-complete-access`, timed out waiting for the initial migration
panel after login. The coordinator strengthened the driver's sign-in response
and navigation waits; the fresh-profile attempt
`1789235810131-complete-access` passed both role and tenant checks. Preserve all
three attempts rather than treating either harness failure as a product pass.

Actual inspection found HC-R2-W02 in the original
`1789235134045-budget-budget-resume` 390px screenshot: the long allowance
confirmation label pushed Cancel partly outside the viewport. The Web author
shortened the label to `Increase allowances`; the coordinator rebuilt the
production Web bundle and ran `--case=budget --phase=budget-layout` as a targeted
confirmation within the same integrated verification. The first layout attempt,
`1789235893764-budget-budget-layout`, incorrectly expected a terminal run to
offer budget actions and timed out. The corrected attempt used a new unconfirmed
proposal with source grants zero. Both enabled action bounds fit 390×844 and
the saved fixed image confirms the layout. The driver then restored desktop
width, clicked Cancel, asserted unchanged run revision and allowances, and
cancelled the unconfirmed proposal. No second allowance increase or source work
was performed for this label correction. The final disconnect phase also used
the corrected build. Keep each attempt's Web manifest hash; the earlier
functional budget phase and later layout confirmation used different bundles.

The independent saved-image record is `visual-inspection-agent.json`, containing
22 actually viewed images and the preserved before/after HC-R2-W02 evidence.
Static viewport framing does not prove all lower controls: several 390px report
images show only the top of a vertically scrollable page. Table-local horizontal
scrolling and source/API assertions are separately recorded by the coordinator.

## Evidence and stopping

Run `audit_retained.py` at named, quiescent boundaries: `baseline` after
People setup, `before-authority` after budget completion but before replacement
or role changes, and `final` after authority/disconnect phases.
It hashes tenant row sets plus the Organization row, and reads table hashes,
history run summaries and the shared ledger in one PostgreSQL
`REPEATABLE READ READ ONLY` snapshot. Source/delivery JSON files are separately
sampled operational counters; they are not part of that database snapshot.
The coordinator must compare expected history/control changes against unchanged
People-parent, native-history/Today/business data; a saved audit alone is not a
reconciliation pass.

Preserve `browser-checkpoints.json`, per-attempt screenshots, source epochs,
runtime process/build records, private API logs and sanitized audit outputs.
The driver records sanitized route hashes/status/byte counts rather than source
bodies, cookies or cursors. Expected negative HTTP/console events must remain
distinguishable from unexplained failures. File hashes and screenshots are
evidence of the named run, not substitutes for response/receipt/DB assertions.

Stop the owned API with `runtime.py stop`; it verifies the exact recorded command
before signalling that PID. Stop the owned Web terminal separately. Preserve
the owned database volume until final audit and reconciliation have been saved.
After the coordinator's final reconciliation, the recorded cleanup stopped the
owned API/Web/Chrome processes and removed only the `crm-010d1-qa` Docker project,
volume and network. The receipt verifies all four owned ports closed and shared
container IDs/ports and shared-listener PIDs unchanged against
`cleanup-baseline.json`. The worktree remains; no commit, merge, push or deploy
occurred. Do not generalize this cleanup to shared services. The archived receipt
currently leaves `private_credentials_and_scratch_removed` false; the
coordinator will update that receipt and its manifest entry after private-file
removal.

The final source audit reconciles all 74 synthetic GET reader calls, including
setup, retries and the discarded late response. Seven worker identity reads
plus 32 history collection attempts, less one discarded response, account for
the 38 retained capture rows. The logs contain no ERROR or HTTP 5xx, but do
contain 23 PostgreSQL already/no-transaction-in-progress WARN notices whose cause
and baseline provenance were not established. This is not warning-free evidence.
`runtime-source-audit.json` and `.md` preserve the exact scope, hashes, line
references and limitations; the runtime manifest preserves original and archive
hashes for those inputs. Private environment/configuration files, full People
context, browser profiles and sessions are excluded from the archive.

The example injects `FixtureReader`, `ReleaseReadiness::for_tests()` and
`Publisher::recording()`. It uses real application routes, authorization,
commands, SQL, encryption, worker settlement and Web code, but **does not prove**
production release inventory/readiness, source HTTP authentication/pacing or
redirect handling, actual Centrifugo delivery, live FUB pagination/visibility,
email/media access, delta/cutover fidelity or customer-data readiness. Separate
transport, release, database and performance evidence remains separately scoped.
Retained capture is not native timeline import, Today activation or permission
to process a live customer account.
