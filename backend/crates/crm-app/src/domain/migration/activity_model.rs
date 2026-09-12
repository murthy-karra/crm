//! Closed encrypted activity plan data. No source-bearing value implements Debug.
use super::{
    activity::Choice,
    activity_source::{NativeActivity, Preview, Record},
    MigrationError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Server-derived request progress is encrypted with the exact observation.
/// Flattening keeps existing Record-only retained readers compatible; source
/// properties remain exclusively inside Record.provenance.
#[derive(Serialize, Deserialize)]
pub(crate) struct CapturedRecord {
    #[serde(flatten)]
    pub record: Record,
    pub request_cursor: super::snapshot_source::Cursor,
    pub next_cursor: Option<super::snapshot_source::Cursor>,
    pub capture_accepted: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct FamilyCounts {
    pub planned: i64,
    pub eligible: i64,
    pub applied: i64,
    pub already_present: i64,
    pub held: i64,
    pub pending: i64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Counts {
    pub notes: FamilyCounts,
    pub tasks: FamilyCounts,
    pub held_count: i64,
    pub source_only_count: i64,
    pub invalid_occurrences: i64,
    pub unavailable_bodies: i64,
}
impl Counts {
    pub fn load(v: Value) -> Result<Self, MigrationError> {
        serde_json::from_value(v).map_err(|_| MigrationError::Crypto)
    }
    pub fn family(&mut self, kind: &str) -> &mut FamilyCounts {
        if kind == "note" {
            &mut self.notes
        } else {
            &mut self.tasks
        }
    }
    pub fn planned(&mut self, kind: &str, disposition: &str, source_only: i64) {
        let c = self.family(kind);
        c.planned += 1;
        c.pending += 1;
        match disposition {
            "eligible" => c.eligible += 1,
            "already_present" => c.already_present += 1,
            _ => {
                c.held += 1;
                self.held_count += 1
            }
        };
        self.source_only_count += source_only;
    }
    pub fn settled(&mut self, kind: &str, old: &str, new: &str) {
        let c = self.family(kind);
        c.pending -= 1;
        if old == "eligible" {
            c.eligible -= 1;
        } else if old == "already_present" {
            c.already_present -= 1;
        } else {
            c.held -= 1;
        }
        match new {
            "applied" => c.applied += 1,
            "already_present" => c.already_present += 1,
            _ => c.held += 1,
        };
        if old == "held" {
            self.held_count -= 1;
        }
        if new == "held" {
            self.held_count += 1;
        }
    }
    pub fn eligible(&self) -> i64 {
        self.notes.eligible
            + self.tasks.eligible
            + self.notes.already_present
            + self.tasks.already_present
    }
    pub fn wire(&self) -> Value {
        super::metadata_model::decimal(
            serde_json::to_value(self).expect("closed counters serialize"),
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Member {
    pub id: Uuid,
    pub display_name: String,
    pub status: String,
    pub role: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Destination {
    pub members: Vec<Member>,
    pub source_timezone: Option<String>,
    pub source_engine: String,
    pub html_profile: String,
    pub time_profile: String,
    pub tzdb_version: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Mapping {
    pub source_value: String,
    pub role: String,
    pub choice: Choice,
    pub suggestions: Vec<Uuid>,
    pub suggested_kind: Option<String>,
    pub source: BTreeMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub record: Record,
    pub preview: Preview,
    pub native: Option<NativeActivity>,
    pub native_kind: Option<String>,
    pub reasons: Vec<String>,
    pub source_only_count: i64,
    pub source_observations: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ResultData {
    pub source: BTreeMap<String, String>,
    pub native: Value,
    pub reasons: Vec<String>,
    pub transformations: Vec<String>,
    pub source_only: BTreeMap<String, u64>,
}
pub(crate) fn mark(codes: &mut Vec<String>, code: &str) {
    if !codes.iter().any(|v| v == code) {
        codes.push(code.into())
    }
}
pub(crate) fn preview_wire(data: &Manifest) -> Value {
    let mut native = json!(data.native);
    // The source type is attribution, not the mapped native task kind. Its exact
    // value remains in authenticated retained fields while the page stays bounded.
    if let Some(value) = native
        .get("source_type")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        native["source_type"] = json!(value.chars().take(1024).collect::<String>());
        native["source_type_abbreviated"] = json!(value.chars().count() > 1024);
        native["source_type_full_utf8_bytes"] = json!(value.len().to_string());
    }
    json!({"native":native,"native_kind":data.native_kind,"reasons":data.reasons,"transformations":data.preview.transformations,"source_only":super::metadata_model::decimal(json!(data.preview.source_only)),"source_observations":data.source_observations.to_string()})
}
