# Today baseline fixture: `9d62e86`

This directory freezes the four Today-domain files from the repository state
`9d62e86`, immediately before Slice 011c added saved-list sources:

| File | SHA-256 of `git show 9d62e86:<source>` |
| --- | --- |
| `mod.rs` | `e12d99634193961aaaeb0269072947cb93b4a42ef502e3db21a0ea4994dfeaf3` |
| `model.rs` | `ec221474e73e5813e25b7823cf1cdba7d23244b2cac3dc87dcae98bb6b1f7f2e` |
| `queries.rs` | `c5daa311dfc770b49b229c346f392e6e3ff47c7e889de00fd4308d1d516cff62` |
| `rank.rs` | `e010357fc29a850ba93b8849f859b512cbbeb0a67094e6aea99856e395a97967` |

The only adaptations are Rust import paths needed because this is an
integration-test fixture: library imports use `crm_api::`, the copied query
imports its copied model through `super::model`, and the copied rank unit-test
imports use the corresponding fixture/library paths. The source SQL text,
aliases, DTOs, ranking, and original direct-query planning/JIT behavior are
otherwise unchanged. The original `sqlx::query_as!` macro remains intact; the
DB lane owns its offline metadata preparation.

`query` is public so another integration harness can load this fixture and
compare the original projection with HTTP output without copying the baseline.
