//! Per-installation register notes (ADR 0119). Admin session only: API keys never write notes (the
//! guard's key allowlist admits no PUT). Notes annotate the explorer; they never grant write
//! permission and never change the catalog or the observation overlay.
use crate::{
    auth::{json_body, AuthError},
    provisioning::ProvisioningState,
};
use axum::{http::StatusCode, Json};
use nibe_persistence::{RegisterNote, Relevance, MAX_REGISTER_NOTES};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request bodies above this are rejected before parsing (a full set for one pump is ~100 KiB).
pub const MAX_NOTES_BODY_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, Default, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotesMode {
    /// Upsert the given registers; others stay.
    #[default]
    Merge,
    /// The given list becomes the complete set; an empty list clears all notes.
    Replace,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterNoteInput {
    pub register: u16,
    pub relevance: Relevance,
    #[serde(default)]
    pub display_verified: Option<bool>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub source: String,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterNotesUpdate {
    #[serde(default)]
    pub mode: NotesMode,
    pub notes: Vec<RegisterNoteInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterNotesView {
    pub register_notes: std::collections::BTreeMap<u16, RegisterNote>,
}

#[utoipa::path(put, path = "/api/v1/registers/notes", request_body = RegisterNotesUpdate, description = "Admin session only; API keys get 403. Replaces or merges this installation's register notes (relevance, display check, label, note). Notes never grant write permission and never change the catalog or the observation overlay. Returns the stored set, also shown as `register_notes` in the explorer. Factory reset clears them.", responses((status = 200, body = RegisterNotesView), (status = 413), (status = 422, body = crate::auth::AuthErrorBody)))]
pub async fn put_notes(
    axum::extract::State(state): axum::extract::State<ProvisioningState>,
    json: Result<Json<RegisterNotesUpdate>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<RegisterNotesView>, AuthError> {
    if let Err(rejection) = &json {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            return Err(AuthError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "too_large",
                "register notes body is limited to 256 KiB",
            ));
        }
    }
    let request = json_body(json)?;
    let invalid = |message| AuthError::new(StatusCode::UNPROCESSABLE_ENTITY, "validation", message);
    if request.notes.len() > MAX_REGISTER_NOTES {
        return Err(invalid("at most 2048 notes"));
    }
    let mut seen = std::collections::BTreeSet::new();
    if !request.notes.iter().all(|n| seen.insert(n.register)) {
        return Err(invalid("each register at most once"));
    }
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let mut entries = Vec::with_capacity(request.notes.len());
    for input in request.notes {
        let note = RegisterNote {
            relevance: input.relevance,
            display_verified: input.display_verified,
            label: input.label,
            note: input.note,
            source: input.source,
            timestamp_ms,
        };
        note.validate().map_err(invalid)?;
        entries.push((input.register, note));
    }
    let replace = matches!(request.mode, NotesMode::Replace);
    let store = state.auth.store();
    let saved = tokio::task::spawn_blocking(move || store.set_register_notes(entries, replace))
        .await
        .map_err(|_| AuthError::unavailable())?;
    match saved {
        Ok(register_notes) => Ok(Json(RegisterNotesView { register_notes })),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {
            Err(invalid("too many register notes"))
        }
        Err(_) => Err(AuthError::unavailable()),
    }
}
