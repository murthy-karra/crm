# Slice 010e1 — Frozen implementation contract

D-075 owns this additive contract. Engine and observed-artifact capability are
`fub-core-change-v1`; canonical semantics are the existing 010b lossless JSON
parser (`snapshot_source::JsonParser`, canonical-number coefficient/exponent
encoding, decoded-key uniqueness, preserved missing/null, array order and unknown
properties). No source fetcher or credential reaches the report worker.

## HTTP serialization

Base `/api/migrations/fub/core-change-reports` (no trailing slash required).
POST accepts exactly `{request_id,parent_import_id,newer_snapshot_id}`. Resume
and cancel accept exactly `{request_id}`. Creation and controls return a stored
receipt `{report_id,state,inputs}`; exact request retries return that receipt.
GET detail returns:

```typescript
interface Report {
  id: string; parent_import_id: string; engine_version: 'fub-core-change-v1';
  state: 'queued'|'running'|'paused'|'completed'|'cancelled';
  phase: 'capture'|'compare'|'sealed';
  created_at: string; updated_at: string; completed_at: string|null;
  pause_reason: string|null; output_revision: string|null;
  inputs: {
    baseline: CaptureBoundary; newer: CaptureBoundary;
    source_account_id: string; workspace_revision: string;
    source_scope: 'consistent_identity'|'changed_identity'|'unknown_identity';
    warnings: string[];
  };
  counts: null | { families: Record<Family, DispositionCounts>;
    source_ids: string; invalid_observations: string;
    observations: string; equal_repeats: string; conflicting_groups: string };
  progress: { captures_processed: string; observations_processed: string;
    groups_compared: string };
  retained_bytes: string; reserved_bytes: string;
  actions: { resume: boolean; cancel: boolean };
}
interface CaptureBoundary {
  snapshot_id: string; capture_sequence: string; profile_version: string;
  schema_version: string; started_at: string; completed_at: string;
  streams: {stream: string; state: string; reported_total: string|null;
    returned_items: string; content_gaps: string; accepted_captures: string}[];
}
type Family = 'people'|'users'|'stages'|'custom_fields'|'notes'|'tasks';
type Disposition = 'unchanged'|'changed'|'newly_observed'|'not_seen_again'|'unresolved';
type DispositionCounts = Record<Disposition,string>;
interface ReportRow {
  id: string; family: Family; source_id: string|null; disposition: Disposition;
  categories: string[]; reasons: string[];
  baseline_observations: string; newer_observations: string;
  components: { representation: string; disposition: Disposition;
    baseline_observations: string; newer_observations: string }[];
  evidence: { snapshot_id: string; capture_id: string; ordinal: number;
    side: 'baseline'|'newer'; stream: string }[];
  evidence_is_exhaustive: false; // representative references, never an evidence export
}
```

GET list requires `parent_import_id`, optional `cursor`, `limit` 1–20;
response `{reports:Report[],next_cursor:string|null}`. GET rows accepts optional
`family`, `disposition`, `cursor`, `limit` 1–50, and returns
`{rows:ReportRow[],next_cursor:string|null,output_revision:string}`. Detail row
returns `{row:ReportRow,output_revision:string}`. Rows/detail are available only
for completed sealed reports. Existing snapshot/import review navigation uses
`inputs.*.snapshot_id`, `parent_import_id`, `source_id` and evidence capture IDs.
No new plaintext viewer or value-bearing field is added.

Closed categories: `contact_information`, `assignment_stage`, `embedded_tags`,
`custom_fields`, `note_content`, `task_status`, `task_due_date`, `identity`,
`other_properties`. Category sets derive from complete canonical property
subsets, never clipped projections. Missing/null differences count. Task stream
membership changes also emit `task_status`. Notes list/detail remain separate.

Retained identity qualification admits at most 100 responses and 16 MiB aggregate
raw evidence before loading ciphertext; larger histories fail closed as ineligible.

## Persistence, work bounds and accounting

Migration timestamp starts `20260924000001`. Tables are
`migration_core_change_report`, `migration_core_change_group`,
`migration_core_change_variant`, `migration_core_change_receipt`,
`migration_core_change_reservation` and
`migration_core_change_note_key`. Tenant-composite foreign keys bind both
snapshots, original People import and group/receipt owners. One active report per
Org; one non-cancelled report per input tuple. Request identity is Org/request UUID,
actor and keyed digest of operation/body/frozen inputs/engine.

The worker keyset-scans captures once in sequence for each frozen snapshot;
individual pages contain at most 100 observations and 16 MiB raw input. Groups
accumulate bounded first semantic/category hashes, conflict flags, counts and up
to 16 references, so arbitrarily many variants cannot force a whole-group load.
A differing variant is never selected. Note request fingerprints are staged from
list IDs for exact restricted-detail linkage without an account rescan. Invalid or
unsupported captures/observations receive opaque group IDs and closed reasons;
rejected evidence makes the relevant enumeration uncertain. All originals remain
in their original captures. Second phase keyset-processes at most 50 groups;
final counts and immutable output revision seal atomically. Partial groups stay
private. Output cursors bind tenant/report/engine/input tuple/revision/filter/limit;
list cursors bind parent/first-page upper timestamp+ID and last key.

Lock order: shared workspace advisory guard, current active membership share
lock, Organization row, report row, newer snapshot and Org storage rows. Work
claims a random 60-second fenced lease; each commit revalidates initiator,
workspace/input binding, engine capability, lease and current storage ceilings.
A persisted lease epoch and token fence both failed claims and work settlement.
Statements are limited to 10 seconds and each worker unit to 45 seconds.
Expired leases are reclaimable by independent processes, with no process-local
identity used as progress state. Report reservations never share a source/import
owner. Separate 8 KiB cancellation reserve survives pause/storage exhaustion.

Retained accounting charges exact UTF-8 variable keys, encrypted nonce/ciphertext,
HMACs, and persisted text fields through report-owned byte triggers. Replacing a
checkpoint/group charges its net retained delta once; stateless transmitted
cursors occupy no retained storage. Unit admission reserves before a bounded
transaction; settlement releases only its own token. Both snapshot references are
retention/erasure dependencies. Additive inventory and release capability wiring
are coordinator-owned. Original import plans, CRM tables and workspace state are
read-only throughout this feature.

Errors use existing non-disclosing migration errors: 404 not found, 403 current
role denial, 409 incompatible inputs/state/storage/release, 400 invalid closed
query/body/cursor, and 503 unavailable key/corrupt retained data. All API responses,
including extractor/auth/error responses, have Cache-Control no-store.
