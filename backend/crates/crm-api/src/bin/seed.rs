//! Idempotently seeds two Organizations, each with the nine D-019 default
//! stages and two local-auth Users (a second member so reassignment has a
//! target — docs/specs/SLICE_002.md §1, §14 default 10). Connects via
//! MIGRATION_DATABASE_URL so crm_app never needs INSERT on identity/stage
//! tables (docs/specs/SLICE_001.md §3; docs/specs/SLICE_002.md §2). Wrapped
//! by scripts/dev-seed.
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::auth::password;
use crm_api::domain::stage;

struct SeedUser {
    email: &'static str,
    display_name: &'static str,
}

struct SeedOrg {
    name: &'static str,
    members: &'static [SeedUser],
}

const SEED_ORGS: &[SeedOrg] = &[
    SeedOrg {
        name: "Acme Realty",
        members: &[
            SeedUser {
                email: "alice@acme.test",
                display_name: "Alice Anderson",
            },
            SeedUser {
                email: "carol@acme.test",
                display_name: "Carol Chen",
            },
        ],
    },
    SeedOrg {
        name: "Best Realty",
        members: &[
            SeedUser {
                email: "bob@best.test",
                display_name: "Bob Baker",
            },
            SeedUser {
                email: "dave@best.test",
                display_name: "Dave Diaz",
            },
        ],
    },
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let url =
        std::env::var("MIGRATION_DATABASE_URL").map_err(|_| "MIGRATION_DATABASE_URL is not set")?;
    let seed_password =
        std::env::var("CRM_DEV_SEED_PASSWORD").map_err(|_| "CRM_DEV_SEED_PASSWORD is not set")?;

    let pool = sqlx::postgres::PgPoolOptions::new().connect(&url).await?;
    // Hashed once and reused for every seeded user; re-run to rotate.
    // argon2's Error type does not implement std::error::Error.
    let password_hash = password::hash_password(&seed_password)
        .map_err(|err| format!("failed to hash seed password: {err}"))?;

    for org in SEED_ORGS {
        let organization_id = find_or_create_organization(&pool, org.name).await?;

        let mut tx = pool.begin().await?;
        stage::seed_defaults(&mut tx, organization_id).await?;
        tx.commit().await?;
        println!("seeded stages for {}", org.name);

        for member in org.members {
            let user_id = find_or_create_user(&pool, member.email, member.display_name).await?;
            upsert_credential(&pool, user_id, &password_hash).await?;
            ensure_membership(&pool, organization_id, user_id).await?;
            println!("seeded {} / {}", org.name, member.email);
        }
    }

    Ok(())
}

async fn find_or_create_organization(pool: &PgPool, name: &str) -> Result<Uuid, sqlx::Error> {
    if let Some((id,)) = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM organization WHERE name = $1")
        .bind(name)
        .fetch_optional(pool)
        .await?
    {
        return Ok(id);
    }
    let (id,) =
        sqlx::query_as::<_, (Uuid,)>("INSERT INTO organization (name) VALUES ($1) RETURNING id")
            .bind(name)
            .fetch_one(pool)
            .await?;
    Ok(id)
}

async fn find_or_create_user(
    pool: &PgPool,
    email: &str,
    display_name: &str,
) -> Result<Uuid, sqlx::Error> {
    if let Some((id,)) =
        sqlx::query_as::<_, (Uuid,)>("SELECT id FROM app_user WHERE lower(email) = lower($1)")
            .bind(email)
            .fetch_optional(pool)
            .await?
    {
        return Ok(id);
    }
    let (id,) = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO app_user (email, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(email)
    .bind(display_name)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn upsert_credential(
    pool: &PgPool,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO local_credential (user_id, password_hash) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET password_hash = excluded.password_hash, updated_at = now()",
    )
    .bind(user_id)
    .bind(password_hash)
    .execute(pool)
    .await?;
    Ok(())
}

async fn ensure_membership(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO organization_membership (organization_id, user_id) VALUES ($1, $2)
         ON CONFLICT (organization_id, user_id) DO NOTHING",
    )
    .bind(organization_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}
