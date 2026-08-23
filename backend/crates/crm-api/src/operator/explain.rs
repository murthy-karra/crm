//! The priority explanation builder (docs/specs/SLICE_005.md §2, §3;
//! D-010): pure index arithmetic over `today::query` output. The model
//! turns this into prose and may not reorder or add to it.

use uuid::Uuid;

use crate::domain::person::model::PersonSummary;
use crate::domain::today::{TodayItem, TodayList, TodayPriority, TodayReason};
use crm_operator::{Ahead, NotOnTodayReason, PersonCard, PriorityExplanation, ORDERING_RULE};

/// Where `person_id` sits on the list: the 0-based index and how many
/// items precede it in each tier. `Ahead` counts the Inquiry tiers only;
/// `low` items (D-033) sort under both, so none precede a high/normal
/// item, and the `low` items ahead of a `low` item are not counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub index: usize,
    pub ahead: Ahead,
}

pub fn placement(items: &[TodayItem], person_id: Uuid) -> Option<Placement> {
    let index = items.iter().position(|item| item.person.id == person_id)?;
    let mut ahead = Ahead { high: 0, normal: 0 };
    for item in &items[..index] {
        match item.priority {
            TodayPriority::High => ahead.high += 1,
            TodayPriority::Normal => ahead.normal += 1,
            TodayPriority::Low => {}
        }
    }
    Some(Placement { index, ahead })
}

pub fn priority_str(priority: TodayPriority) -> &'static str {
    match priority {
        TodayPriority::High => "high",
        TodayPriority::Normal => "normal",
        TodayPriority::Low => "low",
    }
}

/// One fixed line per reason code (docs/specs/SLICE_006c.md §5a), built
/// from the coded payload only — never from outside text.
pub fn reason_text(reason: &TodayReason) -> String {
    match reason {
        TodayReason::NewInquiry { source, .. } => {
            format!("a new inquiry from {source} in the last 24 hours")
        }
        TodayReason::NoContactAttempt { since } => {
            format!("no contact attempt since {}", since.to_rfc3339())
        }
        TodayReason::RepeatInquiry { inquiry_count } => {
            format!("{inquiry_count} inquiries in total")
        }
        TodayReason::CallOutcomeNeeded { ended_at, .. } => {
            format!(
                "a call on {} has no outcome yet",
                ended_at.format("%Y-%m-%d")
            )
        }
    }
}

/// The coded reason objects the tools return (`TodayReason`'s serde
/// shape: `code` plus its fields), each with an `explanation` line for the
/// model. The HTTP `TodayItem.reasons` payload is unchanged — this is the
/// Operator's view only.
pub fn reasons_json(item: &TodayItem) -> Vec<serde_json::Value> {
    item.reasons
        .iter()
        .map(|r| {
            let mut value = serde_json::to_value(r).unwrap_or(serde_json::Value::Null);
            if let serde_json::Value::Object(map) = &mut value {
                map.insert(
                    "explanation".to_string(),
                    serde_json::Value::String(reason_text(r)),
                );
            }
            value
        })
        .collect()
}

/// Builds the explanation for a Person already resolved through the
/// caller's Organization scope (`summary`), against the viewer's Today
/// list. A Person not on the list is `NotAssignedToYou` unless they are
/// assigned to the viewer, in which case (including beyond the 200-item
/// cap, §14 item 11) they read as `AlreadyContacted`.
pub fn build_explanation(
    list: &TodayList,
    summary: &PersonSummary,
    viewer: Uuid,
    card: PersonCard,
) -> PriorityExplanation {
    match placement(&list.items, summary.id) {
        Some(Placement { index, ahead }) => {
            let item = &list.items[index];
            PriorityExplanation::OnToday {
                person: card,
                position: index + 1,
                total: list.items.len(),
                priority: priority_str(item.priority).to_string(),
                reasons: reasons_json(item),
                waiting_since: item.waiting_since,
                recommended_action: serde_json::to_value(item.recommended_action)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                ordering_rule: ORDERING_RULE,
                ahead,
            }
        }
        None => {
            let assigned_to_viewer = summary
                .assigned_user
                .as_ref()
                .is_some_and(|u| u.id == viewer);
            let reason = if assigned_to_viewer {
                NotOnTodayReason::AlreadyContacted
            } else {
                NotOnTodayReason::NotAssignedToYou {
                    assigned_user_display_name: summary
                        .assigned_user
                        .as_ref()
                        .map(|u| u.display_name.clone()),
                }
            };
            PriorityExplanation::NotOnToday {
                person: card,
                reason,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::person::model::{StageRef, UserRef};
    use crate::domain::today::{InquiryRef, RecommendedAction, TodayReason};
    use chrono::{DateTime, TimeZone, Utc};
    use crm_operator::UntrustedText;

    fn ts(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 22, hour, 0, 0).unwrap()
    }

    fn summary(id: Uuid, assignee: Option<Uuid>) -> PersonSummary {
        PersonSummary {
            id,
            first_name: Some("Grace".into()),
            last_name: Some("Hopper".into()),
            display_name: "Grace Hopper".into(),
            stage: StageRef {
                id: Uuid::new_v4(),
                name: "Lead".into(),
            },
            assigned_user: assignee.map(|id| UserRef {
                id,
                display_name: "Carol".into(),
            }),
            primary_email: None,
            primary_phone: Some("+15555550100".into()),
            inquiry_count: 1,
            last_inquiry_at: Some(ts(10)),
            created_at: ts(0),
        }
    }

    fn item(id: Uuid, priority: TodayPriority, hour: u32) -> TodayItem {
        TodayItem {
            person: summary(id, None),
            priority,
            recommended_action: RecommendedAction::Call,
            reasons: vec![TodayReason::NoContactAttempt { since: ts(hour) }],
            waiting_since: ts(hour),
            latest_inquiry: InquiryRef {
                id: Uuid::new_v4(),
                source: "zillow".into(),
                received_at: ts(hour),
            },
            last_contact_attempt: None,
        }
    }

    fn card(id: Uuid) -> PersonCard {
        PersonCard {
            id,
            display_name: UntrustedText::new("Grace Hopper"),
            stage_name: "Lead".into(),
            assigned_user_display_name: None,
            primary_email: None,
            primary_phone: None,
            inquiry_count: 1,
            last_inquiry_at: None,
        }
    }

    #[test]
    fn placement_counts_ahead_per_tier_across_the_boundary() {
        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        let items = vec![
            item(ids[0], TodayPriority::High, 8),
            item(ids[1], TodayPriority::High, 9),
            item(ids[2], TodayPriority::Normal, 1),
            item(ids[3], TodayPriority::Normal, 2),
            item(ids[4], TodayPriority::Normal, 3),
        ];
        assert_eq!(
            placement(&items, ids[0]),
            Some(Placement {
                index: 0,
                ahead: Ahead { high: 0, normal: 0 }
            })
        );
        assert_eq!(
            placement(&items, ids[1]),
            Some(Placement {
                index: 1,
                ahead: Ahead { high: 1, normal: 0 }
            })
        );
        // First normal item: both highs ahead, no normals.
        assert_eq!(
            placement(&items, ids[2]),
            Some(Placement {
                index: 2,
                ahead: Ahead { high: 2, normal: 0 }
            })
        );
        assert_eq!(
            placement(&items, ids[4]),
            Some(Placement {
                index: 4,
                ahead: Ahead { high: 2, normal: 2 }
            })
        );
        assert_eq!(placement(&items, Uuid::new_v4()), None);
    }

    #[test]
    fn on_today_explanation_reports_position_and_rule() {
        let viewer = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let list = TodayList {
            generated_at: ts(12),
            items: vec![
                item(a, TodayPriority::High, 8),
                item(b, TodayPriority::Normal, 1),
            ],
            truncated: false,
        };
        let explanation = build_explanation(&list, &summary(b, Some(viewer)), viewer, card(b));
        match explanation {
            PriorityExplanation::OnToday {
                position,
                total,
                priority,
                reasons,
                ordering_rule,
                ahead,
                recommended_action,
                ..
            } => {
                assert_eq!(position, 2);
                assert_eq!(total, 2);
                assert_eq!(priority, "normal");
                assert_eq!(recommended_action, "call");
                assert_eq!(reasons.len(), 1);
                assert_eq!(reasons[0]["code"], "no_contact_attempt");
                assert_eq!(ordering_rule, ORDERING_RULE);
                assert_eq!(ahead, Ahead { high: 1, normal: 0 });
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn placement_ignores_low_items_in_ahead_and_priority_str_is_low() {
        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        let items = vec![
            item(ids[0], TodayPriority::Normal, 1),
            item(ids[1], TodayPriority::Low, 2),
            item(ids[2], TodayPriority::Low, 3),
        ];
        assert_eq!(
            placement(&items, ids[2]),
            Some(Placement {
                index: 2,
                ahead: Ahead { high: 0, normal: 1 }
            })
        );
        assert_eq!(priority_str(TodayPriority::Low), "low");
    }

    #[test]
    fn call_outcome_needed_reason_is_explained_with_the_call_date() {
        let call_id = Uuid::new_v4();
        let reason = TodayReason::CallOutcomeNeeded {
            call_id,
            ended_at: ts(14),
        };
        assert_eq!(
            reason_text(&reason),
            "a call on 2026-08-22 has no outcome yet"
        );

        let mut it = item(call_id, TodayPriority::Low, 14);
        it.recommended_action = RecommendedAction::SetOutcome;
        it.reasons = vec![reason];
        let json = reasons_json(&it);
        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["code"], "call_outcome_needed");
        assert_eq!(json[0]["call_id"], call_id.to_string());
        assert_eq!(json[0]["ended_at"], "2026-08-22T14:00:00Z");
        assert_eq!(
            json[0]["explanation"],
            "a call on 2026-08-22 has no outcome yet"
        );
        assert_eq!(
            serde_json::to_value(it.recommended_action).unwrap(),
            "set_outcome"
        );
    }

    #[test]
    fn not_on_today_variants() {
        let viewer = Uuid::new_v4();
        let other = Uuid::new_v4();
        let p = Uuid::new_v4();
        let list = TodayList {
            generated_at: ts(12),
            items: vec![],
            truncated: false,
        };

        let e = build_explanation(&list, &summary(p, Some(other)), viewer, card(p));
        assert!(matches!(
            e,
            PriorityExplanation::NotOnToday {
                reason: NotOnTodayReason::NotAssignedToYou {
                    assigned_user_display_name: Some(ref n)
                },
                ..
            } if n == "Carol"
        ));

        let e = build_explanation(&list, &summary(p, None), viewer, card(p));
        assert!(matches!(
            e,
            PriorityExplanation::NotOnToday {
                reason: NotOnTodayReason::NotAssignedToYou {
                    assigned_user_display_name: None
                },
                ..
            }
        ));

        let e = build_explanation(&list, &summary(p, Some(viewer)), viewer, card(p));
        assert!(matches!(
            e,
            PriorityExplanation::NotOnToday {
                reason: NotOnTodayReason::AlreadyContacted,
                ..
            }
        ));
    }
}
