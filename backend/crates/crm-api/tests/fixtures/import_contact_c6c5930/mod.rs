//! Frozen c6c5930 SQL adapter for the opt-in 010c reader regression.
//! SQL bytes/source hashes are recorded in manifest.json; runtime bindings and
//! row decoding mirror the baseline without production SQLx metadata.

pub mod person_sql;
pub mod system_feeds_sql;
pub mod today_sql;

use std::sync::Arc;

use chrono::{DateTime, Utc};
use crm_api::domain::{
    person::{
        filter::PersonFilterParams,
        filter_test_support::{
            frozen_adapter_executed, FilterStatementFamily, FrozenFilterStatements, QueryFuture,
        },
        model::PersonSummary,
        sort::PersonSort,
        visibility::PersonVisibilityScope,
    },
    today::{sources::SourceCandidate, system_feeds::ResolvedFeed, TodayCandidate},
};
use crm_api::ids::{OrganizationId, UserId};
use sqlx::PgConnection;
use uuid::Uuid;

pub fn adapter() -> Arc<dyn FrozenFilterStatements> {
    Arc::new(Frozen)
}

struct Frozen;

impl FrozenFilterStatements for Frozen {
    fn summaries<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        scope: &'a PersonVisibilityScope,
        params: &'a PersonFilterParams,
        sort: PersonSort,
        reference_now: Option<DateTime<Utc>>,
    ) -> QueryFuture<'a, (Vec<PersonSummary>, bool)> {
        Box::pin(async move {
            frozen_adapter_executed(FilterStatementFamily::summary(sort));
            if sort == PersonSort::DEFAULT {
                person_sql::filtered_summaries(conn, scope, params, reference_now).await
            } else {
                use crm_api::domain::person::sort::{SortDirection as D, SortKey as K};
                let statement = match (sort.key, sort.direction) {
                    (K::Created, D::Asc) => person_sql::SortedStatement::CreatedAsc,
                    (K::Name, D::Asc) => person_sql::SortedStatement::NameAsc,
                    (K::Name, D::Desc) => person_sql::SortedStatement::NameDesc,
                    (K::Stage, D::Asc) => person_sql::SortedStatement::StageAsc,
                    (K::Stage, D::Desc) => person_sql::SortedStatement::StageDesc,
                    (K::Assignee, D::Asc) => person_sql::SortedStatement::AssigneeAsc,
                    (K::Assignee, D::Desc) => person_sql::SortedStatement::AssigneeDesc,
                    (K::Created, D::Desc) => person_sql::SortedStatement::Canonical,
                };
                person_sql::filtered_summaries_sorted(conn, scope, params, statement).await
            }
        })
    }
    fn count<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        scope: &'a PersonVisibilityScope,
        params: &'a PersonFilterParams,
    ) -> QueryFuture<'a, (i64, bool)> {
        Box::pin(async move {
            frozen_adapter_executed(FilterStatementFamily::Count);
            person_sql::count_filtered_matches(conn, scope, params).await
        })
    }
    fn source_membership<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        params: &'a PersonFilterParams,
        now: DateTime<Utc>,
        ids: &'a [Uuid],
    ) -> QueryFuture<'a, Vec<Uuid>> {
        Box::pin(async move {
            frozen_adapter_executed(FilterStatementFamily::SourceMembership);
            today_sql::source_membership(conn, org, params, now, ids).await
        })
    }
    fn source_candidates<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        params: &'a PersonFilterParams,
        now: DateTime<Utc>,
        ids: &'a [Uuid],
        members_only: bool,
        limit: i64,
    ) -> QueryFuture<'a, Vec<SourceCandidate>> {
        Box::pin(async move {
            frozen_adapter_executed(FilterStatementFamily::SourceCandidates);
            Ok(
                today_sql::source_candidates(conn, org, params, now, ids, members_only, limit)
                    .await?
                    .into_iter()
                    .map(|row| SourceCandidate {
                        person: row.person,
                        latest_inquiry: row.latest_inquiry,
                        last_contact_attempt: row.last_contact_attempt,
                        last_contact_at: row.last_contact_at,
                    })
                    .collect(),
            )
        })
    }
    fn person_state<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        viewer: UserId,
        now: DateTime<Utc>,
        a: &'a ResolvedFeed,
        b: &'a ResolvedFeed,
    ) -> QueryFuture<'a, (Vec<TodayCandidate>, bool)> {
        Box::pin(async move {
            let pa = a.filter.to_query_params(viewer);
            let pb = b.filter.to_query_params(viewer);
            frozen_adapter_executed(FilterStatementFamily::PersonState);
            system_feeds_sql::person_state_candidates(
                conn,
                org,
                viewer,
                now,
                &pa,
                a.enabled,
                a.fresh_within_hours.unwrap_or(24),
                &pb,
                b.enabled,
                b.fresh_within_hours.unwrap_or(24),
            )
            .await
        })
    }
    fn call_membership<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        viewer: UserId,
        feed: &'a ResolvedFeed,
        retained: &'a [Uuid],
        now: DateTime<Utc>,
    ) -> QueryFuture<'a, Vec<(Uuid, Uuid, DateTime<Utc>)>> {
        Box::pin(async move {
            let p = feed.filter.to_query_params(viewer);
            frozen_adapter_executed(FilterStatementFamily::CallMembership);
            system_feeds_sql::call_membership(conn, org, viewer, &p, retained, now).await
        })
    }
    fn call_only<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        viewer: UserId,
        feed: &'a ResolvedFeed,
        retained: &'a [Uuid],
        limit: i64,
        now: DateTime<Utc>,
    ) -> QueryFuture<'a, Vec<TodayCandidate>> {
        Box::pin(async move {
            let p = feed.filter.to_query_params(viewer);
            frozen_adapter_executed(FilterStatementFamily::CallOnly);
            system_feeds_sql::call_only_candidates(conn, org, viewer, &p, retained, limit, now)
                .await
        })
    }
}
