//! Pure People-filter name resolution (docs/specs/SLICE_013.md §1 rules
//! 1-2, §2; step 2 of docs/tasks/SLICE_013_IMPL.md): turns a
//! model-supplied `PeopleFilterSpec` (crm-operator, names only) into a
//! validated `FilterDefinition` (crm-app, ids) by resolving every name
//! against the caller's own Organization vocabulary, or reports what could
//! not be resolved. No database access — the caller (`operator/backend.rs`)
//! hands in the three org-scoped list reads it already has (the same ones
//! `saved_list::queries::filter_names` uses): stages, active members, tags.
//!
//! `find_by_name` (exact match first, then case-insensitive trimmed,
//! failing closed on a case-insensitive collision) is also used directly by
//! `operator/backend.rs` for `run_saved_list`'s name lookup over
//! `list_saved_lists`' already visibility-filtered rows — the same
//! resolution rule, a different catalog.

use crm_app::domain::admin::queries::MemberView;
use crm_app::domain::admin::MembershipStatus;
use crm_app::domain::person::filter::{
    AgeClause, AgeSpec, AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition,
    SourceClause, StageClause, TagIdsClause,
};
use crm_app::domain::stage::Stage;
use crm_app::domain::tag::TagRow;
use crm_operator::{AgeCondition, PeopleFilterSpec};

/// The result of matching one query string against a name-bearing catalog:
/// zero, exactly one, or (a case-insensitive collision) more than one.
pub enum NameMatch<'a, T> {
    None,
    One(&'a T),
    Many(Vec<&'a T>),
}

/// docs/specs/SLICE_013.md §1 rule 2: exact match first (the model's string
/// as given — already trimmed and control-stripped by the tool parser),
/// then a case-insensitive trimmed match. More than one case-insensitive
/// match is `Many`, not an arbitrary pick — every caller here fails that
/// closed (as "unknown" for stage/tag, as "ambiguous" for an assignee, as a
/// `run_saved_list` clarification for a saved list).
pub fn find_by_name<'a, T>(
    query: &str,
    items: &'a [T],
    name_of: impl Fn(&T) -> &str,
) -> NameMatch<'a, T> {
    // An exact match must still check for a COLLISION among exact matches,
    // not just take the first one: stage/tag names are unique per
    // Organization by construction (a DB constraint, D-051), but saved
    // list names are not — two different visible lists (the whole point
    // of the duplicate-name clarification, D-046) can share the exact
    // same name, and nothing stops two members from sharing an exact
    // display name either.
    let exact: Vec<&T> = items.iter().filter(|item| name_of(item) == query).collect();
    match exact.len() {
        1 => return NameMatch::One(exact[0]),
        n if n > 1 => return NameMatch::Many(exact),
        _ => {}
    }
    let trimmed = query.trim();
    let matches: Vec<&T> = items
        .iter()
        .filter(|item| name_of(item).trim().eq_ignore_ascii_case(trimmed))
        .collect();
    match matches.len() {
        0 => NameMatch::None,
        1 => NameMatch::One(matches[0]),
        _ => NameMatch::Many(matches),
    }
}

/// What could not be resolved, one field per dimension (docs/specs/
/// SLICE_013.md §2's `FilterOutcome::NeedsClarification`, minus
/// `candidate_lists` — `run_saved_list`'s own concern, not
/// `PeopleFilterSpec`'s). Every field is plain `String`/`Vec<String>`:
/// untrusted-text wrapping happens once, in `operator/backend.rs` (the
/// established "one place outside text is wrapped" per that file's own doc
/// comment).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clarification {
    pub unknown_stages: Vec<String>,
    pub unknown_tags: Vec<String>,
    pub unknown_assignees: Vec<String>,
    pub ambiguous_assignees: Vec<String>,
    /// Populated only when `unknown_stages` is non-empty (docs/specs/
    /// SLICE_013.md §2: "populated only for the dimension that failed").
    pub available_stages: Vec<String>,
    /// Populated only when `unknown_tags` is non-empty, same reason.
    pub available_tags: Vec<String>,
    /// Populated only when `unknown_assignees`/`ambiguous_assignees` is
    /// non-empty, same reason. Active members only (§1 rule 2).
    pub members: Vec<String>,
}

impl Clarification {
    fn has_issues(&self) -> bool {
        !self.unknown_stages.is_empty()
            || !self.unknown_tags.is_empty()
            || !self.unknown_assignees.is_empty()
            || !self.ambiguous_assignees.is_empty()
    }
}

pub enum Resolved {
    Definition(FilterDefinition),
    Clarification(Clarification),
}

fn resolve_stages(names: &[String], stages: &[Stage]) -> (Vec<crm_app::ids::StageId>, Vec<String>) {
    let mut ids = Vec::new();
    let mut unknown = Vec::new();
    for name in names {
        match find_by_name(name, stages, |s| s.name.as_str()) {
            // A case-insensitive collision fails closed as unknown (§1
            // rule 2): "impossible today" (stages are seeded case-distinct
            // and no route creates them), but the resolver does not assume
            // that stays true.
            NameMatch::One(stage) => ids.push(stage.id),
            NameMatch::None | NameMatch::Many(_) => unknown.push(name.clone()),
        }
    }
    (ids, unknown)
}

fn resolve_tags(names: &[String], tags: &[TagRow]) -> (Vec<crm_app::ids::TagId>, Vec<String>) {
    let mut ids = Vec::new();
    let mut unknown = Vec::new();
    for name in names {
        match find_by_name(name, tags, |t| t.name.as_str()) {
            // D-051: tag names are unique case-insensitively, so `Many`
            // should not occur; fail closed the same way as stages if it
            // ever does rather than picking arbitrarily.
            NameMatch::One(tag) => ids.push(tag.id),
            NameMatch::None | NameMatch::Many(_) => unknown.push(name.clone()),
        }
    }
    (ids, unknown)
}

/// docs/specs/SLICE_013.md §1 rule 2: `me`/`unassigned` are tokens that win
/// over a member literally so named; active members only; an ambiguous
/// display name is a distinct dimension from unknown.
fn resolve_assignees(
    names: &[String],
    members: &[MemberView],
) -> (Vec<Assignee>, Vec<String>, Vec<String>) {
    let active: Vec<&MemberView> = members
        .iter()
        .filter(|m| m.status == MembershipStatus::Active)
        .collect();
    let mut resolved = Vec::new();
    let mut unknown = Vec::new();
    let mut ambiguous = Vec::new();
    for name in names {
        if name == "me" {
            resolved.push(Assignee::Me);
            continue;
        }
        if name == "unassigned" {
            resolved.push(Assignee::Unassigned);
            continue;
        }
        match find_by_name(name, &active, |m| m.display_name.as_str()) {
            NameMatch::One(member) => resolved.push(Assignee::User(member.user_id)),
            NameMatch::Many(_) => ambiguous.push(name.clone()),
            NameMatch::None => unknown.push(name.clone()),
        }
    }
    (resolved, unknown, ambiguous)
}

fn to_age_spec(cond: AgeCondition) -> AgeSpec {
    match cond {
        AgeCondition::WithinDays(days) => AgeSpec::WithinDays(i64::from(days)),
        AgeCondition::NotWithinDays(days) => AgeSpec::NotWithinDays(i64::from(days)),
        AgeCondition::Never => AgeSpec::Never,
    }
}

/// Resolves every name in `spec` against the caller's own Organization
/// vocabulary (`stages`/`members`/`tags`, already Organization-scoped by
/// the caller) and, if every dimension resolved cleanly, builds a
/// `FilterDefinition` from the public clause structs — never
/// `validate_references` (docs/specs/SLICE_013.md §2: every id here came
/// from the caller's own Organization-scoped list reads in the same
/// request). `sources` and the age/boolean axes need no resolution and are
/// carried through as-is; the adapter's `validate()` call after this
/// catches a malformed `source` value (§1 rule: anything that fails
/// `validate()` is `invalid_arguments`, not a clarification).
pub fn resolve(
    spec: &PeopleFilterSpec,
    stages: &[Stage],
    members: &[MemberView],
    tags: &[TagRow],
) -> Resolved {
    let mut clarification = Clarification::default();

    let (stage_ids, unknown_stages) = resolve_stages(&spec.stage_names, stages);
    if !unknown_stages.is_empty() {
        clarification.unknown_stages = unknown_stages;
        clarification.available_stages = stages.iter().map(|s| s.name.clone()).collect();
    }

    let (assignees, unknown_assignees, ambiguous_assignees) =
        resolve_assignees(&spec.assignees, members);
    if !unknown_assignees.is_empty() || !ambiguous_assignees.is_empty() {
        clarification.unknown_assignees = unknown_assignees;
        clarification.ambiguous_assignees = ambiguous_assignees;
        clarification.members = members
            .iter()
            .filter(|m| m.status == MembershipStatus::Active)
            .map(|m| m.display_name.clone())
            .collect();
    }

    let (tag_ids_any, unknown_tags_any) = resolve_tags(&spec.tag_names_any, tags);
    let (tag_ids_none, unknown_tags_none) = resolve_tags(&spec.tag_names_none, tags);
    if !unknown_tags_any.is_empty() || !unknown_tags_none.is_empty() {
        let mut unknown_tags = unknown_tags_any;
        unknown_tags.extend(unknown_tags_none);
        clarification.unknown_tags = unknown_tags;
        clarification.available_tags = tags.iter().map(|t| t.name.clone()).collect();
    }

    if clarification.has_issues() {
        return Resolved::Clarification(clarification);
    }

    let mut clauses = Vec::new();
    if !stage_ids.is_empty() {
        clauses.push(Clause::Stage(StageClause { stage_ids }));
    }
    if !assignees.is_empty() {
        clauses.push(Clause::AssignedTo(AssignedToClause { assignees }));
    }
    if !spec.sources.is_empty() {
        clauses.push(Clause::Source(SourceClause {
            sources: spec.sources.clone(),
        }));
    }
    if let Some(age) = spec.created {
        clauses.push(Clause::Created(AgeClause {
            age: to_age_spec(age),
        }));
    }
    if let Some(age) = spec.last_inquiry {
        clauses.push(Clause::LastInquiry(AgeClause {
            age: to_age_spec(age),
        }));
    }
    if let Some(age) = spec.last_contact {
        clauses.push(Clause::LastContact(AgeClause {
            age: to_age_spec(age),
        }));
    }
    if let Some(age) = spec.last_inbound {
        clauses.push(Clause::LastInbound(AgeClause {
            age: to_age_spec(age),
        }));
    }
    if let Some(value) = spec.has_replied {
        clauses.push(Clause::HasReplied(BoolClause { value }));
    }
    if let Some(value) = spec.has_phone {
        clauses.push(Clause::HasPhone(BoolClause { value }));
    }
    if let Some(value) = spec.has_email {
        clauses.push(Clause::HasEmail(BoolClause { value }));
    }
    if let Some(value) = spec.awaiting_response {
        clauses.push(Clause::AwaitingResponse(BoolClause { value }));
    }
    if let Some(value) = spec.client_replied_unanswered {
        clauses.push(Clause::ClientRepliedUnanswered(BoolClause { value }));
    }
    if let Some(value) = spec.awaiting_call_outcome {
        clauses.push(Clause::AwaitingCallOutcome(BoolClause { value }));
    }
    if !tag_ids_any.is_empty() {
        clauses.push(Clause::Tags(TagIdsClause {
            tag_ids: tag_ids_any,
        }));
    }
    if !tag_ids_none.is_empty() {
        clauses.push(Clause::NotTags(TagIdsClause {
            tag_ids: tag_ids_none,
        }));
    }

    Resolved::Definition(FilterDefinition {
        version: 1,
        clauses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crm_app::domain::admin::{MembershipStatus, Role};
    use crm_app::ids::{StageId, TagId, UserId};
    use uuid::Uuid;

    fn stage(name: &str) -> Stage {
        Stage {
            id: StageId::new(Uuid::new_v4()),
            name: name.to_string(),
            position: 1,
        }
    }

    fn member(name: &str, status: MembershipStatus) -> MemberView {
        MemberView {
            user_id: UserId::new(Uuid::new_v4()),
            display_name: name.to_string(),
            email: format!("{name}@example.com"),
            role: Role::Member,
            status,
            joined_at: Utc::now(),
            assigned_people_count: 0,
        }
    }

    fn tag(name: &str) -> TagRow {
        TagRow {
            id: TagId::new(Uuid::new_v4()),
            name: name.to_string(),
            person_count: 0,
            created_by_user_id: UserId::new(Uuid::new_v4()),
        }
    }

    fn spec() -> PeopleFilterSpec {
        PeopleFilterSpec {
            has_phone: Some(true),
            limit: 10,
            ..Default::default()
        }
    }

    #[test]
    fn resolves_an_exact_stage_name() {
        let stages = vec![stage("Lead"), stage("Nurture")];
        let s = PeopleFilterSpec {
            stage_names: vec!["Lead".to_string()],
            ..spec()
        };
        match resolve(&s, &stages, &[], &[]) {
            Resolved::Definition(def) => {
                assert_eq!(def.version, 1);
                assert_eq!(def.clauses.len(), 2); // stage + has_phone
                assert!(matches!(def.clauses[0], Clause::Stage(_)));
            }
            Resolved::Clarification(c) => panic!("{c:?}"),
        }
    }

    #[test]
    fn resolves_a_case_insensitive_trimmed_stage_name() {
        let stages = vec![stage("Lead")];
        let s = PeopleFilterSpec {
            stage_names: vec![" lead ".to_string()],
            ..spec()
        };
        assert!(matches!(
            resolve(&s, &stages, &[], &[]),
            Resolved::Definition(_)
        ));
    }

    #[test]
    fn unknown_stage_name_reports_available_stages() {
        let stages = vec![stage("Lead"), stage("Nurture")];
        let s = PeopleFilterSpec {
            stage_names: vec!["Bogus".to_string()],
            ..spec()
        };
        match resolve(&s, &stages, &[], &[]) {
            Resolved::Clarification(c) => {
                assert_eq!(c.unknown_stages, vec!["Bogus".to_string()]);
                assert_eq!(
                    c.available_stages,
                    vec!["Lead".to_string(), "Nurture".to_string()]
                );
                assert!(c.unknown_tags.is_empty());
                assert!(c.unknown_assignees.is_empty());
            }
            Resolved::Definition(_) => panic!("expected a clarification"),
        }
    }

    #[test]
    fn stage_case_insensitive_collision_fails_closed_as_unknown() {
        let stages = vec![stage("Lead"), stage("LEAD")];
        let s = PeopleFilterSpec {
            stage_names: vec!["lead".to_string()],
            ..spec()
        };
        match resolve(&s, &stages, &[], &[]) {
            Resolved::Clarification(c) => assert_eq!(c.unknown_stages, vec!["lead".to_string()]),
            Resolved::Definition(_) => panic!("collision must fail closed"),
        }
    }

    #[test]
    fn me_and_unassigned_are_tokens_that_win_over_a_member_so_named() {
        let members = vec![
            member("me", MembershipStatus::Active),
            member("unassigned", MembershipStatus::Active),
        ];
        let s = PeopleFilterSpec {
            assignees: vec!["me".to_string(), "unassigned".to_string()],
            ..spec()
        };
        match resolve(&s, &[], &members, &[]) {
            Resolved::Definition(def) => {
                let assigned = def
                    .clauses
                    .iter()
                    .find_map(|c| match c {
                        Clause::AssignedTo(a) => Some(a.assignees.clone()),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(assigned, vec![Assignee::Me, Assignee::Unassigned]);
            }
            Resolved::Clarification(c) => panic!("{c:?}"),
        }
    }

    #[test]
    fn inactive_members_are_not_resolved() {
        let members = vec![member("Bob", MembershipStatus::Inactive)];
        let s = PeopleFilterSpec {
            assignees: vec!["Bob".to_string()],
            ..spec()
        };
        match resolve(&s, &[], &members, &[]) {
            Resolved::Clarification(c) => {
                assert_eq!(c.unknown_assignees, vec!["Bob".to_string()]);
                assert!(c.members.is_empty(), "no active members to offer");
            }
            Resolved::Definition(_) => panic!("inactive member must not resolve"),
        }
    }

    #[test]
    fn ambiguous_display_name_is_a_distinct_dimension_from_unknown() {
        let members = vec![
            member("Alex", MembershipStatus::Active),
            member("alex", MembershipStatus::Active),
        ];
        let s = PeopleFilterSpec {
            assignees: vec!["ALEX".to_string()],
            ..spec()
        };
        match resolve(&s, &[], &members, &[]) {
            Resolved::Clarification(c) => {
                assert_eq!(c.ambiguous_assignees, vec!["ALEX".to_string()]);
                assert!(c.unknown_assignees.is_empty());
                assert_eq!(c.members.len(), 2);
            }
            Resolved::Definition(_) => panic!("ambiguous name must not resolve"),
        }
    }

    #[test]
    fn unknown_tags_any_and_none_are_combined() {
        let tags = vec![tag("Investor")];
        let s = PeopleFilterSpec {
            tag_names_any: vec!["Bogus1".to_string()],
            tag_names_none: vec!["Bogus2".to_string()],
            ..spec()
        };
        match resolve(&s, &[], &[], &tags) {
            Resolved::Clarification(c) => {
                assert_eq!(
                    c.unknown_tags,
                    vec!["Bogus1".to_string(), "Bogus2".to_string()]
                );
                assert_eq!(c.available_tags, vec!["Investor".to_string()]);
            }
            Resolved::Definition(_) => panic!("expected a clarification"),
        }
    }

    #[test]
    fn tags_any_and_none_resolve_into_separate_clauses() {
        let tags = vec![tag("Investor"), tag("Cold")];
        let s = PeopleFilterSpec {
            tag_names_any: vec!["Investor".to_string()],
            tag_names_none: vec!["Cold".to_string()],
            ..spec()
        };
        match resolve(&s, &[], &[], &tags) {
            Resolved::Definition(def) => {
                assert!(def.clauses.iter().any(|c| matches!(c, Clause::Tags(_))));
                assert!(def.clauses.iter().any(|c| matches!(c, Clause::NotTags(_))));
            }
            Resolved::Clarification(c) => panic!("{c:?}"),
        }
    }

    #[test]
    fn multiple_dimensions_can_fail_at_once() {
        let stages = vec![stage("Lead")];
        let tags = vec![tag("Investor")];
        let s = PeopleFilterSpec {
            stage_names: vec!["Bogus".to_string()],
            tag_names_any: vec!["AlsoBogus".to_string()],
            ..spec()
        };
        match resolve(&s, &stages, &[], &tags) {
            Resolved::Clarification(c) => {
                assert_eq!(c.unknown_stages, vec!["Bogus".to_string()]);
                assert_eq!(c.unknown_tags, vec!["AlsoBogus".to_string()]);
            }
            Resolved::Definition(_) => panic!("expected a clarification"),
        }
    }

    #[test]
    fn age_and_boolean_axes_pass_through_unresolved() {
        let s = PeopleFilterSpec {
            created: Some(AgeCondition::WithinDays(30)),
            last_contact: Some(AgeCondition::Never),
            sources: vec!["zillow".to_string()],
            ..spec()
        };
        match resolve(&s, &[], &[], &[]) {
            Resolved::Definition(def) => {
                assert!(def.clauses.iter().any(|c| matches!(
                    c,
                    Clause::Created(AgeClause {
                        age: AgeSpec::WithinDays(30)
                    })
                )));
                assert!(def.clauses.iter().any(|c| matches!(
                    c,
                    Clause::LastContact(AgeClause {
                        age: AgeSpec::Never
                    })
                )));
                assert!(def.clauses.iter().any(|c| matches!(c, Clause::Source(_))));
            }
            Resolved::Clarification(c) => panic!("{c:?}"),
        }
    }

    #[test]
    fn find_by_name_prefers_exact_then_case_insensitive_then_reports_collision() {
        let items = vec!["Lead".to_string(), "lead2".to_string()];
        assert!(matches!(
            find_by_name("Lead", &items, |s| s.as_str()),
            NameMatch::One(_)
        ));
        assert!(matches!(
            find_by_name("LEAD2", &items, |s| s.as_str()),
            NameMatch::One(_)
        ));
        assert!(matches!(
            find_by_name("nope", &items, |s| s.as_str()),
            NameMatch::None
        ));
        let dup = vec!["Alex".to_string(), "alex".to_string()];
        assert!(matches!(
            find_by_name("ALEX", &dup, |s| s.as_str()),
            NameMatch::Many(_)
        ));
    }

    /// Regression: two items with the exact SAME name (a real case for
    /// saved lists — a personal and a shared list can share a name, D-046
    /// — and not impossible for member display names either) must report
    /// `Many`, not silently pick the first exact match.
    #[test]
    fn find_by_name_reports_a_collision_even_among_exact_matches() {
        let items = vec!["Weekly".to_string(), "Weekly".to_string()];
        match find_by_name("Weekly", &items, |s| s.as_str()) {
            NameMatch::Many(matches) => assert_eq!(matches.len(), 2),
            other => panic!(
                "expected Many, got a {} match",
                if matches!(other, NameMatch::One(_)) {
                    "One"
                } else {
                    "None"
                }
            ),
        }
    }
}
