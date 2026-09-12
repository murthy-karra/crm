# Slice 010c — Release compatibility and recovery

**Compatibility and recovery runbook.** The authorized shared-development
release completed on 2026-09-11; actual results are in
[SLICE_010c_RELEASE.md](SLICE_010c_RELEASE.md). Reuse this procedure for subsequent
authorized launches and before import confirmation. Live FUB operations remain
user-deferred. Implementation evidence belongs to
[the verification record](SLICE_010c_VERIFICATION.md).

## Durable compatibility boundary

An Organization enters `migration_review` before the import writes People. Its
`migration_workspace` binding survives completion and cancellation. Every API,
worker and administration entry point that can access that database must enforce
`crm-workspace-v1`. The old 010b executable cannot enforce this boundary.

Before the first confirmation, retire every pre-gate process and disable its
launch paths. Inventory in-process workers as well as standalone workloads,
administration executables and recovery launch paths. An Organization admin's
import acknowledgment does not establish these deployment facts.

After a binding exists, recover using a known compatible artifact. Never delete
the binding, reset workspace mode or restore the whole database merely to make an
old application run. Cancellation preserves imported rows and the hold. Existing
operational Organizations continue to operate; this slice has no activation API.

## Operator preflight

[`scripts/migration-release-preflight`](../../scripts/migration-release-preflight)
is read-only. It hashes candidate executables, validates an operator-maintained
build catalog and workload inventory, and queries all persistent bindings. A
completed or cancelled import is still included. It neither deploys nor stops
processes, and it cannot independently discover that an omitted process exists.
The operator must supply a complete, truthful inventory.

Keep these JSON inputs outside tenant-controlled storage, owned by the operating
user or root, without group/world write permission. Version is the JSON number 1.

| File | Exact fields |
|---|---|
| Known builds | `version`, `artifacts`: entries with `sha256`, `role`, `gate_version`, `revision` |
| Candidates | `version`, `artifacts`: entries with `role`, `path` |
| Workload inventory | `version`, `target`, `observed_at`, `complete`, `pre_010c_retired`, `processes`: entries with `id`, `role`, `sha256` |

Roles are `api`, `worker`, `cli`, `migrator`. Gate versions are
`crm-workspace-v1` and `pre-010c`. Catalog revisions must be full Git hashes of
the verified source used to build each artifact. Do not label an uncommitted
working-tree executable with the baseline revision. A single executable used
for multiple roles needs the corresponding catalog and candidate entries.

Use UTC RFC3339 inventory timestamps. Evidence is valid for at most five minutes
from the observation; updating its timestamp alone is not a fresh observation.
Keep DB credentials in the existing protected libpq configuration or `PG*`
environment. Do not place passwords in JSON, command arguments or evidence logs.

For an authorized deployment, invoke from the repository with paths to the actual
operator-owned files and the intended target identity:

```sh
./scripts/migration-release-preflight \
  --artifacts /absolute/private/known-builds.json \
  --candidates /absolute/private/candidates.json \
  --inventory /absolute/private/processes.json \
  --target target-identity \
  --purpose launch
```

Exit 0 permits the requested purpose; 1 is a policy rejection; 2 means evidence
is invalid or unavailable. Preserve the JSON result and input hashes privately.
Require `--purpose confirm` and `confirmation_ready: true` before enabling import
confirmation. A launch result alone does not grant confirmation readiness.
Publish only successful, current confirmation evidence to the server-owned
`CRM_MIGRATION_RELEASE_REPORT` path using atomic replacement and protected
permissions. The runtime independently verifies its executable, database and
current compatibility state; no tenant request may set readiness.

For a later authorized deployment: identify and preserve the current release,
build and catalog the verified replacement, apply the additive migration through
the normal migrator, retire pre-gate workloads and launch paths, capture fresh
inventory, pass the compatibility preflight, then start the compatible API/Web
and verify ordinary access plus admin/member review behavior. Keep confirmation
unavailable whenever evidence cannot be renewed. Record the actual sequence,
artifact hashes, backup status and runtime checks in the eventual release record.

## Scope of current proof

The implementation suite uses synthetic artifacts, mocked preflight DB reads and
disposable runtime databases. The browser example injects readiness through a
compile-time test-support seam. Those checks do not certify a deployed fleet or
replace a release inventory. Live authorized FUB validation remains user-deferred;
customer-data prerequisites and later activation remain open.
