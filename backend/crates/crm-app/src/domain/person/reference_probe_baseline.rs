//! Per-identifier probes frozen from 11f5d85's filter reference validation.
//! Only the batching differs between performance arms; custom-field probes,
//! timeout handling, Today selection and the rest of the request stay shared.
use super::{set_statement_timeout_until, Clause, FilterError};
use crate::domain::{person::queries, stage, tag};
use crate::ids::{OrganizationId, StageId, TagId, UserId};
use sqlx::PgConnection;
use std::time::Instant;
use uuid::Uuid;

tokio::task_local! {
    pub static ENABLED: bool;
}

pub(super) async fn validate(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    clause: &Clause,
    ids: &[Uuid],
    deadline: Option<Instant>,
) -> Result<bool, FilterError> {
    for id in ids {
        if let Some(deadline) = deadline {
            set_statement_timeout_until(conn, deadline).await?;
        }
        let valid = match clause {
            Clause::Stage(_) => stage::exists(conn, StageId(*id), organization_id)
                .await
                .map_err(FilterError::Database)?,
            Clause::AssignedTo(_) => {
                queries::is_organization_member(conn, organization_id, UserId(*id))
                    .await
                    .map_err(FilterError::Database)?
            }
            Clause::Tags(_) | Clause::NotTags(_) => tag::exists(conn, organization_id, TagId(*id))
                .await
                .map_err(|error| match error {
                    tag::TagError::Database(error) => FilterError::Database(error),
                    _ => FilterError::Database(sqlx::Error::Decode(
                        "unexpected tag reference error".into(),
                    )),
                })?,
            _ => unreachable!("only grouped reference clauses reach the baseline"),
        };
        if !valid {
            return Ok(false);
        }
    }
    Ok(true)
}
