//! API keys for scripts and integrations (ADR 0110). Keys are created and revoked only with an admin
//! session; a key never reaches setup, Wi-Fi, updates, debug or key management.
use crate::{
    auth::{json_body, AuthError},
    provisioning::ProvisioningState,
};
use axum::{
    extract::{Path, State},
    http::Method,
    Json,
};
use nibe_persistence::{random_token, ApiKey, ApiScope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

const PREFIX: &str = "nibe_";
const MAX_KEYS: usize = 16;

/// SHA-256 (hex) of a well-formed token. Tokens carry 256 random bits, so a fast hash is enough and
/// avoids Argon2's ~1 s of CPU per request on the Pi Zero.
pub fn hash_token(token: &str) -> Option<String> {
    let body = token.strip_prefix(PREFIX)?;
    if body.len() != 64 || !body.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{:x}", Sha256::digest(token.as_bytes())))
}

/// What a key may reach. Read: GET/HEAD status, readings, explorer, events and the stream.
/// Control: additionally POSTs to the named controls and read-only probe sessions. Nothing else:
/// no PUT or DELETE is ever allowed, so keys cannot write register notes (`PUT /api/v1/registers/notes`,
/// ADR 0041) — notes describe the installation and are the admin's call; keys may read them in the explorer.
pub fn key_allows(scope: ApiScope, method: &Method, path: &str) -> bool {
    const READ: [&str; 20] = [
        "/api/v1/history",
        "/api/v1/history/export.csv",
        "/api/v1/history/status",
        "/api/v1/status",
        "/api/v1/device",
        "/api/v1/device/identity",
        "/api/v1/device/capabilities",
        "/api/v1/measurements",
        "/api/v1/registers",
        "/api/v1/registers/catalog",
        "/api/v1/registers/discovered",
        "/api/v1/registers/explorer",
        "/api/v1/registers/polls",
        "/api/v1/diagnostics",
        "/api/v1/events",
        "/api/v1/stream",
        "/api/v1/discovery/status",
        "/api/v1/discovery/probe/status",
        "/api/v1/network/status",
        "/openapi.json",
    ];
    const CONTROL: [&str; 8] = [
        "/api/v1/controls/heating-offset",
        "/api/v1/controls/register-write",
        "/api/v1/controls/register-read",
        "/api/v1/controls/register-poll",
        "/api/v1/controls/alarm-reset",
        "/api/v1/controls/temporary-lux",
        "/api/v1/discovery/probe",
        "/api/v1/discovery/probe/cancel",
    ];
    match *method {
        Method::GET | Method::HEAD => READ.contains(&path),
        Method::POST => scope == ApiScope::Control && CONTROL.contains(&path),
        _ => false,
    }
}

#[derive(Serialize, ToSchema)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub scope: ApiScope,
    pub created_at_ms: u64,
    /// Since the gateway started; not persisted.
    pub last_used_ms: Option<u64>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateApiKey {
    pub name: String,
    pub scope: ApiScope,
}
#[derive(Serialize, ToSchema)]
pub struct CreatedApiKey {
    #[serde(flatten)]
    pub key: ApiKeyInfo,
    /// Shown once. Use as `Authorization: Bearer <token>`.
    pub token: String,
}

fn info(state: &ProvisioningState, key: &ApiKey) -> ApiKeyInfo {
    ApiKeyInfo {
        id: key.id.clone(),
        name: key.name.clone(),
        scope: key.scope,
        created_at_ms: key.created_at_ms,
        last_used_ms: state.auth.key_last_used(&key.id),
    }
}

#[utoipa::path(get, path = "/api/v1/access/api-keys", description = "Admin session only.", responses((status = 200, body = [ApiKeyInfo])))]
pub async fn list(State(state): State<ProvisioningState>) -> Json<Vec<ApiKeyInfo>> {
    let keys = state.auth.store().config().api_keys;
    Json(keys.iter().map(|k| info(&state, k)).collect())
}

#[utoipa::path(post, path = "/api/v1/access/api-keys", request_body = CreateApiKey, description = "Admin session only. The token is returned once.", responses((status = 200, body = CreatedApiKey), (status = 409), (status = 422)))]
pub async fn create(
    State(state): State<ProvisioningState>,
    json: Result<Json<CreateApiKey>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<CreatedApiKey>, AuthError> {
    let request = json_body(json)?;
    let name = request.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err(AuthError::validation());
    }
    let token = format!("{PREFIX}{}", random_token(32));
    let key = ApiKey {
        id: random_token(4),
        name,
        scope: request.scope,
        hash: hash_token(&token).expect("generated token is well-formed"),
        created_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
    };
    let store = state.auth.store();
    let saved = key.clone();
    let mut full = false;
    tokio::task::spawn_blocking(move || {
        store.update(|config| {
            if config.api_keys.len() >= MAX_KEYS {
                full = true;
            } else {
                config.api_keys.push(saved);
            }
        })
    })
    .await
    .map_err(|_| AuthError::unavailable())?
    .map_err(|_| AuthError::unavailable())?;
    if full {
        return Err(AuthError::new(
            axum::http::StatusCode::CONFLICT,
            "conflict",
            "API key limit reached; revoke one first",
        ));
    }
    Ok(Json(CreatedApiKey {
        key: info(&state, &key),
        token,
    }))
}

#[utoipa::path(delete, path = "/api/v1/access/api-keys/{id}", description = "Admin session only.", responses((status = 200), (status = 404)))]
pub async fn revoke(
    State(state): State<ProvisioningState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let store = state.auth.store();
    let found = store.config().api_keys.iter().any(|k| k.id == id);
    if !found {
        return Err(AuthError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "no such API key",
        ));
    }
    tokio::task::spawn_blocking(move || store.update(|c| c.api_keys.retain(|k| k.id != id)))
        .await
        .map_err(|_| AuthError::unavailable())?
        .map_err(|_| AuthError::unavailable())?;
    Ok(Json(serde_json::json!({ "revoked": true })))
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangePassword {
    pub current_password: String,
    pub new_password: String,
}
#[utoipa::path(post, path = "/api/v1/access/password", request_body = ChangePassword, description = "Admin session only. Ends all sessions and revokes all API keys.", responses((status = 200), (status = 401), (status = 422)))]
pub async fn change_password(
    State(state): State<ProvisioningState>,
    json: Result<Json<ChangePassword>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let request = json_body(json)?;
    state
        .auth
        .change_password(request.current_password, request.new_password)
        .await?;
    Ok(Json(
        serde_json::json!({ "changed": true, "api_keys_revoked": true }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_format_hash_and_allowlist() {
        let token = format!("{PREFIX}{}", "ab".repeat(32));
        assert!(hash_token(&token).is_some());
        assert!(hash_token("nibe_short").is_none());
        assert!(hash_token(&"ab".repeat(32)).is_none(), "prefix required");
        assert!(key_allows(ApiScope::Read, &Method::GET, "/api/v1/stream"));
        assert!(!key_allows(
            ApiScope::Read,
            &Method::POST,
            "/api/v1/controls/heating-offset"
        ));
        assert!(key_allows(
            ApiScope::Control,
            &Method::POST,
            "/api/v1/controls/alarm-reset"
        ));
        assert!(key_allows(
            ApiScope::Control,
            &Method::POST,
            "/api/v1/controls/register-write"
        ));
        assert!(!key_allows(
            ApiScope::Read,
            &Method::POST,
            "/api/v1/controls/register-write"
        ));
        for method in [Method::PUT, Method::POST, Method::DELETE] {
            for scope in [ApiScope::Read, ApiScope::Control] {
                assert!(
                    !key_allows(scope, &method, "/api/v1/registers/notes"),
                    "keys never write register notes"
                );
            }
        }
        assert!(key_allows(
            ApiScope::Read,
            &Method::GET,
            "/api/v1/registers/explorer"
        ));
        for path in [
            "/api/v1/access/api-keys",
            "/api/v1/network/connect",
            "/api/v1/setup/factory-reset",
        ] {
            assert!(
                !key_allows(ApiScope::Control, &Method::POST, path),
                "{path}"
            );
            assert!(!key_allows(ApiScope::Control, &Method::GET, path), "{path}");
        }
    }
}
