//! Additive, mode-specific register explorer and read-only discovery routes.
use crate::{ApiError, Authorizer, Capability};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use nibe_core::ServiceError;
use nibe_device_f730::{
    catalog::{Catalog, MAX_CATALOG_BYTES},
    discovery::ProbeStatus,
    Capabilities, Explorer, F730Handle,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};
#[derive(Clone)]
struct F730State {
    handle: F730Handle,
    authorizer: Arc<dyn Authorizer>,
    store: Option<Arc<nibe_persistence::Store>>,
}
/// Merge beside the provisioning router; every handler enforces its own capability.
/// POST also uses the production CSRF policy, including imported offline catalogs.
pub fn router(handle: F730Handle, authorizer: Arc<dyn Authorizer>) -> Router {
    build_router(handle, authorizer, None)
}
pub fn persisted_router(
    handle: F730Handle,
    authorizer: Arc<dyn Authorizer>,
    store: Arc<nibe_persistence::Store>,
    mode: nibe_core::DeviceMode,
) -> Router {
    let task_handle = handle.clone();
    let task_store = store.clone();
    tokio::spawn(async move {
        let mut failed = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let snapshot = task_handle.snapshot();
            let mode = if mode == nibe_core::DeviceMode::Hardware {
                "hardware"
            } else {
                "replay"
            };
            let mut entries = vec![];
            for (&id, reading) in &snapshot.readings {
                let state = if reading.observation == nibe_core::ObservationState::ConfirmedReadable
                {
                    "confirmed-readable"
                } else {
                    "observed"
                };
                entries.push((id, state.into(), format!("{mode}:{}", reading.source)));
            }
            for (&id, state) in &snapshot.probe.states {
                if *state == nibe_device_f730::discovery::RegisterState::Unsupported
                    && !snapshot.readings.contains_key(&id)
                {
                    entries.push((
                        id,
                        "unsupported".into(),
                        format!("{mode}:probe-timeout-hypothesis"),
                    ));
                }
            }
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let store = task_store.clone();
            // File fsync stays off the async executor and wire task.
            let result =
                tokio::task::spawn_blocking(move || store.record_verification(entries, timestamp))
                    .await;
            let next_failed = !matches!(result, Ok(Ok(())));
            if next_failed && !failed {
                eprintln!("register evidence persistence unavailable; previous evidence retained");
            }
            failed = next_failed;
        }
    });
    build_router(handle, authorizer, Some(store))
}
fn build_router(
    handle: F730Handle,
    authorizer: Arc<dyn Authorizer>,
    store: Option<Arc<nibe_persistence::Store>>,
) -> Router {
    Router::new()
        .route("/api/v1/registers/explorer", get(explorer))
        .route("/api/v1/registers/catalog/import", post(import_catalog))
        .route("/api/v1/device/capabilities", get(capabilities))
        .route("/api/v1/discovery/probe", post(probe))
        .route("/api/v1/discovery/probe/cancel", post(cancel))
        .route("/api/v1/discovery/probe/status", get(probe_status))
        .layer(DefaultBodyLimit::max(MAX_CATALOG_BYTES))
        .with_state(F730State {
            handle,
            authorizer,
            store,
        })
}
fn read(state: &F730State, headers: &HeaderMap) -> Result<(), ApiError> {
    state
        .authorizer
        .authorize(headers, Capability::Read)
        .map_err(Into::into)
}
fn discover(state: &F730State, headers: &HeaderMap) -> Result<(), ApiError> {
    state.authorizer.authorize(headers, Capability::Discover)?;
    crate::auth::check_csrf(headers).map_err(|_| {
        ApiError(ServiceError::new(
            nibe_core::ErrorCode::Forbidden,
            "same-origin X-Requested-With: nibe-gateway required",
        ))
    })
}
#[derive(Serialize, ToSchema)]
pub struct PersistedExplorer {
    #[serde(flatten)]
    pub explorer: Explorer,
    pub verification_overlay:
        std::collections::BTreeMap<u16, nibe_persistence::RegisterVerification>,
    /// Per-installation notes set by the admin (ADR 0119); never grant write permission.
    pub register_notes: std::collections::BTreeMap<u16, nibe_persistence::RegisterNote>,
}
#[utoipa::path(get,path="/api/v1/registers/explorer",description="F730/replay only. Catalog metadata, passive observations, unknown raw values and discovery state, plus the saved observation overlay and this installation's register notes; every writable flag is false.",responses((status=200,body=PersistedExplorer)))]
async fn explorer(
    State(state): State<F730State>,
    headers: HeaderMap,
) -> Result<Json<PersistedExplorer>, ApiError> {
    read(&state, &headers)?;
    Ok(Json(PersistedExplorer {
        explorer: state.handle.explorer(),
        verification_overlay: state
            .store
            .as_ref()
            .map(|s| s.verification())
            .unwrap_or_default(),
        register_notes: state.store.map(|s| s.register_notes()).unwrap_or_default(),
    }))
}
#[utoipa::path(get,path="/api/v1/device/capabilities",responses((status=200,body=Capabilities)))]
async fn capabilities(
    State(state): State<F730State>,
    headers: HeaderMap,
) -> Result<Json<Capabilities>, ApiError> {
    read(&state, &headers)?;
    Ok(Json(state.handle.snapshot().capabilities.clone()))
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeRequest {
    pub register_ids: Vec<u16>,
}
#[utoipa::path(post,path="/api/v1/discovery/probe",request_body=ProbeRequest,description="Authenticated read requests for explicit catalog IDs (settings included; never-write/alarm-reset entries excluded except 48852); max 32, one outstanding read, >=1s spacing, cancellable. Hardware requires the --allow-hardware-reads opt-in (ADR 0101). Never writes. No blind scans.",responses((status=200,body=ProbeStatus),(status=403,body=crate::ApiErrorBody),(status=422,body=crate::ApiErrorBody),(status=429,body=crate::ApiErrorBody)))]
async fn probe(
    State(state): State<F730State>,
    headers: HeaderMap,
    json: Result<Json<ProbeRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<ProbeStatus>, ApiError> {
    discover(&state, &headers)?;
    let request = crate::body(json)?;
    Ok(Json(state.handle.probe(request.register_ids, true).await?))
}
#[utoipa::path(post,path="/api/v1/discovery/probe/cancel",responses((status=200,body=ProbeStatus)))]
async fn cancel(
    State(state): State<F730State>,
    headers: HeaderMap,
) -> Result<Json<ProbeStatus>, ApiError> {
    discover(&state, &headers)?;
    Ok(Json(state.handle.cancel().await?))
}
#[utoipa::path(get,path="/api/v1/discovery/probe/status",responses((status=200,body=ProbeStatus)))]
async fn probe_status(
    State(state): State<F730State>,
    headers: HeaderMap,
) -> Result<Json<ProbeStatus>, ApiError> {
    read(&state, &headers)?;
    Ok(Json(state.handle.snapshot().probe.clone()))
}
#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogImport {
    pub catalog_version: String,
    pub count: usize,
    pub writes_enabled: bool,
}
#[utoipa::path(post,path="/api/v1/registers/catalog/import",request_body=Catalog,description="Offline version-1 F730 JSON import into this process only (not persisted). All entries must be unverified and writes disabled. Clears prior observations; active probes must be cancelled first.",responses((status=200,body=CatalogImport),(status=422,body=crate::ApiErrorBody)))]
async fn import_catalog(
    State(state): State<F730State>,
    headers: HeaderMap,
    bytes: axum::body::Bytes,
) -> Result<Json<CatalogImport>, ApiError> {
    discover(&state, &headers)?;
    let catalog = Catalog::load(&bytes)?;
    let result = CatalogImport {
        catalog_version: catalog.catalog_version.clone(),
        count: catalog.registers.len(),
        writes_enabled: false,
    };
    state.handle.import(catalog).await?;
    Ok(Json(result))
}
#[derive(OpenApi)]
#[openapi(paths(explorer, capabilities, probe, cancel, probe_status, import_catalog))]
pub struct F730Document;
