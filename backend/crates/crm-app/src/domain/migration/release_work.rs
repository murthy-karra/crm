//! Cheap scheduler hints, never authorization or claims. A positive result must
//! still pass the existing release, workspace, executor and lease fences.
use sqlx::PgPool;

#[derive(Clone, Copy, Debug)]
pub enum ReleaseWork {
    CoreChange,
    PeopleRefresh,
    PeopleAdmission,
    AdmittedPeopleRefresh,
    HistoryCapture,
    HistoryImport,
    AdmittedHistory,
    FamilyRefresh,
}

impl ReleaseWork {
    pub async fn is_pending(self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let query = match self {
            Self::CoreChange => super::core_change_worker::CANDIDATE_SQL,
            Self::PeopleRefresh => super::people_refresh_worker::CANDIDATE_SQL,
            Self::PeopleAdmission => super::people_admission_worker::CANDIDATE_SQL,
            Self::AdmittedPeopleRefresh => super::admitted_people_refresh_worker::CANDIDATE_SQL,
            Self::HistoryCapture => super::history_capture_worker::CANDIDATE_SQL,
            Self::HistoryImport => super::history_import_worker::CANDIDATE_SQL,
            Self::AdmittedHistory => super::admitted_history_worker::CANDIDATE_SQL,
            // Include ready/preparing plans: executor-revocation cleanup must
            // still run even when no confirmed execution candidate is claimable.
            // This deliberate superset is only an idle optimization.
            Self::FamilyRefresh => "SELECT id FROM migration_family_refresh_plan WHERE state IN ('preparing','ready','queued','running') LIMIT 1",
        };
        Ok(sqlx::query(query).fetch_optional(pool).await?.is_some())
    }
}
