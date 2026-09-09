//! `SqlxToolBackend`: the `ToolBackend` adapter over the existing
//! `domain::` queries (docs/specs/SLICE_005.md §4; D-008 — no second data
//! path). Every tool acquires one connection, scopes by
//! `PersonVisibilityScope::Organization(ctx.organization_id)` with
//! `viewer = ctx.actor_user_id`, and resolves every Person id through
//! `summary_by_id` first — an invisible id is `ToolError::NotFound`
//! before anything else runs. This is also the one place outside text is
//! wrapped as `UntrustedText`.

use async_trait::async_trait;
use sqlx::pool::PoolConnection;
use sqlx::{PgConnection, PgPool, Postgres};
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::admin::queries as admin_queries;
use crate::domain::inquiry::queries as inquiry_queries;
use crate::domain::person::filter::FilterNames;
use crate::domain::person::model::PersonSummary;
use crate::domain::person::queries::{self as person_queries, HistoryEntry};
use crate::domain::person::PersonVisibilityScope;
use crate::domain::saved_list::{self, SavedListError};
use crate::domain::stage;
use crate::domain::tag;
use crate::domain::today::{self, TodayItem, TodayList};
use crate::ids::{ContactMethodId, OrganizationId, PersonId, SavedListId, UserId};
use crate::operator::explain;
use crate::operator::filter::{self as name_resolver, NameMatch, Resolved};
use crm_operator::{
    ContactMethodView, FilterOutcome, FilterResult, HistoryEntryView, InquiryView, NextWorkItem,
    NoteView, OperatorContext, PeopleFilterSpec, PersonCard, PersonDetail, PhoneOption,
    PriorityExplanation, ProposalView, SavedListRef, SavedListSelector, SearchResult,
    StartCallProposalOutcome, TodayItemView, TodayView, ToolBackend, ToolError, ToolResult,
    UntrustedText,
};

/// `get_person` returns the latest 5 inquiries and latest 20 history
/// entries (docs/specs/SLICE_005.md §3, §14 item 12); the latest 5 live
/// notes (docs/specs/SLICE_015.md §5, the same `MAX_INQUIRIES` precedent).
const MAX_INQUIRIES: usize = 5;
const MAX_HISTORY: usize = 20;
const MAX_NOTES: usize = 5;

pub struct SqlxToolBackend {
    pool: PgPool,
    /// `start_call` proposal lifetime (docs/specs/SLICE_006b.md §2).
    proposal_ttl: std::time::Duration,
    /// `run_saved_list`'s two reads (`list_saved_lists`/`saved_list_detail`,
    /// docs/specs/SLICE_013.md §2) need the request's `AuthContext` — no
    /// other method on this backend touches it, and never its
    /// `actor_email` or `active_organization_name`.
    auth: AuthContext,
}

impl SqlxToolBackend {
    pub fn new(pool: PgPool, proposal_ttl: std::time::Duration, auth: AuthContext) -> Self {
        Self {
            pool,
            proposal_ttl,
            auth,
        }
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

/// `list_saved_lists`/`saved_list_detail` fail (in practice) only with
/// `SavedListError::Database`; every other variant belongs to command
/// paths these two reads never take. Map generically, same reason as
/// `db_error`.
fn saved_list_error(_: SavedListError) -> ToolError {
    ToolError::Backend("database query failed".into())
}

/// docs/specs/SLICE_013.md §2: pre-resolved name maps for `describe()`,
/// built from the same three org-scoped list reads the resolver used —
/// one round trip, not a second query per name.
fn build_filter_names(
    stages: &[stage::Stage],
    members: &[admin_queries::MemberView],
    tags: &[tag::TagRow],
) -> FilterNames {
    FilterNames {
        stage_names: stages.iter().map(|s| (s.id, s.name.clone())).collect(),
        user_names: members
            .iter()
            .map(|m| (m.user_id, m.display_name.clone()))
            .collect(),
        tag_names: tags.iter().map(|t| (t.id, t.name.clone())).collect(),
    }
}

pub fn card_from_summary(summary: &PersonSummary) -> PersonCard {
    PersonCard {
        // crm-operator keeps a bare `Uuid` at the tool seam (D-028 §5
        // crate fence); this is a conversion point back to bare `Uuid` for
        // every crm-app -> crm-operator id crossing this file makes
        // (hardening chunk N3, mirroring N1/N2's org/user seam
        // conversions below).
        id: summary.id.as_uuid(),
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

async fn today_for(conn: PoolConnection<Postgres>, ctx: &OperatorContext) -> ToolResult<TodayList> {
    today::query_owned(
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
    person_id: PersonId,
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
        // crm-operator's `ToolBackend` trait keeps a bare `Uuid` at the
        // tool seam (D-028 §5 crate fence); wrap once here, at the trait
        // boundary, so every crm-app call below this line is typed
        // (hardening chunk N3, mirroring `org_id`/`user_id` above).
        let person_id = PersonId::new(person_id);
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
                id: i.id.as_uuid(),
                source: i.source,
                received_at: i.received_at,
                message: i.message.as_deref().map(UntrustedText::new),
            })
            .collect();

        // The `note` kind is excluded before the `MAX_HISTORY` truncation
        // (docs/specs/SLICE_015.md §5): `notes` below already represents
        // live notes, and twenty recent notes would otherwise push every
        // stage, assignment, and call fact out of the model's bounded
        // view. `history_detail` gains no `"note"` arm — a body never
        // reaches the model through this projection, only through
        // `notes` as `UntrustedText`.
        let all_history: Vec<_> =
            person_queries::history_for_person(&mut conn, org_id(ctx), person_id)
                .await
                .map_err(db_error)?
                .into_iter()
                .filter(|e| e.kind != "note")
                .collect();
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

        let tags = tag::list_for_person(&mut conn, org_id(ctx), person_id)
            .await
            // `TagError`, not `sqlx::Error` (`tag::list_for_person` is a
            // domain-error-wrapped read, unlike the bare-`sqlx::Error`
            // queries above) — same generic backend-failure reason as
            // `db_error`, kept free of any SQL-error text.
            .map_err(|_| ToolError::Backend("database query failed".into()))?
            .into_iter()
            .map(|t| UntrustedText::new(&t.name))
            .collect();

        let notes = crate::domain::note::latest_for_person(
            &mut conn,
            org_id(ctx),
            person_id,
            MAX_NOTES as i64,
        )
        .await
        // `NoteError`, the same reasoning as the `tag::list_for_person`
        // mapping just above — a generic backend-failure reason, never a
        // note body or a SQL-error string.
        .map_err(|_| ToolError::Backend("database query failed".into()))?
        .into_iter()
        .map(|n| NoteView {
            author_display_name: n.author_display_name,
            created_at: n.created_at,
            body: UntrustedText::new(&n.body),
        })
        .collect();

        let today = today_for(conn, ctx).await?;
        let on_your_today = today.items.iter().any(|i| i.person.id == person_id);

        Ok(PersonDetail {
            person: card_from_summary(&summary),
            contact_methods,
            inquiries,
            history,
            on_your_today,
            today_truncated: today.truncated,
            sources: explain::sources_view(&today),
            tags,
            notes,
        })
    }

    async fn get_today(&self, ctx: &OperatorContext, limit: usize) -> ToolResult<TodayView> {
        let conn = self.conn().await?;
        let list = today_for(conn, ctx).await?;
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
            sources: explain::sources_view(&list),
            items,
        })
    }

    async fn get_next_work_item(&self, ctx: &OperatorContext) -> ToolResult<NextWorkItem> {
        let conn = self.conn().await?;
        let list = today_for(conn, ctx).await?;
        Ok(NextWorkItem {
            item: list.items.first().map(|item| item_view(1, item)),
            total: list.items.len(),
            truncated: list.truncated,
            sources: explain::sources_view(&list),
        })
    }

    async fn explain_priority(
        &self,
        ctx: &OperatorContext,
        person_id: Uuid,
    ) -> ToolResult<PriorityExplanation> {
        // Same trait-boundary wrap as `get_person` above.
        let person_id = PersonId::new(person_id);
        let mut conn = self.conn().await?;
        let summary = visible_summary(&mut conn, ctx, person_id).await?;
        let list = today_for(conn, ctx).await?;
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
        // Same trait-boundary wrap as `get_person` above; `.0` at the raw
        // `operator_proposal` INSERT below converts back for that bind.
        let person_id = PersonId::new(person_id);
        // Same wrap for the call cluster's `ContactMethodId` (hardening
        // chunk N4) — `person_id`/`contact_method_id` were the survey's
        // adjacent-bare-`Uuid` pair at this trait boundary; both are now
        // distinct types. `ContactMethodItem.id` (the general
        // contact-method listing, `domain/person/queries.rs` — the V1
        // lane's territory) stays bare, so the comparison below unwraps
        // back to `Uuid` rather than typing that struct.
        let contact_method_id = contact_method_id.map(ContactMethodId::new);
        let mut conn = self.conn().await?;
        let summary = visible_summary(&mut conn, ctx, person_id).await?;
        let methods = person_queries::contact_methods_for_person(&mut conn, org_id(ctx), person_id)
            .await
            .map_err(db_error)?;

        let chosen = match contact_method_id {
            Some(id) => {
                let method = methods
                    .iter()
                    .find(|m| m.id == id.as_uuid())
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
            person_id.0,
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

    /// docs/specs/SLICE_013.md §2: resolve `spec`'s names against the
    /// caller's own Organization (the three org-scoped list reads
    /// `saved_list::queries::filter_names` also uses), then `validate` →
    /// `describe` → `to_query_params` → `filtered_summaries`. Never
    /// `validate_references` — every id here came from these same
    /// Organization-scoped reads in this request (§2).
    async fn filter_people(
        &self,
        ctx: &OperatorContext,
        spec: &PeopleFilterSpec,
    ) -> ToolResult<FilterOutcome> {
        let mut conn = self.conn().await?;
        let stages = stage::list(&mut conn, org_id(ctx))
            .await
            .map_err(db_error)?;
        let members = admin_queries::members(&mut conn, org_id(ctx))
            .await
            .map_err(db_error)?;
        let tags = tag::list_for_organization(&mut conn, org_id(ctx))
            .await
            .map_err(|_| ToolError::Backend("database query failed".into()))?;

        match name_resolver::resolve(spec, &stages, &members, &tags) {
            Resolved::Clarification(c) => {
                // docs/specs/SLICE_013.md §6: `resolution` only — no
                // `FilterDefinition` was ever built, so `filter_kinds`/
                // `filter_clause_count`/`match_count` stay `Empty`.
                tracing::Span::current().record("resolution", "needs_clarification");
                Ok(FilterOutcome::NeedsClarification {
                    unknown_stages: c.unknown_stages,
                    unknown_tags: c.unknown_tags,
                    unknown_assignees: c.unknown_assignees,
                    ambiguous_assignees: c.ambiguous_assignees,
                    available_stages: c.available_stages,
                    available_tags: c
                        .available_tags
                        .iter()
                        .map(|t| UntrustedText::new(t))
                        .collect(),
                    members: c.members,
                    candidate_lists: Vec::new(),
                })
            }
            Resolved::Definition(def) => {
                // Every id here came from the resolve above, and
                // `name_resolver::resolve` already collapses two names
                // that resolve to the same value (docs/tasks/
                // SLICE_013_IMPL.md coordinator decision) — so a
                // duplicate-value rejection from `validate()` cannot come
                // from that. What's left (a non-canonical `sources`
                // value, a clause-count edge) is still `invalid_arguments`,
                // a strike, never a clarification.
                def.validate().map_err(|_| {
                    ToolError::InvalidArguments("the filter is invalid".to_string())
                })?;
                let names = build_filter_names(&stages, &members, &tags);
                let description = def.describe(&names);
                // docs/specs/SLICE_013.md §6: the static clause-kind
                // vocabulary and a clause count — never a name, id, or day
                // count.
                let span = tracing::Span::current();
                span.record("filter_kinds", def.kinds_field());
                span.record("filter_clause_count", def.clauses.len());
                span.record("resolution", "matched");
                let params = def.to_query_params(user_id(ctx));
                let scope = PersonVisibilityScope::Organization(org_id(ctx));
                let (rows, truncated) =
                    person_queries::filtered_summaries(&mut conn, &scope, &params)
                        .await
                        .map_err(db_error)?;
                let matches: Vec<PersonCard> = rows
                    .iter()
                    .take(spec.limit)
                    .map(card_from_summary)
                    .collect();
                span.record("match_count", rows.len());
                span.record("more_than_500", truncated);
                span.record("returned", matches.len());
                Ok(FilterOutcome::Matched(FilterResult {
                    list: None,
                    description: description.iter().map(|d| UntrustedText::new(d)).collect(),
                    count: rows.len(),
                    more_than_500: truncated,
                    returned: matches.len(),
                    matches,
                }))
            }
        }
    }

    /// docs/specs/SLICE_013.md §2, D-046, D-048: resolves `selector` by
    /// `list_id` (the duplicate-name clarification's candidate, re-checked
    /// through the same visibility predicate) or by `name` over
    /// `list_saved_lists`' already visibility-filtered rows — the actor's
    /// own personal lists plus every shared list, byte-identical
    /// `not_found` for another member's personal list, a foreign
    /// Organization's list, or a nonexistent one. The stored sort selects
    /// the sorted statement; `None` the default one.
    async fn run_saved_list(
        &self,
        ctx: &OperatorContext,
        selector: &SavedListSelector,
    ) -> ToolResult<FilterOutcome> {
        // Fail closed if this backend's own `AuthContext` (set once at
        // construction, `routes/operator.rs`) ever disagreed with the
        // per-call `OperatorContext` (set once per turn) about who is
        // asking — unreachable in production (both are built from the
        // same request's `auth`), but this is the one method that reads
        // `self.auth` at all, and it must never silently query on behalf
        // of the wrong identity if that constructor invariant is ever
        // broken.
        if self.auth.active_organization_id != org_id(ctx)
            || self.auth.actor_user_id != user_id(ctx)
        {
            return Err(ToolError::Backend("operator context mismatch".to_string()));
        }

        let mut conn = self.conn().await?;

        let detail = if let Some(list_id) = selector.list_id {
            saved_list::saved_list_detail(&mut conn, &self.auth, SavedListId::new(list_id))
                .await
                .map_err(saved_list_error)?
                .ok_or(ToolError::NotFound)?
        } else {
            let name = selector.name.as_deref().unwrap_or_default();
            let lists = saved_list::list_saved_lists(&mut conn, &self.auth)
                .await
                .map_err(saved_list_error)?;
            match name_resolver::find_by_name(name, &lists, |l| l.name.as_str()) {
                NameMatch::None => return Err(ToolError::NotFound),
                NameMatch::Many(candidates) => {
                    tracing::Span::current().record("resolution", "needs_clarification");
                    return Ok(FilterOutcome::NeedsClarification {
                        unknown_stages: Vec::new(),
                        unknown_tags: Vec::new(),
                        unknown_assignees: Vec::new(),
                        ambiguous_assignees: Vec::new(),
                        available_stages: Vec::new(),
                        available_tags: Vec::new(),
                        members: Vec::new(),
                        candidate_lists: candidates
                            .iter()
                            .map(|l| SavedListRef {
                                list_id: l.id.as_uuid(),
                                name: UntrustedText::new(&l.name),
                                scope: l.scope.as_str().to_string(),
                            })
                            .collect(),
                    });
                }
                // Visibility-filtered by construction: `list_saved_lists`
                // already scoped this row to the caller's own personal
                // lists plus every shared list (§2).
                NameMatch::One(list) => {
                    saved_list::saved_list_detail(&mut conn, &self.auth, list.id)
                        .await
                        .map_err(saved_list_error)?
                        .ok_or(ToolError::NotFound)?
                }
            }
        };

        let span = tracing::Span::current();
        // docs/specs/SLICE_013.md §6: the list's scope token
        // ("personal"/"shared") is not user text, unlike its name.
        span.record("saved_list_scope", detail.list.scope.as_str());

        // A stored definition this binary cannot evaluate — an
        // unsupported/stale filter, or a since-deleted stage/assignee/tag
        // reference — reports `ListInvalid` without re-validation; the
        // fail-closed disposition and its `filter_error` code both already
        // came from `saved_list_detail` (docs/specs/SLICE_013.md §2).
        if detail.filter_error.is_some() || detail.filter.is_none() {
            span.record("resolution", "list_invalid");
            let error = detail
                .filter_error
                .map(|e| e.as_str())
                .unwrap_or("unsupported_filter")
                .to_string();
            return Ok(FilterOutcome::ListInvalid { error });
        }
        let def = detail.filter.expect("checked filter_error/filter above");
        span.record("filter_kinds", def.kinds_field());
        span.record("filter_clause_count", def.clauses.len());
        span.record("resolution", "matched");

        let params = def.to_query_params(user_id(ctx));
        let scope = PersonVisibilityScope::Organization(org_id(ctx));
        let (rows, truncated) = match detail.sort {
            Some(sort) => {
                person_queries::filtered_summaries_sorted(&mut conn, &scope, &params, sort).await
            }
            None => person_queries::filtered_summaries(&mut conn, &scope, &params).await,
        }
        .map_err(db_error)?;
        let matches: Vec<PersonCard> = rows
            .iter()
            .take(selector.limit)
            .map(card_from_summary)
            .collect();
        span.record("match_count", rows.len());
        span.record("more_than_500", truncated);
        span.record("returned", matches.len());

        Ok(FilterOutcome::Matched(FilterResult {
            list: Some(SavedListRef {
                list_id: detail.list.id.as_uuid(),
                name: UntrustedText::new(&detail.list.name),
                scope: detail.list.scope.as_str().to_string(),
            }),
            description: detail
                .description
                .iter()
                .map(|d| UntrustedText::new(d))
                .collect(),
            count: rows.len(),
            more_than_500: truncated,
            returned: matches.len(),
            matches,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crm_app::domain::admin::Role;
    use sqlx::postgres::PgPoolOptions;

    /// A lazy pool never opens a connection until first used — the
    /// context-mismatch guard returns before that happens, so this test
    /// needs no database.
    fn fake_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://user:pass@127.0.0.1:1/db")
            .expect("lazy pool construction never touches the network")
    }

    fn auth(organization_id: Uuid, actor_user_id: Uuid) -> AuthContext {
        AuthContext {
            actor_user_id: UserId::new(actor_user_id),
            actor_email: "fixture@example.test".to_string(),
            actor_display_name: "Fixture".to_string(),
            active_organization_id: OrganizationId::new(organization_id),
            active_organization_name: "Fixture Organization".to_string(),
            role: Role::Member,
        }
    }

    fn ctx(organization_id: Uuid, actor_user_id: Uuid) -> OperatorContext {
        OperatorContext {
            actor_user_id,
            organization_id,
            actor_display_name: "Fixture".to_string(),
            turn_id: Uuid::new_v4(),
            now: Utc::now(),
        }
    }

    /// docs/tasks/SLICE_013_IMPL.md coordinator decision: `run_saved_list`
    /// fails closed rather than querying on behalf of the wrong identity
    /// if the backend's own `AuthContext` and the per-call
    /// `OperatorContext` ever disagree (unreachable in production; both
    /// are built from the same request's `auth` in `routes/operator.rs`).
    #[tokio::test]
    async fn run_saved_list_fails_closed_on_a_context_mismatch() {
        let backend = SqlxToolBackend::new(
            fake_pool(),
            std::time::Duration::from_secs(120),
            auth(Uuid::new_v4(), Uuid::new_v4()),
        );
        // Every id here is independently random: both the organization
        // and the actor disagree with `backend`'s own `auth`.
        let mismatched_ctx = ctx(Uuid::new_v4(), Uuid::new_v4());
        let selector = SavedListSelector {
            name: Some("Any List".to_string()),
            list_id: None,
            limit: 10,
        };
        let result = backend.run_saved_list(&mismatched_ctx, &selector).await;
        assert!(matches!(result, Err(ToolError::Backend(_))), "{result:?}");
    }
}
