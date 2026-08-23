//! `SetLocalPassword` (docs/specs/SLICE_004.md §4). CLI only — no route.

use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::password;
use crate::domain::admin::commands::AdminCommandError;
use crate::domain::admin::queries;
use crate::domain::admin::validation;

pub struct SetLocalPassword {
    pub user_id: Uuid,
    pub password: String,
}

/// Argon2id re-hash; `UPDATE local_credential`. No fact — credential
/// material is not a business fact (D-015). Runs as `crm_app`
/// (docs/specs/SLICE_004.md §4, §11: `crm-admin set-password`).
pub async fn set_local_password(
    pool: &PgPool,
    cmd: SetLocalPassword,
) -> Result<(), AdminCommandError> {
    validation::validate_password(&cmd.password)?;

    let candidate = cmd.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || password::hash_password(&candidate))
        .await
        .map_err(|_| AdminCommandError::Crypto)?
        .map_err(|_| AdminCommandError::Crypto)?;

    let mut conn = pool.acquire().await?;
    queries::update_local_credential(&mut conn, cmd.user_id, &password_hash).await?;
    Ok(())
}
