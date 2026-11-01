//! Audit log: who changed what, through HTTP (admin UI and API keys), MQTT, the gateway itself
//! (lease restores) and what the pump answered to each register write. The HTTP side is one middleware over
//! the whole app, so a new change route is recorded without extra code; secrets in request bodies are removed
//! before anything is stored. Reads, logins and refused (unauthenticated) requests are not changes and are
//! not recorded.
use crate::{auth::Auth, Authorizer};
use axum::{
    body::{to_bytes, Body},
    extract::{Query, Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use nibe_persistence::audit::{AuditEntry, AuditLog, AuditSource, NewAuditEntry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use utoipa::{IntoParams, OpenApi, ToSchema};

/// Bodies larger than this (update uploads, register notes) are recorded without their content.
const MAX_CAPTURED_BODY: usize = 16 * 1024;
/// Error bodies are short JSON; anything bigger is not read for its message.
const MAX_ERROR_BODY: usize = 8 * 1024;
const DEFAULT_PAGE: usize = 100;
const MAX_PAGE: usize = 500;

/// POST routes that change nothing: reads, session handling and update checks.
const NOT_CHANGES: [&str; 7] = [
    "/api/v1/auth/login",
    "/api/v1/auth/logout",
    "/api/v1/controls/register-read",
    "/api/v1/updates/remote/check",
    "/api/v1/discovery/scan",
    "/api/v1/discovery/probe",
    "/api/v1/discovery/probe/cancel",
];

/// The action recorded for a change route; unknown routes are recorded as method and path.
fn action(method: &Method, path: &str) -> String {
    let known = match path {
        "/api/v1/controls/register-write" => "Register write",
        "/api/v1/controls/register-poll" => "Poll choice",
        "/api/v1/controls/heating-offset" => "Heating offset",
        "/api/v1/controls/temporary-lux" => "Temporary lux",
        "/api/v1/controls/alarm-reset" => "Alarm reset",
        "/api/v1/registers/notes" => "Register notes saved",
        "/api/v1/registers/catalog/import" => "Register catalog imported",
        "/api/v1/integrations/mqtt" => "MQTT settings changed",
        "/api/v1/setup/admin" => "Admin account created",
        "/api/v1/setup/factory-reset" => "Factory reset",
        "/api/v1/network/connect" => "Wi-Fi network changed",
        "/api/v1/network/recovery" => "Wi-Fi recovery started",
        "/api/v1/access/api-keys" => "API key created",
        "/api/v1/access/password" => "Admin password changed",
        "/api/v1/access/ssh" => "SSH access changed",
        "/api/v1/access/ssh/add" => "SSH key added",
        "/api/v1/access/ssh/remove" => "SSH key removed",
        "/api/v1/access/tailscale/join" => "Tailscale joined",
        "/api/v1/access/tailscale/disconnect" => "Tailscale disconnected",
        "/api/v1/diagnostics/developer" => "Developer mode changed",
        "/api/v1/updates/upload" => "Update uploaded",
        "/api/v1/updates/remote/stage" => "Update downloaded",
        "/api/v1/updates/activate" => "Update activated",
        "/api/v1/updates/rollback" => "Update rolled back",
        "/api/v1/system/restart" => "Gateway restarted",
        _ if method == Method::DELETE && path.starts_with("/api/v1/access/api-keys/") => {
            "API key revoked"
        }
        _ => return format!("{method} {path}"),
    };
    known.into()
}

fn is_change(method: &Method, path: &str) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
        && (path.starts_with("/api/") || path.starts_with("/__dev/"))
        && !NOT_CHANGES.contains(&path)
}

/// True for JSON keys whose values must never be stored (passwords, passphrases, tokens, keys).
fn secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "password",
        "passphrase",
        "secret",
        "token",
        "key",
        "psk",
        "auth",
    ]
    .iter()
    .any(|word| key.contains(word))
}
fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if secret_key(key) && !value.is_null() {
                    *value = Value::String("(hidden)".into());
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}
/// The request body as stored: JSON with secrets hidden, or nothing for other bodies.
fn summarize(body: &[u8]) -> (Option<String>, Option<String>) {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return (None, None);
    };
    redact(&mut value);
    let target = value
        .get("register")
        .filter(|v| v.is_number() || v.is_string())
        .map(|v| v.to_string().trim_matches('"').to_string());
    let detail = match &value {
        Value::Object(map) if map.is_empty() => None,
        Value::Null => None,
        _ => Some(value.to_string()),
    };
    (target, detail)
}

#[derive(Clone)]
pub struct AuditLayer {
    pub log: Arc<AuditLog>,
    pub auth: Arc<Auth>,
}

/// Records every change request that reached a handler, with its outcome. Layer it over the whole app.
pub async fn record(State(layer): State<AuditLayer>, request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    if !is_change(&method, &path) {
        return next.run(request).await;
    }
    let actor = layer.auth.principal(request.headers());
    let length = request
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    let (request, target, detail) = match length {
        Some(length) if length > 0 && length <= MAX_CAPTURED_BODY => {
            let (parts, body) = request.into_parts();
            let Ok(bytes) = to_bytes(body, MAX_CAPTURED_BODY).await else {
                // The body could not be read; the handler would have failed the same way.
                return StatusCode::BAD_REQUEST.into_response();
            };
            let (target, detail) = summarize(&bytes);
            (
                Request::from_parts(parts, Body::from(bytes)),
                target,
                detail,
            )
        }
        Some(length) if length > MAX_CAPTURED_BODY => (
            request,
            None,
            Some(format!("({length} bytes, not recorded)")),
        ),
        _ => (request, None, None),
    };
    let target = target.or_else(|| {
        path.strip_prefix("/api/v1/access/api-keys/")
            .map(str::to_string)
    });
    let response = next.run(request).await;
    let status = response.status();
    if matches!(
        status,
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
    ) {
        return response;
    }
    let (response, outcome) = if status.is_success() {
        (response, status.as_u16().to_string())
    } else {
        let (parts, body) = response.into_parts();
        let bytes = to_bytes(body, MAX_ERROR_BODY).await.unwrap_or_default();
        let error = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| v.get("error").cloned());
        let code = error.as_ref().and_then(|e| e["code"].as_str());
        let response = Response::from_parts(parts, Body::from(bytes.clone()));
        // Refused before any handler ran (no session, CSRF, unknown route): not a change, and recording it
        // would let anyone on the LAN push real entries out of the log.
        if status == StatusCode::UNAUTHORIZED || code == Some("forbidden") {
            return response;
        }
        let outcome = match error.as_ref().and_then(|e| e["message"].as_str()) {
            Some(message) => format!("{} {message}", status.as_u16()),
            None => status.as_u16().to_string(),
        };
        (response, outcome)
    };
    layer.log.record(NewAuditEntry {
        source: AuditSource::Http,
        actor,
        action: action(&method, &path),
        target,
        detail,
        ok: status.is_success(),
        outcome: Some(outcome),
    });
    response
}

/// Records the pump's answer to every register write, whoever asked (HTTP, MQTT, a lease restore or the
/// automatic alarm reset), with the value read back afterwards when one arrives within `READ_BACK_WAIT`.
/// When `stop` turns true, outcomes still waiting are recorded as they are and the task ends; await it before
/// flushing the log.
pub fn follow_pump_writes(
    handle: nibe_device_f730::F730Handle,
    log: Arc<AuditLog>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    use nibe_core::RegisterValue;
    use nibe_device_f730::writes::{WriteOutcome, WriteState, FINISHED_KEPT};
    const READ_BACK_WAIT: Duration = Duration::from_secs(10);
    struct Waiting {
        outcome: WriteOutcome,
        noticed: tokio::time::Instant,
        read_back: Option<RegisterValue>,
    }
    tokio::spawn(async move {
        // Writes finished before start were recorded before the restart (or never will be).
        let mut total = handle.snapshot().probe.writes.finished().0;
        // Finished writes, oldest first, until their read-back arrives or the wait ends.
        let mut waiting: Vec<Waiting> = Vec::new();
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        loop {
            let stopping = tokio::select! {
                _ = tick.tick() => false,
                _ = stop.wait_for(|stopped| *stopped) => true,
            };
            let snapshot = handle.snapshot();
            let (now_total, finished) = snapshot.probe.writes.finished();
            let new = usize::try_from(now_total.saturating_sub(total)).unwrap_or(usize::MAX);
            if new > FINISHED_KEPT {
                tracing::warn!(
                    missed = new - FINISHED_KEPT,
                    "audit log missed pump write outcomes"
                );
            }
            let noticed = tokio::time::Instant::now();
            waiting.extend(
                finished
                    .iter()
                    .skip(finished.len().saturating_sub(new))
                    .map(|o| Waiting {
                        outcome: o.clone(),
                        noticed,
                        read_back: None,
                    }),
            );
            total = now_total;
            // Each write keeps the first reading after it, even while an older one is still waiting.
            // The read-back can land in the same millisecond as the pump's answer.
            for w in waiting.iter_mut().filter(|w| w.read_back.is_none()) {
                w.read_back = snapshot
                    .readings
                    .get(&w.outcome.register)
                    .filter(|r| r.observed_at_ms >= w.outcome.updated_at_ms)
                    .and_then(|r| r.raw.clone());
            }
            // In order, so the log shows writes as the pump answered them.
            let ready = waiting
                .iter()
                .take_while(|w| {
                    stopping || w.read_back.is_some() || w.noticed.elapsed() >= READ_BACK_WAIT
                })
                .count();
            for w in waiting.drain(..ready) {
                let state = match w.outcome.state {
                    WriteState::Accepted => "accepted",
                    WriteState::Denied => "denied",
                    _ => "timeout (may or may not be applied)",
                };
                let read_back = match &w.read_back {
                    Some(value) => format!("; read back raw {}", nibe_mqtt::plain(value)),
                    None => "; no read-back".into(),
                };
                log.record(NewAuditEntry {
                    source: AuditSource::Pump,
                    actor: "pump".into(),
                    action: "Register write on the pump".into(),
                    target: Some(w.outcome.register.to_string()),
                    detail: Some(format!("raw {}", w.outcome.raw)),
                    ok: w.outcome.state == WriteState::Accepted,
                    outcome: Some(format!("{state}{read_back}")),
                });
            }
            if stopping {
                return;
            }
        }
    })
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    /// Only entries with a smaller `seq` (the next page).
    pub before: Option<u64>,
    /// At most this many entries (default 100, at most 500).
    pub limit: Option<usize>,
}
#[derive(Serialize, ToSchema)]
pub struct AuditPage {
    /// Newest first.
    pub entries: Vec<AuditEntry>,
    /// Older entries remain; ask again with `before` set to the last `seq`.
    pub more: bool,
}

#[utoipa::path(get, path = "/api/v1/audit", params(AuditQuery), description = "Admin session only. Changes to the gateway and the pump, newest first.", responses((status = 200, body = AuditPage)))]
pub async fn list(
    State(state): State<crate::provisioning::ProvisioningState>,
    Query(query): Query<AuditQuery>,
) -> Json<AuditPage> {
    let limit = query.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let (entries, more) = state
        .audit
        .as_ref()
        .map(|log| log.page(query.before, limit))
        .unwrap_or_default();
    Json(AuditPage { entries, more })
}

#[derive(OpenApi)]
#[openapi(paths(list), components(schemas(AuditPage, AuditEntry, AuditSource)))]
pub struct AuditDocument;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_are_hidden_and_the_register_is_the_target() {
        let (target, detail) = summarize(
            br#"{"register": 47011, "value": -2, "lease_seconds": 600, "password": "pw",
                "settings": {"host": "10.0.0.2", "mqtt_password": "x", "username": "ha"}, "ssid": "home",
                "api_key": "nibe_x"}"#,
        );
        assert_eq!(target.as_deref(), Some("47011"));
        let detail = detail.unwrap();
        assert!(
            !detail.contains("\"pw\"") && !detail.contains("nibe_x") && !detail.contains("\"x\"")
        );
        assert!(
            detail.contains("(hidden)") && detail.contains("10.0.0.2") && detail.contains("600")
        );
        assert_eq!(summarize(b"{}"), (None, None));
        assert_eq!(summarize(b"not json"), (None, None));
    }
    #[test]
    fn reads_and_sessions_are_not_changes() {
        assert!(is_change(&Method::POST, "/api/v1/controls/register-write"));
        assert!(is_change(&Method::PUT, "/api/v1/registers/notes"));
        assert!(is_change(&Method::DELETE, "/api/v1/access/api-keys/ab12"));
        assert!(!is_change(&Method::GET, "/api/v1/audit"));
        assert!(!is_change(&Method::POST, "/api/v1/auth/login"));
        assert!(!is_change(&Method::POST, "/api/v1/controls/register-read"));
        assert!(!is_change(&Method::POST, "/assets/x"));
        assert_eq!(
            action(&Method::DELETE, "/api/v1/access/api-keys/ab12"),
            "API key revoked"
        );
        assert_eq!(action(&Method::POST, "/api/v1/new"), "POST /api/v1/new");
    }
}
