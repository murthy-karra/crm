//! Opt-in fixed-fixture/fixed-clock baseline comparison for Mobile001.
use chrono::{DateTime, Utc};
use crm_api::{
    domain::{person::visibility::PersonVisibilityScope, today},
    ids::{OrganizationId, UserId},
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    if !database.starts_with("crm_mobile_") {
        return Err("performance fixture must be isolated crm_mobile_ database".into());
    }
    let actor: Uuid = sqlx::query_scalar("SELECT id FROM app_user WHERE email='agent@mobile.test'")
        .fetch_one(&pool)
        .await?;
    let org:Uuid=sqlx::query_scalar("SELECT organization_id FROM organization_membership WHERE user_id=$1 AND status='active' ORDER BY organization_id LIMIT 1").bind(actor).fetch_one(&pool).await?;
    let clock: DateTime<Utc> = std::env::var("MOBILE_PERF_CLOCK")?.parse()?;
    let mut old_samples = Vec::new();
    let mut new_samples = Vec::new();
    let mut digest = String::new();
    let mut items = 0;
    for round in 0..50 {
        // Alternate order in each pair to reduce ordering/cache bias.
        for baseline in if round % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            let connection = pool.acquire().await?;
            let start = std::time::Instant::now();
            let scope = PersonVisibilityScope::Organization(OrganizationId(org));
            let result = if baseline {
                today::mobile_001_baseline::query_owned_at(connection, &scope, UserId(actor), clock)
                    .await?
            } else {
                today::query_owned_at(connection, &scope, UserId(actor), clock).await?
            };
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if round >= 10 {
                if baseline {
                    old_samples.push(elapsed);
                } else {
                    new_samples.push(elapsed);
                }
            }
            let current: String = Sha256::digest(serde_json::to_vec(&result)?)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect();
            if !digest.is_empty() && digest != current {
                return Err("fixed-clock Today DTO differs in same-build pair".into());
            }
            digest = current;
            items = result.items.len();
        }
    }
    old_samples.sort_by(f64::total_cmp);
    new_samples.sort_by(f64::total_cmp);
    let allowance = 25.0_f64.max(old_samples[37] * 0.10);
    let passed = new_samples[37] <= old_samples[37] + allowance;
    println!(
        "{}",
        serde_json::json!({"baseline_commit":"9eaeb0a","same_build":true,"database":database,"clock":clock,"samples_per_side":40,"warmups_per_side":10,"alternating_order":true,"items":items,"dto_sha256":digest,"dto_equal":true,"old_median_ms":(old_samples[19]+old_samples[20])/2.0,"new_median_ms":(new_samples[19]+new_samples[20])/2.0,"old_p95_ms":old_samples[37],"new_p95_ms":new_samples[37],"allowed_increase_ms":allowance,"passed":passed})
    );
    if !passed {
        return Err("D-050 paired regression limit exceeded".into());
    }
    Ok(())
}
