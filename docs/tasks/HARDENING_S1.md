# Hardening chunk S1 — `Resolution` + `PayloadFormat` enums

Parent plan: `docs/design/type-safety-hardening.md` (chunk 1). Branch:
`hardening-s1`. Single lane, one writer. Read `AGENTS.md` first.

## Goal

Replace the stringly-typed `raw_payload.resolution` and
`payload_format` values with exhaustive enums, and type
`insert_pending`'s three adjacent `&str` params — so a transposed
argument or typo'd state match becomes a compile error. STRICTLY
behavior-preserving except the one flagged change below.

## Hard invariants (violating any of these fails review)

1. **No SQL string changes.** Every `query!`/`query_as!` SQL literal
   stays byte-identical; bind enum values via `.as_str()`. The tracked
   `backend/.sqlx/` cache must be untouched (`git status` proof).
2. **No wire changes.** HTTP JSON responses carry the exact same
   strings (`as_str()` at the DTO boundary). No route contract edits.
3. **No DB changes.** No migrations; the CHECK constraints stay as
   defense-in-depth.
4. **No scope creep.** Only this chunk's files; no unrelated cleanup,
   no other newtypes (those are later chunks).

## The work

In `crm-app` (module placement: `domain/raw_payload/mod.rs` or
`store.rs`, implementer's choice — match house style):

1. `enum Resolution { Pending, Resolved, Unresolved, Discarded }`
   with `as_str()` and a fail-closed `parse(&str) -> Option<Self>`
   (mirror `RoutingStrategy`/`Origin` style, including doc comments).
   Confirm the exact value set against the DB CHECK in the migrations
   before writing it.
2. `enum PayloadFormat { GenericV1, Rfc822V1 }` likewise.
3. Decode at the row boundary: `LockedRawPayload`, `UnresolvedItem`,
   `DetailRawPayload` (store.rs:13,22,206 area) expose typed fields;
   the sqlx row structs may stay `String` internally with conversion
   in the mapping (per the parent doc's sqlx strategy).
4. Replace every string match/comparison on these values with
   exhaustive enum matches:
   - receive_inquiry.rs ~336 (`!= "pending"`), `duplicate_outcome`
     ~615 (see flagged change for its ~646 `unreachable!`);
   - workbench.rs ~163, ~236, ~268, ~308, ~361 — note the DUPLICATED
     payload_format match inside `retry_intake` (~268 vs ~308):
     collapse to one exhaustive dispatch so the retryable-check and
     dispatch can never diverge;
   - worker.rs ~515.
5. `insert_pending(source: &str, payload_format: &str, origin: &str)`
   → typed params: keep `source` as the existing validated `Source`
   type (`domain/inquiry/parse.rs`) or `&Source`, `payload_format:
   PayloadFormat`, `origin: Origin` (enum already exists). Fix both
   callers (receive.rs ~127 literal triple; receive_inquiry.rs ~242).
6. Tests: round-trip pin per enum (`..._round_trips_every_variant`,
   house style); a compile-orientation doctest is NOT needed for
   enums. Keep/adjust existing tests without weakening assertions —
   if a test asserted a string, it should still assert the same
   string through `as_str()`.

## Flagged behavior change (approved, implement exactly this)

`duplicate_outcome`'s `unreachable!` on an unknown resolution value
(receive_inquiry.rs ~646) becomes a fail-closed error on the existing
internal-error pathway (whatever `CommandError`/corrupt-data variant
the surrounding code already uses — do NOT invent a new public error
or HTTP shape). A corrupt DB value must yield a 500-class error, not
a process panic. Add/adjust one test pinning decode-failure →
fail-closed if the existing suite doesn't cover it.

## Checks before reporting done

`source ~/.nvm/nvm.sh && ./scripts/check` and `./scripts/check-db`
(db tests need `DATABASE_URL="$MIGRATION_DATABASE_URL"` from `.env`
when run directly; the scripts handle it themselves). `cargo fmt`
before the final run.

## Report

Files changed (must match `git status` exactly — undisclosed files
are a review failure), behavior delivered, checks run + results,
assumptions taken, anything that surprised you.
