//! Direction-ladder gathering reads (docs/specs/SLICE_009.md §5): whether
//! an address belongs to an ACTIVE member's login (step 1 / the held-queue
//! direction-hint fallback), kept apart from `domain::contact::identify`
//! (the existing org-scoped Person dedup lookup, reused as-is for steps 2
//! and 3 — see `domain/capture/pipeline.rs`).

use sqlx::PgConnection;

use crate::ids::OrganizationId;

/// Step 1's condition and step 4's counterparty-fallback filter: is
/// `email` (already lowercased by the caller) an ACTIVE member's login in
/// this Organization. Case-insensitive on the stored side too
/// (`app_user_email_lower_idx`), so the caller need not pre-normalize
/// beyond lowercasing.
pub async fn is_active_member_email(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    email: &str,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!"
           FROM organization_membership m
           JOIN app_user u ON u.id = m.user_id
           WHERE m.organization_id = $1 AND m.status = 'active' AND lower(u.email) = lower($2)
           LIMIT 1"#,
        organization_id.0,
        email,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}
