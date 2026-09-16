//! Authenticated prior-correction discovery over real retained source indexes.
//! Correction writes are migrator fixtures, not the unfinished refresh executor.
use crate::{db_family_refresh, db_history_capture_support as capture, import_support::Fixture};
use crm_api::domain::migration::{
    family_refresh::{
        cohort::Claim,
        core_source::Progress,
        evidence::{Purpose, Scope},
        history_baseline::{self, Discovery},
        history_index,
        history_resolution::{self, Resolution},
        history_source::HistoryDisplay,
        model::{Family, Hold, Kind},
    },
    history_capture_source::Stream,
    MigrationError,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn indexed(pool: &PgPool, f: &Fixture, capture: Uuid) -> Claim {
    let claim = db_family_refresh::draft_history_refresh(pool, f, capture).await;
    for i in 0..10 {
        if history_index::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
            == Progress::Finished
        {
            return claim;
        }
        assert!(i < 9);
    }
    unreachable!()
}

async fn construct_corrections(
    pool: &PgPool,
    f: &Fixture,
    claim: &Claim,
    ids: &[u64; 3],
    corrupt: bool,
) {
    let mut units = Vec::new();
    for (kind, source) in [Kind::Event, Kind::Call, Kind::Text].into_iter().zip(ids) {
        let selected =
            match history_resolution::resolve(&f.pool, &f.key, claim, kind, &source.to_string())
                .await
                .unwrap()
            {
                Resolution::Ready(s) => s,
                Resolution::Held(h) => panic!("source {h:?}"),
            };
        let cohort:Uuid = sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id=$2")
            .bind(claim.bundle).bind(&selected.evidence.source_person).fetch_one(pool).await.unwrap();
        let baseline = match history_baseline::discover(
            &f.pool,
            &f.key,
            claim,
            cohort,
            kind,
            &selected.evidence.identity_hmac,
        )
        .await
        .unwrap()
        {
            Discovery::Proven(b) => b,
            Discovery::Held(h) => panic!("baseline {h:?}"),
        };
        assert_ne!(baseline.semantic, selected.evidence.semantic_hmac);
        units.push((
            kind,
            cohort,
            selected,
            baseline,
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ));
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: 1,
    };
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.history_reader','fub-history-timeline-v1',true),set_config('crm.admitted_history_reader','fub-admitted-history-v1',true),set_config('crm.family_refresh_lease',$1,true)")
        .bind(claim.token.to_string()).execute(&mut *tx).await.unwrap();
    let reservation = Uuid::new_v4();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,$5,65536,'unit',2147483648,4294967296)"
    )
    .bind(f.org)
    .bind(claim.bundle)
    .bind(claim.plan)
    .bind(reservation)
    .bind(claim.epoch)
    .fetch_one(&mut *tx)
    .await
    .unwrap());
    for (position, (kind, cohort, source, baseline, manifest, _, _)) in units.iter().enumerate() {
        let (kind, family) = match kind {
            Kind::Event => ("event", "events"),
            Kind::Call => ("call", "calls"),
            Kind::Text => ("text", "text_messages"),
            _ => unreachable!(),
        };
        if baseline.version == 1 {
            sqlx::query("INSERT INTO migration_family_refresh_history_head(organization_id,identity_id,person_id,family,original_fact_id,plan_id,bundle_id,capture_id,semantic_hmac,source_created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
                .bind(f.org).bind(baseline.identity).bind(baseline.person).bind(family).bind(baseline.original_fact).bind(claim.plan).bind(claim.bundle).bind(baseline.capture).bind(&baseline.semantic).bind(baseline.created).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,source_row_id,position,kind,source_key_hmac,person_id,target_id,expected_head_id,disposition,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'correction','{}',decode(repeat('00',24),'hex'),decode(repeat('00',16),'hex'),8192)")
            .bind(manifest).bind(claim.bundle).bind(claim.plan).bind(f.org).bind(cohort).bind(source.row).bind(position as i64+1).bind(kind).bind(source.evidence.identity_hmac.as_slice()).bind(baseline.person).bind(baseline.original_fact).bind(baseline.result).execute(&mut *tx).await.unwrap();
    }
    sqlx::query("INSERT INTO migration_family_refresh_requirement(organization_id,capability,bundle_id) VALUES($1,'fub-family-refresh-v1',$2) ON CONFLICT DO NOTHING").bind(f.org).bind(claim.bundle).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='running',confirmed_at=clock_timestamp(),digest=decode(repeat('00',32),'hex') WHERE id=$1").bind(claim.bundle).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET state='running',phase='apply',confirmed_at=clock_timestamp(),digest=decode(repeat('00',32),'hex') WHERE id=$1").bind(claim.plan).execute(&mut *tx).await.unwrap();
    for (kind, _, source, baseline, manifest, result, version) in &units {
        let stem = match kind {
            Kind::Event => "event",
            Kind::Call => "call",
            Kind::Text => "text",
            _ => unreachable!(),
        };
        let sealed = scope
            .seal(
                &f.key,
                *result,
                Purpose::Result,
                &json!({"version":baseline.version+1,"identity":baseline.identity}),
            )
            .unwrap();
        sqlx::query("INSERT INTO migration_family_refresh_result(id,bundle_id,plan_id,organization_id,manifest_id,disposition,person_id,target_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,'applied',$6,$7,$8,$9)")
            .bind(result).bind(claim.bundle).bind(claim.plan).bind(f.org).bind(manifest).bind(baseline.person).bind(baseline.original_fact).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await.unwrap();
        let wrong: HistoryDisplay =
            serde_json::from_value(json!({"type":"Unrelated metadata"})).unwrap();
        let display = if corrupt && *kind == Kind::Call {
            &wrong
        } else {
            &source.evidence.display
        };
        let display_row = if corrupt && *kind == Kind::Event {
            Uuid::new_v4()
        } else {
            *version
        };
        let sealed = display.seal(scope, &f.key, display_row).unwrap();
        sqlx::query("INSERT INTO migration_family_refresh_history_display(id,organization_id,identity_id,plan_id,bundle_id,result_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(version).bind(f.org).bind(baseline.identity).bind(claim.plan).bind(claim.bundle).bind(result).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await.unwrap();
        let sql=format!("INSERT INTO fub_{stem}_record_corrected(id,organization_id,identity_id,original_fact_id,person_id,actor_kind,origin,occurred_at,correlation_id,version,corrects_id,plan_id,bundle_id,result_id,manifest_id,capture_id,semantic_hmac,source_created_at,source_time_basis) VALUES($1,$2,$3,$4,$5,'system','migration',clock_timestamp(),$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)");
        sqlx::query(&sql)
            .bind(version)
            .bind(f.org)
            .bind(baseline.identity)
            .bind(baseline.original_fact)
            .bind(baseline.person)
            .bind(claim.bundle)
            .bind(baseline.version + 1)
            .bind(baseline.version_id)
            .bind(claim.plan)
            .bind(claim.bundle)
            .bind(result)
            .bind(manifest)
            .bind(source.capture)
            .bind(source.evidence.semantic_hmac.as_slice())
            .bind(source.evidence.created)
            .bind(if source.evidence.created.is_some() {
                "fub_record_created"
            } else {
                "unknown"
            })
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(f.org)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(reservation)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET state='completed',phase='finished',lease_token=NULL,lease_expires_at=NULL WHERE id=$1").bind(claim.plan).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='completed',updated_at=clock_timestamp() WHERE id=$1").bind(claim.bundle).execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT crm_family_refresh_settle(organization_id,bundle_id,plan_id,token,$2,true) FROM migration_family_refresh_reservation WHERE plan_id=$1 AND purpose='control'").bind(claim.plan).bind(claim.epoch).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

async fn exercise(pool: PgPool, admitted: bool, corrupt: bool) {
    let (f, parent, ids, people) = if admitted {
        let (f, admission, capture) = crate::db_admitted_history::fixture(&pool).await;
        let root = crate::db_admitted_history::ready(&f, admission, capture).await;
        crate::db_admitted_history::confirm(&f, root).await;
        crate::db_admitted_history::drain(&f).await;
        let parent = sqlx::query_scalar(
            "SELECT parent_import_id FROM migration_history_capture_run WHERE id=$1",
        )
        .bind(capture)
        .fetch_one(&pool)
        .await
        .unwrap();
        (f, parent, [81, 82, 83], [104, 104, 104])
    } else {
        let (f, parent, capture, _) = crate::db_history_import_support::fixture(&pool).await;
        let root = crate::db_history_import_support::ready(&f, parent, capture).await;
        crate::db_history_import_support::confirm(&f, root).await;
        crate::db_history_import_support::drain(&f).await;
        (f, parent, [1, 2, 3], [101, 101, 102])
    };
    let original:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM migration_history_import_identity i WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    let book = capture::HistoryBook::new();
    let mut captures = Vec::new();
    for round in 1..=3 {
        book.set_records(Stream::Events,vec![json!({"id":ids[0],"personId":people[0],"type":"Inquiry","created":if round==1 {"unknown"}else{"2026-01-04T00:00:00Z"},"description":format!("PRIVATE_BODY_{round}")})]);
        book.set_records(Stream::Calls,vec![json!({"id":ids[1],"personId":people[1],"userId":3,"created":"2026-01-02T00:00:00Z","duration":round,"phone":format!("PRIVATE_PHONE_{round}")})]);
        book.set_records(Stream::TextMessages,vec![json!({"id":ids[2],"personId":people[2],"created":if round==1 {"2026-01-04T00:00:00Z"}else{"unknown"},"sent":"2026-01-03T00:00:00Z","message":format!("PRIVATE_TEXT_{round}")})]);
        let (capture, _) = capture::propose(&f, parent).await;
        capture::confirm(&f, capture).await;
        capture::drain(&f, &book).await;
        captures.push(capture);
    }
    let source_calls = f.reader.calls();
    let rounds = if corrupt { 1 } else { 2 };
    let mut previous_bundle = Uuid::nil();
    for capture in captures.iter().take(rounds) {
        let claim = indexed(&pool, &f, *capture).await;
        construct_corrections(&pool, &f, &claim, &ids, corrupt).await;
        previous_bundle = claim.bundle;
    }
    let current = indexed(&pool, &f, captures[2]).await;
    let before:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(s) ORDER BY person_id) FROM migration_history_review_state s WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    for (kind, source) in [Kind::Event, Kind::Call, Kind::Text].into_iter().zip(ids) {
        let selected =
            match history_resolution::resolve(&f.pool, &f.key, &current, kind, &source.to_string())
                .await
                .unwrap()
            {
                Resolution::Ready(s) => s,
                Resolution::Held(h) => panic!("{h:?}"),
            };
        assert!(!selected
            .evidence
            .display
            .metadata()
            .to_string()
            .contains("PRIVATE_"));
        let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id=$2").bind(current.bundle).bind(&selected.evidence.source_person).fetch_one(&pool).await.unwrap();
        if corrupt && kind != Kind::Text {
            assert!(
                matches!(
                    history_baseline::discover(
                        &f.pool,
                        &f.key,
                        &current,
                        cohort,
                        kind,
                        &selected.evidence.identity_hmac
                    )
                    .await,
                    Err(MigrationError::Crypto)
                ),
                "invalid prior display must not return the older initial fact"
            );
            continue;
        }
        for _ in 0..2 {
            let b = match history_baseline::discover(
                &f.pool,
                &f.key,
                &current,
                cohort,
                kind,
                &selected.evidence.identity_hmac,
            )
            .await
            .unwrap()
            {
                Discovery::Proven(b) => b,
                Discovery::Held(h) => panic!("{h:?}"),
            };
            assert_eq!(b.version, rounds as i64 + 1);
            assert_eq!(b.capture, captures[rounds - 1]);
            let head=sqlx::query("SELECT * FROM migration_family_refresh_history_head WHERE organization_id=$1 AND identity_id=$2").bind(f.org).bind(b.identity).fetch_one(&pool).await.unwrap();
            assert_eq!(b.version_id, head.get("version_id"));
            assert_eq!(b.result, head.get("result_id"));
            assert_eq!(b.semantic, head.get::<Vec<u8>, _>("semantic_hmac"));
            assert_eq!(b.created, head.get("source_created_at"));
        }
        // A typed correction committed outside the frozen predecessor boundary
        // is not a usable baseline, even though its initial fact still verifies.
        let stamp: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT updated_at FROM migration_family_refresh_bundle WHERE id=$1",
        )
        .bind(previous_bundle)
        .fetch_one(&pool)
        .await
        .unwrap();
        for future in [true, false] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query(
                "SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)",
            )
            .execute(&mut *tx)
            .await
            .unwrap();
            sqlx::query("UPDATE migration_family_refresh_bundle SET updated_at=$2 WHERE id=$1")
                .bind(previous_bundle)
                .bind(if future {
                    chrono::Utc::now() + chrono::Duration::minutes(1)
                } else {
                    stamp
                })
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            if future {
                assert!(matches!(
                    history_baseline::discover(
                        &f.pool,
                        &f.key,
                        &current,
                        cohort,
                        kind,
                        &selected.evidence.identity_hmac
                    )
                    .await
                    .unwrap(),
                    Discovery::Held(Hold::BaselineUnproven)
                ));
            }
        }
    }
    let after:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(s) ORDER BY person_id) FROM migration_history_review_state s WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    assert_eq!(
        before, after,
        "discovery cannot alter counts or review revisions"
    );
    let unchanged:Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM migration_history_import_identity i WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    assert_eq!(
        original, unchanged,
        "corrections retain first ownership and original semantic"
    );
    assert_eq!(
        f.reader.calls(),
        source_calls,
        "discovery and corrections use retained evidence only"
    );
    let unsettled:i64=sqlx::query_scalar("SELECT count(*) FROM migration_family_refresh_plan WHERE organization_id=$1 AND measured_bytes<>retained_bytes").bind(f.org).fetch_one(&pool).await.unwrap();
    assert_eq!(unsettled, 0);
    // Erasure prevents fallback to any of the preserved versions.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE organization_id=$1 AND family='events'").bind(f.org).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let identity=sqlx::query("SELECT identity_hmac,person_id FROM migration_history_import_identity WHERE organization_id=$1 AND family='events'").bind(f.org).fetch_one(&pool).await.unwrap();
    let cohort: Uuid = sqlx::query_scalar(
        "SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND person_id=$2",
    )
    .bind(current.bundle)
    .bind(identity.get::<Uuid, _>("person_id"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let hash: [u8; 32] = identity
        .get::<Vec<u8>, _>("identity_hmac")
        .try_into()
        .unwrap();
    assert!(matches!(
        history_baseline::discover(&f.pool, &f.key, &current, cohort, Kind::Event, &hash)
            .await
            .unwrap(),
        Discovery::Held(Hold::TargetErased)
    ));
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn original_correction_chain_authenticates_current_head_and_source(pool: PgPool) {
    exercise(pool, false, false).await;
}
#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn admitted_correction_chain_authenticates_current_head_and_source(pool: PgPool) {
    exercise(pool, true, false).await;
}
#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn correction_display_scope_and_source_mismatch_fail_closed(pool: PgPool) {
    exercise(pool, false, true).await;
}
