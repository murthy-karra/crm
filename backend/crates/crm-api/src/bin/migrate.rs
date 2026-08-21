//! Applies pending migrations as crm_migrator. Wrapped by scripts/db-migrate.
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let url =
        std::env::var("MIGRATION_DATABASE_URL").map_err(|_| "MIGRATION_DATABASE_URL is not set")?;

    let pool = PgPoolOptions::new().connect(&url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    println!("migrations applied");
    Ok(())
}
