//! Closed update proposals after retained activity conversion and explicit role,
//! kind and timezone mapping. This code neither discovers nor authorizes writes.
use super::model::{compare_native, Baseline, Counts, Current, Hold, Kind, Outcome};
use crate::domain::{
    note::NoteBody,
    task::{TaskKind, TaskTitle},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub enum Content {
    Note {
        body: String,
        author: Option<Uuid>,
    },
    Task {
        title: String,
        kind: TaskKind,
        creator: Option<Uuid>,
        assignee: Option<Uuid>,
        due: Option<DateTime<Utc>>,
        completed: Option<DateTime<Utc>>,
    },
}
#[derive(Serialize, Deserialize)]
pub struct Source {
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub content: Content,
}
pub struct Scope<'a> {
    pub organization: Uuid,
    pub source_external_id: &'a str,
    pub correlation: Uuid,
}
pub struct Members {
    /// Historical authors/creators may be inactive, but must belong to this Org.
    pub all: BTreeSet<Uuid>,
    pub active: BTreeSet<Uuid>,
}
#[derive(Serialize, Deserialize)]
pub struct Proposal {
    pub kind: Kind,
    pub target: Uuid,
    pub person: Uuid,
    pub expected_revision: i64,
    pub expected_head: Option<Uuid>,
    pub after: Value,
    pub counts: Counts,
}
/// Build an initial native row only after source/cohort/coverage discovery and
/// global/native collision checks. Zero means no existing native revision;
/// execution uses an INSERT permit and repeats those checks under its locks.
pub fn propose_insert(
    scope: Scope<'_>,
    person: Uuid,
    target: Uuid,
    source: &Source,
    members: &Members,
    identity_consumed: bool,
    first_coverage: bool,
) -> Result<Proposal, Hold> {
    match compare_native(None, None, &Value::Null, identity_consumed, first_coverage) {
        Outcome::Insert => {}
        Outcome::Held(h) => return Err(h),
        _ => return Err(Hold::BaselineUnproven),
    }
    if source.updated < source.created {
        return Err(Hold::UnsupportedSource);
    }
    let mut after = json!({
        "id":target,"organization_id":scope.organization,"person_id":person,
        "origin":"migration","source":"fub","source_external_id":scope.source_external_id,
        "correlation_id":scope.correlation,"created_at":source.created,"updated_at":source.updated,
        "deleted_at":null,"deleted_by_user_id":null,"revision":1
    });
    let kind = match &source.content {
        Content::Note { body, author } => {
            if !NoteBody::parse(body).is_ok_and(|v| v == *body) {
                return Err(Hold::UnsupportedSource);
            }
            member(*author, &members.all)?;
            after["body"] = json!(body);
            after["author_user_id"] = json!(author);
            Kind::Note
        }
        Content::Task {
            title,
            kind,
            creator,
            assignee,
            due,
            completed,
        } => {
            if !TaskTitle::parse(title).is_ok_and(|v| v == *title) {
                return Err(Hold::UnsupportedSource);
            }
            member(*creator, &members.all)?;
            member(*assignee, &members.active)?;
            after["title"] = json!(title);
            after["kind"] = json!(kind);
            after["created_by_user_id"] = json!(creator);
            after["assignee_user_id"] = json!(assignee);
            after["due_at"] = json!(due);
            after["completed_at"] = json!(completed);
            after["completed_by_user_id"] = Value::Null;
            Kind::Task
        }
    };
    Ok(Proposal {
        kind,
        target,
        person,
        expected_revision: 0,
        expected_head: None,
        after,
        counts: Counts {
            units: 1,
            inserts: 1,
            ..Counts::default()
        },
    })
}

fn time(value: &Value) -> Result<Option<DateTime<Utc>>, Hold> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| Some(d.with_timezone(&Utc)))
        .ok_or(Hold::BaselineUnproven)
}
fn put_time(row: &mut Value, field: &str, desired: Option<DateTime<Utc>>) -> Result<(), Hold> {
    // PostgreSQL and chrono may render the same instant with different offsets.
    // Preserve the actual row representation when the instant did not change.
    if time(&row[field])? != desired {
        row[field] = json!(desired);
    }
    Ok(())
}
fn member(id: Option<Uuid>, members: &BTreeSet<Uuid>) -> Result<(), Hold> {
    if id.is_some_and(|id| !members.contains(&id)) {
        Err(Hold::MappingRequired)
    } else {
        Ok(())
    }
}
/// `organization` and source identity are resolved by the trusted preparation
/// transaction. Every native field is retained in B/C, including local fields
/// this update does not own. Correlation changes only when native data changes.
pub fn propose_update(
    scope: Scope<'_>,
    baseline: Option<&Baseline>,
    current: Option<&Current>,
    source: &Source,
    members: &Members,
    first_coverage: bool,
) -> Result<Proposal, Hold> {
    // Compare against C first: this validates B==C and the separate revision
    // without interpreting source equality as permission to erase local edits.
    let desired = current.map(|c| &c.native).unwrap_or(&Value::Null);
    match compare_native(baseline, current, desired, true, first_coverage) {
        Outcome::Held(reason) => return Err(reason),
        Outcome::AlreadyCurrent => {}
        _ => return Err(Hold::BaselineUnproven),
    }
    let baseline = baseline.ok_or(Hold::BaselineUnproven)?;
    let current = current.ok_or(Hold::TargetErased)?;
    let row = &current.native;
    if !row.is_object()
        || [
            "id",
            "organization_id",
            "person_id",
            "origin",
            "source",
            "source_external_id",
            "created_at",
            "updated_at",
            "deleted_at",
            "deleted_by_user_id",
            "correlation_id",
            "revision",
        ]
        .iter()
        .any(|k| row.get(*k).is_none())
    {
        return Err(Hold::BaselineUnproven);
    }
    if row["id"] != json!(current.target_id)
        || row["person_id"] != json!(current.person_id)
        || row["organization_id"] != json!(scope.organization)
        || row["source"] != "fub"
        || row["source_external_id"] != scope.source_external_id
        || row["origin"] != "migration"
    {
        return Err(Hold::IdentityMismatch);
    }
    if !row["deleted_at"].is_null() || !row["deleted_by_user_id"].is_null() {
        return Err(Hold::TargetErased);
    }
    if current.revision <= 0 || row["revision"].as_i64() != Some(current.revision) {
        return Err(Hold::BaselineUnproven);
    }
    if time(&row["created_at"])? != Some(source.created) || source.updated < source.created {
        return Err(Hold::UnsupportedSource);
    }
    let mut after = row.clone();
    let mut counts = Counts {
        units: 1,
        ..Default::default()
    };
    let kind = match &source.content {
        Content::Note { body, author } => {
            if row.get("body").and_then(Value::as_str).is_none()
                || row.get("author_user_id").is_none()
            {
                return Err(Hold::BaselineUnproven);
            }
            if !NoteBody::parse(body).is_ok_and(|v| v == *body) {
                return Err(Hold::UnsupportedSource);
            }
            member(*author, &members.all)?;
            after["body"] = json!(body);
            after["author_user_id"] = json!(author);
            Kind::Note
        }
        Content::Task {
            title,
            kind,
            creator,
            assignee,
            due,
            completed,
        } => {
            if [
                "title",
                "kind",
                "created_by_user_id",
                "assignee_user_id",
                "due_at",
                "completed_at",
                "completed_by_user_id",
            ]
            .iter()
            .any(|k| row.get(*k).is_none())
            {
                return Err(Hold::BaselineUnproven);
            }
            if !TaskTitle::parse(title).is_ok_and(|v| v == *title) {
                return Err(Hold::UnsupportedSource);
            }
            member(*creator, &members.all)?;
            member(*assignee, &members.all)?;
            member(*assignee, &members.active)?;
            // A native completion actor is never replaced by source inference.
            if !row["completed_by_user_id"].is_null() {
                return Err(Hold::LocalChange);
            }
            let was_complete = time(&row["completed_at"])?.is_some();
            counts.task_completions = u64::from(!was_complete && completed.is_some());
            counts.task_reopens = u64::from(was_complete && completed.is_none());
            after["title"] = json!(title);
            after["kind"] = json!(kind.as_str());
            after["created_by_user_id"] = json!(creator);
            after["assignee_user_id"] = json!(assignee);
            put_time(&mut after, "due_at", *due)?;
            put_time(&mut after, "completed_at", *completed)?;
            after["completed_by_user_id"] = Value::Null;
            Kind::Task
        }
    };
    put_time(&mut after, "updated_at", Some(source.updated))?;
    match compare_native(Some(baseline), Some(current), &after, true, true) {
        Outcome::AlreadyCurrent => counts.already_current = 1,
        Outcome::Update => {
            counts.updates = 1;
            after["correlation_id"] = json!(scope.correlation);
        }
        Outcome::Held(reason) => return Err(reason),
        Outcome::Insert => return Err(Hold::BaselineUnproven),
    }
    Ok(Proposal {
        kind,
        target: current.target_id,
        person: current.person_id,
        expected_revision: current.revision,
        expected_head: current.head_id,
        after,
        counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn date() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn fixture(task: bool) -> (Uuid, Baseline, Current, Source, Members) {
        let org = Uuid::new_v4();
        let target = Uuid::new_v4();
        let person = Uuid::new_v4();
        let mut native = json!({"id":target,"organization_id":org,"person_id":person,"origin":"migration","source":"fub","source_external_id":"v1:17:1","created_at":"2026-09-01T00:00:00+00:00","updated_at":"2026-09-01T00:00:00+00:00","deleted_at":null,"deleted_by_user_id":null,"correlation_id":Uuid::new_v4(),"revision":1,"local_extension":"preserve"});
        let content = if task {
            native.as_object_mut().unwrap().extend(json!({"title":"Follow up","kind":"call","created_by_user_id":null,"assignee_user_id":null,"due_at":null,"completed_at":null,"completed_by_user_id":null}).as_object().unwrap().clone());
            Content::Task {
                title: "Follow up".into(),
                kind: TaskKind::Call,
                creator: None,
                assignee: None,
                due: None,
                completed: None,
            }
        } else {
            native["body"] = json!("Note");
            native["author_user_id"] = Value::Null;
            Content::Note {
                body: "Note".into(),
                author: None,
            }
        };
        (
            org,
            Baseline {
                result_id: Uuid::new_v4(),
                person_id: person,
                target_id: target,
                revision: 1,
                native: native.clone(),
                owned: true,
                head_id: None,
            },
            Current {
                person_id: person,
                target_id: target,
                revision: 1,
                native,
                deleted: false,
                head_id: None,
            },
            Source {
                created: date(),
                updated: date(),
                content,
            },
            Members {
                all: BTreeSet::new(),
                active: BTreeSet::new(),
            },
        )
    }
    fn run(
        org: Uuid,
        b: &Baseline,
        c: &Current,
        s: &Source,
        m: &Members,
    ) -> Result<Proposal, Hold> {
        propose_update(
            Scope {
                organization: org,
                source_external_id: "v1:17:1",
                correlation: Uuid::new_v4(),
            },
            Some(b),
            Some(c),
            s,
            m,
            true,
        )
    }
    #[test]
    fn new_activity_preserves_fixed_ids_source_times_and_completion_attribution() {
        for task in [false, true] {
            let (org, b, _, mut source, members) = fixture(task);
            if let Content::Task { completed, .. } = &mut source.content {
                *completed = Some(date());
            }
            let p = propose_insert(
                Scope {
                    organization: org,
                    source_external_id: "v1:17:1",
                    correlation: b.result_id,
                },
                b.person_id,
                b.target_id,
                &source,
                &members,
                false,
                true,
            )
            .unwrap();
            assert_eq!(p.target, b.target_id);
            assert_eq!(p.after["id"], json!(b.target_id));
            assert_eq!(p.after["created_at"], json!(source.created));
            assert_eq!(p.after["revision"], 1);
            assert_eq!(p.expected_revision, 0);
            assert!(p.expected_head.is_none());
            assert_eq!(p.counts.inserts, 1);
            assert!(p.counts.reconciles());
            if task {
                assert_eq!(p.after["completed_at"], json!(date()));
                assert!(p.after["completed_by_user_id"].is_null());
                assert_eq!(
                    p.counts.task_completions, 0,
                    "initial source state is not a native completion action"
                );
            }
        }
    }
    #[test]
    fn new_activity_needs_unconsumed_identity_and_valid_roles_and_content() {
        let (org, b, _, mut source, members) = fixture(true);
        let run = |source: &Source, consumed, coverage| {
            propose_insert(
                Scope {
                    organization: org,
                    source_external_id: "v1:17:1",
                    correlation: b.result_id,
                },
                b.person_id,
                b.target_id,
                source,
                &members,
                consumed,
                coverage,
            )
        };
        assert!(matches!(
            run(&source, true, true),
            Err(Hold::BaselineUnproven)
        ));
        assert!(matches!(
            run(&source, false, false),
            Err(Hold::FirstCoverageRequired)
        ));
        if let Content::Task { assignee, .. } = &mut source.content {
            *assignee = Some(Uuid::new_v4());
        }
        assert!(matches!(
            run(&source, false, true),
            Err(Hold::MappingRequired)
        ));
        if let Content::Task {
            assignee, title, ..
        } = &mut source.content
        {
            *assignee = None;
            *title = "invalid\nmultiline".into();
        }
        assert!(matches!(
            run(&source, false, true),
            Err(Hold::UnsupportedSource)
        ));
    }
    #[test]
    fn identical_source_does_not_write_merely_for_a_new_correlation_or_timestamp_format() {
        for task in [false, true] {
            let (org, b, c, s, m) = fixture(task);
            let p = run(org, &b, &c, &s, &m).unwrap();
            assert_eq!(p.after, c.native);
            assert_eq!(p.counts.already_current, 1);
            assert!(p.counts.reconciles());
        }
    }
    #[test]
    fn completion_and_reopen_are_exact_and_never_invent_a_completion_actor() {
        let (org, mut b, mut c, mut s, m) = fixture(true);
        if let Content::Task { completed, .. } = &mut s.content {
            *completed = Some(date());
        }
        let completed = run(org, &b, &c, &s, &m).unwrap();
        assert_eq!(completed.counts.task_completions, 1);
        assert_eq!(completed.counts.task_reopens, 0);
        assert!(completed.after["completed_by_user_id"].is_null());
        b.native = completed.after.clone();
        c.native = completed.after;
        if let Content::Task { completed, .. } = &mut s.content {
            *completed = None;
        }
        let reopened = run(org, &b, &c, &s, &m).unwrap();
        assert_eq!(reopened.counts.task_reopens, 1);
        assert_eq!(reopened.counts.task_completions, 0);
        assert!(reopened.after["completed_at"].is_null());
        assert_eq!(reopened.after["local_extension"], "preserve");
    }
    #[test]
    fn local_edits_reverts_and_native_completions_hold_whole_task() {
        let (org, b, mut c, s, m) = fixture(true);
        c.revision += 2;
        assert!(matches!(run(org, &b, &c, &s, &m), Err(Hold::LocalChange)));
        c.revision = b.revision;
        c.native["local_extension"] = json!("changed");
        assert!(matches!(run(org, &b, &c, &s, &m), Err(Hold::LocalChange)));
        c.native = b.native.clone();
        c.native["completed_at"] = json!(date());
        c.native["completed_by_user_id"] = json!(Uuid::new_v4());
        let mut b = b;
        b.native = c.native.clone();
        assert!(matches!(run(org, &b, &c, &s, &m), Err(Hold::LocalChange)));
    }
    #[test]
    fn role_rules_distinguish_historical_creator_from_active_assignee() {
        let (org, b, c, mut s, mut m) = fixture(true);
        let member = Uuid::new_v4();
        m.all.insert(member);
        if let Content::Task { creator, .. } = &mut s.content {
            *creator = Some(member);
        }
        assert!(run(org, &b, &c, &s, &m).is_ok());
        if let Content::Task { assignee, .. } = &mut s.content {
            *assignee = Some(member);
        }
        assert!(matches!(
            run(org, &b, &c, &s, &m),
            Err(Hold::MappingRequired)
        ));
        m.active.insert(member);
        assert!(run(org, &b, &c, &s, &m).is_ok());
    }
    #[test]
    fn note_update_preserves_immutable_fields_and_rejects_unowned_erased_foreign_targets() {
        let (org, mut b, mut c, mut s, m) = fixture(false);
        if let Content::Note { body, .. } = &mut s.content {
            *body = "Updated\nNote".into();
        }
        let p = run(org, &b, &c, &s, &m).unwrap();
        for field in [
            "id",
            "person_id",
            "organization_id",
            "source",
            "source_external_id",
            "created_at",
            "origin",
            "revision",
            "local_extension",
        ] {
            assert_eq!(p.after[field], c.native[field]);
        }
        assert!(matches!(
            run(Uuid::new_v4(), &b, &c, &s, &m),
            Err(Hold::IdentityMismatch)
        ));
        b.owned = false;
        assert!(matches!(
            run(org, &b, &c, &s, &m),
            Err(Hold::BaselineUnproven)
        ));
        b.owned = true;
        c.deleted = true;
        assert!(matches!(run(org, &b, &c, &s, &m), Err(Hold::TargetErased)));
    }
    #[test]
    fn invalid_native_content_or_changed_creation_time_is_held_not_silently_adjusted() {
        let (org, b, c, mut s, m) = fixture(false);
        s.created += chrono::Duration::seconds(1);
        assert!(matches!(
            run(org, &b, &c, &s, &m),
            Err(Hold::UnsupportedSource)
        ));
        s.created = date();
        if let Content::Note { body, .. } = &mut s.content {
            *body = " ".into();
        }
        assert!(matches!(
            run(org, &b, &c, &s, &m),
            Err(Hold::UnsupportedSource)
        ));
    }
}
