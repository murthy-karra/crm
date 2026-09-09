# Release profile and binary size (2026-09-09)

Coordinator-run measurement on `main` after Slice 016, on the development
Mac (Apple Silicon, Rust 1.98, `aarch64-apple-darwin`); the production
target will be Linux, so absolute sizes there will differ slightly, the
ratios will not. No deployment exists yet; this records the release
profile added to `backend/Cargo.toml` and why.

| Build | `crm-api` | `migrate` | Release build time |
|---|---|---|---|
| Cargo default release (no profile), unstripped | 17.7 MB | 4.5 MB | 66 s |
| the same, `strip` applied afterwards | 13.7 MB | | |
| `strip = true`, `lto = "fat"`, `codegen-units = 1` | 8.7 MB | 2.5 MB | 123 s |
| the above plus `opt-level = "s"` (**committed**) | 7.4 MB | 2.1 MB | 85 s incremental |

The development runtime's debug `crm-api` is 59.7 MB; it never ships. The
production Web bundle (`web/dist`) is 2.4 MB, 1.4 MB of it JavaScript.

`panic = "abort"` was deliberately not added: the Today query's savepoint
recovery and the test harness rely on normal unwinding, and the saving is a
few percent.

## What fills the binary (`cargo bloat --release --crates`, symbols kept for that run)

| Share of `.text` | Crate |
|---|---|
| 24% | `crm_app` (our domain: commands, queries, Today) |
| 16% | `axum` |
| 12% | `crm_api` (routes, Operator adapter) |
| 12% | `std` |
| 5% | `rustls` |
| 4% | `sqlx_postgres` |
| 3% | `reqwest` |
| 4% | `regex_automata` + `regex_syntax` |
| 2% | `ring` |
| 2% | `mail_parser` |
| 1–2% each | `crm_operator`, `tokio`, `sqlx_core`, `tracing_subscriber`, `hyper_util`, `hyper`, `webpki` |
| 8% | 86 further crates |

Reading: our own code and the framework are the bulk; no single dependency
is a removal candidate. `regex` (about 210 KiB) and `mail_parser` (about
90 KiB) are the only optional-looking entries and both are load-bearing
(intake parsing, correspondence capture). Going below roughly 7 MB means
feature-flag work on Axum, reqwest and SQLx for marginal gains; not
recommended under D-050.

`cargo bloat` (v0.12.1) is installed on the development machine for
future measurements; run it with `CARGO_PROFILE_RELEASE_STRIP=false` since
the committed profile strips symbols.
