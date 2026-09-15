# Slice 010f4 concrete contract

This checkpoint implements the accepted D-084 010f4 contract. It is additive to
010f2 and does not alter original activity rows, source keys, or mutation DTOs.

## Boundary and ownership

`POST /api/migrations/fub/admitted-activity-imports` accepts
`{request_id,admission_id,report_id}`. The server qualifies a terminal completed
or cancelled `migration_people_admission`, its immutable confirmed admission
plan/result cohort, and one completed same-account core-change report. The report
must be the admission report or begin strictly after the admission capture
completed. Its snapshot must have exhausted `people`, `users`, `notes`,
`note_detail`, `tasks_open`, and `tasks_completed` streams.

The implementation uses the existing 010f2 retained reader and interpreter
limits: at most 4 MiB and 100 records per raw representation. List/detail source
reads remain encrypted, no-store, page-bounded to 50 rows and 512 KiB. The same
normalization, HTML conversion, role, kind, exact-time, and source-pair equality
rules apply to admitted records.

## Schema and atomic writer

`migration_admitted_activity_import` retains its workspace parent tuple and adds
`admission_id`, immutable `admission_plan_id`, `source_report_id`, output
revision, selected snapshot/capture sequence and optional predecessor. New plan,
choice, source, mapping, manifest, result, issue, receipt, and reservation tables
are all tenant-keyed and use the existing 64 MiB unit reservation / 60-second
lease limits.

`migration_activity_identity` remains globally unique on
`(organization_id, source_account_id, kind, source_id)`. Existing rows keep only
`import_id/plan_id/manifest_id`; an admitted identity has those three columns null
and carries only `admitted_import_id/admitted_plan_id/admitted_manifest_id`. The
exclusive-owner CHECK and composite FKs reject mixed or ownerless rows. The
replacement identity measurement function charges original rows to their existing
activity root and admitted rows to their admitted root.

The private `crm_admitted_activity_insert_allowed` permit accepts only an exact
confirmed manifest, active current admin, migration-review workspace, fenced lease,
native UUID/source pair and INSERT into `note` or `task`. It cannot update/delete
native rows or authorize ordinary commands. Native insert/equality check, identity,
encrypted result, counter/revision, and reservation settlement occur in one
transaction. A conflict, tombstone, target change, or local edit is held.

## Reads, capability, and HTTP

The capability is `fub-admitted-activity-v1`. Preparation/confirmation and the
original activity worker's preparation, claim, equality, and settlement paths must
require the admitted-owner schema when it is present. First confirmation adds the
same durable complete-activity reader barrier, including a cancelled zero-write
attempt. Bounded review computes one checked decimal revision from original plus
admitted activity counters. Its four note/task provenance joins select the one
exclusive identity owner; an admitted commit invalidates a cursor created before
that commit.

The route family has typed prepare, replan, confirm, retry, cancel, remainder,
list/detail/records/mappings/results/targets/observations/field reads. All body
limits are 64 KiB; request identity replay is authoritative. Errors preserve the
existing precedence: unauthenticated 401, member 403, foreign/missing 404,
malformed 400, stale/incompatible 409 and unavailable retained evidence 503.
