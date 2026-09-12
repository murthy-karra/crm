# Slice 010f2 — Code and source contracts for notes/tasks planning

2026-09-11; inspected main `cd3b010`, after deployed 010f1 `e36ce36`. Read-only
research for [the proposed spec](../specs/SLICE_010f2.md) and
[brief](../tasks/SLICE_010f2_IMPL.md). This is evidence, not accepted new policy.
No FUB credentials, customer records, DB operations or runtime changes were used.

## Retained source

[010b public qualification](SLICE_010b_FUB_SOURCE_CONTRACT.md) and its
[schema hash manifest](SLICE_010b_SOURCE_SCHEMA_SHA256.json) distinguish public
documentation from live source guarantees. The saved notes-list, note-detail and
tasks schemas still match that manifest. Several examples are not valid JSON;
synthetic fixtures must not be represented as live captures.

- `snapshot_source.rs:112–125` freezes distinct notes/list and
  notes/detail/replies,reactions representations; both task partitions share
  tasks/default. Lines 159–201 define bounded requests: notes list, note detail
  with both include flags, tasks open and completed explicitly. Lines 450–532
  show selected fields, not a guarantee every source response supplies them.
- Full raw records are preserved before interpretation. Projection fields can
  be clipped/omitted; child execution must requalify raw encrypted captures.
  Existing `snapshot_compare.rs` task preview checks title/kind/due/assignee/state,
  not a complete import schema or reliable completion timestamp/actor.
- `snapshot_worker.rs:272–277,463–469` commits restricted note-detail404 as a
  settled inaccessible-content gap. No native deletion follows. Its retained
  request fingerprint identifies the requested note even without a record row.
- [Notes list](https://docs.followupboss.com/reference/notes-get) and
  [detail](https://docs.followupboss.com/reference/notes-id-get) examples include
  subject/body/isHtml and author names/IDs; detail adds separately identified
  replies and reactions. List/detail content equality is not qualified, and
  reactions have inconsistent public example shapes. Preserve these as source
  data; never complete a detail record from a list projection.
  [Threaded replies](https://docs.followupboss.com/reference/threaded-replies) and
  [reactions](https://docs.followupboss.com/reference/reactions) remain separate
  source structures; neither proves a native reply/reaction capability.
- [Tasks](https://docs.followupboss.com/reference/tasks-get) documents separate
  completion filtering. Its example has numeric `isCompleted:1` and a distinct
  `completed` timestamp later than `updated`, plus created/updated author names
  and a numeric assignee ID. It does not establish `updatedBy` as completer.
  Optionality and partition completeness still require later live qualification.
- The task example carries both a calendar `dueDate` and offset-bearing
  `dueDateTime`. D-067 explicitly chooses confirmed-source-zone end of day for
  date-only tasks; source examples do not authorize an implicit UTC/default zone.
  [Task field documentation](https://docs.followupboss.com/reference/tasks-post)
  describes date and timezone-bearing timestamp input independently.
- [Tasks Overview](https://help.followupboss.com/hc/en-us/articles/360014274453-Tasks-Overview)
  documents undated tasks and tasks assigned to users without access to the
  Person. A task record is not authority to create a missing parent Person.
  [Recurring Tasks](https://help.followupboss.com/hc/en-us/articles/4402371808407-Recurring-Tasks)
  describes Action Plan-generated work; existing task rows do not prove future
  scheduled/recurring task coverage.

Public evidence supports conservative representability rules; it does not prove
the user's account has complete notes, replies, tasks or source permissions.

## Native models and explicit contract gaps

| Existing code | Verified behavior and planning implication |
|---|---|
| `domain/note/model.rs:49–83` | Plaintext NoteBody normalizes CRLF/trim, accepts 1–10,000 code points, rejects other controls except newline/tab. HTML conversion/subjects are new import policies, not existing native behavior. |
| `domain/task/model.rs:26–41,77–94` | Closed five-kind Task enum, title only, nullable due/actors/completion timestamp. No description, recurrence, independent completed-with-unknown-time or source author display identity. |
| `migrations/20260912000001_note.sql`, `20260913000001_task.sql` | Scoped native Person/member FKs, nullable historical actors for migration origin, unique Org/source/external key, tombstone text erasure. The native source key has no separate account column; draft explicitly owns account-qualified encoding and legacy-key holds. |
| `domain/note/commands.rs:147`, `domain/task/commands.rs:159` | Ordinary commands require operational workspace and current actors/defaults/time; calling them unchanged cannot faithfully import historical activity. New private typed INSERT operations must preserve these ordinary contracts. |
| `domain/note/queries.rs` | Native note history fetches every live body for the Person; Operator latest reads are separately bounded. No source/import path exists. |
| `domain/task/queries.rs:378`, `domain/person/queries.rs:1565` | Open tasks and aggregate history are unpaginated. Hundreds of imported bodies motivate a separate bounded review representation, not a silent first-N import cap. |
| `crm-api/src/routes/people.rs:241` | Existing Person detail fetches full history/open tasks and decorates can_manage. New review DTO/pages must explicitly scope history and suppress ordinary actions; older full readers must fail before loading imported activity. |

Paths above are under `backend/crates/crm-app/src/` except the API paths and
migrations, which are under `backend/crates/crm-api/`. All referenced source
files were inspected at the baseline; line numbers are retrieval aids, not a
claim about future implementation locations.

## Coordinator, atomicity and recovery reuse

`domain/migration/metadata.rs:118–146` checks completed parent, exact snapshot/
account/final sequence/preview and original review binding. Its required streams
are People/users/stages/custom_fields; the activity child must qualify its own
notes/detail/open+completed-task streams instead. Metadata's raw grouping assumes
stream/family equality in places; that assumption is invalid for these sources.

`metadata_worker.rs:763` verifies the parent's identity/result/native Person
agreement. Activity should use that invariant, but one source note/task per
transaction: the metadata unit's bounded tag/field fan-out does not bound a
Person's activity history. `imports.rs:203` supplies workspace/membership/Org
locking; `metadata_worker.rs:77` and `metadata_store.rs:93,182` show fenced claims,
current ceilings/settlement and scoped immutable receipts. Shared ledger/Org locks
can serialize siblings; cancellation must release only the owning child units.
Native row bytes are separate from retained-source accounting.

`20260919000001_fub_metadata_import.sql:166` adds a narrow native INSERT permit to
the existing workspace guard. Activity needs its own closed note/task targets,
executor/lease/parent checks without widening metadata or ordinary write powers.
Identity tombstones must survive native deletion and contain no source text.

`auth/workspace.rs:67` wraps reads in a transaction with a shared workspace guard.
An activity confirmation that introduces bounded-only review must take the
existing exclusive barrier; an Org row lock alone does not serialize a legacy
reader's check-then-fetch. Keep the confirmed-child boundary after cancellation
and completion. Old 010f1 binaries enforce the review hold but retain unbounded
activity readers, so activity-capable forward recovery is a real added constraint.

D-053 explicitly limits body exposure sites; a paged native review body endpoint
requires the declared amendment in the full specification, even though its
audience remains admin-only. D-015 erasure readiness and live FUB qualification
remain open. Planning establishes neither tested implementation nor deployment.
