//! Test-feature-only request-scoped statement selection for the paired
//! Slice 019b authenticated performance harness. Product requests cannot
//! observe or set this task-local state.

use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    filter::PersonFilterParams, model::PersonSummary, sort::PersonSort,
    visibility::PersonVisibilityScope,
};
use crate::{
    domain::today::{sources::SourceCandidate, system_feeds::ResolvedFeed, TodayCandidate},
    ids::{OrganizationId, UserId},
};

pub type QueryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, sqlx::Error>> + Send + 'a>>;

/// Static baseline SQL supplied only by the performance test fixture. The
/// live application never constructs this adapter.
pub trait FrozenFilterStatements: Send + Sync {
    fn summaries<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        scope: &'a PersonVisibilityScope,
        params: &'a PersonFilterParams,
        sort: PersonSort,
        reference_now: Option<DateTime<Utc>>,
    ) -> QueryFuture<'a, (Vec<PersonSummary>, bool)>;
    fn count<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        scope: &'a PersonVisibilityScope,
        params: &'a PersonFilterParams,
    ) -> QueryFuture<'a, (i64, bool)>;
    fn source_membership<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        params: &'a PersonFilterParams,
        now: DateTime<Utc>,
        builtin_ids: &'a [Uuid],
    ) -> QueryFuture<'a, Vec<Uuid>>;
    #[allow(clippy::too_many_arguments)]
    fn source_candidates<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        params: &'a PersonFilterParams,
        now: DateTime<Utc>,
        builtin_ids: &'a [Uuid],
        members_only: bool,
        limit: i64,
    ) -> QueryFuture<'a, Vec<SourceCandidate>>;
    fn person_state<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        viewer: UserId,
        now: DateTime<Utc>,
        a: &'a ResolvedFeed,
        b: &'a ResolvedFeed,
    ) -> QueryFuture<'a, (Vec<TodayCandidate>, bool)>;
    fn call_membership<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        viewer: UserId,
        feed: &'a ResolvedFeed,
        retained: &'a [Uuid],
        now: DateTime<Utc>,
    ) -> QueryFuture<'a, Vec<(Uuid, Uuid, DateTime<Utc>)>>;
    #[allow(clippy::too_many_arguments)]
    fn call_only<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        org: OrganizationId,
        viewer: UserId,
        feed: &'a ResolvedFeed,
        retained: &'a [Uuid],
        limit: i64,
        now: DateTime<Utc>,
    ) -> QueryFuture<'a, Vec<TodayCandidate>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FilterStatementFamily {
    SummaryCreatedDesc,
    SummaryCreatedAsc,
    SummaryNameAsc,
    SummaryNameDesc,
    SummaryStageAsc,
    SummaryStageDesc,
    SummaryAssigneeAsc,
    SummaryAssigneeDesc,
    Count,
    SourceMembership,
    SourceCandidates,
    PersonState,
    CallMembership,
    CallOnly,
}

#[derive(Clone, Default)]
pub struct FilterStatementOverrides {
    frozen: bool,
    adapter: Option<Arc<dyn FrozenFilterStatements>>,
    live_hits: Arc<Mutex<BTreeMap<FilterStatementFamily, usize>>>,
    frozen_hits: Arc<Mutex<BTreeMap<FilterStatementFamily, usize>>>,
}

impl FilterStatementOverrides {
    pub fn live() -> Self {
        Self::default()
    }
    pub fn frozen(adapter: Arc<dyn FrozenFilterStatements>) -> Self {
        Self {
            frozen: true,
            adapter: Some(adapter),
            ..Self::default()
        }
    }
    pub fn frozen_selected(&self) -> bool {
        self.frozen
    }
    pub fn hits(
        &self,
    ) -> (
        BTreeMap<FilterStatementFamily, usize>,
        BTreeMap<FilterStatementFamily, usize>,
    ) {
        (
            self.live_hits
                .lock()
                .expect("filter statement hit lock")
                .clone(),
            self.frozen_hits
                .lock()
                .expect("filter statement hit lock")
                .clone(),
        )
    }
    fn hit_live(&self, family: FilterStatementFamily) {
        *self
            .live_hits
            .lock()
            .expect("filter statement hit lock")
            .entry(family)
            .or_default() += 1;
    }
    fn hit_frozen(&self, family: FilterStatementFamily) {
        *self
            .frozen_hits
            .lock()
            .expect("filter statement hit lock")
            .entry(family)
            .or_default() += 1;
    }
}

impl FilterStatementFamily {
    pub fn summary(sort: PersonSort) -> Self {
        use super::sort::{SortDirection as D, SortKey as K};
        match (sort.key, sort.direction) {
            (K::Created, D::Desc) => Self::SummaryCreatedDesc,
            (K::Created, D::Asc) => Self::SummaryCreatedAsc,
            (K::Name, D::Asc) => Self::SummaryNameAsc,
            (K::Name, D::Desc) => Self::SummaryNameDesc,
            (K::Stage, D::Asc) => Self::SummaryStageAsc,
            (K::Stage, D::Desc) => Self::SummaryStageDesc,
            (K::Assignee, D::Asc) => Self::SummaryAssigneeAsc,
            (K::Assignee, D::Desc) => Self::SummaryAssigneeDesc,
        }
    }
}

tokio::task_local! { static ACTIVE_OVERRIDES: FilterStatementOverrides; }

pub async fn scope<F>(overrides: FilterStatementOverrides, future: F) -> F::Output
where
    F: Future,
{
    ACTIVE_OVERRIDES.scope(overrides, future).await
}

/// Selection is private to the task-local harness scope. It does not count
/// as execution: only the concrete live/frozen adapters below may do that.
pub fn frozen_selected() -> bool {
    ACTIVE_OVERRIDES
        .try_with(|overrides| overrides.frozen_selected())
        .unwrap_or(false)
}

pub fn frozen_adapter() -> Option<Arc<dyn FrozenFilterStatements>> {
    ACTIVE_OVERRIDES
        .try_with(|overrides| overrides.adapter.clone())
        .ok()
        .flatten()
}

pub fn live_adapter_executed(family: FilterStatementFamily) {
    let _ = ACTIVE_OVERRIDES.try_with(|overrides| {
        overrides.hit_live(family);
    });
}

pub fn frozen_adapter_executed(family: FilterStatementFamily) {
    let _ = ACTIVE_OVERRIDES.try_with(|overrides| {
        overrides.hit_frozen(family);
    });
}
