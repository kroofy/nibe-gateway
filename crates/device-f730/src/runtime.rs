use crate::{
    catalog::Catalog,
    decode::{Observer, WordOrder},
    discovery::{ProbeEngine, ProbeStatus, RegisterState},
    transport::{Capture, CaptureHeader, Recorder, ReplayTransport},
    writes::WriteEngine,
};
use nibe_core::*;
use nibe_protocol::{Decoder, Event, Frame, MODBUS40};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch};
use utoipa::ToSchema;

/// Hardware hypotheses; tune only from HP-21 measurements.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WirePolicy {
    pub reply_deadline_ms: u64,
    pub tolerated_consecutive_misses: u32,
    pub reopen_attempts: u32,
    pub reopen_backoff_ms: u64,
    pub frame_timeout_ms: u64,
    pub thread_nice: i32,
}
impl Default for WirePolicy {
    fn default() -> Self {
        Self {
            reply_deadline_ms: 10,
            tolerated_consecutive_misses: 3,
            reopen_attempts: 5,
            reopen_backoff_ms: 250,
            frame_timeout_ms: 60_000,
            thread_nice: -5,
        }
    }
}
impl WirePolicy {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if !(1..=1000).contains(&self.reply_deadline_ms)
            || !(1..=100).contains(&self.tolerated_consecutive_misses)
            || !(1..=10).contains(&self.reopen_attempts)
            || !(1..=5000).contains(&self.reopen_backoff_ms)
            || !(1000..=300_000).contains(&self.frame_timeout_ms)
            || !(-20..=19).contains(&self.thread_nice)
        {
            return Err(ServiceError::validation(
                "wire policy outside supported bounds",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WireHealth {
    pub device_fatal: bool,
    pub device_degraded: bool,
    pub recovering: bool,
    pub last_frame_age_ms: Option<u64>,
    pub reopen_attempts: u64,
    pub priority_applied: bool,
    /// Complete poll rounds with every read answered, since start (ADR 0109).
    pub poll_rounds: u64,
}
struct WireState {
    started: Instant,
    last_frame: Option<Instant>,
    fatal: bool,
    recovering: bool,
    misses: u32,
    reopen_attempts: u64,
    priority_applied: bool,
}
impl Default for WireState {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            last_frame: None,
            fatal: false,
            recovering: false,
            misses: 0,
            reopen_attempts: 0,
            priority_applied: false,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct WireDiagnostics {
    pub frames_for_us: u64,
    pub frames_for_other_addresses: u64,
    pub checksum_errors: u64,
    pub invalid_escapes: u64,
    pub malformed_payloads: u64,
    pub garbage_bytes: u64,
    pub acks_received: u64,
    pub naks_received: u64,
    pub acks_sent: u64,
    pub naks_sent: u64,
    pub read_requests_sent: u64,
    #[serde(default)]
    pub write_requests_sent: u64,
    pub observations_dropped: u64,
    pub ack_deadline_misses: u64,
    pub max_reply_us: u64,
    pub capture_dropped: u64,
    pub capture_failed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Capabilities {
    pub writes: bool,
    pub raw_writes: bool,
    pub rmu40_emulation: bool,
    pub passive_discovery: bool,
    pub active_probing: bool,
    pub hardware_verified: bool,
}
impl Capabilities {
    fn new(mode: DeviceMode, hardware_reads: bool) -> Self {
        Self {
            writes: false,
            raw_writes: false,
            rmu40_emulation: false,
            passive_discovery: true,
            active_probing: mode == DeviceMode::Replay
                || (mode == DeviceMode::Hardware && hardware_reads),
            hardware_verified: false,
        }
    }
}
#[derive(Debug, Clone)]
pub struct AdapterSnapshot {
    pub catalog: Arc<Catalog>,
    pub identity: Option<DeviceIdentity>,
    pub word_order: WordOrder,
    pub readings: BTreeMap<u16, RegisterReading>,
    pub probe: ProbeStatus,
    pub diagnostics: WireDiagnostics,
    pub capabilities: Capabilities,
    pub connection: ConnectionState,
    pub last_error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExplorerEntry {
    pub id: u16,
    pub catalog_known: bool,
    pub state: RegisterState,
    pub verification: String,
    pub definition: Option<crate::catalog::Entry>,
    pub reading: Option<RegisterReading>,
    pub writable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Explorer {
    pub items: Vec<ExplorerEntry>,
    pub catalog_version: String,
    pub word_order: WordOrder,
    pub capabilities: Capabilities,
    pub probe: ProbeStatus,
    pub diagnostics: WireDiagnostics,
    pub last_error: Option<String>,
}
enum CommandRequest {
    Probe(
        Vec<u16>,
        bool,
        oneshot::Sender<Result<ProbeStatus, ServiceError>>,
    ),
    Cancel(oneshot::Sender<Result<ProbeStatus, ServiceError>>),
    Write(u16, i32, oneshot::Sender<Result<(), ServiceError>>),
    Read(u16, oneshot::Sender<Result<(), ServiceError>>),
    Polls(
        Vec<u16>,
        Vec<u16>,
        oneshot::Sender<Result<(), ServiceError>>,
    ),
    AlarmReset(oneshot::Sender<Result<(), ServiceError>>),
    Import(Arc<Catalog>, oneshot::Sender<Result<(), ServiceError>>),
}
enum Observation {
    Frame(Frame, u64, Option<u16>),
    Stats(WireDiagnostics, Box<ProbeStatus>),
    Error(String),
    Catalog(Arc<Catalog>),
}
struct Lifecycle {
    stop: watch::Sender<bool>,
}
impl Drop for Lifecycle {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}
#[derive(Clone)]
pub struct F730Handle {
    snapshots: watch::Receiver<Arc<AdapterSnapshot>>,
    commands: mpsc::Sender<CommandRequest>,
    lifecycle: Arc<Lifecycle>,
    finished: watch::Receiver<bool>,
    wire: Arc<RwLock<WireState>>,
    policy: WirePolicy,
    hardware: bool,
}
impl F730Handle {
    pub fn wire_health(&self) -> WireHealth {
        let state = self.wire.read().expect("wire health lock");
        let age = state.last_frame.map(|t| t.elapsed().as_millis() as u64);
        WireHealth {
            device_fatal: state.fatal
                || (*self.finished.borrow() && !*self.lifecycle.stop.borrow()),
            device_degraded: state.fatal
                || state.recovering
                || state.misses > 0
                || (self.hardware
                    && age.unwrap_or(state.started.elapsed().as_millis() as u64)
                        > self.policy.frame_timeout_ms),
            recovering: state.recovering,
            last_frame_age_ms: age,
            reopen_attempts: state.reopen_attempts,
            priority_applied: state.priority_applied,
            poll_rounds: self.snapshots.borrow().probe.poll.rounds_ok,
        }
    }
    pub async fn wait_fatal(&self) {
        let mut finished = self.finished.clone();
        if !*finished.borrow() {
            let _ = finished.changed().await;
        }
    }

    pub fn snapshot(&self) -> Arc<AdapterSnapshot> {
        self.snapshots.borrow().clone()
    }
    pub fn explorer(&self) -> Explorer {
        let s = self.snapshot();
        let mut items = vec![];
        for e in &s.catalog.registers {
            let reading = s.readings.get(&e.id).cloned();
            let state = match reading.as_ref().map(|r| r.observation) {
                Some(ObservationState::ConfirmedReadable) => RegisterState::ConfirmedReadable,
                Some(_) => RegisterState::Observed,
                None => s
                    .probe
                    .states
                    .get(&e.id)
                    .copied()
                    .unwrap_or(RegisterState::Unverified),
            };
            items.push(ExplorerEntry {
                id: e.id,
                catalog_known: true,
                state,
                verification: e.verification.clone(),
                definition: Some(e.clone()),
                reading,
                writable: false,
            });
        }
        for (&id, r) in &s.readings {
            if s.catalog.entry(id).is_none() {
                items.push(ExplorerEntry {
                    id,
                    catalog_known: false,
                    state: RegisterState::Unknown,
                    verification: "unverified".into(),
                    definition: None,
                    reading: Some(r.clone()),
                    writable: false,
                });
            }
        }
        Explorer {
            items,
            catalog_version: s.catalog.catalog_version.clone(),
            word_order: s.word_order,
            capabilities: s.capabilities.clone(),
            probe: s.probe.clone(),
            diagnostics: s.diagnostics.clone(),
            last_error: s.last_error.clone(),
        }
    }
    pub async fn probe(
        &self,
        ids: Vec<u16>,
        authenticated: bool,
    ) -> Result<ProbeStatus, ServiceError> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(CommandRequest::Probe(ids, authenticated, tx))?;
        response(rx).await
    }
    /// Queue one gated write (ADR 0104). Ok means queued; the outcome is in `probe.writes.last`.
    pub async fn write(&self, register: u16, raw: i32) -> Result<(), ServiceError> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(CommandRequest::Write(register, raw, tx))?;
        response(rx).await
    }
    /// Queue one read of any register beside the polls (ADR 0126). The value arrives as a reading.
    pub async fn read_once(&self, register: u16) -> Result<(), ServiceError> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(CommandRequest::Read(register, tx))?;
        response(rx).await
    }
    /// Replace the fast and slow poll lists at runtime; same rules and 64-register limit as the config.
    pub async fn set_polls(&self, fast: Vec<u16>, slow: Vec<u16>) -> Result<(), ServiceError> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(CommandRequest::Polls(fast, slow, tx))?;
        response(rx).await
    }
    /// Queue the gated alarm reset (ADR 0105): a 0 -> 1 edge on 45171, then read-backs of 45001 and 45171.
    pub async fn reset_alarm(&self) -> Result<(), ServiceError> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(CommandRequest::AlarmReset(tx))?;
        response(rx).await
    }
    pub async fn cancel(&self) -> Result<ProbeStatus, ServiceError> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(CommandRequest::Cancel(tx))?;
        response(rx).await
    }
    pub async fn import(&self, catalog: Catalog) -> Result<(), ServiceError> {
        catalog.validate()?;
        let (tx, rx) = oneshot::channel();
        self.enqueue(CommandRequest::Import(Arc::new(catalog), tx))?;
        response(rx).await
    }
    pub async fn shutdown_and_wait(&self) -> Result<(), ServiceError> {
        self.shutdown();
        let mut finished = self.finished.clone();
        if *finished.borrow() {
            return Ok(());
        }
        tokio::time::timeout(Duration::from_secs(2), finished.changed())
            .await
            .map_err(|_| {
                ServiceError::new(
                    ErrorCode::Timeout,
                    "wire/capture shutdown did not complete in 2s",
                )
            })?
            .map_err(|_| {
                ServiceError::new(ErrorCode::Unavailable, "wire completion channel closed")
            })
    }
    pub fn shutdown(&self) {
        let _ = self.lifecycle.stop.send(true);
    }
    fn enqueue(&self, command: CommandRequest) -> Result<(), ServiceError> {
        self.commands.try_send(command).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => {
                ServiceError::new(ErrorCode::Busy, "adapter command queue full")
            }
            mpsc::error::TrySendError::Closed(_) => {
                ServiceError::new(ErrorCode::Unavailable, "wire task stopped")
            }
        })
    }
}
async fn response<T>(rx: oneshot::Receiver<Result<T, ServiceError>>) -> Result<T, ServiceError> {
    tokio::time::timeout(Duration::from_millis(500), rx)
        .await
        .map_err(|_| ServiceError::new(ErrorCode::Timeout, "adapter request timed out"))?
        .map_err(|_| ServiceError::new(ErrorCode::Unavailable, "wire task stopped"))?
}
/// Heat offset S1 (s8, -10..=10): the `heating-offset` control.
/// Temporary Lux (ADR 0113); writable only once allowlisted after HP-32.
pub const TEMPORARY_LUX: u16 = 48132;
pub const HEAT_OFFSET_S1: u16 = 47011;
pub struct F730Device {
    handle: F730Handle,
    mode: DeviceMode,
}
fn offset_completion(status: &crate::writes::WriteStatus) -> Option<(u64, bool)> {
    let w = status.last_offset.as_ref()?;
    if w.register != HEAT_OFFSET_S1 {
        return None;
    }
    match w.state {
        crate::writes::WriteState::Accepted => Some((w.updated_at_ms, true)),
        crate::writes::WriteState::Denied | crate::writes::WriteState::Timeout => {
            Some((w.updated_at_ms, false))
        }
        _ => None,
    }
}
impl Device for F730Device {
    fn offset_completion(&self) -> Option<(u64, bool)> {
        offset_completion(&self.handle.snapshot().probe.writes)
    }

    fn mode(&self) -> DeviceMode {
        self.mode
    }
    fn poll(&mut self, _: u64) -> DeviceFuture<'_, DeviceUpdate> {
        Box::pin(async move {
            let s = self.handle.snapshot();
            if s.connection == ConnectionState::Disconnected {
                if let Some(error) = &s.last_error {
                    return Err(ServiceError::new(ErrorCode::Disconnected, error.clone()));
                }
            }
            Ok(DeviceUpdate {
                identity: s.identity.clone().unwrap_or(DeviceIdentity {
                    model: "Unknown (awaiting product info)".into(),
                    firmware: None,
                    serial: None,
                    source: "NIBE bus; catalog is not identity evidence".into(),
                    verification: VerificationState::Unverified,
                }),
                connection: s.connection,
                network: NetworkState::Unmanaged,
                catalog: s.catalog.definitions(),
                // Live readings are the poll list; one-off probe results stay in the explorer, so
                // they neither make the snapshot stale nor leak into MQTT or history. Without a poll
                // list (passive replay), every observation is live.
                readings: Some({
                    let poll = &s.probe.poll;
                    // A slow register counts as stale after three missed intervals.
                    let slow_max_age = poll.slow_interval_s.saturating_mul(3_000);
                    s.readings
                        .values()
                        .filter_map(|r| {
                            let id = u16::try_from(r.id).ok()?;
                            if poll.registers.is_empty() && poll.slow_registers.is_empty()
                                || poll.registers.contains(&id)
                            {
                                Some(r.clone())
                            } else if poll.slow_registers.contains(&id) {
                                let mut r = r.clone();
                                r.max_age_ms = Some(slow_max_age);
                                Some(r)
                            } else {
                                None
                            }
                        })
                        .collect()
                }),
            })
        })
    }
    fn execute(&mut self, command: Command, _: u64) -> DeviceFuture<'_, String> {
        Box::pin(async move {
            // Only named controls map to registers; there is no generic register write.
            let (register, raw) = match command {
                Command::HeatingOffset { offset } => (HEAT_OFFSET_S1, i32::from(offset)),
                Command::AlarmReset => {
                    self.handle.reset_alarm().await?;
                    return Ok("alarm reset queued; confirm with probe.writes.last and the 45001 read-back".into());
                }
                Command::TemporaryLux { mode } => (TEMPORARY_LUX, mode.raw()),
            };
            self.handle.write(register, raw).await?;
            Ok(format!(
                "write {register}={raw} queued; confirm with probe.writes.last and the read-back value"
            ))
        })
    }
    fn discover(&mut self) -> DeviceFuture<'_, DiscoveryStatus> {
        Box::pin(async move {
            let s = self.handle.snapshot();
            Ok(DiscoveryStatus{state:"passive".into(),observed_count:s.readings.len(),active_probing_supported:s.capabilities.active_probing,detail:if s.capabilities.active_probing {"Passive observations do not establish firmware support; read requests confirm readability, never writes"} else {"Passive observations do not establish firmware support; hardware read requests need --allow-hardware-reads"}.into()})
        })
    }
}
pub enum TransportConfig {
    Replay(Capture),
    Hardware(String),
}
pub struct AdapterConfig {
    pub transport: TransportConfig,
    pub catalog: Catalog,
    pub word_order: WordOrder,
    pub capture_path: Option<PathBuf>,
    pub wire: WirePolicy,
    /// Explicit opt-in for bounded read requests on hardware (ADR 0101). Ignored for replay.
    pub hardware_reads: bool,
    /// Registers read continuously, one per free read token (ADR 0102).
    pub poll_registers: Vec<u16>,
    /// Registers read about every `slow_poll_interval_s` (counters, settings).
    pub slow_poll_registers: Vec<u16>,
    pub slow_poll_interval_s: u64,
    /// Owner-approved writable registers; empty disables gated writes (ADR 0104).
    pub writable_registers: Vec<u16>,
    /// Every register writable with any encodable value (ADR 0125). Hardware with reads only.
    pub open_writes: bool,
    /// Alarm-reset control and NibePi-style auto-reset of alarm 251 (ADR 0105).
    pub alarm_reset: crate::writes::AlarmResetPolicy,
}
impl AdapterConfig {
    /// Bundled fixture explicitly assumes 48852=1. This is not a hardware default.
    pub fn replay() -> Self {
        Self {
            transport: TransportConfig::Replay(Capture::bundled()),
            catalog: Catalog::bundled(),
            word_order: WordOrder::LowWordFirst,
            capture_path: None,
            wire: WirePolicy::default(),
            hardware_reads: false,
            poll_registers: vec![],
            slow_poll_registers: vec![],
            slow_poll_interval_s: 600,
            writable_registers: vec![],
            open_writes: false,
            alarm_reset: crate::writes::AlarmResetPolicy::default(),
        }
    }
}
/// The UART has its own OS thread + current-thread reactor. HTTP runtime load,
/// bounded observation loss, slow SSE and disk capture cannot hold its executor.
pub async fn start_adapter(
    config: AdapterConfig,
    clock: Arc<dyn Clock>,
) -> Result<(F730Device, F730Handle), ServiceError> {
    config.catalog.validate()?;
    config.wire.validate()?;
    let mode = match &config.transport {
        TransportConfig::Replay(_) => DeviceMode::Replay,
        TransportConfig::Hardware(_) => DeviceMode::Hardware,
    };
    let any_writes = config.open_writes
        || !config.writable_registers.is_empty()
        || config.alarm_reset.manual
        || config.alarm_reset.auto_251;
    let catalog = Arc::new(config.catalog);
    let (stop, stopped) = watch::channel(false);
    let (commands, rx) = mpsc::channel(16);
    let (observations, mut events) = mpsc::channel(256);
    let initial = AdapterSnapshot {
        catalog: catalog.clone(),
        identity: None,
        word_order: config.word_order,
        readings: BTreeMap::new(),
        probe: ProbeStatus::default(),
        diagnostics: WireDiagnostics::default(),
        capabilities: Capabilities {
            writes: any_writes,
            ..Capabilities::new(mode, config.hardware_reads)
        },
        connection: ConnectionState::Connecting,
        last_error: None,
    };
    let (snapshots, state) = watch::channel(Arc::new(initial));
    let mut observer = Observer::new((*catalog).clone(), config.word_order);
    let mut aggregate_stop = stopped.clone();
    let (ready, ready_rx) = oneshot::channel();
    let (complete, finished) = watch::channel(false);
    let mut probes = ProbeEngine::new(catalog.clone(), mode, config.hardware_reads);
    probes.set_polls(
        &config.poll_registers,
        &config.slow_poll_registers,
        config.slow_poll_interval_s,
    )?;
    if any_writes && (mode != DeviceMode::Hardware || !config.hardware_reads) {
        return Err(ServiceError::validation(
            "writes need hardware mode with hardware reads for the read-back",
        ));
    }
    let writes = WriteEngine::with_policy(
        catalog.clone(),
        &config.writable_registers,
        config.alarm_reset,
        config.open_writes,
    )?;
    probes.status.writes = writes.status.clone();
    let wire_clock = clock.clone();
    let policy = config.wire.clone();
    let wire = Arc::new(RwLock::new(WireState::default()));
    let wire_state = wire.clone();
    std::thread::Builder::new().name("nibe-serial".into()).spawn(move||{
        struct Completion(watch::Sender<bool>);
        impl Drop for Completion {fn drop(&mut self){self.0.send_replace(true);}}
        let _complete=Completion(complete);
        let runtime=match tokio::runtime::Builder::new_current_thread().enable_all().build() {Ok(r)=>r,Err(e)=>{let _=ready.send(Err(crate::transport::io_error(e)));return;}};
        runtime.block_on(async move {
            let recorder=match config.capture_path {
                Some(path)=>match Recorder::create(&path,CaptureHeader{format:"nibe-gateway/capture-jsonl".into(),version:1,synthetic:mode==DeviceMode::Replay,device:"NIBE accessory bus (identity unverified)".into(),serial:"9600 8N1".into(),provenance:"Gateway wire recorder; unsanitized, timing is monotonic microseconds; inspect .summary.json for completeness".into(),metadata:BTreeMap::new()}) {Ok(r)=>Some(r),Err(e)=>{let _=ready.send(Err(e));return;}},None=>None,
            };
        // Linux nice is per-thread; lowering only this thread leaves Argon2 and
        // HTTP at normal priority. Failure is visible in health, never hidden.
        #[cfg(target_os = "linux")]
        { wire_state.write().expect("wire health lock").priority_applied = unsafe {
            libc::setpriority(libc::PRIO_PROCESS, 0, config.wire.thread_nice) == 0
        }; }

            let hardware_path = match &config.transport { TransportConfig::Hardware(path) => Some(path.clone()), _ => None };
            let transport:Result<Box<dyn DeviceTransport>,ServiceError>=match config.transport {
                TransportConfig::Replay(c)=>ReplayTransport::new(&c).map(|r|Box::new(r) as Box<dyn DeviceTransport>),
                TransportConfig::Hardware(path)=>open_serial(&path),
            };
            let transport=match transport {Ok(t)=>t,Err(e)=>{let _=ready.send(Err(e));return;}};
            let _=ready.send(Ok(()));
            wire_loop(transport,probes,writes,rx,observations,stopped,wire_clock,
                WireContext { recorder, policy: config.wire, state: wire_state, hardware_path }).await;
        });
    }).map_err(crate::transport::io_error)?;
    ready_rx
        .await
        .map_err(|_| ServiceError::new(ErrorCode::Unavailable, "wire startup stopped"))??;
    let frame_timeout_ms = policy.frame_timeout_ms;
    tokio::spawn(async move {
        let mut snapshot = (*snapshots.borrow().clone()).clone();
        let mut last_rx = None;
        let mut last_alarm: Option<i16> = None;
        let mut last_write: Option<(u16, u64, crate::writes::WriteState)> = None;
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {
                _=aggregate_stop.changed()=>break,
                event=events.recv()=>match event {
                    Some(Observation::Frame(f,at,requested))=>{
                        last_rx=Some(clock.now_ms());
                        observer.observe(&f,at,requested);
                        if f.command==0x6a && f.payload.len()==6 && f.payload[..2]==crate::writes::ALARM.to_le_bytes() {
                            let code=i16::from_le_bytes([f.payload[2],f.payload[3]]);
                            if last_alarm!=Some(code) {
                                tracing::info!(alarm=code, previous=?last_alarm, "pump alarm register 45001 changed");
                                last_alarm=Some(code);
                            }
                        }
                        if f.command==0x6a && f.payload.len()==6 && f.payload[..2]==crate::writes::ALARM_RESET.to_le_bytes() {
                            tracing::info!(value=f.payload[2], "alarm reset register 45171 read");
                        }
                        snapshot.identity=observer.identity.clone();
                        snapshot.word_order=observer.word_order;
                        snapshot.readings=observer.readings.clone();
                        snapshot.connection=ConnectionState::Connected;
                        snapshot.last_error=None;
                    }
                    Some(Observation::Stats(mut d,p))=>{let mut p=*p;
                        if let Some(last)=&p.writes.last {
                            let key=(last.register,last.queued_at_ms,last.state);
                            if last_write!=Some(key) {
                                tracing::info!(register=last.register, raw=last.raw, state=?last.state, auto_resets=p.writes.auto_resets, "write outcome");
                                last_write=Some(key);
                            }
                        }
                        d.malformed_payloads=observer.malformed;
                        for (&id,state) in &mut p.states {
                            if observer.readings.get(&id).is_some_and(|r|r.observation==ObservationState::ConfirmedReadable) {
                                *state=RegisterState::ConfirmedReadable;
                            }
                        }
                        snapshot.diagnostics=d;snapshot.probe=p;
                    }
                    Some(Observation::Error(e))=>{snapshot.last_error=Some(e);snapshot.connection=ConnectionState::Disconnected;}
                    Some(Observation::Catalog(c))=>{
                        snapshot.catalog=c.clone();
                        let identity=observer.identity.take();
                        observer=Observer::new((*c).clone(),observer.word_order);
                        observer.identity=identity;
                        snapshot.readings.clear();
                    }
                    None=>{
                        snapshot.connection=ConnectionState::Disconnected;
                        snapshot.last_error.get_or_insert_with(||"wire task stopped; inspect UART and diagnostics".into());
                        snapshots.send_replace(Arc::new(snapshot));return;
                    }
                },
                _=interval.tick()=>{if mode==DeviceMode::Hardware && last_rx.is_some_and(|t|clock.now_ms().saturating_sub(t)>frame_timeout_ms){snapshot.connection=ConnectionState::Disconnected;snapshot.last_error=Some(format!("no MODBUS40 frame for {frame_timeout_ms} ms"));}}
            }
            snapshots.send_replace(Arc::new(snapshot.clone()));
        }
    });
    let handle = F730Handle {
        snapshots: state,
        commands,
        lifecycle: Arc::new(Lifecycle { stop }),
        finished,
        wire,
        policy,
        hardware: mode == DeviceMode::Hardware,
    };
    Ok((
        F730Device {
            handle: handle.clone(),
            mode,
        },
        handle,
    ))
}
#[cfg(feature = "serial")]
fn open_serial(path: &str) -> Result<Box<dyn DeviceTransport>, ServiceError> {
    Ok(Box::new(crate::transport::SerialTransport::open(path)?))
}
#[cfg(not(feature = "serial"))]
fn open_serial(_: &str) -> Result<Box<dyn DeviceTransport>, ServiceError> {
    Err(ServiceError::new(
        ErrorCode::Unavailable,
        "hardware transport requires --features serial; no simulation fallback",
    ))
}

struct WireContext {
    recorder: Option<Recorder>,
    policy: WirePolicy,
    state: Arc<RwLock<WireState>>,
    hardware_path: Option<String>,
}
impl WireContext {
    async fn reconnect(
        &self,
        transport: &mut Option<Box<dyn DeviceTransport>>,
        attempts: &mut u32,
        stopped: &mut watch::Receiver<bool>,
    ) -> bool {
        // Close the old descriptor before opening: TIOCEXCL/flock must be released.
        transport.take();
        self.state.write().expect("wire health lock").recovering = true;
        if let Some(path) = &self.hardware_path {
            while *attempts < self.policy.reopen_attempts {
                *attempts += 1;
                self.state
                    .write()
                    .expect("wire health lock")
                    .reopen_attempts += 1;
                let delay = self
                    .policy
                    .reopen_backoff_ms
                    .saturating_mul(1 << (*attempts - 1))
                    .min(5000);
                tokio::select! { biased; _ = stopped.changed() => return false,
                _ = tokio::time::sleep(Duration::from_millis(delay)) => {} }
                if let Ok(port) = open_serial(path) {
                    *transport = Some(port);
                    return true;
                }
            }
        }
        self.state.write().expect("wire health lock").fatal = true;
        false
    }
}
// Each argument is an independent owned piece of wire-thread state; bundling them hides nothing.
#[allow(clippy::too_many_arguments)]
async fn wire_loop(
    transport: Box<dyn DeviceTransport>,
    mut probes: ProbeEngine,
    mut writes: WriteEngine,
    mut commands: mpsc::Receiver<CommandRequest>,
    observations: mpsc::Sender<Observation>,
    mut stopped: watch::Receiver<bool>,
    clock: Arc<dyn Clock>,
    context: WireContext,
) {
    let mut transport = Some(transport);
    let mut reopen_attempts = 0;
    let mut consecutive_misses = 0;
    let deadline = Duration::from_millis(context.policy.reply_deadline_ms);
    let mut decoder = Decoder::default();
    let mut stats = WireDiagnostics::default();
    let start = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    'wire: loop {
        tokio::select! {
            biased;
            _=stopped.changed()=>break,
            bytes=transport.as_mut().expect("open transport").receive()=>{
                let bytes=match bytes {Ok(b)=>b,Err(e)=>{
                    let _=observations.try_send(Observation::Error(e.to_string()));
                    if context.reconnect(&mut transport, &mut reopen_attempts, &mut stopped).await {
                        decoder=Decoder::default(); continue 'wire;
                    }
                    break;
                }};
                probes.expire(clock.now_ms());
                let received_instant=Instant::now();
                let received_at=start.elapsed().as_micros() as u64;
                // All replies precede decoding, publishing state, or disk recording.
                let mut observed=vec![];let mut sent=vec![];
                for event in decoder.feed(&bytes) {

                    let pending=match &event {Event::Frame(f) if f.address==MODBUS40 && f.command==0x69 && f.payload.is_empty()=>probes.on_token(clock.now_ms()),_=>None};
                    let write=match &event {Event::Frame(f) if f.address==MODBUS40 && f.command==0x6b && f.payload.is_empty()=>writes.on_token(clock.now_ms()),_=>None};
                    let reply=match &write {Some(w)=>Some(w.as_bytes().to_vec()),None=>nibe_protocol::reply(&event,pending.as_ref())};
                    if let Some(reply)=reply {
                        let result = tokio::time::timeout(deadline.saturating_sub(received_instant.elapsed()),
                            transport.as_mut().expect("open transport").send(&reply)).await;
                        let elapsed=received_instant.elapsed();
                        stats.max_reply_us=stats.max_reply_us.max(elapsed.as_micros() as u64);
                        let uart_error = matches!(&result, Ok(Err(_)));
                        let missed = result.is_err() || elapsed > deadline;
                        if missed { stats.ack_deadline_misses += 1; consecutive_misses += 1; }
                        else if !uart_error { consecutive_misses = 0; reopen_attempts = 0; }
                        context.state.write().expect("wire health lock").misses = consecutive_misses;
                        if uart_error || consecutive_misses > context.policy.tolerated_consecutive_misses {
                            let _=observations.try_send(Observation::Stats(stats.clone(),Box::new(ProbeStatus{writes:writes.status.clone(),..probes.status.clone()})));
                            let _=observations.try_send(Observation::Error(format!("UART/reply failure: {result:?}")));
                            if context.reconnect(&mut transport, &mut reopen_attempts, &mut stopped).await {
                                decoder=Decoder::default(); consecutive_misses=0; continue 'wire;
                            }
                            break 'wire;
                        }
                        // A timed-out ACK is dropped, never retried after its reply window.
                        if result.is_ok() {
                        if reply.len()==10 {stats.write_requests_sent+=1;}else if reply.len()==6 {stats.read_requests_sent+=1;}else if reply==[nibe_protocol::ACK]{stats.acks_sent+=1;}else{stats.naks_sent+=1;}
                        sent.push((start.elapsed().as_micros() as u64,reply));
                        }
                    }
                    match event {
                        Event::Frame(f)=>{if f.address==MODBUS40{stats.frames_for_us+=1;
                            let mut state=context.state.write().expect("wire health lock");
                            state.last_frame=Some(Instant::now()); state.recovering=false;
                        }else{stats.frames_for_other_addresses+=1;}
                            if f.address==MODBUS40 && f.command==0x6c && f.payload.len()==1 {
                                for id in writes.response(f.payload[0]==1,clock.now_ms()) {probes.readback(id);}
                            }
                            if f.address==MODBUS40 && f.command==0x6a && f.payload.len()==6 && f.payload[..2]==crate::writes::ALARM.to_le_bytes() {
                                let code=i32::from(i16::from_le_bytes([f.payload[2],f.payload[3]]));
                                writes.observe_alarm(code,clock.now_ms());
                            }
                            let requested=if f.address==MODBUS40 && f.command==0x6a && f.payload.len()==6 {let id=u16::from_le_bytes([f.payload[0],f.payload[1]]);probes.response(id).then_some(id)}else{None};
                            if f.address==MODBUS40 {observed.push(Observation::Frame(f,clock.now_ms(),requested));}}
                        Event::ChecksumError{..}=>stats.checksum_errors+=1,Event::InvalidEscape{..}=>stats.invalid_escapes+=1,
                        Event::Ack=>stats.acks_received+=1,Event::Nak=>stats.naks_received+=1,Event::Garbage=>stats.garbage_bytes+=1,Event::Accessory=>{},
                    }
                }
                if let Some(r)=&context.recorder {r.record(received_at,"rx",&bytes);for (at,reply) in sent {r.record(at,"tx",&reply);}}
                for event in observed {if observations.try_send(event).is_err(){stats.observations_dropped+=1;}}
            }
            Some(command)=commands.recv()=>match command {
                CommandRequest::Probe(ids,authenticated,tx)=>{if !tx.is_closed(){let _=tx.send(probes.start(&ids,authenticated));}}
                CommandRequest::Cancel(tx)=>{if !tx.is_closed(){let _=tx.send(Ok(probes.cancel()));}}
                CommandRequest::Write(id,raw,tx)=>{if !tx.is_closed(){let _=tx.send(writes.queue(id,raw,clock.now_ms()));}}
                CommandRequest::AlarmReset(tx)=>{if !tx.is_closed(){let _=tx.send(writes.queue_alarm_reset(clock.now_ms()));}}
                CommandRequest::Read(id,tx)=>{if !tx.is_closed(){let _=tx.send(probes.read_once(id));}}
                CommandRequest::Polls(fast,slow,tx)=>{if !tx.is_closed(){let _=tx.send(probes.replace_polls(&fast,&slow));}}
                CommandRequest::Import(c,tx)=>{if !tx.is_closed(){
                    if probes.status.running {let _=tx.send(Err(ServiceError::new(ErrorCode::Busy,"cancel active probes before importing")));}
                    else if observations.try_send(Observation::Catalog(c.clone())).is_err(){let _=tx.send(Err(ServiceError::new(ErrorCode::Busy,"observation queue full")));}
                    else {probes.catalog=c;probes.cancel();let _=tx.send(Ok(()));}
                }}
            },
            _=interval.tick()=>{probes.expire(clock.now_ms());if let Some(id)=writes.expire(clock.now_ms()){probes.readback(id);}}
        }
        if let Some(r) = &context.recorder {
            stats.capture_dropped = r.dropped.load(std::sync::atomic::Ordering::Relaxed);
            stats.capture_failed = r.failed();
        }
        if observations
            .try_send(Observation::Stats(
                stats.clone(),
                Box::new(ProbeStatus {
                    writes: writes.status.clone(),
                    ..probes.status.clone()
                }),
            ))
            .is_err()
        {
            stats.observations_dropped += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offset_completion_survives_other_controls() {
        let mut writes = WriteEngine::with_alarm_reset(
            Arc::new(Catalog::bundled()),
            &[HEAT_OFFSET_S1, TEMPORARY_LUX],
            crate::writes::AlarmResetPolicy {
                manual: true,
                auto_251: true,
            },
        )
        .unwrap();
        writes.queue(HEAT_OFFSET_S1, -10, 1).unwrap();
        writes.on_token(2).unwrap();
        writes.response(true, 3);
        writes.queue(TEMPORARY_LUX, 1, 4).unwrap();
        writes.on_token(5).unwrap();
        writes.response(true, 6);
        assert_eq!(offset_completion(&writes.status), Some((3, true)));
        assert!(writes.observe_alarm(251, 7));
        writes.on_token(8).unwrap();
        writes.response(true, 9);
        assert_eq!(offset_completion(&writes.status), Some((3, true)));
        // A new offset in the same timestamp must invalidate the old evidence.
        writes.on_token(10).unwrap();
        writes.response(true, 11);
        writes.queue(HEAT_OFFSET_S1, 0, 3).unwrap();
        assert_eq!(offset_completion(&writes.status), None);
        writes.on_token(4).unwrap();
        writes.response(false, 5);
        assert_eq!(offset_completion(&writes.status), Some((5, false)));
    }
    struct TestTransport {
        incoming: mpsc::Receiver<Vec<u8>>,
        replies: mpsc::Sender<Vec<u8>>,
        stalls: u32,
        send_error: bool,
    }
    impl DeviceTransport for TestTransport {
        fn receive(&mut self) -> DeviceFuture<'_, Vec<u8>> {
            Box::pin(async move {
                self.incoming
                    .recv()
                    .await
                    .ok_or_else(|| ServiceError::new(ErrorCode::Disconnected, "test EOF"))
            })
        }
        fn send<'a>(&'a mut self, frame: &'a [u8]) -> DeviceFuture<'a, ()> {
            Box::pin(async move {
                if self.send_error {
                    return Err(ServiceError::new(
                        ErrorCode::Disconnected,
                        "injected UART error",
                    ));
                }
                if self.stalls > 0 {
                    self.stalls -= 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                self.replies.try_send(frame.to_vec()).unwrap();
                Ok(())
            })
        }
    }
    #[tokio::test]
    async fn full_observation_channel_never_delays_ack_and_other_addresses_stay_silent() {
        let (incoming, rx) = mpsc::channel(256);
        let (replies, mut sent) = mpsc::channel(256);
        let (observations, _not_consumed) = mpsc::channel(1);
        let (stop, stopped) = watch::channel(false);
        let (_requests, commands) = mpsc::channel(1);
        let task = tokio::spawn(wire_loop(
            Box::new(TestTransport {
                incoming: rx,
                replies,
                stalls: 0,
                send_error: false,
            }),
            ProbeEngine::new(Arc::new(Catalog::bundled()), DeviceMode::Hardware, false),
            crate::writes::WriteEngine::new(Arc::new(Catalog::bundled()), &[]).unwrap(),
            commands,
            observations,
            stopped,
            Arc::new(SystemClock::default()),
            WireContext {
                recorder: None,
                policy: WirePolicy::default(),
                state: Arc::new(RwLock::new(WireState::default())),
                hardware_path: None,
            },
        ));
        for _ in 0..100 {
            incoming
                .send(nibe_protocol::encode_pump(MODBUS40, 0x6b, &[]).unwrap())
                .await
                .unwrap();
            incoming
                .send(nibe_protocol::encode_pump(0x19, 0xee, &[]).unwrap())
                .await
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            for _ in 0..100 {
                assert_eq!(sent.recv().await.unwrap(), [6]);
            }
        })
        .await
        .unwrap();
        assert!(sent.try_recv().is_err());
        let _ = stop.send(true);
        task.await.unwrap();
    }
    #[tokio::test]
    async fn opted_in_hardware_sends_one_read_request_per_token_and_acks_everything_else() {
        let (incoming, rx) = mpsc::channel(8);
        let (replies, mut sent) = mpsc::channel(8);
        let (observations, _events) = mpsc::channel(32);
        let (stop, stopped) = watch::channel(false);
        let (requests, commands) = mpsc::channel(1);
        let task = tokio::spawn(wire_loop(
            Box::new(TestTransport {
                incoming: rx,
                replies,
                stalls: 0,
                send_error: false,
            }),
            ProbeEngine::new(Arc::new(Catalog::bundled()), DeviceMode::Hardware, true),
            crate::writes::WriteEngine::new(Arc::new(Catalog::bundled()), &[]).unwrap(),
            commands,
            observations,
            stopped,
            Arc::new(SystemClock::default()),
            WireContext {
                recorder: None,
                policy: WirePolicy::default(),
                state: Arc::new(RwLock::new(WireState::default())),
                hardware_path: None,
            },
        ));
        let (tx, started) = oneshot::channel();
        requests
            .send(CommandRequest::Probe(vec![47011], true, tx))
            .await
            .unwrap();
        assert!(started.await.unwrap().unwrap().running);
        let mut next = async |frame: Vec<u8>| {
            incoming.send(frame).await.unwrap();
            tokio::time::timeout(Duration::from_secs(1), sent.recv())
                .await
                .unwrap()
                .unwrap()
        };
        // The write token and the 0xEE version request seen on the real F730 only get an ACK.
        assert_eq!(
            next(nibe_protocol::encode_pump(MODBUS40, 0x6b, &[]).unwrap()).await,
            [6]
        );
        assert_eq!(
            next(nibe_protocol::encode_pump(MODBUS40, 0xee, &[]).unwrap()).await,
            [6]
        );
        assert_eq!(
            next(nibe_protocol::encode_pump(MODBUS40, 0x69, &[]).unwrap()).await,
            nibe_protocol::read_request(47011).unwrap().as_bytes()
        );
        let response = [&47011u16.to_le_bytes()[..], &[0xfe, 0, 0, 0]].concat();
        assert_eq!(
            next(nibe_protocol::encode_pump(MODBUS40, 0x6a, &response).unwrap()).await,
            [6]
        );
        // Nothing left to read: the next read token is only acknowledged.
        assert_eq!(
            next(nibe_protocol::encode_pump(MODBUS40, 0x69, &[]).unwrap()).await,
            [6]
        );
        stop.send(true).unwrap();
        task.await.unwrap();
    }
    #[tokio::test]
    async fn allowlisted_write_replaces_one_write_token_ack_then_reads_back() {
        let (incoming, rx) = mpsc::channel(8);
        let (replies, mut sent) = mpsc::channel(8);
        let (observations, _events) = mpsc::channel(64);
        let (stop, stopped) = watch::channel(false);
        let (requests, commands) = mpsc::channel(1);
        let catalog = Arc::new(Catalog::bundled());
        let task = tokio::spawn(wire_loop(
            Box::new(TestTransport {
                incoming: rx,
                replies,
                stalls: 0,
                send_error: false,
            }),
            ProbeEngine::new(catalog.clone(), DeviceMode::Hardware, true),
            crate::writes::WriteEngine::new(catalog, &[HEAT_OFFSET_S1]).unwrap(),
            commands,
            observations,
            stopped,
            Arc::new(SystemClock::default()),
            WireContext {
                recorder: None,
                policy: WirePolicy::default(),
                state: Arc::new(RwLock::new(WireState::default())),
                hardware_path: None,
            },
        ));
        let write = async |id, raw| {
            let (tx, rx) = oneshot::channel();
            requests
                .send(CommandRequest::Write(id, raw, tx))
                .await
                .unwrap();
            rx.await.unwrap()
        };
        assert!(write(47007, 5).await.is_err(), "not allowlisted");
        assert!(write(HEAT_OFFSET_S1, 11).await.is_err(), "out of range");
        write(HEAT_OFFSET_S1, -1).await.unwrap();
        let mut next = async |frame: Vec<u8>| {
            incoming.send(frame).await.unwrap();
            tokio::time::timeout(Duration::from_secs(1), sent.recv())
                .await
                .unwrap()
                .unwrap()
        };
        let pump = |command, payload: &[u8]| {
            nibe_protocol::encode_pump(MODBUS40, command, payload).unwrap()
        };
        assert_eq!(
            next(pump(0x6b, &[])).await,
            nibe_protocol::write_request(HEAT_OFFSET_S1, -1)
                .unwrap()
                .as_bytes()
        );
        assert_eq!(next(pump(0x6b, &[])).await, [6], "a write is sent once");
        assert_eq!(next(pump(0x6c, &[1])).await, [6]);
        assert_eq!(
            next(pump(0x69, &[])).await,
            nibe_protocol::read_request(HEAT_OFFSET_S1)
                .unwrap()
                .as_bytes(),
            "accepted write is read back first"
        );
        stop.send(true).unwrap();
        task.await.unwrap();
    }
    #[tokio::test]
    async fn isolated_miss_recovers_on_next_reply_but_uart_error_is_fatal() {
        for send_error in [false, true] {
            let (incoming, rx) = mpsc::channel(8);
            let (replies, mut sent) = mpsc::channel(8);
            let (observations, _events) = mpsc::channel(32);
            let (stop, stopped) = watch::channel(false);
            let (_requests, commands) = mpsc::channel(1);
            let state = Arc::new(RwLock::new(WireState::default()));
            let task = tokio::spawn(wire_loop(
                Box::new(TestTransport {
                    incoming: rx,
                    replies,
                    stalls: 1,
                    send_error,
                }),
                ProbeEngine::new(Arc::new(Catalog::bundled()), DeviceMode::Hardware, false),
                crate::writes::WriteEngine::new(Arc::new(Catalog::bundled()), &[]).unwrap(),
                commands,
                observations,
                stopped,
                Arc::new(SystemClock::default()),
                WireContext {
                    recorder: None,
                    policy: WirePolicy::default(),
                    state: state.clone(),
                    hardware_path: None,
                },
            ));
            incoming
                .send(nibe_protocol::encode_pump(MODBUS40, 0x69, &[]).unwrap())
                .await
                .unwrap();
            if send_error {
                tokio::time::timeout(Duration::from_secs(1), task)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(state.read().unwrap().fatal);
                assert!(sent.try_recv().is_err());
            } else {
                incoming
                    .send(nibe_protocol::encode_pump(MODBUS40, 0x6b, &[]).unwrap())
                    .await
                    .unwrap();
                assert_eq!(
                    tokio::time::timeout(Duration::from_secs(1), sent.recv())
                        .await
                        .unwrap()
                        .unwrap(),
                    [6]
                );
                assert!(!task.is_finished(), "one miss must not stop the wire task");
                assert_eq!(state.read().unwrap().misses, 0);
                assert!(!state.read().unwrap().fatal);
                stop.send(true).unwrap();
                task.await.unwrap();
            }
        }
    }
    #[tokio::test]
    async fn sustained_reply_misses_stop_transport_and_report_error() {
        let (incoming, rx) = mpsc::channel(8);
        let (replies, mut sent) = mpsc::channel(1);
        let (observations, mut events) = mpsc::channel(16);
        let (_stop, stopped) = watch::channel(false);
        let (_requests, commands) = mpsc::channel(1);
        for _ in 0..4 {
            incoming
                .send(nibe_protocol::encode_pump(MODBUS40, 0x69, &[]).unwrap())
                .await
                .unwrap();
        }
        let task = tokio::spawn(wire_loop(
            Box::new(TestTransport {
                incoming: rx,
                replies,
                stalls: 4,
                send_error: false,
            }),
            ProbeEngine::new(Arc::new(Catalog::bundled()), DeviceMode::Hardware, false),
            crate::writes::WriteEngine::new(Arc::new(Catalog::bundled()), &[]).unwrap(),
            commands,
            observations,
            stopped,
            Arc::new(SystemClock::default()),
            WireContext {
                recorder: None,
                policy: WirePolicy::default(),
                state: Arc::new(RwLock::new(WireState::default())),
                hardware_path: None,
            },
        ));
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(sent.try_recv().is_err());
        let mut missed = false;
        while let Some(event) = events.recv().await {
            if let Observation::Stats(stats, _) = event {
                missed |= stats.ack_deadline_misses == 4;
            }
        }
        assert!(missed);
    }
}
