//! DB-backed tests for Slice 009's live capture pipeline
//! (docs/specs/SLICE_009.md §10): CC/BCC outbound, client reply-all
//! inbound + the `client_replied` Today arm, retroactive forwards
//! (backdating + dedup), forged/deactivated tokens, tenant isolation,
//! tracing-capture, the future-Date clamp, concurrency, the
//! multi-recipient blast-radius bound, and the held-queue flood cap. Run
//! only via ./scripts/check-db.
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::config::{Config, RawPayloadKey};
use crm_api::domain::capture::address::mint_capture_address_if_absent;
use crm_api::domain::raw_payload::crypto;
use crm_api::ids::{CorrespondenceRawId, OrganizationId, UserId};
use crm_api::realtime::Publisher;
use crm_api::state::AppState;

const PW: &str = "pw";
const TEST_INBOUND_EMAIL_SECRET: &str = "test-inbound-email-secret-value-32b";

fn test_config() -> Config {
    Config::from_source(|key| match key {
        "CRM_SESSION_SECRET" => Some("a".repeat(32)),
        "CRM_RAW_PAYLOAD_KEY" => Some(common::TEST_RAW_PAYLOAD_KEY_HEX.to_string()),
        "CENTRIFUGO_HTTP_API_KEY" => Some(common::TEST_CENTRIFUGO_HTTP_API_KEY.to_string()),
        "CENTRIFUGO_TOKEN_HMAC_SECRET" => {
            Some(common::TEST_CENTRIFUGO_TOKEN_HMAC_SECRET.to_string())
        }
        "CRM_INBOUND_EMAIL_SECRET" => Some(TEST_INBOUND_EMAIL_SECRET.to_string()),
        _ => None,
    })
    .unwrap()
}

async fn build_router(migrator_pool: &PgPool, publisher: Publisher) -> Router {
    let app_pool = common::connect_as_app(migrator_pool).await;
    let config = test_config();
    let state = AppState::for_tests(app_pool, &config, publisher);
    crm_api::build_app(state)
}

async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected Publisher::Recording");
    };
    recorded.lock().await.clone()
}

fn capture_recipient(token: &str) -> String {
    format!("save-{token}@leads.elysianfeld.com")
}

/// Mint-if-absent + read back the plaintext token — mirrors what
/// `AcceptInvitation`/`SetMemberStatus` do in production; fixture users
/// created via `common::add_membership` bypass those commands, so tests
/// mint explicitly.
async fn capture_token_for(pool: &PgPool, org_id: Uuid, user_id: Uuid) -> String {
    let mut tx = pool.begin().await.unwrap();
    mint_capture_address_if_absent(&mut tx, OrganizationId::new(org_id), UserId::new(user_id))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    sqlx::query_scalar(
        "SELECT token FROM capture_address WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn post_inbound_email(
    router: &Router,
    recipient: &str,
    raw: &[u8],
) -> axum::response::Response {
    let body = json!({ "recipient": recipient, "raw": STANDARD.encode(raw) }).to_string();
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/inbound/email")
                .header("content-type", "application/json")
                .header(
                    "authorization",
                    format!("Bearer {TEST_INBOUND_EMAIL_SECRET}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn first_stage_id(pool: &PgPool, org_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_person(
    pool: &PgPool,
    org_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_contact_method(
    pool: &PgPool,
    org_id: Uuid,
    person_id: Uuid,
    kind: &str,
    value: &str,
) {
    sqlx::query(
        "INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value)
         VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(kind)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_inquiry(pool: &PgPool, org_id: Uuid, person_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)
         VALUES ($1, $2, $3, 'website', now() - interval '2 days') RETURNING id",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn count(pool: &PgPool, table: &str, org_id: Uuid) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {table} WHERE organization_id = $1"
    ))
    .bind(org_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

use tower::ServiceExt;

/// A ready-to-use org: seeded stages, one active member `alice`, a
/// "client" Person assigned to alice with `email` as her only contact
/// method and one (backdated) Inquiry — every Today-arm test needs the
/// Inquiry to exist regardless of which arm ultimately qualifies (spec §6:
/// "existing latest.id IS NOT NULL constraint kept").
struct Fixture {
    org_id: Uuid,
    alice_id: Uuid,
    alice_email: String,
    person_id: Uuid,
    client_email: String,
    alice_token: String,
}

async fn fixture(pool: &PgPool, org_name: &str, client_email: &str) -> Fixture {
    let slug: String = org_name.to_lowercase().replace(' ', "");
    let alice_email = format!("alice@{slug}.test");
    let (org_id, alice_id) =
        common::create_org_with_stages_and_member(pool, org_name, &alice_email, "Alice", PW).await;
    let stage_id = first_stage_id(pool, org_id).await;
    let person_id = insert_person(pool, org_id, stage_id, Some(alice_id)).await;
    insert_contact_method(pool, org_id, person_id, "email", client_email).await;
    insert_inquiry(pool, org_id, person_id).await;
    let alice_token = capture_token_for(pool, org_id, alice_id).await;
    Fixture {
        org_id,
        alice_id,
        alice_email,
        person_id,
        client_email: client_email.to_string(),
        alice_token,
    }
}

async fn today_reason_codes(router: &Router, cookie: &str, person_id: Uuid) -> Option<Vec<String>> {
    let today =
        common::body_json(common::get_with_cookie(router, "/api/today", cookie).await).await;
    let items = today["items"].as_array().unwrap();
    items
        .iter()
        .find(|i| i["person"]["id"] == person_id.to_string())
        .map(|item| {
            item["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["code"].as_str().unwrap().to_string())
                .collect()
        })
}

async fn person_history(router: &Router, cookie: &str, person_id: Uuid) -> Vec<Value> {
    let detail = common::body_json(
        common::get_with_cookie(router, &format!("/api/people/{person_id}"), cookie).await,
    )
    .await;
    detail["history"].as_array().unwrap().clone()
}

// --- Criterion 1: CC outbound ------------------------------------------

/// Criterion 1 + criterion 11 (history shape): a CC from the agent's own
/// login email, To the client, Cc the capture address -> an outbound
/// `correspondence_captured` row (System actor, on_behalf_of = alice),
/// the D-042.4 auto-`contact_attempted` (causation_id = the fact id),
/// clears alice's Today item, publishes exactly one `person.changed
/// {correspondence_captured}`, and the history detail carries no
/// address/subject/message-id key.
#[sqlx::test]
#[ignore]
async fn cc_from_agent_login_creates_outbound_row_clears_today_and_shape_pins_history(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool, "Acme Realty Cc", "client-cc@example.com").await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let alice_cookie = common::login_cookie(&router, &f.alice_email, PW).await;

    // Before: the Person is on alice's Today (unanswered Inquiry).
    assert!(today_reason_codes(&router, &alice_cookie, f.person_id)
        .await
        .is_some());

    let raw = format!(
        "From: Alice <{}>\r\nTo: Client <{}>\r\nCc: {}\r\nSubject: Re: showing\r\nMessage-ID: <cc-1@acmerealtycc.test>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSee you Thursday.\r\n",
        f.alice_email,
        f.client_email,
        capture_recipient(&f.alice_token),
    );
    let resp =
        post_inbound_email(&router, &capture_recipient(&f.alice_token), raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" })
    );

    let (direction, agent_user_id, on_behalf, actor_kind, via, backdated): (
        String,
        Uuid,
        Option<Uuid>,
        String,
        String,
        bool,
    ) = sqlx::query_as(
        "SELECT direction, agent_user_id, on_behalf_of_user_id, actor_kind, via, backdated
         FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(direction, "outbound");
    assert_eq!(agent_user_id, f.alice_id);
    assert_eq!(on_behalf, Some(f.alice_id));
    assert_eq!(actor_kind, "system");
    assert_eq!(via, "cc");
    assert!(!backdated);

    // The auto-attempt: causation_id = the correspondence fact id.
    let (fact_id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    let (channel, outcome, causation_id): (String, String, Option<Uuid>) = sqlx::query_as(
        "SELECT channel, outcome, causation_id FROM contact_attempted
         WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(channel, "email");
    assert_eq!(outcome, "sent");
    assert_eq!(causation_id, Some(fact_id));

    // Today: cleared.
    assert_eq!(
        today_reason_codes(&router, &alice_cookie, f.person_id).await,
        None
    );

    // Exactly one realtime publish for this person.
    let events: Vec<_> = recorded(&publisher)
        .await
        .into_iter()
        .filter(|(_, v)| v["data"]["person_id"] == f.person_id.to_string())
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1["data"]["change"], "correspondence_captured");

    // Criterion 11: the history shape carries no address/subject/message-id.
    let history = person_history(&router, &alice_cookie, f.person_id).await;
    let corr = history
        .iter()
        .find(|h| h["kind"] == "correspondence")
        .unwrap();
    assert_eq!(corr["detail"]["direction"], "outbound");
    assert_eq!(corr["detail"]["agent"]["id"], f.alice_id.to_string());
    assert_eq!(corr["detail"]["via"], "cc");
    assert_eq!(corr["detail"]["backdated"], false);
    let detail_obj = corr["detail"].as_object().unwrap();
    for forbidden in ["address", "subject", "message_id", "from", "to", "cc"] {
        assert!(!detail_obj.contains_key(forbidden), "leaked {forbidden}");
    }
    let whole = serde_json::to_string(&corr).unwrap();
    assert!(
        !whole.contains("cc-1@acmerealtycc.test"),
        "message-id leaked"
    );
    assert!(!whole.contains(&f.client_email), "address leaked");
    assert!(!whole.contains("showing"), "subject leaked");
}

// --- Criterion 2: client reply-all inbound + client_replied ------------

/// Criterion 2: a client reply-all -> an inbound row + the assignee's
/// Today gains `client_replied {occurred_at}`, in realtime.
#[sqlx::test]
#[ignore]
async fn client_reply_all_creates_inbound_row_and_arms_client_replied(migrator_pool: PgPool) {
    let f = fixture(
        &migrator_pool,
        "Acme Realty Reply",
        "client-reply@example.com",
    )
    .await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let alice_cookie = common::login_cookie(&router, &f.alice_email, PW).await;

    // First, an outbound CC clears the Inquiry-based Today arm so the
    // subsequent inbound reply is unambiguously what re-arms it.
    let cc_raw = format!(
        "From: Alice <{}>\r\nTo: Client <{}>\r\nCc: {}\r\nSubject: Intro\r\nMessage-ID: <cc-intro@acmerealtyreply.test>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHi there.\r\n",
        f.alice_email,
        f.client_email,
        capture_recipient(&f.alice_token),
    );
    let resp = post_inbound_email(
        &router,
        &capture_recipient(&f.alice_token),
        cc_raw.as_bytes(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        today_reason_codes(&router, &alice_cookie, f.person_id).await,
        None
    );

    // The client replies-all: From = client, To = alice, Cc = capture addr.
    let reply_raw = format!(
        "From: Client <{}>\r\nTo: Alice <{}>\r\nCc: {}\r\nSubject: Re: Intro\r\nMessage-ID: <reply-1@example.com>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nThanks, following up.\r\n",
        f.client_email,
        f.alice_email,
        capture_recipient(&f.alice_token),
    );
    let resp = post_inbound_email(
        &router,
        &capture_recipient(&f.alice_token),
        reply_raw.as_bytes(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (direction,): (String,) = sqlx::query_as(
        "SELECT direction FROM correspondence_captured
         WHERE organization_id = $1 AND person_id = $2 AND via = 'cc' ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(direction, "inbound");

    let codes = today_reason_codes(&router, &alice_cookie, f.person_id)
        .await
        .unwrap();
    assert_eq!(codes, vec!["client_replied"]);

    let events: Vec<_> = recorded(&publisher)
        .await
        .into_iter()
        .filter(|(_, v)| {
            v["data"]["person_id"] == f.person_id.to_string()
                && v["data"]["change"] == "correspondence_captured"
        })
        .collect();
    assert_eq!(events.len(), 2, "one for the CC, one for the reply");
}

// --- Criterion 3: retroactive forward, backdating, dedup ----------------

fn forward_raw(
    capture_addr: &str,
    alice_email: &str,
    client_email: &str,
    references: &str,
) -> String {
    format!(
        "From: Alice <{alice_email}>\r\nTo: {capture_addr}\r\nSubject: Fwd: old thread\r\nMessage-ID: <fwd-outer@acmerealtyfwd.test>\r\nReferences: {references}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n---------- Forwarded message ---------\r\nFrom: Client <{client_email}>\r\nDate: Mon, Aug 3, 2026 at 9:00 AM\r\nSubject: old thread\r\n\r\nMissed this one, sorry!\r\n"
    )
}

/// Criterion 3: a retroactive forward of an old (missed) client email
/// lands at the INNER date, `backdated=true`, correct (inbound) direction;
/// re-forwarding the byte-identical raw dedups at the raw layer; a SECOND,
/// byte-different forward whose outer References chain names the SAME
/// original id dedups at the fact layer (References dedup, spec §5) — all
/// three checks live-verify to exactly ONE `correspondence_captured` row.
#[sqlx::test]
#[ignore]
async fn retroactive_forward_backdates_and_dedups_both_layers(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, "Acme Realty Fwd", "client-fwd@example.com").await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let capture_addr = capture_recipient(&f.alice_token);

    let raw = forward_raw(
        &capture_addr,
        &f.alice_email,
        &f.client_email,
        "<original-1@example.com>",
    );
    let resp = post_inbound_email(&router, &capture_addr, raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (direction, via, backdated, occurred_at, message_id): (
        String,
        String,
        bool,
        chrono::DateTime<chrono::Utc>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT direction, via, backdated, occurred_at, message_id
         FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(direction, "inbound", "inner From is the client");
    assert_eq!(via, "forward");
    assert!(backdated);
    assert_eq!(
        occurred_at,
        chrono::DateTime::parse_from_rfc3339("2026-08-03T09:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        "the INNER date position"
    );
    assert_eq!(message_id.as_deref(), Some("original-1@example.com"));

    // Re-forward: byte-identical raw -> raw-layer dedup, still 1 row.
    let resp = post_inbound_email(&router, &capture_addr, raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        count(&migrator_pool, "correspondence_captured", f.org_id).await,
        1
    );
    assert_eq!(
        count(&migrator_pool, "correspondence_raw", f.org_id).await,
        1
    );

    // A DIFFERENT forward (different outer Message-ID/body -> different
    // raw bytes) whose References chain names the SAME original id: the
    // per-person Message-ID UNIQUE dedups it at the fact layer.
    let raw2 = format!(
        "From: Alice <{}>\r\nTo: {capture_addr}\r\nSubject: Fwd: old thread (again)\r\nMessage-ID: <fwd-outer-2@acmerealtyfwd.test>\r\nReferences: <original-1@example.com>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n---------- Forwarded message ---------\r\nFrom: Client <{}>\r\nDate: Mon, Aug 3, 2026 at 9:00 AM\r\nSubject: old thread\r\n\r\nMissed this one, sorry! (identical resend with extra padding text)\r\n",
        f.alice_email, f.client_email,
    );
    assert_ne!(raw2.as_bytes(), raw.as_bytes());
    let resp = post_inbound_email(&router, &capture_addr, raw2.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        count(&migrator_pool, "correspondence_raw", f.org_id).await,
        2,
        "a new raw WAS stored"
    );
    assert_eq!(
        count(&migrator_pool, "correspondence_captured", f.org_id).await,
        1,
        "but the fact-level References dedup absorbed it"
    );
}

// --- Criterion 5: forged/deactivated tokens ------------------------------

/// Criterion 5: a syntactically-valid but never-minted 12-char token ->
/// 200 rejected, nothing stored anywhere.
#[sqlx::test]
#[ignore]
async fn forged_token_is_rejected_and_nothing_stored(migrator_pool: PgPool) {
    let org_id = common::create_org(&migrator_pool, "Acme Realty Forge").await;
    common::seed_stages(&migrator_pool, org_id).await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;

    let resp = post_inbound_email(
        &router,
        "save-zzzzzzzzzzzz@leads.elysianfeld.com",
        b"From: a@b.com\r\nTo: x@y.com\r\n\r\nbody\r\n",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "rejected" })
    );
    assert_eq!(count(&migrator_pool, "correspondence_raw", org_id).await, 0);
}

// --- Tenant isolation ----------------------------------------------------

/// §9: org A's token never matches a Person in org B, even when the SAME
/// email address is a contact method there — the mail lands in org A's
/// held queue (match-never-create, never cross-tenant), and org B is
/// untouched entirely.
#[sqlx::test]
#[ignore]
async fn org_a_token_never_matches_an_org_b_person_with_the_same_email(migrator_pool: PgPool) {
    let shared_email = "shared-across-orgs@example.com";
    let f = fixture(&migrator_pool, "Acme Realty Iso A", shared_email).await;

    // Org B has a DIFFERENT Person with the identical email address.
    let org_b = common::create_org(&migrator_pool, "Acme Realty Iso B").await;
    common::seed_stages(&migrator_pool, org_b).await;
    let stage_b = first_stage_id(&migrator_pool, org_b).await;
    let person_b = insert_person(&migrator_pool, org_b, stage_b, None).await;
    insert_contact_method(&migrator_pool, org_b, person_b, "email", shared_email).await;

    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let capture_addr = capture_recipient(&f.alice_token);
    let raw = format!(
        "From: {shared_email}\r\nTo: {capture_addr}\r\nSubject: hi\r\nContent-Type: text/plain\r\n\r\nbody\r\n"
    );
    let resp = post_inbound_email(&router, &capture_addr, raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Org A: the From matched ITS OWN Person (f.person_id has this email
    // too via the fixture) -> inbound row for f.person_id, never org B.
    assert_eq!(
        count(&migrator_pool, "correspondence_captured", f.org_id).await,
        1
    );
    assert_eq!(
        count(&migrator_pool, "correspondence_captured", org_b).await,
        0
    );
    assert_eq!(count(&migrator_pool, "capture_message", org_b).await, 0);
    assert_eq!(count(&migrator_pool, "correspondence_raw", org_b).await, 0);
}

// --- Tracing capture ------------------------------------------------------

/// §9: the capture pipeline's spans/logs never carry tokens, addresses,
/// subjects, or message-ids — mirrors `db_intake_rotation.rs`'s
/// `rotation_spans_and_logs_carry_no_token_material` pattern. Exercises
/// all three outcomes named by §9's "Tracing-capture tests over
/// matched/unmatched/forward paths": the CC delivery below is the matched
/// path; an unmatched and a forwarded delivery follow it through the SAME
/// installed subscriber (a process can only install one global default).
#[sqlx::test]
#[ignore]
async fn capture_spans_and_logs_carry_no_content_or_tokens(migrator_pool: PgPool) {
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
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

    let f = fixture(
        &migrator_pool,
        "Acme Realty Trace",
        "client-trace@example.com",
    )
    .await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let capture_addr = capture_recipient(&f.alice_token);

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    tracing::subscriber::set_global_default(subscriber)
        .expect("the capture test must be the only one installing a subscriber");

    let raw = format!(
        "From: Alice <{}>\r\nTo: Client <{}>\r\nCc: {capture_addr}\r\nSubject: SECRET-SUBJECT-TEXT\r\nMessage-ID: <trace-secret-id@acmerealtytrace.test>\r\nContent-Type: text/plain\r\n\r\nSECRET-BODY-TEXT\r\n",
        f.alice_email, f.client_email,
    );
    let resp = post_inbound_email(&router, &capture_addr, raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // The matched/CC path is covered above; now the unmatched (held) path
    // and the forward path, through this SAME subscriber, before reading
    // the buffer once at the end.
    let unmatched_counterparty = "unmatched-secret-counterparty@example.com";
    let unmatched_raw = format!(
        "From: {unmatched_counterparty}\r\nTo: {capture_addr}\r\nSubject: UNMATCHED-SECRET-SUBJECT\r\nMessage-ID: <unmatched-secret-id@acmerealtytrace.test>\r\nContent-Type: text/plain\r\n\r\nUNMATCHED-SECRET-BODY\r\n"
    );
    let resp = post_inbound_email(&router, &capture_addr, unmatched_raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let forward_secret_raw = format!(
        "From: Alice <{}>\r\nTo: {capture_addr}\r\nSubject: FORWARD-SECRET-SUBJECT\r\nMessage-ID: <forward-secret-outer@acmerealtytrace.test>\r\nReferences: <forward-secret-original@example.com>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n---------- Forwarded message ---------\r\nFrom: Client <{}>\r\nDate: Mon, Aug 3, 2026 at 9:00 AM\r\nSubject: old thread\r\n\r\nFORWARD-SECRET-BODY\r\n",
        f.alice_email, f.client_email,
    );
    let resp = post_inbound_email(&router, &capture_addr, forward_secret_raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(captured.contains("capture.inbound_email"), "span present");
    assert!(
        captured.contains("captured"),
        "matched-path outcome recorded"
    );
    assert!(
        captured.contains("capture_unmatched"),
        "unmatched-path outcome recorded"
    );
    for secret in [
        f.alice_token.as_str(),
        f.client_email.as_str(),
        f.alice_email.as_str(),
        "SECRET-SUBJECT-TEXT",
        "SECRET-BODY-TEXT",
        "trace-secret-id@acmerealtytrace.test",
        unmatched_counterparty,
        "UNMATCHED-SECRET-SUBJECT",
        "UNMATCHED-SECRET-BODY",
        "unmatched-secret-id@acmerealtytrace.test",
        "FORWARD-SECRET-SUBJECT",
        "FORWARD-SECRET-BODY",
        "forward-secret-outer@acmerealtytrace.test",
        "forward-secret-original@example.com",
    ] {
        assert!(!captured.contains(secret), "leaked {secret:?}");
    }
}

// --- Criterion 9: the future-Date clamp -----------------------------------

/// Criterion 9, CC path: a header Date far in the future lands at receipt
/// time, never suppressing Today for years.
#[sqlx::test]
#[ignore]
async fn future_header_date_is_clamped_on_the_cc_path(migrator_pool: PgPool) {
    let f = fixture(
        &migrator_pool,
        "Acme Realty Clamp Cc",
        "client-clampcc@example.com",
    )
    .await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let capture_addr = capture_recipient(&f.alice_token);
    let before = chrono::Utc::now();

    let raw = format!(
        "From: Alice <{}>\r\nTo: Client <{}>\r\nCc: {capture_addr}\r\nSubject: future\r\nDate: Thu, 27 Aug 2099 12:00:00 +0000\r\nContent-Type: text/plain\r\n\r\nbody\r\n",
        f.alice_email, f.client_email,
    );
    let resp = post_inbound_email(&router, &capture_addr, raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let after = chrono::Utc::now();

    let (occurred_at, backdated): (chrono::DateTime<chrono::Utc>, bool) = sqlx::query_as(
        "SELECT occurred_at, backdated FROM correspondence_captured
         WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert!(
        occurred_at >= before && occurred_at <= after,
        "clamped to receipt time, not 2099: {occurred_at}"
    );
    assert!(!backdated);
}

/// Criterion 9, forward path: a future INNER date also clamps to receipt
/// time — `backdated` stays true regardless (spec §4: "Backdating is
/// unaffected — only future dates clamp"), since a real historical-date
/// CLAIM was still parsed; only its resulting VALUE is bounded.
#[sqlx::test]
#[ignore]
async fn future_inner_date_is_clamped_on_the_forward_path_but_backdated_stays_true(
    migrator_pool: PgPool,
) {
    let f = fixture(
        &migrator_pool,
        "Acme Realty Clamp Fwd",
        "client-clampfwd@example.com",
    )
    .await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let capture_addr = capture_recipient(&f.alice_token);
    let before = chrono::Utc::now();

    let raw = format!(
        "From: Alice <{}>\r\nTo: {capture_addr}\r\nSubject: Fwd: future inner\r\nMessage-ID: <fwd-future@acmerealtyclampfwd.test>\r\nContent-Type: text/plain\r\n\r\n---------- Forwarded message ---------\r\nFrom: Client <{}>\r\nDate: Mon, Aug 3, 2099 at 9:00 AM\r\nSubject: old thread\r\n\r\nbody\r\n",
        f.alice_email, f.client_email,
    );
    let resp = post_inbound_email(&router, &capture_addr, raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let after = chrono::Utc::now();

    let (occurred_at, backdated): (chrono::DateTime<chrono::Utc>, bool) = sqlx::query_as(
        "SELECT occurred_at, backdated FROM correspondence_captured
         WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(f.org_id)
    .bind(f.person_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert!(
        occurred_at >= before && occurred_at <= after,
        "clamped to receipt time, not 2099: {occurred_at}"
    );
    assert!(
        backdated,
        "a real date claim was parsed, even though clamped"
    );
}

// --- Criterion 10: concurrency --------------------------------------------

/// Criterion 10: two simultaneous byte-identical deliveries -> one raw
/// row, one fact row, one realtime publish (the 007b race-test pattern).
#[sqlx::test]
#[ignore]
async fn concurrent_identical_deliveries_produce_exactly_one_of_everything(migrator_pool: PgPool) {
    let f = fixture(
        &migrator_pool,
        "Acme Realty Race",
        "client-race@example.com",
    )
    .await;
    let publisher = Publisher::recording();
    let router = build_router(&migrator_pool, publisher.clone()).await;
    let capture_addr = capture_recipient(&f.alice_token);

    let raw = format!(
        "From: Alice <{}>\r\nTo: Client <{}>\r\nCc: {capture_addr}\r\nSubject: race\r\nMessage-ID: <race-1@acmerealtyrace.test>\r\nContent-Type: text/plain\r\n\r\nbody\r\n",
        f.alice_email, f.client_email,
    );

    let r1 = router.clone();
    let r2 = router.clone();
    let addr1 = capture_addr.clone();
    let addr2 = capture_addr.clone();
    let raw1 = raw.clone();
    let raw2 = raw.clone();
    let (resp1, resp2) = tokio::join!(
        async move { post_inbound_email(&r1, &addr1, raw1.as_bytes()).await },
        async move { post_inbound_email(&r2, &addr2, raw2.as_bytes()).await },
    );
    assert_eq!(resp1.status(), StatusCode::OK);
    assert_eq!(resp2.status(), StatusCode::OK);

    assert_eq!(
        count(&migrator_pool, "correspondence_raw", f.org_id).await,
        1
    );
    assert_eq!(
        count(&migrator_pool, "correspondence_captured", f.org_id).await,
        1
    );
    assert_eq!(
        count(&migrator_pool, "contact_attempted", f.org_id).await,
        1
    );

    let events: Vec<_> = recorded(&publisher)
        .await
        .into_iter()
        .filter(|(_, v)| {
            v["data"]["person_id"] == f.person_id.to_string()
                && v["data"]["change"] == "correspondence_captured"
        })
        .collect();
    assert_eq!(events.len(), 1, "exactly one publish, not two");
}

// --- Multi-recipient bound (spec §5 step 3 / §9 blast-radius) -------------

/// §9's stated outbound-forgery blast radius ("one row + one auto-attempt
/// PER MATCHED RECIPIENT") pinned on the legitimate path: an outbound
/// mail (From = a member login) naming 3 matched recipient Persons on
/// To/Cc creates EXACTLY 3 `correspondence_captured` rows and 3
/// `contact_attempted` rows — one row/attempt per Person, from ONE raw,
/// never a 4th (no phantom row from the capture address itself or a
/// stray partial match).
#[sqlx::test]
#[ignore]
async fn multi_recipient_outbound_creates_one_row_and_attempt_per_matched_person(
    migrator_pool: PgPool,
) {
    let org_name = "Acme Realty Multi";
    let alice_email = "alice@acmerealtymulti.test";
    let (org_id, alice_id) = common::create_org_with_stages_and_member(
        &migrator_pool,
        org_name,
        alice_email,
        "Alice",
        PW,
    )
    .await;
    let stage_id = first_stage_id(&migrator_pool, org_id).await;

    let mut person_ids = Vec::new();
    let mut person_emails = Vec::new();
    for i in 0..3 {
        let person_id = insert_person(&migrator_pool, org_id, stage_id, None).await;
        let email = format!("client-multi-{i}@example.com");
        insert_contact_method(&migrator_pool, org_id, person_id, "email", &email).await;
        person_ids.push(person_id);
        person_emails.push(email);
    }
    let alice_token = capture_token_for(&migrator_pool, org_id, alice_id).await;

    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let capture_addr = capture_recipient(&alice_token);

    let raw = format!(
        "From: Alice <{alice_email}>\r\nTo: {}, {}, {}\r\nCc: {capture_addr}\r\nSubject: group update\r\nMessage-ID: <multi-1@acmerealtymulti.test>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nUpdate for all three.\r\n",
        person_emails[0], person_emails[1], person_emails[2],
    );
    let resp = post_inbound_email(&router, &capture_addr, raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);

    assert_eq!(count(&migrator_pool, "correspondence_raw", org_id).await, 1);
    assert_eq!(
        count(&migrator_pool, "correspondence_captured", org_id).await,
        3,
        "no 4th row"
    );
    assert_eq!(
        count(&migrator_pool, "contact_attempted", org_id).await,
        3,
        "no 4th attempt"
    );

    for person_id in &person_ids {
        let (direction,): (String,) = sqlx::query_as(
            "SELECT direction FROM correspondence_captured WHERE organization_id = $1 AND person_id = $2",
        )
        .bind(org_id)
        .bind(person_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(direction, "outbound");

        let (attempt_count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM contact_attempted WHERE organization_id = $1 AND person_id = $2",
        )
        .bind(org_id)
        .bind(person_id)
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
        assert_eq!(attempt_count, 1, "exactly one attempt for {person_id}");
    }
}

// --- Held-path concurrency (mirrors criterion 10 on the unmatched path) ---

/// The held-queue analogue of
/// `concurrent_identical_deliveries_produce_exactly_one_of_everything`:
/// two simultaneous byte-identical UNMATCHED deliveries still dedup at
/// the raw layer (`(organization_id, content_hmac)` UNIQUE) before either
/// transaction reaches the ladder, so exactly one `correspondence_raw`
/// row and exactly one `capture_message` held row result — never two
/// held rows for the same message.
#[sqlx::test]
#[ignore]
async fn concurrent_identical_unmatched_deliveries_produce_exactly_one_held_row(
    migrator_pool: PgPool,
) {
    let f = fixture(
        &migrator_pool,
        "Acme Realty Race Held",
        "client-raceheld@example.com",
    )
    .await;
    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let capture_addr = capture_recipient(&f.alice_token);

    let raw = format!(
        "From: stranger-race@example.com\r\nTo: {capture_addr}\r\nSubject: unmatched race\r\nMessage-ID: <unmatched-race-1@example.com>\r\nContent-Type: text/plain\r\n\r\nbody\r\n"
    );

    let r1 = router.clone();
    let r2 = router.clone();
    let addr1 = capture_addr.clone();
    let addr2 = capture_addr.clone();
    let raw1 = raw.clone();
    let raw2 = raw.clone();
    let (resp1, resp2) = tokio::join!(
        async move { post_inbound_email(&r1, &addr1, raw1.as_bytes()).await },
        async move { post_inbound_email(&r2, &addr2, raw2.as_bytes()).await },
    );
    assert_eq!(resp1.status(), StatusCode::OK);
    assert_eq!(resp2.status(), StatusCode::OK);

    assert_eq!(
        count(&migrator_pool, "correspondence_raw", f.org_id).await,
        1
    );
    assert_eq!(
        count(&migrator_pool, "capture_message", f.org_id).await,
        1,
        "exactly one held row, not two"
    );
    assert_eq!(
        count(&migrator_pool, "correspondence_captured", f.org_id).await,
        0
    );
}

// --- §9: the held-queue flood cap (500 live held rows per agent) ----------

/// One validly-sealed `correspondence_raw` fixture row, its own id folded
/// into the plaintext for a distinct `content_hmac` per call — used only
/// to satisfy `capture_message.correspondence_raw_id`'s FK for held-row
/// fixtures inserted directly via migrator SQL rather than through the
/// live pipeline. Mirrors `db_capture_unmatched.rs`'s
/// `insert_correspondence_raw_fixture` (not shared across test binaries,
/// so duplicated here); `capture_message.correspondence_raw_id` carries
/// no uniqueness constraint (migrations/20260904000001_correspondence_capture.sql
/// item 4), so many held rows may legally share the ONE raw this returns.
async fn insert_correspondence_raw_fixture(pool: &PgPool, org_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    let plaintext = format!(
        "From: flood-seed@example.com\r\nTo: agent@example.com\r\nSubject: x\r\nMessage-ID: <{id}@example.test>\r\n\r\nbody\r\n"
    );
    let key = RawPayloadKey::new([0x11; 32]);
    let sealed = crypto::seal_correspondence(
        &key,
        OrganizationId::new(org_id),
        CorrespondenceRawId::new(id),
        plaintext.as_bytes(),
    )
    .unwrap();
    let hmac = crypto::content_hmac(&key, plaintext.as_bytes());

    sqlx::query_scalar(
        "INSERT INTO correspondence_raw
            (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed)
         VALUES ($1, $2, now(), $3, $4, $5, $6, true) RETURNING id",
    )
    .bind(id)
    .bind(org_id)
    .bind(sealed.nonce.to_vec())
    .bind(sealed.ciphertext)
    .bind(hmac.to_vec())
    .bind(plaintext.len() as i32)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// §9's held-queue flood cap: at 500 already-live held rows for the
/// agent, one more unmatched delivery creates NO new held row (the count
/// stays exactly 500) — but the raw IS still stored and marked processed
/// (nothing silently lost, just un-queued), and the frozen HTTP envelope
/// is unaffected: still 200 `{"status":"accepted"}` (the finer
/// `capture_held_overflow` outcome lives only in the span — see
/// `receive.rs`'s `process_and_record`, which maps every
/// `PhaseBResult::Processed` to `CaptureEmailOutcome::Captured` regardless
/// of the ladder outcome underneath).
#[sqlx::test]
#[ignore]
async fn held_queue_flood_cap_admits_no_new_row_past_500(migrator_pool: PgPool) {
    let f = fixture(
        &migrator_pool,
        "Acme Realty Flood",
        "client-flood@example.com",
    )
    .await;

    let seed_raw_id = insert_correspondence_raw_fixture(&migrator_pool, f.org_id).await;
    for _ in 0..500 {
        sqlx::query(
            "INSERT INTO capture_message
                (organization_id, agent_user_id, correspondence_raw_id, counterparty_email,
                 direction_hint, captured_at, status)
             VALUES ($1, $2, $3, $4, 'inbound', now(), 'held')",
        )
        .bind(f.org_id)
        .bind(f.alice_id)
        .bind(seed_raw_id)
        .bind(format!("flood-{}@example.com", Uuid::new_v4()))
        .execute(&migrator_pool)
        .await
        .unwrap();
    }
    assert_eq!(
        count(&migrator_pool, "capture_message", f.org_id).await,
        500
    );

    let router = build_router(&migrator_pool, Publisher::recording()).await;
    let capture_addr = capture_recipient(&f.alice_token);
    let raw = format!(
        "From: one-more-stranger@example.com\r\nTo: {capture_addr}\r\nSubject: overflow\r\nMessage-ID: <flood-overflow-1@example.com>\r\nContent-Type: text/plain\r\n\r\nbody\r\n"
    );
    let resp = post_inbound_email(&router, &capture_addr, raw.as_bytes()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(resp).await,
        json!({ "status": "accepted" }),
        "the frozen envelope is unaffected by the overflow"
    );

    assert_eq!(
        count(&migrator_pool, "capture_message", f.org_id).await,
        500,
        "no new held row past the cap"
    );
    assert_eq!(
        count(&migrator_pool, "correspondence_raw", f.org_id).await,
        2,
        "the seed fixture + the new delivery's raw"
    );
    let (processed,): (bool,) = sqlx::query_as(
        "SELECT processed FROM correspondence_raw WHERE organization_id = $1 AND id != $2",
    )
    .bind(f.org_id)
    .bind(seed_raw_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert!(processed, "the overflow raw is still stored and processed");
}
