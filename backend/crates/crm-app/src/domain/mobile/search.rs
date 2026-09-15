//! Transient online discovery: no receipt, selection write or access renewal.
use super::{authority, begin, code, context, MobileError};
use crate::{
    auth::AuthContext,
    domain::person::{discovery, PersonVisibilityScope},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchPeopleRequest {
    pub term: String,
}

#[derive(Serialize)]
pub struct SearchPeopleResponse {
    pub context_id: Uuid,
    pub items: Vec<discovery::DiscoveryItem>,
    pub has_more: bool,
}

fn term(value: &str) -> Result<&str, MobileError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 800 || trimmed.chars().count() > 200 {
        return Err(code(422, "invalid_input"));
    }
    Ok(trimmed)
}

#[tracing::instrument(name="mobile.search_people",skip_all,fields(organization_id=%auth.active_organization_id,actor_id=%auth.actor_user_id,returned_count=tracing::field::Empty,has_more=tracing::field::Empty))]
pub async fn search_people(
    pool: &PgPool,
    auth: &AuthContext,
    context_id: Uuid,
    request: SearchPeopleRequest,
) -> Result<SearchPeopleResponse, MobileError> {
    let mut tx = begin(pool, auth, false).await?;
    authority(&mut tx, auth, false).await?;
    context(&mut tx, auth, context_id, false).await?;
    let term = term(&request.term)?;
    let (items, has_more) =
        discovery::search(&mut tx, &PersonVisibilityScope::from_auth(auth), term).await?;
    for item in &items {
        if serde_json::to_vec(item)
            .map_err(|_| code(503, "unavailable"))?
            .len()
            > 4096
        {
            return Err(code(503, "unavailable"));
        }
    }
    let response = SearchPeopleResponse {
        context_id,
        items,
        has_more,
    };
    if serde_json::to_vec(&response)
        .map_err(|_| code(503, "unavailable"))?
        .len()
        > 128 * 1024
    {
        return Err(code(503, "unavailable"));
    }
    tx.commit().await?;
    tracing::Span::current().record("returned_count", response.items.len());
    tracing::Span::current().record("has_more", has_more);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn term_bounds_count_unicode_scalars_after_trimming() {
        assert_eq!(term("\u{2003} Ada \n").unwrap(), "Ada");
        assert!(term(" \t\n").is_err());
        assert!(term(&"x".repeat(201)).is_err());
        assert!(term(&"😀".repeat(200)).is_ok());
        assert!(term(&"😀".repeat(201)).is_err());
    }
}
