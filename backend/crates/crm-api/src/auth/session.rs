use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use rand::Rng;
use sha2::Sha256;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::config::SessionSecret;
use crate::domain::admin::Role;
use crate::ids::{OrganizationId, UserId};

pub const COOKIE_NAME: &str = "crm_session";
/// 256-bit token, base64url (no padding) encoded: fixed length so the
/// extractor can reject a syntactically invalid cookie before touching the
/// database (docs/specs/SLICE_001.md §3, §9).
const TOKEN_LEN_BYTES: usize = 32;

pub use super::token_format::{is_valid_token_format, TOKEN_STR_LEN};

type HmacSha256 = Hmac<Sha256>;

/// A session's active Organization, when it has one
/// (docs/specs/SLICE_004.md §3).
#[derive(Debug, Clone)]
pub struct SessionOrganization {
    pub id: OrganizationId,
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct SessionIdentity {
    pub user_id: UserId,
    pub email: String,
    pub display_name: String,
    pub organization: Option<SessionOrganization>,
    pub platform_admin: bool,
}

/// Raw row shape from `verify`'s query, before the Rust-side invariant
/// re-assertion (docs/specs/SLICE_004.md §3: "Rust additionally asserts the
/// invariant on the returned row and treats a violation as invalid —
/// defense in depth, not the primary control").
#[derive(Debug, Clone, sqlx::FromRow)]
struct SessionIdentityRow {
    user_id: Uuid,
    email: String,
    display_name: String,
    organization_id: Option<Uuid>,
    organization_name: Option<String>,
    membership_role: Option<String>,
    platform_admin: bool,
}

fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_LEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(secret: &SessionSecret, token: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(token.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

/// Inserts a fresh session row and returns the raw token (for the cookie)
/// and its expiry (for the cookie's Max-Age). `active_organization_id` is
/// `None` for a platform-admin session with no Organization
/// (docs/specs/SLICE_004.md §3).
pub async fn create(
    pool: &PgPool,
    secret: &SessionSecret,
    user_id: UserId,
    active_organization_id: Option<OrganizationId>,
    ttl: Duration,
) -> Result<(String, DateTime<Utc>), sqlx::Error> {
    let token = generate_token();
    let token_hash = hash_token(secret, &token);
    let expires_at =
        Utc::now() + chrono::Duration::from_std(ttl).expect("ttl fits in chrono::Duration");

    sqlx::query(
        "INSERT INTO user_session (token_hash, user_id, active_organization_id, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&token_hash)
    .bind(user_id.0)
    .bind(active_organization_id.map(|organization_id| organization_id.0))
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok((token, expires_at))
}

/// The single statement that re-verifies both session validity and
/// Organization membership on every request (docs/specs/SLICE_001.md §3;
/// amended by docs/specs/SLICE_004.md §3): a session whose membership has
/// since been revoked or deactivated matches no row here, same as an
/// expired or revoked session — *unless* it is a platform-admin session
/// with no active Organization at all. The `WHERE` predicate is the primary
/// control: `(s.active_organization_id IS NULL AND pa.user_id IS NOT NULL)
/// OR (m.status = 'active')`. This lives in SQL so "Organization present ⇒
/// active membership present" stays true by construction, matching the
/// pre-004 INNER JOIN's safe-by-construction property.
pub async fn verify(
    pool: &PgPool,
    secret: &SessionSecret,
    token: &str,
) -> Result<Option<SessionIdentity>, sqlx::Error> {
    let token_hash = hash_token(secret, token);

    let row = sqlx::query_as::<_, SessionIdentityRow>(
        "SELECT u.id AS user_id, u.email, u.display_name,
                o.id AS organization_id, o.name AS organization_name,
                m.role AS membership_role,
                (pa.user_id IS NOT NULL) AS platform_admin
         FROM user_session s
         JOIN app_user u ON u.id = s.user_id
         LEFT JOIN organization o ON o.id = s.active_organization_id
         LEFT JOIN organization_membership m
             ON m.user_id = s.user_id AND m.organization_id = s.active_organization_id
         LEFT JOIN platform_admin pa ON pa.user_id = s.user_id
         WHERE s.token_hash = $1
           AND s.revoked_at IS NULL
           AND s.expires_at > now()
           AND ((s.active_organization_id IS NULL AND pa.user_id IS NOT NULL)
                OR (m.status = 'active'))",
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    // Defense in depth: re-assert the invariant the WHERE predicate above
    // already enforces (docs/specs/SLICE_004.md §3).
    let organization = match (
        row.organization_id,
        row.organization_name,
        row.membership_role,
    ) {
        (Some(id), Some(name), Some(role_str)) => {
            let Some(role) = Role::from_db_str(&role_str) else {
                return Ok(None);
            };
            Some(SessionOrganization {
                id: OrganizationId::new(id),
                name,
                role,
            })
        }
        (None, None, None) if row.platform_admin => None,
        _ => return Ok(None),
    };

    Ok(Some(SessionIdentity {
        user_id: UserId::new(row.user_id),
        email: row.email,
        display_name: row.display_name,
        organization,
        platform_admin: row.platform_admin,
    }))
}

/// Revokes the session matching `token`, if any. Idempotent: revoking an
/// already-revoked, expired, or unknown token is not an error.
pub async fn revoke_by_token(
    pool: &PgPool,
    secret: &SessionSecret,
    token: &str,
) -> Result<(), sqlx::Error> {
    let token_hash = hash_token(secret, token);
    sqlx::query(
        "UPDATE user_session SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(&token_hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// `domain` is `None` by default (host-only cookie). It is set only for
/// the cross-subdomain tunnel case (e.g. app.tarams.org's browser calling
/// api.tarams.org directly), alongside CORS — see `config::Config::
/// cors_allowed_origin`.
pub fn build_cookie(
    token: String,
    ttl: Duration,
    secure: bool,
    domain: Option<&str>,
) -> Cookie<'static> {
    let mut builder = Cookie::build((COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(ttl.as_secs() as i64));
    if let Some(domain) = domain {
        builder = builder.domain(domain.to_string());
    }
    builder.build()
}

/// A `Set-Cookie` with identical attributes to `build_cookie` (including
/// `domain`) and `Max-Age=0`, so browsers actually match and clear the
/// original cookie (docs/specs/SLICE_001.md §4).
pub fn build_clearing_cookie(secure: bool, domain: Option<&str>) -> Cookie<'static> {
    let mut builder = Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::ZERO);
    if let Some(domain) = domain {
        builder = builder.domain(domain.to_string());
    }
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secret() -> SessionSecret {
        // Config::from_source normally constructs this; tests build one
        // directly via the same public parsing path.
        crate::config::Config::from_source(|key| match key {
            "CRM_SESSION_SECRET" => Some("a".repeat(32)),
            "CRM_RAW_PAYLOAD_KEY" => Some("ab".repeat(32)),
            "CENTRIFUGO_HTTP_API_KEY" => Some("test-key".to_string()),
            "CENTRIFUGO_TOKEN_HMAC_SECRET" => Some("c".repeat(32)),
            _ => None,
        })
        .unwrap()
        .session_secret
    }

    #[test]
    fn generated_token_has_expected_format() {
        let token = generate_token();
        assert!(
            is_valid_token_format(&token),
            "token {token:?} failed format check"
        );
    }

    #[test]
    fn hash_is_deterministic_for_same_secret_and_token() {
        let secret = test_secret();
        let token = "fixed-token-value-for-this-test-1234567890a";
        assert_eq!(hash_token(&secret, token), hash_token(&secret, token));
    }

    #[test]
    fn hash_differs_across_secrets() {
        let secret_a = test_secret();
        let secret_b = crate::config::Config::from_source(|key| match key {
            "CRM_SESSION_SECRET" => Some("b".repeat(32)),
            "CRM_RAW_PAYLOAD_KEY" => Some("ab".repeat(32)),
            "CENTRIFUGO_HTTP_API_KEY" => Some("test-key".to_string()),
            "CENTRIFUGO_TOKEN_HMAC_SECRET" => Some("c".repeat(32)),
            _ => None,
        })
        .unwrap()
        .session_secret;
        let token = "fixed-token-value-for-this-test-1234567890a";
        assert_ne!(hash_token(&secret_a, token), hash_token(&secret_b, token));
    }

    #[test]
    fn cookie_has_expected_attributes() {
        let cookie = build_cookie(
            "token-value".to_string(),
            Duration::from_secs(3600),
            true,
            None,
        );
        assert_eq!(cookie.name(), COOKIE_NAME);
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(cookie.domain(), None);
        assert_eq!(cookie.max_age(), Some(time::Duration::seconds(3600)));
    }

    #[test]
    fn cookie_secure_flag_follows_config() {
        let cookie = build_cookie(
            "token-value".to_string(),
            Duration::from_secs(3600),
            false,
            None,
        );
        assert_eq!(cookie.secure(), Some(false));
    }

    #[test]
    fn cookie_domain_set_when_configured() {
        // No leading dot: RFC 6265 already implies subdomain matching for
        // any Domain attribute, and the cookie crate normalizes a leading
        // dot away, so ".tarams.org" and "tarams.org" are equivalent —
        // use the modern form to avoid confusing round-trip mismatches.
        let cookie = build_cookie(
            "token-value".to_string(),
            Duration::from_secs(3600),
            true,
            Some("tarams.org"),
        );
        assert_eq!(cookie.domain(), Some("tarams.org"));
    }

    #[test]
    fn clearing_cookie_matches_attributes_and_expires_immediately() {
        let cookie = build_clearing_cookie(true, None);
        assert_eq!(cookie.name(), COOKIE_NAME);
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(cookie.domain(), None);
        assert_eq!(cookie.max_age(), Some(time::Duration::ZERO));
    }

    #[test]
    fn clearing_cookie_domain_matches_when_configured() {
        let cookie = build_clearing_cookie(true, Some("tarams.org"));
        assert_eq!(cookie.domain(), Some("tarams.org"));
    }
}
