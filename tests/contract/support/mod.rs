#![allow(dead_code)]
use nibe_api::{auth::Auth, provisioning::ProvisioningState, ApiState, Authorizer};
use nibe_core::{start_device, DeviceService, Gateway, ServiceConfig};
use nibe_network::{Provisioner, SimulatedNetwork};
use nibe_persistence::Store;
use nibe_simulator::{SimClock, SimulatedDevice};
use serde_json::Value;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub struct Server {
    pub address: SocketAddr,
    pub gateway: Gateway,
    pub clock: Arc<SimClock>,
    pub service: DeviceService,
    task: tokio::task::JoinHandle<()>,
    pub auth: Option<Arc<Auth>>,
    pub store: Option<Arc<Store>>,
    pub cookie: Option<String>,
    _data_dir: Option<tempfile::TempDir>,
}
impl Server {
    pub async fn new(scenario: &str, controls: bool) -> Self {
        Self::start(scenario, controls, None, true).await
    }
    /// Production router with the bundled F730 replay adapter, so the register explorer exists.
    pub async fn with_replay_explorer() -> Self {
        Self::start_with("normal_heating", false, None, true, true, None).await
    }
    pub async fn authorized(
        scenario: &str,
        controls: bool,
        authorizer: Arc<dyn Authorizer>,
    ) -> Self {
        Self::start(scenario, controls, Some(authorizer), false).await
    }
    pub async fn unclaimed() -> Self {
        Self::start("normal_heating", false, None, false).await
    }
    /// Unclaimed production router that sees every request as coming from `peer`, as the real
    /// listener's `ConnectInfo` would (e.g. a phone on the setup AP, ADR 0060).
    pub async fn unclaimed_from(peer: &str) -> Self {
        let peer: SocketAddr = peer.parse().unwrap();
        Self::start_with("normal_heating", false, None, false, false, Some(peer)).await
    }
    async fn start(
        scenario: &str,
        controls: bool,
        authorizer: Option<Arc<dyn Authorizer>>,
        bootstrap: bool,
    ) -> Self {
        Self::start_with(scenario, controls, authorizer, bootstrap, false, None).await
    }
    async fn start_with(
        scenario: &str,
        controls: bool,
        authorizer: Option<Arc<dyn Authorizer>>,
        bootstrap: bool,
        replay_explorer: bool,
        peer: Option<SocketAddr>,
    ) -> Self {
        let clock = Arc::new(SimClock::default());
        clock.freeze(true);
        clock.reset();
        let device = SimulatedDevice::new(clock.clone(), scenario).unwrap();
        let config = ServiceConfig {
            poll_interval: Duration::from_millis(10),
            adapter_timeout: Duration::from_millis(20),
            request_timeout: Duration::from_millis(250),
            queue_capacity: 2,
            event_capacity: 2,
            allow_simulated_controls: controls,
            ..ServiceConfig::default()
        };
        let (gateway, service) = start_device(Box::new(device), clock.clone(), config);
        let mut auth = None;
        let mut store = None;
        let mut cookie = None;
        let mut data_dir = None;
        let router = if let Some(authorizer) = authorizer {
            nibe_api::router(ApiState {
                gateway: gateway.clone(),
                authorizer,
                leases: None,
            })
        } else {
            let dir = tempfile::tempdir().unwrap();
            let state_store = Arc::new(Store::open(dir.path(), "1234").unwrap());
            let network = Arc::new(
                Provisioner::new(Box::<SimulatedNetwork>::default(), state_store.clone())
                    .await
                    .unwrap(),
            );
            let state_auth = Arc::new(Auth::new(state_store.clone(), false));
            if bootstrap {
                let mut headers = axum::http::HeaderMap::new();
                headers.insert(
                    "x-setup-token",
                    state_store.setup_secrets().setup_token.parse().unwrap(),
                );
                let response = state_auth
                    .setup(&headers, "contract-admin-password".into(), false)
                    .await
                    .unwrap();
                cookie = Some(
                    response.headers()["set-cookie"]
                        .to_str()
                        .unwrap()
                        .split(';')
                        .next()
                        .unwrap()
                        .to_string(),
                );
            }
            let f730 = if replay_explorer {
                let clock: Arc<dyn nibe_core::Clock> = Arc::new(nibe_core::SystemClock::default());
                let (_device, handle) = nibe_device_f730::start_adapter(
                    nibe_device_f730::AdapterConfig::replay(),
                    clock,
                )
                .await
                .unwrap();
                Some(handle)
            } else {
                None
            };
            let leases = nibe_api::lease::Leases::start_with(
                gateway.clone(),
                Some(state_store.clone()),
                Duration::from_millis(10),
            );
            let audit = Arc::new(nibe_persistence::audit::AuditLog::open(dir.path()));
            let router = nibe_api::provisioning::router_with_f730(
                gateway.clone(),
                ProvisioningState {
                    auth: state_auth.clone(),
                    network,
                    identity: "1234".into(),
                    mqtt: Some(nibe_mqtt::start(
                        gateway.clone(),
                        nibe_mqtt::Identity {
                            device_id: nibe_mqtt::device_id("1234"),
                            gateway_version: "test".into(),
                            audit: None,
                        },
                        nibe_mqtt::Controls::default(),
                        None,
                        None,
                    )),
                    history: Some({
                        let history = nibe_history::History::open(
                            nibe_history::Config::new(dir.path().join("history")),
                            || true,
                        )
                        .unwrap();
                        nibe_history::spawn_recorder(gateway.clone(), history.clone());
                        history
                    }),
                    leases: Some(leases.clone()),
                    audit: Some(audit.clone()),
                    registers: f730.clone().map(|handle| {
                        nibe_api::registers::Registers::new(
                            handle,
                            gateway.clone(),
                            Some(state_store.clone()),
                            vec![],
                            vec![],
                            Some(leases.clone()),
                        )
                    }),
                },
                f730,
            )
            .layer(axum::middleware::from_fn_with_state(
                nibe_api::audit::AuditLayer {
                    log: audit,
                    auth: state_auth.clone(),
                },
                nibe_api::audit::record,
            ));
            auth = Some(state_auth);
            store = Some(state_store);
            data_dir = Some(dir);
            router
        };
        let router = match peer {
            // The same extension `into_make_service_with_connect_info` inserts in production.
            Some(peer) => router.layer(axum::middleware::from_fn(
                move |mut request: axum::extract::Request, next: axum::middleware::Next| {
                    request
                        .extensions_mut()
                        .insert(axum::extract::ConnectInfo(peer));
                    next.run(request)
                },
            )),
            None => router,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while gateway.snapshot().diagnostics.polls == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        Self {
            address,
            gateway,
            clock,
            service,
            task,
            auth,
            store,
            cookie,
            _data_dir: data_dir,
        }
    }
    pub async fn request(&self, method: &str, path: &str, value: Option<Value>) -> (u16, Value) {
        let body = value.map(|v| v.to_string()).unwrap_or_default();
        self.raw(method, path, &body).await
    }
    pub async fn raw(&self, method: &str, path: &str, body: &str) -> (u16, Value) {
        let mut headers = vec![("X-Requested-With", "nibe-gateway")];
        if let Some(cookie) = &self.cookie {
            headers.push(("Cookie", cookie));
        }
        let (code, value, _) = self.raw_headers(method, path, body, &headers).await;
        (code, value)
    }
    pub async fn raw_headers(
        &self,
        method: &str,
        path: &str,
        body: &str,
        headers: &[(&str, &str)],
    ) -> (u16, Value, axum::http::HeaderMap) {
        let mut stream = TcpStream::connect(self.address).await.unwrap();
        let extra = headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}\r\n"))
            .collect::<String>();
        let request = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n{extra}Connection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        // A hang guard, not a latency assertion: argon2 in an unoptimized build on a shared 2-core CI runner can take
        // seconds (password change verifies and re-hashes).
        tokio::time::timeout(Duration::from_secs(20), stream.read_to_end(&mut response))
            .await
            .unwrap()
            .unwrap();
        let response = String::from_utf8(response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (
            status,
            serde_json::from_str(body)
                .unwrap_or_else(|e| panic!("invalid HTTP JSON: {e}: {response}")),
            headers
                .lines()
                .skip(1)
                .filter_map(|line| line.split_once(":"))
                .map(|(key, value)| {
                    (
                        key.parse::<axum::http::HeaderName>().unwrap(),
                        value.trim().parse().unwrap(),
                    )
                })
                .collect(),
        )
    }
    pub async fn sse(&self) -> TcpStream {
        let mut stream = TcpStream::connect(self.address).await.unwrap();
        stream
            .write_all(
                format!(
                    "GET /api/v1/events HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nCookie: {}\r\n\r\n",
                    self.cookie.as_deref().unwrap_or("")
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        stream
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
