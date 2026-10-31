//! Transactional Wi-Fi provisioning, independent of pump simulation and HTTP.
use nibe_persistence::{NetworkConfig, SetupSecrets, Store};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use utoipa::ToSchema;

#[cfg(feature = "network-manager")]
pub mod network_manager;

pub type NetworkResult<T> = Result<T, NetworkError>;
pub type Operation<'a, T> = Pin<Box<dyn Future<Output = NetworkResult<T>> + Send + 'a>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkError {
    InvalidCredentials,
    Unavailable,
    Persistence,
}
impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidCredentials => "connection failed; check SSID/password and retry",
            Self::Unavailable => "network backend unavailable; previous configuration retained",
            Self::Persistence => "could not persist network configuration",
        })
    }
}
impl std::error::Error for NetworkError {}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct AccessPoint {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[schema(as = ProvisioningNetworkState)]
pub enum State {
    Ap,
    Connecting,
    Connected,
    Offline,
    RecoveryAp,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = NetworkStatus)]
pub struct Status {
    pub ip_addresses: Vec<String>,
    pub state: State,
    pub ssid: Option<String>,
    pub setup_ssid: Option<String>,
    pub last_error: Option<String>,
    pub simulated: bool,
}
/// Backends must verify station activation before success, bound every operation,
/// and restore the prior state on failed connect (NM uses checkpoints).
/// `restore` must never start an AP when a saved station is unavailable.
pub trait Network: Send + Sync {
    fn simulated(&self) -> bool;
    fn ip_addresses(&mut self) -> Operation<'_, Vec<String>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn scan(&mut self) -> Operation<'_, Vec<AccessPoint>>;
    fn start_ap<'a>(&'a mut self, secrets: &'a SetupSecrets) -> Operation<'a, ()>;
    fn connect<'a>(&'a mut self, ssid: &'a str, password: &'a str) -> Operation<'a, ()>;
    fn restore<'a>(&'a mut self, config: &'a NetworkConfig) -> Operation<'a, ()>;
    fn finish(&mut self, commit: bool) -> Operation<'_, ()>;
    fn connected<'a>(&'a mut self, ssid: &'a str) -> Operation<'a, bool>;
    /// Delete only gateway-owned profiles, never arbitrary system connections.
    fn forget(&mut self) -> Operation<'_, ()>;
}
struct Inner {
    backend: Box<dyn Network>,
    status: Status,
    /// An AP that should be up but NetworkManager refused; retried with a backoff while Offline.
    pending_ap: Option<PendingAp>,
}
/// First AP retry after this long, doubling per failure up to `AP_RETRY_MAX`.
const AP_RETRY_FIRST: std::time::Duration = std::time::Duration::from_secs(30);
const AP_RETRY_MAX: std::time::Duration = std::time::Duration::from_secs(300);
struct PendingAp {
    state: State,
    failures: u32,
    next: tokio::time::Instant,
}
impl Inner {
    /// Records a refused AP start: the gateway stays up Offline (HTTP keeps answering) and retries later.
    fn ap_failed(&mut self, state: State, error: NetworkError) {
        let failures = self
            .pending_ap
            .as_ref()
            .filter(|pending| pending.state == state)
            .map_or(1, |pending| pending.failures.saturating_add(1));
        let delay = AP_RETRY_FIRST
            .saturating_mul(1 << (failures - 1).min(4))
            .min(AP_RETRY_MAX);
        self.pending_ap = Some(PendingAp {
            state,
            failures,
            next: tokio::time::Instant::now() + delay,
        });
        self.status.state = State::Offline;
        self.status.setup_ssid = None;
        self.status.last_error = Some(format!("setup access point did not start: {error}"));
    }
}
pub struct Provisioner {
    inner: Mutex<Inner>,
    visible: std::sync::Mutex<Status>,
    store: Arc<Store>,
}
impl Provisioner {
    pub async fn new(mut backend: Box<dyn Network>, store: Arc<Store>) -> NetworkResult<Self> {
        let simulated = backend.simulated();
        let saved = store.config().network;
        let mut ap_error = None;
        let (state, ssid, setup_ssid) = if let Some(config) = saved {
            // A gateway restart (app update) must not drop an already working Wi-Fi link: re-activating the
            // profile makes NetworkManager reassociate. Only activate when the saved station is not up.
            let up = backend.connected(&config.ssid).await.unwrap_or(false);
            let state = if up || backend.restore(&config).await.is_ok() {
                State::Connected
            } else {
                State::Offline
            };
            (state, Some(config.ssid), None)
        } else {
            let secrets = store.setup_secrets();
            // A refused AP must not take the web UI down with it (HP-47): stay up and retry.
            match backend.start_ap(&secrets).await {
                Ok(()) => (State::Ap, None, Some(secrets.ssid())),
                Err(error) => {
                    ap_error = Some(error);
                    (State::Offline, None, None)
                }
            }
        };
        let ip_addresses = backend.ip_addresses().await.unwrap_or_default();
        let status = Status {
            ip_addresses,
            state,
            ssid,
            setup_ssid,
            last_error: (state == State::Offline).then(|| NetworkError::Unavailable.to_string()),
            simulated,
        };
        let mut inner = Inner {
            backend,
            status,
            pending_ap: None,
        };
        if let Some(error) = ap_error {
            inner.ap_failed(State::Ap, error);
        }
        Ok(Self {
            store,
            visible: std::sync::Mutex::new(inner.status.clone()),
            inner: Mutex::new(inner),
        })
    }
    /// Re-tries the saved station while Offline. Never starts an AP and never touches Ap/RecoveryAp
    /// or an in-progress operation. Returns true when the station came back.
    pub async fn retry_restore(&self) -> bool {
        let Ok(mut inner) = self.inner.try_lock() else {
            return false;
        };
        if inner.status.state != State::Offline {
            return false;
        }
        let Some(saved) = self.store.config().network else {
            return false;
        };
        if inner.backend.restore(&saved).await.is_err() {
            return false;
        }
        inner.status.state = State::Connected;
        inner.status.ssid = Some(saved.ssid);
        inner.status.last_error = None;
        inner.pending_ap = None;
        inner.status.ip_addresses = inner.backend.ip_addresses().await.unwrap_or_default();
        self.publish(&inner.status);
        true
    }
    /// Re-tries an AP that NetworkManager refused (first boot, factory reset, failed setup connect), once
    /// its backoff has passed. Only while Offline with no operation in progress; an AP that is not
    /// pending is never started here, so an outage of a saved station still never opens one.
    pub async fn retry_ap(&self) -> bool {
        let Ok(mut inner) = self.inner.try_lock() else {
            return false;
        };
        let Some(pending) = inner.pending_ap.as_ref() else {
            return false;
        };
        if inner.status.state != State::Offline || tokio::time::Instant::now() < pending.next {
            return false;
        }
        let state = pending.state;
        if state == State::Ap && self.store.config().network.is_some() {
            inner.pending_ap = None;
            return false;
        }
        let secrets = self.store.setup_secrets();
        if let Err(error) = inner.backend.start_ap(&secrets).await {
            inner.ap_failed(state, error);
            self.publish(&inner.status);
            return false;
        }
        inner.pending_ap = None;
        inner.status.state = state;
        inner.status.setup_ssid = Some(secrets.ssid());
        inner.status.last_error = None;
        inner.status.ip_addresses = inner.backend.ip_addresses().await.unwrap_or_default();
        self.publish(&inner.status);
        true
    }
    /// Background loop for `retry_restore` and `retry_ap`; the station may appear late at boot or after an
    /// outage, and NetworkManager may refuse the AP at first.
    pub fn spawn_restore_retries(
        self: &Arc<Self>,
        every: std::time::Duration,
    ) -> tokio::task::JoinHandle<()> {
        let network = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                if !network.retry_restore().await {
                    network.retry_ap().await;
                }
            }
        })
    }
    pub fn cached_status(&self) -> Status {
        self.visible.lock().unwrap().clone()
    }
    fn publish(&self, status: &Status) {
        *self.visible.lock().unwrap() = status.clone();
    }
    pub async fn status(&self) -> Status {
        // UI can observe Connecting while the single radio is switching.
        let Ok(mut inner) = self.inner.try_lock() else {
            return self.cached_status();
        };
        if matches!(inner.status.state, State::Connected | State::Offline) {
            let ssid = inner.status.ssid.clone().unwrap_or_default();
            inner.status.state = if inner.backend.connected(&ssid).await.unwrap_or(false) {
                State::Connected
            } else {
                State::Offline
            };
        }
        inner.status.ip_addresses = inner.backend.ip_addresses().await.unwrap_or_default();
        self.publish(&inner.status);
        inner.status.clone()
    }
    pub async fn scan(&self) -> NetworkResult<Vec<AccessPoint>> {
        self.inner.lock().await.backend.scan().await
    }
    pub async fn connect(&self, ssid: String, password: String) -> NetworkResult<Status> {
        if ssid.is_empty()
            || ssid.len() > 32
            || ssid.chars().any(char::is_control)
            || password.len() > 64
        {
            return Err(NetworkError::InvalidCredentials);
        }
        let mut inner = self.inner.lock().await;
        let previous = inner.status.clone();
        inner.status.state = State::Connecting;
        self.publish(&inner.status);
        let result = inner.backend.connect(&ssid, &password).await;
        if let Err(error) = result {
            inner.status = previous;
            // On a single radio, restore AP explicitly even if NM already rolled back.
            inner.status.last_error = Some(error.to_string());
            let ap = inner.status.state;
            if matches!(ap, State::Ap | State::RecoveryAp) {
                if let Err(ap_error) = inner.backend.start_ap(&self.store.setup_secrets()).await {
                    inner.ap_failed(ap, ap_error);
                }
            } else if let Some(saved) = self.store.config().network {
                if inner.backend.restore(&saved).await.is_err() {
                    inner.status.state = State::Offline;
                }
            }
            self.publish(&inner.status);
            return Err(error);
        }
        let previous_config = self.store.config().network;
        let store = self.store.clone();
        let config = NetworkConfig {
            ssid: ssid.clone(),
            extra: previous_config
                .as_ref()
                .map(|config| config.extra.clone())
                .unwrap_or_default(),
        };
        let saved =
            tokio::task::spawn_blocking(move || store.update(|c| c.network = Some(config))).await;
        if !matches!(saved, Ok(Ok(()))) || inner.backend.finish(true).await.is_err() {
            let _ = inner.backend.finish(false).await;
            let store = self.store.clone();
            let _ =
                tokio::task::spawn_blocking(move || store.update(|c| c.network = previous_config))
                    .await;
            let restored = if let Some(saved) = self.store.config().network {
                inner.backend.restore(&saved).await.map_err(|_| None)
            } else {
                let _ = inner.backend.forget().await;
                inner
                    .backend
                    .start_ap(&self.store.setup_secrets())
                    .await
                    .map_err(Some)
            };
            inner.status = previous;
            inner.status.last_error = Some(NetworkError::Persistence.to_string());
            match restored {
                Ok(()) => {}
                Err(Some(ap_error)) => {
                    let ap = match inner.status.state {
                        State::RecoveryAp => State::RecoveryAp,
                        _ => State::Ap,
                    };
                    inner.ap_failed(ap, ap_error);
                }
                Err(None) => {
                    inner.status.state = State::Offline;
                    inner.status.setup_ssid = None;
                }
            }
            self.publish(&inner.status);
            return Err(NetworkError::Persistence);
        }
        inner.pending_ap = None;
        inner.status = Status {
            ip_addresses: inner.backend.ip_addresses().await.unwrap_or_default(),
            state: State::Connected,
            ssid: Some(ssid),
            setup_ssid: None,
            last_error: None,
            simulated: inner.backend.simulated(),
        };
        self.publish(&inner.status);
        Ok(inner.status.clone())
    }
    pub async fn recovery(&self) -> NetworkResult<Status> {
        let mut inner = self.inner.lock().await;
        let secrets = self.store.setup_secrets();
        inner.backend.start_ap(&secrets).await?;
        inner.pending_ap = None;
        inner.status.state = State::RecoveryAp;
        inner.status.setup_ssid = Some(secrets.ssid());
        inner.status.last_error = None;
        self.publish(&inner.status);
        Ok(inner.status.clone())
    }
    /// Auth serializes this operation with setup/login; callers invalidate all sessions.
    pub async fn factory_reset(&self, identity: String) -> NetworkResult<()> {
        let mut inner = self.inner.lock().await;
        inner.backend.forget().await?;
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.factory_reset(&identity))
            .await
            .map_err(|_| NetworkError::Persistence)?
            .map_err(|_| NetworkError::Persistence)?;
        let secrets = self.store.setup_secrets();
        let activated = inner.backend.start_ap(&secrets).await;
        inner.status = Status {
            ip_addresses: inner.backend.ip_addresses().await.unwrap_or_default(),
            state: if activated.is_ok() {
                State::Ap
            } else {
                State::Offline
            },
            ssid: None,
            setup_ssid: activated.is_ok().then(|| secrets.ssid()),
            last_error: activated.as_ref().err().map(ToString::to_string),
            simulated: inner.backend.simulated(),
        };
        inner.pending_ap = None;
        if let Err(error) = activated {
            inner.ap_failed(State::Ap, error);
        }
        self.publish(&inner.status);
        activated
    }
}

#[derive(Default)]
pub struct SimulatedNetwork {
    station: Option<String>,
    outage: bool,
    /// Restores that fail before the radio "comes up" (boot-time race in tests).
    failing_restores: u32,
    /// AP starts that fail, as NetworkManager refusing the AP (polkit, driver).
    failing_aps: u32,
    connect_delay: std::time::Duration,
}
impl SimulatedNetwork {
    pub fn with_delay(delay: std::time::Duration) -> Self {
        Self {
            connect_delay: delay,
            ..Self::default()
        }
    }
    pub fn with_outage() -> Self {
        Self {
            outage: true,
            ..Self::default()
        }
    }
    /// Wi-Fi already up when the gateway (re)starts, e.g. after an app update.
    pub fn already_connected(ssid: &str) -> Self {
        Self {
            station: Some(ssid.into()),
            ..Self::default()
        }
    }
    pub fn with_failing_aps(count: u32) -> Self {
        Self {
            failing_aps: count,
            ..Self::default()
        }
    }
    pub fn with_failing_restores(count: u32) -> Self {
        Self {
            failing_restores: count,
            ..Self::default()
        }
    }
}
impl Network for SimulatedNetwork {
    fn ip_addresses(&mut self) -> Operation<'_, Vec<String>> {
        // Simulation does not invent a LAN lease.
        Box::pin(async { Ok(vec!["127.0.0.1".into()]) })
    }
    fn simulated(&self) -> bool {
        true
    }
    fn scan(&mut self) -> Operation<'_, Vec<AccessPoint>> {
        Box::pin(async {
            Ok(vec![
                AccessPoint {
                    ssid: "Home WiFi".into(),
                    signal: 80,
                    secured: true,
                },
                AccessPoint {
                    ssid: "Guest WiFi".into(),
                    signal: 55,
                    secured: false,
                },
            ])
        })
    }
    fn start_ap<'a>(&'a mut self, _secrets: &'a SetupSecrets) -> Operation<'a, ()> {
        Box::pin(async move {
            self.station = None;
            if self.failing_aps > 0 {
                self.failing_aps -= 1;
                return Err(NetworkError::Unavailable);
            }
            Ok(())
        })
    }
    fn connect<'a>(&'a mut self, ssid: &'a str, password: &'a str) -> Operation<'a, ()> {
        Box::pin(async move {
            if !self.connect_delay.is_zero() {
                tokio::time::sleep(self.connect_delay).await;
            }
            if self.outage {
                return Err(NetworkError::Unavailable);
            }
            if !((ssid == "Home WiFi" && password == "correct-password")
                || (ssid == "Guest WiFi" && password.is_empty()))
            {
                return Err(NetworkError::InvalidCredentials);
            }
            self.station = Some(ssid.into());
            Ok(())
        })
    }
    fn restore<'a>(&'a mut self, config: &'a NetworkConfig) -> Operation<'a, ()> {
        Box::pin(async move {
            if self.outage {
                return Err(NetworkError::Unavailable);
            }
            if self.failing_restores > 0 {
                self.failing_restores -= 1;
                return Err(NetworkError::Unavailable);
            }
            self.station = Some(config.ssid.clone());
            Ok(())
        })
    }
    fn finish(&mut self, _commit: bool) -> Operation<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn connected<'a>(&'a mut self, ssid: &'a str) -> Operation<'a, bool> {
        Box::pin(async move { Ok(self.station.as_deref() == Some(ssid) && !self.outage) })
    }
    fn forget(&mut self) -> Operation<'_, ()> {
        Box::pin(async move {
            self.station = None;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn connecting_status_does_not_wait_for_radio_activation() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "1234").unwrap());
        let service = Arc::new(
            Provisioner::new(
                Box::new(SimulatedNetwork::with_delay(
                    std::time::Duration::from_millis(100),
                )),
                store,
            )
            .await
            .unwrap(),
        );
        let job_service = service.clone();
        let job = tokio::spawn(async move {
            job_service
                .connect("Home WiFi".into(), "correct-password".into())
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while service.cached_status().state != State::Connecting {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_millis(20), service.status())
                .await
                .unwrap()
                .state,
            State::Connecting
        );
        assert_eq!(job.await.unwrap().unwrap().state, State::Connected);
    }
    #[tokio::test]
    async fn offline_station_is_retried_without_ever_starting_the_ap() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "serialA4F2").unwrap());
        store
            .update(|c| {
                c.network = Some(NetworkConfig {
                    ssid: "Home WiFi".into(),
                    extra: Default::default(),
                })
            })
            .unwrap();
        let service = Provisioner::new(Box::new(SimulatedNetwork::with_failing_restores(2)), store)
            .await
            .unwrap();
        assert_eq!(service.status().await.state, State::Offline);
        assert!(!service.retry_restore().await, "radio still not ready");
        assert_eq!(
            service.cached_status().setup_ssid,
            None,
            "no AP while a station is saved"
        );
        assert!(service.retry_restore().await);
        assert_eq!(service.cached_status().state, State::Connected);
        assert!(
            !service.retry_restore().await,
            "nothing to do once connected"
        );
    }
    #[tokio::test]
    async fn a_restart_keeps_an_already_connected_station_without_reactivating_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "serialA4F2").unwrap());
        store
            .update(|c| {
                c.network = Some(NetworkConfig {
                    ssid: "Home WiFi".into(),
                    extra: Default::default(),
                })
            })
            .unwrap();
        // A restore would fail here; being connected already must be enough.
        let mut backend = SimulatedNetwork::already_connected("Home WiFi");
        backend.failing_restores = 1;
        let service = Provisioner::new(Box::new(backend), store).await.unwrap();
        assert_eq!(service.cached_status().state, State::Connected);
    }
    #[tokio::test]
    async fn first_boot_bad_password_connect_outage_recovery_and_reset() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "serialA4F2").unwrap());
        let service = Provisioner::new(Box::<SimulatedNetwork>::default(), store.clone())
            .await
            .unwrap();
        assert_eq!(service.status().await.state, State::Ap);
        assert_eq!(service.scan().await.unwrap().len(), 2);
        assert!(service
            .connect("Home WiFi".into(), "wrong".into())
            .await
            .is_err());
        assert_eq!(service.status().await.state, State::Ap);
        assert!(store.config().network.is_none());
        assert_eq!(
            service
                .connect("Home WiFi".into(), "correct-password".into())
                .await
                .unwrap()
                .state,
            State::Connected
        );
        assert!(service
            .connect("Home WiFi".into(), "wrong".into())
            .await
            .is_err());
        assert_eq!(service.status().await.state, State::Connected);
        let outage = Provisioner::new(Box::new(SimulatedNetwork::with_outage()), store.clone())
            .await
            .unwrap();
        assert_eq!(outage.status().await.state, State::Offline);
        assert!(outage.status().await.setup_ssid.is_none());
        assert_eq!(outage.recovery().await.unwrap().state, State::RecoveryAp);
        assert!(outage
            .connect("Home WiFi".into(), "wrong".into())
            .await
            .is_err());
        assert_eq!(outage.status().await.state, State::RecoveryAp);
        outage.factory_reset("serialA4F2".into()).await.unwrap();
        assert_eq!(outage.status().await.state, State::Ap);
        assert!(store.config().network.is_none());
    }
    #[tokio::test(start_paused = true)]
    async fn a_refused_setup_ap_keeps_the_gateway_up_and_is_retried_with_a_backoff() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "serialA4F2").unwrap());
        let service = Provisioner::new(Box::new(SimulatedNetwork::with_failing_aps(2)), store)
            .await
            .expect("a refused AP must not stop the gateway");
        let status = service.cached_status();
        assert_eq!(status.state, State::Offline);
        assert_eq!(status.setup_ssid, None);
        assert!(status.last_error.unwrap().contains("setup access point"));
        assert!(!service.retry_ap().await, "backoff not yet passed");
        tokio::time::advance(AP_RETRY_FIRST).await;
        assert!(!service.retry_ap().await, "second refusal");
        assert_eq!(service.cached_status().state, State::Offline);
        tokio::time::advance(AP_RETRY_FIRST).await;
        assert!(!service.retry_ap().await, "the backoff doubled");
        tokio::time::advance(AP_RETRY_FIRST).await;
        assert!(service.retry_ap().await);
        let status = service.cached_status();
        assert_eq!(status.state, State::Ap);
        assert_eq!(status.setup_ssid.as_deref(), Some("NIBE-Gateway-A4F2"));
        assert_eq!(status.last_error, None);
        assert!(
            !service.retry_ap().await,
            "nothing pending once the AP is up"
        );
    }
    #[tokio::test(start_paused = true)]
    async fn an_offline_station_never_gets_an_ap_from_the_ap_retry() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "serialA4F2").unwrap());
        store
            .update(|c| {
                c.network = Some(NetworkConfig {
                    ssid: "Home WiFi".into(),
                    extra: Default::default(),
                })
            })
            .unwrap();
        let service = Provisioner::new(Box::new(SimulatedNetwork::with_outage()), store)
            .await
            .unwrap();
        tokio::time::advance(AP_RETRY_MAX).await;
        assert!(!service.retry_ap().await);
        assert_eq!(service.cached_status().setup_ssid, None);
    }
    #[tokio::test(start_paused = true)]
    async fn a_factory_reset_whose_ap_is_refused_retries_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "serialA4F2").unwrap());
        // The first boot's AP and the reset's AP are both refused.
        let service = Provisioner::new(Box::new(SimulatedNetwork::with_failing_aps(2)), store)
            .await
            .unwrap();
        assert!(service.factory_reset("serialA4F2".into()).await.is_err());
        assert_eq!(service.cached_status().state, State::Offline);
        tokio::time::advance(AP_RETRY_FIRST).await;
        assert!(service.retry_ap().await, "the reset restarted the backoff");
        assert_eq!(service.cached_status().state, State::Ap);
    }
}
