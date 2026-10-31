//! Independent maintenance routes use the same session + CSRF boundary as core.
//! Support export is constructed from numeric/enum fields, never by redacting logs.
use crate::{
    auth::{check_csrf, json_body, Auth, AuthError},
    Capability,
};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use nibe_core::Gateway;
use nibe_updater::rpc::{self, Action};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};
use utoipa::{OpenApi, ToSchema};
#[derive(Clone)]
pub struct Maintenance {
    pub auth: Arc<Auth>,
    pub gateway: Gateway,
    pub socket: PathBuf,
    /// `nibe-system` helper socket (ADR 0111).
    pub system_socket: PathBuf,
    /// A single configured capture file, never a request-supplied filesystem path.
    pub capture: Option<PathBuf>,
    pub verbose: Arc<dyn Fn(bool) + Send + Sync>,
    /// GitHub Releases update source (ADR 0124).
    pub releases: Arc<crate::releases::Releases>,
}
pub fn router(state: Maintenance) -> Router {
    let expiry = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            (expiry.verbose)(expiry.auth.debug_enabled());
        }
    });
    Router::new()
        .route("/api/v1/updates/status", get(update_status))
        .route(
            "/api/v1/updates/upload",
            post(upload).layer(DefaultBodyLimit::max(nibe_updater::artifact::MAX_ARTIFACT)),
        )
        .route("/api/v1/updates/remote", get(remote_status))
        .route("/api/v1/updates/remote/check", post(remote_check))
        .route("/api/v1/updates/remote/stage", post(remote_stage))
        .route("/api/v1/updates/activate", post(activate))
        .route("/api/v1/updates/rollback", post(rollback))
        .route("/api/v1/system/restart", post(restart))
        .route(
            "/api/v1/diagnostics/developer",
            get(developer_status).post(developer),
        )
        .route("/api/v1/diagnostics/support", get(support))
        .route("/api/v1/diagnostics/capture", get(capture))
        .route(
            "/api/v1/access/ssh",
            get(crate::system::ssh_status).post(crate::system::ssh_set),
        )
        .route("/api/v1/access/ssh/add", post(crate::system::ssh_add))
        .route("/api/v1/access/ssh/remove", post(crate::system::ssh_remove))
        .route(
            "/api/v1/access/tailscale",
            get(crate::system::tailscale_status),
        )
        .route(
            "/api/v1/access/tailscale/join",
            post(crate::system::tailscale_join),
        )
        .route(
            "/api/v1/access/tailscale/disconnect",
            post(crate::system::tailscale_disconnect),
        )
        .layer(DefaultBodyLimit::max(4096))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}
async fn guard(
    State(state): State<Maintenance>,
    request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    if !state.auth.authenticated(request.headers()) {
        return Err(AuthError::unauthorized());
    }
    if request.method() != axum::http::Method::GET {
        check_csrf(request.headers())?;
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    Ok(response)
}
#[derive(Serialize, ToSchema)]
pub struct UpdateStatus {
    pub available: bool,
    pub state: Option<serde_json::Value>,
}
#[derive(Serialize, ToSchema)]
pub struct Accepted {
    pub accepted: bool,
}
async fn call(
    state: &Maintenance,
    action: Action,
    // Bytes, not Vec: a 48 MiB upload must not be copied again on a 512 MB Pi.
    bytes: Bytes,
) -> Result<serde_json::Value, AuthError> {
    let socket = state.socket.clone();
    let reply = tokio::task::spawn_blocking(move || rpc::request(&socket, action, &bytes))
        .await
        .map_err(|_| AuthError::unavailable())?
        .map_err(|_| AuthError::unavailable())?;
    if !reply.ok {
        return Err(AuthError::new(
            StatusCode::CONFLICT,
            "update_rejected",
            "update rejected; verify signature, compatibility and updater status",
        ));
    }
    Ok(reply.status)
}
#[utoipa::path(get,path="/api/v1/updates/status",responses((status=200,body=UpdateStatus)))]
async fn update_status(State(state): State<Maintenance>) -> Json<UpdateStatus> {
    match call(&state, Action::Status, Bytes::new()).await {
        Ok(value) => Json(UpdateStatus {
            available: true,
            state: Some(value),
        }),
        Err(_) => Json(UpdateStatus {
            available: false,
            state: None,
        }),
    }
}
#[derive(Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadOptions {
    #[serde(default)]
    pub confirm_downgrade: bool,
}
#[utoipa::path(post,path="/api/v1/updates/upload",request_body(content=String,content_type="application/octet-stream"),params(("confirm_downgrade" = Option<bool>, Query, description = "Explicitly permit an older signed semantic version (default false)")),description="Upload a signed version-1 release tar (max 48 MiB). Older versions require confirm_downgrade=true. Verifies and stages only; activation requires a separate explicit action.",responses((status=200,body=UpdateStatus),(status=409,body=crate::auth::AuthErrorBody)))]
async fn upload(
    State(state): State<Maintenance>,
    Query(options): Query<UploadOptions>,
    bytes: Bytes,
) -> Result<Json<UpdateStatus>, AuthError> {
    let action = if options.confirm_downgrade {
        Action::UploadDowngrade
    } else {
        Action::Upload
    };
    let value = call(&state, action, bytes).await?;
    Ok(Json(UpdateStatus {
        available: true,
        state: Some(value),
    }))
}
#[utoipa::path(get,path="/api/v1/updates/remote",description="Result of the last check against the configured GitHub repository. Never contacts GitHub.",responses((status=200,body=crate::releases::RemoteStatus)))]
async fn remote_status(State(state): State<Maintenance>) -> Json<crate::releases::RemoteStatus> {
    Json(state.releases.status())
}
#[utoipa::path(post,path="/api/v1/updates/remote/check",description="Ask GitHub for the latest published release. Repeated checks within 15 seconds return the previous answer.",responses((status=200,body=crate::releases::RemoteStatus)))]
async fn remote_check(State(state): State<Maintenance>) -> Json<crate::releases::RemoteStatus> {
    Json(state.releases.check().await)
}
#[utoipa::path(post,path="/api/v1/updates/remote/stage",request_body=crate::releases::StageRequest,description="Download the signed release found by the last check and stage it exactly like an upload (signature, target and compatibility are verified by the updater). Only newer versions; activation stays a separate action.",responses((status=200,body=UpdateStatus),(status=409,body=crate::auth::AuthErrorBody),(status=502,body=crate::auth::AuthErrorBody)))]
async fn remote_stage(
    State(state): State<Maintenance>,
    json: Result<Json<crate::releases::StageRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<UpdateStatus>, AuthError> {
    use crate::releases::StageError;
    let request = json_body(json)?;
    let bytes = state
        .releases
        .download(&request.version)
        .await
        .map_err(|e| match e {
            StageError::Disabled => AuthError::new(
                StatusCode::CONFLICT,
                "update_source_disabled",
                "no update repository is configured",
            ),
            StageError::Busy => AuthError::new(
                StatusCode::CONFLICT,
                "download_in_progress",
                "a release download is already in progress",
            ),
            StageError::NotOffered => AuthError::new(
                StatusCode::CONFLICT,
                "release_not_offered",
                "check for updates first; only a newer published release can be staged",
            ),
            StageError::Download => AuthError::new(
                StatusCode::BAD_GATEWAY,
                "download_failed",
                "the release could not be downloaded from GitHub",
            ),
        })?;
    let value = call(&state, Action::Upload, Bytes::from(bytes)).await?;
    Ok(Json(UpdateStatus {
        available: true,
        state: Some(value),
    }))
}
#[utoipa::path(post,path="/api/v1/updates/activate",description="Independent updater completes after gateway restart. Session expires on restart; log in again to inspect status.",responses((status=202,body=Accepted)))]
async fn activate(
    State(state): State<Maintenance>,
) -> Result<(StatusCode, Json<Accepted>), AuthError> {
    call(&state, Action::Activate, Bytes::new()).await?;
    Ok((StatusCode::ACCEPTED, Json(Accepted { accepted: true })))
}
#[utoipa::path(post,path="/api/v1/updates/rollback",responses((status=202,body=Accepted)))]
async fn rollback(
    State(state): State<Maintenance>,
) -> Result<(StatusCode, Json<Accepted>), AuthError> {
    call(&state, Action::Rollback, Bytes::new()).await?;
    Ok((StatusCode::ACCEPTED, Json(Accepted { accepted: true })))
}
#[utoipa::path(post,path="/api/v1/system/restart",responses((status=202,body=Accepted)))]
async fn restart(
    State(state): State<Maintenance>,
) -> Result<(StatusCode, Json<Accepted>), AuthError> {
    call(&state, Action::Restart, Bytes::new()).await?;
    Ok((StatusCode::ACCEPTED, Json(Accepted { accepted: true })))
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeveloperRequest {
    pub seconds: u64,
    pub confirmation: String,
}
#[derive(Serialize, ToSchema)]
pub struct DeveloperStatus {
    pub enabled: bool,
    pub seconds_remaining: u64,
    pub capture_available: bool,
}
fn developer_view(state: &Maintenance) -> DeveloperStatus {
    DeveloperStatus {
        enabled: state.auth.debug_enabled(),
        seconds_remaining: state.auth.debug_remaining(),
        capture_available: state.capture.as_ref().is_some_and(|p| p.is_file()),
    }
}
#[utoipa::path(get,path="/api/v1/diagnostics/developer",responses((status=200,body=DeveloperStatus)))]
async fn developer_status(State(state): State<Maintenance>) -> Json<DeveloperStatus> {
    Json(developer_view(&state))
}
#[utoipa::path(post,path="/api/v1/diagnostics/developer",request_body=DeveloperRequest,description="Explicit admin lease, 0 disables; max 900 seconds, confirmation AUTHORIZE_DIAGNOSTICS. Enables temporary verbose tracing and protected capture/dev access, never controls/auth bypass. Restart revokes the lease.",responses((status=200,body=DeveloperStatus)))]
async fn developer(
    State(state): State<Maintenance>,
    json: Result<Json<DeveloperRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<DeveloperStatus>, AuthError> {
    let request = json_body(json)?;
    if request.seconds > 900 || request.confirmation != "AUTHORIZE_DIAGNOSTICS" {
        return Err(AuthError::validation());
    }
    state.auth.set_debug(request.seconds);
    (state.verbose)(request.seconds > 0);
    Ok(Json(developer_view(&state)))
}
#[derive(Serialize, ToSchema)]
pub struct SupportBundle {
    pub format_version: u32,
    pub gateway_version: String,
    pub mode: nibe_core::DeviceMode,
    pub connection: nibe_core::ConnectionState,
    pub stale: bool,
    pub controls_enabled: bool,
    pub register_count: usize,
    pub counters: nibe_core::Diagnostics,
}
pub fn support_bundle(gateway: &Gateway) -> SupportBundle {
    bundle_from_snapshot(gateway.snapshot())
}
fn bundle_from_snapshot(d: nibe_core::DeviceSnapshot) -> SupportBundle {
    SupportBundle {
        format_version: 1,
        gateway_version: env!("CARGO_PKG_VERSION").into(),
        mode: d.mode,
        connection: d.connection,
        stale: d.stale,
        controls_enabled: d.controls_enabled,
        register_count: d.registers.len(),
        counters: d.diagnostics,
    }
}
#[utoipa::path(get,path="/api/v1/diagnostics/support",description="Allowlisted support JSON; excludes all logs, capture bytes, config, SSIDs, credentials, identity and free-text errors.",responses((status=200,body=SupportBundle)))]
async fn support(State(state): State<Maintenance>) -> Json<SupportBundle> {
    Json(support_bundle(&state.gateway))
}
#[utoipa::path(get,path="/api/v1/diagnostics/capture",description="Admin + active diagnostic lease required. Raw serial capture is sensitive and deliberately excluded from support bundles. WS04 capture-jsonl v1; only the configured recording file can be exported.",responses((status=200,body=String,content_type="application/x-ndjson"),(status=403,body=crate::auth::AuthErrorBody)))]
async fn capture(
    State(state): State<Maintenance>,
    headers: HeaderMap,
) -> Result<impl axum::response::IntoResponse, AuthError> {
    use crate::Authorizer;
    state
        .auth
        .authorize(&headers, Capability::Debug)
        .map_err(|_| AuthError::lease_required())?;
    let path = state.capture.ok_or_else(AuthError::unavailable)?;
    let bytes = tokio::task::spawn_blocking(move || -> nibe_updater::Result<Vec<u8>> {
        use std::io::Read;
        let f = std::fs::File::open(path)?;
        let mut bytes = vec![];
        f.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(nibe_updater::fail("capture limit"));
        }
        // A live recorder may have one partial final line. Export completed lines only.
        let end = bytes
            .iter()
            .rposition(|b| *b == b'\n')
            .ok_or_else(|| nibe_updater::fail("incomplete capture"))?;
        bytes.truncate(end + 1);
        nibe_device_f730::transport::Capture::read(bytes.as_slice())
            .map_err(|_| nibe_updater::fail("invalid capture"))?;
        Ok(bytes)
    })
    .await
    .map_err(|_| AuthError::unavailable())?
    .map_err(|_| AuthError::unavailable())?;
    // Recheck lease/session after disk work.
    state
        .auth
        .authorize(&headers, Capability::Debug)
        .map_err(|_| AuthError::lease_required())?;
    Ok((
        [
            ("content-type", "application/x-ndjson"),
            (
                "content-disposition",
                "attachment; filename=serial.capture.jsonl",
            ),
        ],
        bytes,
    ))
}
#[derive(OpenApi)]
#[openapi(paths(
    crate::system::ssh_status,
    crate::system::ssh_set,
    crate::system::ssh_add,
    crate::system::ssh_remove,
    crate::system::tailscale_status,
    crate::system::tailscale_join,
    crate::system::tailscale_disconnect,
    update_status,
    upload,
    remote_status,
    remote_check,
    remote_stage,
    activate,
    rollback,
    restart,
    developer_status,
    developer,
    support,
    capture
))]
pub struct MaintenanceDocument;

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn support_never_serializes_untrusted_text_or_persistent_secrets() {
        let clock = Arc::new(nibe_simulator::SimClock::default());
        let device = nibe_simulator::SimulatedDevice::new(clock.clone(), "normal_heating").unwrap();
        let (gateway, service) =
            nibe_core::start_device(Box::new(device), clock, nibe_core::ServiceConfig::default());
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut snapshot = gateway.snapshot();
        const SECRET: &str = "CANARY-password-cookie-token-SSID-serial";
        snapshot.last_error = Some(SECRET.into());
        if let Some(identity) = &mut snapshot.identity {
            identity.serial = Some(SECRET.into());
            identity.model = SECRET.into();
            identity.firmware = Some(SECRET.into());
        }
        snapshot.discovery.detail = SECRET.into();
        for r in &mut snapshot.catalog {
            r.name = SECRET.into();
        }
        let report = serde_json::to_string(&bundle_from_snapshot(snapshot)).unwrap();
        assert!(!report.contains(SECRET));
        let keys = serde_json::from_str::<serde_json::Value>(&report).unwrap();
        assert!(keys["counters"]["polls"].as_u64().unwrap() > 0);
        assert!(!report.contains("identity"));
        assert!(!report.contains("last_error"));
        assert!(!report.contains("registers"));
        assert!(!report.contains("discovery"));
        service.shutdown().await;
    }
}
