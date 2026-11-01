//! Admin-only bridge to the root `nibe-system` helper: SSH keys and Tailscale.
//! Routed through the maintenance router, whose guard requires an admin session (API keys never
//! reach it) and CSRF proof for writes.
use crate::{
    auth::{json_body, AuthError, AuthErrorBody, ErrorDetail},
    maintenance::Maintenance,
};
use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use nibe_system::Request;
use serde::Deserialize;
use utoipa::ToSchema;

pub enum SystemError {
    Unavailable,
    Rejected(String),
    Auth(AuthError),
}
impl From<AuthError> for SystemError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}
impl IntoResponse for SystemError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Auth(error) => return error.into_response(),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "system_helper_unavailable",
                "system helper unavailable — needs a newer OS image".to_string(),
            ),
            Self::Rejected(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "system_helper_rejected",
                message,
            ),
        };
        (
            status,
            Json(AuthErrorBody {
                error: ErrorDetail {
                    code: code.into(),
                    message,
                },
            }),
        )
            .into_response()
    }
}

async fn call(
    state: &Maintenance,
    request: Request,
) -> Result<Json<serde_json::Value>, SystemError> {
    let socket = state.system_socket.clone();
    let reply = tokio::task::spawn_blocking(move || nibe_system::request(&socket, &request))
        .await
        .map_err(|_| SystemError::Unavailable)?
        .map_err(|_| SystemError::Unavailable)?;
    if reply.ok {
        Ok(Json(reply.result))
    } else {
        // The helper's messages are fixed strings and never echo request content or secrets.
        Err(SystemError::Rejected(
            reply.error.unwrap_or_else(|| "rejected".into()),
        ))
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SshKeys {
    /// Complete list of authorized public keys for `nibe-admin`. Empty turns SSH off.
    pub keys: Vec<String>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SshKey {
    pub key: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SshKeyFingerprint {
    pub fingerprint_sha256: String,
}
#[utoipa::path(post, path = "/api/v1/access/ssh/add", request_body = SshKey, description = "Admin session only. Adds one public key (turns SSH on).", responses((status = 200), (status = 422), (status = 503)))]
pub async fn ssh_add(
    State(state): State<Maintenance>,
    json: Result<Json<SshKey>, JsonRejection>,
) -> Result<Json<serde_json::Value>, SystemError> {
    let body = json_body(json)?;
    call(&state, Request::SshAddKey { key: body.key }).await
}
#[utoipa::path(post, path = "/api/v1/access/ssh/remove", request_body = SshKeyFingerprint, description = "Admin session only. Removes one key by fingerprint; removing the last one turns SSH off.", responses((status = 200), (status = 422), (status = 503)))]
pub async fn ssh_remove(
    State(state): State<Maintenance>,
    json: Result<Json<SshKeyFingerprint>, JsonRejection>,
) -> Result<Json<serde_json::Value>, SystemError> {
    let body = json_body(json)?;
    call(
        &state,
        Request::SshRemoveKey {
            fingerprint_sha256: body.fingerprint_sha256,
        },
    )
    .await
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TailscaleJoin {
    pub auth_key: String,
    #[serde(default)]
    pub hostname: Option<String>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TailscaleDisconnect {
    #[serde(default)]
    pub logout: bool,
}

#[utoipa::path(get, path = "/api/v1/access/ssh", description = "Admin session only. SSH status and authorized keys (fingerprints) via the system helper.", responses((status = 200), (status = 503)))]
pub async fn ssh_status(
    State(state): State<Maintenance>,
) -> Result<Json<serde_json::Value>, SystemError> {
    call(&state, Request::SshStatus).await
}
#[utoipa::path(post, path = "/api/v1/access/ssh", request_body = SshKeys, description = "Admin session only. Replaces the key list; an empty list turns SSH off.", responses((status = 200), (status = 422), (status = 503)))]
pub async fn ssh_set(
    State(state): State<Maintenance>,
    json: Result<Json<SshKeys>, JsonRejection>,
) -> Result<Json<serde_json::Value>, SystemError> {
    let body = json_body(json)?;
    call(&state, Request::SshSetKeys { keys: body.keys }).await
}
#[utoipa::path(get, path = "/api/v1/access/tailscale", description = "Admin session only. Tailscale state via the system helper.", responses((status = 200), (status = 503)))]
pub async fn tailscale_status(
    State(state): State<Maintenance>,
) -> Result<Json<serde_json::Value>, SystemError> {
    call(&state, Request::TailscaleStatus).await
}
#[utoipa::path(post, path = "/api/v1/access/tailscale/join", request_body = TailscaleJoin, description = "Admin session only. Joins the tailnet with a one-time auth key; the key is never stored.", responses((status = 200), (status = 422), (status = 503)))]
pub async fn tailscale_join(
    State(state): State<Maintenance>,
    json: Result<Json<TailscaleJoin>, JsonRejection>,
) -> Result<Json<serde_json::Value>, SystemError> {
    let body = json_body(json)?;
    call(
        &state,
        Request::TailscaleJoin {
            auth_key: body.auth_key,
            hostname: body.hostname,
        },
    )
    .await
}
#[utoipa::path(post, path = "/api/v1/access/tailscale/disconnect", request_body = TailscaleDisconnect, description = "Admin session only. `logout: true` removes this device from the tailnet; otherwise only disconnects.", responses((status = 200), (status = 422), (status = 503)))]
pub async fn tailscale_disconnect(
    State(state): State<Maintenance>,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
    json: Result<Json<TailscaleDisconnect>, JsonRejection>,
) -> Result<Json<serde_json::Value>, SystemError> {
    // Disconnecting Tailscale over Tailscale would cut the request's own path and can strand a Pi
    // that sits inside a heat pump. Require the local network for this one.
    if peer.is_some_and(|p| via_tailscale(p.0 .0.ip())) {
        return Err(AuthError::new(
            axum::http::StatusCode::CONFLICT,
            "via_tailscale",
            "you are connected over Tailscale; disconnect it from the local network instead",
        )
        .into());
    }
    let body = json_body(json)?;
    call(
        &state,
        Request::TailscaleDisconnect {
            logout: body.logout,
        },
    )
    .await
}

/// Tailscale's CGNAT range (100.64.0.0/10) and its IPv6 ULA prefix fd7a:115c:a1e0::/48.
fn via_tailscale(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 100 && (64..128).contains(&b)
        }
        std::net::IpAddr::V6(v6) => {
            v6.to_ipv4_mapped()
                .is_some_and(|v4| via_tailscale(v4.into()))
                || v6.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tailscale_addresses_are_recognised() {
        assert!(via_tailscale("100.83.89.16".parse().unwrap()));
        assert!(via_tailscale("fd7a:115c:a1e0::2e33:5911".parse().unwrap()));
        assert!(!via_tailscale("192.168.1.181".parse().unwrap()));
        assert!(!via_tailscale("100.128.0.1".parse().unwrap()));
    }
    use crate::auth::Auth;
    use nibe_persistence::{ApiKey, ApiScope, Store};
    use std::{sync::Arc, time::Duration};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Succeed;
    impl nibe_system::Runner for Succeed {
        fn run(&self, _: &str, _: &[&str], _: Duration) -> std::io::Result<nibe_system::Output> {
            Ok(nibe_system::Output {
                success: true,
                stdout: vec![],
            })
        }
    }

    async fn http(address: std::net::SocketAddr, request: String) -> (u16, String) {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        let code = response.split_whitespace().nth(1).unwrap().parse().unwrap();
        (code, response)
    }

    #[tokio::test]
    async fn system_routes_are_admin_only_and_report_a_missing_helper() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "1234").unwrap());
        let auth = Arc::new(Auth::new(store.clone(), false));
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-setup-token",
            store.setup_secrets().setup_token.parse().unwrap(),
        );
        let response = auth
            .setup(&headers, "system-admin-password".into(), false)
            .await
            .unwrap();
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let token = format!("nibe_{}", "ab".repeat(32));
        store
            .update(|c| {
                c.api_keys.push(ApiKey {
                    id: "k1".into(),
                    name: "control".into(),
                    scope: ApiScope::Control,
                    hash: crate::access::hash_token(&token).unwrap(),
                    created_at_ms: 0,
                })
            })
            .unwrap();
        let clock = Arc::new(nibe_simulator::SimClock::default());
        let device = nibe_simulator::SimulatedDevice::new(clock.clone(), "normal_heating").unwrap();
        let (gateway, _service) =
            nibe_core::start_device(Box::new(device), clock, nibe_core::ServiceConfig::default());
        let helper_socket = dir.path().join("system.sock");
        let router = crate::maintenance::router(Maintenance {
            auth,
            gateway,
            socket: dir.path().join("no-updater.sock"),
            system_socket: helper_socket.clone(),
            capture: None,
            verbose: Arc::new(|_| {}),
            releases: Arc::new(crate::releases::Releases::new(
                None,
                Arc::new(crate::releases::Curl),
            )),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let get = |extra: String| {
            format!("GET /api/v1/access/ssh HTTP/1.1\r\nHost: {address}\r\n{extra}Connection: close\r\n\r\n")
        };

        let (code, _) = http(address, get(format!("Authorization: Bearer {token}\r\n"))).await;
        assert_eq!(code, 401, "API keys never reach system settings");
        let (code, body) = http(address, get(format!("Cookie: {cookie}\r\n"))).await;
        assert_eq!(code, 503);
        assert!(body.contains("system_helper_unavailable"), "{body}");
        let post = format!(
            "POST /api/v1/access/ssh HTTP/1.1\r\nHost: {address}\r\nCookie: {cookie}\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{{\"keys\":[]}}"
        );
        assert_eq!(http(address, post).await.0, 403, "writes need CSRF proof");

        // A real helper on the socket (fake command runner) answers through the gateway.
        let listener = std::os::unix::net::UnixListener::bind(&helper_socket).unwrap();
        let root = dir.path().to_path_buf();
        std::thread::spawn(move || {
            let helper = nibe_system::Helper {
                paths: nibe_system::Paths {
                    ssh_keys: root.join("keys/nibe-admin"),
                    tailscale_dir: root.join("tailscale"),
                    runtime_dir: root.join("run"),
                },
                runner: Succeed,
            };
            let (mut stream, _) = listener.accept().unwrap();
            nibe_system::serve_one(&helper, &mut stream).unwrap();
        });
        let (code, body) = http(address, get(format!("Cookie: {cookie}\r\n"))).await;
        assert_eq!(code, 200, "{body}");
        assert!(body.contains("\"enabled\":false"), "{body}");
    }
}
