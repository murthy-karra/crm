//! `crm-admin`: the Slice 004 administration CLI, replacing `seed`
//! (docs/specs/SLICE_004.md §11). Every subcommand but
//! `bootstrap-platform-admin` runs as `crm_app` through the same domain
//! functions the API uses, which doubles as a grants check. No password
//! ever appears on argv or in output; passwords come only from
//! `CRM_DEV_SEED_PASSWORD`.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::commands::{
    create_organization, grant_platform_admin, issue_invitation, set_local_password,
    CreateOrganization, GrantPlatformAdmin, IssueInvitation, SetLocalPassword,
};
use crm_api::domain::admin::queries as admin_queries;
use crm_api::domain::admin::{AdminActor, Role};
use crm_api::domain::envelope::Origin;

const DEFAULT_INVITATION_TTL: Duration = Duration::from_secs(168 * 3600);

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        std::process::exit(2);
    }
    let subcommand = args.remove(0);

    let result = match subcommand.as_str() {
        "bootstrap-platform-admin" => run_bootstrap_platform_admin(args).await,
        "create-organization" => run_create_organization(args).await,
        "invite" => run_invite(args).await,
        "set-password" => run_set_password(args).await,
        other => {
            eprintln!("unknown subcommand: {other}");
            print_usage();
            std::process::exit(2);
        }
    };

    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn print_usage() {
    eprintln!(
        "usage: crm-admin <subcommand> [flags]\n\n\
         subcommands:\n\
         \x20 bootstrap-platform-admin --email <email> --display-name <name>\n\
         \x20 create-organization --name <name> [--as <email>]\n\
         \x20 invite --organization <id> --email <email> --role <admin|member> [--print-link] [--as <email>]\n\
         \x20 set-password --email <email> [--as <email>]"
    );
}

// --- Small argv helpers (no external dependency for five subcommands
// worth of flags — AGENTS.md §13: no new dependency without a concrete
// requirement). ------------------------------------------------------------

fn take_flag(args: &mut Vec<String>, name: &str) -> Option<String> {
    let flag = format!("--{name}");
    let idx = args.iter().position(|a| a == &flag)?;
    if idx + 1 >= args.len() {
        return None;
    }
    args.remove(idx); // the flag
    Some(args.remove(idx)) // the value, now at the same index
}

fn take_bool_flag(args: &mut Vec<String>, name: &str) -> bool {
    let flag = format!("--{name}");
    if let Some(idx) = args.iter().position(|a| a == &flag) {
        args.remove(idx);
        true
    } else {
        false
    }
}

fn require_flag(args: &mut Vec<String>, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    take_flag(args, name).ok_or_else(|| format!("--{name} is required").into())
}

// --- Connections -----------------------------------------------------

fn require_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name).map_err(|_| format!("{name} is not set").into())
}

async fn connect(url: &str) -> Result<PgPool, Box<dyn std::error::Error>> {
    PgPoolOptions::new()
        .connect(url)
        .await
        .map_err(|_| "could not connect to the database".into())
}

async fn migrator_pool() -> Result<PgPool, Box<dyn std::error::Error>> {
    let url = require_env("MIGRATION_DATABASE_URL")?;
    connect(&url).await
}

async fn app_pool() -> Result<PgPool, Box<dyn std::error::Error>> {
    let url = require_env("DATABASE_URL")?;
    connect(&url).await
}

fn invitation_ttl() -> Duration {
    match std::env::var("CRM_INVITATION_TTL_HOURS") {
        Ok(value) => match value.parse::<u64>() {
            Ok(hours) if (1..=720).contains(&hours) => Duration::from_secs(hours * 3600),
            _ => DEFAULT_INVITATION_TTL,
        },
        Err(_) => DEFAULT_INVITATION_TTL,
    }
}

/// `--as <email>` actor resolution (docs/specs/SLICE_004.md §11): resolves
/// to an `app_user` with a `platform_admin` row. Without `--as`, uses the
/// sole `platform_admin` row and refuses to run if there are zero or
/// several.
async fn resolve_actor(
    pool: &PgPool,
    as_email: Option<&str>,
) -> Result<(AdminActor, String), Box<dyn std::error::Error>> {
    let user_id = match as_email {
        Some(email) => {
            let normalized = email.trim().to_lowercase();
            let user_id =
                admin_queries::app_user_id_by_email(&mut *pool.acquire().await?, &normalized)
                    .await?
                    .ok_or_else(|| format!("no app_user found for --as {email}"))?;
            if !admin_queries::is_platform_admin(pool, user_id).await? {
                return Err(format!("{email} is not a platform admin").into());
            }
            user_id
        }
        None => {
            let count = admin_queries::count_platform_admins(pool).await?;
            if count == 0 {
                return Err("no platform admin exists; run bootstrap-platform-admin first".into());
            }
            admin_queries::sole_platform_admin_user_id(pool)
                .await?
                .ok_or_else(|| {
                    format!("{count} platform admins exist; pass --as <email> to disambiguate")
                })?
        }
    };

    let (_, display_name) = admin_queries::app_user_basic(pool, user_id)
        .await?
        .ok_or("platform admin's app_user row is missing")?;

    Ok((
        AdminActor {
            actor_user_id: user_id,
            origin: Origin::Cli,
        },
        display_name,
    ))
}

// --- Subcommands -------------------------------------------------------

async fn run_bootstrap_platform_admin(
    mut args: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let email = require_flag(&mut args, "email")?;
    let display_name = require_flag(&mut args, "display-name")?;
    let password = require_env("CRM_DEV_SEED_PASSWORD")?;

    let pool = migrator_pool().await?;
    let user_id = grant_platform_admin(
        &pool,
        GrantPlatformAdmin {
            email,
            display_name,
            password,
        },
    )
    .await
    .map_err(|err| format!("{err}"))?;

    println!("platform admin ready: {user_id}");
    Ok(())
}

async fn run_create_organization(mut args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let name = require_flag(&mut args, "name")?;
    let as_email = take_flag(&mut args, "as");

    let pool = app_pool().await?;
    let (actor, _) = resolve_actor(&pool, as_email.as_deref()).await?;

    let organization = create_organization(&pool, actor, CreateOrganization { name })
        .await
        .map_err(|err| format!("{err}"))?;

    println!(
        "organization created: {} ({})",
        organization.name, organization.id
    );
    // Slice 007a: the intake address, rendered from the same two env vars
    // the API uses. The token reaching a local terminal is accepted for
    // the local CLI (docs/specs/SLICE_007a.md §5).
    let intake_mail = crm_api::config::intake_mail_config(&|key| std::env::var(key).ok())?;
    let mut conn = pool.acquire().await?;
    if let Some((slug, token)) =
        crm_api::domain::admin::queries::organization_intake_address(&mut conn, organization.id)
            .await?
    {
        let address = crm_api::domain::intake::IntakeAddress { slug, token }.render(&intake_mail);
        println!("intake address: {address}");
    }
    Ok(())
}

async fn run_invite(mut args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let organization_id_raw = require_flag(&mut args, "organization")?;
    let organization_id =
        Uuid::parse_str(&organization_id_raw).map_err(|_| "--organization must be a UUID")?;
    let email = require_flag(&mut args, "email")?;
    let role_raw = require_flag(&mut args, "role")?;
    let role = match role_raw.as_str() {
        "admin" => Role::Admin,
        "member" => Role::Member,
        _ => return Err("--role must be admin or member".into()),
    };
    let print_link = take_bool_flag(&mut args, "print-link");
    let as_email = take_flag(&mut args, "as");

    let pool = app_pool().await?;
    let (actor, actor_display_name) = resolve_actor(&pool, as_email.as_deref()).await?;

    let outcome = issue_invitation(
        &pool,
        actor,
        &actor_display_name,
        invitation_ttl(),
        IssueInvitation {
            organization_id,
            email,
            role,
        },
    )
    .await
    .map_err(|err| format!("{err}"))?;

    println!("invitation issued: {}", outcome.invitation.id);
    if print_link {
        println!("accept link: {}", outcome.accept_path);
    }
    Ok(())
}

async fn run_set_password(mut args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let email = require_flag(&mut args, "email")?;
    let as_email = take_flag(&mut args, "as");
    let password = require_env("CRM_DEV_SEED_PASSWORD")?;

    let pool = app_pool().await?;
    let (_, _) = resolve_actor(&pool, as_email.as_deref()).await?;

    let normalized = email.trim().to_lowercase();
    let user_id = admin_queries::app_user_id_by_email(&mut *pool.acquire().await?, &normalized)
        .await?
        .ok_or_else(|| format!("no app_user found for {email}"))?;

    set_local_password(&pool, SetLocalPassword { user_id, password })
        .await
        .map_err(|err| format!("{err}"))?;

    println!("password set for {email}");
    Ok(())
}
