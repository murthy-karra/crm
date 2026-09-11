# Slice 010a — FUB connection and migration assessment

**Delivery: implemented, synthetically verified, merged, pushed and deployed to
shared development (2026-09-11). Live FUB validation remains user-deferred.** See
[verification](../tasks/SLICE_010a_VERIFICATION.md) and
[release](../tasks/SLICE_010a_RELEASE.md).

**Status: APPROVED for implementation, 2026-09-10.** The user explicitly
approved API-first assessment with encrypted saved credentials after reviewing
this specification and brief (D-060). D-059 accepts a new, empty destination
Organization first. Source qualification and live validation remain required;
no live account has been tested. D-060's 2026-09-11 follow-up separately authorizes source cleanup, commit, merge
and push. A later same-day follow-up authorized the shared-development
deployment; reading a real customer book remains outside that authorization.

**Validation amendment:** the user explicitly chose “Not yet; leave live
validation pending” when asked about a synthetic FUB test account. Complete
implementation and synthetic verification now; acceptance §7.7 stays visibly
pending, not an implementation blocker or a passing live result.
The [source contract qualification](../research/SLICE_010a_FUB_SOURCE_CONTRACT.md)
records the published response shapes and their limits.

Required context: [migration summary](../plans/SLICE_010_MIGRATION_SUMMARY.md),
[decision log](../decisions/DECISION_LOG.md),
[architecture baseline](../architecture/ARCHITECTURE_BASELINE.md), and
[implementation brief](../tasks/SLICE_010a_IMPL.md).

## 1. Outcome and limits

An Organization admin opens **Manage → Migration**, connects an authorized FUB
account, runs an assessment, and sees which source collections were checked,
what FUB reported, what remains unknown, and which destination capabilities
exist. Reloading the page recovers the saved assessment.

This is a **bounded access assessment**, not full enumeration or an import
preview. A completed assessment means its planned checks finished; it does
not mean the account can be migrated completely. No imported People, contact
methods, Inquiry/history facts, notes, tasks, tags, fields, users or stages are
created. FUB requests are GET only. No outbound communication is enabled.

In scope: one Organization-bound source connection, credential replacement and
disconnect, restartable assessment jobs, encrypted probe evidence, an honest
report and admin Web controls. Out: CSV upload, OAuth, source writes, full
snapshot/pagination, mappings, deduplication, import commands, delta, rollback,
cutover, Operator tools, realtime events and a new CLI. Those remain later rungs.

The destination may have ordinary setup and members. Assessment performs no
import and makes no guarantee of destination emptiness at a later time; 010c
must enforce D-059's precise emptiness and rerun rules transactionally.

## 2. Decisions and source qualification

The approved package is:

1. **API-first assessment.** Customer-provided exports may supplement later
   rungs, with their limits separately reported. If the intended API use cannot
   be established, return to planning rather than silently substituting CSV.
2. **Encrypted persistent credentials.** Organization admins manage the
   connection; the worker can recover after a restart. Memory-only credentials
   are the alternative, with reconnect required after restart and different
   connection/job contracts. Never keep the key in browser persistence, query
   caches, logs, model context or a response, including its suffix.
3. **Bounded probes.** First-page access and reported totals now; full record
   inventory and exact importability in 010b. This reduces source reads without
   claiming fidelity that has not been measured.

Before the connector implementation is called ready, record the actual source
contract: identity/account identifiers, role representation, collection keys,
count metadata, source version evidence and permitted identification headers.
Use official schemas and synthetic fixtures; do not invent field names from
examples. Live validation separately requires an authorized test account and
confirmation of the intended access route. A fixture passing is not live proof.

FUB documents API keys as Basic-auth usernames with an empty password; keys
inherit their user's privileges. Successful identity access does not establish
all endpoint permissions. [Authentication](https://docs.followupboss.com/reference/authentication),
[identity](https://docs.followupboss.com/reference/identity).
Customer-facing integration identification and the access-route question are
recorded in the summary, with [registration documentation](https://docs.followupboss.com/reference/identification)
and [API terms](https://docs.followupboss.com/reference/fub-api-tou).
This specification makes no determination that a particular use is permitted.

Use synthetic source records while D-015's erasure runbook and O-012/O-013's
first-external-customer prerequisites remain open. Retention, erasure policy,
production key management and broader source-data audience rules are not
settled by approving this assessment slice.

## 3. Source adapter and assessment profile v1

**010b amendment (D-063, 2026-09-11):** [SLICE_010b](SLICE_010b.md) owns an
additive closed pagination/representation seam and snapshot/preview contracts.
010a's six fixed probes, one-MiB bound and response envelopes stay unchanged.
Source pacing and Organization job exclusion cover both assessments and snapshots;
credential replacement/disconnect also fence snapshot work. Retained snapshot
reads use current Organization-admin authority independently of live credentials.
010a retry continues to adopt the retrying admin; 010b source retry does not.
This amendment applies to §§3–6 when 010b is implemented; it does not redeploy 010a.

An injected, typed FUB reader lives under `crm-app::domain::migration`. The
runtime adapter uses the existing HTTP dependency. Production requests target
only `https://api.followupboss.com/v1/` and a closed path/query allowlist. Disable
redirects. Never accept a source URL, hostname, account identifier or role as
trusted browser input. The test fake cannot be enabled through a public route.
No mutation method is exposed by the reader.

The approved fixed profile is deliberately small:

| Check | Request / evidence | What the report can say |
|---|---|---|
| Identity | `/identity`, before connecting and again before each assessment | Verified source account and source-user scope, or scope unknown |
| People, excluding Trash | `/people?limit=1&fields=id&includeTrash=false` | Reported total for this query and records actually returned |
| People, including Trash | `/people?limit=1&fields=id&includeTrash=true` | Separate reported total; do not subtract two moving totals to invent an exact Trash count |
| Users | `/users?limit=1` | Source-reported total and access evidence; no member mapping |
| Stages | `/stages?limit=1` | Source-reported total and access evidence; no stage mapping |
| Custom-field definitions | `/customFields?limit=1` | Source-reported total and access evidence; no statement about all types or values |
| Other families | No request in this profile | “Not checked” plus known destination/source limitations |

FUB documents identity's account/user purpose, selected People fields and Trash
behavior, and collection pagination/count metadata. The adapter must verify
each response shape; absent or inconsistent metadata remains unknown.
[People](https://docs.followupboss.com/reference/people-get),
[users](https://docs.followupboss.com/reference/users-get),
[stages](https://docs.followupboss.com/reference/stages-get),
[custom fields](https://docs.followupboss.com/reference/customfields-get),
[pagination](https://docs.followupboss.com/reference/pagination).

Always include report rows for notes, tasks, tags, inquiries/history, calls,
texts, emails, recordings, addresses, relationships, appointments, deals and
automation/settings. Their absence from this probe profile is not evidence of
absence from FUB. Use the summary's destination matrix, versioned with the
profile; do not guess per-record readiness from a collection name or one item.

No continuation is followed in 010a. A returned continuation means the read
is partial; source-provided links are stored only in encrypted evidence, never
executed. 010b owns full pagination, checkpoints and record reconciliation.

Approved operational bounds: 10-second total request timeout, 1 MiB decoded
response-body cap, one request in flight per source account, and one active
assessment per Organization. Oversized bodies produce `response_too_large`
and incomplete evidence, never a silently truncated successful response.
These are local safety limits, not FUB limits or promises about account size.
Honor upstream rate headers and `Retry-After`; persist the next eligible time
rather than sleeping with a DB connection held. Bound transport/429/5xx retries
to three attempts per check, then pause for explicit retry. No automatic
retry of a rejected credential; a 403 is an access failure, not a zero count.
An explicit retry from `paused` begins one new bounded attempt cycle for the
unfinished check, while retaining cumulative attempt evidence. Repeated retry
requests while queued/running/waiting do not begin another cycle.

## 4. Approved local contracts and ownership

Before 010a, no migration HTTP, persistence or Web contract existed. D-060
approved the following additive admin API, tables and page, now implemented
in `crm-app`, `crm-api` and Web with SQLx verification. Existing
Person/Today/Operator contracts and all business tables are unchanged. The
implementation includes one additive schema migration and no business-data
backfill. This specification owns those new contracts. Source integration
does not itself update the running application or its database.

All paths begin `/api/migrations/fub`. All reads and writes require the trusted
active Organization admin context; the application command/query layer also
enforces it. A platform-admin flag alone is not an Organization bypass.

| Method and suffix | Proposed behavior |
|---|---|
| `GET /` | Connection summary, active assessment and latest report; nullable when absent |
| `POST /connections` | `{request_id, api_key}`; validate identity, then create the Organization's connection; 201, or same receipt on replay |
| `PUT /connections/{id}/credential` | `{request_id, api_key, expected_revision}`; validate identity and replace only for the same source account; 200 |
| `DELETE /connections/{id}` | Disconnect and erase the stored credential; fence/cancel active work; 204, also when already disconnected |
| `POST /assessments` | `{request_id, connection_id, expected_revision}`; persist and enqueue; 202 with assessment ID |
| `GET /assessments/{id}` | Durable report/progress projection; 200 |
| `POST /assessments/{id}/retry` | Resume a paused assessment on the same connection revision; completed checks are not duplicated; 202 |
| `POST /assessments/{id}/cancel` | Idempotent cancel; return authoritative state; a finished result is not erased |

UUIDs in paths are untrusted locators, not authorization. Wrong-Organization
IDs are 404. Follow existing path/admin/body extraction precedence: malformed
path 400, no session 401, non-admin 403, malformed body 400; then scoped lookup,
revision/state conflicts 409, invalid credential/input 422, dependency failure
503. Reject unknown JSON fields; credential requests have a 16 KiB body cap.
Upstream error text/bodies are never exposed in these envelopes. Reads use
`Cache-Control: no-store`; Web uses the existing actor/Organization cache scope.

Request IDs deduplicate connection creation/replacement and assessment start
within Organization and operation. Same ID with different input is 409; use a
keyed, domain-separated request digest for secret-bearing inputs, never a
plaintext key copy. Persist a successful mutation and its receipt atomically.
An uncertain network response is recovered by replay or a scoped GET, not a
second logical operation. Retry/cancel commands are intrinsically idempotent
for the current state and must not reset retry budgets while already running.

One connection row per Organization remains bound to its verified source
account, including after disconnect. Reconnection uses credential replacement;
changing source accounts requires a separately specified operation, not
deleting the binding. A new credential revision cancels old active work; start
a new assessment for it. Old reports keep their original account/revision.

Proposed tenant-owned persistence:

- `migration_connection`: verified source account ID, encrypted display/identity
  metadata, status, revision, credential nonce/ciphertext, initiating actor and
  timestamps. Credential bytes become NULL on disconnect.
- `migration_assessment`: connection ID/revision, profile version, initiator,
  state, progress, retry timing, lease token/expiry and timestamps.
- `migration_assessment_check`: fixed check key, state, count evidence, error
  code, attempts and committed evidence pointer; unique within the assessment.
- `migration_assessment_evidence`: bounded encrypted raw probe response bytes,
  capture time, HTTP status, byte count, keyed content hash and selected safe
  source-version metadata. Never request Authorization/Cookie headers. No raw
  read/download surface in 010a; no customer record content in the report.
- Idempotency receipts/digests scoped to Organization and operation.

Every relation uses Organization-scoped keys/foreign keys, including the
assessment-to-connection and check-to-evidence relationship. Index connection
lookup, assessment lookup/latest and due-work claims; no scan over People.
All local mutations use typed commands with server-owned context.

Use the existing XChaCha20-Poly1305/development-key conventions through separate
typed credential/evidence wrappers. Bind authenticated data to purpose,
Organization, row ID and version; preserve all existing raw-payload formats.
Never place probe evidence in `raw_payload`, where intake extraction can see
it. Raw captures are encrypted/deletable content per D-015, not PII in an
immutable history table. 010a owns these bounded evidence records; 010b owns
the distinct full-snapshot design. Disconnect is credential removal, not a
claim that historical evidence or backups were erased. No automatic retention
expiry is introduced by this slice.

## 5. Worker state, cancellation and recovery

Assessment states: `queued`, `running`, `waiting_retry`, `paused`, `completed`,
`cancelled`. A completed assessment may contain denied, unknown or partial
checks. A pause records a closed reason and next action; it is not completion.

Reuse the in-process sweep pattern: short transaction claims one due job with
`FOR UPDATE SKIP LOCKED`, establishes a lease and commits; HTTP runs outside
the transaction. Proposed lease: 60 seconds for one bounded request/commit.
Commit evidence, check result and progress together, conditional on the lease
token, connection revision and non-cancelled state. Stale workers cannot
overwrite a retry, disconnect or newer assessment. If persisting evidence
fails, no successful result/count is committed; retry the check.

Recheck the initiating actor's active admin membership and connection before
each request and commit. Lost authorization pauses work; no new request starts.
Another current admin may explicitly retry, becoming the recorded initiator.
An already in-flight source GET may finish after cancellation; its result must
not update a cancelled report. State this limit in the implementation record.
Source account identity mismatch pauses before any collection probe.

An expired lease permits safe reclaim; the request may repeat but its successful
check/evidence commit is unique. Persisted checks recover after process restart.
Never infer continued authority from the actor's membership at job creation.
Serialize source-account reads across connections in the worker; do not treat
two different Organization connections as independent upstream rate budgets.

## 6. Report and Web behavior

Each report identifies the destination, verified source display name, source
access scope (`owner`, `admin`, `restricted`, `unknown`), assessment window,
profile version and state. Display names are escaped untrusted text. Source
role alone never makes a collection's coverage complete.

Each fixed row carries:

- `reported_total`: nullable nonnegative count, explicitly labelled as FUB's
  count for the recorded query/time/scope; never treated as an atomic snapshot.
- `retrieved_count`: records actually returned by committed successful checks,
  not bytes, guessed totals or items from failed attempts.
- `coverage`: `partial`, `unavailable`, `not_checked`, or
  `complete_for_query`; the last requires evidence that this bounded query was
  exhausted and consistent, and never means all fields/history were captured.
- `destination_readiness`: `model_available`, `review_required`, or
  `destination_missing`, with a reason from the versioned destination matrix.
- `reason_codes`, observation time and a concrete next action.

Counts travel as nullable decimal strings to preserve integer precision across
Rust, PostgreSQL and JavaScript. Unknown, rejected, malformed or inconsistent
metadata is NULL with a reason, never coerced to zero. People including/excluding
Trash are overlapping views: never sum them. Do not sum counts across different
entity families into a fictional migration total. A sample must not produce a
duplicate forecast, percent importable or “ready to cut over” badge.

The Web page uses the existing admin navigation, session lifecycle and query
patterns. It shows connection status, a password input for the API key,
**Assess account**, progress by checks completed (not records migrated), the
report table, Retry/Cancel where valid, Replace key and Disconnect. Explain
before connection that credentials are saved encrypted and this step reads
FUB without importing records. Clear the key after submission or leaving the
view; do not put it in TanStack mutation variables/history. Uncertain credential
submission prompts status recovery or deliberate re-entry, not hidden storage.

Poll the durable assessment every two seconds while active, back off on errors,
stop on terminal/paused states and refetch on focus/reconnect. Preserve the last
report with its timestamp while a new assessment is running; never relabel it
as the current result. Authentication loss clears scoped views through the
existing lifecycle. Use keyboard-accessible controls, labelled unknown/error
states and a restrained live region; no new design system or AI report writer.

## 7. Acceptance and verification

1. Admin commands and reads succeed; ordinary members, deactivated admins and
   cross-Organization connection/assessment/evidence references are denied.
2. Fixture server records only the exact allowlisted GET requests. Redirects,
   hostile continuation links, untrusted role/account input and source HTML
   cannot escape their boundaries. No business table or source record changes.
3. Both People scopes, absent/inconsistent totals, restricted access, 401/403,
   valid-empty results, malformed JSON and over-limit responses produce honest
   reports. Unchecked families remain visible; no invented import forecast.
4. Credential encryption/redaction, wrong-purpose/Organization/row decrypt,
   replacement to another account and disconnect fencing are verified. Tracing
   capture contains no key, content, ciphertext or upstream error-body leak.
5. Idempotent start, concurrent start, lease expiry, restart, retry exhaustion,
   `Retry-After`, cancel during HTTP and DB commit failure preserve one result
   per check. No DB transaction is held while waiting for FUB.
6. Browser walkthrough: connect synthetic account, assess, inspect partial
   report, reload, repair paused access, replace/disconnect; also member denial
   and Organization switch. Evidence contains synthetic data only.
7. Authorized live test confirms identity/scope and this exact probe profile.
   Until performed, explicitly mark **live source validation unverified**;
   fixture-only completion must not be called full connector readiness.

Run targeted Rust/Web tests, `./scripts/sqlx-prepare` for the new SQL and final
`./scripts/check` / `./scripts/check-db` under the existing single DB gate.
Inspect new claim/report query plans. Existing hot queries are unchanged; do
not rerun the 019b benchmark. If runtime integration changes a hot path, declare
it and apply D-050's one paired benchmark. At most two review/fix rounds.

Record exact files, commands and evidence in `SLICE_010a_VERIFICATION.md` during
implementation; the verification record distinguishes completed checks from deferred live validation.
