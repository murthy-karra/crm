//! Note HTTP surface (Slice 015). The command/read module owns rule-1
//! authorization (D-053 §4), body validation, and the tombstone
//! semantics; this adapter owns only trusted extractor order, strict wire
//! decoding, and the stable JSON envelopes — the `tags.rs` adapter's
//! shape. No note body is ever logged, spanned, or put in an error
//! envelope from this file (AGENTS.md §9, docs/specs/SLICE_015.md §1
//! rule 7): every handler below passes the body straight through to the
//! command and never formats or records it itself.

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, post, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::domain::envelope::CommandContext;
use crate::domain::note::{self, AddNote, DeleteNote, EditNote};
use crate::error::ApiError;
use crate::ids::{NoteId, PersonId};
use crate::state::AppState;

const MAX_NOTE_BODY_BYTES: usize = 128 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/people/{person_id}/notes",
            post(add_note).layer(DefaultBodyLimit::max(MAX_NOTE_BODY_BYTES)),
        )
        .route(
            "/api/people/{person_id}/notes/{note_id}",
            put(edit_note).layer(DefaultBodyLimit::max(MAX_NOTE_BODY_BYTES)),
        )
        .route(
            "/api/people/{person_id}/notes/{note_id}",
            delete(delete_note),
        )
}

/// A `{person_id}` path segment, matching the established Person-route
/// precedent (docs/specs/SLICE_002.md §5 error precedence): a malformed
/// id is 400 independent of auth state, tested service-free.
struct PersonIdPath(PersonId);

impl FromRequestParts<AppState> for PersonIdPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(PersonIdPath(PersonId::new(id)))
    }
}

/// The `{person_id}/notes/{note_id}` pair (docs/specs/SLICE_015.md §5),
/// the `PersonTagIdsPath` pattern: either id being a non-UUID is a 400
/// independent of auth state.
struct PersonNoteIdsPath(PersonId, NoteId);

impl FromRequestParts<AppState> for PersonNoteIdsPath {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path((person_id, note_id)) = Path::<(Uuid, Uuid)>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedRequest)?;
        Ok(PersonNoteIdsPath(
            PersonId::new(person_id),
            NoteId::new(note_id),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddNoteRequest {
    body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditNoteRequest {
    body: String,
}

/// `POST /api/people/{person_id}/notes` (docs/specs/SLICE_015.md §5): any
/// active member.
async fn add_note(
    State(state): State<AppState>,
    PersonIdPath(person_id): PersonIdPath,
    auth: AuthContext,
    body: Result<Json<AddNoteRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let note = note::add_note(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        AddNote {
            person_id,
            body: req.body,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "note": note }))))
}

/// `PUT /api/people/{person_id}/notes/{note_id}` (docs/specs/SLICE_015.md
/// §5): member route; rule 1 is decided inside the command under the note
/// row's lock, not at the route.
async fn edit_note(
    State(state): State<AppState>,
    PersonNoteIdsPath(person_id, note_id): PersonNoteIdsPath,
    auth: AuthContext,
    body: Result<Json<EditNoteRequest>, JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::MalformedRequest)?;

    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = note::edit_note(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        EditNote {
            person_id,
            note_id,
            body: req.body,
        },
    )
    .await?;

    Ok(Json(
        json!({ "note": outcome.note, "changed": outcome.changed }),
    ))
}

/// `DELETE /api/people/{person_id}/notes/{note_id}` (docs/specs/SLICE_015.md
/// §5): member route; rule 1 is decided inside the command. A repeat
/// delete, or delete of a tombstone, is 404 — identical to a nonexistent
/// id.
async fn delete_note(
    State(state): State<AppState>,
    PersonNoteIdsPath(person_id, note_id): PersonNoteIdsPath,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::Unavailable)?;
    let outcome = note::delete_note(
        pool,
        &state.publisher,
        &CommandContext::from_auth(&auth),
        DeleteNote { person_id, note_id },
    )
    .await?;

    Ok(Json(json!({ "deleted": outcome.deleted })))
}
