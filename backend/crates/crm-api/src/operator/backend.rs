//! `SqlxToolBackend`: the `ToolBackend` adapter over the existing
//! `domain::` queries (docs/specs/SLICE_005.md §4; D-008 — no second data
//! path). Every tool acquires one connection, scopes by
//! `PersonVisibilityScope::Organization(ctx.organization_id)` with
//! `viewer = ctx.actor_user_id`, and resolves every Person id through
//! `summary_by_id` first — an invisible id is `ToolError::NotFound`
//! before anything else runs. This is also the one place outside text is
//! wrapped as `UntrustedText`.

use async_trait::async_trait;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::inquiry::queries as inquiry_queries;
use crate::domain::person::model::PersonSummary;
use crate::domain::person::queries::{self as person_queries, HistoryEntry};
use crate::domain::person::PersonVisibilityScope;
use crate::domain::today::{self, TodayItem, TodayList};
use crate::ids::{OrganizationId, UserId};
use crate::operator::explain;
use crm_operator::{
    ContactMethodView, HistoryEntryView, InquiryView, NextWorkItem, OperatorContext, PersonCard,
    PersonDetail, PhoneOption, PriorityExplanation, ProposalView, SearchResult,
    StartCallProposalOutcome, TodayItemView, TodayView, ToolBackend, ToolError, ToolResult,
    UntrustedText,
};

/// `get_person` returns the latest 5 inquiries and latest 20 history
/// entries (docs/specs/SLICE_005.md §3, §14 item 12).
const MAX_INQUIRIES: usize = 5;
const MAX_HISTORY: usize = 20;

pub struct SqlxToolBackend {
    pool: PgPool,
    /// `start_call` proposal lifetime (docs/specs/SLICE_006b.md §2).
    proposal_ttl: std::time::Duration,
}

impl SqlxToolBackend {
    pub fn new(pool: PgPool, proposal_ttl: std::time::Duration) -> Self {
        Self { pool, proposal_ttl }
    }

    async fn conn(&self) -> ToolResult<sqlx::pool::PoolConnection<sqlx::Postgres>> {
        self.pool
            .acquire()
            .await
            .map_err(|_| ToolError::Backend("database connection unavailable".into()))
    }
}

fn db_error(_: sqlx::Error) -> ToolError {
    // The sqlx error text can carry SQL fragments; keep the reason generic.
    ToolError::Backend("database query failed".into())
}

pub fn card_from_summary(summary: &PersonSummary) -> PersonCard {
    PersonCard {
        id: summary.id,
        display_name: UntrustedText::new(&summary.display_name),
        stage_name: summary.stage.name.clone(),
        assigned_user_display_name: summary
            .assigned_user
            .as_ref()
            .map(|u| u.display_name.clone()),
        primary_email: summary.primary_email.as_deref().map(UntrustedText::new),
        primary_phone: summary.primary_phone.as_deref().map(UntrustedText::new),
        inquiry_count: summary.inquiry_count,
        last_inquiry_at: summary.last_inquiry_at,
    }
}

fn item_view(position: usize, item: &TodayItem) -> TodayItemView {
    TodayItemView {
        position,
        person: card_from_summary(&item.person),
        priority: explain::priority_str(item.priority).to_string(),
        recommended_action: serde_json::to_value(item.recommended_action)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
        reasons: explain::reasons_json(item),
        waiting_since: item.waiting_since,
        last_contact_attempt: item.last_contact_attempt.as_ref().map(|a| a.occurred_at),
    }
}

/// `HistoryEntry.detail` rendered from reference-table values only (stage
/// names, member display names, fixed reason/strategy codes) — never
/// outside text (§3).
fn history_detail(entry: &HistoryEntry) -> Option<String> {
    let d = &entry.detail;
    let name = |v: &serde_json::Value| {
        v.get("display_name")
            .and_then(|n| n.as_str())
            .map(str::to_string)
    };
    let stage = |v: &serde_json::Value| v.get("name").and_then(|n| n.as_str()).map(str::to_string);
    match entry.kind {
        "inquiry_received" => d
            .get("source")
            .and_then(|s| s.as_str())
            .map(|s| format!("source {s}")),
        "routing_decision" => {
            let strategy = d.get("strategy").and_then(|s| s.as_str()).unwrap_or("");
            let assignee = d.get("assignee").and_then(name);
            Some(match assignee {
                Some(a) => format!("{strategy}: assigned to {a}"),
                None => format!("{strategy}: unassigned"),
            })
        }
        "assignment_changed" => {
            let from = d.get("from").and_then(name);
            let to = d.get("to").and_then(name);
            Some(format!(
                "from {} to {}",
                from.unwrap_or_else(|| "unassigned".into()),
                to.unwrap_or_else(|| "unassigned".into())
            ))
        }
        "stage_changed" => {
            let from = d.get("from_stage").and_then(stage);
            let to = d.get("to_stage").and_then(stage);
            Some(format!(
                "from {} to {}",
                from.unwrap_or_else(|| "(none)".into()),
                to.unwrap_or_else(|| "(none)".into())
            ))
        }
        "contact_attempted" => {
            let channel = d.get("channel").and_then(|s| s.as_str()).unwrap_or("");
            let outcome = d.get("outcome").and_then(|s| s.as_str()).unwrap_or("");
            // docs/specs/SLICE_006c.md §4: a superseded row is not a live
            // attempt, and a correction is the agent's restatement.
            let corrected = d.get("corrects_id").is_some_and(|v| !v.is_null());
            let superseded = d.get("superseded").and_then(|v| v.as_bool()) == Some(true);
            let mut text = if corrected {
                format!("corrected outcome {channel}: {outcome}")
            } else {
                format!("{channel}: {outcome}")
            };
            if superseded {
                text.push_str(" (superseded)");
            }
            Some(text)
        }
        _ => None,
    }
}

// crm-operator keeps a bare `Uuid` at the tool seam (D-028 §5 crate
// fence); this is the one conversion point back to `OrganizationId` for
// every crm-app call this file makes (hardening chunk N1).
fn org_id(ctx: &OperatorContext) -> OrganizationId {
    OrganizationId::new(ctx.organization_id)
}

// Same seam, for the User id (hardening chunk N2).
fn user_id(ctx: &OperatorContext) -> UserId {
    UserId::new(ctx.actor_user_id)
}

async fn today_for(conn: &mut PgConnection, ctx: &OperatorContext) -> ToolResult<TodayList> {
    today::query(
        conn,
        &PersonVisibilityScope::Organization(org_id(ctx)),
        user_id(ctx),
        ctx.now,
    )
    .await
    .map_err(db_error)
}

async fn visible_summary(
    conn: &mut PgConnection,
    ctx: &OperatorContext,
    person_id: Uuid,
) -> ToolResult<PersonSummary> {
    person_queries::summary_by_id(conn, org_id(ctx), person_id)
        .await
        .map_err(db_error)?
        .ok_or(ToolError::NotFound)
}

#[async_trait]
impl ToolBackend for SqlxToolBackend {
    async fn search_people(
        &self,
        ctx: &OperatorContext,
        query: &str,
        limit: usize,
    ) -> ToolResult<SearchResult> {
        let mut conn = self.conn().await?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let (summaries, truncated) = person_queries::search_summaries(
            &mut conn,
            &PersonVisibilityScope::Organization(org_id(ctx)),
            query,
            limit,
        )
        .await
        .map_err(db_error)?;
        Ok(SearchResult {
            matches: summaries.iter().map(card_from_summary).collect(),
            truncated,
        })
    }

    async fn get_person(&self, ctx: &OperatorContext, person_id: Uuid) -> ToolResult<PersonDetail> {
        let mut conn = self.conn().await?;
        let summary = visible_summary(&mut conn, ctx, person_id).await?;

        let contact_methods =
            person_queries::contact_methods_for_person(&mut conn, org_id(ctx), person_id)
                .await
                .map_err(db_error)?
                .into_iter()
                .map(|m| ContactMethodView {
                    kind: m.kind,
                    value: UntrustedText::new(&m.value),
                })
                .collect();

        let inquiries = inquiry_queries::list_for_person(&mut conn, org_id(ctx), person_id)
            .await
            .map_err(db_error)?
            .into_iter()
            .take(MAX_INQUIRIES)
            .map(|i| InquiryView {
                id: i.id,
                source: i.source,
                received_at: i.received_at,
                message: i.message.as_deref().map(UntrustedText::new),
            })
            .collect();

        let all_history = person_queries::history_for_person(&mut conn, org_id(ctx), person_id)
            .await
            .map_err(db_error)?;
        let skip = all_history.len().saturating_sub(MAX_HISTORY);
        let history = all_history
            .iter()
            .skip(skip)
            .map(|e| HistoryEntryView {
                kind: e.kind.to_string(),
                occurred_at: e.occurred_at,
                actor_display_name: e.actor.as_ref().map(|a| a.display_name.clone()),
                detail: history_detail(e),
            })
            .collect();

        let today = today_for(&mut conn, ctx).await?;
        let on_your_today = today.items.iter().any(|i| i.person.id == person_id);

        Ok(PersonDetail {
            person: card_from_summary(&summary),
            contact_methods,
            inquiries,
            history,
            on_your_today,
        })
    }

    async fn get_today(&self, ctx: &OperatorContext, limit: usize) -> ToolResult<TodayView> {
        let mut conn = self.conn().await?;
        let list = today_for(&mut conn, ctx).await?;
        let items = list
            .items
            .iter()
            .enumerate()
            .take(limit)
            .map(|(i, item)| item_view(i + 1, item))
            .collect();
        Ok(TodayView {
            generated_at: list.generated_at,
            total: list.items.len(),
            truncated: list.truncated || list.items.len() > limit,
            items,
        })
    }

    async fn get_next_work_item(&self, ctx: &OperatorContext) -> ToolResult<NextWorkItem> {
        let mut conn = self.conn().await?;
        let list = today_for(&mut conn, ctx).await?;
        Ok(NextWorkItem {
            item: list.items.first().map(|item| item_view(1, item)),
            total: list.items.len(),
        })
    }

    async fn explain_priority(
        &self,
        ctx: &OperatorContext,
        person_id: Uuid,
    ) -> ToolResult<PriorityExplanation> {
        let mut conn = self.conn().await?;
        let summary = visible_summary(&mut conn, ctx, person_id).await?;
        let list = today_for(&mut conn, ctx).await?;
        Ok(explain::build_explanation(
            &list,
            &summary,
            user_id(ctx),
            card_from_summary(&summary),
        ))
    }

    /// `start_call` (docs/specs/SLICE_006b.md §3): validates and inserts a
    /// proposal — never executes. Person resolved through the same
    /// visibility gate as every read tool; the contact method must belong
    /// to the Person (foreign/nonexistent → byte-identical NotFound) and
    /// be a phone (an email id → structured invalid_arguments).
    async fn propose_start_call(
        &self,
        ctx: &OperatorContext,
        person_id: Uuid,
        contact_method_id: Option<Uuid>,
    ) -> ToolResult<StartCallProposalOutcome> {
        let mut conn = self.conn().await?;
        let summary = visible_summary(&mut conn, ctx, person_id).await?;
        let methods = person_queries::contact_methods_for_person(&mut conn, org_id(ctx), person_id)
            .await
            .map_err(db_error)?;

        let chosen = match contact_method_id {
            Some(id) => {
                let method = methods
                    .iter()
                    .find(|m| m.id == id)
                    .ok_or(ToolError::NotFound)?;
                if method.kind != "phone" {
                    return Err(ToolError::InvalidArguments(
                        "that contact method is not a phone number".to_string(),
                    ));
                }
                Some(method)
            }
            None => {
                let mut phones = methods.iter().filter(|m| m.kind == "phone");
                match (phones.next(), phones.next()) {
                    (None, _) => return Ok(StartCallProposalOutcome::NoPhone),
                    (Some(only), None) => Some(only),
                    (Some(_), Some(_)) => {
                        return Ok(StartCallProposalOutcome::NeedsNumberChoice {
                            phones: methods
                                .iter()
                                .filter(|m| m.kind == "phone")
                                .map(|m| PhoneOption {
                                    contact_method_id: m.id,
                                    value: UntrustedText::new(&m.value),
                                })
                                .collect(),
                        })
                    }
                }
            }
        };
        let method = chosen.expect("all None paths returned above");

        let proposal_id = Uuid::new_v4();
        let ttl_secs = i64::try_from(self.proposal_ttl.as_secs()).unwrap_or(120);
        let expires_at = sqlx::query_scalar!(
            r#"INSERT INTO operator_proposal
                 (id, organization_id, actor_user_id, turn_id, tool,
                  person_id, contact_method_id, status, expires_at)
               VALUES ($1, $2, $3, $4, 'start_call', $5, $6, 'proposed',
                       now() + make_interval(secs => $7::double precision))
               RETURNING expires_at"#,
            proposal_id,
            ctx.organization_id,
            ctx.actor_user_id,
            ctx.turn_id,
            person_id,
            method.id,
            ttl_secs as f64,
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(db_error)?;

        Ok(StartCallProposalOutcome::Proposed(Box::new(ProposalView {
            proposal_id,
            person: card_from_summary(&summary),
            phone: UntrustedText::new(&method.value),
            contact_method_id: method.id,
            expires_at,
        })))
    }
}
