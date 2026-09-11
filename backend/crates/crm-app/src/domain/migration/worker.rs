//! One bounded GET per lease; settlement atomically fences and persists evidence.
use super::{
    crypto,
    reader::{Capture, FubReader, Probe, ReaderError},
    store::{self, CheckResult},
    MigrationError,
};
use crate::config::RawPayloadKey;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);
pub const MAX_ATTEMPTS: i32 = 3;
pub fn spawn(
    pool: PgPool,
    key: RawPayloadKey,
    reader: Arc<dyn FubReader>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            loop {
                match run_once(&pool, &key, reader.as_ref()).await {
                    Ok(true) => continue,
                    Ok(false) => break,
                    Err(error) => {
                        tracing::warn!(outcome=%error,"migration sweep failed");
                        break;
                    }
                }
            }
        }
    })
}
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &dyn FubReader,
) -> Result<bool, MigrationError> {
    let _permit = super::reader::source_read_permit().await;
    let Some(claim) = store::claim_one(pool).await? else {
        return Ok(false);
    };
    process(pool, key, reader, &claim).await
}

#[tracing::instrument(name="migration_check",skip_all,fields(organization_id=%claim.org.0,assessment_id=%claim.id,actor_id=%claim.actor.0,connection_id=%claim.connection_id,check=%claim.check,attempt=claim.cycle_attempts))]
async fn process(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &dyn FubReader,
    claim: &store::Claim,
) -> Result<bool, MigrationError> {
    let Some((account, nonce, ciphertext)) = store::preflight(pool, claim).await? else {
        tracing::info!(outcome = "preflight_rejected");
        return Ok(true);
    };
    let credential = crypto::open_credential(
        key,
        claim.org,
        claim.connection_id,
        claim.revision,
        &nonce,
        &ciphertext,
    )
    .ok()
    .and_then(|v| String::from_utf8(v).ok());
    let outcome = match credential {
        None => Err(ReaderError::InvalidCredential),
        Some(credential) => probe(reader, &credential, account, &claim.check).await,
    };
    let result = match outcome {
        Ok(result) => result,
        Err(error) => failure(error, claim),
    };
    let state = result.state;
    let reason = result.reason.unwrap_or("success");
    let committed = store::settle(pool, key, claim, result).await?;
    tracing::info!(
        state,
        outcome = reason,
        committed,
        "migration check settled"
    );
    Ok(true)
}
async fn probe(
    reader: &dyn FubReader,
    credential: &str,
    account: i64,
    check: &str,
) -> Result<CheckResult, ReaderError> {
    if check == "identity" {
        let (identity, body) = reader.identity(credential).await?;
        let capture = Capture {
            status: 200,
            body,
            truncated: false,
            source_version: None,
        };
        if identity.account_id != account || identity.user_id.unwrap_or_default() <= 0 {
            return Err(ReaderError::IdentityMismatch.with_capture(capture));
        }
        return Ok(CheckResult {
            state: "completed",
            coverage: "complete_for_query",
            total: None,
            retrieved: 1,
            reason: None,
            next: None,
            capture: Some(capture),
        });
    }
    let probe = match check {
        "people_excluding_trash" => Probe::PeopleExcludingTrash,
        "people_including_trash" => Probe::PeopleIncludingTrash,
        "users" => Probe::Users,
        "stages" => Probe::Stages,
        "custom_fields" => Probe::CustomFields,
        _ => return Err(ReaderError::MalformedResponse),
    };
    let result = reader.probe(credential, probe).await?;
    let complete = !result.continuation
        && result
            .reported_total
            .as_deref()
            .and_then(|v| v.parse::<i32>().ok())
            == Some(result.retrieved_count);
    Ok(CheckResult {
        state: "completed",
        coverage: if complete {
            "complete_for_query"
        } else {
            "partial"
        },
        total: result.reported_total,
        retrieved: result.retrieved_count,
        reason: None,
        next: None,
        capture: Some(Capture {
            status: result.status,
            body: result.body,
            truncated: false,
            source_version: result.source_version,
        }),
    })
}
fn failure(error: ReaderError, claim: &store::Claim) -> CheckResult {
    let (error, capture) = error.split();
    let mut reason = error.code();
    let delay = match error {
        ReaderError::RateLimited(v) => Some(v.unwrap_or(60)),
        ReaderError::Unavailable => Some(30),
        _ => None,
    };
    let retry = delay.is_some() && claim.cycle_attempts < MAX_ATTEMPTS;
    let next = if retry {
        delay.filter(|v| *v <= 86400).and_then(|v| {
            chrono::Utc::now().checked_add_signed(chrono::Duration::seconds(v as i64))
        })
    } else {
        None
    };
    if delay.is_some() && claim.cycle_attempts >= MAX_ATTEMPTS {
        reason = "retry_exhausted";
    }
    if retry && next.is_none() {
        reason = "invalid_rate_timing";
    }
    let denied_collection = matches!(error, ReaderError::AccessDenied) && claim.check != "identity";
    CheckResult {
        state: if next.is_some() {
            "waiting_retry"
        } else if denied_collection {
            "completed"
        } else {
            "paused"
        },
        coverage: "unavailable",
        total: None,
        retrieved: 0,
        reason: Some(reason),
        next,
        capture,
    }
}
