# Migration coverage completion — bounded implementation brief

**Status:** accepted bounded implementation scope, 2026-09-17. This document
owns only the static coverage taxonomy in
`backend/crates/crm-app/src/domain/migration/coverage_inventory.rs` and its unit
tests. It makes no shared API/persistence contract change and authorizes no source
call, customer processing, activation, deployment, merge or push.

The inventory is implemented and integrated. Final verification is recorded in
the [integration evidence](../reviews/migration-completion-2026-09-17.md).

## Decision and implementation baseline

The source of truth is D-012, D-015, D-050 and D-059–D-092. The deployed
`010g1` package already provides repeatable, guarded mutations for the admitted
families: whole-Person metadata deltas, note/task deltas, and immutable
event/call/text corrections. It also preserves existing-Person repair and
never-imported-Person recovery. Its release is recorded in
[SLICE_010g1_RELEASE.md](SLICE_010g1_RELEASE.md).

This work supplies the static vocabulary that a separate reconciliation reader
uses to describe retained source and destination boundaries. It does not account
for a particular Organization, determine a record outcome, turn an absence into
a deletion, expand activation, or make a cutover claim.

The final acceptance fixture is synthetic only. Live FUB qualification, any
customer book, source authorization/registration, production readiness, and
activation remain deferred.

## Current coverage inventory

| Family / concern | Existing durable path | Current honest status | Required final-report treatment |
| --- | --- | --- | --- |
| People, contacts, stages, source users | `010b` retained `fub-core-v1` snapshot; `010c`, `010e2`, `010e3`, `010e5`, `010e6` and `010g1` result/head records | Implemented for qualified capture and the accepted import/refresh/recovery cohorts; no assertion that the live account was fully visible | Reconcile each retained source identity to one exclusive terminal destination disposition; show source enumeration scope separately |
| Embedded Person tags | `010b` People evidence; `010f1`/admitted metadata/`010g1` typed tag and link results | Implemented as Person-embedded membership, subject to native limits and holds | Report as `embedded_person_tags`; never relabel as a standalone catalog |
| Standalone tag catalog | No `fub-core-v1` stream, source ID or documented endpoint profile | **Unqualified and not captured** | Always report `unqualified_source`; do not count embedded values as catalog coverage or schedule an import |
| Custom fields and values | `custom_fields` capture plus `010f1`/admitted metadata/`010g1` catalog and value heads | Implemented for qualified compatible values; recurrence, invalid forms, collisions, local conflicts and bounds can be held | Reconcile definition/option/value/link outcomes separately, retaining exact holds |
| Notes and tasks | `010b` notes/list-detail and open/completed-task captures; `010f2`, admitted activity and `010g1` heads | Implemented for captured, compatible source rows; restricted notes and unsupported/reply/attachment forms remain explicit | Reconcile source ID and representation/partition, with a retained-source gap distinct from an import hold |
| Events, calls and texts | `010d1` capture, `010d2` and admitted-history/`010g1` immutable import/correction heads | Implemented metadata-first only; FUB documents restricted API visibility for some calls/events | Reconcile identities and versions, keeping inaccessible/restricted evidence as coverage gaps; no body/media claim |
| Addresses, relationships, appointments, deals | Some embedded bytes may be retained in People payloads; no complete destination model/import | Preserved evidence may exist, but usable import coverage is missing | `preserved_unrepresented` when bytes exist; otherwise `not_captured`/`unknown` |
| Email, recordings, external files | Not in the core source profile or destination import; D-062 governs future email bulk placement | Not captured/imported; email/media source and storage work remain separate | `not_captured`, with no zero count inferred |
| Automation/settings and other account configuration | No capture/import path | Not captured | `not_captured` |
| Source deletions and full synchronization | No accepted deletion semantic; `updatedAfter` is not a child-family change feed | Unknown | `unknown_deletion_semantics`; a later missing source ID cannot be “deleted” |
| Privacy/erasure and activation | O-012/O-013 and later activation remain open | Not ready for customer use | Report prerequisite blockers, never mark the workspace activation-ready |

The existing core `coverage()` implementation deliberately declares tags,
addresses, relationships, appointments, deals, automation/settings and external
files `not_captured`. The metadata import explicitly emits
`embedded_tags_only`; its source parser has no standalone tag ID. These are
correct existing boundaries, not defects to erase.

## Standalone-tag qualification result

No standalone-tag support belongs in this implementation. The current official
FUB People reference documents `tags` as a Person response/filter field, and
the current public API reference discovered no `GET /tags` catalog endpoint.
The existing public qualification record reaches the same conclusion. A
standalone tag catalog therefore has neither a documented collection boundary,
stable source identity, pagination/exhaustion contract, nor deletion semantics.

If FUB later publishes a qualified catalog endpoint, that is a separate source
profile change: capture its public schema/version/hash, collection and cursor
rules, stable identity, access limits and deletion behavior; revise the
coverage matrix; then obtain shared-contract approval before extending the
closed `Family`/`Stream` enums. This brief does not authorize that work.

Source evidence: [People API](https://docs.followupboss.com/reference/people-get),
[common filters](https://docs.followupboss.com/reference/common-filters), and
[current local source qualification](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md).

## Accepted static taxonomy

`coverage_inventory::inventory()` returns an immutable, stable-order slice of
`CoverageDescriptor` values. Each descriptor has a `family`, a `path`, its
`source_evidence`, its `destination_surface`, and safe `blocker_codes`.

`CoveragePath` is the report vocabulary: `implemented`, `metadata_only`,
`preserved_unrepresented`, `unqualified_source`, `not_captured`, `unknown`, or
`deferred_prerequisite`. It describes the product boundary, never account-level
enumeration, byte retention, per-record disposition, or activation readiness.
Those values remain dynamic reconciliation concerns.

The taxonomy deliberately makes these distinctions non-negotiable:

- `embedded_person_tags` is an `implemented` typed-import path backed only by
  a Person record and carries `embedded_person_tags_only`.
- `standalone_tag_catalog` is `unqualified_source` with no available destination
  path. It has no fabricated source total, vendor identity, endpoint or import.
- events, calls and texts are `metadata_only`; their entries cannot imply body or
  media coverage.
- addresses and relationships are `preserved_unrepresented` only when returned
  inside a retained Person record. Appointments, deals, email, recordings,
  external files and automation settings are `not_captured`.
- source deletion semantics stay `unknown`; activation and privacy erasure are
  `deferred_prerequisite` blockers.

The module has no database, route, source-reader, worker, control-table or Web
dependency. The reconciliation lane may serialize descriptors unchanged beside
its Organization-specific counts, but it must not derive a readiness flag or
reclassify a family.

## Checks

Run only the module unit tests and formatting in this lane, with an isolated
Cargo target directory:

1. `cd backend && CARGO_TARGET_DIR=/private/tmp/migration-coverage-target cargo fmt --check`
2. `cd backend && CARGO_TARGET_DIR=/private/tmp/migration-coverage-target cargo test -p crm-app coverage_inventory`
3. `cd backend && CARGO_TARGET_DIR=/private/tmp/migration-coverage-target cargo clippy -p crm-app --all-targets -- -D warnings`

The tests lock unique family keys, a no-false-readiness constraint, the
embedded-vs-standalone-tag distinction, metadata-only history, unrepresented
or not-captured destination gaps, deferred prerequisites and exact JSON wire
vocabulary.

## Non-goals

No FUB request, customer data, schema change, route, Worker, report accounting,
business mutation, activation, cutover, deletion or automated repair is in this
scope. A future source catalog or destination model needs its own qualification
and shared-contract approval.
