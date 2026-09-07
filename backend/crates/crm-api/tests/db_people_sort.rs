//! DB-backed tests for `GET /api/people?sort=` (docs/specs/SLICE_011b_SORT.md
//! §11.3–§11.8). Harness + fixture style per `db_people.rs` (isolation/cap
//! patterns) and `db_people_filter.rs` (filter composition, error contract).
//! Run only via ./scripts/check-db.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

// --- Fixture helpers --------------------------------------------------------

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn filter_uri(path: &str, filter: &Value) -> String {
    format!("{path}?filter={}", percent_encode(&filter.to_string()))
}

async fn stage_ids_by_position(pool: &PgPool, org_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position, id")
        .bind(org_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn insert_person(
    pool: &PgPool,
    org_id: Uuid,
    stage_id: Uuid,
    first_name: Option<&str>,
    last_name: Option<&str>,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, first_name, last_name, assigned_user_id)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .bind(first_name)
    .bind(last_name)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Gate-speedup lever 4 style batch insert: `count` bare People, one per
/// caller-supplied `name_prefix{i:04}` last name, `count - 1 - i` seconds
/// apart in `created_at` so `i = 0` is OLDEST and `i = count - 1` is
/// NEWEST — matching insertion order to both `created_at ASC` and
/// alphabetical intuition for zero-padded names.
async fn insert_named_people_batch(
    pool: &PgPool,
    org_id: Uuid,
    stage_id: Uuid,
    name_prefix: &str,
    count: i64,
) {
    sqlx::query(
        "INSERT INTO person (organization_id, stage_id, last_name, created_at)
         SELECT $1, $2, $3 || lpad(s.i::text, 4, '0'), now() - make_interval(secs => ($4 - s.i))
         FROM generate_series(0, $4 - 1) AS s(i)",
    )
    .bind(org_id)
    .bind(stage_id)
    .bind(name_prefix)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

/// Same shape as `insert_named_people_batch`, but every row's `last_name`
/// (and `first_name`) stays NULL — used to exercise `NULLS LAST` under
/// volume. `i = 0` is OLDEST, `i = count - 1` is NEWEST.
async fn insert_null_last_name_batch(pool: &PgPool, org_id: Uuid, stage_id: Uuid, count: i64) {
    sqlx::query(
        "INSERT INTO person (organization_id, stage_id, created_at)
         SELECT $1, $2, now() - make_interval(secs => ($3 - s.i))
         FROM generate_series(0, $3 - 1) AS s(i)",
    )
    .bind(org_id)
    .bind(stage_id)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

/// Minimal Inquiry fixture (mirrors `db_people_filter.rs`'s helper of the
/// same name) — enough to bind a `source` clause's `latest_src` LATERAL
/// probe for the sorted-matrix composition test below.
async fn insert_inquiry(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    source: &str,
    received_at: chrono::DateTime<chrono::Utc>,
) {
    sqlx::query(
        "INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, received_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(source)
    .bind(received_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn people_ids(router: &axum::Router, cookie: &str, uri: &str) -> Vec<String> {
    let body =
        crate::common::body_json(crate::common::get_with_cookie(router, uri, cookie).await).await;
    body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect()
}

async fn people_response(router: &axum::Router, cookie: &str, uri: &str) -> Value {
    crate::common::body_json(crate::common::get_with_cookie(router, uri, cookie).await).await
}

// --- §11.3: default-order parity --------------------------------------------

#[sqlx::test]
#[ignore]
async fn default_order_is_byte_identical_with_and_without_explicit_created_desc(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort default parity",
        "alice@sort-default.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    for i in 0..5 {
        insert_person(
            &migrator_pool,
            org_id,
            stage_id,
            Some("Person"),
            Some(&format!("N{i}")),
            None,
        )
        .await;
    }

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-default.test", "pw").await;

    // Absent filter: `/people` vs `/people?sort=created.desc`.
    let without_sort = crate::common::get_with_cookie(&router, "/api/people", &cookie).await;
    let without_bytes = without_sort.into_body().collect().await.unwrap().to_bytes();
    let with_sort =
        crate::common::get_with_cookie(&router, "/api/people?sort=created.desc", &cookie).await;
    let with_bytes = with_sort.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        without_bytes, with_bytes,
        "absent sort must be byte-identical to sort=created.desc"
    );

    // Present filter: `?filter=X` vs `?filter=X&sort=created.desc`.
    let filter = json!({"version": 1, "clauses": [{"kind": "stage", "stage_ids": [stage_id]}]});
    let filter_only_uri = filter_uri("/api/people", &filter);
    let filter_only = crate::common::get_with_cookie(&router, &filter_only_uri, &cookie).await;
    let filter_only_bytes = filter_only.into_body().collect().await.unwrap().to_bytes();
    let filter_sorted_uri = format!("{filter_only_uri}&sort=created.desc");
    let filter_sorted = crate::common::get_with_cookie(&router, &filter_sorted_uri, &cookie).await;
    let filter_sorted_bytes = filter_sorted
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(
        filter_only_bytes, filter_sorted_bytes,
        "?filter=X must be byte-identical to ?filter=X&sort=created.desc"
    );
}

// --- §11.4: every key and direction ------------------------------------------

#[sqlx::test]
#[ignore]
async fn created_key_orders_both_directions(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort created",
        "alice@sort-created.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    insert_named_people_batch(&migrator_pool, org_id, stage_id, "created", 5).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-created.test", "pw").await;

    let asc = people_response(&router, &cookie, "/api/people?sort=created.asc").await;
    let asc_names: Vec<&str> = asc["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["last_name"].as_str().unwrap())
        .collect();
    assert_eq!(
        asc_names,
        vec![
            "created0000",
            "created0001",
            "created0002",
            "created0003",
            "created0004"
        ],
        "created.asc must be oldest first"
    );

    let desc = people_response(&router, &cookie, "/api/people?sort=created.desc").await;
    let desc_names: Vec<&str> = desc["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["last_name"].as_str().unwrap())
        .collect();
    assert_eq!(
        desc_names,
        vec![
            "created0004",
            "created0003",
            "created0002",
            "created0001",
            "created0000"
        ],
        "created.desc must be newest first (today's default)"
    );
}

/// Missing-last, both directions, and the empty-string-versus-missing-name
/// pin: an empty-string last name sorts as text (first in ASC, ahead of a
/// present name in DESC too... precisely, ahead of NULL in both
/// directions), a missing (`NULL`) last name always sorts LAST.
#[sqlx::test]
#[ignore]
async fn name_key_orders_both_directions_with_missing_last_and_empty_string_pin(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort name",
        "alice@sort-name.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];

    let alpha = insert_person(&migrator_pool, org_id, stage_id, None, Some("Alpha"), None).await;
    let zeta = insert_person(&migrator_pool, org_id, stage_id, None, Some("Zeta"), None).await;
    let missing = insert_person(&migrator_pool, org_id, stage_id, None, None, None).await;
    // An empty-string last name, should a write path ever store one, is
    // distinct from a missing (NULL) name (spec §3).
    let empty = insert_person(&migrator_pool, org_id, stage_id, None, Some(""), None).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-name.test", "pw").await;

    let asc_ids = people_ids(&router, &cookie, "/api/people?sort=name.asc").await;
    assert_eq!(
        asc_ids,
        vec![
            empty.to_string(),
            alpha.to_string(),
            zeta.to_string(),
            missing.to_string(),
        ],
        "name.asc: empty string first (text order), then A-Z, missing name LAST"
    );

    let desc_ids = people_ids(&router, &cookie, "/api/people?sort=name.desc").await;
    assert_eq!(
        desc_ids,
        vec![
            zeta.to_string(),
            alpha.to_string(),
            empty.to_string(),
            missing.to_string(),
        ],
        "name.desc: Z-A, empty string after every non-empty name, missing name LAST even in DESC"
    );
}

/// §3's `name` key table row also names `first_name` as the secondary sort
/// key when `last_name` ties. Three People share the same `last_name`; one
/// has a NULL `first_name`, pinning `first_name ASC/DESC NULLS LAST`.
#[sqlx::test]
#[ignore]
async fn name_key_orders_by_first_name_as_the_secondary_key_with_missing_first_name_last(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort name secondary",
        "alice@sort-name-secondary.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];

    let alpha = insert_person(
        &migrator_pool,
        org_id,
        stage_id,
        Some("Alpha"),
        Some("Smith"),
        None,
    )
    .await;
    let zeta = insert_person(
        &migrator_pool,
        org_id,
        stage_id,
        Some("Zeta"),
        Some("Smith"),
        None,
    )
    .await;
    let missing_first =
        insert_person(&migrator_pool, org_id, stage_id, None, Some("Smith"), None).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-name-secondary.test", "pw").await;

    let asc_ids = people_ids(&router, &cookie, "/api/people?sort=name.asc").await;
    assert_eq!(
        asc_ids,
        vec![
            alpha.to_string(),
            zeta.to_string(),
            missing_first.to_string(),
        ],
        "name.asc: tied last_name -> first_name ASC, missing first_name LAST"
    );

    let desc_ids = people_ids(&router, &cookie, "/api/people?sort=name.desc").await;
    assert_eq!(
        desc_ids,
        vec![
            zeta.to_string(),
            alpha.to_string(),
            missing_first.to_string(),
        ],
        "name.desc: tied last_name -> first_name DESC, missing first_name LAST even in DESC"
    );
}

#[sqlx::test]
#[ignore]
async fn stage_key_orders_by_pipeline_position_both_directions(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort stage",
        "alice@sort-stage.test",
        "Alice",
        "pw",
    )
    .await;
    let stages = stage_ids_by_position(&migrator_pool, org_id).await;
    // D-019 seeds 9 stages; use the first three by position.
    let first = insert_person(&migrator_pool, org_id, stages[0], None, Some("First"), None).await;
    let second = insert_person(
        &migrator_pool,
        org_id,
        stages[1],
        None,
        Some("Second"),
        None,
    )
    .await;
    let third = insert_person(&migrator_pool, org_id, stages[2], None, Some("Third"), None).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-stage.test", "pw").await;

    let asc_ids = people_ids(&router, &cookie, "/api/people?sort=stage.asc").await;
    assert_eq!(
        asc_ids,
        vec![first.to_string(), second.to_string(), third.to_string()],
        "stage.asc must follow pipeline order"
    );
    let desc_ids = people_ids(&router, &cookie, "/api/people?sort=stage.desc").await;
    assert_eq!(
        desc_ids,
        vec![third.to_string(), second.to_string(), first.to_string()],
        "stage.desc must reverse pipeline order"
    );
}

#[sqlx::test]
#[ignore]
async fn assignee_key_orders_both_directions_with_unassigned_last(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort assignee",
        "alice@sort-assignee.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    let amy_id =
        crate::common::create_user(&migrator_pool, "amy@sort-assignee.test", "Amy", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, amy_id).await;
    let zack_id =
        crate::common::create_user(&migrator_pool, "zack@sort-assignee.test", "Zack", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, zack_id).await;

    let amy_person = insert_person(
        &migrator_pool,
        org_id,
        stage_id,
        None,
        Some("A"),
        Some(amy_id),
    )
    .await;
    let zack_person = insert_person(
        &migrator_pool,
        org_id,
        stage_id,
        None,
        Some("Z"),
        Some(zack_id),
    )
    .await;
    let unassigned_person =
        insert_person(&migrator_pool, org_id, stage_id, None, Some("U"), None).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-assignee.test", "pw").await;

    let asc_ids = people_ids(&router, &cookie, "/api/people?sort=assignee.asc").await;
    assert_eq!(
        asc_ids,
        vec![
            amy_person.to_string(),
            zack_person.to_string(),
            unassigned_person.to_string(),
        ],
        "assignee.asc: A-Z by display name, unassigned LAST"
    );
    let desc_ids = people_ids(&router, &cookie, "/api/people?sort=assignee.desc").await;
    assert_eq!(
        desc_ids,
        vec![
            zack_person.to_string(),
            amy_person.to_string(),
            unassigned_person.to_string(),
        ],
        "assignee.desc: Z-A by display name, unassigned LAST even in DESC"
    );
}

/// The tie-break chain for every non-`created` key: equal keys fall back to
/// `created_at DESC, id ASC` — proven with two People sharing the same
/// `stage_id` (so `stage.asc` alone cannot order them) but a fixture-forced
/// tied `created_at`, leaving `id ASC` as the only remaining discriminator.
#[sqlx::test]
#[ignore]
async fn non_created_keys_tie_break_by_created_at_desc_then_id_asc(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort tie break",
        "alice@sort-tiebreak.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    let a = insert_person(&migrator_pool, org_id, stage_id, None, Some("Same"), None).await;
    let b = insert_person(&migrator_pool, org_id, stage_id, None, Some("Same"), None).await;
    let (lower_id, higher_id) = if a < b { (a, b) } else { (b, a) };
    let tied_at = chrono::Utc::now();
    sqlx::query("UPDATE person SET created_at = $1 WHERE id = ANY($2)")
        .bind(tied_at)
        .bind(vec![a, b])
        .execute(&migrator_pool)
        .await
        .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-tiebreak.test", "pw").await;
    let ids = people_ids(&router, &cookie, "/api/people?sort=name.asc").await;
    let lower_pos = ids
        .iter()
        .position(|id| id == &lower_id.to_string())
        .unwrap();
    let higher_pos = ids
        .iter()
        .position(|id| id == &higher_id.to_string())
        .unwrap();
    assert!(
        lower_pos < higher_pos,
        "equal name and created_at -> id ASC: {lower_id} must sort before {higher_id}"
    );
}

/// Strengthens the tie-break pin above: that test ties `created_at` too, so
/// it can only prove `id ASC` is the LAST discriminator. This one gives the
/// lower-UUID row the OLDER `created_at` and the higher-UUID row the NEWER
/// one, so `created_at DESC` and `id ASC` disagree about the order — a
/// wrong implementation that fell straight to `id ASC` (skipping
/// `created_at DESC`) would put the lower id first here, not the higher
/// one. Proven for both `name` and `stage`, per §11.4's "at least name and
/// stage" (both fully tie for this pair: identical `last_name`/no
/// `first_name`, and identical `stage_id`).
#[sqlx::test]
#[ignore]
async fn non_created_keys_apply_created_at_desc_before_id_asc_for_name_and_stage(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort tie precedence",
        "alice@sort-tie-precedence.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];

    let a = insert_person(&migrator_pool, org_id, stage_id, None, Some("Same"), None).await;
    let b = insert_person(&migrator_pool, org_id, stage_id, None, Some("Same"), None).await;
    let (lower_id, higher_id) = if a < b { (a, b) } else { (b, a) };
    sqlx::query("UPDATE person SET created_at = now() - interval '60 seconds' WHERE id = $1")
        .bind(lower_id)
        .execute(&migrator_pool)
        .await
        .unwrap();
    sqlx::query("UPDATE person SET created_at = now() WHERE id = $1")
        .bind(higher_id)
        .execute(&migrator_pool)
        .await
        .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-tie-precedence.test", "pw").await;

    for (key, uri) in [
        ("name", "/api/people?sort=name.asc"),
        ("stage", "/api/people?sort=stage.asc"),
    ] {
        let ids = people_ids(&router, &cookie, uri).await;
        assert_eq!(
            ids,
            vec![higher_id.to_string(), lower_id.to_string()],
            "{key}.asc: fully-tied {key} key -> created_at DESC must outrank id ASC"
        );
    }
}

// --- §11.5: sort before truncation -------------------------------------------

#[sqlx::test]
#[ignore]
async fn sort_applies_before_truncation_so_the_alphabetical_500_differs_from_the_newest_500(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort truncation",
        "alice@sort-truncation.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    // 501 rows: last_name "person0000".."person0500", i = 0 OLDEST .. 500
    // NEWEST. The alphabetical-first 500 (`person0000`..`person0499`) is
    // exactly the OLDEST 500 by construction, so it excludes the single
    // newest row (`person0500`) that `created.desc`'s top-500 would
    // instead include and the alphabetical-last row (`person0500`) would
    // instead exclude — proving the two 500-sets genuinely differ.
    insert_named_people_batch(&migrator_pool, org_id, stage_id, "person", 501).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-truncation.test", "pw").await;

    let default_body = people_response(&router, &cookie, "/api/people").await;
    assert_eq!(default_body["truncated"], true);
    let default_names: std::collections::BTreeSet<String> = default_body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["last_name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(default_names.len(), 500);
    // The default (newest-first) order retains every row EXCEPT the
    // single oldest one.
    assert!(!default_names.contains("person0000"));
    assert!(default_names.contains("person0500"));

    let sorted_body = people_response(&router, &cookie, "/api/people?sort=name.asc").await;
    assert_eq!(sorted_body["truncated"], true);
    let sorted_names: Vec<&str> = sorted_body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["last_name"].as_str().unwrap())
        .collect();
    assert_eq!(sorted_names.len(), 500);
    assert_eq!(sorted_names[0], "person0000");
    assert_eq!(sorted_names[499], "person0499");
    assert!(
        !sorted_names.contains(&"person0500"),
        "name.asc's top 500 must exclude the alphabetically-last row"
    );

    let sorted_set: std::collections::BTreeSet<String> =
        sorted_names.into_iter().map(str::to_string).collect();
    assert_ne!(
        sorted_set, default_names,
        "the alphabetical 500 and the newest 500 must differ"
    );
}

// --- §11.6: composition with a filter --------------------------------------

#[sqlx::test]
#[ignore]
async fn sort_composes_with_a_positive_clause_me_and_empty_clauses(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort composition",
        "alice@sort-composition.test",
        "Alice",
        "pw",
    )
    .await;
    let stages = stage_ids_by_position(&migrator_pool, org_id).await;
    let matching_a = insert_person(
        &migrator_pool,
        org_id,
        stages[0],
        None,
        Some("Zed"),
        Some(alice_id),
    )
    .await;
    let matching_b = insert_person(
        &migrator_pool,
        org_id,
        stages[0],
        None,
        Some("Ann"),
        Some(alice_id),
    )
    .await;
    let non_matching =
        insert_person(&migrator_pool, org_id, stages[1], None, Some("Mid"), None).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-composition.test", "pw").await;

    // A positive clause (stage) plus a sort: membership matches
    // `filtered_summaries`, only order changes.
    let stage_filter =
        json!({"version": 1, "clauses": [{"kind": "stage", "stage_ids": [stages[0]]}]});
    let stage_uri = format!("{}&sort=name.asc", filter_uri("/api/people", &stage_filter));
    let stage_ids_sorted = people_ids(&router, &cookie, &stage_uri).await;
    assert_eq!(
        stage_ids_sorted,
        vec![matching_b.to_string(), matching_a.to_string()],
        "stage clause + name.asc: membership from the clause, order from the sort"
    );
    assert!(!stage_ids_sorted.contains(&non_matching.to_string()));

    // `me` plus a sort.
    let me_filter = json!({
        "version": 1,
        "clauses": [{"kind": "assigned_to", "assignees": ["me"]}]
    });
    let me_uri = format!("{}&sort=name.desc", filter_uri("/api/people", &me_filter));
    let me_ids_sorted = people_ids(&router, &cookie, &me_uri).await;
    assert_eq!(
        me_ids_sorted,
        vec![matching_a.to_string(), matching_b.to_string()],
        "me clause + name.desc: membership resolves the viewer, order from the sort"
    );

    // Empty clauses `[]` plus a sort: the full org slice, sorted.
    let empty_filter = json!({"version": 1, "clauses": []});
    let empty_uri = format!("{}&sort=name.asc", filter_uri("/api/people", &empty_filter));
    let empty_ids_sorted = people_ids(&router, &cookie, &empty_uri).await;
    assert_eq!(
        empty_ids_sorted,
        vec![
            matching_b.to_string(),
            non_matching.to_string(),
            matching_a.to_string(),
        ],
        "empty clauses + sort: every Person, sorted"
    );
}

// --- §11.7: error contract --------------------------------------------------

#[sqlx::test]
#[ignore]
async fn malformed_sort_tokens_are_400_before_pool_acquisition(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort malformed",
        "alice@sort-malformed.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-malformed.test", "pw").await;

    for bad in [
        "NAME.asc",
        "name",
        "name.",
        "name.up",
        "inquiry_count.asc",
        "%20",
        "created.desc.extra",
    ] {
        let uri = format!("/api/people?sort={}", percent_encode(bad));
        let response = crate::common::get_with_cookie(&router, &uri, &cookie).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{bad:?} must be 400"
        );
        assert_eq!(
            crate::common::body_json(response).await["error"],
            "malformed_request",
            "{bad:?}"
        );
    }
}

#[sqlx::test]
#[ignore]
async fn empty_sort_param_is_400(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort empty",
        "alice@sort-empty.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-empty.test", "pw").await;

    let response = crate::common::get_with_cookie(&router, "/api/people?sort=", &cookie).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(response).await["error"],
        "malformed_request"
    );
}

#[sqlx::test]
#[ignore]
async fn repeated_sort_param_is_400(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort repeated",
        "alice@sort-repeated.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-repeated.test", "pw").await;

    let response = crate::common::get_with_cookie(
        &router,
        "/api/people?sort=name.asc&sort=name.desc",
        &cookie,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(response).await["error"],
        "malformed_request"
    );
}

/// A foreign-Organization stage id (which alone would be 422
/// `invalid_stage`) combined with a malformed sort must still be 400, not
/// 422 — sort decoding happens at query extraction, strictly before the
/// filter's own reference validation.
#[sqlx::test]
#[ignore]
async fn foreign_organization_stage_with_a_malformed_sort_is_400_not_422(migrator_pool: PgPool) {
    let (_org_a, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort precedence A",
        "alice@sort-precedence.test",
        "Alice",
        "pw",
    )
    .await;
    let (org_b, _bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort precedence B",
        "bob@sort-precedence.test",
        "Bob",
        "pw",
    )
    .await;
    let foreign_stage_id = stage_ids_by_position(&migrator_pool, org_b).await[0];

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-precedence.test", "pw").await;

    let filter =
        json!({"version": 1, "clauses": [{"kind": "stage", "stage_ids": [foreign_stage_id]}]});
    let uri = format!(
        "{}&sort=not-a-real-token",
        filter_uri("/api/people", &filter)
    );
    let response = crate::common::get_with_cookie(&router, &uri, &cookie).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        crate::common::body_json(response).await["error"],
        "malformed_request"
    );

    // Sanity: the SAME foreign stage, with a VALID sort, is the expected
    // 422 (proving the 400 above is really about the sort, not a
    // pre-existing quirk of the filter fixture).
    let valid_sort_uri = format!("{}&sort=name.asc", filter_uri("/api/people", &filter));
    let valid_sort_response =
        crate::common::get_with_cookie(&router, &valid_sort_uri, &cookie).await;
    assert_eq!(
        valid_sort_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        crate::common::body_json(valid_sort_response).await["error"],
        "invalid_stage"
    );
}

#[sqlx::test]
#[ignore]
async fn unknown_extra_query_param_with_a_valid_sort_is_still_200(migrator_pool: PgPool) {
    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort extra param",
        "alice@sort-extra.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-extra.test", "pw").await;

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/people?sort=name.asc&foo=bar")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// A sort with no ad-hoc filter records no `filter_kinds` on the request
/// span (spec §6) — telemetry stays independent per axis.
#[sqlx::test]
#[ignore]
async fn sort_without_filter_records_no_filter_kinds_on_the_span(migrator_pool: PgPool) {
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for CaptureWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let (_org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort span",
        "alice@sort-span.test",
        "Alice",
        "pw",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-span.test", "pw").await;

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    let guard = tracing::subscriber::set_default(subscriber);

    let response =
        crate::common::get_with_cookie(&router, "/api/people?sort=name.asc", &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    drop(guard);

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        captured.contains("sort=\"name.asc\"") || captured.contains("sort=name.asc"),
        "the static sort token must be recorded: {captured}"
    );
    assert!(
        !captured.contains("filter_kinds=\""),
        "a sort without a filter must record no filter_kinds: {captured}"
    );
}

// --- §11.8: tenant isolation -------------------------------------------------

#[sqlx::test]
#[ignore]
async fn name_asc_never_returns_another_organizations_alphabetically_first_person(
    migrator_pool: PgPool,
) {
    let (org_a, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort isolation A",
        "alice@sort-isolation.test",
        "Alice",
        "pw",
    )
    .await;
    let (org_b, _bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort isolation B",
        "bob@sort-isolation.test",
        "Bob",
        "pw",
    )
    .await;
    let stage_a = stage_ids_by_position(&migrator_pool, org_a).await[0];
    let stage_b = stage_ids_by_position(&migrator_pool, org_b).await[0];

    // Org B's Person is alphabetically first — it must never leak into Org
    // A's `name.asc` list, no matter how it would sort.
    let org_b_first = insert_person(&migrator_pool, org_b, stage_b, None, Some("AAA"), None).await;
    let org_a_person = insert_person(&migrator_pool, org_a, stage_a, None, Some("ZZZ"), None).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let alice_cookie =
        crate::common::login_cookie(&router, "alice@sort-isolation.test", "pw").await;

    let ids = people_ids(&router, &alice_cookie, "/api/people?sort=name.asc").await;
    assert_eq!(ids, vec![org_a_person.to_string()]);
    assert!(!ids.contains(&org_b_first.to_string()));
}

// --- §3: collation --------------------------------------------------------

/// Spec §3's deployment note, made observable: this repo's DB image (like
/// production, once initialized identically) uses a non-`C` locale
/// collation where letter case is a tertiary distinction, not primary —
/// "amy" sorts before "Bob" because 'a' < 'b' at the primary comparison
/// level. Under a `C` (byte-order) collation this would flip: 'B' (0x42)
/// precedes 'a' (0x61). This pin exists so a collation regression fails a
/// test instead of only surfacing as a silent ordering surprise later.
#[sqlx::test]
#[ignore]
async fn name_asc_collation_orders_amy_before_bob_not_by_byte_order(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort collation",
        "alice@sort-collation.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    let amy = insert_person(&migrator_pool, org_id, stage_id, None, Some("amy"), None).await;
    let bob = insert_person(&migrator_pool, org_id, stage_id, None, Some("Bob"), None).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-collation.test", "pw").await;

    let ids = people_ids(&router, &cookie, "/api/people?sort=name.asc").await;
    assert_eq!(
        ids,
        vec![amy.to_string(), bob.to_string()],
        "name.asc must use locale collation (amy, Bob), not C/byte order (Bob, amy)"
    );
}

// --- §11.5 at the boundary --------------------------------------------------

/// Strengthens §11.5's truncation pin: `stage.asc`'s and `created.asc`'s
/// truncation each drop a SPECIFIC single row (the oldest, respectively the
/// newest) rather than merely differing in membership from one another, and
/// a batch that exactly fits under the cap is never marked truncated.
#[sqlx::test]
#[ignore]
async fn ties_at_the_500_boundary_drop_the_expected_single_row(migrator_pool: PgPool) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort boundary ties",
        "alice@sort-boundary.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    // 501 rows in ONE stage: `stage.asc`'s own key ties completely, so its
    // whole order collapses to `created_at DESC, id ASC` — identical to the
    // default order — and `created.asc` is the literal reverse.
    insert_named_people_batch(&migrator_pool, org_id, stage_id, "boundary", 501).await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-boundary.test", "pw").await;

    let stage_body = people_response(&router, &cookie, "/api/people?sort=stage.asc").await;
    assert_eq!(stage_body["truncated"], true);
    let stage_names: std::collections::BTreeSet<String> = stage_body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["last_name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(stage_names.len(), 500);
    assert!(
        !stage_names.contains("boundary0000"),
        "stage.asc: full tie on stage -> created_at DESC drops the single OLDEST row"
    );
    assert!(stage_names.contains("boundary0500"));

    let created_body = people_response(&router, &cookie, "/api/people?sort=created.asc").await;
    assert_eq!(created_body["truncated"], true);
    let created_names: std::collections::BTreeSet<String> = created_body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["last_name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(created_names.len(), 500);
    assert!(
        !created_names.contains("boundary0500"),
        "created.asc drops the single NEWEST row"
    );
    assert!(created_names.contains("boundary0000"));

    // A separate Organization with EXACTLY 500 rows must never truncate.
    let (org_id_2, _bob_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort boundary exact",
        "bob@sort-boundary.test",
        "Bob",
        "pw",
    )
    .await;
    let stage_id_2 = stage_ids_by_position(&migrator_pool, org_id_2).await[0];
    insert_named_people_batch(&migrator_pool, org_id_2, stage_id_2, "exact", 500).await;
    let cookie_2 = crate::common::login_cookie(&router, "bob@sort-boundary.test", "pw").await;
    let exact_body = people_response(&router, &cookie_2, "/api/people?sort=name.asc").await;
    assert_eq!(exact_body["truncated"], false);
    assert_eq!(exact_body["people"].as_array().unwrap().len(), 500);
}

/// §3, §11.4 combined with §11.5: with a NULLS-LAST-heavy tail pushed past
/// the cap, both directions still put the single named row first
/// (`NULLS LAST` applies to `asc` AND `desc`), the NULL block itself orders
/// by the tie-break chain (`created_at DESC, id ASC`), and truncation drops
/// exactly the globally OLDEST null row.
#[sqlx::test]
#[ignore]
async fn null_heavy_tail_respects_nulls_last_and_truncates_the_oldest_null_row(
    migrator_pool: PgPool,
) {
    let (org_id, _alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort null heavy",
        "alice@sort-null-heavy.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    let named = insert_person(&migrator_pool, org_id, stage_id, None, Some("Aaa"), None).await;
    insert_null_last_name_batch(&migrator_pool, org_id, stage_id, 500).await;

    let oldest_null: Uuid = sqlx::query_scalar(
        "SELECT id FROM person WHERE organization_id = $1 AND last_name IS NULL
         ORDER BY created_at ASC, id ASC LIMIT 1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let second_oldest_null: Uuid = sqlx::query_scalar(
        "SELECT id FROM person WHERE organization_id = $1 AND last_name IS NULL
         ORDER BY created_at ASC, id ASC OFFSET 1 LIMIT 1",
    )
    .bind(org_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-null-heavy.test", "pw").await;

    for (label, uri) in [
        ("asc", "/api/people?sort=name.asc"),
        ("desc", "/api/people?sort=name.desc"),
    ] {
        let body = people_response(&router, &cookie, uri).await;
        assert_eq!(body["truncated"], true, "{label}");
        let people = body["people"].as_array().unwrap();
        assert_eq!(people.len(), 500, "{label}");
        assert_eq!(
            people[0]["id"].as_str().unwrap(),
            named.to_string(),
            "{label}: the single non-null name must be first, NULLS LAST in both directions"
        );
        let returned_ids: std::collections::BTreeSet<String> = people
            .iter()
            .map(|p| p["id"].as_str().unwrap().to_string())
            .collect();
        assert!(
            !returned_ids.contains(&oldest_null.to_string()),
            "{label}: the oldest NULL row must be the one truncated"
        );
        assert!(
            returned_ids.contains(&second_oldest_null.to_string()),
            "{label}: the second-oldest NULL row must survive"
        );
    }
}

// --- §11.6 composition, extended -------------------------------------------

/// A `source` clause (which binds the sorted matrix's guarded `latest_src`
/// LATERAL probe, spec §4) composed with `assignee.asc` — membership must
/// still equal the unsorted `filtered_summaries` result for the same
/// filter params; only order changes.
#[sqlx::test]
#[ignore]
async fn sort_composes_with_a_source_clause_binding_the_latest_source_probe(migrator_pool: PgPool) {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Sort source composition",
        "alice@sort-source.test",
        "Alice",
        "pw",
    )
    .await;
    let stage_id = stage_ids_by_position(&migrator_pool, org_id).await[0];
    let zillow_person = insert_person(
        &migrator_pool,
        org_id,
        stage_id,
        None,
        Some("Zed"),
        Some(alice_id),
    )
    .await;
    let other_source_person =
        insert_person(&migrator_pool, org_id, stage_id, None, Some("Ann"), None).await;
    let no_inquiry_person =
        insert_person(&migrator_pool, org_id, stage_id, None, Some("Mid"), None).await;

    insert_inquiry(
        &migrator_pool,
        org_id,
        zillow_person,
        "zillow",
        chrono::Utc::now(),
    )
    .await;
    insert_inquiry(
        &migrator_pool,
        org_id,
        other_source_person,
        "website",
        chrono::Utc::now(),
    )
    .await;

    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "alice@sort-source.test", "pw").await;

    let source_filter =
        json!({"version": 1, "clauses": [{"kind": "source", "sources": ["zillow"]}]});
    let default_ids: std::collections::BTreeSet<String> =
        people_ids(&router, &cookie, &filter_uri("/api/people", &source_filter))
            .await
            .into_iter()
            .collect();
    let sorted_uri = format!(
        "{}&sort=assignee.asc",
        filter_uri("/api/people", &source_filter)
    );
    let sorted_ids: std::collections::BTreeSet<String> = people_ids(&router, &cookie, &sorted_uri)
        .await
        .into_iter()
        .collect();

    assert_eq!(
        default_ids, sorted_ids,
        "source clause + assignee.asc membership must equal filtered_summaries"
    );
    assert_eq!(
        default_ids,
        [zillow_person.to_string()].into_iter().collect()
    );
    assert!(!default_ids.contains(&other_source_person.to_string()));
    assert!(!default_ids.contains(&no_inquiry_person.to_string()));
}
