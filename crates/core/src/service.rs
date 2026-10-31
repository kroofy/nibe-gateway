use crate::*;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

pub type DeviceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ServiceError>> + Send + 'a>>;

/// WS04 transport seam. The protocol task owns timing/ACKs; HTTP never owns the wire.
/// A real implementation must bound buffers and service ACKs even without API consumers.
pub trait DeviceTransport: Send {
    fn receive(&mut self) -> DeviceFuture<'_, Vec<u8>>;
    fn send<'a>(&'a mut self, frame: &'a [u8]) -> DeviceFuture<'a, ()>;
}

/// WS04 supplies F730/replay implementations. Poll must be cancellation safe: a timed-out
/// poll may be dropped; wire-critical work belongs to an independent transport task.
pub trait HeatingOffsetWriter: Send + Sync {
    fn write_offset(&self, offset: i8) -> DeviceFuture<'_, CommandReceipt>;
}

pub trait Device: Send + 'static {
    /// None means no asynchronous write outcome is available.
    fn offset_completion(&self) -> Option<(u64, bool)> {
        None
    }
    fn mode(&self) -> DeviceMode;
    fn poll(&mut self, now_ms: u64) -> DeviceFuture<'_, DeviceUpdate>;
    fn execute(&mut self, command: Command, now_ms: u64) -> DeviceFuture<'_, String>;
    fn discover(&mut self) -> DeviceFuture<'_, DiscoveryStatus>;
    #[cfg(feature = "dev")]
    fn dev_action(&mut self, _action: DevAction) -> Result<(), ServiceError> {
        Err(ServiceError::new(
            ErrorCode::Unsupported,
            "development actions unsupported by this adapter",
        ))
    }
}

#[derive(Debug, Clone)]
pub struct DeviceUpdate {
    pub identity: DeviceIdentity,
    pub connection: ConnectionState,
    pub network: NetworkState,
    pub catalog: Vec<RegisterDefinition>,
    /// None preserves the cached readings, letting them age naturally.
    pub readings: Option<Vec<RegisterReading>>,
}

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}
/// Wall-clock milliseconds that never go backwards. The bus runtime times tokens and timeouts with this clock,
/// so it must be monotonic; but timestamps it produces are shown and recorded as real time. A Pi without an RTC
/// starts before NTP sync (old time), so when the wall clock runs ahead of this clock by more than
/// `CATCH_UP_MS` (the boot-time NTP step) the clock jumps forward once to real time. It never steps back:
/// backward wall-clock steps are absorbed by the monotonic part.
pub struct SystemClock {
    start: Instant,
    epoch_ms: std::sync::atomic::AtomicU64,
    wall: fn() -> u64,
}
const CATCH_UP_MS: u64 = 60_000;
fn system_wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
impl Default for SystemClock {
    fn default() -> Self {
        Self::with_wall(system_wall_ms)
    }
}
impl SystemClock {
    /// Test seam: the wall-clock source.
    pub fn with_wall(wall: fn() -> u64) -> Self {
        Self {
            start: Instant::now(),
            epoch_ms: std::sync::atomic::AtomicU64::new(wall()),
            wall,
        }
    }
}
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        use std::sync::atomic::Ordering;
        let elapsed = self.start.elapsed().as_millis() as u64;
        let monotonic = self
            .epoch_ms
            .load(Ordering::Relaxed)
            .saturating_add(elapsed);
        let wall = (self.wall)();
        if wall > monotonic.saturating_add(CATCH_UP_MS) {
            self.epoch_ms
                .store(wall.saturating_sub(elapsed), Ordering::Relaxed);
            return wall;
        }
        monotonic
    }
}

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub poll_interval: Duration,
    pub adapter_timeout: Duration,
    pub request_timeout: Duration,
    pub stale_after_ms: u64,
    pub queue_capacity: usize,
    pub event_capacity: usize,
    /// Effective only in a dev-feature build with a Simulated adapter.
    pub allow_simulated_controls: bool,
    /// Owner opt-in for named controls on a Hardware adapter (ADR 0104); the adapter still gates registers.
    pub hardware_controls: bool,
}
impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            adapter_timeout: Duration::from_millis(500),
            request_timeout: Duration::from_secs(2),
            stale_after_ms: 5000,
            queue_capacity: 16,
            event_capacity: 32,
            allow_simulated_controls: false,
            hardware_controls: false,
        }
    }
}

enum Request {
    Command(
        Command,
        oneshot::Sender<Result<CommandReceipt, ServiceError>>,
    ),
    Discover(oneshot::Sender<Result<DiscoveryStatus, ServiceError>>),
    Notify(String, String),
    #[cfg(feature = "dev")]
    Dev(DevAction, oneshot::Sender<Result<(), ServiceError>>),
}
#[derive(Clone)]
pub struct Gateway {
    offset_writer: Arc<RwLock<Option<std::sync::Weak<dyn HeatingOffsetWriter>>>>,
    offset_completion: Arc<RwLock<Option<(u64, bool)>>>,
    cache: Arc<RwLock<DeviceSnapshot>>,
    clock: Arc<dyn Clock>,
    requests: mpsc::Sender<Request>,
    events: broadcast::Sender<GatewayEvent>,
    history: Arc<RwLock<VecDeque<CommandReceipt>>>,
    request_timeout: Duration,
    stopped: watch::Receiver<bool>,
    heartbeat: Arc<RwLock<Option<Instant>>>,
    heartbeat_limit: Duration,
}
impl Gateway {
    /// Application readiness uses host monotonic time, independent of simulated
    /// device time/freshness. Adapter failures still advance the core heartbeat.
    pub fn is_ready(&self) -> bool {
        !*self.stopped.borrow()
            && !self.requests.is_closed()
            && self
                .heartbeat
                .read()
                .expect("heartbeat lock")
                .is_some_and(|last| last.elapsed() <= self.heartbeat_limit)
    }
    pub fn snapshot(&self) -> DeviceSnapshot {
        let mut snapshot = self.cache.read().expect("cache lock poisoned").clone();
        snapshot.refresh(self.clock.now_ms());
        snapshot
    }
    /// Current gateway clock time (the same clock that stamps readings and receipts).
    pub fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }
    /// Publish a gateway-side event (for example a lease restore) on the SSE stream. Best effort.
    pub fn notify(&self, kind: &str, detail: &str) {
        let _ = self.enqueue(Request::Notify(kind.into(), detail.into()));
    }
    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.stopped.clone()
    }
    pub fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.events.subscribe()
    }
    pub fn record_event_lag(&self) {
        self.cache
            .write()
            .expect("cache lock poisoned")
            .diagnostics
            .event_receivers_lagged += 1;
    }
    pub fn command_history(&self) -> Vec<CommandReceipt> {
        self.history
            .read()
            .expect("history lock poisoned")
            .iter()
            .cloned()
            .collect()
    }
    pub async fn command(&self, command: Command) -> Result<CommandReceipt, ServiceError> {
        if let Command::HeatingOffset { offset } = command {
            let writer = self
                .offset_writer
                .read()
                .unwrap()
                .as_ref()
                .and_then(|w| w.upgrade());
            if let Some(writer) = writer {
                return writer.write_offset(offset).await;
            }
        }
        self.dispatch_command(command).await
    }
    pub fn set_offset_writer(&self, writer: std::sync::Weak<dyn HeatingOffsetWriter>) {
        *self.offset_writer.write().unwrap() = Some(writer);
    }
    pub fn offset_completion(&self) -> Option<(u64, bool)> {
        *self.offset_completion.read().unwrap()
    }
    /// Internal dispatch for the serialized named-control coordinator.
    pub async fn dispatch_command(&self, command: Command) -> Result<CommandReceipt, ServiceError> {
        command.validate()?;
        if !self.snapshot().controls_enabled {
            return Err(ServiceError::new(
                ErrorCode::ControlsDisabled,
                "controls disabled: hardware needs open writes, or allow_hardware_writes with writable_registers; dev builds can enable simulated controls",
            ));
        }
        let (tx, rx) = oneshot::channel();
        self.enqueue(Request::Command(command, tx))?;
        self.response(rx).await
    }
    pub async fn discover(&self) -> Result<DiscoveryStatus, ServiceError> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Request::Discover(tx))?;
        self.response(rx).await
    }
    #[cfg(feature = "dev")]
    pub async fn dev_action(&self, action: DevAction) -> Result<(), ServiceError> {
        if self.snapshot().mode != DeviceMode::Simulated {
            return Err(ServiceError::new(
                ErrorCode::Forbidden,
                "development actions require simulation",
            ));
        }
        let (tx, rx) = oneshot::channel();
        self.enqueue(Request::Dev(action, tx))?;
        self.response(rx).await
    }
    fn enqueue(&self, request: Request) -> Result<(), ServiceError> {
        self.requests
            .try_send(request)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    ServiceError::new(ErrorCode::Busy, "device request queue is full")
                }
                mpsc::error::TrySendError::Closed(_) => {
                    ServiceError::new(ErrorCode::Unavailable, "device service stopped")
                }
            })
    }
    async fn response<T>(
        &self,
        rx: oneshot::Receiver<Result<T, ServiceError>>,
    ) -> Result<T, ServiceError> {
        tokio::time::timeout(self.request_timeout, rx).await
            .map_err(|_| ServiceError::new(ErrorCode::Timeout, "device request deadline elapsed; outcome may be unknown, do not retry writes blindly"))?
            .map_err(|_| ServiceError::new(ErrorCode::Unavailable, "device service stopped"))?
    }
}

pub struct DeviceService {
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}
impl DeviceService {
    pub async fn shutdown(mut self) {
        let _ = self.stop.send(true);
        let _ = (&mut self.task).await;
    }
    pub fn shutdown_trigger(&self) -> impl FnOnce() + Send + 'static {
        let stop = self.stop.clone();
        move || {
            let _ = stop.send(true);
        }
    }
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}
impl Drop for DeviceService {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

pub fn start_device(
    mut device: Box<dyn Device>,
    clock: Arc<dyn Clock>,
    config: ServiceConfig,
) -> (Gateway, DeviceService) {
    let mode = device.mode();
    let controls_enabled =
        (cfg!(feature = "dev") && config.allow_simulated_controls && mode == DeviceMode::Simulated)
            || (config.hardware_controls && mode == DeviceMode::Hardware);
    let (requests, mut rx) = mpsc::channel(config.queue_capacity.max(1));
    let (events, _) = broadcast::channel(config.event_capacity.max(1));
    let (stop, mut stopped) = watch::channel(false);
    let cache = Arc::new(RwLock::new(DeviceSnapshot {
        mode,
        identity: None,
        connection: ConnectionState::Connecting,
        network: NetworkState::Unmanaged,
        catalog: vec![],
        registers: vec![],
        measurements: vec![],
        discovery: DiscoveryStatus::default(),
        diagnostics: Diagnostics::default(),
        now_ms: clock.now_ms(),
        last_update_ms: None,
        stale_after_ms: config.stale_after_ms,
        stale: true,
        last_error: None,
        controls_enabled,
    }));
    let gateway = Gateway {
        offset_writer: Arc::new(RwLock::new(None)),
        offset_completion: Arc::new(RwLock::new(None)),
        cache,
        clock,
        requests,
        events,
        history: Arc::default(),
        request_timeout: config.request_timeout,
        stopped: stopped.clone(),
        heartbeat: Arc::new(RwLock::new(None)),
        heartbeat_limit: config
            .poll_interval
            .saturating_add(config.adapter_timeout)
            .saturating_mul(3),
    };
    let state = gateway.clone();
    let task = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(config.poll_interval.max(Duration::from_millis(1)));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut sequence = 0;
        loop {
            tokio::select! {
                _ = stopped.changed() => break,
                _ = interval.tick() => {
                    poll_device(device.as_mut(), &state, config.adapter_timeout).await;
                    emit(&state, &mut sequence, "state", "device cache refreshed");
                }
                Some(request) = rx.recv() => {
                    handle_request(device.as_mut(), &state, request, config.adapter_timeout, &mut sequence).await;
                }
            }
        }
        let mut snapshot = state.cache.write().expect("cache lock poisoned");
        snapshot.connection = ConnectionState::Disconnected;
        snapshot.last_error = Some("device service stopped".into());
        snapshot.refresh(state.clock.now_ms());
    });
    (gateway, DeviceService { stop, task })
}
async fn handle_request(
    device: &mut dyn Device,
    state: &Gateway,
    request: Request,
    timeout: Duration,
    sequence: &mut u64,
) {
    match request {
        Request::Command(command, response) => {
            // Dropped HTTP requests queued before execution do not reach the device.
            if response.is_closed() {
                return;
            }
            let result = deadline(
                timeout,
                device.execute(command.clone(), state.clock.now_ms()),
            )
            .await;
            if matches!(command, Command::HeatingOffset { .. }) {
                *state.offset_completion.write().unwrap() =
                    if state.snapshot().mode == DeviceMode::Simulated {
                        Some((state.now_ms(), result.is_ok()))
                    } else {
                        device.offset_completion()
                    };
            }
            let result = {
                let mut snapshot = state.cache.write().expect("cache lock poisoned");
                let result = result.map(|detail| {
                    snapshot.diagnostics.commands_accepted += 1;
                    CommandReceipt {
                        sequence: snapshot.diagnostics.commands_accepted,
                        command,
                        accepted_at_ms: state.clock.now_ms(),
                        detail,
                    }
                });
                if result.is_err() {
                    snapshot.diagnostics.commands_rejected += 1;
                }
                result
            };
            if let Ok(receipt) = &result {
                let mut history = state.history.write().expect("history lock poisoned");
                if history.len() == 128 {
                    history.pop_front();
                }
                history.push_back(receipt.clone());
            }
            let _ = response.send(result);
            emit(
                state,
                sequence,
                "command",
                "command completed; query cache for current state",
            );
        }
        Request::Discover(response) => {
            if response.is_closed() {
                return;
            }
            let result = deadline(timeout, device.discover()).await;
            if let Ok(status) = &result {
                state.cache.write().expect("cache lock poisoned").discovery = status.clone();
            }
            let _ = response.send(result);
            emit(state, sequence, "discovery", "passive discovery completed");
        }
        Request::Notify(kind, detail) => emit(state, sequence, &kind, &detail),
        #[cfg(feature = "dev")]
        Request::Dev(action, response) => {
            if response.is_closed() {
                return;
            }
            let reset = matches!(action, DevAction::Reset);
            let result = device.dev_action(action);
            if result.is_ok() {
                if reset {
                    state
                        .history
                        .write()
                        .expect("history lock poisoned")
                        .clear();
                    let mut snapshot = state.cache.write().expect("cache lock poisoned");
                    snapshot.diagnostics = Diagnostics::default();
                    snapshot.registers.clear();
                    snapshot.last_update_ms = None;
                    snapshot.discovery = DiscoveryStatus::default();
                }
                poll_device(device, state, timeout).await;
                emit(state, sequence, "scenario", "simulation updated");
            }
            let _ = response.send(result);
        }
    }
}
async fn deadline<T>(duration: Duration, future: DeviceFuture<'_, T>) -> Result<T, ServiceError> {
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| ServiceError::new(ErrorCode::Timeout, "adapter operation timed out"))?
}
async fn poll_device(device: &mut dyn Device, state: &Gateway, timeout: Duration) {
    if state.snapshot().mode != DeviceMode::Simulated {
        *state.offset_completion.write().unwrap() = device.offset_completion();
    }
    let now = state.clock.now_ms();
    let result = deadline(timeout, device.poll(now)).await;
    let mut snapshot = state.cache.write().expect("cache lock poisoned");
    snapshot.diagnostics.polls += 1;
    *state.heartbeat.write().expect("heartbeat lock") = Some(Instant::now());
    match result {
        Ok(update) => {
            snapshot.identity = Some(update.identity);
            snapshot.connection = update.connection;
            snapshot.network = update.network;
            snapshot.catalog = update.catalog;
            if let Some(readings) = update.readings {
                snapshot.registers = readings;
                snapshot.last_update_ms = snapshot
                    .registers
                    .iter()
                    .map(|reading| reading.observed_at_ms)
                    .max();
                snapshot.diagnostics.updates += 1;
            }
            snapshot.discovery.observed_count = snapshot.registers.len();
            snapshot.last_error = None;
        }
        Err(error) => {
            snapshot.diagnostics.transport_errors += 1;
            match error.code {
                ErrorCode::Timeout => snapshot.diagnostics.timeouts += 1,
                ErrorCode::Malformed => snapshot.diagnostics.malformed_frames += 1,
                _ => {}
            }
            snapshot.connection = ConnectionState::Disconnected;
            snapshot.last_error = Some(error.to_string());
        }
    }
    snapshot.refresh(state.clock.now_ms());
    // Explicit numeric/enum allowlist: verbose diagnostics never log device
    // identities, credentials, raw frames, free-text errors or register values.
    tracing::debug!(polls=snapshot.diagnostics.polls, updates=snapshot.diagnostics.updates,
        errors=snapshot.diagnostics.transport_errors, connection=?snapshot.connection,
        "device poll completed");
}
fn emit(state: &Gateway, sequence: &mut u64, kind: &str, detail: &str) {
    *sequence += 1;
    // Broadcast never awaits. Lagging SSE clients lose old events and refetch the cache.
    let _ = state.events.send(GatewayEvent {
        sequence: *sequence,
        at_ms: state.clock.now_ms(),
        kind: kind.into(),
        detail: detail.into(),
    });
}

#[cfg(test)]
mod tests {
    static FAKE_WALL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1_000);
    fn fake_wall() -> u64 {
        FAKE_WALL.load(std::sync::atomic::Ordering::SeqCst)
    }
    #[test]
    fn system_clock_catches_up_with_ntp_once_and_never_goes_back() {
        use std::sync::atomic::Ordering::SeqCst;
        FAKE_WALL.store(1_000, SeqCst);
        let clock = SystemClock::with_wall(fake_wall);
        // Small differences (drift, a slewing NTP) are ignored: the clock stays monotonic.
        FAKE_WALL.store(31_000, SeqCst);
        assert!(clock.now_ms() < 2_000);
        // The boot-time NTP step (wall clock far ahead): follow it once.
        FAKE_WALL.store(9_000_000, SeqCst);
        let caught_up = clock.now_ms();
        assert!((9_000_000..9_001_000).contains(&caught_up));
        // A later backward step is absorbed; time keeps moving forward from the new anchor.
        FAKE_WALL.store(5_000_000, SeqCst);
        assert!(clock.now_ms() >= caught_up);
    }

    use super::*;
    struct UncataloguedHardware;
    impl Device for UncataloguedHardware {
        fn mode(&self) -> DeviceMode {
            DeviceMode::Hardware
        }
        fn poll(&mut self, now_ms: u64) -> DeviceFuture<'_, DeviceUpdate> {
            Box::pin(async move {
                Ok(DeviceUpdate {
                    identity: DeviceIdentity {
                        model: "adapter contract fixture".into(),
                        firmware: None,
                        serial: None,
                        source: "test".into(),
                        verification: VerificationState::Unverified,
                    },
                    connection: ConnectionState::Connected,
                    network: NetworkState::Unmanaged,
                    catalog: vec![],
                    readings: Some(vec![RegisterReading {
                        id: 100_000,
                        raw: Some(RegisterValue::Integer(-123)),
                        decoded: None,
                        observed_at_ms: now_ms,
                        source: "unknown frame".into(),
                        observation: ObservationState::Unknown,
                        error: Some("no decoder installed".into()),
                        max_age_ms: None,
                    }]),
                })
            })
        }
        fn execute(&mut self, _: Command, _: u64) -> DeviceFuture<'_, String> {
            panic!("unverified hardware must never receive a command")
        }
        fn discover(&mut self) -> DeviceFuture<'_, DiscoveryStatus> {
            Box::pin(async { Ok(DiscoveryStatus::default()) })
        }
    }
    #[tokio::test]
    async fn unknown_registers_remain_visible_and_hardware_writes_stay_disabled() {
        let config = ServiceConfig {
            allow_simulated_controls: true,
            ..ServiceConfig::default()
        };
        let (gateway, service) = start_device(
            Box::new(UncataloguedHardware),
            Arc::new(SystemClock::default()),
            config,
        );
        tokio::time::timeout(Duration::from_secs(1), async {
            while gateway.snapshot().registers.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(gateway.is_ready());
        *gateway.heartbeat.write().unwrap() = Some(Instant::now() - Duration::from_secs(60));
        assert!(!gateway.is_ready());
        *gateway.heartbeat.write().unwrap() = Some(Instant::now());
        // A stale physical reading does not make an otherwise live core unready.
        gateway.cache.write().unwrap().connection = ConnectionState::Disconnected;
        assert!(gateway.is_ready());
        let snapshot = gateway.snapshot();
        assert_eq!(
            snapshot.registers[0].raw,
            Some(RegisterValue::Integer(-123))
        );
        assert_eq!(snapshot.measurements[0].name, "unknown_100000");
        assert!(snapshot.measurements[0].value.is_none());
        assert!(snapshot.measurements[0].stale);
        assert_eq!(
            gateway
                .command(Command::HeatingOffset { offset: 0 })
                .await
                .unwrap_err()
                .code,
            ErrorCode::ControlsDisabled
        );
        service.shutdown().await;
        assert!(!gateway.is_ready());
        assert_eq!(gateway.snapshot().connection, ConnectionState::Disconnected);
    }
}
