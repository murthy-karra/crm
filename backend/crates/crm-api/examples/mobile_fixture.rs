//! Retained synthetic native-app fixture. Never clears an existing database.
//! Run with --features test-support and the mobile lane's private .env.
#[path = "../tests/common/mod.rs"]
mod common;
use sqlx::PgPool;
use uuid::Uuid;
const PASSWORD: &str = "Mobile-demo-only-123!";
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = std::env::var("MIGRATION_DATABASE_URL")?;
    let migrator = PgPool::connect(&database).await?;
    let name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&migrator)
        .await?;
    if !name.starts_with("crm_mobile_") {
        return Err("mobile fixture requires an isolated crm_mobile_ database".into());
    }
    sqlx::migrate!("./migrations").run(&migrator).await?;
    let app = common::connect_as_app(&migrator).await;
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM app_user WHERE email='agent@mobile.test'")
            .fetch_optional(&app)
            .await?;
    let (org, actor) = if let Some(actor) = existing {
        let org:Uuid=sqlx::query_scalar("SELECT organization_id FROM organization_membership WHERE user_id=$1 AND status='active' ORDER BY organization_id LIMIT 1").bind(actor).fetch_one(&app).await?;
        (org, actor)
    } else {
        let (org, actor) = common::create_org_with_stages_and_member(
            &migrator,
            "Mobile Synthetic Demo",
            "agent@mobile.test",
            "Field Agent",
            PASSWORD,
        )
        .await;
        let second =
            common::create_user(&migrator, "second@mobile.test", "Second Agent", PASSWORD).await;
        common::add_membership_with(
            &migrator,
            org,
            second,
            crm_api::domain::admin::Role::Member,
            crm_api::domain::admin::MembershipStatus::Active,
        )
        .await;
        common::create_org_with_stages_and_member(
            &migrator,
            "Mobile Foreign Fixture",
            "foreign@mobile.test",
            "Foreign Agent",
            PASSWORD,
        )
        .await;
        let stage: Uuid = sqlx::query_scalar(
            "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
        )
        .bind(org)
        .fetch_one(&app)
        .await?;
        let mut tx = app.begin().await?;
        sqlx::query("INSERT INTO person(organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT $1,'Mobile Person',lpad(n::text,3,'0'),$2,$3 FROM generate_series(1,100)n").bind(org).bind(stage).bind(actor).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value) SELECT $1,id,'email','person'||last_name||'@synthetic.test','person'||last_name||'@synthetic.test' FROM person WHERE organization_id=$1").bind(org).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO note(organization_id,person_id,author_user_id,body,origin,correlation_id) SELECT $1,p.id,$2,'Synthetic field note '||n,'web_session',gen_random_uuid() FROM person p CROSS JOIN generate_series(1,10)n WHERE p.organization_id=$1").bind(org).bind(actor).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,due_at,created_by_user_id,assignee_user_id,origin,correlation_id) SELECT $1,p.id,'Synthetic follow-up '||n,'follow_up',now()+make_interval(hours=>n-5),$2,$2,'web_session',gen_random_uuid() FROM person p CROSS JOIN generate_series(1,10)n WHERE p.organization_id=$1").bind(org).bind(actor).execute(&mut *tx).await?;
        tx.commit().await?;
        (org, actor)
    };
    let first: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM person WHERE organization_id=$1 ORDER BY last_name,id LIMIT 1",
    )
    .bind(org)
    .fetch_optional(&app)
    .await?;
    println!(
        "{}",
        serde_json::json!({"database":name,"organization_id":org,"actor_user_id":actor,"first_person_id":first,"login_email":"agent@mobile.test","second_login_email":"second@mobile.test","synthetic_password":PASSWORD,"preserved_existing":existing.is_some()})
    );
    Ok(())
}
