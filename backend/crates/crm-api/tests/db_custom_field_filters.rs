//! DB-backed custom-field filter boundaries.  These deliberately use real
//! definition/value rows: the SQL matrices must keep absence and archived
//! option semantics identical to the typed filter model.

use std::collections::HashSet;

use axum::http::StatusCode;
use chrono::{Duration, NaiveDate, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::person::filter::FilterDefinition;
use crm_api::domain::person::sort::{PersonSort, SortDirection, SortKey};
use crm_api::domain::person::{queries as person_queries, PersonVisibilityScope};
use crm_api::domain::saved_list::{self, SavedListError, SavedListScope, UpdateSavedList};
use crm_api::domain::today::sources as today_sources;
use crm_api::domain::today::system_feeds::evaluate as system_feed_queries;
use crm_api::domain::today::system_feeds::{
    canonical_default, canonical_fresh_within_hours, FeedKey, ResolvedFeed,
};
use crm_api::ids::{OrganizationId, UserId};

const PASSWORD: &str = "correct horse battery staple";

struct Fixture {
    organization_id: Uuid,
    actor_id: Uuid,
    text: Uuid,
    text_two: Uuid,
    number: Uuid,
    date: Uuid,
    choice: Uuid,
    hot: Uuid,
    cold: Uuid,
    matched: Uuid,
    other: Uuid,
    absent: Uuid,
}

async fn stage_id(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn field(
    pool: &PgPool,
    organization_id: Uuid,
    actor_id: Uuid,
    label: &str,
    field_type: &str,
    position: i32,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO custom_field (organization_id, label, field_type, position, created_by_user_id) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(organization_id)
    .bind(label)
    .bind(field_type)
    .bind(position)
    .bind(actor_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn option(
    pool: &PgPool,
    organization_id: Uuid,
    field_id: Uuid,
    label: &str,
    position: i32,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO custom_field_option (organization_id, field_id, label, position) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(organization_id)
    .bind(field_id)
    .bind(label)
    .bind(position)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn value(
    pool: &PgPool,
    organization_id: Uuid,
    person_id: Uuid,
    field_id: Uuid,
    field_type: &str,
    text: Option<&str>,
    number: Option<&str>,
    date: Option<NaiveDate>,
    option_id: Option<Uuid>,
) {
    sqlx::query(
        "INSERT INTO person_custom_field_value \
         (organization_id, person_id, field_id, field_type, text_value, number_value, date_value, option_id, \
          updated_by_user_id, origin, correlation_id) \
         VALUES ($1, $2, $3, $4, $5, $6::numeric, $7, $8, NULL, 'migration', gen_random_uuid())",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(field_id)
    .bind(field_type)
    .bind(text)
    .bind(number)
    .bind(date)
    .bind(option_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn fixture(migrator_pool: &PgPool) -> Fixture {
    let (organization_id, actor_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Filter matrix Realty",
        "filter-matrix@acme.test",
        "Filter Matrix",
        PASSWORD,
    )
    .await;
    let pool = crate::common::connect_as_app(migrator_pool).await;
    let stage_id = stage_id(&pool, organization_id).await;
    let matched: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id, assigned_user_id) \
         VALUES ($1, 'Matched', $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let other: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id, assigned_user_id) \
         VALUES ($1, 'Other', $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let absent: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id, assigned_user_id) \
         VALUES ($1, 'Absent', $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let text = field(&pool, organization_id, actor_id, "Literal text", "text", 1).await;
    let text_two = field(&pool, organization_id, actor_id, "Second text", "text", 2).await;
    let number = field(&pool, organization_id, actor_id, "Budget", "number", 3).await;
    let date = field(&pool, organization_id, actor_id, "Anniversary", "date", 4).await;
    let choice = field(&pool, organization_id, actor_id, "Temperature", "choice", 5).await;
    let hot = option(&pool, organization_id, choice, "Hot", 1).await;
    let cold = option(&pool, organization_id, choice, "Cold", 2).await;

    value(
        &pool,
        organization_id,
        matched,
        text,
        "text",
        Some("A%_\\B"),
        None,
        None,
        None,
    )
    .await;
    value(
        &pool,
        organization_id,
        matched,
        text_two,
        "text",
        Some("second field"),
        None,
        None,
        None,
    )
    .await;
    value(
        &pool,
        organization_id,
        matched,
        number,
        "number",
        None,
        Some("0.0000"),
        None,
        None,
    )
    .await;
    value(
        &pool,
        organization_id,
        matched,
        date,
        "date",
        None,
        None,
        Some(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()),
        None,
    )
    .await;
    value(
        &pool,
        organization_id,
        matched,
        choice,
        "choice",
        None,
        None,
        None,
        Some(hot),
    )
    .await;
    value(
        &pool,
        organization_id,
        other,
        text,
        "text",
        Some("ordinary"),
        None,
        None,
        None,
    )
    .await;
    value(
        &pool,
        organization_id,
        other,
        text_two,
        "text",
        Some("different"),
        None,
        None,
        None,
    )
    .await;
    value(
        &pool,
        organization_id,
        other,
        number,
        "number",
        None,
        Some("-1.0000"),
        None,
        None,
    )
    .await;
    value(
        &pool,
        organization_id,
        other,
        date,
        "date",
        None,
        None,
        Some(NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()),
        None,
    )
    .await;
    value(
        &pool,
        organization_id,
        other,
        choice,
        "choice",
        None,
        None,
        None,
        Some(cold),
    )
    .await;
    Fixture {
        organization_id,
        actor_id,
        text,
        text_two,
        number,
        date,
        choice,
        hot,
        cold,
        matched,
        other,
        absent,
    }
}

fn percent_encode(input: &str) -> String {
    input.bytes().fold(String::new(), |mut encoded, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
        encoded
    })
}

async fn people_ids(router: &axum::Router, cookie: &str, filter: Value) -> HashSet<Uuid> {
    let uri = format!("/api/people?filter={}", percent_encode(&filter.to_string()));
    let response = crate::common::get_with_cookie(router, &uri, cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    crate::common::body_json(response).await["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|person| serde_json::from_value(person["id"].clone()).unwrap())
        .collect()
}

fn custom(kind: &str, field_id: Uuid, test: Value) -> Value {
    json!({"version": 1, "clauses": [{"kind": kind, "field_id": field_id, "test": test}]})
}

#[sqlx::test]
#[ignore]
async fn custom_field_filters_preserve_literals_bounds_absence_and_field_identity(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "filter-matrix@acme.test", PASSWORD).await;
    let expected_match = HashSet::from([f.matched]);
    let expected_negative = HashSet::from([f.other, f.absent]);

    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_text",
                f.text,
                json!({"op":"contains","text":"a%_\\b"})
            )
        )
        .await,
        expected_match
    );
    // The inline count statement has its own fixed five-slot matrix. Keep
    // its literal escaping pinned to the People statement rather than only
    // testing the eight file-backed summary matrices through HTTP.
    let count_filter: FilterDefinition = serde_json::from_value(custom(
        "custom_text",
        f.text,
        json!({"op":"contains","text":"a%_\\b"}),
    ))
    .unwrap();
    let app = crate::common::connect_as_app(&migrator_pool).await;
    let mut connection = app.acquire().await.unwrap();
    let (count, truncated) = person_queries::count_filtered_matches(
        &mut connection,
        &PersonVisibilityScope::Organization(OrganizationId::new(f.organization_id)),
        &count_filter.to_query_params(UserId::new(f.actor_id)),
    )
    .await
    .unwrap();
    assert_eq!((count, truncated), (1, false));
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_text",
                f.text,
                json!({"op":"not_contains","text":"a%_\\b"})
            )
        )
        .await,
        expected_negative
    );
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_text",
                f.text_two,
                json!({"op":"contains","text":"second"})
            )
        )
        .await,
        HashSet::from([f.matched])
    );
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_number",
                f.number,
                json!({"op":"range","min":"0","max":"0"})
            )
        )
        .await,
        HashSet::from([f.matched])
    );
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_number",
                f.number,
                json!({"op":"not_range","min":"0","max":"0"})
            )
        )
        .await,
        HashSet::from([f.other, f.absent])
    );
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_date",
                f.date,
                json!({"op":"range","min":"2024-02-29","max":"2024-02-29"})
            )
        )
        .await,
        HashSet::from([f.matched])
    );
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_date",
                f.date,
                json!({"op":"not_range","min":"2024-02-29","max":"2024-02-29"})
            )
        )
        .await,
        HashSet::from([f.other, f.absent])
    );
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_choice",
                f.choice,
                json!({"op":"any_of","option_ids":[f.hot]})
            )
        )
        .await,
        HashSet::from([f.matched])
    );
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_choice",
                f.choice,
                json!({"op":"none_of","option_ids":[f.hot]})
            )
        )
        .await,
        HashSet::from([f.other, f.absent])
    );

    sqlx::query("UPDATE custom_field_option SET archived_at = now() WHERE id = $1")
        .bind(f.hot)
        .execute(&crate::common::connect_as_app(&migrator_pool).await)
        .await
        .unwrap();
    assert_eq!(
        people_ids(
            &router,
            &cookie,
            custom(
                "custom_choice",
                f.choice,
                json!({"op":"any_of","option_ids":[f.hot]})
            )
        )
        .await,
        HashSet::from([f.matched])
    );
}

#[sqlx::test]
#[ignore]
async fn custom_presence_tests_cover_all_types_and_zero(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "filter-matrix@acme.test", PASSWORD).await;
    for (kind, field_id) in [
        ("custom_text", f.text),
        ("custom_number", f.number),
        ("custom_date", f.date),
        ("custom_choice", f.choice),
    ] {
        assert_eq!(
            people_ids(
                &router,
                &cookie,
                custom(kind, field_id, json!({"op":"is_set"}))
            )
            .await,
            HashSet::from([f.matched, f.other]),
            "{kind} treats zero/a value as set",
        );
        assert_eq!(
            people_ids(
                &router,
                &cookie,
                custom(kind, field_id, json!({"op":"is_not_set"}))
            )
            .await,
            HashSet::from([f.absent]),
            "{kind} is_not_set is the absent complement",
        );
    }
}

#[sqlx::test]
#[ignore]
async fn custom_field_filter_reference_errors_are_non_leaking(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "filter-matrix@acme.test", PASSWORD).await;
    let app = crate::common::connect_as_app(&migrator_pool).await;
    let foreign_org =
        crate::common::create_org(&migrator_pool, "foreign custom field filter").await;
    crate::common::add_membership_with(
        &migrator_pool,
        foreign_org,
        f.actor_id,
        crm_api::domain::admin::Role::Member,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let foreign_field = field(&app, foreign_org, f.actor_id, "Foreign text", "text", 1).await;
    for filter in [
        custom("custom_text", Uuid::new_v4(), json!({"op":"is_set"})),
        custom("custom_text", foreign_field, json!({"op":"is_set"})),
        custom("custom_number", f.text, json!({"op":"is_set"})),
    ] {
        let uri = format!("/api/people?filter={}", percent_encode(&filter.to_string()));
        let response = crate::common::get_with_cookie(&router, &uri, &cookie).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            crate::common::body_json(response).await,
            json!({"error":"invalid_field"})
        );
    }
    let other_choice = field(
        &app,
        f.organization_id,
        f.actor_id,
        "Other choice",
        "choice",
        6,
    )
    .await;
    let wrong_field_option = option(&app, f.organization_id, other_choice, "Other", 1).await;
    let foreign_org = crate::common::create_org(&migrator_pool, "foreign choice filter").await;
    crate::common::add_membership_with(
        &migrator_pool,
        foreign_org,
        f.actor_id,
        crm_api::domain::admin::Role::Member,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let foreign_choice = field(&app, foreign_org, f.actor_id, "Foreign choice", "choice", 1).await;
    let foreign_option = option(&app, foreign_org, foreign_choice, "Foreign", 1).await;
    for invalid_option in [Uuid::new_v4(), wrong_field_option, foreign_option] {
        let filter = custom(
            "custom_choice",
            f.choice,
            json!({"op":"any_of","option_ids":[invalid_option]}),
        );
        let uri = format!("/api/people?filter={}", percent_encode(&filter.to_string()));
        let response = crate::common::get_with_cookie(&router, &uri, &cookie).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            crate::common::body_json(response).await,
            json!({"error":"invalid_option"})
        );
    }

    // A definition that becomes inactive after it was a valid filter must
    // not turn a negated predicate into an accidental broad match.
    sqlx::query("UPDATE custom_field SET archived_at = now() WHERE id = $1")
        .bind(f.text)
        .execute(&app)
        .await
        .unwrap();
    let filter = custom(
        "custom_text",
        f.text,
        json!({"op":"not_contains","text":"ordinary"}),
    );
    let uri = format!("/api/people?filter={}", percent_encode(&filter.to_string()));
    let response = crate::common::get_with_cookie(&router, &uri, &cookie).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        crate::common::body_json(response).await,
        json!({"error":"invalid_field"})
    );
}

#[sqlx::test]
#[ignore]
async fn custom_filter_reaches_every_statement_family_with_five_slots(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app = crate::common::connect_as_app(&migrator_pool).await;
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, gen_random_uuid(), 'fixture', $3)",
    )
    .bind(f.organization_id)
    .bind(f.matched)
    .bind(now - Duration::hours(2))
    .execute(&app)
    .await
    .unwrap();
    let call_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO call (id, organization_id, person_id, contact_method_id, caller_user_id, origin, correlation_id, \
          status, end_reason, provider, provider_room, placed_at, ended_at) \
         VALUES ($1, $2, $3, $4, $5, 'web_session', gen_random_uuid(), 'ended', 'agent_hangup', \
          'scripted', 'custom-filter-matrix', $6, $6)",
    )
    .bind(call_id)
    .bind(f.organization_id)
    .bind(f.matched)
    .bind(Uuid::new_v4())
    .bind(f.actor_id)
    .bind(now - Duration::hours(3))
    .execute(&app)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contact_attempted (organization_id, actor_kind, origin, occurred_at, correlation_id, causation_id, \
          person_id, channel, outcome) \
         VALUES ($1, 'system', 'migration', $2, gen_random_uuid(), $3, $4, 'call', 'reached')",
    )
    .bind(f.organization_id)
    .bind(now - Duration::hours(3))
    .bind(call_id)
    .bind(f.matched)
    .execute(&app)
    .await
    .unwrap();

    // A reply-qualified candidate exercises the independent B side of the
    // person-state matrix. Its five custom predicates deliberately differ
    // from A's, so accidentally binding A twice cannot pass this test.
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, gen_random_uuid(), 'fixture', $3)",
    )
    .bind(f.organization_id)
    .bind(f.other)
    .bind(now - Duration::days(30))
    .execute(&app)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contact_attempted \
         (organization_id, actor_kind, origin, occurred_at, correlation_id, person_id, channel, outcome) \
         VALUES ($1, 'system', 'migration', $2, gen_random_uuid(), $3, 'email', 'reached')",
    )
    .bind(f.organization_id)
    .bind(now - Duration::days(2))
    .bind(f.other)
    .execute(&app)
    .await
    .unwrap();
    crate::common::today_system_feed::insert_correspondence(
        &app,
        f.organization_id,
        f.other,
        f.actor_id,
        "inbound",
        now - Duration::hours(1),
    )
    .await;

    let filter_a: FilterDefinition = serde_json::from_value(json!({"version":1,"clauses":[
        {"kind":"custom_text","field_id":f.text,"test":{"op":"contains","text":"a%_\\b"}},
        {"kind":"custom_text","field_id":f.text_two,"test":{"op":"contains","text":"second"}},
        {"kind":"custom_number","field_id":f.number,"test":{"op":"range","min":"0","max":"0"}},
        {"kind":"custom_date","field_id":f.date,"test":{"op":"range","min":"2024-02-29","max":"2024-02-29"}},
        {"kind":"custom_choice","field_id":f.choice,"test":{"op":"any_of","option_ids":[f.hot]}}
    ]}))
    .unwrap();
    let filter_b: FilterDefinition = serde_json::from_value(json!({"version":1,"clauses":[
        {"kind":"custom_text","field_id":f.text,"test":{"op":"contains","text":"ordinary"}},
        {"kind":"custom_text","field_id":f.text_two,"test":{"op":"contains","text":"different"}},
        {"kind":"custom_number","field_id":f.number,"test":{"op":"range","min":"-1","max":"-1"}},
        {"kind":"custom_date","field_id":f.date,"test":{"op":"range","min":"2023-02-28","max":"2023-02-28"}},
        {"kind":"custom_choice","field_id":f.choice,"test":{"op":"any_of","option_ids":[f.cold]}}
    ]}))
    .unwrap();
    filter_a.validate().unwrap();
    filter_b.validate().unwrap();
    let params = filter_a.to_query_params(UserId::new(f.actor_id));
    assert!(params.custom_slot(4).is_some());
    let scope = PersonVisibilityScope::Organization(OrganizationId::new(f.organization_id));
    let mut conn = app.acquire().await.unwrap();

    let (people, _) = person_queries::filtered_summaries(&mut conn, &scope, &params)
        .await
        .unwrap();
    assert_eq!(
        people
            .iter()
            .map(|person| person.id.as_uuid())
            .collect::<HashSet<_>>(),
        HashSet::from([f.matched])
    );
    let (count, _) = person_queries::count_filtered_matches(&mut conn, &scope, &params)
        .await
        .unwrap();
    assert_eq!(count, 1);
    for (key, direction) in [
        (SortKey::Created, SortDirection::Asc),
        (SortKey::Name, SortDirection::Asc),
        (SortKey::Name, SortDirection::Desc),
        (SortKey::Stage, SortDirection::Asc),
        (SortKey::Stage, SortDirection::Desc),
        (SortKey::Assignee, SortDirection::Asc),
        (SortKey::Assignee, SortDirection::Desc),
    ] {
        let (rows, _) = person_queries::filtered_summaries_sorted(
            &mut conn,
            &scope,
            &params,
            PersonSort { key, direction },
        )
        .await
        .unwrap();
        assert_eq!(
            rows.iter()
                .map(|person| person.id.as_uuid())
                .collect::<HashSet<_>>(),
            HashSet::from([f.matched])
        );
    }
    assert_eq!(
        today_sources::source_membership(
            &mut conn,
            OrganizationId::new(f.organization_id),
            &params,
            now,
            &[f.matched]
        )
        .await
        .unwrap(),
        vec![f.matched]
    );
    assert_eq!(
        today_sources::source_candidates(
            &mut conn,
            OrganizationId::new(f.organization_id),
            &params,
            now,
            &[],
            false,
            501
        )
        .await
        .unwrap()
        .iter()
        .map(|row| row.person.id.as_uuid())
        .collect::<HashSet<_>>(),
        HashSet::from([f.matched])
    );

    let resolved = |key, filter: &FilterDefinition| ResolvedFeed {
        feed_key: key,
        enabled: true,
        filter: {
            let mut anchored = canonical_default(key);
            anchored.clauses.extend(filter.clauses.clone());
            anchored
        },
        fresh_within_hours: canonical_fresh_within_hours(key),
        is_default: false,
        fallback: false,
        revision: 0,
        updated_at: now,
        updated_by_user_id: None,
    };
    let (person_state, _) = system_feed_queries::person_state_candidates(
        &mut conn,
        OrganizationId::new(f.organization_id),
        UserId::new(f.actor_id),
        now,
        &resolved(FeedKey::UnansweredInquiry, &filter_a),
        &resolved(FeedKey::ClientReplied, &filter_b),
    )
    .await
    .unwrap();
    assert_eq!(
        person_state
            .iter()
            .map(|row| row.person.id.as_uuid())
            .collect::<HashSet<_>>(),
        HashSet::from([f.matched, f.other])
    );
    let reply = person_state
        .iter()
        .find(|row| row.person.id.as_uuid() == f.other)
        .unwrap();
    assert!(
        reply.client_replied.is_some(),
        "B-only candidate keeps reply reason"
    );
    let inquiry = person_state
        .iter()
        .find(|row| row.person.id.as_uuid() == f.matched)
        .unwrap();
    assert!(
        inquiry.client_replied.is_none(),
        "A-only candidate is not a reply"
    );
    let call_feed = resolved(FeedKey::CallOutcomeNeeded, &filter_a);
    assert_eq!(
        system_feed_queries::call_membership(
            &mut conn,
            OrganizationId::new(f.organization_id),
            UserId::new(f.actor_id),
            &call_feed,
            &[f.matched],
            now
        )
        .await
        .unwrap()
        .iter()
        .map(|row| row.0)
        .collect::<HashSet<_>>(),
        HashSet::from([f.matched])
    );
    assert_eq!(
        system_feed_queries::call_only_candidates(
            &mut conn,
            OrganizationId::new(f.organization_id),
            UserId::new(f.actor_id),
            &call_feed,
            &[],
            201,
            now
        )
        .await
        .unwrap()
        .iter()
        .map(|row| row.person.id.as_uuid())
        .collect::<HashSet<_>>(),
        HashSet::from([f.matched])
    );

    // The defaults are deliberately inspected so this test cannot quietly
    // remove the system-feed anchors while only checking SQL execution.
    assert!(!canonical_default(FeedKey::UnansweredInquiry)
        .clauses
        .is_empty());
}

/// A persisted one-sided range must remain a valid definition.  This also
/// exercises the shared saved-list/source/feed invalidation behavior when a
/// formerly-valid field is archived and later restored.
#[sqlx::test]
#[ignore]
async fn custom_ranges_survive_saved_lifecycle_and_field_archive_restore(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool).await;
    let app = crate::common::connect_as_app(&migrator_pool).await;
    let filter: FilterDefinition = serde_json::from_value(json!({"version":1,"clauses":[
        {"kind":"custom_number","field_id":f.number,"test":{"op":"range","min":"0"}},
        {"kind":"custom_date","field_id":f.date,"test":{"op":"range","max":"2024-02-29"}}
    ]}))
    .unwrap();
    let request_id = Uuid::new_v4();
    let created = crate::common::saved_lists::create_list(
        &app,
        f.organization_id,
        f.actor_id,
        request_id,
        SavedListScope::Personal,
        "One-sided ranges",
        filter.clone(),
    )
    .await;
    let replay = crate::common::saved_lists::create_list(
        &app,
        f.organization_id,
        f.actor_id,
        request_id,
        SavedListScope::Personal,
        "One-sided ranges",
        filter.clone(),
    )
    .await;
    assert!(!replay.created);
    assert_eq!(replay.list.id, created.list.id);
    let stored: Value = sqlx::query_scalar("SELECT filter FROM saved_list WHERE id = $1")
        .bind(created.list.id.as_uuid())
        .fetch_one(&app)
        .await
        .unwrap();
    assert!(stored["clauses"][0]["test"].get("max").is_none());
    assert!(stored["clauses"][1]["test"].get("min").is_none());

    let updated = saved_list::update_saved_list(
        &app,
        &crate::common::saved_lists::command_context(f.organization_id, f.actor_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 1,
            name: "One-sided ranges updated".into(),
            filter: filter.clone(),
            sort: None,
        },
    )
    .await
    .unwrap();
    assert!(updated.changed);
    assert_eq!(updated.list.revision, 2);
    assert!(matches!(
        saved_list::update_saved_list(
            &app,
            &crate::common::saved_lists::command_context(f.organization_id, f.actor_id),
            UpdateSavedList {
                list_id: created.list.id,
                expected_revision: 1,
                name: "stale".into(),
                filter: filter.clone(),
                sort: None,
            },
        )
        .await,
        Err(SavedListError::Conflict)
    ));

    let auth = crate::common::saved_lists::auth_context(
        f.organization_id,
        f.actor_id,
        crm_api::domain::admin::Role::Member,
    );
    let mut conn = app.acquire().await.unwrap();
    let count = saved_list::count_saved_list_matches(&mut conn, &auth, created.list.id, 2)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.count, 1);
    drop(conn);

    today_sources::enable_today_work_source(
        &app,
        &crate::common::saved_lists::command_context(f.organization_id, f.actor_id),
        today_sources::EnableTodayWorkSource {
            list_id: created.list.id,
            expected_list_revision: 2,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(f.organization_id)
    .bind(f.actor_id)
    .execute(&app)
    .await
    .unwrap();
    let mut feed_filter = canonical_default(FeedKey::UnansweredInquiry);
    feed_filter.clauses.push(filter.clauses[0].clone());
    crm_api::domain::today::system_feeds::commands::update_today_system_feed(
        &app,
        &crate::common::saved_lists::command_context(f.organization_id, f.actor_id),
        crm_api::domain::today::system_feeds::commands::UpdateTodaySystemFeed {
            feed_key: FeedKey::UnansweredInquiry,
            expected_revision: 1,
            filter: feed_filter,
            fresh_within_hours: Some(24),
        },
    )
    .await
    .unwrap();

    sqlx::query("UPDATE custom_field SET archived_at = now() WHERE id = $1")
        .bind(f.number)
        .execute(&app)
        .await
        .unwrap();
    let mut conn = app.acquire().await.unwrap();
    assert!(matches!(
        saved_list::count_saved_list_matches(&mut conn, &auth, created.list.id, 2).await,
        Err(SavedListError::InvalidField)
    ));
    let sources = today_sources::list_today_work_sources(&mut conn, &auth)
        .await
        .unwrap();
    assert_eq!(
        sources[0].filter_error.map(|e| e.as_str()),
        Some("invalid_field")
    );
    let feeds = crm_api::domain::today::system_feeds::queries::admin_feed_view(
        &mut conn,
        OrganizationId::new(f.organization_id),
    )
    .await
    .unwrap();
    assert_eq!(feeds[0].filter_error, Some("invalid_field"));
    drop(conn);

    sqlx::query("UPDATE custom_field SET archived_at = NULL WHERE id = $1")
        .bind(f.number)
        .execute(&app)
        .await
        .unwrap();
    let mut conn = app.acquire().await.unwrap();
    assert_eq!(
        saved_list::count_saved_list_matches(&mut conn, &auth, created.list.id, 2)
            .await
            .unwrap()
            .unwrap()
            .count,
        1
    );
    assert!(today_sources::list_today_work_sources(&mut conn, &auth)
        .await
        .unwrap()[0]
        .filter_error
        .is_none());
    let feeds = crm_api::domain::today::system_feeds::queries::admin_feed_view(
        &mut conn,
        OrganizationId::new(f.organization_id),
    )
    .await
    .unwrap();
    assert!(feeds[0].filter_error.is_none());
}
