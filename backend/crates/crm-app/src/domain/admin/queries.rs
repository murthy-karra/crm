//! Slice 004 read models and shared query helpers
//! (docs/specs/SLICE_004.md §4). Read models back the HTTP routes; the
//! lower-level helpers in the second half of this file are used by the
//! `commands::*` modules to keep each command's SQL close to its
//! behavior while sharing the row shapes and enum decoding in one place.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::{MembershipStatus, Role};
use crate::ids::{InvitationId, OrganizationId, UserId};

fn decode_role(s: &str) -> Result<Role, sqlx::Error> {
    Role::from_db_str(s).ok_or_else(|| sqlx::Error::Decode(format!("invalid role: {s}").into()))
}

fn decode_status(s: &str) -> Result<MembershipStatus, sqlx::Error> {
    MembershipStatus::from_db_str(s)
        .ok_or_else(|| sqlx::Error::Decode(format!("invalid membership status: {s}").into()))
}

// --- Organization ----------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct OrganizationRef {
    pub id: OrganizationId,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationState {
    Ok,
    PendingFirstAdmin,
    NeedsAttention,
}

impl OrganizationState {
    /// D-026 §5 / docs/specs/SLICE_004.md §2: `ok` = ≥1 active admin;
    /// `pending_first_admin` = 0 active admins and ≥1 pending (unexpired)
    /// admin invitation; `needs_attention` = 0 active admins and no
    /// pending admin invitation.
    fn derive(admin_count: i64, pending_admin_invitations: i64) -> Self {
        if admin_count > 0 {
            OrganizationState::Ok
        } else if pending_admin_invitations > 0 {
            OrganizationState::PendingFirstAdmin
        } else {
            OrganizationState::NeedsAttention
        }
    }

    /// Sort priority for the platform list: `needs_attention`,
    /// `pending_first_admin`, `ok`, then name (docs/specs/SLICE_004.md §5).
    fn sort_rank(self) -> u8 {
        match self {
            OrganizationState::NeedsAttention => 0,
            OrganizationState::PendingFirstAdmin => 1,
            OrganizationState::Ok => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformOrganizationItem {
    pub id: OrganizationId,
    pub name: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub member_count: i64,
    pub admin_count: i64,
    pub pending_admin_invitations: i64,
    pub state: OrganizationState,
}

struct PlatformOrganizationRow {
    id: Uuid,
    name: String,
    status: String,
    created_at: DateTime<Utc>,
    member_count: i64,
    admin_count: i64,
    pending_admin_invitations: i64,
}

/// `organization::list_for_platform()` (docs/specs/SLICE_004.md §4): one
/// statement with subqueries, no N+1; ordered `needs_attention`,
/// `pending_first_admin`, `ok`, then name (§5).
pub async fn list_for_platform(
    conn: &mut PgConnection,
) -> Result<Vec<PlatformOrganizationItem>, sqlx::Error> {
    let rows = sqlx::query_as!(
        PlatformOrganizationRow,
        r#"SELECT o.id, o.name, o.status, o.created_at,
             (SELECT count(*) FROM organization_membership m
                WHERE m.organization_id = o.id AND m.status = 'active') as "member_count!",
             (SELECT count(*) FROM organization_membership m
                WHERE m.organization_id = o.id AND m.status = 'active' AND m.role = 'admin')
                as "admin_count!",
             (SELECT count(*) FROM invitation i
                WHERE i.organization_id = o.id AND i.role = 'admin'
                  AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > now())
                as "pending_admin_invitations!"
           FROM organization o"#,
    )
    .fetch_all(conn)
    .await?;

    let mut items: Vec<PlatformOrganizationItem> = rows
        .into_iter()
        .map(|r| {
            let state = OrganizationState::derive(r.admin_count, r.pending_admin_invitations);
            PlatformOrganizationItem {
                id: OrganizationId::new(r.id),
                name: r.name,
                status: r.status,
                created_at: r.created_at,
                member_count: r.member_count,
                admin_count: r.admin_count,
                pending_admin_invitations: r.pending_admin_invitations,
                state,
            }
        })
        .collect();

    items.sort_by(|a, b| {
        a.state
            .sort_rank()
            .cmp(&b.state.sort_rank())
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(items)
}

/// A single Organization for the platform detail route
/// (`GET /api/platform/organizations/{id}`).
pub async fn platform_organization_by_id(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Option<PlatformOrganizationItem>, sqlx::Error> {
    let row = sqlx::query_as!(
        PlatformOrganizationRow,
        r#"SELECT o.id, o.name, o.status, o.created_at,
             (SELECT count(*) FROM organization_membership m
                WHERE m.organization_id = o.id AND m.status = 'active') as "member_count!",
             (SELECT count(*) FROM organization_membership m
                WHERE m.organization_id = o.id AND m.status = 'active' AND m.role = 'admin')
                as "admin_count!",
             (SELECT count(*) FROM invitation i
                WHERE i.organization_id = o.id AND i.role = 'admin'
                  AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > now())
                as "pending_admin_invitations!"
           FROM organization o
           WHERE o.id = $1"#,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;

    Ok(row.map(|r| {
        let state = OrganizationState::derive(r.admin_count, r.pending_admin_invitations);
        PlatformOrganizationItem {
            id: OrganizationId::new(r.id),
            name: r.name,
            status: r.status,
            created_at: r.created_at,
            member_count: r.member_count,
            admin_count: r.admin_count,
            pending_admin_invitations: r.pending_admin_invitations,
            state,
        }
    }))
}

/// Whether `organization_id` exists at all — used by the platform routes
/// to return a byte-identical 404 for a nonexistent Organization
/// (docs/specs/SLICE_004.md §7).
pub async fn organization_exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM organization WHERE id = $1"#,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

// --- Members -----------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MemberView {
    pub user_id: UserId,
    pub display_name: String,
    pub email: String,
    pub role: Role,
    pub status: MembershipStatus,
    pub joined_at: DateTime<Utc>,
    pub assigned_people_count: i64,
}

struct MemberRow {
    user_id: Uuid,
    display_name: String,
    email: String,
    role: String,
    status: String,
    joined_at: DateTime<Utc>,
    assigned_people_count: i64,
}

impl MemberRow {
    fn into_view(self) -> Result<MemberView, sqlx::Error> {
        Ok(MemberView {
            user_id: UserId::new(self.user_id),
            display_name: self.display_name,
            email: self.email,
            role: decode_role(&self.role)?,
            status: decode_status(&self.status)?,
            joined_at: self.joined_at,
            assigned_people_count: self.assigned_people_count,
        })
    }
}

/// `organization::members(org)` (docs/specs/SLICE_004.md §4):
/// `GET /api/organization/members`, additively extended with `role`,
/// `status`, `assigned_people_count` (D-027 §3).
pub async fn members(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Vec<MemberView>, sqlx::Error> {
    let rows = sqlx::query_as!(
        MemberRow,
        r#"SELECT u.id as user_id, u.display_name, u.email, m.role, m.status,
             m.created_at as joined_at,
             (SELECT count(*) FROM person p
                WHERE p.organization_id = $1 AND p.assigned_user_id = u.id) as "assigned_people_count!"
           FROM organization_membership m
           JOIN app_user u ON u.id = m.user_id
           WHERE m.organization_id = $1
           ORDER BY m.created_at, u.id"#,
        organization_id.0,
    )
    .fetch_all(conn)
    .await?;

    rows.into_iter().map(MemberRow::into_view).collect()
}

/// A single member row, scoped to the Organization — used to build the
/// `{"member": {...}}` response after `ChangeMemberRole`/`SetMemberStatus`
/// and by the platform detail route.
pub async fn member_view(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<Option<MemberView>, sqlx::Error> {
    let row = sqlx::query_as!(
        MemberRow,
        r#"SELECT u.id as user_id, u.display_name, u.email, m.role, m.status,
             m.created_at as joined_at,
             (SELECT count(*) FROM person p
                WHERE p.organization_id = $1 AND p.assigned_user_id = u.id) as "assigned_people_count!"
           FROM organization_membership m
           JOIN app_user u ON u.id = m.user_id
           WHERE m.organization_id = $1 AND m.user_id = $2"#,
        organization_id.0,
        user_id.0,
    )
    .fetch_optional(conn)
    .await?;

    row.map(MemberRow::into_view).transpose()
}

/// Row shape shared by `ChangeMemberRole`/`SetMemberStatus`'s
/// check-then-act (docs/specs/SLICE_004.md §4).
pub struct MemberRoleStatus {
    pub role: Role,
    pub status: MembershipStatus,
}

pub async fn member_role_status(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<Option<MemberRoleStatus>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT role, status FROM organization_membership
           WHERE organization_id = $1 AND user_id = $2"#,
        organization_id.0,
        user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;

    row.map(|r| {
        Ok(MemberRoleStatus {
            role: decode_role(&r.role)?,
            status: decode_status(&r.status)?,
        })
    })
    .transpose()
}

/// Active admins in `organization_id`, optionally excluding one user — the
/// last-active-admin invariant check (D-026 §2, D-027 §4;
/// docs/specs/SLICE_004.md §4/§7), always called under the per-Organization
/// `admin:` advisory lock.
pub async fn count_active_admins_excluding(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    excluding_user_id: UserId,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT count(*) as "count!" FROM organization_membership
           WHERE organization_id = $1 AND role = 'admin' AND status = 'active' AND user_id != $2"#,
        organization_id.0,
        excluding_user_id.0,
    )
    .fetch_one(conn)
    .await?;
    Ok(row.count)
}

pub async fn update_membership_role(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
    role: Role,
) -> Result<(), sqlx::Error> {
    let role_str = role.as_str();
    sqlx::query!(
        r#"UPDATE organization_membership SET role = $3, updated_at = now()
           WHERE organization_id = $1 AND user_id = $2"#,
        organization_id.0,
        user_id.0,
        role_str,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update_membership_status(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
    status: MembershipStatus,
) -> Result<(), sqlx::Error> {
    let status_str = status.as_str();
    sqlx::query!(
        r#"UPDATE organization_membership SET status = $3, updated_at = now()
           WHERE organization_id = $1 AND user_id = $2"#,
        organization_id.0,
        user_id.0,
        status_str,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The per-Organization `admin:` advisory lock (docs/specs/SLICE_004.md
/// §4): serializes `ChangeMemberRole`/`SetMemberStatus` for one
/// Organization, in a distinct namespace from `receive_inquiry`'s
/// `intake:` lock so membership changes never contend with intake. Unlike
/// that lock, this one blocks (the critical section is one count plus one
/// UPDATE — cheap enough that a blocking wait is acceptable, docs/specs/
/// SLICE_004.md §7).
pub async fn acquire_admin_lock(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    let organization_id_text = organization_id.to_string();
    sqlx::query!(
        r#"SELECT pg_advisory_xact_lock(hashtextextended('admin:' || $1::text, 0))"#,
        organization_id_text,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
    role: Role,
    status: MembershipStatus,
) -> Result<(), sqlx::Error> {
    let role_str = role.as_str();
    let status_str = status.as_str();
    sqlx::query!(
        r#"INSERT INTO organization_membership (organization_id, user_id, role, status)
           VALUES ($1, $2, $3, $4)"#,
        organization_id.0,
        user_id.0,
        role_str,
        status_str,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Whether `email` (already normalized) belongs to a current member (any
/// status) of `organization_id` — `IssueInvitation`'s `AlreadyMember` check
/// (docs/specs/SLICE_004.md §4).
pub async fn is_member_by_email(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    normalized_email: &str,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM organization_membership m
           JOIN app_user u ON u.id = m.user_id
           WHERE m.organization_id = $1 AND lower(u.email) = $2"#,
        organization_id.0,
        normalized_email,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

pub async fn app_user_id_by_email(
    conn: &mut PgConnection,
    normalized_email: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT id FROM app_user WHERE lower(email) = $1"#,
        normalized_email,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| r.id))
}

/// Revokes every active session this member holds for this Organization
/// (docs/specs/SLICE_004.md §4 `SetMemberStatus`: "On `inactive`").
pub async fn revoke_sessions_for_member(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE user_session SET revoked_at = now()
           WHERE user_id = $1 AND active_organization_id = $2 AND revoked_at IS NULL"#,
        user_id.0,
        organization_id.0,
    )
    .execute(conn)
    .await?;
    Ok(())
}

// --- Invitations ---------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationStatus {
    Pending,
    Expired,
    Accepted,
    Revoked,
}

/// Invitation state is derived, never stored (docs/specs/SLICE_004.md §2).
fn derive_invitation_status(
    accepted_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> InvitationStatus {
    if accepted_at.is_some() {
        InvitationStatus::Accepted
    } else if revoked_at.is_some() {
        InvitationStatus::Revoked
    } else if expires_at <= now {
        InvitationStatus::Expired
    } else {
        InvitationStatus::Pending
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InvitedByRef {
    pub id: UserId,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvitationView {
    pub id: InvitationId,
    pub email: String,
    pub role: Role,
    pub status: InvitationStatus,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub invited_by: InvitedByRef,
}

struct InvitationListRow {
    id: Uuid,
    email: String,
    role: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    accepted_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    invited_by_id: Uuid,
    invited_by_display_name: String,
}

/// `invitation::list(org)` (docs/specs/SLICE_004.md §4): never
/// `token_hash`; accepted/revoked/expired invitations older than 30 days
/// omitted (§14 default 13).
pub async fn list_invitations(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Vec<InvitationView>, sqlx::Error> {
    let now = Utc::now();
    let rows = sqlx::query_as!(
        InvitationListRow,
        r#"SELECT i.id, i.email, i.role, i.expires_at, i.created_at,
             i.accepted_at, i.revoked_at,
             u.id as invited_by_id, u.display_name as invited_by_display_name
           FROM invitation i
           JOIN app_user u ON u.id = i.invited_by_user_id
           WHERE i.organization_id = $1
             AND NOT (
               (i.accepted_at IS NOT NULL AND i.accepted_at < now() - interval '30 days')
               OR (i.revoked_at IS NOT NULL AND i.revoked_at < now() - interval '30 days')
               OR (i.accepted_at IS NULL AND i.revoked_at IS NULL
                   AND i.expires_at <= now() AND i.expires_at < now() - interval '30 days')
             )
           ORDER BY i.created_at DESC, i.id"#,
        organization_id.0,
    )
    .fetch_all(conn)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(InvitationView {
                id: InvitationId::new(r.id),
                email: r.email,
                role: decode_role(&r.role)?,
                status: derive_invitation_status(r.accepted_at, r.revoked_at, r.expires_at, now),
                expires_at: r.expires_at,
                created_at: r.created_at,
                invited_by: InvitedByRef {
                    id: UserId::new(r.invited_by_id),
                    display_name: r.invited_by_display_name,
                },
            })
        })
        .collect()
}

/// Full row for a single invitation, looked up by id and scoped to an
/// Organization (`RevokeInvitation`) or by token hash (public preview/
/// accept) — one shape, two lookups.
pub struct InvitationRow {
    pub id: InvitationId,
    pub organization_id: OrganizationId,
    pub organization_name: String,
    pub email: String,
    pub role: Role,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl InvitationRow {
    pub fn status(&self, now: DateTime<Utc>) -> InvitationStatus {
        derive_invitation_status(self.accepted_at, self.revoked_at, self.expires_at, now)
    }
}

struct InvitationFullDbRow {
    id: Uuid,
    organization_id: Uuid,
    organization_name: String,
    email: String,
    role: String,
    expires_at: DateTime<Utc>,
    accepted_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

impl InvitationFullDbRow {
    fn into_row(self) -> Result<InvitationRow, sqlx::Error> {
        Ok(InvitationRow {
            id: InvitationId::new(self.id),
            organization_id: OrganizationId::new(self.organization_id),
            organization_name: self.organization_name,
            email: self.email,
            role: decode_role(&self.role)?,
            expires_at: self.expires_at,
            accepted_at: self.accepted_at,
            revoked_at: self.revoked_at,
        })
    }
}

/// `invitation::preview(token_hash)` (docs/specs/SLICE_004.md §4), also
/// used by `AcceptInvitation`'s pre-hash validation pass (no lock; state is
/// re-checked under `FOR UPDATE` inside the transaction).
pub async fn find_invitation_by_token_hash(
    conn: &mut PgConnection,
    token_hash: &str,
) -> Result<Option<InvitationRow>, sqlx::Error> {
    let row = sqlx::query_as!(
        InvitationFullDbRow,
        r#"SELECT i.id, i.organization_id, o.name as organization_name,
             i.email, i.role, i.expires_at, i.accepted_at, i.revoked_at
           FROM invitation i
           JOIN organization o ON o.id = i.organization_id
           WHERE i.token_hash = $1"#,
        token_hash,
    )
    .fetch_optional(conn)
    .await?;

    row.map(InvitationFullDbRow::into_row).transpose()
}

/// `SELECT … FOR UPDATE` by token hash, for `AcceptInvitation`'s
/// transactional re-check (docs/specs/SLICE_004.md §4).
pub async fn lock_invitation_by_token_hash(
    conn: &mut PgConnection,
    token_hash: &str,
) -> Result<Option<InvitationRow>, sqlx::Error> {
    let row = sqlx::query_as!(
        InvitationFullDbRow,
        r#"SELECT i.id, i.organization_id, o.name as organization_name,
             i.email, i.role, i.expires_at, i.accepted_at, i.revoked_at
           FROM invitation i
           JOIN organization o ON o.id = i.organization_id
           WHERE i.token_hash = $1
           FOR UPDATE OF i"#,
        token_hash,
    )
    .fetch_optional(conn)
    .await?;

    row.map(InvitationFullDbRow::into_row).transpose()
}

/// `SELECT … FOR UPDATE`, scoped to `organization_id` — `RevokeInvitation`
/// (docs/specs/SLICE_004.md §4): unknown or other-Organization id returns
/// `None`, mapped to a byte-identical `NotFound`.
pub async fn lock_invitation_in_org(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    invitation_id: InvitationId,
) -> Result<Option<InvitationRow>, sqlx::Error> {
    let row = sqlx::query_as!(
        InvitationFullDbRow,
        r#"SELECT i.id, i.organization_id, o.name as organization_name,
             i.email, i.role, i.expires_at, i.accepted_at, i.revoked_at
           FROM invitation i
           JOIN organization o ON o.id = i.organization_id
           WHERE i.id = $1 AND i.organization_id = $2
           FOR UPDATE OF i"#,
        invitation_id.0,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;

    row.map(InvitationFullDbRow::into_row).transpose()
}

/// The currently-open (not accepted, not revoked) invitation for
/// `(organization_id, email)`, if any — `IssueInvitation`'s supersede
/// check.
pub async fn find_open_invitation(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    normalized_email: &str,
) -> Result<Option<InvitationId>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT id FROM invitation
           WHERE organization_id = $1 AND email = $2
             AND accepted_at IS NULL AND revoked_at IS NULL"#,
        organization_id.0,
        normalized_email,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| InvitationId::new(r.id)))
}

pub async fn supersede_invitation(
    conn: &mut PgConnection,
    invitation_id: InvitationId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE invitation SET revoked_at = now(), revoke_reason = 'superseded' WHERE id = $1"#,
        invitation_id.0,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub struct NewInvitation<'a> {
    pub organization_id: OrganizationId,
    pub email: &'a str,
    pub role: Role,
    pub token_hash: &'a str,
    pub invited_by_user_id: UserId,
    pub expires_at: DateTime<Utc>,
}

pub async fn insert_invitation(
    conn: &mut PgConnection,
    new: NewInvitation<'_>,
) -> Result<(InvitationId, DateTime<Utc>), sqlx::Error> {
    let role_str = new.role.as_str();
    let row = sqlx::query!(
        r#"INSERT INTO invitation
            (organization_id, email, role, token_hash, invited_by_user_id, expires_at)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, created_at"#,
        new.organization_id.0,
        new.email,
        role_str,
        new.token_hash,
        new.invited_by_user_id.0,
        new.expires_at,
    )
    .fetch_one(conn)
    .await?;
    Ok((InvitationId::new(row.id), row.created_at))
}

pub async fn revoke_invitation_row(
    conn: &mut PgConnection,
    invitation_id: InvitationId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE invitation SET revoked_at = now(), revoke_reason = 'revoked' WHERE id = $1"#,
        invitation_id.0,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn mark_invitation_accepted(
    conn: &mut PgConnection,
    invitation_id: InvitationId,
    accepted_user_id: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE invitation SET accepted_at = now(), accepted_user_id = $2 WHERE id = $1"#,
        invitation_id.0,
        accepted_user_id.0,
    )
    .execute(conn)
    .await?;
    Ok(())
}

// --- app_user / local_credential / organization (write helpers) ---------

pub async fn insert_organization(
    conn: &mut PgConnection,
    name: &str,
    intake_slug: &str,
    intake_token: &str,
) -> Result<OrganizationId, sqlx::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO organization (name, intake_slug, intake_token)
           VALUES ($1, $2, $3) RETURNING id"#,
        name,
        intake_slug,
        intake_token,
    )
    .fetch_one(conn)
    .await?;
    Ok(OrganizationId::new(row.id))
}

/// Which of `candidates` are already taken (docs/specs/SLICE_007a.md §4:
/// pre-select rather than retry-after-violation, which would abort the
/// transaction).
pub async fn taken_intake_slugs(
    conn: &mut PgConnection,
    candidates: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT intake_slug FROM organization WHERE intake_slug = ANY($1)"#,
        candidates,
    )
    .fetch_all(conn)
    .await
}

/// `(intake_slug, intake_token)` for one Organization.
pub async fn organization_intake_address(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT intake_slug, intake_token FROM organization WHERE id = $1"#,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.intake_slug, r.intake_token)))
}

// --- Slice 007c: unattended intake routing (docs/specs/SLICE_007c.md §3, §4) ---

/// The stored `intake_default_assignee_user_id`, regardless of that
/// member's current status — `GET /api/organization/intake-settings`
/// (§5) must keep reflecting the setting after the member is later
/// deactivated (criterion 10); the deactivated-warning state is computed
/// client-side from the members list.
pub async fn intake_default_assignee_user_id(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Option<UserId>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT intake_default_assignee_user_id FROM organization WHERE id = $1"#,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row
        .and_then(|r| r.intake_default_assignee_user_id)
        .map(UserId::new))
}

/// `PUT /api/organization/intake-settings` (§5): sets or clears the
/// default assignee and maintains `updated_at` (the column-level grant
/// includes it — the membership-grant precedent).
pub async fn update_intake_default_assignee(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: Option<UserId>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE organization SET intake_default_assignee_user_id = $2, updated_at = now()
           WHERE id = $1"#,
        organization_id.0,
        user_id.map(|id| id.0),
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// PUT validation (§5): true only for an active member of *this*
/// Organization — a nonexistent user, another Organization's member, and
/// an inactive member all read `false` here, giving the route's
/// byte-identical 422 `invalid_assignee` (no existence leak).
pub async fn is_active_member(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM organization_membership
           WHERE organization_id = $1 AND user_id = $2 AND status = 'active'"#,
        organization_id.0,
        user_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

/// Routing-time lookup (§4 routing matrix step 4): the configured default
/// assignee, but only if they are still an active member of this
/// Organization — the join fails closed to `None` (routed `unassigned`)
/// when the setting is unset, the member was deactivated, or the member
/// row is otherwise gone. Best-effort under READ COMMITTED by design (§3
/// "Deactivated default assignee"): no row locking against a deactivation
/// racing this read.
pub async fn active_intake_default_assignee(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Option<UserId>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT o.intake_default_assignee_user_id as "user_id!"
           FROM organization o
           JOIN organization_membership m
             ON m.organization_id = o.id
            AND m.user_id = o.intake_default_assignee_user_id
            AND m.status = 'active'
           WHERE o.id = $1"#,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| UserId::new(r.user_id)))
}

pub async fn insert_app_user(
    conn: &mut PgConnection,
    email: &str,
    display_name: &str,
) -> Result<UserId, sqlx::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO app_user (email, display_name) VALUES ($1, $2) RETURNING id"#,
        email,
        display_name,
    )
    .fetch_one(conn)
    .await?;
    Ok(UserId::new(row.id))
}

pub async fn insert_local_credential(
    conn: &mut PgConnection,
    user_id: UserId,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO local_credential (user_id, password_hash) VALUES ($1, $2)"#,
        user_id.0,
        password_hash,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update_local_credential(
    conn: &mut PgConnection,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE local_credential SET password_hash = $2, updated_at = now() WHERE user_id = $1"#,
        user_id,
        password_hash,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// `crm_migrator`-only helpers for `GrantPlatformAdmin` (bootstrap, runs on
/// `MIGRATION_DATABASE_URL` — docs/specs/SLICE_004.md §11).
pub async fn find_or_create_app_user(
    pool: &PgPool,
    email: &str,
    display_name: &str,
) -> Result<Uuid, sqlx::Error> {
    if let Some(id) = app_user_id_by_email(&mut *pool.acquire().await?, email).await? {
        return Ok(id);
    }
    let mut conn = pool.acquire().await?;
    insert_app_user(&mut conn, email, display_name)
        .await
        .map(UserId::as_uuid)
}

pub async fn upsert_local_credential(
    pool: &PgPool,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO local_credential (user_id, password_hash) VALUES ($1, $2)
           ON CONFLICT (user_id) DO UPDATE SET password_hash = excluded.password_hash, updated_at = now()"#,
        user_id,
        password_hash,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_platform_admin_if_absent(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO platform_admin (user_id, granted_via) VALUES ($1, 'cli')
           ON CONFLICT (user_id) DO NOTHING"#,
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The sole `platform_admin` row's user id — used by the CLI when `--as`
/// is not given (docs/specs/SLICE_004.md §11): `Ok(None)` for zero rows,
/// `Ok(None)` is also returned for "several" (the caller distinguishes by
/// also checking `count_platform_admins`).
pub async fn count_platform_admins(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!(r#"SELECT count(*) as "count!" FROM platform_admin"#)
        .fetch_one(pool)
        .await?;
    Ok(row.count)
}

pub async fn sole_platform_admin_user_id(pool: &PgPool) -> Result<Option<Uuid>, sqlx::Error> {
    let row = sqlx::query!(r#"SELECT user_id FROM platform_admin"#)
        .fetch_all(pool)
        .await?;
    if row.len() == 1 {
        Ok(Some(row[0].user_id))
    } else {
        Ok(None)
    }
}

/// Whether `user_id` has a `platform_admin` row — used by the CLI to
/// resolve `--as <email>` (docs/specs/SLICE_004.md §11).
pub async fn is_platform_admin(pool: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM platform_admin WHERE user_id = $1"#,
        user_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// `(email, display_name)` for a known user id — used by the CLI when
/// resolving the sole platform admin without `--as` (docs/specs/
/// SLICE_004.md §11).
pub async fn app_user_basic(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT email, display_name FROM app_user WHERE id = $1"#,
        user_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.email, r.display_name)))
}
