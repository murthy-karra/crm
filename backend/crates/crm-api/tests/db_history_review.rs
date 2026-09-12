//! Reader trust/bounds tests on genuine imported-review bindings. Additional
//! native facts are inert migrator fixtures, not native command/import evidence.
use crate::{
    common::{body_json, get_with_cookie},
    db_activity_review, db_history_capture_support as capture,
    db_history_import_support as imports,
    import_support::Fixture,
};
use axum::http::StatusCode;
use crm_api::{
    auth::AuthContext,
    domain::{
        admin::Role,
        migration::{
            history_capture_source::Stream,
            history_review::{self as h, Dated, Family, PageQuery, ReviewError, TimelineQuery},
        },
    },
    ids::{OrganizationId, PersonId, UserId},
};
use serde_json::{json, Value};
use sqlx::PgPool;
#[cfg(feature = "perf-harness")]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use uuid::Uuid;
fn auth(f: &Fixture) -> AuthContext {
    AuthContext {
        actor_user_id: UserId::new(f.actor),
        actor_email: "review@synthetic.test".into(),
        actor_display_name: "Review admin".into(),
        active_organization_id: OrganizationId::new(f.org),
        active_organization_name: "Synthetic history review".into(),
        role: Role::Admin,
    }
}
async fn native(pool: &PgPool, f: &Fixture, p: PersonId, n: i64) {
    sqlx::query("INSERT INTO inquiry(organization_id,person_id,raw_payload_id,source,source_external_id,message,received_at) SELECT $1,$2,gen_random_uuid(),'Synthetic source','source-'||g,'INQUIRY_BODY_MUST_NOT_APPEAR','2020-01-01Z'::timestamptz FROM generate_series(1,$3) g").bind(f.org).bind(p.0).bind(n).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO inquiry_received(organization_id,person_id,actor_kind,occurred_at,recorded_at,correlation_id,inquiry_id,raw_payload_id,content_hmac,source,person_created,origin) VALUES($1,$2,'system','2020-01-01Z','2020-01-02Z',gen_random_uuid(),gen_random_uuid(),gen_random_uuid(),decode(repeat('01',32),'hex'),'Synthetic source',false,'migration')").bind(f.org).bind(p.0).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO routing_decision(organization_id,person_id,actor_kind,occurred_at,recorded_at,correlation_id,inquiry_id,strategy,assignee_user_id,origin) VALUES($1,$2,'system','2020-01-01Z','2020-01-02Z',gen_random_uuid(),gen_random_uuid(),'explicit',$3,'migration')").bind(f.org).bind(p.0).bind(f.actor).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO assignment_changed(organization_id,person_id,actor_kind,occurred_at,recorded_at,correlation_id,to_user_id,reason,origin) VALUES($1,$2,'system','2020-01-01Z','2020-01-02Z',gen_random_uuid(),$3,'Synthetic assignment','migration')").bind(f.org).bind(p.0).bind(f.actor).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO stage_changed(organization_id,person_id,actor_kind,occurred_at,recorded_at,correlation_id,to_stage_id,reason,origin) VALUES($1,$2,'system','2020-01-01Z','2020-01-02Z',gen_random_uuid(),$3,'Synthetic stage','migration')").bind(f.org).bind(p.0).bind(f.lead_stage).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO contact_attempted(organization_id,person_id,actor_kind,occurred_at,recorded_at,correlation_id,channel,outcome,origin) SELECT $1,$2,'system','2020-01-01Z'::timestamptz,'2020-01-02Z'::timestamptz,gen_random_uuid(),'call','no_answer','migration' FROM generate_series(1,$3)").bind(f.org).bind(p.0).bind(n).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO call_completed(organization_id,person_id,actor_kind,occurred_at,recorded_at,correlation_id,call_id,contact_method_id,outcome,ended_at,origin) VALUES($1,$2,'system','2020-01-01Z','2020-01-02Z',gen_random_uuid(),gen_random_uuid(),gen_random_uuid(),'no_answer','2020-01-01Z','migration')").bind(f.org).bind(p.0).execute(pool).await.unwrap();
    let raw = Uuid::new_v4();
    sqlx::query("INSERT INTO correspondence_raw(id,organization_id,received_at,nonce,ciphertext,content_hmac,byte_len,processed) VALUES($1,$2,now(),decode('01','hex'),decode('02','hex'),$3,1,true)").bind(raw).bind(f.org).bind(raw.as_bytes().as_slice()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO correspondence_captured(organization_id,person_id,actor_kind,on_behalf_of_user_id,agent_user_id,occurred_at,recorded_at,correlation_id,direction,via,correspondence_raw_id,backdated,origin,message_id) VALUES($1,$2,'system',$3,$3,'2020-01-01Z','2020-01-02Z',gen_random_uuid(),'inbound','cc',$4,true,'migration','HIDDEN_EMAIL_IDENTIFIER')").bind(f.org).bind(p.0).bind(f.actor).bind(raw).execute(pool).await.unwrap();
}
async fn pages(f: &Fixture, p: PersonId, family: Family, dated: Dated, limit: usize) -> Vec<Value> {
    let a = auth(f);
    let mut q = TimelineQuery {
        family: Some(family),
        dated: Some(dated),
        limit: Some(limit),
        cursor: None,
    };
    let mut all = Vec::new();
    let mut revision = None;
    for _ in 0..1000 {
        let page = h::timeline(&f.pool, &f.key, &a, p, &q).await.unwrap();
        assert!(page.items.len() <= limit);
        if let Some(r) = &revision {
            assert_eq!(r, &page.read_revision)
        } else {
            revision = Some(page.read_revision)
        };
        all.extend(page.items);
        q.cursor = page.next_cursor;
        if q.cursor.is_none() {
            return all;
        }
    }
    panic!("bounded fixture did not terminate")
}
#[sqlx::test]
#[ignore]
async fn all_native_families_and_inquiries_are_bounded_body_free_and_fully_traversable(
    pool: PgPool,
) {
    let (f, _, p) = db_activity_review::parent(&pool).await;
    native(&pool, &f, p, 123).await;
    let a = auth(&f);
    let calls = f.reader.calls();
    // Deliberately share rank, UUID and both timestamps across two tables.
    sqlx::query("INSERT INTO inquiry_received(id,organization_id,person_id,actor_kind,origin,occurred_at,recorded_at,correlation_id,inquiry_id,raw_payload_id,content_hmac,source,person_created) SELECT id,organization_id,person_id,'system','migration',occurred_at,recorded_at,gen_random_uuid(),gen_random_uuid(),gen_random_uuid(),decode(repeat('02',32),'hex'),'Tie',false FROM person_imported WHERE organization_id=$1 AND person_id=$2").bind(f.org).bind(p.0).execute(&pool).await.unwrap();
    let original: Uuid =
        sqlx::query_scalar("SELECT id FROM contact_attempted WHERE organization_id=$1 LIMIT 1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap();
    let correction:Uuid=sqlx::query_scalar("INSERT INTO contact_attempted(organization_id,person_id,actor_kind,origin,occurred_at,recorded_at,correlation_id,channel,outcome,corrects_id) VALUES($1,$2,'system','migration','2020-01-01Z','2030-01-01Z',gen_random_uuid(),'call','reached',$3) RETURNING id").bind(f.org).bind(p.0).bind(original).fetch_one(&pool).await.unwrap();
    let rows = pages(&f, p, Family::All, Dated::Known, 7).await;
    let kinds: BTreeSet<_> = rows.iter().map(|r| r["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        BTreeSet::from([
            "person_imported",
            "inquiry_received",
            "routing_decision",
            "assignment_changed",
            "stage_changed",
            "contact_attempted",
            "call_completed",
            "correspondence"
        ])
    );
    let ids: BTreeSet<_> = rows
        .iter()
        .map(|r| (r["kind"].as_str().unwrap(), r["id"].as_str().unwrap()))
        .collect();
    assert_eq!(ids.len(), rows.len());
    assert_eq!(rows[0]["id"], correction.to_string());
    assert_eq!(rows[0]["display_at"], "2030-01-01T00:00:00Z");
    assert_eq!(rows[0]["occurred_at"], "2020-01-01T00:00:00Z");
    let detail = h::entry(&f.pool, &f.key, &a, p, "contact_attempted", original)
        .await
        .unwrap();
    assert_eq!(detail["metadata"]["superseded"], true);
    assert!(matches!(
        h::entry(&f.pool, &f.key, &a, p, "call_completed", original).await,
        Err(ReviewError::NotFound)
    ));
    let core = h::core(&f.pool, &a, p).await.unwrap();
    assert!(core.get("inquiries").unwrap().is_object());
    assert!(core.get("core_history").is_none());
    assert_eq!(core["person"]["inquiry_count"], "123");
    assert_eq!(
        core["history"]["known_count"]
            .as_str()
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        rows.len()
    );
    let mut query = PageQuery {
        limit: Some(50),
        cursor: None,
    };
    let mut inquiry_ids = BTreeSet::new();
    loop {
        let page = h::inquiries(&f.pool, &f.key, &a, p, &query).await.unwrap();
        assert!(page.items.len() <= 50);
        for item in page.items {
            assert!(item.get("message").is_none());
            assert!(inquiry_ids.insert(item["id"].as_str().unwrap().to_owned()));
        }
        query.cursor = page.next_cursor;
        if query.cursor.is_none() {
            break;
        }
    }
    assert_eq!(inquiry_ids.len(), 123);
    let text = serde_json::to_string(&rows).unwrap();
    assert!(!text.contains("INQUIRY_BODY_MUST_NOT_APPEAR"));
    assert!(!text.contains("HIDDEN_EMAIL_IDENTIFIER"));
    assert_eq!(f.reader.calls(), calls);
    assert!(pages(&f, p, Family::Native, Dated::Unknown, 50)
        .await
        .is_empty());
    assert!(pages(&f, p, Family::Calls, Dated::Known, 50)
        .await
        .is_empty());
}
#[sqlx::test]
#[ignore]
async fn current_authority_cursor_scope_and_partial_parent_are_enforced(pool: PgPool) {
    let (f, parent, p) = db_activity_review::parent(&pool).await;
    native(&pool, &f, p, 3).await;
    let (other, _, foreign) = db_activity_review::parent(&pool).await;
    let base = format!("/api/people/{p}/migration-review");
    for suffix in [
        "/v2",
        "/inquiries",
        "/timeline",
        "/timeline/contact_attempted/00000000-0000-0000-0000-000000000000",
    ] {
        for (cookie, status) in [
            (f.member_cookie.as_str(), StatusCode::FORBIDDEN),
            ("", StatusCode::UNAUTHORIZED),
        ] {
            let response = get_with_cookie(&f.app, &format!("{base}{suffix}"), cookie).await;
            assert_eq!(response.status(), status);
            assert_eq!(response.headers()["cache-control"], "no-store");
        }
    }
    for suffix in ["/v2", "/inquiries", "/timeline"] {
        let response = get_with_cookie(
            &f.app,
            &format!("/api/people/{foreign}/migration-review{suffix}"),
            &f.cookie,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    let a = auth(&f);
    let page = h::timeline(
        &f.pool,
        &f.key,
        &a,
        p,
        &TimelineQuery {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let token = page.next_cursor.unwrap();
    for path in [
        format!("{base}/timeline?limit=51"),
        format!("{base}/timeline?family=arbitrary"),
        format!("{base}/timeline?dated=all"),
        format!("{base}/timeline?cursor=bad"),
        format!("{base}/timeline?limit=2&cursor={token}"),
        format!("{base}/inquiries?limit=1&cursor={token}"),
    ] {
        assert_eq!(
            get_with_cookie(&f.app, &path, &f.cookie).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        get_with_cookie(
            &other.app,
            &format!("/api/people/{foreign}/migration-review/timeline?limit=1&cursor={token}"),
            &other.cookie
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    sqlx::query("UPDATE migration_import SET state='cancelled' WHERE id=$1")
        .bind(parent)
        .execute(&pool)
        .await
        .unwrap();
    assert!(h::core(&f.pool, &a, p).await.is_ok());
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        h::core(&f.pool, &a, p).await,
        Err(ReviewError::Forbidden)
    ));
}
#[sqlx::test]
#[ignore]
async fn maintained_counts_metadata_changes_and_backdated_inserts_require_refresh(pool: PgPool) {
    let (f, _, p) = db_activity_review::parent(&pool).await;
    native(&pool, &f, p, 2).await;
    let a = auth(&f);
    let mut previous = h::core(&f.pool, &a, p).await.unwrap();
    let mut q = TimelineQuery {
        limit: Some(1),
        ..Default::default()
    };
    q.cursor = h::timeline(&f.pool, &f.key, &a, p, &q)
        .await
        .unwrap()
        .next_cursor;
    for sql in [
        "UPDATE app_user SET display_name=display_name||' changed' WHERE id=$1",
        "UPDATE stage SET name=name||' changed' WHERE id=$1",
    ] {
        sqlx::query(sql)
            .bind(if sql.contains("app_user") {
                f.actor
            } else {
                f.lead_stage
            })
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            h::timeline(&f.pool, &f.key, &a, p, &q).await,
            Err(ReviewError::RefreshRequired)
        ));
        let next = h::core(&f.pool, &a, p).await.unwrap();
        assert_ne!(
            previous["history"]["read_revision"],
            next["history"]["read_revision"]
        );
        assert_eq!(
            previous["history"]["known_count"],
            next["history"]["known_count"]
        );
        previous = next;
        q.cursor = None;
        q.cursor = h::timeline(&f.pool, &f.key, &a, p, &q)
            .await
            .unwrap()
            .next_cursor;
    }
    sqlx::query("INSERT INTO contact_attempted(organization_id,person_id,actor_kind,origin,occurred_at,correlation_id,channel,outcome) VALUES($1,$2,'system','migration','1900-01-01Z',gen_random_uuid(),'other','sent')").bind(f.org).bind(p.0).execute(&pool).await.unwrap();
    assert!(matches!(
        h::timeline(&f.pool, &f.key, &a, p, &q).await,
        Err(ReviewError::RefreshRequired)
    ));
    let response = get_with_cookie(
        &f.app,
        &format!(
            "/api/people/{p}/migration-review/timeline?limit=1&cursor={}",
            q.cursor.unwrap()
        ),
        &f.cookie,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(body_json(response)
        .await
        .to_string()
        .contains("history_refresh_required"));
}
#[sqlx::test]
#[ignore]
async fn committed_write_between_families_cannot_mix_snapshot_rows_and_revision(pool: PgPool) {
    let (f, _, p) = db_activity_review::parent(&pool).await;
    let a = auth(&f);
    let before = h::core(&f.pool, &a, p).await.unwrap();
    let gate = h::SnapshotTestGate::default();
    let q = TimelineQuery {
        limit: Some(50),
        ..Default::default()
    };
    let reader = h::timeline_with_test_gate(&f.pool, &f.key, &a, p, &q, &gate);
    let writer = async {
        tokio::time::timeout(std::time::Duration::from_secs(5), gate.reached.notified())
            .await
            .unwrap();
        sqlx::query("INSERT INTO contact_attempted(organization_id,person_id,actor_kind,origin,occurred_at,correlation_id,channel,outcome) VALUES($1,$2,'system','migration','1900-01-01Z',gen_random_uuid(),'other','sent')").bind(f.org).bind(p.0).execute(&pool).await.unwrap();
        gate.proceed.notify_one();
    };
    let (page, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(reader, writer)
    })
    .await
    .unwrap();
    let page = page.unwrap();
    assert_eq!(
        page.read_revision,
        before["history"]["read_revision"].as_str().unwrap()
    );
    assert!(!page.items.iter().any(|v| v["kind"] == "contact_attempted"));
    let after = h::core(&f.pool, &a, p).await.unwrap();
    assert_ne!(
        after["history"]["read_revision"],
        before["history"]["read_revision"]
    );
    assert_eq!(after["history"]["counts"]["contact_attempted"], "1");
}
#[sqlx::test]
#[ignore]
async fn imported_families_known_unknown_filters_provenance_and_suppression(pool: PgPool) {
    let (f, parent, book) = capture::fixture(&pool).await;
    for stream in [Stream::Events, Stream::Calls, Stream::TextMessages] {
        book.set_records(stream,(1..=70).map(|id|json!({"id":id,"personId":101,"userId":3,"created":if id%2==0{"2020-01-01T00:00:00Z"}else{"unknown"},"type":"Synthetic","message":"BODY_NOT_VISIBLE","phone":"PHONE_NOT_VISIBLE","url":"https://invalid.example/never"})).collect());
    }
    let (capture_id, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, capture_id).await;
    capture::drain(&f, &book).await;
    let run = imports::ready(&f, parent, capture_id).await;
    imports::confirm(&f, run).await;
    imports::drain(&f).await;
    let p=PersonId::new(sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND family='people' AND source_id='101'").bind(f.org).fetch_one(&pool).await.unwrap());
    let a = auth(&f);
    let requests = book.count();
    for family in [Family::Events, Family::Calls, Family::TextMessages] {
        for dated in [Dated::Known, Dated::Unknown] {
            let rows = pages(&f, p, family, dated, 7).await;
            assert_eq!(rows.len(), 35);
            assert_eq!(
                rows.iter()
                    .map(|v| v["id"].as_str().unwrap())
                    .collect::<BTreeSet<_>>()
                    .len(),
                35
            );
            for item in rows {
                assert_eq!(item["display_at"].is_null(), dated == Dated::Unknown);
                let text = item.to_string();
                assert!(!text.contains("BODY_NOT_VISIBLE"));
                assert!(!text.contains("PHONE_NOT_VISIBLE"));
                assert!(!text.contains("invalid.example"));
            }
        }
    }
    let q = TimelineQuery {
        family: Some(Family::Calls),
        limit: Some(1),
        ..Default::default()
    };
    let page = h::timeline(&f.pool, &f.key, &a, p, &q).await.unwrap();
    let item = &page.items[0];
    let id = Uuid::parse_str(item["id"].as_str().unwrap()).unwrap();
    let detail = h::entry(&f.pool, &f.key, &a, p, "fub_call_record_imported", id)
        .await
        .unwrap();
    assert_eq!(
        detail["provenance"]["source_time_basis"],
        "fub_record_created"
    );
    assert!(detail["provenance"]["stable_position"].is_string());
    assert_eq!(detail["actor"]["id"], f.actor.to_string());
    let identity = Uuid::parse_str(detail["provenance"]["identity_id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE id=$1",
    )
    .bind(identity)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        h::entry(&f.pool, &f.key, &a, p, "fub_call_record_imported", id).await,
        Err(ReviewError::NotFound)
    ));
    assert!(matches!(
        h::timeline(
            &f.pool,
            &f.key,
            &a,
            p,
            &TimelineQuery {
                cursor: page.next_cursor,
                ..q
            }
        )
        .await,
        Err(ReviewError::RefreshRequired)
    ));
    let core = h::core(&f.pool, &a, p).await.unwrap();
    assert_eq!(core["history"]["counts"]["fub_call_record_imported"], "69");
    assert_eq!(book.count(), requests);
}

/// One opt-in paired Today workload and plan-shape run. Five concurrent ordinary
/// fixed-clock reads before/during/after import preserve exact results; a bounded
/// Person review read represents one active tab alongside the before/after waves.
#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore]
async fn dense_and_sparse_history_review_plans_on_25000_people(pool: PgPool) {
    use std::time::Instant;
    let (f, parent, capture_id, book) = imports::fixture(&pool).await;
    let run = imports::ready(&f, parent, capture_id).await;
    let (ordinary_org, viewers) = ordinary_today_fixture(&pool).await;
    let p=PersonId::new(sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND family='people' AND source_id='101'").bind(f.org).fetch_one(&pool).await.unwrap());
    // 25,000 People in this Organization, 75,000 distributed native rows and a
    // dense target Person; exactly one external call makes its filter sparse.
    review_scale_fixture(&pool, &f, p, 501).await;
    for table in [
        "person",
        "inquiry",
        "contact_attempted",
        "migration_history_review_state",
        "fub_call_record_imported",
        "migration_history_import_identity",
        "migration_history_import_display",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    let fixed_now: chrono::DateTime<chrono::Utc> = "2026-09-12T12:00:00Z".parse().unwrap();
    let a = auth(&f);
    let (before_today, before_review) = tokio::join!(
        today_wave(&f.pool, ordinary_org, &viewers, fixed_now),
        h::core(&f.pool, &a, p)
    );
    before_review.unwrap();
    let native_writers_before = native_writer_plans(&pool, &f, p).await;
    imports::confirm(&f, run).await;
    let ((), during_today) = tokio::join!(
        imports::drain(&f),
        today_wave(&f.pool, ordinary_org, &viewers, fixed_now)
    );
    let native_writers_after = native_writer_plans(&pool, &f, p).await;
    let (after_today, after_review) = tokio::join!(
        today_wave(&f.pool, ordinary_org, &viewers, fixed_now),
        h::core(&f.pool, &a, p)
    );
    after_review.unwrap();
    for index in 0..5 {
        assert_eq!(
            before_today[index].0, during_today[index].0,
            "Today changed while importing another review Org"
        );
        assert_eq!(
            before_today[index].0, after_today[index].0,
            "Today changed after retained import"
        );
    }
    let paired_today = json!({"organization_mode":"operational","people":"25000","members":"50","concurrent_loads":5,"fixed_clock":fixed_now,"same_results_before_during_after":true,
      "before_us":before_today.iter().map(|v|v.1.to_string()).collect::<Vec<_>>(),"during_us":during_today.iter().map(|v|v.1.to_string()).collect::<Vec<_>>(),"after_us":after_today.iter().map(|v|v.1.to_string()).collect::<Vec<_>>(),"result_rows":before_today.iter().map(|v|v.0["items"].as_array().unwrap().len()).collect::<Vec<_>>(),"active_review_reads":2});
    let source_calls = book.count();
    let started = Instant::now();
    let core = h::core(&f.pool, &a, p).await.unwrap();
    let core_us = started.elapsed().as_micros();
    let mut evidence = BTreeMap::new();
    for (label, family, kind) in [
        ("dense_native", Family::Native, "contact_attempted"),
        ("sparse_external", Family::Calls, "fub_call_record_imported"),
    ] {
        let started = Instant::now();
        let page = h::timeline(
            &f.pool,
            &f.key,
            &a,
            p,
            &TimelineQuery {
                family: Some(family),
                limit: Some(50),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let elapsed = started.elapsed().as_micros();
        assert!(page.items.len() <= 50);
        if family == Family::Calls {
            assert_eq!(page.items.len(), 1)
        } else {
            assert_eq!(page.items.len(), 50)
        }
        let sql = format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            h::candidate_sql_for_test(kind, Dated::Known).unwrap()
        );
        let plan: Value = sqlx::query_scalar(&sql)
            .bind(f.org)
            .bind(p.0)
            .bind(None::<chrono::DateTime<chrono::Utc>>)
            .bind(None::<chrono::DateTime<chrono::Utc>>)
            .bind(None::<i16>)
            .bind(None::<Uuid>)
            .bind(None::<String>)
            .bind(51i64)
            .bind(None::<i64>)
            .fetch_one(&pool)
            .await
            .unwrap();
        evidence.insert(
            label,
            json!({"returned":page.items.len(),"elapsed_us":elapsed.to_string(),"plan":plan}),
        );
    }
    let boundary: chrono::DateTime<chrono::Utc> = "2020-01-01T00:04:10Z".parse().unwrap();
    let deep_sql = format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        h::candidate_after_sql_for_test("contact_attempted", Dated::Known, "contact_attempted")
            .unwrap()
    );
    let deep_plan: Value = sqlx::query_scalar(&deep_sql)
        .bind(f.org)
        .bind(p.0)
        .bind(Some(boundary))
        .bind(Some(boundary))
        .bind(Some(4i16))
        .bind(Some(Uuid::from_bytes([255; 16])))
        .bind(Some("contact_attempted"))
        .bind(51i64)
        .bind(None::<i64>)
        .fetch_one(&pool)
        .await
        .unwrap();
    evidence.insert(
        "dense_after_cursor",
        json!({"boundary_seconds":"250","plan":deep_plan}),
    );
    let hot_queries = all_hot_query_plans(&pool, &f, p).await;
    let state: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        h::STATE_SQL
    ))
    .bind(f.org)
    .bind(p.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let persons: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(persons, 25000);
    let native_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM contact_attempted WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(native_count, 75501);
    assert_eq!(core["history"]["counts"]["contact_attempted"], "504");
    assert_eq!(book.count(), source_calls);
    println!(
        "HISTORY_REVIEW_PLAN_EVIDENCE {}",
        json!({"people":persons.to_string(),"native_rows":native_count.to_string(),"dense_person_native":"504","sparse_external_calls":"1","core_elapsed_us":core_us.to_string(),"state_plan":state,"queries":evidence,"paired_today":paired_today,"native_writer_plans_before":native_writers_before,"native_writer_plans_after":native_writers_after,"all_reader_hot_queries":hot_queries,"limitations":"Synthetic fixed-clock SQL reader workload; no public browser, vendor-completeness or host capacity claim"})
    );
}

#[cfg(feature = "perf-harness")]
async fn review_scale_fixture(pool: &PgPool, f: &Fixture, p: PersonId, dense: i64) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("CREATE TEMP TABLE review_scale_members ON COMMIT DROP AS SELECT gen_random_uuid() AS id FROM generate_series(1,48)").execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,id::text||'@review-scale.synthetic.test','Synthetic member' FROM review_scale_members").execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM review_scale_members").bind(f.org).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Synthetic distribution '||g,$2,$3 FROM generate_series(1,24998) g").bind(f.org).bind(f.lead_stage).bind(f.actor).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO contact_attempted(organization_id,person_id,actor_kind,origin,occurred_at,recorded_at,correlation_id,channel,outcome) SELECT p.organization_id,p.id,'system','migration','2019-01-01Z'::timestamptz+g*interval '1 second','2019-01-01Z'::timestamptz+g*interval '1 second',gen_random_uuid(),'call','no_answer' FROM person p CROSS JOIN generate_series(1,3) g WHERE p.organization_id=$1").bind(f.org).execute(pool).await.unwrap();
    native(pool, f, p, dense).await;
}

#[cfg(feature = "perf-harness")]
async fn ordinary_today_fixture(pool: &PgPool) -> (Uuid, Vec<Uuid>) {
    let org = crate::common::create_org(pool, "Synthetic paired Today").await;
    crate::common::seed_stages(pool, org).await;
    let stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(org)
    .fetch_one(pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(role, "crm_migrator");
    sqlx::query("CREATE TEMP TABLE history_today_members ON COMMIT DROP AS SELECT gen_random_uuid() AS id FROM generate_series(1,50)").execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app_user(id,email,display_name) SELECT id,id::text||'@today-scale.synthetic.test','Synthetic Today member' FROM history_today_members").execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT $1,id,'member','active' FROM history_today_members").bind(org).execute(&mut *tx).await.unwrap();
    let members: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM history_today_members ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Synthetic Today '||g,$2,($3::uuid[])[1+((g-1)%50)] FROM generate_series(1,25000) g").bind(org).bind(stage).bind(&members).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO inquiry(organization_id,person_id,raw_payload_id,source,received_at) SELECT $1,id,gen_random_uuid(),'Synthetic intake','2026-09-12T11:00:00Z' FROM person WHERE organization_id=$1").bind(org).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    for table in ["person", "inquiry", "organization_membership"] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(pool)
            .await
            .unwrap();
    }
    let counts:Value=sqlx::query_scalar("SELECT jsonb_build_object('mode',o.workspace_mode,'people',(SELECT count(*) FROM person WHERE organization_id=o.id),'members',(SELECT count(*) FROM organization_membership WHERE organization_id=o.id)) FROM organization o WHERE o.id=$1").bind(org).fetch_one(pool).await.unwrap();
    assert_eq!(
        counts,
        json!({"mode":"operational","people":25000,"members":50})
    );
    (org, members)
}
#[cfg(feature = "perf-harness")]
async fn today_wave(
    pool: &PgPool,
    org: Uuid,
    viewers: &[Uuid],
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<(Value, u128)> {
    use crm_api::domain::{person::visibility::PersonVisibilityScope, today};
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(6));
    let mut tasks = tokio::task::JoinSet::new();
    for (index, viewer) in viewers.iter().take(5).copied().enumerate() {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            let connection = pool.acquire().await.unwrap();
            barrier.wait().await;
            let start = std::time::Instant::now();
            let list = today::query_owned_at(
                connection,
                &PersonVisibilityScope::Organization(OrganizationId::new(org)),
                UserId::new(viewer),
                now,
            )
            .await
            .unwrap();
            let elapsed = start.elapsed().as_micros();
            let value = serde_json::to_value(list).unwrap();
            assert!(!value["items"].as_array().unwrap().is_empty());
            (index, value, elapsed)
        });
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), barrier.wait())
        .await
        .unwrap();
    let mut outcomes = Vec::new();
    while let Some(result) = tasks.join_next().await {
        outcomes.push(result.unwrap());
    }
    outcomes.sort_by_key(|v| v.0);
    outcomes.into_iter().map(|(_, v, t)| (v, t)).collect()
}

#[cfg(feature = "perf-harness")]
const NATIVE_TABLES: [(&str, &str); 8] = [
    ("person_imported", "person_imported"),
    ("inquiry_received", "inquiry_received"),
    ("routing_decision", "routing_decision"),
    ("assignment_changed", "assignment_changed"),
    ("stage_changed", "stage_changed"),
    ("contact_attempted", "contact_attempted"),
    ("call_completed", "call_completed"),
    ("correspondence", "correspondence_captured"),
];
#[cfg(feature = "perf-harness")]
async fn native_writer_plans(pool: &PgPool, f: &Fixture, p: PersonId) -> Value {
    // EXPLAIN's actual trigger timings measure the new revision work without
    // disabling triggers or modifying an imported Person. Every inert insert is
    // rolled back; these are storage probes, not command/fidelity evidence.
    let target:Uuid=sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1 AND id<>$2 AND first_name LIKE 'Synthetic distribution%' LIMIT 1").bind(f.org).bind(p.0).fetch_one(pool).await.unwrap();
    let mut result = BTreeMap::new();
    for (kind, table) in NATIVE_TABLES {
        let mut tx = pool.begin().await.unwrap();
        let before:Value=sqlx::query_scalar("SELECT jsonb_build_object('revision',revision,'count',counts->$3::text) FROM migration_history_review_state WHERE organization_id=$1 AND person_id=$2").bind(f.org).bind(target).bind(kind).fetch_one(&mut *tx).await.unwrap();
        let sql=format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) INSERT INTO {table} SELECT (jsonb_populate_record(NULL::{table},to_jsonb(src)||jsonb_build_object('id',gen_random_uuid(),'person_id',$2::uuid))).* FROM {table} src WHERE organization_id=$1 AND person_id=$3 LIMIT 1");
        let plan: Value = sqlx::query_scalar(&sql)
            .bind(f.org)
            .bind(target)
            .bind(p.0)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let after:Value=sqlx::query_scalar("SELECT jsonb_build_object('revision',revision,'count',counts->$3::text) FROM migration_history_review_state WHERE organization_id=$1 AND person_id=$2").bind(f.org).bind(target).bind(kind).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(
            after["count"].as_i64().unwrap(),
            before["count"].as_i64().unwrap() + 1
        );
        assert!(after["revision"].as_i64().unwrap() > before["revision"].as_i64().unwrap());
        assert!(plan[0]["Triggers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["Trigger Name"]
                .as_str()
                .is_some_and(|name| name.contains("history_review"))));
        tx.rollback().await.unwrap();
        result.insert(
            kind,
            json!({"plan":plan,"count_delta":"1","revision_advanced":true,"rolled_back":true}),
        );
    }
    json!({"role":"crm_migrator","kind":"inert native insert probes; all triggers enabled; no migration-source claim","families":result})
}
#[cfg(feature = "perf-harness")]
async fn all_hot_query_plans(pool: &PgPool, f: &Fixture, p: PersonId) -> Value {
    let mut plans = BTreeMap::new();
    let external = [
        ("fub_event_record_imported", "fub_event_record_imported"),
        ("fub_call_record_imported", "fub_call_record_imported"),
        ("fub_text_record_imported", "fub_text_record_imported"),
    ];
    for (kind, table) in NATIVE_TABLES.into_iter().chain(external) {
        for dated in [Dated::Known, Dated::Unknown] {
            if dated == Dated::Unknown && !kind.starts_with("fub_") {
                continue;
            }
            let sql = format!(
                "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
                h::candidate_sql_for_test(kind, dated).unwrap()
            );
            let plan: Value = sqlx::query_scalar(&sql)
                .bind(f.org)
                .bind(p.0)
                .bind(None::<chrono::DateTime<chrono::Utc>>)
                .bind(None::<chrono::DateTime<chrono::Utc>>)
                .bind(None::<i16>)
                .bind(None::<Uuid>)
                .bind(None::<String>)
                .bind(51i64)
                .bind(None::<i64>)
                .fetch_one(pool)
                .await
                .unwrap();
            assert!(plan[0]["Plan"]["Actual Rows"].as_f64().unwrap() <= 51.0);
            plans.insert(
                format!(
                    "{kind}_{}",
                    if dated == Dated::Known {
                        "known"
                    } else {
                        "unknown"
                    }
                ),
                plan,
            );
        }
        let row: Option<(Uuid, Uuid)> = sqlx::query_as(&format!(
            "SELECT id,person_id FROM {table} WHERE organization_id=$1 LIMIT 1"
        ))
        .bind(f.org)
        .fetch_optional(pool)
        .await
        .unwrap();
        let (id, person) = row.unwrap();
        let sql = format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            h::entry_sql_for_test(kind).unwrap()
        );
        let detail: Value = sqlx::query_scalar(&sql)
            .bind(f.org)
            .bind(person)
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(detail[0]["Plan"]["Actual Rows"].as_f64().unwrap() <= 1.0);
        plans.insert(format!("{kind}_detail"), detail);
    }
    for kind in ["contacts", "tags", "custom_fields"] {
        let sql = format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            h::core_collection_sql_for_test(kind).unwrap()
        );
        let plan: Value = sqlx::query_scalar(&sql)
            .bind(f.org)
            .bind(p.0)
            .bind(0i64)
            .fetch_one(pool)
            .await
            .unwrap();
        plans.insert(format!("core_{kind}"), plan);
    }
    let inquiry: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        h::INQUIRIES_SQL
    ))
    .bind(f.org)
    .bind(p.0)
    .bind(None::<chrono::DateTime<chrono::Utc>>)
    .bind(None::<Uuid>)
    .bind(51i64)
    .fetch_one(pool)
    .await
    .unwrap();
    plans.insert("inquiries".into(), inquiry);
    let core: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        h::CORE_PERSON_SQL
    ))
    .bind(f.org)
    .bind(p.0)
    .bind("501")
    .fetch_one(pool)
    .await
    .unwrap();
    plans.insert("core_person".into(), core);
    let binding: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        crm_api::domain::migration::activity_review::REVIEW_BINDING_SQL
    ))
    .bind(f.org)
    .bind(p.0)
    .fetch_one(pool)
    .await
    .unwrap();
    plans.insert("review_binding".into(), binding);
    let activity_revision: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        crm_api::domain::migration::activity_review::REVIEW_REVISION_SQL
    ))
    .bind(f.org)
    .fetch_one(pool)
    .await
    .unwrap();
    plans.insert("activity_revision".into(), activity_revision);
    json!(plans)
}

/// Affected validation after the first plan collector exposed whole-table
/// hashed correction probes. Retains that original evidence; does not repeat
/// unaffected plans or claim that the earlier same-code waves were an old/new
/// comparison. The HTTP component below freezes only the changed guard seam.
#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore]
async fn corrected_contact_plans_and_previous_guard_pair_on_25000_people(pool: PgPool) {
    let (f, parent, capture_id, _) = imports::fixture(&pool).await;
    let run = imports::ready(&f, parent, capture_id).await;
    let (ordinary_org, viewers) = ordinary_today_fixture(&pool).await;
    let p=PersonId::new(sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND family='people' AND source_id='101'").bind(f.org).fetch_one(&pool).await.unwrap());
    review_scale_fixture(&pool, &f, p, 501).await;
    // Analyze after the dense insertion too: the original inquiry plan used
    // stale one-row estimates and sorted all 501 equal-time Inquiry rows.
    for table in [
        "person",
        "inquiry",
        "contact_attempted",
        "migration_history_review_state",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    imports::confirm(&f, run).await;
    imports::drain(&f).await;
    let mut plans = BTreeMap::new();
    for after in [false, true] {
        let body = if after {
            h::candidate_after_sql_for_test("contact_attempted", Dated::Known, "contact_attempted")
                .unwrap()
        } else {
            h::candidate_sql_for_test("contact_attempted", Dated::Known).unwrap()
        };
        // Same-time tie is inside the dense 501-row native set; preserve the
        // real recorded_at value so the continuation exercises a populated seek.
        let at: chrono::DateTime<chrono::Utc> = "2020-01-01T00:00:00Z".parse().unwrap();
        let recorded: chrono::DateTime<chrono::Utc> = "2020-01-02T00:00:00Z".parse().unwrap();
        let plan: Value =
            sqlx::query_scalar(&format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {body}"))
                .bind(f.org)
                .bind(p.0)
                .bind(after.then_some(at))
                .bind(after.then_some(recorded))
                .bind(after.then_some(4i16))
                .bind(after.then_some(Uuid::from_bytes([255; 16])))
                .bind(after.then_some("contact_attempted"))
                .bind(51i64)
                .bind(None::<i64>)
                .fetch_one(&pool)
                .await
                .unwrap();
        plans.insert(
            if after {
                "contact_after"
            } else {
                "contact_first"
            },
            plan,
        );
    }
    let inquiry: Value = sqlx::query_scalar(&format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        h::INQUIRIES_SQL
    ))
    .bind(f.org)
    .bind(p.0)
    .bind(None::<chrono::DateTime<chrono::Utc>>)
    .bind(None::<Uuid>)
    .bind(51i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    plans.insert("inquiries", inquiry);
    let pair = previous_guard_pair(&pool, &f, ordinary_org, &viewers, p).await;
    let writers = incremental_native_writer_plans(&pool, &f, p).await;
    println!(
        "HISTORY_REVIEW_AFFECTED_EVIDENCE {}",
        json!({"plans":plans,"previous_guard_pair":pair,"incremental_native_writers":writers,"fixture":{"review_people":"25000","ordinary_people":"25000","members_per_org":"50","native_contact_rows":"75501","dense_contact_rows":"504","retained_external_facts":"3"},"limitations":"Matched HTTP component, not full production-router/network latency; unchanged Today evaluator plus frozen previous workspace guard on the same upgraded schema. Original unaffected plans remain separate. Trigger controls remove only new history review revision triggers inside rolled-back migrator transactions, never a deployed database."})
    );
    // Fail on examined work, not merely LIMIT output. The failed original
    // hashed subplans each returned 75,501 rows and would fail this assertion.
    for (label, plan) in &plans {
        let relation = if *label == "inquiries" {
            "inquiry"
        } else {
            "contact_attempted"
        };
        assert_bounded_relation_work(plan, relation, 51.0);
    }
    assert_eq!(pair["payloads_equal"], true);
    assert_eq!(
        pair["p95_within_budget"], true,
        "D-050 old/new guard component regression; see retained evidence"
    );
}

#[cfg(feature = "perf-harness")]
fn plan_nodes<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            if fields.contains_key("Node Type") {
                out.push(value);
            }
            for child in fields.values() {
                plan_nodes(child, out);
            }
        }
        Value::Array(values) => {
            for child in values {
                plan_nodes(child, out);
            }
        }
        _ => {}
    }
}
#[cfg(feature = "perf-harness")]
fn assert_bounded_relation_work(plan: &Value, relation: &str, bound: f64) {
    let mut nodes = Vec::new();
    plan_nodes(plan, &mut nodes);
    let scans: Vec<_> = nodes
        .iter()
        .filter(|v| v["Relation Name"] == relation)
        .collect();
    assert!(!scans.is_empty());
    for scan in scans {
        assert!(
            scan["Actual Rows"].as_f64().unwrap() <= bound,
            "unbounded {relation} scan: {scan}"
        );
        assert!(
            scan["Actual Loops"].as_f64().unwrap() <= bound,
            "unbounded {relation} probe loops: {scan}"
        );
        assert!(
            scan["Rows Removed by Filter"].as_f64().unwrap_or(0.0) <= bound,
            "unbounded {relation} filtered work: {scan}"
        );
    }
}

/// The post-fix detail statement is distinct from page SQL, so retain its own
/// single EXPLAIN rather than inferring the detail plan from the page plan.
#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore]
async fn corrected_contact_detail_plan_on_25000_people(pool: PgPool) {
    use sha2::{Digest, Sha256};
    let (f, _, _, book) = imports::fixture(&pool).await;
    let person=PersonId::new(sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND family='people' AND source_id='101'").bind(f.org).fetch_one(&pool).await.unwrap());
    // Keep exactly 75,501 contact facts: 75,000 distributed, 500 dense
    // originals, and one genuine correction appended below. No fact updates.
    review_scale_fixture(&pool, &f, person, 500).await;
    let original:Uuid=sqlx::query_scalar("SELECT id FROM contact_attempted WHERE organization_id=$1 AND person_id=$2 AND occurred_at='2020-01-01Z' ORDER BY id LIMIT 1").bind(f.org).bind(person.0).fetch_one(&pool).await.unwrap();
    let correction:Uuid=sqlx::query_scalar("INSERT INTO contact_attempted(organization_id,person_id,actor_kind,origin,occurred_at,recorded_at,correlation_id,channel,outcome,corrects_id) VALUES($1,$2,'system','migration','2020-01-01Z','2030-01-01Z',gen_random_uuid(),'call','reached',$3) RETURNING id").bind(f.org).bind(person.0).bind(original).fetch_one(&pool).await.unwrap();
    for table in [
        "person",
        "contact_attempted",
        "migration_history_review_state",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap();
    let facts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM contact_attempted WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(people, 25000);
    assert_eq!(facts, 75501);
    let requests = book.count();
    let a = auth(&f);
    let detail = h::entry(&f.pool, &f.key, &a, person, "contact_attempted", original)
        .await
        .unwrap();
    let current = h::entry(&f.pool, &f.key, &a, person, "contact_attempted", correction)
        .await
        .unwrap();
    assert_eq!(detail["metadata"]["superseded"], true);
    assert_eq!(current["metadata"]["superseded"], false);
    assert_eq!(current["metadata"]["corrects_id"], original.to_string());
    assert_eq!(detail["read_revision"], current["read_revision"]);
    assert_eq!(book.count(), requests);
    let sql = h::entry_sql_for_test("contact_attempted").unwrap();
    let plan: Value = sqlx::query_scalar(&format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {sql}"))
        .bind(f.org)
        .bind(person.0)
        .bind(original)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut nodes = Vec::new();
    plan_nodes(&plan, &mut nodes);
    let probes:Vec<_>=nodes.iter().map(|node|json!({"node":node["Node Type"],"relation":node["Relation Name"],"index":node["Index Name"],"rows":node["Actual Rows"],"loops":node["Actual Loops"],"rows_removed_by_filter":node.get("Rows Removed by Filter").unwrap_or(&Value::Null),"shared_hit_blocks":node["Shared Hit Blocks"],"shared_read_blocks":node["Shared Read Blocks"]})).collect();
    let root = &plan[0]["Plan"];
    let blocks =
        root["Shared Hit Blocks"].as_u64().unwrap() + root["Shared Read Blocks"].as_u64().unwrap();
    println!(
        "HISTORY_REVIEW_DETAIL_PLAN_EVIDENCE {}",
        json!({"people":people.to_string(),"native_contact_facts":facts.to_string(),"dense_contact_facts":"504","corrected_actual_detail_sql_sha256":Sha256::digest(sql.as_bytes()).iter().map(|b|format!("{b:02x}")).collect::<String>(),"superseded_positive_and_negative_controls":true,"same_read_revision":true,"no_source_reads":true,"execution_ms":plan[0]["Execution Time"],"root_rows":root["Actual Rows"],"root_loops":root["Actual Loops"],"root_shared_blocks":blocks,"temp_read_blocks":root["Temp Read Blocks"],"temp_written_blocks":root["Temp Written Blocks"],"nodes":probes,"limits":{"relation_rows":1,"relation_loops":1,"root_shared_blocks":128},"scope":"One post-fix contact-detail plan on the synthetic scale fixture; no repeated paired benchmark or production-capacity claim"})
    );
    assert_eq!(root["Actual Rows"], 1.0);
    assert_eq!(root["Actual Loops"], 1);
    assert_bounded_relation_work(&plan, "contact_attempted", 1.0);
    assert!(nodes
        .iter()
        .all(|node| node["Actual Loops"].as_f64().unwrap() <= 1.0));
    assert!(nodes.iter().any(
        |node| node["Index Name"] == "contact_attempted_corrects_once"
            && node["Actual Rows"] == 1.0
            && node["Actual Loops"] == 1
    ));
    assert!(
        blocks <= 128,
        "detail buffer work grew beyond fixed fixture lookup bound"
    );
    assert_eq!(root["Temp Read Blocks"], 0);
    assert_eq!(root["Temp Written Blocks"], 0);
}

#[cfg(feature = "perf-harness")]
#[derive(Clone)]
struct GuardPair {
    state: crm_api::state::AppState,
    previous: bool,
    hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
/// A shared HTTP component wrapper, equivalent in both arms except the guard.
/// The existing AuthContext extractor and existing fixed-clock Today route run
/// unmodified. Today's nested transaction uses workspace::ordinary, unchanged
/// from the baseline; it does not silently invoke current read_check in old arm.
#[cfg(feature = "perf-harness")]
async fn guard_pair_middleware(
    axum::extract::State(pair): axum::extract::State<GuardPair>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::extract::FromRequestParts;
    let (mut parts, body) = request.into_parts();
    let auth = AuthContext::from_request_parts(&mut parts, &pair.state)
        .await
        .unwrap_or_else(|_| panic!("guard component authentication failed"));
    let pool = pair.state.db.as_ref().unwrap();
    let _slot = pair.state.workspace_read_slots.acquire().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    if pair.previous {
        // Frozen previous read_check body: one SELECT, without set_config.
        // Function body is extracted verbatim from the retained old migration;
        // only its name is changed so both implementations coexist in one DB.
        sqlx::query("SELECT crm_history_review_perf_previous_read($1,$2,$3)")
            .bind(auth.active_organization_id.0)
            .bind(auth.actor_user_id.0)
            .bind(true)
            .execute(&mut *tx)
            .await
            .unwrap();
    } else {
        crm_app::auth::workspace::read_check(
            &mut tx,
            auth.active_organization_id,
            auth.actor_user_id,
            true,
        )
        .await
        .unwrap();
    }
    pair.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let response = crm_app::auth::workspace::with_reader(
        &auth,
        next.run(axum::extract::Request::from_parts(parts, body)),
    )
    .await;
    tx.rollback().await.unwrap();
    response
}

#[cfg(feature = "perf-harness")]
async fn previous_guard_pair(
    pool: &PgPool,
    f: &Fixture,
    org: Uuid,
    viewers: &[Uuid],
    person: PersonId,
) -> Value {
    use sha2::{Digest, Sha256};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let old_schema = include_str!("../migrations/20260918000001_fub_people_import.sql");
    let start = old_schema
        .find("CREATE FUNCTION crm_workspace_read(")
        .unwrap();
    let end = start + old_schema[start..].find("END $$;").unwrap() + "END $$;".len();
    let original = &old_schema[start..end];
    let frozen = original.replacen(
        "crm_workspace_read(",
        "crm_history_review_perf_previous_read(",
        1,
    );
    sqlx::raw_sql(&frozen).execute(pool).await.unwrap();
    let config = crate::common::test_config();
    let state = crm_api::state::AppState::for_tests(
        f.pool.clone(),
        &config,
        crm_api::realtime::Publisher::recording(),
    );
    let mut tokens = Vec::new();
    let mut cookies = Vec::new();
    for viewer in viewers.iter().take(5) {
        let (token, _) = crm_api::auth::session::create(
            &f.pool,
            &config.session_secret,
            UserId::new(*viewer),
            Some(OrganizationId::new(org)),
            std::time::Duration::from_secs(600),
        )
        .await
        .unwrap();
        cookies.push(format!("crm_session={token}"));
        tokens.push(token);
    }
    let now: chrono::DateTime<chrono::Utc> = "2026-09-12T12:00:00Z".parse().unwrap();
    let hits = [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))];
    let routers = [true, false].map(|previous| {
        crm_api::routes::today::router_with_test_clock(now)
            .layer(axum::middleware::from_fn_with_state(
                GuardPair {
                    state: state.clone(),
                    previous,
                    hits: hits[usize::from(!previous)].clone(),
                },
                guard_pair_middleware,
            ))
            .with_state(state.clone())
    });
    let old_warm = guard_http_wave(&routers[0], &cookies).await;
    let new_warm = guard_http_wave(&routers[1], &cookies).await;
    let reference: Vec<_> = old_warm.iter().map(|v| v.0.clone()).collect();
    let mut parity = reference == new_warm.iter().map(|v| v.0.clone()).collect::<Vec<_>>();
    let mut times = [Vec::<u128>::new(), Vec::new()];
    // Four paired five-request waves: 20 measured samples per arm, alternated
    // AB/BA. Warmups are retained separately and excluded from the p95.
    for round in 0..4 {
        for arm in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
            let a = auth(f);
            let (wave, review) = tokio::join!(
                guard_http_wave(&routers[arm], &cookies),
                h::core(&f.pool, &a, person)
            );
            review.unwrap();
            parity &= wave.iter().map(|v| v.0.clone()).collect::<Vec<_>>() == reference;
            times[arm].extend(wave.into_iter().map(|v| v.1));
        }
    }
    for token in tokens {
        crm_api::auth::session::revoke_by_token(&f.pool, &config.session_secret, &token)
            .await
            .unwrap();
        assert!(
            crm_api::auth::session::verify(&f.pool, &config.session_secret, &token)
                .await
                .unwrap()
                .is_none()
        );
    }
    let p95 = |samples: &[u128]| {
        let mut v = samples.to_vec();
        v.sort_unstable();
        v[(v.len() * 95).div_ceil(100) - 1]
    };
    let old = p95(&times[0]);
    let new = p95(&times[1]);
    let limit = old + (old / 10).max(25_000);
    assert_eq!(hits[0].load(Ordering::SeqCst), 25);
    assert_eq!(hits[1].load(Ordering::SeqCst), 25);
    json!({"baseline_commit":"27f3fe4654ace5840412e1364ea36974a1266ad8","baseline_scope":"prior read_check SELECT and exact prior crm_workspace_read body; unchanged Today router/evaluator in both arms","original_guard_sql_sha256":Sha256::digest(original.as_bytes()).iter().map(|b|format!("{b:02x}")).collect::<String>(),"current_guard_sql_sha256":Sha256::digest(include_bytes!("../../crm-app/src/auth/workspace.rs")).iter().map(|b|format!("{b:02x}")).collect::<String>(),"component":"actual AuthContext/session verification, matched workspace wrapper, unchanged fixed-clock Today HTTP route; full body drained before timer ends; excludes network and unrelated production middleware","warmup_per_arm":5,"measured_per_arm":20,"concurrent_loads":5,"wave_order":"AB BA AB BA","old_us":times[0].iter().map(ToString::to_string).collect::<Vec<_>>(),"new_us":times[1].iter().map(ToString::to_string).collect::<Vec<_>>(),"old_p95_us":old.to_string(),"new_p95_us":new.to_string(),"new_p95_limit_us":limit.to_string(),"p95_within_budget":new<=limit,"payloads_equal":parity,"rows_per_response":serde_json::from_slice::<Value>(&reference[0]).unwrap()["items"].as_array().unwrap().len(),"old_guard_hits":hits[0].load(Ordering::SeqCst),"new_guard_hits":hits[1].load(Ordering::SeqCst),"all_created_sessions_revoked":true})
}

#[cfg(feature = "perf-harness")]
async fn guard_http_wave(router: &axum::Router, cookies: &[String]) -> Vec<(Vec<u8>, u128)> {
    use http_body_util::BodyExt;
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(6));
    let mut jobs = tokio::task::JoinSet::new();
    for (index, cookie) in cookies.iter().cloned().enumerate() {
        let router = router.clone();
        let barrier = barrier.clone();
        jobs.spawn(async move {
            barrier.wait().await;
            let start = std::time::Instant::now();
            let response = get_with_cookie(&router, "/api/today", &cookie).await;
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let elapsed = start.elapsed().as_micros();
            assert_eq!(status, StatusCode::OK);
            (index, bytes.to_vec(), elapsed)
        });
    }
    barrier.wait().await;
    let mut values = Vec::new();
    while let Some(value) = jobs.join_next().await {
        values.push(value.unwrap());
    }
    values.sort_by_key(|v| v.0);
    values
        .into_iter()
        .map(|(_, body, time)| (body, time))
        .collect()
}

#[cfg(feature = "perf-harness")]
async fn incremental_native_writer_plans(pool: &PgPool, f: &Fixture, p: PersonId) -> Value {
    let target:Uuid=sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1 AND id<>$2 AND first_name LIKE 'Synthetic distribution%' LIMIT 1").bind(f.org).bind(p.0).fetch_one(pool).await.unwrap();
    // Catalog names are still checked against this closed inventory before
    // interpolation. No safety/immutability/old operational trigger is disabled.
    let allowed = [
        "person",
        "person_imported",
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
        "contact_attempted",
        "call_completed",
        "correspondence_captured",
        "inquiry",
        "note",
        "task",
        "contact_method",
        "person_tag",
        "person_custom_field_value",
        "app_user",
        "stage",
        "tag",
        "custom_field",
        "custom_field_option",
        "call",
    ];
    let triggers:Vec<(String,String)>=sqlx::query_as("SELECT c.relname::text,t.tgname::text FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_proc p ON p.oid=t.tgfoid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND NOT t.tgisinternal AND t.tgname LIKE 'history_review_%' AND p.proname LIKE 'crm_history_review_%' ORDER BY c.relname,t.tgname").fetch_all(pool).await.unwrap();
    assert_eq!(triggers.len(), allowed.len());
    for (table, trigger) in &triggers {
        assert!(allowed.contains(&table.as_str()));
        assert!(trigger.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
    }
    let mut results = BTreeMap::new();
    for (kind, table) in NATIVE_TABLES {
        let mut arms = BTreeMap::new();
        for previous in [true, false] {
            let mut tx = pool.begin().await.unwrap();
            let role: String = sqlx::query_scalar("SELECT current_user::text")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            assert_eq!(role, "crm_migrator");
            if previous {
                for (relation, trigger) in &triggers {
                    sqlx::query(&format!("ALTER TABLE {relation} DISABLE TRIGGER {trigger}"))
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
            }
            let before:Value=sqlx::query_scalar("SELECT jsonb_build_object('revision',revision,'count',counts->$3::text) FROM migration_history_review_state WHERE organization_id=$1 AND person_id=$2").bind(f.org).bind(target).bind(kind).fetch_one(&mut *tx).await.unwrap();
            let sql=format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) INSERT INTO {table} SELECT (jsonb_populate_record(NULL::{table},to_jsonb(src)||jsonb_build_object('id',gen_random_uuid(),'person_id',$2::uuid))).* FROM {table} src WHERE organization_id=$1 AND person_id=$3 LIMIT 1");
            let plan: Value = sqlx::query_scalar(&sql)
                .bind(f.org)
                .bind(target)
                .bind(p.0)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            let after:Value=sqlx::query_scalar("SELECT jsonb_build_object('revision',revision,'count',counts->$3::text) FROM migration_history_review_state WHERE organization_id=$1 AND person_id=$2").bind(f.org).bind(target).bind(kind).fetch_one(&mut *tx).await.unwrap();
            if previous {
                assert_eq!(before, after);
            } else {
                assert_eq!(
                    after["count"].as_i64().unwrap(),
                    before["count"].as_i64().unwrap() + 1
                );
                assert!(after["revision"].as_i64().unwrap() > before["revision"].as_i64().unwrap());
            }
            tx.rollback().await.unwrap();
            let disabled:i64=sqlx::query_scalar("SELECT count(*) FROM pg_trigger WHERE tgname LIKE 'history_review_%' AND tgenabled<>'O'").fetch_one(pool).await.unwrap();
            assert_eq!(disabled, 0);
            arms.insert(
                if previous {
                    "previous_revision_behavior"
                } else {
                    "current_revision_behavior"
                },
                json!({"plan":plan,"rolled_back":true,"derived_state_changed":!previous}),
            );
        }
        let old = arms["previous_revision_behavior"]["plan"][0]["Execution Time"]
            .as_f64()
            .unwrap();
        let new = arms["current_revision_behavior"]["plan"][0]["Execution Time"]
            .as_f64()
            .unwrap();
        results.insert(
            kind,
            json!({"arms":arms,"incremental_execution_ms":new-old}),
        );
    }
    json!({"scope":"inert native insert; old/new revision-trigger behavior on same schema/fixture; DDL outside measured EXPLAIN; each transaction rolled back","baseline_disabled_inventory":triggers,"all_revision_triggers_restored":true,"families":results})
}
