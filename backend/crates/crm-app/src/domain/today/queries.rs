//! Today candidates query (docs/specs/SLICE_003.md §3, §4). `assigned_user_id
//! = $viewer` is a Today-ownership rule applied inside
//! `PersonVisibilityScope::Organization`, not a new scope variant
//! (AGENTS.md §4.4) — nothing here adds an `AssignedUser` variant to
//! `visibility.rs`.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::commands::{ContactAttemptRef, ContactChannel, ContactOutcome};
use crate::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crate::domain::person::visibility::PersonVisibilityScope;
use crate::domain::today::model::{InquiryRef, OutcomeNeededCall, TodayCandidate};
use crate::ids::{InquiryId, PersonId, StageId, UserId};

struct TodayCandidateRow {
    id: Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    created_at: DateTime<Utc>,
    stage_id: Uuid,
    stage_name: String,
    assigned_user_id: Option<Uuid>,
    assigned_user_display_name: Option<String>,
    primary_email: Option<String>,
    primary_phone: Option<String>,
    inquiry_count: i64,
    latest_inquiry_id: Option<Uuid>,
    latest_inquiry_source: Option<String>,
    latest_inquiry_received_at: Option<DateTime<Utc>>,
    last_attempt_id: Option<Uuid>,
    last_attempt_channel: Option<String>,
    last_attempt_outcome: Option<String>,
    last_attempt_occurred_at: Option<DateTime<Utc>>,
    waiting_since: Option<DateTime<Utc>>,
    fresh: Option<bool>,
    by_inquiry: Option<bool>,
    outcome_call_id: Option<Uuid>,
    outcome_call_ended_at: Option<DateTime<Utc>>,
    /// Slice 009 (docs/specs/SLICE_009.md §6): the qualifying inbound
    /// correspondence's `occurred_at`, or `NULL` when the Person does not
    /// qualify for the `client_replied` arm.
    client_replied_occurred_at: Option<DateTime<Utc>>,
}

/// A read path fails closed on unexpected data rather than panicking
/// (AGENTS.md convention, e.g. `RoutingStrategy::from_str` ->
/// `CommandError::Corrupt`): these columns are guaranteed non-null by the
/// query's own `WHERE waiting.received_at IS NOT NULL` filter, but sqlx's
/// static nullability inference can't see that through the LATERAL joins,
/// so the row struct types them `Option<T>` and this converts, erroring
/// (never panicking) if the invariant is ever violated.
fn required<T>(value: Option<T>, column: &'static str) -> Result<T, sqlx::Error> {
    value.ok_or_else(|| {
        sqlx::Error::Decode(
            format!("today candidates query: expected {column} to be non-null").into(),
        )
    })
}

impl TryFrom<TodayCandidateRow> for TodayCandidate {
    type Error = sqlx::Error;

    fn try_from(row: TodayCandidateRow) -> Result<Self, sqlx::Error> {
        let display_name = compute_display_name(
            row.first_name.as_deref(),
            row.last_name.as_deref(),
            row.primary_email.as_deref(),
            row.primary_phone.as_deref(),
        );
        let assigned_user = match (row.assigned_user_id, row.assigned_user_display_name) {
            (Some(id), Some(display_name)) => Some(UserRef {
                id: UserId::new(id),
                display_name,
            }),
            _ => None,
        };
        let latest_inquiry_received_at =
            required(row.latest_inquiry_received_at, "latest_inquiry.received_at")?;

        let person = PersonSummary {
            id: PersonId::new(row.id),
            first_name: row.first_name,
            last_name: row.last_name,
            display_name,
            stage: StageRef {
                id: StageId::new(row.stage_id),
                name: row.stage_name,
            },
            assigned_user,
            primary_email: row.primary_email,
            primary_phone: row.primary_phone,
            inquiry_count: row.inquiry_count,
            last_inquiry_at: Some(latest_inquiry_received_at),
            created_at: row.created_at,
        };

        let latest_inquiry = InquiryRef {
            id: InquiryId::new(required(row.latest_inquiry_id, "latest_inquiry.id")?),
            source: required(row.latest_inquiry_source, "latest_inquiry.source")?,
            received_at: latest_inquiry_received_at,
        };

        let last_contact_attempt = match (
            row.last_attempt_id,
            row.last_attempt_channel,
            row.last_attempt_outcome,
            row.last_attempt_occurred_at,
        ) {
            (Some(id), Some(channel), Some(outcome), Some(occurred_at)) => {
                let channel = ContactChannel::decode(&channel).ok_or_else(|| {
                    sqlx::Error::Decode(
                        format!("today candidates query: unexpected contact_attempted.channel {channel:?}")
                            .into(),
                    )
                })?;
                let outcome = ContactOutcome::decode(&outcome).ok_or_else(|| {
                    sqlx::Error::Decode(
                        format!("today candidates query: unexpected contact_attempted.outcome {outcome:?}")
                            .into(),
                    )
                })?;
                Some(ContactAttemptRef {
                    id,
                    channel,
                    outcome,
                    occurred_at,
                })
            }
            _ => None,
        };

        let outcome_needed = match (row.outcome_call_id, row.outcome_call_ended_at) {
            (Some(call_id), Some(ended_at)) => Some(OutcomeNeededCall { call_id, ended_at }),
            (None, None) => None,
            _ => {
                return Err(sqlx::Error::Decode(
                    "today candidates query: outcome call id and ended_at must be set together"
                        .into(),
                ))
            }
        };
        let by_inquiry = required(row.by_inquiry, "by_inquiry")?;
        let client_replied = row.client_replied_occurred_at;
        // Slice 009 (docs/specs/SLICE_009.md §6): a third qualifying arm
        // alongside by_inquiry/outcome_needed — TryFrom's own "a candidate
        // must qualify by SOMETHING the query actually verified" invariant
        // extends to it rather than being superseded by it.
        if !by_inquiry && outcome_needed.is_none() && client_replied.is_none() {
            return Err(sqlx::Error::Decode(
                "today candidates query: a candidate must qualify by inquiry, by call, or by reply"
                    .into(),
            ));
        }

        Ok(TodayCandidate {
            person,
            latest_inquiry,
            last_contact_attempt,
            waiting_since: required(row.waiting_since, "waiting_since")?,
            inquiry_count: row.inquiry_count,
            fresh: required(row.fresh, "fresh")?,
            by_inquiry,
            outcome_needed,
            client_replied,
        })
    }
}

/// Candidates for `viewer`'s Today (docs/specs/SLICE_003.md §3, §4;
/// docs/specs/SLICE_006c.md §5a). Two membership sources, one statement:
///
/// 1. **By Inquiry** (§3): People assigned to `viewer` in `scope`'s
///    Organization whose latest Inquiry has not yet been answered by a
///    contact attempt at or after it. `fresh` is computed once here, in
///    SQL, from `now`, and used directly in the `ORDER BY` (tier before
///    `LIMIT 201`) so a fresh lead can never fall off the cap behind stale
///    ones. Tie-breaks (`received_at DESC, id DESC` for the latest Inquiry;
///    `occurred_at DESC, id DESC` for the last attempt) are contract, not
///    choice (§14a).
/// 2. **By outcome-needed call** (D-033): a `call` of the Organization
///    with `caller_user_id = viewer`, status `ended|failed`, whose
///    effective attempt (`causation_id = call.id`, no corrector) is the
///    automatic root (`corrects_id IS NULL`). One call per Person — the
///    most recent by `ended_at DESC, id DESC`. Such a Person is a `low`
///    item unless it also qualifies by Inquiry, in which case the Inquiry
///    tier wins and `waiting_since` stays the Inquiry's. Assignment is not
///    consulted for this source: the caller owes the outcome.
///
/// The outer `WHERE` keeps an index-able predicate (`p.assigned_user_id
/// = $2 OR p.id IN (outcome_call)`) alongside the membership condition so
/// the planner can narrow `person` before the LATERAL joins run.
///
/// Ordering: Inquiry-based tiers first (`fresh DESC, waiting_since ASC,
/// id`), then `low` by `ended_at ASC, id`. The `LIMIT 201` / `truncated`
/// cap applies to the merged list, so `low` items are the first to fall
/// off — an outcome nag never displaces a lead.
///
/// `last_contact_attempt` is the **effective** attempt (docs/specs/SLICE_006c.md
/// §2, §3): rows that have a corrector (`corrects_id` chain) are excluded
/// before the tie-break — a correction inherits its original's
/// `occurred_at`, so without the filter the `id DESC` tie-break would pick
/// original or correction at random.
pub async fn candidates(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    now: DateTime<Utc>,
) -> Result<(Vec<TodayCandidate>, bool), sqlx::Error> {
    let organization_id = scope.organization_id();

    let mut rows = sqlx::query_as!(
        TodayCandidateRow,
        r#"WITH outcome_call AS (
               SELECT DISTINCT ON (c.person_id) c.person_id, c.id, c.ended_at
               FROM call c
               WHERE c.organization_id = $1
                 AND c.caller_user_id = $2
                 AND c.status IN ('ended', 'failed')
                 AND c.ended_at IS NOT NULL
                 AND EXISTS (
                     SELECT 1 FROM contact_attempted root
                     WHERE root.organization_id = $1
                       AND root.causation_id = c.id
                       AND root.corrects_id IS NULL
                       AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = root.id)
                 )
               ORDER BY c.person_id, c.ended_at DESC, c.id DESC
           )
           SELECT
             p.id, p.first_name, p.last_name, p.created_at,
             s.id as stage_id, s.name as stage_name,
             u.id as "assigned_user_id?", u.display_name as "assigned_user_display_name?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'email'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_email?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'phone'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_phone?",
             (SELECT count(*) FROM inquiry i WHERE i.person_id = p.id) as "inquiry_count!",
             latest.id as "latest_inquiry_id?",
             latest.source as "latest_inquiry_source?",
             latest.received_at as "latest_inquiry_received_at?",
             last_attempt.id as "last_attempt_id?",
             last_attempt.channel as "last_attempt_channel?",
             last_attempt.outcome as "last_attempt_outcome?",
             last_attempt.occurred_at as "last_attempt_occurred_at?",
             CASE WHEN reply_membership.by_reply THEN last_inbound.occurred_at
                  WHEN membership.by_inquiry THEN waiting.received_at
                  ELSE oc.ended_at END
                 as "waiting_since?",
             (CASE WHEN reply_membership.by_reply
                        THEN last_inbound.occurred_at > $3::timestamptz - interval '24 hours'
                   WHEN membership.by_inquiry
                        THEN latest.received_at > $3::timestamptz - interval '24 hours'
                   ELSE false END)
                 as "fresh?",
             membership.by_inquiry as "by_inquiry?",
             oc.id as "outcome_call_id?",
             oc.ended_at as "outcome_call_ended_at?",
             CASE WHEN reply_membership.by_reply THEN last_inbound.occurred_at ELSE NULL END
                 as "client_replied_occurred_at?"
           FROM person p
           JOIN stage s ON s.id = p.stage_id
           LEFT JOIN app_user u ON u.id = p.assigned_user_id
           LEFT JOIN LATERAL (
               SELECT i.id, i.source, i.received_at
               FROM inquiry i
               WHERE i.person_id = p.id
               ORDER BY i.received_at DESC, i.id DESC
               LIMIT 1
           ) latest ON true
           LEFT JOIN LATERAL (
               SELECT ca.id, ca.channel, ca.outcome, ca.occurred_at
               FROM contact_attempted ca
               WHERE ca.person_id = p.id
                 AND ca.organization_id = p.organization_id
                 AND NOT EXISTS (SELECT 1 FROM contact_attempted c WHERE c.corrects_id = ca.id)
               ORDER BY ca.occurred_at DESC, ca.id DESC
               LIMIT 1
           ) last_attempt ON true
           LEFT JOIN LATERAL (
               SELECT i2.received_at
               FROM inquiry i2
               WHERE i2.person_id = p.id
                 AND i2.received_at > COALESCE(last_attempt.occurred_at, '-infinity'::timestamptz)
               ORDER BY i2.received_at ASC
               LIMIT 1
           ) waiting ON true
           LEFT JOIN outcome_call oc ON oc.person_id = p.id
           -- Slice 009 (docs/specs/SLICE_009.md §6): the latest inbound/
           -- outbound correspondence per Person, feeding the client_replied
           -- arm below. No organization_id filter here, matching every
           -- other LATERAL join in this query (`latest`/`last_attempt`/
           -- `waiting`) — person_id is trusted as org-consistent by
           -- construction (the application never writes a fact row whose
           -- person_id and organization_id disagree); the outer `WHERE
           -- p.organization_id = $1` is the actual tenant boundary.
           LEFT JOIN LATERAL (
               SELECT cc.occurred_at
               FROM correspondence_captured cc
               WHERE cc.person_id = p.id
                 AND cc.organization_id = p.organization_id
                 AND cc.direction = 'inbound'
               ORDER BY cc.occurred_at DESC, cc.id DESC
               LIMIT 1
           ) last_inbound ON true
           LEFT JOIN LATERAL (
               SELECT cc.occurred_at
               FROM correspondence_captured cc
               WHERE cc.person_id = p.id
                 AND cc.organization_id = p.organization_id
                 AND cc.direction = 'outbound'
               ORDER BY cc.occurred_at DESC, cc.id DESC
               LIMIT 1
           ) last_outbound ON true
           CROSS JOIN LATERAL (
               SELECT (COALESCE(p.assigned_user_id = $2, false) AND waiting.received_at IS NOT NULL) as by_inquiry
           ) membership
           -- client_replied (spec §6): assigned to the viewer, an inbound
           -- correspondence exists, and it is later than every effective
           -- contact_attempted AND every outbound correspondence — checking
           -- only the LATEST of each is equivalent to "later than every"
           -- (a later attempt/outbound would itself be the latest, so it
           -- alone is sufficient to disqualify).
           CROSS JOIN LATERAL (
               SELECT (
                   COALESCE(p.assigned_user_id = $2, false)
                   AND last_inbound.occurred_at IS NOT NULL
                   AND last_inbound.occurred_at > COALESCE(last_attempt.occurred_at, '-infinity'::timestamptz)
                   AND last_inbound.occurred_at > COALESCE(last_outbound.occurred_at, '-infinity'::timestamptz)
               ) as by_reply
           ) reply_membership
           WHERE p.organization_id = $1
             AND (p.assigned_user_id = $2 OR p.id IN (SELECT person_id FROM outcome_call))
             AND latest.id IS NOT NULL
             AND (membership.by_inquiry OR oc.id IS NOT NULL OR reply_membership.by_reply)
           ORDER BY (membership.by_inquiry OR reply_membership.by_reply) DESC,
                    "fresh?" DESC,
                    CASE WHEN reply_membership.by_reply THEN last_inbound.occurred_at
                         WHEN membership.by_inquiry THEN waiting.received_at
                         ELSE oc.ended_at END ASC,
                    p.id ASC
           LIMIT 201"#,
        organization_id.0,
        viewer.0,
        now,
    )
    .fetch_all(conn)
    .await?;

    let truncated = rows.len() > 200;
    rows.truncate(200);

    let candidates = rows
        .into_iter()
        .map(TodayCandidate::try_from)
        .collect::<Result<Vec<_>, _>>()?;

    Ok((candidates, truncated))
}
