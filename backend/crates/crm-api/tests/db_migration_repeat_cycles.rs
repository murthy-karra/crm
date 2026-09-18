//! Three retained captures on one original Person prove repeat-refresh safety.
use sqlx::PgPool;

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn repeat_refresh_cycles_preserve_heads_holds_absence_and_remainders(pool: PgPool) {
    crate::db_family_refresh_commands::activity_execution_scenario(pool, 4).await;
}
