//! `rank()`: pure and order-preserving (docs/specs/SLICE_003.md §3, §4,
//! §14a). The SQL `ORDER BY` in `queries::candidates` (on the SQL-computed
//! `fresh` column) is the single ordering authority — this function never
//! re-sorts and never re-evaluates the 24h freshness window; it reads
//! `fresh` directly off each candidate.

use chrono::{DateTime, Utc};

use super::model::{RecommendedAction, TodayCandidate, TodayItem, TodayPriority, TodayReason};

/// `now` is accepted for interface symmetry with `queries::candidates`
/// (and so a future reason needing "now" for explanatory text has
/// somewhere to get it) but is not used to compute freshness — `fresh` is
/// already a fixed fact of each candidate by the time it reaches here.
pub fn rank(candidates: Vec<TodayCandidate>, now: DateTime<Utc>) -> Vec<TodayItem> {
    let _ = now;
    candidates.into_iter().map(rank_one).collect()
}

fn rank_one(candidate: TodayCandidate) -> TodayItem {
    let mut reasons = Vec::new();

    // Fixed order (§3): new_inquiry (if fresh), no_contact_attempt
    // (always), repeat_inquiry (if inquiry_count >= 2).
    if candidate.fresh {
        reasons.push(TodayReason::NewInquiry {
            source: candidate.latest_inquiry.source.clone(),
            received_at: candidate.latest_inquiry.received_at,
        });
    }
    reasons.push(TodayReason::NoContactAttempt {
        since: candidate.waiting_since,
    });
    if candidate.inquiry_count >= 2 {
        reasons.push(TodayReason::RepeatInquiry {
            inquiry_count: candidate.inquiry_count,
        });
    }

    let priority = if candidate.fresh {
        TodayPriority::High
    } else {
        TodayPriority::Normal
    };
    let recommended_action = if candidate.person.primary_phone.is_some() {
        RecommendedAction::Call
    } else {
        RecommendedAction::Email
    };

    TodayItem {
        person: candidate.person,
        priority,
        recommended_action,
        reasons,
        waiting_since: candidate.waiting_since,
        latest_inquiry: candidate.latest_inquiry,
        last_contact_attempt: candidate.last_contact_attempt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::commands::{ContactAttemptRef, ContactChannel, ContactOutcome};
    use crate::domain::person::model::{PersonSummary, StageRef, UserRef};
    use crate::domain::today::model::InquiryRef;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn ts(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, hour, 0, 0).unwrap()
    }

    fn person(phone: Option<&str>) -> PersonSummary {
        PersonSummary {
            id: Uuid::new_v4(),
            first_name: Some("Ada".to_string()),
            last_name: Some("Lovelace".to_string()),
            display_name: "Ada Lovelace".to_string(),
            stage: StageRef {
                id: Uuid::new_v4(),
                name: "Lead".to_string(),
            },
            assigned_user: Some(UserRef {
                id: Uuid::new_v4(),
                display_name: "Alice".to_string(),
            }),
            primary_email: Some("ada@example.com".to_string()),
            primary_phone: phone.map(str::to_string),
            inquiry_count: 1,
            last_inquiry_at: Some(ts(10)),
            created_at: ts(0),
        }
    }

    fn candidate(fresh: bool, inquiry_count: i64, phone: Option<&str>) -> TodayCandidate {
        TodayCandidate {
            person: person(phone),
            latest_inquiry: InquiryRef {
                id: Uuid::new_v4(),
                source: "zillow".to_string(),
                received_at: ts(10),
            },
            last_contact_attempt: None,
            waiting_since: ts(9),
            inquiry_count,
            fresh,
        }
    }

    #[test]
    fn fresh_candidate_gets_new_inquiry_reason_and_high_priority() {
        let items = rank(vec![candidate(true, 1, Some("+15555550100"))], ts(12));
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.priority, TodayPriority::High);
        assert!(matches!(item.reasons[0], TodayReason::NewInquiry { .. }));
    }

    #[test]
    fn stale_candidate_has_no_new_inquiry_reason_and_normal_priority() {
        let items = rank(vec![candidate(false, 1, Some("+15555550100"))], ts(12));
        let item = &items[0];
        assert_eq!(item.priority, TodayPriority::Normal);
        assert!(!item
            .reasons
            .iter()
            .any(|r| matches!(r, TodayReason::NewInquiry { .. })));
    }

    #[test]
    fn no_contact_attempt_reason_is_always_present() {
        for fresh in [true, false] {
            let items = rank(vec![candidate(fresh, 1, Some("+15555550100"))], ts(12));
            assert!(items[0]
                .reasons
                .iter()
                .any(|r| matches!(r, TodayReason::NoContactAttempt { since } if *since == ts(9))));
        }
    }

    #[test]
    fn repeat_inquiry_reason_present_iff_count_at_least_two() {
        let one = rank(vec![candidate(true, 1, None)], ts(12));
        assert!(!one[0]
            .reasons
            .iter()
            .any(|r| matches!(r, TodayReason::RepeatInquiry { .. })));

        let two = rank(vec![candidate(true, 2, None)], ts(12));
        assert!(two[0]
            .reasons
            .iter()
            .any(|r| matches!(r, TodayReason::RepeatInquiry { inquiry_count: 2 })));
    }

    #[test]
    fn reason_order_is_fixed_new_inquiry_then_no_contact_then_repeat() {
        let items = rank(vec![candidate(true, 2, None)], ts(12));
        let codes: Vec<&str> = items[0]
            .reasons
            .iter()
            .map(|r| match r {
                TodayReason::NewInquiry { .. } => "new_inquiry",
                TodayReason::NoContactAttempt { .. } => "no_contact_attempt",
                TodayReason::RepeatInquiry { .. } => "repeat_inquiry",
            })
            .collect();
        assert_eq!(
            codes,
            vec!["new_inquiry", "no_contact_attempt", "repeat_inquiry"]
        );
    }

    #[test]
    fn recommended_action_is_call_when_phone_present_else_email() {
        let with_phone = rank(vec![candidate(true, 1, Some("+15555550100"))], ts(12));
        assert_eq!(with_phone[0].recommended_action, RecommendedAction::Call);

        let without_phone = rank(vec![candidate(true, 1, None)], ts(12));
        assert_eq!(
            without_phone[0].recommended_action,
            RecommendedAction::Email
        );
    }

    #[test]
    fn rank_preserves_input_order_never_resorts() {
        // Intentionally "wrong" order relative to what a re-sort by
        // priority/waiting_since would produce, proving rank() trusts the
        // caller's (SQL) ordering.
        let stale_first = candidate(false, 1, None);
        let fresh_second = candidate(true, 1, None);
        let stale_id = stale_first.person.id;
        let fresh_id = fresh_second.person.id;

        let items = rank(vec![stale_first, fresh_second], ts(12));
        assert_eq!(items[0].person.id, stale_id);
        assert_eq!(items[1].person.id, fresh_id);
    }

    #[test]
    fn rank_carries_through_last_contact_attempt_and_waiting_since() {
        let mut c = candidate(true, 2, None);
        c.last_contact_attempt = Some(ContactAttemptRef {
            id: Uuid::new_v4(),
            channel: ContactChannel::Call,
            outcome: ContactOutcome::NoAnswer,
            occurred_at: ts(8),
        });
        c.waiting_since = ts(11);
        let items = rank(vec![c], ts(12));
        assert_eq!(items[0].waiting_since, ts(11));
        assert!(items[0].last_contact_attempt.is_some());
    }
}
