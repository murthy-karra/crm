# Today baseline fixture: `f51bff8`

This directory freezes the Today "Legacy" builtins statement from the
repository state `f51bff8`, immediately before Slice 011d began (the
commit the coordinator named for this freeze).

| File | SHA-256 of `git show f51bff8:<source>` |
| --- | --- |
| `queries.rs` | `3d549eb48b52469afafddf5fe215fa05dfd9da6c39e8087f5ab6e09ce4b85eca` |

Diffed byte-for-byte against the CURRENT (pre-deletion)
`backend/crates/crm-app/src/domain/today/queries.rs` and
`backend/crates/crm-app/src/domain/today/rank.rs` before this freeze was
made: `queries.rs` and `rank.rs` were **identical** — Slice 011d's
`TodayProvider::Legacy` never touched either file, only added the
`Feeds` path alongside them. This confirms `queries::candidates` really
is exactly the statement Legacy always ran, right up to its deletion.

Because of that, this fixture only needs to freeze `queries.rs` itself.
`rank()` and every model type it depends on (`TodayCandidate`,
`TodayItem`, `PersonSummary`, …) are the SAME live types `Feeds` still
uses — they were never Legacy-specific (the only Legacy-era model
addition, `TodaySources.system_feed_issues`, is purely additive and
postdates this freeze; it is never part of what this fixture computes).
So unlike `fixtures/today_9d62e86` (which freezes `model.rs` and
`rank.rs` too, because THAT freeze predates Slice 011c's sources concept
entirely), this fixture's own `mod.rs` is hand-written, not copied: it
wires the frozen `queries::candidates` to the live `rank()` and returns
only `(items, truncated)` — no `sources` field, matching the same
restriction `db_today_builtin_parity.rs` already applies to the
`today_9d62e86` comparison (sources are asserted separately, never part
of the frozen side's own type).

The only adaptations to `queries.rs` are Rust import paths needed because
this is an integration-test fixture: library imports use `crm_api::`
instead of `crate::`. The source SQL text, aliases, and row-to-DTO
conversion logic are otherwise unchanged. The original `sqlx::query_as!`
macro remains intact; the DB lane owns its offline metadata preparation.

`query` is public so `db_today_feed_equivalence.rs` can load this fixture
and compare the frozen builtins projection with the live `Feeds` builtins
without copying the baseline.
