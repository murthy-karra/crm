# Type-Safety Hardening Ladder

Status: SURVEYED 2026-08-26 (full-backend planner sweep at `main`
`105f730`); ladder accepted as the working plan. One chunk at a time,
each behavior-preserving and independently landable, gates
(`./scripts/check` + `./scripts/check-db`) green after every chunk.
External JSON contracts and the DB schema stay byte-identical
throughout — this ladder changes what COMPILES, never what runs.

The principle (house precedent: `SenderTrust`, `IntakeActor`,
`RoutingStrategy`): make invalid states unrepresentable, so whole bug
classes die at compile time instead of surviving to runtime, review,
or production.

## Why (the found hazards, abbreviated)

- **Bare `Uuid` confusion:** 60 functions take ≥2 bare `Uuid`s; both
  `(org, x)` and `(x, org)` argument orders coexist in the codebase, so
  a habituated caller can swap them and it compiles. Worst sites:
  `crypto::seal/open` (the org half of the AAD tenant binding),
  `RealtimeEvent::call_changed` (three trailing adjacent ids),
  `realtime::token::mint` (user-first against the org-first
  convention), `determine_routing` (two adjacent `Option<Uuid>` user
  ids with different routing semantics).
- **Stringly-typed states:** `raw_payload.resolution` and
  `payload_format` are `String` end-to-end, matched as literals in six
  places (including an `unreachable!` panic on unknown resolution —
  against the fail-closed posture); `insert_pending` takes THREE
  adjacent `&str`s (`source, payload_format, origin`) and both intake
  entry points pass literal triples — a transposition compiles.
- **Actor pair:** `FactEnvelope` carries `actor_kind` +
  `actor_user_id: Option<Uuid>` as pub fields, literal-constructed in
  7 files; `(System, Some(user))` and `(User, None)` are representable
  and only a DB CHECK (runtime 500) catches them.
- **Contact dedup keys:** `identify(conn, org, email: Option<&str>,
  phone: Option<&str>)` — adjacent same-typed options; a swap silently
  breaks dedup and creates a duplicate Person.
- **`intake_token` is a plain `String`** (a tenant credential; a
  stray `{:?}` on the `(Uuid, String)` tuples it rides in would leak
  it). `Publication { channel, event }` has pub fields, so a cross-org
  realtime publish is representable.
- Already healthy (no chunks needed): fact vocab enums exist
  (strings leak only at fact-struct boundaries), realtime events are
  typed with exact-shape pins, operator tool schemas are
  snapshot-pinned, config secrets all have redacted newtypes.

## sqlx strategy (applies to every id chunk)

Newtypes are `#[repr(transparent)]`, `Copy`, serde-transparent, in a
new `crm-app/src/ids.rs`. **Bind `.0`** at `query!` sites — keeps the
macros' full typechecking, keeps every SQL string byte-identical, so
the tracked `.sqlx` cache is untouched and offline checks stay valid.
DB-row structs stay on bare `Uuid`; conversion happens at the public
query-fn boundary. crm-operator keeps bare `Uuid` at the tool seam
(the D-028 §5 crate fence; crm-api converts at its backend impl) — a
shared ids crate was considered and rejected (new crate, fence
change). Each confusion pair gets a ```compile_fail doctest; each
enum gets a round-trip pin.

## The ladder

| # | Chunk | What | Size |
|---|-------|------|------|
| 1 | **S1** | `Resolution` + `PayloadFormat` enums; `insert_pending` takes typed `(Source-ish, PayloadFormat, Origin)`; `unreachable!` → fail-closed `Corrupt`. ~6 files. | S |
| 2 | **N1** | `OrganizationId` — the tenant key, everywhere in crm-app + crm-api edges (~35–40 files, ~96 binds). Highest absolute payoff. May sub-split crm-app / crm-api. | M/L |
| 3 | **N2** | `UserId` (~20 files) — kills the most frequent adjacency (user-vs-org). N1 review carry-overs: type the session layer below `AuthContext` (`session::create`, `SessionOrganization.id` — the login-path user/org adjacency still compiles until then, mitigated by the membership-join 401); consider typing `OrganizationRef.id`/`PlatformOrganizationItem.id` to remove typed→bare→typed chains. | M |
| 4 | **A1** | `Actor` enum (`User(UserId) \| System`) replacing the kind+option pair in `FactEnvelope`; DB CHECK demoted to defense-in-depth. | S/M |
| 5 | **N3** | Intake cluster: `PersonId`, `InquiryId`, `RawPayloadId`, `StageId` (~25 files). | M |
| 6 | **N4** | Call cluster: `CallId`, `ContactMethodId`, `ProposalId`, `CorrelationId`, `TurnId`, `InvitationId` (~20 files). | M |
| 7 | **V1** | `NormalizedEmail` / `NormalizedPhone` — constructible only via the normalize fns; kills the `identify` swap (duplicate-Person bug). ~6 files. | M |
| 8 | **S2** | Typed fact fields + extraction ledger `AttemptOutcome` (retryability can't diverge from tags) + `IntakeToken` (redacted Debug, constant-time-only verify) + constructor-only `Publication` + S1 review carry-over: map `sqlx::Error::Decode` → `Corrupt` in the `From<sqlx::Error>` impls so row-boundary decode failures 500 instead of 503. Splittable. | M |

Dependencies: S1 and V1 are free-standing; N2..N4 build on N1's
module/pattern; A1 after N2 (holds `UserId`); S2 after S1+N1.

Recorded residuals (LATER, from the N2 review — a shared newtype
cannot distinguish SAME-role parameters): `determine_routing`'s two
adjacent `Option<UserId>` params remain transposable (smallest fix: a
small params struct, fold into A1 or N3); `routes/platform.rs`'s
`TwoUuidPathIds` positional org/user path pair (typed path extractor,
later); the CLI/migrator bootstrap cluster stays bare-Uuid by design
(no adjacency; type it if it ever grows one, closing N2's
`find_or_create_app_user` unwrap). From N4/V1 (2026-08-26):
`today/model.rs`'s `CallOutcomeNeeded.call_id`/`OutcomeNeededCall.
call_id` stay bare (no adjacency; type when Today files next open);
`ParsedLead.email/phone` can be fully retyped once the identify
integration lands (zero non-test readers remain) — deleting the
re-deriving accessors; a construction-monopoly compile_fail doctest
on NormalizedEmail is worth adding then too.

**Recommended order: as numbered. Start with S1** — smallest,
zero-wire-risk, kills a real compiling transposition immediately, and
establishes the decode-at-row-boundary pattern every id chunk reuses.

## Flagged for sign-off in the relevant chunk's gate

1. S1: corrupt-DB `resolution` changes from `unreachable!` panic to a
   fail-closed `CommandError::Corrupt` (500) — a failure-behavior
   change toward the documented posture.
2. S2: `IntakeToken` makes constant-time comparison the only equality
   on the inbound auth path; the dummy-token timing equalizer in
   `receive.rs` must be preserved exactly — security-sensitive review.
3. Rejected alternative, revisit only if ever needed: shared
   `crm-ids` crate to type the crm-operator seam.

## Process

Each chunk: its own short branch (`hardening-s1`, `hardening-n1`, …),
implemented against this doc (no per-chunk spec unless a chunk grows
surprises), gates green, independent review for the flagged chunks
(S1, S2) and any chunk that ends up touching a frozen contract,
commit/merge with user approval per AGENTS Phase 9. Feature slices
may interleave between chunks; this ladder never blocks product work.
