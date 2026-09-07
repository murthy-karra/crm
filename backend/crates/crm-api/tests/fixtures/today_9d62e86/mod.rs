//! Today: a computed, deterministic read model (AGENTS.md §4.7, D-010;
//! docs/specs/SLICE_003.md §3, §4). Not a table — computed per request
//! from authoritative rows inside one statement, so there is no
//! projection lag and no second source of truth.

// The frozen fixture deliberately preserves the original public surface even
// where a particular parity test only consumes TodayList. Keep lint handling
// at this adapter boundary so the copied source remains behavior-identical.
#[allow(dead_code)]
pub mod model;
pub mod queries;
pub mod rank;

#[allow(unused_imports)]
pub use model::{
    InquiryRef, RecommendedAction, TodayCandidate, TodayItem, TodayList, TodayPriority,
    TodayReason, FRESH_INQUIRY_WINDOW_HOURS,
};
pub use rank::rank;

use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::ids::UserId;

/// What a Slice 005 Operator tool calls — never a separate path
/// (docs/specs/SLICE_003.md §3).
pub async fn query(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    now: DateTime<Utc>,
) -> Result<TodayList, sqlx::Error> {
    let (candidates, truncated) = queries::candidates(conn, scope, viewer, now).await?;
    let items = rank(candidates, now);
    Ok(TodayList {
        generated_at: now,
        items,
        truncated,
    })
}
