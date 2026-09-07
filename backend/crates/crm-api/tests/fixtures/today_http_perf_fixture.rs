//! Primary-owned isolated fixture for Slice 011c's authenticated HTTP
//! performance matrix. The workload harness owns scheduling and evidence;
//! this module owns only synthetic data, real source-command setup, and
//! test-pool lifecycle. It never writes fixture names, filters, credentials,
//! cookies, URLs, or customer-like payloads to retained benchmark evidence.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use crm_api::config::Config;
use crm_api::domain::admin::commands::{set_local_password, SetLocalPassword};
use crm_api::domain::admin::queries as admin_queries;
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AgeClause, AgeSpec, BoolClause, Clause, FilterDefinition, SourceClause, StageClause,
};
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::ids::{CorrelationId, OrganizationId, SavedListId, StageId, UserId};
use reqwest::header::COOKIE;
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use uuid::Uuid;

const FIXED_CLOCK: &str = "2026-09-06T12:00:00Z";
const PERF_PASSWORD: &str = "slice-011c-http-fixture-password";
const PEOPLE: usize = 50_000;
const SOURCE_LIMIT: usize = 5;
const EXPECTED_INQUIRIES: usize = 46_064;
const EXPECTED_CONTACT_FACTS: usize = 52_306;
const EXPECTED_CONTACT_CORRECTIONS: usize = 2_906;
const EXPECTED_INBOUND_RECORDS: usize = 2_843;

/// The four builtin queue shapes measured by the approved Phase B matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TodayHttpPerfCase {
    Concentrated,
    Typical,
    PartialBuiltins,
    EmptyBuiltins,
}

/// The source preference configurations measured for an individual viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerfSourceMode {
    Zero,
    OneDense,
    OneAbsence,
    FiveOverlap,
}

#[derive(Debug, Clone)]
pub struct TodayHttpPerfViewer {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct TodayHttpPerfSource {
    pub list_id: SavedListId,
    pub expected_list_revision: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TodayHttpPerfCounts {
    pub people: usize,
    pub inquiries: usize,
    pub contact_facts: usize,
    pub contact_corrections: usize,
    pub inbound_records: usize,
}

#[derive(Debug, Clone)]
struct CaseSources {
    all: [TodayHttpPerfSource; SOURCE_LIMIT],
}

impl CaseSources {
    fn selected(&self, mode: PerfSourceMode) -> &[TodayHttpPerfSource] {
        match mode {
            PerfSourceMode::Zero => &[],
            PerfSourceMode::OneDense => &self.all[..1],
            // Phase A's measured single-absence shape is source #3: the
            // Stage + LastContact Never anti-join. `has_phone=false` remains
            // the fifth member only of the FiveOverlap configuration.
            PerfSourceMode::OneAbsence => &self.all[2..3],
            PerfSourceMode::FiveOverlap => &self.all,
        }
    }
}

pub struct TodayHttpPerfFixture {
    app_options: PgConnectOptions,
    setup_pool: PgPool,
    config: Config,
    fixed_clock: DateTime<Utc>,
    viewers: BTreeMap<TodayHttpPerfCase, TodayHttpPerfViewer>,
    sources: BTreeMap<TodayHttpPerfCase, CaseSources>,
    counts: TodayHttpPerfCounts,
    source_hash: String,
    build_hash: String,
}

impl TodayHttpPerfFixture {
    /// Creates a fresh fixture within sqlx's isolated database. The caller
    /// must use a `#[sqlx::test]` database; this method never targets shared
    /// dev data or reads a URL from the environment.
    pub async fn create(migrator_pool: PgPool) -> Self {
        let build_hash = std::env::var("CRM_SLICE_011C_BUILD_HASH")
            .expect("Phase B requires CRM_SLICE_011C_BUILD_HASH from the exact optimized build");
        assert!(
            build_hash.len() == 64
                && build_hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "CRM_SLICE_011C_BUILD_HASH must be the lowercase SHA-256 of the actual optimized test binary"
        );

        let fixed_clock = chrono::DateTime::parse_from_rfc3339(FIXED_CLOCK)
            .expect("fixed Phase B clock is valid")
            .with_timezone(&Utc);
        let config = crate::common::test_config();
        let organization_id =
            crate::common::create_org(&migrator_pool, "Slice 011c HTTP perf").await;
        crate::common::seed_stages(&migrator_pool, organization_id).await;
        let setup_pool = crate::common::connect_as_app(&migrator_pool).await;
        let stage_id = first_stage_id(&setup_pool, organization_id).await;

        let concentrated =
            create_viewer(&migrator_pool, &setup_pool, organization_id, "concentrated").await;
        let typical = create_viewer(&migrator_pool, &setup_pool, organization_id, "typical").await;
        let partial = create_viewer(&migrator_pool, &setup_pool, organization_id, "partial").await;
        let empty = create_viewer(&migrator_pool, &setup_pool, organization_id, "empty").await;
        let other = create_viewer(&migrator_pool, &setup_pool, organization_id, "other").await;

        seed_people_and_history(
            &setup_pool,
            organization_id,
            stage_id,
            [concentrated.0, typical.0, partial.0, empty.0, other.0],
            fixed_clock,
        )
        .await;

        let viewers_by_case = [
            (TodayHttpPerfCase::Concentrated, concentrated),
            (TodayHttpPerfCase::Typical, typical),
            (TodayHttpPerfCase::PartialBuiltins, partial),
            (TodayHttpPerfCase::EmptyBuiltins, empty),
        ];
        let mut viewers = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for (case, (viewer_id, viewer)) in viewers_by_case {
            let created = create_source_lists(
                &setup_pool,
                organization_id,
                viewer_id,
                stage_id,
                fixed_clock,
            )
            .await;
            viewers.insert(case, viewer);
            sources.insert(case, CaseSources { all: created });
        }

        // ANALYZE is explicitly outside timed waves. The current macro SQL,
        // frozen original query, and all source shapes therefore get the same
        // fresh statistics without relying on planner luck.
        for relation in [
            "person",
            "contact_method",
            "inquiry",
            "contact_attempted",
            "correspondence_raw",
            "correspondence_captured",
            "saved_list",
            "today_work_source",
        ] {
            sqlx::query(&format!("ANALYZE {relation}"))
                .execute(&migrator_pool)
                .await
                .expect("analyze isolated Phase B fixture relation");
        }

        let counts = counts(&setup_pool, organization_id).await;
        assert_eq!(
            counts.people, PEOPLE,
            "fixture must contain the declared 50k People"
        );
        assert_eq!(
            counts.inquiries, EXPECTED_INQUIRIES,
            "fixture must retain the approved inquiry-history cardinality"
        );
        assert_eq!(
            counts.contact_facts, EXPECTED_CONTACT_FACTS,
            "fixture must retain the approved contact-history cardinality"
        );
        assert_eq!(
            counts.contact_corrections, EXPECTED_CONTACT_CORRECTIONS,
            "fixture must retain the approved correction cardinality"
        );
        assert_eq!(
            counts.inbound_records, EXPECTED_INBOUND_RECORDS,
            "fixture must retain the approved final inbound-history cardinality"
        );

        let app_options = migrator_pool
            .connect_options()
            .as_ref()
            .clone()
            .username("crm_app")
            .password(&crate::common::app_password());
        Self {
            app_options,
            setup_pool,
            config,
            fixed_clock,
            viewers,
            sources,
            counts,
            source_hash: current_source_hash(),
            build_hash,
        }
    }

    /// A fresh app-role pool for exactly one sequential arm. Callers must close
    /// it before constructing the next arm, keeping the declared max-10 pool
    /// configuration instead of ever combining original/final arms.
    pub async fn arm_pool(&self) -> PgPool {
        PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(2))
            .connect_with(self.app_options.clone())
            .await
            .expect("connect isolated Phase B app pool")
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn fixed_clock(&self) -> DateTime<Utc> {
        self.fixed_clock
    }

    pub fn counts(&self) -> TodayHttpPerfCounts {
        self.counts
    }

    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }

    pub fn build_hash(&self) -> &str {
        &self.build_hash
    }

    pub fn viewer(&self, case: TodayHttpPerfCase) -> &TodayHttpPerfViewer {
        self.viewers
            .get(&case)
            .expect("declared Phase B viewer exists")
    }

    pub fn sources(&self, case: TodayHttpPerfCase, mode: PerfSourceMode) -> &[TodayHttpPerfSource] {
        self.sources
            .get(&case)
            .expect("declared Phase B source set exists")
            .selected(mode)
    }

    /// Applies the mode through real authenticated source commands and then
    /// checks the private source configuration from its real GET response.
    /// This runs before warm-up, never inside a timed request wave.
    pub async fn assert_sources_via_http(
        &self,
        base_url: &str,
        cookie: &str,
        case: TodayHttpPerfCase,
        mode: PerfSourceMode,
    ) {
        let all = &self
            .sources
            .get(&case)
            .expect("declared Phase B source set exists")
            .all;
        let expected = self.sources(case, mode);
        let client = reqwest::Client::new();

        for source in all {
            let response = client
                .delete(format!(
                    "{base_url}/api/today/sources/{}",
                    source.list_id.as_uuid()
                ))
                .header(COOKIE, cookie)
                .send()
                .await
                .expect("isolated source disable request reaches loopback router");
            assert!(
                response.status().is_success(),
                "source disable setup must succeed"
            );
            let _ = response
                .bytes()
                .await
                .expect("source disable body is complete");
        }
        for source in expected {
            let response = client
                .put(format!(
                    "{base_url}/api/today/sources/{}",
                    source.list_id.as_uuid()
                ))
                .header(COOKIE, cookie)
                .json(&serde_json::json!({
                    "expected_list_revision": source.expected_list_revision,
                }))
                .send()
                .await
                .expect("isolated source enable request reaches loopback router");
            assert!(
                response.status().is_success(),
                "source enable setup must succeed"
            );
            let _ = response
                .bytes()
                .await
                .expect("source enable body is complete");
        }

        let response = client
            .get(format!("{base_url}/api/today/sources"))
            .header(COOKIE, cookie)
            .send()
            .await
            .expect("source configuration GET reaches loopback router");
        assert!(
            response.status().is_success(),
            "source configuration GET must succeed"
        );
        let body = response
            .bytes()
            .await
            .expect("source configuration body is complete");
        let value: serde_json::Value =
            serde_json::from_slice(&body).expect("source configuration response is JSON");
        assert_eq!(
            value.get("limit").and_then(serde_json::Value::as_u64),
            Some(5)
        );
        let observed = value
            .get("sources")
            .and_then(serde_json::Value::as_array)
            .expect("source configuration has source array")
            .iter()
            .filter_map(|source| source.get("list_id"))
            .filter_map(serde_json::Value::as_str)
            .filter_map(|id| id.parse::<Uuid>().ok())
            .collect::<std::collections::BTreeSet<_>>();
        let wanted = expected
            .iter()
            .map(|source| source.list_id.as_uuid())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(observed, wanted, "source command setup must be exact");
    }

    /// Closes setup connections explicitly. sqlx's test harness owns the
    /// ephemeral database itself; this method never attempts a shared-target
    /// drop and does not retain a private URL.
    pub async fn cleanup(self) {
        self.setup_pool.close().await;
    }
}

async fn create_viewer(
    migrator_pool: &PgPool,
    app_pool: &PgPool,
    organization_id: Uuid,
    label: &str,
) -> (Uuid, TodayHttpPerfViewer) {
    let email = format!("slice-011c-{label}@example.invalid");
    let display_name = format!("Slice 011c {label}");
    let mut connection = app_pool.acquire().await.expect("fixture app connection");
    let user_id = admin_queries::insert_app_user(&mut connection, &email, &display_name)
        .await
        .expect("create synthetic perf user");
    let placeholder_hash = crm_api::auth::password::hash_password("initial fixture credential")
        .expect("hash initial fixture credential");
    admin_queries::insert_local_credential(&mut connection, user_id, &placeholder_hash)
        .await
        .expect("insert initial local credential");
    drop(connection);

    // Keep credential provisioning on the actual command path, outside all
    // phases that are measured or retained as request timing evidence.
    set_local_password(
        app_pool,
        SetLocalPassword {
            user_id: user_id.as_uuid(),
            password: PERF_PASSWORD.to_owned(),
        },
    )
    .await
    .expect("set synthetic perf password through command");
    crate::common::add_membership(migrator_pool, organization_id, user_id.as_uuid()).await;

    (
        user_id.as_uuid(),
        TodayHttpPerfViewer {
            email,
            password: PERF_PASSWORD.to_owned(),
        },
    )
}

async fn first_stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id LIMIT 1",
    )
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .expect("isolated fixture stage exists")
}

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn filter(stage_id: Uuid, additional_clauses: Vec<Clause>) -> FilterDefinition {
    let mut clauses = vec![Clause::Stage(StageClause {
        stage_ids: vec![StageId::new(stage_id)],
    })];
    clauses.extend(additional_clauses);
    FilterDefinition {
        version: 1,
        clauses,
    }
}

async fn create_source_lists(
    pool: &PgPool,
    organization_id: Uuid,
    viewer_id: Uuid,
    stage_id: Uuid,
    _fixed_clock: DateTime<Utc>,
) -> [TodayHttpPerfSource; SOURCE_LIMIT] {
    let definitions = [
        vec![Clause::Source(SourceClause {
            sources: vec!["zillow".to_owned()],
        })],
        vec![
            Clause::Source(SourceClause {
                sources: vec!["zillow".to_owned()],
            }),
            Clause::HasPhone(BoolClause { value: true }),
        ],
        vec![Clause::LastContact(AgeClause {
            age: AgeSpec::Never,
        })],
        vec![
            Clause::Source(SourceClause {
                sources: vec!["zillow".to_owned()],
            }),
            Clause::LastContact(AgeClause {
                age: AgeSpec::Never,
            }),
        ],
        vec![Clause::HasPhone(BoolClause { value: false })],
    ];

    let mut sources = Vec::with_capacity(SOURCE_LIMIT);
    for (index, clauses) in definitions.into_iter().enumerate() {
        let created = saved_list::create_saved_list(
            pool,
            &command_context(organization_id, viewer_id),
            CreateSavedList {
                request_id: Uuid::new_v4(),
                scope: SavedListScope::Personal,
                name: format!("Phase B source {}", index + 1),
                filter: filter(stage_id, clauses),
                sort: None,
            },
        )
        .await
        .expect("create Phase B saved-list definition through command");
        sources.push(TodayHttpPerfSource {
            list_id: created.list.id,
            expected_list_revision: created.list.revision,
        });
    }
    sources
        .try_into()
        .expect("declared Phase B source definition count")
}

async fn seed_people_and_history(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    users: [Uuid; 5],
    now: DateTime<Utc>,
) {
    let mut tx = pool
        .begin()
        .await
        .expect("begin isolated fixture transaction");
    sqlx::query(
        "CREATE TEMP TABLE phase_b_people (id uuid PRIMARY KEY, ordinal integer NOT NULL UNIQUE) ON COMMIT DROP",
    )
    .execute(&mut *tx)
    .await
    .expect("create isolated fixture temp table");
    sqlx::query(
        "INSERT INTO phase_b_people (id, ordinal) \
         SELECT md5('slice-011c-phase-b-person-' || value::text)::uuid, value \
         FROM generate_series(1, 50000) AS value",
    )
    .execute(&mut *tx)
    .await
    .expect("seed isolated fixture IDs");
    sqlx::query(
        "INSERT INTO person (id, organization_id, first_name, last_name, stage_id, assigned_user_id, created_at, updated_at) \
         SELECT id, $1, 'Synthetic', lpad(ordinal::text, 5, '0'), $2, \
           CASE WHEN ordinal <= 30000 THEN $3 WHEN ordinal <= 32000 THEN $4 \
                WHEN ordinal <= 32100 THEN $5 WHEN ordinal <= 45000 THEN $6 ELSE NULL END, \
           $7 - ((ordinal % 365) * interval '1 day'), \
           $7 - ((ordinal % 365) * interval '1 day') \
         FROM phase_b_people",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(users[0])
    .bind(users[1])
    .bind(users[2])
    .bind(users[4])
    .bind(now)
    .execute(&mut *tx)
    .await
    .expect("seed isolated fixture People");
    sqlx::query(
        "INSERT INTO contact_method (id, organization_id, person_id, kind, value, normalized_value, created_at) \
         SELECT md5('slice-011c-phase-b-phone-' || ordinal::text)::uuid, $1, id, 'phone', \
           '+1555' || lpad(ordinal::text, 7, '0'), '+1555' || lpad(ordinal::text, 7, '0'), $2 \
         FROM phase_b_people WHERE ordinal % 10 <> 0",
    )
    .bind(organization_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .expect("seed synthetic phones");
    sqlx::query(
        "INSERT INTO contact_method (id, organization_id, person_id, kind, value, normalized_value, created_at) \
         SELECT md5('slice-011c-phase-b-email-' || ordinal::text)::uuid, $1, id, 'email', \
           'synthetic-' || ordinal::text || '@example.invalid', \
           'synthetic-' || ordinal::text || '@example.invalid', $2 \
         FROM phase_b_people WHERE ordinal % 5 <> 0",
    )
    .bind(organization_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .expect("seed synthetic emails");
    sqlx::query(
        "INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, source_external_id, received_at, created_at) \
         SELECT md5('slice-011c-phase-b-inquiry-old-' || ordinal::text)::uuid, $1, id, \
           md5('slice-011c-phase-b-raw-' || ordinal::text)::uuid, \
           CASE WHEN ordinal % 10 < 8 THEN 'zillow' ELSE 'website' END, \
           'fixture-' || ordinal::text, $2 - interval '30 days' - ((ordinal % 20) * interval '1 hour'), $2 \
         FROM phase_b_people WHERE ordinal % 11 <> 0 OR ordinal BETWEEN 32001 AND 32100",
    )
    .bind(organization_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .expect("seed historical inquiries");
    sqlx::query(
        "INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, source_external_id, received_at, created_at) \
         SELECT md5('slice-011c-phase-b-inquiry-waiting-' || ordinal::text)::uuid, $1, id, \
           md5('slice-011c-phase-b-raw-waiting-' || ordinal::text)::uuid, 'zillow', \
           'waiting-' || ordinal::text, $2 - interval '2 hours', $2 \
         FROM phase_b_people \
         WHERE ordinal BETWEEN 1 AND 100 \
            OR (ordinal BETWEEN 30001 AND 32000 AND ordinal % 5 = 0) \
            OR ordinal BETWEEN 32001 AND 32100",
    )
    .bind(organization_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .expect("seed fresh inquiry queues");
    sqlx::query(
        "INSERT INTO contact_attempted \
           (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id, channel, outcome) \
         SELECT md5('slice-011c-phase-b-contact-root-' || ordinal::text)::uuid, $1, 'system', NULL, NULL, 'fixture', \
           $2 - ((ordinal % 20 + 1) * interval '1 day'), $2, \
           md5('slice-011c-phase-b-contact-correlation-' || ordinal::text)::uuid, NULL, NULL, id, 'call', 'no_answer' \
         FROM phase_b_people \
         WHERE (ordinal > 100 OR ordinal > 30000) \
           AND NOT (ordinal BETWEEN 32001 AND 32100) \
           AND NOT (ordinal BETWEEN 30001 AND 32000 AND ordinal % 5 = 0)",
    )
    .bind(organization_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .expect("seed contact facts");
    sqlx::query(
        "INSERT INTO contact_attempted \
           (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id, channel, outcome) \
         SELECT md5('slice-011c-phase-b-contact-correction-' || ordinal::text)::uuid, $1, 'system', NULL, NULL, 'fixture', \
           $2 - ((ordinal % 20 + 1) * interval '1 day'), $2, \
           md5('slice-011c-phase-b-contact-correction-correlation-' || ordinal::text)::uuid, NULL, \
           md5('slice-011c-phase-b-contact-root-' || ordinal::text)::uuid, id, 'call', 'reached' \
         FROM phase_b_people \
         WHERE (ordinal > 100 OR ordinal > 30000) \
           AND NOT (ordinal BETWEEN 32001 AND 32100) \
           AND NOT (ordinal BETWEEN 30001 AND 32000 AND ordinal % 5 = 0) \
           AND ordinal % 17 = 0",
    )
    .bind(organization_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .expect("seed contact corrections");
    sqlx::query(
        "INSERT INTO correspondence_raw (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed) \
         SELECT md5('slice-011c-phase-b-correspondence-raw-' || ordinal::text)::uuid, $1, $2 - interval '12 hours', \
           decode('00', 'hex'), decode('00', 'hex'), \
           decode(md5('slice-011c-phase-b-correspondence-hmac-a-' || ordinal::text) || md5('slice-011c-phase-b-correspondence-hmac-b-' || ordinal::text), 'hex'), \
           1, true \
         FROM phase_b_people WHERE ordinal % 7 = 0 AND ordinal > 30000 AND ordinal NOT BETWEEN 32001 AND 32100",
    )
    .bind(organization_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .expect("seed inbound raw correspondence");
    sqlx::query(
        "INSERT INTO correspondence_captured \
           (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin, occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id, agent_user_id, direction, message_id, thread_key, via, correspondence_raw_id, backdated) \
         SELECT md5('slice-011c-phase-b-correspondence-' || ordinal::text)::uuid, $1, 'system', NULL, NULL, 'fixture', \
           $2 - interval '12 hours', $2, \
           md5('slice-011c-phase-b-correspondence-correlation-' || ordinal::text)::uuid, NULL, NULL, id, $3, 'inbound', \
           'fixture-' || ordinal::text || '@example.invalid', NULL, 'cc', \
           md5('slice-011c-phase-b-correspondence-raw-' || ordinal::text)::uuid, false \
         FROM phase_b_people WHERE ordinal % 7 = 0 AND ordinal > 30000 AND ordinal NOT BETWEEN 32001 AND 32100",
    )
    .bind(organization_id)
    .bind(now)
    .bind(users[4])
    .execute(&mut *tx)
    .await
    .expect("seed inbound correspondence");
    tx.commit().await.expect("commit isolated perf fixture");
}

async fn counts(pool: &PgPool, organization_id: Uuid) -> TodayHttpPerfCounts {
    let people = count(pool, "person", organization_id).await;
    let inquiries = count(pool, "inquiry", organization_id).await;
    let contact_facts = count(pool, "contact_attempted", organization_id).await;
    let contact_corrections: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM contact_attempted WHERE organization_id = $1 AND corrects_id IS NOT NULL",
    )
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .expect("count isolated contact corrections");
    let inbound_records: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM correspondence_captured WHERE organization_id = $1 AND direction = 'inbound'",
    )
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .expect("count isolated inbound correspondence");
    TodayHttpPerfCounts {
        people: people as usize,
        inquiries: inquiries as usize,
        contact_facts: contact_facts as usize,
        contact_corrections: contact_corrections as usize,
        inbound_records: inbound_records as usize,
    }
}

async fn count(pool: &PgPool, relation: &str, organization_id: Uuid) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {relation} WHERE organization_id = $1"
    ))
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .expect("count isolated Phase B fixture relation")
}

/// The raw evidence stores only this digest. The adjacent manifest names
/// every compile-time input, while the explicit release runner retains the
/// per-file SHA-256 manifest beside the raw run. This keeps fixture/filter
/// content out of output without letting a builtin SQL, frozen baseline,
/// router/auth, migration, fixture, driver, or Cargo input change pass as the
/// same measured source.
fn current_source_hash() -> String {
    let mut hasher = Sha256::new();
    for source in [
        include_str!("today_http_perf_manifest.txt"),
        include_str!("../../../crm-app/src/domain/today/mod.rs"),
        include_str!("../../../crm-app/src/domain/today/model.rs"),
        // STEP 6: `crm-app`'s `queries.rs` (Legacy's compiled-in statement)
        // was deleted once the DB-level equivalence suite proved it
        // byte-identical to this frozen copy (see
        // `tests/fixtures/today_f51bff8/README.md`) — included from its new
        // location so this manifest keeps fingerprinting the same text,
        // not a deleted path.
        include_str!("today_f51bff8/queries.rs"),
        include_str!("../../../crm-app/src/domain/today/rank.rs"),
        include_str!("../../../crm-app/src/domain/today/sources.rs"),
        include_str!("../../../crm-app/src/domain/today/source_candidates.sql"),
        include_str!("../../../crm-app/src/domain/today/source_membership.sql"),
        include_str!("../../../crm-app/src/domain/today/test_support.rs"),
        include_str!("../../../crm-app/src/domain/person/filter.rs"),
        include_str!("../../src/auth/session.rs"),
        include_str!("../../src/config.rs"),
        include_str!("../../src/error.rs"),
        include_str!("../../src/routes/today.rs"),
        include_str!("../../src/state.rs"),
        include_str!("../../src/lib.rs"),
        include_str!("../db_today_http_perf.rs"),
        include_str!("../common/mod.rs"),
        include_str!("today_http_perf_driver.rs"),
        include_str!("today_http_perf_fixture.rs"),
        include_str!("today_9d62e86/mod.rs"),
        include_str!("today_9d62e86/model.rs"),
        include_str!("today_9d62e86/queries.rs"),
        include_str!("today_9d62e86/rank.rs"),
        include_str!("../../migrations/20260906000001_saved_list.sql"),
        include_str!("../../migrations/20260906000002_today_work_source.sql"),
        include_str!("../../Cargo.toml"),
        include_str!("../../../../Cargo.toml"),
        include_str!("../../../../Cargo.lock"),
    ] {
        hasher.update(source.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
