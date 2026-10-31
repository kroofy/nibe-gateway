//! MQTT settings and status (ADR 0115). Admin session only; API keys never reach these routes.
//! The broker password is write-only: it is stored but never returned.
use crate::{
    auth::{json_body, AuthError},
    provisioning::ProvisioningState,
};
use axum::{extract::State, Json};
use nibe_persistence::MqttSettings;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct MqttView {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password_set: bool,
    pub topic_prefix: String,
    pub discovery: bool,
    pub discovery_prefix: String,
    pub allow_commands: bool,
    /// Controls the gateway configuration enables; only these accept MQTT commands.
    pub available_commands: Vec<String>,
    pub status: MqttStatus,
    /// Base topic for this gateway, e.g. `nibe/nibe_a1b2c3d4`.
    pub base_topic: String,
}
#[derive(Serialize, ToSchema)]
pub struct MqttStatus {
    /// disabled, connecting, connected or error.
    pub state: String,
    pub broker: Option<String>,
    pub last_error: Option<String>,
    pub connected_since_ms: Option<u64>,
    pub messages_published: u64,
    pub commands_received: u64,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MqttUpdate {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    /// Omit or null to keep the stored password; empty string clears it.
    pub password: Option<String>,
    pub topic_prefix: String,
    pub discovery: bool,
    pub discovery_prefix: String,
    pub allow_commands: bool,
}

fn view(state: &ProvisioningState, bridge: &nibe_mqtt::Bridge) -> MqttView {
    let settings = state.auth.store().config().mqtt.unwrap_or_default();
    let status = bridge.status();
    let controls = bridge.controls();
    let available_commands = [
        ("heating_offset", controls.heating_offset),
        ("temporary_lux", controls.temporary_lux),
        ("alarm_reset", controls.alarm_reset),
    ]
    .into_iter()
    .filter(|(_, on)| *on)
    .map(|(name, _)| name.to_string())
    .collect();
    MqttView {
        base_topic: format!("{}/{}", settings.topic_prefix, status.device_id),
        enabled: settings.enabled,
        host: settings.host,
        port: settings.port,
        username: settings.username,
        password_set: settings.password.as_deref().is_some_and(|p| !p.is_empty()),
        topic_prefix: settings.topic_prefix,
        discovery: settings.discovery,
        discovery_prefix: settings.discovery_prefix,
        allow_commands: settings.allow_commands,
        available_commands,
        status: MqttStatus {
            state: serde_json::to_value(&status.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            broker: status.broker,
            last_error: status.last_error,
            connected_since_ms: status.connected_since_ms,
            messages_published: status.messages_published,
            commands_received: status.commands_received,
        },
    }
}

fn bridge(state: &ProvisioningState) -> Result<&nibe_mqtt::Bridge, AuthError> {
    state.mqtt.as_ref().ok_or_else(AuthError::unavailable)
}

#[utoipa::path(get, path = "/api/v1/integrations/mqtt", description = "Admin session only. MQTT settings (password never returned) and connection status (ADR 0115).", responses((status = 200, body = MqttView)))]
pub async fn get_mqtt(State(state): State<ProvisioningState>) -> Result<Json<MqttView>, AuthError> {
    let bridge = bridge(&state)?;
    Ok(Json(view(&state, bridge)))
}

#[utoipa::path(post, path = "/api/v1/integrations/mqtt", request_body = MqttUpdate, description = "Admin session only. Saves MQTT settings to the persistent config and reconnects (ADR 0115).", responses((status = 200, body = MqttView), (status = 422)))]
pub async fn set_mqtt(
    State(state): State<ProvisioningState>,
    json: Result<Json<MqttUpdate>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<MqttView>, AuthError> {
    let request = json_body(json)?;
    let bridge = bridge(&state)?.clone();
    let store = state.auth.store();
    let previous = store.config().mqtt.unwrap_or_default();
    let blank = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let settings = MqttSettings {
        enabled: request.enabled,
        host: request.host.trim().to_string(),
        port: request.port,
        username: blank(request.username),
        password: match request.password {
            None => previous.password,
            Some(p) if p.is_empty() => None,
            Some(p) => Some(p),
        },
        topic_prefix: request.topic_prefix.trim().to_string(),
        discovery: request.discovery,
        discovery_prefix: request.discovery_prefix.trim().to_string(),
        allow_commands: request.allow_commands,
    };
    settings.validate().map_err(|message| {
        AuthError::new(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "validation",
            message,
        )
    })?;
    let saved = settings.clone();
    tokio::task::spawn_blocking(move || store.update(|config| config.mqtt = Some(saved)))
        .await
        .map_err(|_| AuthError::unavailable())?
        .map_err(|_| AuthError::unavailable())?;
    bridge.apply(Some(settings));
    Ok(Json(view(&state, &bridge)))
}
