//! Today: a computed, deterministic read model (AGENTS.md §4.7, D-010;
//! docs/specs/SLICE_003.md §3, §4). Not a table — computed per request
//! from authoritative rows inside one statement, so there is no
//! projection lag and no second source of truth.
//!
//! This is the frozen Legacy-provider comparison fixture for Slice 011d
//! step 6 (deleting `TodayProvider::Legacy` and its seam), mirroring
//! `fixtures/today_9d62e86`'s own pattern: `queries.rs` freezes the
//! `candidates()` statement exactly as it stood at commit `f51bff8`
//! (immediately before Slice 011d began) — see `README.md`. Unlike
//! `today_9d62e86` (frozen from BEFORE Slice 011c added sources, so its
//! own `query()` never touches sources at all), this fixture's `query()`
//! deliberately covers ONLY the builtins/items comparison — the SAME
//! restriction `db_today_builtin_parity.rs` already uses for the 9d62e86
//! fixture (`sources` is asserted separately, never part of the frozen
//! side's own type). `rank()` and every model type this needs
//! (`TodayCandidate`, `TodayItem`, `TodayList`, …) are the SAME live types
//! `Feeds` still uses today — they were never Legacy-specific, so there is
//! no separate frozen copy of them.

pub mod queries;

use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::domain::today::rank::rank;
use crm_api::domain::today::TodayItem;
use crm_api::ids::UserId;

/// The frozen builtins-only projection: candidates, ranked, capped at 200
/// with `truncated`. No `sources` field — callers compare this against
/// `Feeds`' own builtins (its `items`/`truncated` before any list source
/// contributes) or restrict themselves to a zero-source `Feeds` result,
/// exactly as `db_today_builtin_parity.rs` already does for the older
/// `today_9d62e86` fixture.
pub async fn query(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    now: DateTime<Utc>,
) -> Result<(Vec<TodayItem>, bool), sqlx::Error> {
    let (candidates, truncated) = queries::candidates(conn, scope, viewer, now).await?;
    Ok((rank(candidates, now), truncated))
}
