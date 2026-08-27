//! Intake routing modes and round-robin rotation (docs/specs/SLICE_008.md
//! §4; D-041). Distinct from `rotate.rs` (intake ADDRESS token rotation,
//! SLICE_007g) — an unrelated mechanism despite the similar name.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crate::ids::{OrganizationId, UserId};

/// The Organization's configured unattended-intake routing mode (D-041;
/// docs/specs/SLICE_008.md §2), persisted as `organization.intake_routing_mode`
/// (three-value CHECK, migration `20260903000001`). `as_str`/`parse` house
/// style (`Origin::as_str`/`decode`, `RoutingStrategy::as_str`/`from_str`):
/// round-trips, and `parse` fails closed (`None`) on an unrecognized value
/// rather than panicking — the DB CHECK is defense in depth, not the only
/// guard on a read path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakeRoutingMode {
    /// Route to the Organization's configured default assignee (D-035;
    /// docs/specs/SLICE_007c.md §4) — today's behavior, now one of three
    /// explicit modes rather than the only one.
    DefaultAssignee,
    /// Rotate fairly across all active members, continue-anchored, never
    /// reset (D-041; see `next_in_rotation`/`take_next` below).
    RoundRobin,
    /// Leads land unassigned; the settings page warns.
    Unassigned,
}

impl IntakeRoutingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            IntakeRoutingMode::DefaultAssignee => "default_assignee",
            IntakeRoutingMode::RoundRobin => "round_robin",
            IntakeRoutingMode::Unassigned => "unassigned",
        }
    }

    /// `None` on anything else — fails closed rather than panicking; the
    /// DB CHECK (`organization_intake_routing_mode_check`) is defense in
    /// depth, not the only guard on the read path that decodes this back.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "default_assignee" => Some(IntakeRoutingMode::DefaultAssignee),
            "round_robin" => Some(IntakeRoutingMode::RoundRobin),
            "unassigned" => Some(IntakeRoutingMode::Unassigned),
            _ => None,
        }
    }
}

/// A membership's position in the round-robin join order:
/// `(created_at, user_id)` compared lexicographically — identical to the
/// SQL `ORDER BY m.created_at, u.id` both the members list
/// (`admin_queries::members`) and `take_next` below use. `user_id` breaks
/// a same-instant tie (reviewer I1), which is otherwise possible for two
/// memberships inserted in the same batch/transaction — `created_at` alone
/// is not a total order.
///
/// A REACTIVATED member's key is unchanged, because their
/// `organization_membership` row is never deleted, only its `status`
/// flips (D-027): they resume their ORIGINAL join-order slot when they
/// reappear in `take_next`'s active-member query, not the end of the
/// line. This is a stated decision (reviewer S4), not an accident of the
/// implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MembershipKey {
    pub created_at: DateTime<Utc>,
    pub user_id: UserId,
}

impl PartialOrd for MembershipKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MembershipKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.created_at
            .cmp(&other.created_at)
            .then_with(|| self.user_id.as_uuid().cmp(&other.user_id.as_uuid()))
    }
}

/// Pure rotation step (D-041 "continue-anchored, never reset"): the first
/// entry of `ordered` whose key is strictly greater than `anchor`,
/// wrapping to the front when none is. `ordered` is the CURRENT active
/// membership order only — a deactivated member is simply absent from it,
/// which is how "skip deactivated members" falls out of this function
/// without special-casing, and why the anchor is compared by VALUE rather
/// than by searching for it in `ordered`: the last-assigned member may no
/// longer be in the slice at all (deactivated) and the pointer must still
/// advance from their old position, not reset.
///
/// `anchor: None` — no prior rotation (first-ever assignment for this
/// Organization), or the pointer names a member with no membership row at
/// all (unreachable under D-027's never-delete rule, but handled rather
/// than assumed) — starts at the front. `ordered` empty (no active
/// members) → `None`, the empty-pool fail-safe; checked first so an empty
/// slice never indexes.
///
/// `ordered` must already be sorted ascending by `MembershipKey` — this
/// function does not sort it (the caller's query `ORDER BY` does).
pub fn next_in_rotation(
    ordered: &[(MembershipKey, UserId)],
    anchor: Option<MembershipKey>,
) -> Option<UserId> {
    if ordered.is_empty() {
        return None;
    }
    let selected = match anchor {
        None => ordered[0],
        Some(anchor_key) => ordered
            .iter()
            .find(|(key, _)| *key > anchor_key)
            .copied()
            .unwrap_or(ordered[0]),
    };
    Some(selected.1)
}

/// Loads the active-member join order, reads the pointer, computes the
/// next assignee via [`next_in_rotation`], and — only when the pool is
/// non-empty — upserts the pointer to that assignee. Returns `None` on an
/// empty pool, writing nothing (docs/specs/SLICE_008.md §4/§6: the
/// pointer "advances ONLY on an actual round-robin assignment"; "empty
/// pool → unassigned, never an error, no pointer write").
///
/// **Precondition, not enforced by the type system: the caller must
/// already hold the per-Organization `intake:` advisory lock.** This must
/// only ever be called from inside `complete_intake`'s Phase-B
/// transaction, after `pg_try_advisory_xact_lock` has succeeded. That
/// lock makes this read-then-bump single-writer per Organization for the
/// transaction's lifetime, so no additional row locking is taken here —
/// the membership read is READ COMMITTED, the same accepted, self-healing
/// window `active_intake_default_assignee` (SLICE_007c §3) already
/// documents; do not add `FOR SHARE`/`FOR UPDATE` to chase it. A rollback
/// of the surrounding transaction (an `IntakeBusy` abort of a LATER Phase
/// B step, or any other failure) undoes the pointer bump atomically along
/// with everything else, because it was never committed.
pub async fn take_next(
    tx: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Option<UserId>, sqlx::Error> {
    let members = sqlx::query!(
        r#"SELECT m.user_id, m.created_at
           FROM organization_membership m
           WHERE m.organization_id = $1 AND m.status = 'active'
           ORDER BY m.created_at, m.user_id"#,
        organization_id.0,
    )
    .fetch_all(&mut *tx)
    .await?;

    let ordered: Vec<(MembershipKey, UserId)> = members
        .into_iter()
        .map(|r| {
            let user_id = UserId::new(r.user_id);
            (
                MembershipKey {
                    created_at: r.created_at,
                    user_id,
                },
                user_id,
            )
        })
        .collect();

    // The pointer, if any, joined against ITS member's membership row
    // regardless of that member's current status (D-027: never deleted)
    // — so a deactivated pointer member still anchors the cycle
    // correctly instead of vanishing along with their `ordered` entry. No
    // membership row at all (unreachable under D-027, handled rather than
    // assumed) decodes to `anchor = None`, same as "never rotated".
    let pointer = sqlx::query!(
        r#"SELECT r.last_assigned_user_id, m.created_at as "anchor_created_at?"
           FROM intake_rotation r
           LEFT JOIN organization_membership m
             ON m.organization_id = r.organization_id AND m.user_id = r.last_assigned_user_id
           WHERE r.organization_id = $1"#,
        organization_id.0,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let anchor = pointer.and_then(|p| {
        p.anchor_created_at.map(|created_at| MembershipKey {
            created_at,
            user_id: UserId::new(p.last_assigned_user_id),
        })
    });

    let next = next_in_rotation(&ordered, anchor);

    if let Some(user_id) = next {
        sqlx::query!(
            r#"INSERT INTO intake_rotation (organization_id, last_assigned_user_id, updated_at)
               VALUES ($1, $2, now())
               ON CONFLICT (organization_id) DO UPDATE
                 SET last_assigned_user_id = excluded.last_assigned_user_id,
                     updated_at = excluded.updated_at"#,
            organization_id.0,
            user_id.0,
        )
        .execute(&mut *tx)
        .await?;
    }

    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn key(seconds: i64, user_id: UserId) -> MembershipKey {
        MembershipKey {
            created_at: DateTime::from_timestamp(seconds, 0).unwrap(),
            user_id,
        }
    }

    fn user(byte: u8) -> UserId {
        UserId::new(Uuid::from_bytes([byte; 16]))
    }

    #[test]
    fn intake_routing_mode_round_trips_every_variant() {
        for mode in [
            IntakeRoutingMode::DefaultAssignee,
            IntakeRoutingMode::RoundRobin,
            IntakeRoutingMode::Unassigned,
        ] {
            assert_eq!(IntakeRoutingMode::parse(mode.as_str()), Some(mode));
        }
    }

    #[test]
    fn intake_routing_mode_parse_fails_closed_on_unknown_values() {
        assert_eq!(IntakeRoutingMode::parse("bogus"), None);
        assert_eq!(IntakeRoutingMode::parse(""), None);
        assert_eq!(IntakeRoutingMode::parse("Default_Assignee"), None);
    }

    #[test]
    fn empty_pool_is_none_regardless_of_anchor() {
        assert_eq!(next_in_rotation(&[], None), None);
        assert_eq!(next_in_rotation(&[], Some(key(1, user(1)))), None);
    }

    #[test]
    fn no_anchor_starts_at_the_front() {
        let a = user(1);
        let b = user(2);
        let ordered = [(key(1, a), a), (key(2, b), b)];
        assert_eq!(next_in_rotation(&ordered, None), Some(a));
    }

    #[test]
    fn advances_to_the_member_after_the_anchor() {
        let a = user(1);
        let b = user(2);
        let c = user(3);
        let ordered = [(key(1, a), a), (key(2, b), b), (key(3, c), c)];
        assert_eq!(next_in_rotation(&ordered, Some(key(1, a))), Some(b));
        assert_eq!(next_in_rotation(&ordered, Some(key(2, b))), Some(c));
    }

    #[test]
    fn wraps_from_the_last_member_back_to_the_first() {
        let a = user(1);
        let b = user(2);
        let c = user(3);
        let ordered = [(key(1, a), a), (key(2, b), b), (key(3, c), c)];
        assert_eq!(next_in_rotation(&ordered, Some(key(3, c))), Some(a));
    }

    #[test]
    fn skips_a_deactivated_member_absent_from_ordered() {
        // a, b, c join order; b is deactivated, hence absent from
        // `ordered` — rotating past a lands on c, not b. D-041's "skip
        // deactivated members" falls out of b's mere absence.
        let a = user(1);
        let c = user(3);
        let ordered = [(key(1, a), a), (key(3, c), c)]; // b absent
        assert_eq!(next_in_rotation(&ordered, Some(key(1, a))), Some(c));
    }

    #[test]
    fn continues_from_a_deactivated_pointer_member_instead_of_resetting() {
        // The anchor is b's key even though b itself is no longer in
        // `ordered` (deactivated): the pointer survives its member's
        // deactivation and the cycle continues from where it left off
        // rather than resetting to the front.
        let a = user(1);
        let b = user(2);
        let c = user(3);
        let ordered = [(key(1, a), a), (key(3, c), c)]; // b absent
        assert_eq!(next_in_rotation(&ordered, Some(key(2, b))), Some(c));
    }

    #[test]
    fn missing_anchor_row_falls_back_to_the_front() {
        let a = user(1);
        let b = user(2);
        let ordered = [(key(1, a), a), (key(2, b), b)];
        assert_eq!(next_in_rotation(&ordered, None), Some(a));
    }

    #[test]
    fn a_newcomer_joins_at_the_end_of_the_cycle() {
        let a = user(1);
        let b = user(2);
        let newcomer = user(3);
        // The newcomer's created_at is later than everyone's, so they
        // sort last regardless of position in this literal array.
        let ordered = [
            (key(1, a), a),
            (key(2, b), b),
            (key(10, newcomer), newcomer),
        ];
        assert_eq!(next_in_rotation(&ordered, Some(key(2, b))), Some(newcomer));
        assert_eq!(next_in_rotation(&ordered, Some(key(10, newcomer))), Some(a));
    }

    #[test]
    fn created_at_ties_break_on_user_id() {
        let same_instant = 5;
        let lo = user(1);
        let hi = user(2);
        // Two memberships inserted in the same transaction/batch: equal
        // created_at, ordered by user_id — matches the SQL `ORDER BY
        // m.created_at, u.id`, and is what keeps `MembershipKey: Ord` a
        // TOTAL order (reviewer I1).
        let ordered = [(key(same_instant, lo), lo), (key(same_instant, hi), hi)];
        assert_eq!(next_in_rotation(&ordered, None), Some(lo));
        assert_eq!(
            next_in_rotation(&ordered, Some(key(same_instant, lo))),
            Some(hi)
        );
    }

    #[test]
    fn reactivation_resumes_the_original_slot() {
        let a = user(1);
        let b = user(2);
        let c = user(3);
        // b deactivated: absent from `ordered`; rotation continues a -> c.
        let without_b = [(key(1, a), a), (key(3, c), c)];
        assert_eq!(next_in_rotation(&without_b, Some(key(1, a))), Some(c));

        // b reactivates with its ORIGINAL created_at (membership row
        // unchanged — D-027): it reappears at its original slot between a
        // and c. The very next rotation after a lands on b again, not c
        // — a second consecutive "after a" advance would have skipped
        // straight to c had b been treated as a newcomer instead.
        let with_b = [(key(1, a), a), (key(2, b), b), (key(3, c), c)];
        assert_eq!(next_in_rotation(&with_b, Some(key(1, a))), Some(b));
    }

    #[test]
    fn single_member_pool_always_returns_to_itself() {
        let a = user(1);
        let ordered = [(key(1, a), a)];
        assert_eq!(next_in_rotation(&ordered, None), Some(a));
        assert_eq!(next_in_rotation(&ordered, Some(key(1, a))), Some(a));
    }
}
