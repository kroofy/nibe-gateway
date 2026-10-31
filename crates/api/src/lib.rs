//! Versioned HTTP presentation and local admin/provisioning boundary.
pub mod access;
pub mod audit;
pub mod auth;
pub mod f730;
pub mod history;
pub mod integrations;
pub mod lease;
pub mod live;
pub mod maintenance;
pub mod notes;
pub mod provisioning;
pub mod registers;
pub mod releases;
pub mod system;
use axum::{
    extract::{rejection::JsonRejection, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Response, Sse,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::stream;
use nibe_core::*;
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, sync::Arc, time::Duration};
use utoipa::{OpenApi, ToSchema};

#[derive(Debug, Clone, Copy)]
pub enum Capability {
    Read,
    Discover,
    Control,
    Debug,
}
/// Injected capability policy. Production uses `auth::Auth` via the provisioning router.
/// Implementations must not log credentials or block the async executor.
pub trait Authorizer: Send + Sync {
    fn authorize(&self, headers: &HeaderMap, capability: Capability) -> Result<(), ServiceError>;
    /// Who an authorized request acts for, shown as a lease holder (ADR 0123). Never a credential.
    fn principal(&self, _headers: &HeaderMap) -> String {
        "client".into()
    }
}
#[derive(Clone)]
pub struct ApiState {
    pub gateway: Gateway,
    pub authorizer: Arc<dyn Authorizer>,
    /// Heating-offset control lease (ADR 0123); `None` without a state store, then leased writes are refused.
    pub leases: Option<Arc<lease::Leases>>,
}

static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
/// Initialize before radio/device setup so uptime includes startup work.
pub fn record_process_start() {
    STARTED.get_or_init(std::time::Instant::now);
}
static PROCESS_INSTANCE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| nibe_persistence::random_token(16));
/// Cache policy for responses that did not set one: content-hashed build assets are immutable, every
/// other document (index.html, the SPA fallback, favicon) is revalidated so a new release shows at once.
fn cache_policy(path: &str) -> &'static str {
    if path.starts_with("/assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}
pub async fn security_headers(request: Request, next: Next) -> Response {
    let cache = cache_policy(request.uri().path());
    let mut response = next.run(request).await;
    if !response.headers().contains_key("cache-control") {
        response
            .headers_mut()
            .insert("cache-control", cache.parse().unwrap());
    }
    for (name, value) in [
        ("content-security-policy", "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"),
        ("x-frame-options", "DENY"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
    ] { response.headers_mut().insert(name, value.parse().unwrap()); }
    response
}
pub fn router(state: ApiState) -> Router {
    STARTED.get_or_init(std::time::Instant::now);
    let routes = Router::new()
        .route("/api/v1/status", get(status))
        .route("/api/v1/device", get(device))
        .route("/api/v1/device/identity", get(identity))
        .route("/api/v1/measurements", get(measurements))
        .route("/api/v1/registers", get(registers))
        .route("/api/v1/registers/catalog", get(catalog))
        .route("/api/v1/registers/discovered", get(discovered))
        .route("/api/v1/diagnostics", get(diagnostics))
        .route("/api/v1/events", get(events))
        .route("/api/v1/stream", get(live::stream))
        .route("/api/v1/discovery/scan", post(scan))
        .route("/api/v1/discovery/status", get(discovery_status))
        .route("/api/v1/controls/temporary-lux", post(temporary_lux))
        .route("/api/v1/controls/heating-offset", post(heating_offset))
        .route("/api/v1/controls/alarm-reset", post(alarm_reset))
        .route("/api/v1/controls/register-write", post(register_write))
        .route("/api/v1/controls/register-read", post(register_read))
        .route("/api/v1/controls/register-poll", post(register_poll))
        .route("/api/v1/registers/polls", get(register_polls))
        .route("/openapi.json", get(openapi))
        .route("/health", get(health));
    #[cfg(feature = "dev")]
    let routes = routes
        .route("/__dev/scenario", post(dev_scenario))
        .route("/__dev/time/advance", post(dev_advance))
        .route("/__dev/time/freeze", post(dev_freeze))
        .route("/__dev/fault", post(dev_fault))
        .route("/__dev/reset", post(dev_reset))
        .route("/__dev/commands", get(dev_commands));
    routes
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn_with_state(state.clone(), authorize))
        .with_state(state)
}
async fn authorize(
    State(state): State<ApiState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let path = request.uri().path();
    let capability = if cfg!(feature = "dev") && path.starts_with("/__dev/") {
        Capability::Debug
    } else if path.starts_with("/api/v1/controls/") {
        Capability::Control
    } else if path == "/api/v1/discovery/scan" {
        Capability::Discover
    } else {
        Capability::Read
    };
    if path != "/health" && path != "/openapi.json" {
        state.authorizer.authorize(request.headers(), capability)?;
    }
    Ok(next.run(request).await)
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorBody {
    pub error: ServiceError,
}
pub struct ApiError(pub ServiceError);
impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        Self(error)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0.code {
            ErrorCode::Validation => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::ControlsDisabled | ErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ErrorCode::Busy => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::Timeout => StatusCode::GATEWAY_TIMEOUT,
            ErrorCode::Disconnected | ErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::Malformed => StatusCode::BAD_GATEWAY,
            ErrorCode::Rejected => StatusCode::CONFLICT,
            ErrorCode::Unsupported => StatusCode::NOT_IMPLEMENTED,
        };
        (status, Json(ApiErrorBody { error: self.0 })).into_response()
    }
}
fn body<T>(json: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    json.map(|Json(value)| value).map_err(|_| {
        ServiceError::validation("invalid JSON body, content type, or field types").into()
    })
}
async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ApiErrorBody {
            error: ServiceError::new(ErrorCode::Unsupported, "route not found"),
        }),
    )
}
async fn method_not_allowed() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(ApiErrorBody {
            error: ServiceError::new(ErrorCode::Unsupported, "method not allowed"),
        }),
    )
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Status {
    pub uptime_s: u64,
    pub ip_addresses: Vec<String>,
    pub mode: DeviceMode,
    pub connection: ConnectionState,
    pub network: NetworkState,
    pub stale: bool,
    pub now_ms: u64,
    pub last_update_ms: Option<u64>,
    pub stale_after_ms: u64,
    pub controls_enabled: bool,
    pub last_error: Option<String>,
    /// Heating-offset control lease (ADR 0123); absent where leases are unsupported.
    pub heating_offset_lease: Option<lease::LeaseStatus>,
    /// Every control lease by register, the heating offset included (ADR 0123); absent like the above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register_leases: Option<lease::RegisterLeaseStatus>,
}
#[utoipa::path(get, path = "/api/v1/status", responses((status = 200, body = Status)))]
async fn status(
    State(state): State<ApiState>,
    network: Option<axum::Extension<Arc<nibe_network::Provisioner>>>,
) -> Json<Status> {
    let ip_addresses = match network {
        Some(network) => network.status().await.ip_addresses,
        None => vec![],
    };
    let s = state.gateway.snapshot();
    Json(Status {
        uptime_s: STARTED
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs(),
        ip_addresses,
        mode: s.mode,
        connection: s.connection,
        network: s.network,
        stale: s.stale,
        now_ms: s.now_ms,
        last_update_ms: s.last_update_ms,
        stale_after_ms: s.stale_after_ms,
        controls_enabled: s.controls_enabled,
        last_error: s.last_error,
        heating_offset_lease: state.leases.as_ref().map(|l| l.status()),
        register_leases: state.leases.as_ref().map(|l| l.register_status()),
    })
}
#[utoipa::path(get, path = "/api/v1/device", responses((status = 200, body = DeviceSnapshot)))]
async fn device(State(state): State<ApiState>) -> Json<DeviceSnapshot> {
    Json(state.gateway.snapshot())
}
#[utoipa::path(get, path = "/api/v1/device/identity", responses((status = 200, body = Option<DeviceIdentity>)))]
async fn identity(State(state): State<ApiState>) -> Json<Option<DeviceIdentity>> {
    Json(state.gateway.snapshot().identity)
}
#[utoipa::path(get, path = "/api/v1/measurements", responses((status = 200, body = [Measurement])))]
async fn measurements(State(state): State<ApiState>) -> Json<Vec<Measurement>> {
    Json(state.gateway.snapshot().measurements)
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterList {
    pub items: Vec<RegisterReading>,
    pub now_ms: u64,
    pub stale_after_ms: u64,
    pub stale: bool,
}
fn register_list(snapshot: DeviceSnapshot, observed_only: bool) -> RegisterList {
    RegisterList {
        items: snapshot
            .registers
            .into_iter()
            .filter(|r| {
                !observed_only
                    || matches!(
                        r.observation,
                        ObservationState::Observed | ObservationState::ConfirmedReadable
                    )
            })
            .collect(),
        now_ms: snapshot.now_ms,
        stale_after_ms: snapshot.stale_after_ms,
        stale: snapshot.stale,
    }
}
#[utoipa::path(get, path = "/api/v1/registers", responses((status = 200, body = RegisterList)))]
async fn registers(State(state): State<ApiState>) -> Json<RegisterList> {
    Json(register_list(state.gateway.snapshot(), false))
}
#[utoipa::path(get, path = "/api/v1/registers/catalog", responses((status = 200, body = [RegisterDefinition])))]
async fn catalog(State(state): State<ApiState>) -> Json<Vec<RegisterDefinition>> {
    Json(state.gateway.snapshot().catalog)
}
#[utoipa::path(get, path = "/api/v1/registers/discovered", responses((status = 200, body = RegisterList)))]
async fn discovered(State(state): State<ApiState>) -> Json<RegisterList> {
    Json(register_list(state.gateway.snapshot(), true))
}
#[utoipa::path(get, path = "/api/v1/diagnostics", responses((status = 200, body = Diagnostics)))]
async fn diagnostics(State(state): State<ApiState>) -> Json<Diagnostics> {
    Json(state.gateway.snapshot().diagnostics)
}
#[utoipa::path(get, path = "/api/v1/discovery/status", responses((status = 200, body = DiscoveryStatus)))]
async fn discovery_status(State(state): State<ApiState>) -> Json<DiscoveryStatus> {
    Json(state.gateway.snapshot().discovery)
}
#[utoipa::path(post, path = "/api/v1/discovery/scan", description = "Collect passive observations only. Active probing is unsupported until a verified adapter is installed.", responses((status = 200, body = DiscoveryStatus), (status = 429, body = ApiErrorBody), (status = 503, body = ApiErrorBody), (status = 504, body = ApiErrorBody)))]
async fn scan(State(state): State<ApiState>) -> Result<Json<DiscoveryStatus>, ApiError> {
    Ok(Json(state.gateway.discover().await?))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporaryLuxRequest {
    pub mode: TemporaryLux,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HeatingOffsetRequest {
    pub offset: i8,
    /// Optional control lease, 60..=3600 s (ADR 0123): unless the caller writes again with a lease before it
    /// lapses, the gateway restores the offset read back before the first leased write. A write without a
    /// lease ends any lease.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 60, maximum = 3600)]
    pub lease_seconds: Option<u32>,
}
#[utoipa::path(post, path = "/api/v1/controls/temporary-lux", request_body = TemporaryLuxRequest, description = "NIBE Temporary Lux (extra hot water): off, three_hours, six_hours, twelve_hours or one_time_increase. On hardware: 48132 (raw 0..=4), only with allow_hardware_writes and 48132 in writable_registers (ADR 0113, HP-32); the receipt means queued, confirm via the read-back. Never retry blindly.", responses((status = 200, body = CommandReceipt), (status = 403, body = ApiErrorBody), (status = 409, body = ApiErrorBody), (status = 422, body = ApiErrorBody), (status = 429, body = ApiErrorBody), (status = 503, body = ApiErrorBody), (status = 504, body = ApiErrorBody)))]
async fn temporary_lux(
    State(state): State<ApiState>,
    json: Result<Json<TemporaryLuxRequest>, JsonRejection>,
) -> Result<Json<CommandReceipt>, ApiError> {
    let request = body(json)?;
    Ok(Json(
        state
            .gateway
            .command(Command::TemporaryLux { mode: request.mode })
            .await?,
    ))
}
#[utoipa::path(post, path = "/api/v1/controls/alarm-reset", description = "Acknowledge the pump's current alarm (writes 45171=1, then reads back 45001). Hardware needs allow_alarm_reset (ADR 0105). The receipt means queued.", responses((status = 200, body = CommandReceipt), (status = 403, body = ApiErrorBody), (status = 409, body = ApiErrorBody), (status = 429, body = ApiErrorBody), (status = 503, body = ApiErrorBody), (status = 504, body = ApiErrorBody)))]
async fn alarm_reset(State(state): State<ApiState>) -> Result<Json<CommandReceipt>, ApiError> {
    Ok(Json(state.gateway.command(Command::AlarmReset).await?))
}
#[utoipa::path(post, path = "/api/v1/controls/heating-offset", request_body = HeatingOffsetRequest, description = "Heat offset -10..10. On hardware: 47011, only with allow_hardware_writes and 47011 in writable_registers (ADR 0125); the receipt means queued, confirm via the read-back. Never retry blindly. With lease_seconds the write holds a control lease (ADR 0123): renew by writing again with a lease before it lapses, or the gateway restores the baseline; lease state is in /api/v1/status. A leased write needs a fresh read-back of the offset (503 otherwise).", responses((status = 200, body = CommandReceipt), (status = 403, body = ApiErrorBody), (status = 409, body = ApiErrorBody), (status = 422, body = ApiErrorBody), (status = 429, body = ApiErrorBody), (status = 501, body = ApiErrorBody), (status = 503, body = ApiErrorBody), (status = 504, body = ApiErrorBody)))]
async fn heating_offset(
    State(state): State<ApiState>,
    headers: HeaderMap,
    json: Result<Json<HeatingOffsetRequest>, JsonRejection>,
) -> Result<Json<CommandReceipt>, ApiError> {
    let request = body(json)?;
    let receipt = match (&state.leases, request.lease_seconds) {
        (Some(leases), _) => {
            leases
                .write_offset(
                    request.offset,
                    request.lease_seconds,
                    state.authorizer.principal(&headers),
                )
                .await?
        }
        (None, Some(_)) => {
            return Err(ServiceError::new(
                ErrorCode::Unsupported,
                "control leases need the gateway state store",
            )
            .into())
        }
        (None, None) => {
            state
                .gateway
                .command(Command::HeatingOffset {
                    offset: request.offset,
                })
                .await?
        }
    };
    Ok(Json(receipt))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterWriteRequest {
    pub register: u16,
    /// Raw value as sent on the bus. Give exactly one of `raw` and `value`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<i32>,
    /// Scaled value (for example 47.0 °C): multiplied by the catalog scale and rounded. Needs a catalog entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Optional lease, 60..=3600 s, for 47011 (ADR 0123) and the setpoints 47045, 47049 and 47375
    /// (ADR 0123): unless the caller writes again with a lease before it lapses, the gateway restores the
    /// value read from the pump before the first leased write. A write without a lease ends any lease.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 60, maximum = 3600)]
    pub lease_seconds: Option<u32>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterWriteReceipt {
    pub register: u16,
    pub raw: i32,
    pub detail: String,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterReadRequest {
    pub register: u16,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterReadReceipt {
    pub register: u16,
    pub reading: RegisterReading,
}
type RegisterOps = Option<axum::Extension<Arc<registers::Registers>>>;
fn register_ops(ops: RegisterOps) -> Result<Arc<registers::Registers>, ApiError> {
    ops.map(|axum::Extension(r)| r).ok_or_else(|| {
        ServiceError::new(
            ErrorCode::Unsupported,
            "register operations need --device hardware or replay",
        )
        .into()
    })
}
#[utoipa::path(post, path = "/api/v1/controls/register-write", request_body = RegisterWriteRequest, description = "Write any register number with any value (ADR 0125): no allowlist and no catalog range, the caller decides. Hardware only, on unless the gateway runs with --no-open-writes. One write at a time, sent once, never retried, then read back: the receipt means queued, confirm with probe.writes.last and the register's reading. 47011 goes through the heating-offset control (-10..10) and its lease (ADR 0123). lease_seconds is accepted for 47011, 47045, 47049 and 47375 (ADR 0123): renew by writing again with a lease, or the gateway restores the value read before the first leased write; a write without a lease ends the lease. A first leased write reads the register first (503 when the pump does not answer or a write to it is still in flight). Lease state is in /api/v1/status. MQTT equivalent: <base>/register/<id>/set and /set_raw, with a number or {\"value\": n, \"lease_seconds\": s} (ADR 0125).", responses((status = 200, body = RegisterWriteReceipt), (status = 403, body = ApiErrorBody), (status = 422, body = ApiErrorBody), (status = 429, body = ApiErrorBody), (status = 501, body = ApiErrorBody), (status = 503, body = ApiErrorBody)))]
async fn register_write(
    State(state): State<ApiState>,
    headers: HeaderMap,
    ops: RegisterOps,
    json: Result<Json<RegisterWriteRequest>, JsonRejection>,
) -> Result<Json<RegisterWriteReceipt>, ApiError> {
    let request = body(json)?;
    let value = match (request.raw, request.value) {
        (Some(raw), None) => registers::WriteValue::Raw(raw),
        (None, Some(value)) => registers::WriteValue::Scaled(value),
        _ => return Err(ServiceError::validation("give exactly one of raw and value").into()),
    };
    let (raw, detail) = register_ops(ops)?
        .write(
            request.register,
            value,
            request.lease_seconds,
            state.authorizer.principal(&headers),
        )
        .await?;
    Ok(Json(RegisterWriteReceipt {
        register: request.register,
        raw,
        detail,
    }))
}
#[utoipa::path(post, path = "/api/v1/controls/register-read", request_body = RegisterReadRequest, description = "Read any register number now, beside the polls, and return the pump's answer (ADR 0125). Waits up to 20 s (504 when the pump never answers, e.g. an unsupported register). Registers that are not polled do not appear in /api/v1/registers or history. Hardware needs --allow-hardware-reads. MQTT equivalent: <base>/register/<id>/get.", responses((status = 200, body = RegisterReadReceipt), (status = 403, body = ApiErrorBody), (status = 422, body = ApiErrorBody), (status = 501, body = ApiErrorBody), (status = 503, body = ApiErrorBody), (status = 504, body = ApiErrorBody)))]
async fn register_read(
    ops: RegisterOps,
    json: Result<Json<RegisterReadRequest>, JsonRejection>,
) -> Result<Json<RegisterReadReceipt>, ApiError> {
    let request = body(json)?;
    let reading = register_ops(ops)?.read(request.register).await?;
    Ok(Json(RegisterReadReceipt {
        register: request.register,
        reading,
    }))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterPollRequest {
    pub register: u16,
    pub mode: nibe_persistence::PollMode,
}
#[utoipa::path(post, path = "/api/v1/controls/register-poll", request_body = RegisterPollRequest, description = "Poll a catalog register fast (every round), slow (about every slow interval) or not at all (ADR 0125). Applied at once and saved in the gateway settings file, overriding the configuration for that register; choosing the configured mode removes the override. At most 64 polled registers. Polled registers appear in /api/v1/registers, history and MQTT (Home Assistant discovers them). MQTT equivalent: <base>/register/<id>/poll with fast, slow or off.", responses((status = 200, body = registers::PollLists), (status = 403, body = ApiErrorBody), (status = 422, body = ApiErrorBody), (status = 501, body = ApiErrorBody), (status = 503, body = ApiErrorBody)))]
async fn register_poll(
    ops: RegisterOps,
    json: Result<Json<RegisterPollRequest>, JsonRejection>,
) -> Result<Json<registers::PollLists>, ApiError> {
    let request = body(json)?;
    Ok(Json(
        register_ops(ops)?
            .set_poll(request.register, request.mode)
            .await?,
    ))
}
#[utoipa::path(get, path = "/api/v1/registers/polls", description = "The poll lists in effect and the per-register choices saved in the settings file (ADR 0125).", responses((status = 200, body = registers::PollLists), (status = 501, body = ApiErrorBody)))]
async fn register_polls(ops: RegisterOps) -> Result<Json<registers::PollLists>, ApiError> {
    Ok(Json(register_ops(ops)?.polls()))
}

#[utoipa::path(get, path = "/api/v1/events", description = "Bounded live SSE stream, not an event log. Fetch /api/v1/device on connect/reconnect and on resync; event IDs are process-local. Slow consumers lose old notifications without blocking the device.", responses((status = 200, description = "JSON GatewayEvent notifications; resync after lag", content_type = "text/event-stream", body = GatewayEvent)))]
async fn events(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.gateway.subscribe();
    let shutdown = state.gateway.shutdown_receiver();
    let events = stream::unfold(
        (receiver, state.gateway, shutdown, state.authorizer, headers),
        |(mut receiver, gateway, mut shutdown, authorizer, headers)| async move {
            if *shutdown.borrow() || authorizer.authorize(&headers, Capability::Read).is_err() {
                return None;
            }
            let received = loop {
                tokio::select! {
                    _ = shutdown.changed() => return None,
                    received = receiver.recv() => break received,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        if authorizer.authorize(&headers, Capability::Read).is_err() { return None; }
                    }
                }
            };
            // Logout/reset may have happened while awaiting the next notification.
            if authorizer.authorize(&headers, Capability::Read).is_err() {
                return None;
            }
            let event = match received {
                Ok(event) => Event::default()
                    .event(&event.kind)
                    .id(event.sequence.to_string())
                    .json_data(event)
                    .expect("event JSON"),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    gateway.record_event_lag();
                    Event::default().event("resync").data(format!(
                        "{{\"missed\":{count},\"refetch\":\"/api/v1/device\"}}"
                    ))
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            };
            Some((
                Ok(event),
                (receiver, gateway, shutdown, authorizer, headers),
            ))
        },
    );
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Health {
    pub process_instance: String,
    pub status: String,
    pub ready: bool,
    pub version: String,
    pub release_id: String,
    pub device_degraded: bool,
    pub device_fatal: bool,
    pub wire_recovering: bool,
    pub last_frame_age_ms: Option<u64>,
    pub wire_reopen_attempts: u64,
    pub wire_priority_applied: bool,
    /// Complete, error-free passes over the poll list since start; update probation can end early
    /// after one (ADR 0070). Absent without a hardware/replay adapter.
    pub poll_rounds: Option<u64>,
}
#[utoipa::path(get, path = "/health", security(()), description = "Application readiness: core heartbeat is recent on monotonic time and its worker is running. Wire recovery/failure makes readiness fail; physical frame silence is degraded and reported as last_frame_age_ms. OTA also matches version and release_id.", responses((status = 200, body = Health)))]
async fn health(
    State(state): State<ApiState>,
    wire: Option<axum::Extension<nibe_device_f730::F730Handle>>,
) -> (StatusCode, Json<Health>) {
    let snapshot = state.gateway.snapshot();
    let wire = wire.map(|axum::Extension(handle)| handle.wire_health());
    let ready = state.gateway.is_ready()
        && !wire
            .as_ref()
            .is_some_and(|w| w.device_fatal || w.recovering);
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(Health {
            process_instance: PROCESS_INSTANCE.clone(),
            status: if ready { "ok" } else { "starting" }.into(),
            ready,
            version: env!("CARGO_PKG_VERSION").into(),
            release_id: std::env::var("NIBE_RELEASE_ID").unwrap_or_else(|_| "development".into()),
            device_degraded: snapshot.stale || wire.as_ref().is_some_and(|w| w.device_degraded),
            device_fatal: wire.as_ref().is_some_and(|w| w.device_fatal),
            wire_recovering: wire.as_ref().is_some_and(|w| w.recovering),
            last_frame_age_ms: wire.as_ref().and_then(|w| w.last_frame_age_ms),
            wire_reopen_attempts: wire.as_ref().map_or(0, |w| w.reopen_attempts),
            wire_priority_applied: wire.as_ref().is_some_and(|w| w.priority_applied),
            poll_rounds: wire.as_ref().map(|w| w.poll_rounds),
        }),
    )
}
#[utoipa::path(get, path = "/openapi.json", security(()), responses((status = 200, description = "OpenAPI 3.1 document")))]
async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(api_document())
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "NIBE Gateway API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Authenticated local API. Cookie sessions and initial setup proof are documented in WS06. POST requests require X-Requested-With: nibe-gateway. Synthetic register IDs are not F730 addresses."
    ),
    paths(
        status,
        device,
        identity,
        measurements,
        registers,
        catalog,
        discovered,
        diagnostics,
        discovery_status,
        scan,
        temporary_lux,
        heating_offset,
        register_write,
        register_read,
        register_poll,
        register_polls,
        alarm_reset,
        events,
        health,
        openapi
    ),
    components(schemas(ApiErrorBody, ServiceError, GatewayEvent))
)]
struct ApiDocument;
pub fn api_document() -> utoipa::openapi::OpenApi {
    #[allow(unused_mut)]
    let mut document = ApiDocument::openapi();
    document.merge(f730::F730Document::openapi());
    #[cfg(feature = "dev")]
    document.merge(dev::DevDocument::openapi());
    document.merge(provisioning::ProvisioningDocument::openapi());
    document.merge(maintenance::MaintenanceDocument::openapi());
    document.merge(audit::AuditDocument::openapi());
    use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityRequirement, SecurityScheme};
    let components = document.components.as_mut().expect("API components");
    components.add_security_scheme(
        "admin_session",
        SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("nibe_session"))),
    );
    components.add_security_scheme(
        "setup_token",
        SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-Setup-Token"))),
    );
    document.security = Some(vec![SecurityRequirement::new(
        "admin_session",
        Vec::<String>::new(),
    )]);
    document
}

#[cfg(feature = "dev")]
mod dev;
#[cfg(feature = "dev")]
use dev::*;
