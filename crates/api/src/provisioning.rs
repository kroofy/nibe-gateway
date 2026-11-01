use super::*;
use crate::auth::{check_csrf, json_body, Auth, AuthError, PasswordRequest, SessionStatus};
use nibe_network::{
    AccessPoint, NetworkError, Provisioner, State as NetworkState, Status as NetworkStatus,
};

#[derive(Clone)]
pub struct ProvisioningState {
    pub auth: Arc<Auth>,
    pub network: Arc<Provisioner>,
    pub identity: String,
    /// MQTT bridge; `None` where the binary runs without one (tests).
    pub mqtt: Option<nibe_mqtt::Bridge>,
    /// History store; `None` when the data directory has none (tests, read-only setups).
    pub history: Option<nibe_history::History>,
    /// Heating-offset control lease; `None` disables leased writes.
    pub leases: Option<Arc<crate::lease::Leases>>,
    /// Register write/read/poll shared with MQTT; `None` in simulated mode.
    pub registers: Option<Arc<crate::registers::Registers>>,
    /// Audit log behind `GET /api/v1/audit`; `None` returns an empty log.
    pub audit: Option<Arc<nibe_persistence::audit::AuditLog>>,
}
#[derive(Serialize, ToSchema)]
pub struct SetupStatus {
    pub setup_required: bool,
    /// Whether creating the administrator from *this* client needs the device setup token. False only
    /// while no admin exists and the request came over the open setup AP.
    pub setup_token_required: bool,
    pub network_state: NetworkState,
    pub setup_ssid: Option<String>,
    pub setup_url: String,
    pub simulated: bool,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectRequest {
    pub ssid: String,
    pub password: String,
}
#[derive(Serialize, ToSchema)]
pub struct ScanResponse {
    pub networks: Vec<AccessPoint>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResetRequest {
    pub confirmation: String,
    /// Keep recorded history; otherwise it is erased with the settings.
    #[serde(default)]
    pub keep_history: bool,
}
#[derive(Serialize, ToSchema)]
pub struct ResetResponse {
    pub reset: bool,
}
/// [`crate::auth::setup_ap_client`] for this request: socket peer from `ConnectInfo`, AP state from the
/// provisioner's last published status (no radio I/O on the request path).
fn on_setup_ap(network: &Provisioner, peer: Option<std::net::SocketAddr>) -> bool {
    let ap_up = matches!(
        network.cached_status().state,
        NetworkState::Ap | NetworkState::RecoveryAp
    );
    crate::auth::setup_ap_client(peer.map(|p| p.ip()), ap_up)
}
fn request_peer(request: &Request) -> Option<std::net::SocketAddr> {
    request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0)
}
impl From<NetworkError> for AuthError {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::InvalidCredentials => AuthError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation",
                "connection failed; check SSID/password and retry",
            ),
            _ => AuthError::unavailable(),
        }
    }
}
/// Production entry point: adds session/setup operations and authenticates the
/// complete existing core API. The legacy `router` is for policy-injection tests.
pub fn router(gateway: Gateway, state: ProvisioningState) -> Router {
    router_with_f730(gateway, state, None)
}
/// WS04 adapter routes share the complete production session/CSRF guard.
pub fn router_with_f730(
    gateway: Gateway,
    state: ProvisioningState,
    f730: Option<nibe_device_f730::F730Handle>,
) -> Router {
    let mode = gateway.snapshot().mode;
    let mut core = super::router(ApiState {
        gateway,
        authorizer: state.auth.clone(),
        leases: state.leases.clone(),
    });
    if let Some(handle) = f730 {
        core = core
            .merge(crate::f730::persisted_router(
                handle.clone(),
                state.auth.clone(),
                state.auth.store(),
                mode,
            ))
            .layer(axum::Extension(handle));
    }
    if let Some(registers) = state.registers.clone() {
        core = core.layer(axum::Extension(registers));
    }
    let setup = Router::new()
        .route("/api/v1/setup/status", get(setup_status))
        .route("/api/v1/setup/admin", post(setup_admin))
        .route("/api/v1/auth/session", get(session))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/network/status", get(network_status))
        .route("/api/v1/network/scan", get(network_scan))
        .route("/api/v1/network/connect", post(network_connect))
        .route("/api/v1/network/recovery", post(network_recovery))
        .route("/api/v1/setup/factory-reset", post(factory_reset))
        .route(
            "/api/v1/access/api-keys",
            get(crate::access::list).post(crate::access::create),
        )
        .route(
            "/api/v1/access/api-keys/{id}",
            axum::routing::delete(crate::access::revoke),
        )
        .route(
            "/api/v1/access/password",
            post(crate::access::change_password),
        )
        .route("/api/v1/audit", get(crate::audit::list))
        .route("/api/v1/history", get(crate::history::get_history))
        .route(
            "/api/v1/history/export.csv",
            get(crate::history::export_csv),
        )
        .route("/api/v1/history/status", get(crate::history::status))
        .route(
            "/api/v1/integrations/mqtt",
            get(crate::integrations::get_mqtt).post(crate::integrations::set_mqtt),
        )
        .with_state(state.clone());
    // The notes route allows a larger body than the 16 KiB default below; the inner limit wins.
    let notes = Router::new()
        .route(
            "/api/v1/registers/notes",
            axum::routing::put(crate::notes::put_notes),
        )
        .layer(axum::extract::DefaultBodyLimit::max(
            crate::notes::MAX_NOTES_BODY_BYTES,
        ))
        .with_state(state.clone());
    core.merge(setup)
        .merge(notes)
        .layer(axum::Extension(state.network.clone()))
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(state, guard))
}
async fn guard(
    State(state): State<ProvisioningState>,
    request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let path = request.uri().path();
    let method = request.method();
    // API keys: bearer requests need no cookie, so no CSRF; the allowlist decides.
    if request.headers().contains_key("authorization") {
        let Some(scope) = state.auth.api_scope(request.headers()) else {
            return Err(AuthError::unauthorized());
        };
        if !crate::access::key_allows(scope, method, path) {
            return Err(AuthError::forbidden());
        }
        let mut response = next.run(request).await;
        response
            .headers_mut()
            .insert("cache-control", "no-store".parse().unwrap());
        return Ok(response);
    }
    if !matches!(
        *method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    ) {
        check_csrf(request.headers())?;
    }
    let public = matches!(
        path,
        "/health"
            | "/openapi.json"
            | "/api/v1/setup/status"
            | "/api/v1/auth/session"
            | "/api/v1/auth/login"
    );
    let provisioning = matches!(
        path,
        "/api/v1/setup/admin"
            | "/api/v1/network/status"
            | "/api/v1/network/scan"
            | "/api/v1/network/connect"
    );
    let allowed = public
        || if provisioning {
            state.auth.provisioning(
                request.headers(),
                on_setup_ap(&state.network, request_peer(&request)),
            )
        } else {
            state.auth.authenticated(request.headers())
        };
    if !allowed {
        return Err(AuthError::unauthorized());
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    Ok(response)
}
#[utoipa::path(get, path = "/api/v1/setup/status", security(()), responses((status = 200, body = SetupStatus)))]
async fn setup_status(
    State(state): State<ProvisioningState>,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
) -> Json<SetupStatus> {
    let network = state.network.status().await;
    let setup_required = state.auth.setup_required();
    Json(SetupStatus {
        setup_required,
        setup_token_required: setup_required && !on_setup_ap(&state.network, peer.map(|p| p.0 .0)),
        network_state: network.state,
        setup_ssid: network.setup_ssid,
        setup_url: "http://192.168.4.1:8080/".into(),
        simulated: network.simulated,
    })
}
#[utoipa::path(get, path = "/api/v1/auth/session", security(()), responses((status = 200, body = SessionStatus)))]
async fn session(
    State(state): State<ProvisioningState>,
    headers: HeaderMap,
) -> Json<SessionStatus> {
    Json(state.auth.status(&headers))
}
#[utoipa::path(post, path = "/api/v1/setup/admin", security(("admin_session" = []), ("setup_token" = [])), request_body = PasswordRequest, responses((status = 200, body = SessionStatus), (status = 401, body = auth::AuthErrorBody), (status = 409, body = auth::AuthErrorBody)))]
async fn setup_admin(
    State(state): State<ProvisioningState>,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
    headers: HeaderMap,
    json: Result<Json<PasswordRequest>, JsonRejection>,
) -> Result<Response, AuthError> {
    let on_ap = on_setup_ap(&state.network, peer.map(|p| p.0 .0));
    state
        .auth
        .setup(&headers, json_body(json)?.password, on_ap)
        .await
}
#[utoipa::path(post, path = "/api/v1/auth/login", security(()), request_body = PasswordRequest, responses((status = 200, body = SessionStatus), (status = 401, body = auth::AuthErrorBody), (status = 429, body = auth::AuthErrorBody)))]
async fn login(
    State(state): State<ProvisioningState>,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
    headers: HeaderMap,
    json: Result<Json<PasswordRequest>, JsonRejection>,
) -> Result<Response, AuthError> {
    // Socket peer only: forwarded headers are untrusted on this LAN listener.
    let client = peer
        .map(|p| p.0 .0.ip())
        .unwrap_or(std::net::Ipv4Addr::LOCALHOST.into());
    state
        .auth
        .login_from(&headers, json_body(json)?.password, client)
        .await
}
#[utoipa::path(post, path = "/api/v1/auth/logout", responses((status = 200, body = SessionStatus)))]
async fn logout(State(state): State<ProvisioningState>, headers: HeaderMap) -> Response {
    state.auth.logout(&headers)
}
#[utoipa::path(get, path = "/api/v1/network/status", security(("admin_session" = []), ("setup_token" = [])), responses((status = 200, body = NetworkStatus)))]
async fn network_status(State(state): State<ProvisioningState>) -> Json<NetworkStatus> {
    Json(state.network.status().await)
}
#[utoipa::path(get, path = "/api/v1/network/scan", security(("admin_session" = []), ("setup_token" = [])), responses((status = 200, body = ScanResponse)))]
async fn network_scan(
    State(state): State<ProvisioningState>,
) -> Result<Json<ScanResponse>, AuthError> {
    Ok(Json(ScanResponse {
        networks: state.network.scan().await?,
    }))
}
#[utoipa::path(post, path = "/api/v1/network/connect", security(("admin_session" = []), ("setup_token" = [])), request_body = ConnectRequest, responses((status = 200, body = NetworkStatus), (status = 422, body = auth::AuthErrorBody)))]
async fn network_connect(
    State(state): State<ProvisioningState>,
    json: Result<Json<ConnectRequest>, JsonRejection>,
) -> Result<Json<NetworkStatus>, AuthError> {
    let permit = state.auth.permit()?;
    let request = json_body(json)?;
    // Complete the transaction if the single-radio transition drops this HTTP client.
    let result = tokio::spawn(async move {
        let _permit = permit;
        state.network.connect(request.ssid, request.password).await
    })
    .await
    .map_err(|_| AuthError::unavailable())?;
    Ok(Json(result?))
}
#[utoipa::path(post, path = "/api/v1/network/recovery", responses((status = 200, body = NetworkStatus)))]
async fn network_recovery(
    State(state): State<ProvisioningState>,
) -> Result<Json<NetworkStatus>, AuthError> {
    let permit = state.auth.permit()?;
    let result = tokio::spawn(async move {
        let _permit = permit;
        state.network.recovery().await
    })
    .await
    .map_err(|_| AuthError::unavailable())?;
    Ok(Json(result?))
}
#[utoipa::path(post, path = "/api/v1/setup/factory-reset", request_body = ResetRequest, responses((status = 200, body = ResetResponse)))]
async fn factory_reset(
    State(state): State<ProvisioningState>,
    json: Result<Json<ResetRequest>, JsonRejection>,
) -> Result<Json<ResetResponse>, AuthError> {
    let permit = state.auth.permit()?;
    let request = json_body(json)?;
    if request.confirmation != "ERASE" {
        return Err(AuthError::validation());
    }
    if !request.keep_history {
        if let Some(history) = state.history.clone() {
            tokio::task::spawn_blocking(move || history.erase())
                .await
                .map_err(|_| AuthError::unavailable())?
                .map_err(|_| AuthError::unavailable())?;
        }
    }
    tokio::spawn(async move {
        let _permit = permit;
        let result = state.network.factory_reset(state.identity).await;
        // Even if AP activation fails, persistent reset may already have completed.
        state.auth.invalidate_all();
        result
    })
    .await
    .map_err(|_| AuthError::unavailable())??;
    Ok(Json(ResetResponse { reset: true }))
}
#[derive(OpenApi)]
#[openapi(paths(
    setup_status,
    setup_admin,
    session,
    login,
    logout,
    network_status,
    network_scan,
    network_connect,
    network_recovery,
    factory_reset,
    crate::access::list,
    crate::access::create,
    crate::access::revoke,
    crate::access::change_password,
    crate::integrations::get_mqtt,
    crate::integrations::set_mqtt,
    crate::history::get_history,
    crate::history::export_csv,
    crate::history::status,
    crate::notes::put_notes
))]
pub struct ProvisioningDocument;
