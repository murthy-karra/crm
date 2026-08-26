//! `StartCall` (docs/specs/SLICE_006.md §3): lock the Person, resolve the
//! phone contact method server-side, insert the `placing` call, commit,
//! create the provider room, mint the one-room join grant last. The 409
//! derives from the partial unique index (`23505`), never a pre-check.

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::commands::CallError;
use crate::domain::envelope::CommandContext;
use crate::domain::person::queries as person_queries;
use crate::domain::telephony::queries::{self as call_queries, CallView, NewCall};
use crate::domain::telephony::{settle, Signal};
use crate::realtime::{Publication, Publisher, RealtimeEvent};
use crate::telephony::{JoinGrant, Telephony};

pub struct StartCall {
    pub person_id: Uuid,
    pub contact_method_id: Uuid,
}

/// `(CallView, JoinGrant)`; the grant is returned once and never stored.
#[tracing::instrument(
    name = "call.start",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        call_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn start_call(
    pool: &PgPool,
    publisher: &Publisher,
    telephony: &Arc<Telephony>,
    ctx: &CommandContext,
    cmd: StartCall,
) -> Result<(CallView, JoinGrant), CallError> {
    let result = start_call_attempt(pool, publisher, telephony, ctx, cmd).await;
    match &result {
        Ok((view, _)) => {
            let span = tracing::Span::current();
            span.record("call_id", tracing::field::display(view.id));
            span.record("outcome", "placing");
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "start_call failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

async fn start_call_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    telephony: &Arc<Telephony>,
    ctx: &CommandContext,
    cmd: StartCall,
) -> Result<(CallView, JoinGrant), CallError> {
    let call_id = Uuid::new_v4();
    let room = Telephony::room_for(call_id);
    let placed_at = Utc::now();

    // One bounded retry covers the race where the index rejects us but
    // the other call ends before we read it back — so the 409 body can
    // always carry a `call_id`.
    let mut inserted_row = false;
    for _ in 0..2 {
        let mut tx = pool.begin().await?;

        person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
            .await?
            .ok_or(CallError::PersonNotFound)?;

        if !call_queries::phone_contact_method_exists(
            &mut tx,
            ctx.organization_id,
            cmd.person_id,
            cmd.contact_method_id,
        )
        .await?
        {
            return Err(CallError::InvalidContactMethod);
        }

        let inserted = call_queries::insert_placing(
            &mut tx,
            NewCall {
                id: call_id,
                organization_id: ctx.organization_id,
                person_id: cmd.person_id,
                contact_method_id: cmd.contact_method_id,
                caller_user_id: ctx.actor_user_id,
                origin: ctx.origin.as_str(),
                correlation_id: ctx.correlation_id,
                provider: telephony.provider_name,
                provider_room: &room,
                placed_at,
            },
        )
        .await;

        match inserted {
            Ok(()) => {
                tx.commit().await?;
                inserted_row = true;
                break;
            }
            Err(err) if is_active_call_conflict(&err) => {
                tx.rollback().await?;
                let mut conn = pool.acquire().await?;
                if let Some(active) = call_queries::active_call_for_user(
                    &mut conn,
                    ctx.organization_id,
                    ctx.actor_user_id,
                )
                .await?
                {
                    return Err(CallError::CallInProgress { call_id: active });
                }
                // The other call ended in between: retry once.
            }
            Err(err) => return Err(err.into()),
        }
    }

    // Fail closed: both attempts lost the race and the other call was gone
    // each time we looked. No room is created and no grant minted without
    // a row; the client sees 503 `unavailable` and may retry.
    if !inserted_row {
        tracing::warn!("start_call: no call row after the bounded retry");
        return Err(CallError::Database(sqlx::Error::Protocol(
            "call insert lost the active-call race twice".to_string(),
        )));
    }

    // The row exists and is visible: from here every failure settles.
    let limits = &telephony.limits;
    if let Err(err) = telephony.provider.create_room(&room, limits.max_call).await {
        tracing::warn!(call_id = %call_id, error_kind = err.kind(), "create_room failed");
        settle(
            pool,
            publisher,
            ctx.organization_id,
            call_id,
            &Signal::ProviderError,
            Utc::now(),
        )
        .await?;
        return Err(CallError::TelephonyUnavailable);
    }

    let grant = JoinGrant {
        url: telephony.join_url.clone(),
        token: telephony.signer.mint(
            ctx.actor_user_id.as_uuid(),
            &room,
            Utc::now(),
            limits.join_ttl,
        ),
        room: room.clone(),
    };

    let mut conn = pool.acquire().await?;
    let view = call_queries::call_view_by_id(&mut conn, ctx.organization_id, call_id)
        .await?
        .ok_or(CallError::Corrupt)?;
    drop(conn);

    publisher
        .publish_after_commit(Publication::for_event(RealtimeEvent::call_changed(
            ctx.organization_id,
            placed_at,
            ctx.correlation_id,
            call_id,
            cmd.person_id,
        )))
        .await;

    Ok((view, grant))
}

/// `23505` on `call_one_active_per_caller` (docs/specs/SLICE_006.md §7).
fn is_active_call_conflict(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => {
            db.code().as_deref() == Some("23505")
                && db.constraint() == Some("call_one_active_per_caller")
        }
        _ => false,
    }
}
