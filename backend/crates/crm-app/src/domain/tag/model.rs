use serde::Serialize;

use crate::domain::admin::Role;
use crate::ids::{TagId, UserId};

/// The exact `GET /api/tags` row / `POST`/`PUT /api/tags/{id}` response
/// shape (docs/specs/SLICE_011e.md §5): `{"id","name","person_count",
/// "can_manage"}`. `can_manage` is the server's rule-1 verdict (D-051) for
/// the VIEWER at read time — a display hint; every command re-decides the
/// same rule under the tag row's lock.
#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    pub person_count: i64,
    pub can_manage: bool,
}

/// The Person-detail/Operator tag shape: `{"id","name"}`, no count
/// (docs/specs/SLICE_011e.md §5 — `GET /api/people/{id}` and the
/// `AddPersonTag`/`RemovePersonTag` response both use this narrower
/// shape).
#[derive(Debug, Clone, Serialize)]
pub struct TagRef {
    pub id: TagId,
    pub name: String,
}

/// Rule 1 (D-051): an Organization admin, or the tag's creator while no
/// Person carries it. Shared by every read (`can_manage` on `Tag`) and
/// write (the command's own permission check) so the displayed hint and
/// the enforced rule can never diverge.
pub fn can_manage(
    role: Role,
    actor_user_id: UserId,
    created_by_user_id: UserId,
    person_count: i64,
) -> bool {
    role == Role::Admin || (created_by_user_id == actor_user_id && person_count == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn admin_can_always_manage() {
        let admin = UserId::new(Uuid::new_v4());
        let creator = UserId::new(Uuid::new_v4());
        assert!(can_manage(Role::Admin, admin, creator, 5));
        assert!(can_manage(Role::Admin, admin, creator, 0));
    }

    #[test]
    fn creator_can_manage_only_while_unused() {
        let creator = UserId::new(Uuid::new_v4());
        assert!(can_manage(Role::Member, creator, creator, 0));
        assert!(!can_manage(Role::Member, creator, creator, 1));
    }

    #[test]
    fn non_creator_member_can_never_manage() {
        let member = UserId::new(Uuid::new_v4());
        let creator = UserId::new(Uuid::new_v4());
        assert!(!can_manage(Role::Member, member, creator, 0));
        assert!(!can_manage(Role::Member, member, creator, 1));
    }
}
