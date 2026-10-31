//! Authenticated WebSocket pipe of live readings. Read-only: client messages are ignored.
use crate::{ApiState, Capability};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use nibe_core::{DeviceSnapshot, RegisterReading};
use serde_json::{json, Value};
use std::{collections::HashMap, time::Duration};

const STATUS_EVERY: Duration = Duration::from_secs(10);

/// Browsers do not apply CORS to WebSockets, so a cookie-authenticated upgrade must come from our
/// own origin. Clients without an Origin header (scripts, Home Assistant) are accepted.
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    let origin_host = origin
        .to_str()
        .ok()
        .and_then(|o| o.split_once("://"))
        .map(|(_, rest)| rest.trim_end_matches('/'));
    matches!((origin_host, host), (Some(o), Some(h)) if o.eq_ignore_ascii_case(h))
}

pub async fn stream(
    State(state): State<ApiState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !same_origin(&headers) {
        return (StatusCode::FORBIDDEN, "cross-origin WebSocket rejected").into_response();
    }
    upgrade.on_upgrade(move |socket| pipe(socket, state, headers))
}

fn reading_message(snapshot: &DeviceSnapshot, reading: &RegisterReading) -> Value {
    let definition = snapshot.catalog.iter().find(|d| d.id == reading.id);
    json!({
        "type": "reading",
        "id": reading.id,
        "name": definition.map(|d| d.name.as_str()),
        "title": definition.map(|d| d.description.as_str()),
        "unit": definition.and_then(|d| d.unit.as_deref()),
        "value": reading.decoded,
        "raw": reading.raw,
        "observed_at_ms": reading.observed_at_ms,
        "observation": reading.observation,
        "error": reading.error,
    })
}

fn status_message(snapshot: &DeviceSnapshot) -> Value {
    json!({
        "type": "status",
        "connection": snapshot.connection,
        "stale": snapshot.stale,
        "last_error": snapshot.last_error,
        "now_ms": snapshot.now_ms,
        "diagnostics": snapshot.diagnostics,
    })
}

/// Sends `hello` and every current reading, then each reading whose observation time changed.
async fn pipe(mut socket: WebSocket, state: ApiState, headers: HeaderMap) {
    let mut events = state.gateway.subscribe();
    let mut shutdown = state.gateway.shutdown_receiver();
    let mut sent: HashMap<_, u64> = HashMap::new();
    let mut status = tokio::time::interval(STATUS_EVERY);
    let authorized = || {
        state
            .authorizer
            .authorize(&headers, Capability::Read)
            .is_ok()
    };
    let snapshot = state.gateway.snapshot();
    let hello = json!({
        "type": "hello",
        "mode": snapshot.mode,
        "identity": snapshot.identity,
        "controls_enabled": snapshot.controls_enabled,
    });
    if socket.send(Message::text(hello.to_string())).await.is_err() {
        return;
    }
    let mut check = tokio::time::interval(Duration::from_secs(1));
    loop {
        let snapshot = state.gateway.snapshot();
        for reading in &snapshot.registers {
            if sent.get(&reading.id) == Some(&reading.observed_at_ms) {
                continue;
            }
            sent.insert(reading.id, reading.observed_at_ms);
            let message = reading_message(&snapshot, reading).to_string();
            if socket.send(Message::text(message)).await.is_err() {
                return;
            }
        }
        tokio::select! {
            _ = shutdown.changed() => break,
            event = events.recv() => {
                if let Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) = event {
                    state.gateway.record_event_lag();
                } else if event.is_err() {
                    break;
                }
            }
            _ = status.tick() => {
                let message = status_message(&state.gateway.snapshot()).to_string();
                if socket.send(Message::text(message)).await.is_err() { return; }
            }
            _ = check.tick() => {}
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            },
        }
        // Logout, password change or factory reset ends the pipe.
        if !authorized() {
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn headers(origin: Option<&str>, host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, host.parse().unwrap());
        if let Some(o) = origin {
            h.insert(header::ORIGIN, o.parse().unwrap());
        }
        h
    }
    #[test]
    fn origin_must_match_host_when_present() {
        assert!(same_origin(&headers(None, "nibe:8080")));
        assert!(same_origin(&headers(Some("http://nibe:8080"), "nibe:8080")));
        assert!(!same_origin(&headers(
            Some("https://evil.example"),
            "nibe:8080"
        )));
        assert!(!same_origin(&headers(Some("null"), "nibe:8080")));
    }
}
