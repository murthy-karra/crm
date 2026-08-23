//! `GrantPlatformAdmin` (docs/specs/SLICE_004.md §4). CLI only, migrator
//! connection — the only subcommand that uses `MIGRATION_DATABASE_URL`
//! (docs/specs/SLICE_004.md §11), because `crm_app` has SELECT-only on
//! `platform_admin` (it must not be able to mint one).

use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::password;
use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries;
use crate::domain::admin::validation;

pub struct GrantPlatformAdmin {
    pub email: String,
    pub display_name: String,
    pub password: String,
}

/// Create `app_user` + `local_credential` if absent (same normalization as
/// elsewhere), insert `platform_admin` if absent; idempotent. No actor and
/// no Organization, so this is recorded in the `platform_admin` row itself
/// (`granted_at`, `granted_via`), not as a fact (docs/specs/SLICE_004.md
/// §2, §4).
pub async fn grant_platform_admin(
    pool: &PgPool,
    cmd: GrantPlatformAdmin,
) -> Result<Uuid, AdminCommandError> {
    let email = validation::normalize_email(&cmd.email)?;
    let display_name = validation::validate_display_name(&cmd.display_name)?;
    validation::validate_password(&cmd.password)?;

    let candidate = cmd.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || password::hash_password(&candidate))
        .await
        .map_err(|_| AdminCommandError::Crypto)?
        .map_err(|_| AdminCommandError::Crypto)?;

    let user_id = queries::find_or_create_app_user(pool, &email, &display_name).await?;
    queries::upsert_local_credential(pool, user_id, &password_hash).await?;
    queries::insert_platform_admin_if_absent(pool, user_id).await?;

    Ok(user_id)
}
