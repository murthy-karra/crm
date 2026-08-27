//! Realtime event envelope and channel naming (docs/specs/SLICE_003.md §6;
//! D-023). Events are ids-only invalidation hints — never state, never
//! PII: a client responds by re-fetching authoritative data over the
//! normal authenticated API.

use chrono::{DateTime, Utc};
use serde::Serialize;
#[cfg(test)]
use uuid::Uuid;

use crate::ids::{CallId, CorrelationId, OrganizationId, PersonId, RawPayloadId};

/// One Centrifugo channel per Organization (docs/specs/SLICE_003.md §6):
/// `org:<organization_id>`, lowercase hyphenated UUID.
pub fn channel_for(organization_id: OrganizationId) -> String {
    format!("org:{organization_id}")
}

/// `data.change` on a `person.changed` event (docs/specs/SLICE_003.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonChange {
    InquiryReceived,
    AssignmentChanged,
    StageChanged,
    ContactAttempted,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonChangedData {
    pub person_id: PersonId,
    pub change: PersonChange,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnresolvedChangedData {
    pub raw_payload_id: RawPayloadId,
}

/// `data` on a `call.changed` event (docs/specs/SLICE_006.md §6): ids
/// only, published after every committed call transition.
#[derive(Debug, Clone, Serialize)]
pub struct CallChangedData {
    pub call_id: CallId,
    pub person_id: PersonId,
}

/// The exact §6 event envelope. `v: 1` always this slice; additive fields
/// are allowed under `v: 1`, a breaking change bumps it. Internally
/// tagged on `type` so the wire shape matches §6 exactly:
/// `{"v","type","organization_id","occurred_at","correlation_id","data"}`
/// (JSON object key order is not part of the contract — tests compare
/// parsed `serde_json::Value`s, not raw bytes).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum RealtimeEvent {
    #[serde(rename = "person.changed")]
    PersonChanged {
        v: u8,
        organization_id: OrganizationId,
        occurred_at: DateTime<Utc>,
        correlation_id: CorrelationId,
        data: PersonChangedData,
    },
    #[serde(rename = "intake.unresolved_changed")]
    IntakeUnresolvedChanged {
        v: u8,
        organization_id: OrganizationId,
        occurred_at: DateTime<Utc>,
        correlation_id: CorrelationId,
        data: UnresolvedChangedData,
    },
    #[serde(rename = "call.changed")]
    CallChanged {
        v: u8,
        organization_id: OrganizationId,
        occurred_at: DateTime<Utc>,
        correlation_id: CorrelationId,
        data: CallChangedData,
    },
}

impl RealtimeEvent {
    pub fn person_changed(
        organization_id: OrganizationId,
        occurred_at: DateTime<Utc>,
        correlation_id: CorrelationId,
        person_id: PersonId,
        change: PersonChange,
    ) -> Self {
        RealtimeEvent::PersonChanged {
            v: 1,
            organization_id,
            occurred_at,
            correlation_id,
            data: PersonChangedData { person_id, change },
        }
    }

    pub fn intake_unresolved_changed(
        organization_id: OrganizationId,
        occurred_at: DateTime<Utc>,
        correlation_id: CorrelationId,
        raw_payload_id: RawPayloadId,
    ) -> Self {
        RealtimeEvent::IntakeUnresolvedChanged {
            v: 1,
            organization_id,
            occurred_at,
            correlation_id,
            data: UnresolvedChangedData { raw_payload_id },
        }
    }

    /// `call.changed` (docs/specs/SLICE_006.md §6). As of hardening chunk
    /// N4, every parameter here is a distinct type — `organization_id`,
    /// `correlation_id`, `call_id`, and `person_id` can no longer be
    /// transposed and still compile.
    pub fn call_changed(
        organization_id: OrganizationId,
        occurred_at: DateTime<Utc>,
        correlation_id: CorrelationId,
        call_id: CallId,
        person_id: PersonId,
    ) -> Self {
        RealtimeEvent::CallChanged {
            v: 1,
            organization_id,
            occurred_at,
            correlation_id,
            data: CallChangedData { call_id, person_id },
        }
    }

    pub fn organization_id(&self) -> OrganizationId {
        match self {
            RealtimeEvent::PersonChanged {
                organization_id, ..
            }
            | RealtimeEvent::IntakeUnresolvedChanged {
                organization_id, ..
            }
            | RealtimeEvent::CallChanged {
                organization_id, ..
            } => *organization_id,
        }
    }

    pub fn correlation_id(&self) -> CorrelationId {
        match self {
            RealtimeEvent::PersonChanged { correlation_id, .. }
            | RealtimeEvent::IntakeUnresolvedChanged { correlation_id, .. }
            | RealtimeEvent::CallChanged { correlation_id, .. } => *correlation_id,
        }
    }

    pub fn type_tag(&self) -> &'static str {
        match self {
            RealtimeEvent::PersonChanged { .. } => "person.changed",
            RealtimeEvent::IntakeUnresolvedChanged { .. } => "intake.unresolved_changed",
            RealtimeEvent::CallChanged { .. } => "call.changed",
        }
    }
}

/// A channel plus the event to publish on it — what
/// `Publisher::publish_after_commit`/`publish_now` take
/// (docs/specs/SLICE_003.md §4 module layout).
#[derive(Debug, Clone)]
pub struct Publication {
    pub channel: String,
    pub event: RealtimeEvent,
}

impl Publication {
    /// Convenience: builds the channel from the event's own
    /// `organization_id` (the common case — every publish in this slice
    /// targets the event's own Organization channel).
    pub fn for_event(event: RealtimeEvent) -> Self {
        let channel = channel_for(event.organization_id());
        Publication { channel, event }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 18, 2, 11).unwrap()
    }

    #[test]
    fn channel_for_is_org_prefixed_lowercase_uuid() {
        let org_id =
            OrganizationId::new(Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap());
        assert_eq!(
            channel_for(org_id),
            "org:11111111-2222-3333-4444-555555555555"
        );
    }

    #[test]
    fn person_changed_serializes_to_exact_shape() {
        let org_id = OrganizationId::new(Uuid::new_v4());
        let corr_id = CorrelationId::new(Uuid::new_v4());
        let person_id = PersonId::new(Uuid::new_v4());
        let event = RealtimeEvent::person_changed(
            org_id,
            ts(),
            corr_id,
            person_id,
            PersonChange::InquiryReceived,
        );
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "v": 1,
                "type": "person.changed",
                "organization_id": org_id,
                "occurred_at": "2026-08-21T18:02:11Z",
                "correlation_id": corr_id,
                "data": { "person_id": person_id, "change": "inquiry_received" },
            })
        );
    }

    #[test]
    fn every_person_change_variant_serializes_to_its_snake_case_tag() {
        for (variant, expected) in [
            (PersonChange::InquiryReceived, "inquiry_received"),
            (PersonChange::AssignmentChanged, "assignment_changed"),
            (PersonChange::StageChanged, "stage_changed"),
            (PersonChange::ContactAttempted, "contact_attempted"),
        ] {
            let event = RealtimeEvent::person_changed(
                OrganizationId::new(Uuid::new_v4()),
                ts(),
                CorrelationId::new(Uuid::new_v4()),
                PersonId::new(Uuid::new_v4()),
                variant,
            );
            let value = serde_json::to_value(&event).unwrap();
            assert_eq!(value["data"]["change"], expected);
        }
    }

    #[test]
    fn intake_unresolved_changed_serializes_to_exact_shape() {
        let org_id = OrganizationId::new(Uuid::new_v4());
        let corr_id = CorrelationId::new(Uuid::new_v4());
        let raw_payload_id = RawPayloadId::new(Uuid::new_v4());
        let event = RealtimeEvent::intake_unresolved_changed(org_id, ts(), corr_id, raw_payload_id);
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "v": 1,
                "type": "intake.unresolved_changed",
                "organization_id": org_id,
                "occurred_at": "2026-08-21T18:02:11Z",
                "correlation_id": corr_id,
                "data": { "raw_payload_id": raw_payload_id },
            })
        );
    }

    #[test]
    fn call_changed_serializes_to_exact_shape() {
        let org_id = OrganizationId::new(Uuid::new_v4());
        let corr_id = CorrelationId::new(Uuid::new_v4());
        let call_id = CallId::new(Uuid::new_v4());
        let person_id = PersonId::new(Uuid::new_v4());
        let event = RealtimeEvent::call_changed(org_id, ts(), corr_id, call_id, person_id);
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "v": 1,
                "type": "call.changed",
                "organization_id": org_id,
                "occurred_at": "2026-08-21T18:02:11Z",
                "correlation_id": corr_id,
                "data": { "call_id": call_id, "person_id": person_id },
            })
        );
        assert_eq!(event.type_tag(), "call.changed");
        assert_eq!(event.organization_id(), org_id);
        assert_eq!(event.correlation_id(), corr_id);
    }

    #[test]
    fn accessors_return_the_envelope_fields() {
        let org_id = OrganizationId::new(Uuid::new_v4());
        let corr_id = CorrelationId::new(Uuid::new_v4());
        let event = RealtimeEvent::person_changed(
            org_id,
            ts(),
            corr_id,
            PersonId::new(Uuid::new_v4()),
            PersonChange::StageChanged,
        );
        assert_eq!(event.organization_id(), org_id);
        assert_eq!(event.correlation_id(), corr_id);
        assert_eq!(event.type_tag(), "person.changed");
    }

    #[test]
    fn publication_for_event_derives_channel_from_organization_id() {
        let org_id = OrganizationId::new(Uuid::new_v4());
        let event = RealtimeEvent::intake_unresolved_changed(
            org_id,
            ts(),
            CorrelationId::new(Uuid::new_v4()),
            RawPayloadId::new(Uuid::new_v4()),
        );
        let publication = Publication::for_event(event);
        assert_eq!(publication.channel, channel_for(org_id));
    }
}
