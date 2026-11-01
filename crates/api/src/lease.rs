//! Control leases: a dead-man's switch on a few registers. A write may carry
//! `lease_seconds`; if the writer does not write again with a lease before it lapses, the gateway writes
//! back the value read from the pump before the first leased write. Every write to a leasable register goes
//! through this coordinator, so whoever wrote last decides: a write without a lease ends the lease.
//!
//! The rules live here once. How a register is written, read and confirmed is its `LeaseChannel`: the heating
//! offset through the gateway's named control (so its gates and simulation apply), the others through
//! the shared register write. Nothing else is automated.
use nibe_core::*;
use nibe_persistence::{
    ControlLease, LeaseRestore, LeaseState, RegisterLease, RegisterLeaseRestore, Store,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use utoipa::ToSchema;

pub const MIN_LEASE_SECONDS: u32 = 60;
pub const MAX_LEASE_SECONDS: u32 = 3600;
/// Wait between restore attempts after a transient failure (busy queue, wire down, timeout).
const RETRY_MS: u64 = 30_000;
/// F730 heat offset S1. The simulator has no 47011 and names its synthetic register `heating_offset`.
pub const HEAT_OFFSET_REGISTER: u16 = nibe_persistence::HEAT_OFFSET_REGISTER;
const SIMULATED_NAME: &str = "heating_offset";

/// What a lapsed lease writes back. Every leasable register states this, because the earlier value is not
/// always the right one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// The value read from the pump before the first leased write. For settings that belong to the house,
    /// which no fixed value could match.
    Earlier,
    /// Always this raw value, whatever the pump had. For a mode with one safe state, where the pump cannot
    /// say whether its current value was chosen or left behind by a writer that died.
    Fixed(i32),
}

/// The leasable registers besides the heating offset, each with what a lapse restores. Counters such as
/// degree minutes do not belong here: the pump changes them, so no earlier value is right to restore.
pub const REGISTER_LEASES: [(u16, Restore); 4] = [
    // Hot-water start and stop temperature (economy) and the heating stop temperature: house settings.
    (47045, Restore::Earlier),
    (47049, Restore::Earlier),
    (47375, Restore::Earlier),
    // Holiday mode, 0 = off, 10 = on. Wrongly on means no hot water until someone notices; wrongly off
    // only spends some energy, so a lapse always turns it off, even if it was set at the pump.
    (48043, Restore::Fixed(0)),
];

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A queued write: the named-control receipt where there is one, and a human-readable detail.
pub struct Written {
    pub receipt: Option<CommandReceipt>,
    pub detail: String,
}

/// How one leasable register is written and read.
pub trait LeaseChannel: Send + Sync {
    /// Rejects values the register can never take, before any intent is saved.
    fn validate(&self, _raw: i32) -> Result<(), ServiceError> {
        Ok(())
    }
    /// Queue one write. Ok means queued (applied, in simulation).
    fn write(&self, raw: i32) -> BoxFuture<'_, Result<Written, ServiceError>>;
    /// A value the pump reported after `after_ms`, or with `None` a current one. Errors when there is none.
    fn read(&self, after_ms: Option<u64>) -> BoxFuture<'_, Result<i32, ServiceError>>;
    /// The latest finished write: (finished at, accepted). `None` while one is pending or none was made.
    fn completion(&self) -> Option<(u64, bool)>;
    /// What a lapsed lease writes back.
    fn restore(&self) -> Restore;
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LeaseStatus {
    /// The active lease, or one that lapsed and whose restore is still being retried.
    pub heating_offset: Option<ControlLease>,
    /// Seconds until the baseline is restored; 0 once lapsed.
    pub remaining_s: Option<u64>,
    pub last_restore: Option<LeaseRestore>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActiveRegisterLease {
    pub lease: RegisterLease,
    /// Seconds until the baseline is restored; 0 once lapsed.
    pub remaining_s: u64,
}
/// Every lease, the heating offset included.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterLeaseStatus {
    /// Registers whose writes may carry a lease.
    pub leasable: Vec<u16>,
    /// Active leases, and lapsed ones whose restore is still being retried.
    pub leases: Vec<ActiveRegisterLease>,
    /// The last restore per register.
    pub restores: Vec<RegisterLeaseRestore>,
}

pub struct Leases {
    gateway: Gateway,
    store: Option<Arc<Store>>,
    channels: RwLock<BTreeMap<u16, Arc<dyn LeaseChannel>>>,
    /// Serializes leased writes, unleased writes and restores, so a renewal never races a restore.
    operation: tokio::sync::Mutex<()>,
    state: Mutex<Inner>,
    /// Records lease restores; set once at start.
    audit: std::sync::OnceLock<Arc<nibe_persistence::audit::AuditLog>>,
}
#[derive(Clone)]
struct Inner {
    leases: BTreeMap<u16, RegisterLease>,
    restores: BTreeMap<u16, RegisterLeaseRestore>,
    progress: BTreeMap<u16, Progress>,
    /// A crash-hidden outcome is decided only by a reading after this.
    recovery_after_ms: u64,
}
#[derive(Clone, Default)]
struct Progress {
    /// Our last write to the register (owner, holder or restore), started at.
    last_write_ms: Option<u64>,
    restore_started_ms: Option<u64>,
    next_attempt_ms: u64,
}

impl Leases {
    pub fn start(gateway: Gateway, store: Option<Arc<Store>>) -> Arc<Self> {
        Self::start_with(gateway, store, Duration::from_secs(1))
    }
    /// `tick` is how often expiry is checked against the gateway clock (tests use a short one). The heating
    /// offset is leasable from the start; other registers once their channel is added.
    pub fn start_with(gateway: Gateway, store: Option<Arc<Store>>, tick: Duration) -> Arc<Self> {
        let (mut leases, restores) = store
            .as_ref()
            .map(|s| s.lease_state().by_register())
            .unwrap_or_default();
        // A clock still behind real time (no RTC, before NTP) must not stretch a persisted lease: it ends
        // at most one maximum lease from now. A later forward NTP step only makes it end sooner.
        let now = gateway.now_ms();
        let latest = now.saturating_add(u64::from(MAX_LEASE_SECONDS) * 1000);
        for lease in leases.values_mut() {
            lease.expires_at_ms = lease.expires_at_ms.min(latest);
            tracing::info!(register = lease.register, holder = %lease.holder, baseline = lease.baseline, "lease loaded from state");
        }
        let offset: Arc<dyn LeaseChannel> = Arc::new(OffsetChannel {
            gateway: gateway.clone(),
        });
        let leases = Arc::new(Self {
            gateway: gateway.clone(),
            store,
            channels: RwLock::new(BTreeMap::from([(HEAT_OFFSET_REGISTER, offset)])),
            operation: tokio::sync::Mutex::new(()),
            state: Mutex::new(Inner {
                leases,
                restores,
                progress: BTreeMap::new(),
                recovery_after_ms: now.max(offset_observed(&gateway.snapshot()).unwrap_or(0)),
            }),
            audit: std::sync::OnceLock::new(),
        });
        let writer: Arc<dyn HeatingOffsetWriter> = leases.clone();
        gateway.set_offset_writer(Arc::downgrade(&writer));
        if let Err(error) = leases.save() {
            tracing::error!(%error, "lease startup persistence failed");
        }
        let weak = Arc::downgrade(&leases);
        let mut stopped = gateway.shutdown_receiver();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = stopped.changed() => break,
                    _ = interval.tick() => {}
                }
                let Some(leases) = weak.upgrade() else { break };
                leases.check().await;
            }
        });
        leases
    }

    /// Makes `register` leasable. A lease saved before a restart is handled from the next check.
    pub fn add_channel(&self, register: u16, channel: Arc<dyn LeaseChannel>) {
        self.channels.write().unwrap().insert(register, channel);
    }
    pub fn is_leasable(&self, register: u16) -> bool {
        self.channels.read().unwrap().contains_key(&register)
    }
    fn channel(&self, register: u16) -> Result<Arc<dyn LeaseChannel>, ServiceError> {
        self.channels
            .read()
            .unwrap()
            .get(&register)
            .cloned()
            .ok_or_else(|| {
                ServiceError::validation(format!(
                    "register {register} cannot be leased; leasable: {:?}",
                    self.leasable()
                ))
            })
    }
    pub fn leasable(&self) -> Vec<u16> {
        self.channels.read().unwrap().keys().copied().collect()
    }

    /// The heating-offset lease, as shown on the Controls page.
    pub fn status(&self) -> LeaseStatus {
        let inner = self.state.lock().unwrap();
        let now = self.gateway.now_ms();
        let lease = inner.leases.get(&HEAT_OFFSET_REGISTER);
        LeaseStatus {
            heating_offset: lease.and_then(ControlLease::from_register),
            remaining_s: lease.map(|l| l.expires_at_ms.saturating_sub(now).div_ceil(1000)),
            last_restore: inner
                .restores
                .get(&HEAT_OFFSET_REGISTER)
                .and_then(LeaseRestore::from_register),
        }
    }
    pub fn register_status(&self) -> RegisterLeaseStatus {
        let inner = self.state.lock().unwrap();
        let now = self.gateway.now_ms();
        RegisterLeaseStatus {
            leasable: self.leasable(),
            leases: inner
                .leases
                .values()
                .map(|lease| ActiveRegisterLease {
                    lease: lease.clone(),
                    remaining_s: lease.expires_at_ms.saturating_sub(now).div_ceil(1000),
                })
                .collect(),
            restores: inner.restores.values().cloned().collect(),
        }
    }

    /// A heating-offset write through the named control.
    pub async fn write_offset(
        &self,
        offset: i8,
        lease_seconds: Option<u32>,
        holder: String,
    ) -> Result<CommandReceipt, ServiceError> {
        let written = self
            .write(HEAT_OFFSET_REGISTER, offset.into(), lease_seconds, holder)
            .await?;
        written
            .receipt
            .ok_or_else(|| ServiceError::new(ErrorCode::Unavailable, "no command receipt"))
    }

    /// A write to a leasable register. With `lease_seconds` it starts or renews the lease; without, it
    /// ends any lease on that register without a restore (whoever wrote last decided the value).
    pub async fn write(
        &self,
        register: u16,
        raw: i32,
        lease_seconds: Option<u32>,
        holder: String,
    ) -> Result<Written, ServiceError> {
        let channel = self.channel(register)?;
        channel.validate(raw)?;
        if lease_seconds.is_some_and(|s| !(MIN_LEASE_SECONDS..=MAX_LEASE_SECONDS).contains(&s)) {
            return Err(ServiceError::validation(format!(
                "lease_seconds must be between {MIN_LEASE_SECONDS} and {MAX_LEASE_SECONDS}"
            )));
        }
        let _operation = self.operation.lock().await;
        // A new owner write replaces an unresolved one (its marker carries the new request), so
        // a missing read-back never locks the owner out; only renewals must wait for the outcome.
        if !self.reconcile(register, &channel).await? && lease_seconds.is_some() {
            return Err(ServiceError::new(
                ErrorCode::Unavailable,
                format!("waiting for a fresh read-back of {register} after an unresolved write"),
            ));
        }
        let existing = self.lease(register);
        let baseline = match (lease_seconds, &existing) {
            (None, _) => None,
            (Some(_), Some(lease)) => Some(lease.baseline),
            (Some(_), None) => Some(match channel.restore() {
                Restore::Fixed(raw) => raw,
                Restore::Earlier => self.fresh_baseline(register, &channel).await?,
            }),
        };
        let previous_intent = self.state.lock().unwrap().clone();
        // Intent is durable before a request can reach the device.
        let now = self.gateway.now_ms();
        {
            let mut inner = self.state.lock().unwrap();
            let previous = inner.leases.remove(&register);
            if let (Some(seconds), Some(baseline)) = (lease_seconds, baseline) {
                tracing::info!(register, %holder, raw, baseline, seconds, "lease granted");
                inner.leases.insert(
                    register,
                    RegisterLease {
                        register,
                        granted_at_ms: previous.map_or(now, |p| p.granted_at_ms),
                        holder,
                        raw,
                        baseline,
                        expires_at_ms: now.saturating_add(u64::from(seconds) * 1000),
                        pending_supersession: None,
                    },
                );
                inner.progress.entry(register).or_default().next_attempt_ms = 0;
            } else if let Some(mut lease) = previous {
                lease.pending_supersession = Some((raw, now));
                inner.leases.insert(register, lease);
            }
        }
        self.save()?;
        let written = channel.write(raw).await;
        // These errors occur before enqueueing: queue saturation, range/encoding validation and
        // control gates. Unavailable is ambiguous (also a lost reply after enqueueing), as are
        // Timeout/Disconnected/Malformed/Rejected; never roll those back.
        if written.as_ref().is_err_and(|e| {
            matches!(
                e.code,
                ErrorCode::Busy | ErrorCode::Validation | ErrorCode::ControlsDisabled
            )
        }) {
            *self.state.lock().unwrap() = previous_intent.clone();
            if let Err(error) = self.save() {
                // Preserve the safe retry intent locally even if durable rollback failed.
                *self.state.lock().unwrap() = previous_intent;
                return Err(error);
            }
            return written;
        }
        {
            let mut inner = self.state.lock().unwrap();
            let progress = inner.progress.entry(register).or_default();
            progress.last_write_ms = Some(now);
            progress.restore_started_ms = None;
            if lease_seconds.is_none() {
                inner.leases.remove(&register);
            }
        }
        self.save()?;
        written.map(|mut w| {
            let note = match (lease_seconds, existing) {
                (Some(seconds), _) => format!("; lease {seconds} s"),
                (None, Some(lease)) => format!(
                    "; the lease held by {} ended without a restore",
                    lease.holder
                ),
                (None, None) => String::new(),
            };
            w.detail.push_str(&note);
            if let Some(receipt) = &mut w.receipt {
                receipt.detail.push_str(&note);
            }
            w
        })
    }

    fn lease(&self, register: u16) -> Option<RegisterLease> {
        self.state.lock().unwrap().leases.get(&register).cloned()
    }

    /// The first leased write's baseline: a reading taken after our last write to the register finished.
    async fn fresh_baseline(
        &self,
        register: u16,
        channel: &Arc<dyn LeaseChannel>,
    ) -> Result<i32, ServiceError> {
        let last_write = self
            .state
            .lock()
            .unwrap()
            .progress
            .get(&register)
            .and_then(|p| p.last_write_ms);
        let after = match last_write {
            None => None,
            Some(at) => match channel.completion() {
                Some((done, _)) if done >= at => Some(done),
                _ => {
                    return Err(ServiceError::new(
                        ErrorCode::Unavailable,
                        format!(
                            "a write to {register} is still in flight; retry when it completes"
                        ),
                    ))
                }
            },
        };
        channel.read(after).await.map_err(|e| match e.code {
            ErrorCode::ControlsDisabled => e,
            _ => ServiceError::new(
                ErrorCode::Unavailable,
                format!(
                    "no fresh reading of {register} to use as the lease baseline; retry after the next poll ({})",
                    e.message
                ),
            ),
        })
    }

    // An unresolved durable marker means a crash hid the dispatch outcome. Only a new
    // read-back can decide: matching the owner's request ends protection; any other
    // value retains the lease and its original expiry/baseline. Never replay the owner write.
    async fn reconcile(
        &self,
        register: u16,
        channel: &Arc<dyn LeaseChannel>,
    ) -> Result<bool, ServiceError> {
        let Some((requested, _)) = self.lease(register).and_then(|l| l.pending_supersession) else {
            return Ok(true);
        };
        let after = self.state.lock().unwrap().recovery_after_ms;
        let Ok(value) = channel.read(Some(after)).await else {
            return Ok(false);
        };
        {
            let mut inner = self.state.lock().unwrap();
            if value == requested {
                inner.leases.remove(&register);
            } else if let Some(lease) = inner.leases.get_mut(&register) {
                lease.pending_supersession = None;
            }
        }
        self.save()?;
        Ok(true)
    }

    /// Record lease restores in the audit log.
    pub fn set_audit(&self, audit: Arc<nibe_persistence::audit::AuditLog>) {
        let _ = self.audit.set(audit);
    }

    /// Restores lapsed leases under the same coordinator lock used by HTTP and MQTT writes.
    async fn check(&self) {
        let _operation = self.operation.lock().await;
        let registers: Vec<u16> = self.state.lock().unwrap().leases.keys().copied().collect();
        for register in registers {
            // A saved lease whose channel is not added yet waits for it.
            if let Ok(channel) = self.channel(register) {
                self.check_one(register, &channel).await;
            }
        }
    }

    async fn check_one(&self, register: u16, channel: &Arc<dyn LeaseChannel>) {
        if !matches!(self.reconcile(register, channel).await, Ok(true)) {
            return;
        }
        let Some(lease) = self.lease(register) else {
            return;
        };
        let now = self.gateway.now_ms();
        if now < lease.expires_at_ms {
            return;
        }
        let progress = self
            .state
            .lock()
            .unwrap()
            .progress
            .get(&register)
            .cloned()
            .unwrap_or_default();
        if let Some(started) = progress.restore_started_ms {
            // Queued is not applied: confirm with an accepted outcome and a reading taken after it.
            if let Some((done, true)) = channel.completion().filter(|(t, _)| *t >= started) {
                if channel.read(Some(done)).await.ok() == Some(lease.baseline) {
                    self.finish(
                        &lease,
                        true,
                        "baseline confirmed by post-write read-back".into(),
                    );
                    return;
                }
            }
        }
        if now < progress.next_attempt_ms {
            return;
        }
        // The retained lease itself is the durable restore intent.
        if self.save().is_err() {
            return;
        }
        match channel.write(lease.baseline).await {
            Ok(_) => {
                let mut inner = self.state.lock().unwrap();
                let progress = inner.progress.entry(register).or_default();
                progress.restore_started_ms = Some(now);
                progress.last_write_ms = Some(now);
                progress.next_attempt_ms = now.saturating_add(RETRY_MS);
            }
            Err(error)
                if matches!(
                    error.code,
                    ErrorCode::Busy
                        | ErrorCode::Timeout
                        | ErrorCode::Unavailable
                        | ErrorCode::Disconnected
                        | ErrorCode::Malformed
                ) =>
            {
                // Writing an absolute baseline again is harmless, unlike retrying an unknown change.
                tracing::warn!(register, holder = %lease.holder, %error, "lease restore failed; retrying");
                // A late receipt of this attempt shows up as a write of the baseline and ends the lease.
                self.state
                    .lock()
                    .unwrap()
                    .progress
                    .entry(register)
                    .or_default()
                    .next_attempt_ms = now.saturating_add(RETRY_MS);
            }
            Err(error) => self.finish(&lease, false, error.to_string()),
        }
    }

    fn finish(&self, lease: &RegisterLease, ok: bool, detail: String) {
        let register = lease.register;
        let audit_outcome = format!(
            "{}: {detail}",
            if ok { "restored" } else { "restore failed" }
        );
        if ok {
            tracing::info!(register, holder = %lease.holder, baseline = lease.baseline, "lease lapsed; baseline restored");
        } else {
            tracing::error!(register, holder = %lease.holder, baseline = lease.baseline, %detail, "lease lapsed; restore not possible");
        }
        let at_ms = self.gateway.now_ms();
        {
            let mut inner = self.state.lock().unwrap();
            if ok {
                inner.leases.remove(&register);
            }
            inner.progress.entry(register).or_default().next_attempt_ms =
                if ok { 0 } else { u64::MAX };
            inner.restores.insert(
                register,
                RegisterLeaseRestore {
                    register,
                    at_ms,
                    holder: lease.holder.clone(),
                    baseline: lease.baseline,
                    ok,
                    detail,
                },
            );
        }
        let _ = self.save();
        if let Some(audit) = self.audit.get() {
            audit.record(nibe_persistence::audit::NewAuditEntry {
                source: nibe_persistence::audit::AuditSource::Gateway,
                actor: "gateway".into(),
                action: "Lease lapsed; earlier value restored".into(),
                target: Some(register.to_string()),
                detail: Some(format!(
                    "lease held by {}; restoring raw {}",
                    lease.holder, lease.baseline
                )),
                ok,
                outcome: Some(audit_outcome),
            });
        }
        let what = if register == HEAT_OFFSET_REGISTER {
            "heating offset".to_string()
        } else {
            format!("register {register}")
        };
        self.gateway.notify(
            "lease",
            &format!(
                "{what} lease lapsed; {}",
                if ok {
                    "baseline restored"
                } else {
                    "restore failed"
                }
            ),
        );
    }

    fn save(&self) -> Result<(), ServiceError> {
        let state = {
            let inner = self.state.lock().unwrap();
            LeaseState::from_registers(&inner.leases, &inner.restores)
        };
        if let Some(store) = &self.store {
            if let Err(error) = store.set_lease_state(&state) {
                let (leases, restores) = store.lease_state().by_register();
                let mut inner = self.state.lock().unwrap();
                inner.leases = leases;
                inner.restores = restores;
                return Err(ServiceError::new(
                    ErrorCode::Unavailable,
                    format!("cannot persist control intent: {error}"),
                ));
            }
        }
        Ok(())
    }
}

impl HeatingOffsetWriter for Leases {
    fn write_offset(&self, offset: i8) -> DeviceFuture<'_, CommandReceipt> {
        Box::pin(Leases::write_offset(
            self,
            offset,
            None,
            "unleased writer".into(),
        ))
    }
}

/// The heating offset through the gateway's named control: its gates, simulation and command history
/// apply, and the polled reading is the read-back.
struct OffsetChannel {
    gateway: Gateway,
}
impl LeaseChannel for OffsetChannel {
    fn validate(&self, raw: i32) -> Result<(), ServiceError> {
        let offset = i8::try_from(raw)
            .map_err(|_| ServiceError::validation("offset must be between -10 and 10"))?;
        Command::HeatingOffset { offset }.validate()
    }
    fn write(&self, raw: i32) -> BoxFuture<'_, Result<Written, ServiceError>> {
        Box::pin(async move {
            self.validate(raw)?;
            let receipt = self
                .gateway
                .dispatch_command(Command::HeatingOffset { offset: raw as i8 })
                .await?;
            Ok(Written {
                detail: receipt.detail.clone(),
                receipt: Some(receipt),
            })
        })
    }
    fn read(&self, after_ms: Option<u64>) -> BoxFuture<'_, Result<i32, ServiceError>> {
        Box::pin(async move {
            let snapshot = self.gateway.snapshot();
            // Disabled controls report their usual error, not a missing read-back.
            if !snapshot.controls_enabled {
                return Err(ServiceError::new(
                    ErrorCode::ControlsDisabled,
                    "controls disabled",
                ));
            }
            current_offset(&snapshot)
                .filter(|_| {
                    after_ms
                        .is_none_or(|after| offset_observed(&snapshot).is_some_and(|t| t > after))
                })
                .map(i32::from)
                .ok_or_else(|| {
                    ServiceError::new(ErrorCode::Unavailable, "no fresh heating offset reading")
                })
        })
    }
    fn completion(&self) -> Option<(u64, bool)> {
        self.gateway.offset_completion()
    }
    fn restore(&self) -> Restore {
        // The owner's own curve offset, which differs per house.
        Restore::Earlier
    }
}
fn offset_observed(snapshot: &DeviceSnapshot) -> Option<u64> {
    snapshot
        .measurements
        .iter()
        .find(|m| m.id == RegisterId::from(HEAT_OFFSET_REGISTER) || m.name == SIMULATED_NAME)
        .map(|m| m.observed_at_ms)
}

/// Fresh read-back of the heating offset, if any.
fn current_offset(snapshot: &DeviceSnapshot) -> Option<i8> {
    let reading = snapshot
        .measurements
        .iter()
        .find(|m| m.id == RegisterId::from(HEAT_OFFSET_REGISTER) || m.name == SIMULATED_NAME)
        .filter(|m| !m.stale)?;
    let value = match reading.value.as_ref()? {
        RegisterValue::Integer(v) => *v as f64,
        RegisterValue::Decimal(v) => *v,
        _ => return None,
    };
    (value.fract() == 0.0 && (-10.0..=10.0).contains(&value)).then_some(value as i8)
}

#[cfg(all(test, feature = "dev"))]
mod tests {
    use super::*;
    use nibe_simulator::{SimClock, SimulatedDevice};

    type DispatchHook = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;
    struct Rig {
        hook: DispatchHook,
        gateway: Gateway,
        clock: Arc<SimClock>,
        store: Arc<Store>,
        _service: DeviceService,
        _dir: tempfile::TempDir,
        reject: Arc<Mutex<Option<ErrorCode>>>,
    }
    type Completion = Arc<Mutex<Option<(u64, bool)>>>;
    struct QueuedDevice {
        hook: DispatchHook,
        inner: SimulatedDevice,
        completion: Completion,
        reject: Arc<Mutex<Option<ErrorCode>>>,
    }
    impl Device for QueuedDevice {
        fn mode(&self) -> DeviceMode {
            DeviceMode::Hardware
        }
        fn offset_completion(&self) -> Option<(u64, bool)> {
            *self.completion.lock().unwrap()
        }
        fn poll(&mut self, now: u64) -> DeviceFuture<'_, DeviceUpdate> {
            self.inner.poll(now)
        }
        fn execute(&mut self, command: Command, now: u64) -> DeviceFuture<'_, String> {
            if let Some(hook) = self.hook.lock().unwrap().take() {
                hook();
            }
            if let Some(code) = self.reject.lock().unwrap().take() {
                return Box::pin(async move {
                    Err(ServiceError::new(code, "injected dispatch rejection"))
                });
            }
            self.inner.execute(command, now)
        }
        fn discover(&mut self) -> DeviceFuture<'_, DiscoveryStatus> {
            self.inner.discover()
        }
    }
    fn rig(state: Option<LeaseState>) -> Rig {
        rig_with(state, None)
    }
    fn rig_with(state: Option<LeaseState>, completion: Option<Completion>) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "1234").unwrap());
        if let Some(state) = state {
            store.set_lease_state(&state).unwrap();
        }
        let clock = Arc::new(SimClock::default());
        clock.freeze(true);
        clock.reset();
        let device = SimulatedDevice::new(clock.clone(), "normal_heating").unwrap();
        let reject = Arc::new(Mutex::new(None));
        let hook: DispatchHook = Arc::new(Mutex::new(None));
        let device: Box<dyn Device> = match completion {
            Some(completion) => Box::new(QueuedDevice {
                hook: hook.clone(),
                inner: device,
                completion,
                reject: reject.clone(),
            }),
            None => Box::new(device),
        };
        let (gateway, service) = start_device(
            device,
            clock.clone(),
            ServiceConfig {
                poll_interval: Duration::from_millis(10),
                allow_simulated_controls: true,
                hardware_controls: true,
                ..ServiceConfig::default()
            },
        );
        Rig {
            hook,
            gateway,
            clock,
            store,
            _service: service,
            _dir: dir,
            reject,
        }
    }
    async fn until(what: &str, mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }
    fn lease(offset: i8, baseline: i8, expires_at_ms: u64) -> LeaseState {
        LeaseState {
            heating_offset: Some(ControlLease {
                holder: "neo".into(),
                offset,
                baseline,
                granted_at_ms: 0,
                expires_at_ms,
                pending_supersession: None,
            }),
            ..LeaseState::default()
        }
    }

    #[tokio::test]
    async fn lease_crash_before_owner_outcome_retains_durable_intent() {
        for rejection in [None, Some(ErrorCode::Busy)] {
            let rig = rig_with(None, Some(Arc::new(Mutex::new(None))));
            let leases = Leases::start_with(
                rig.gateway.clone(),
                Some(rig.store.clone()),
                Duration::from_secs(3600),
            );
            until("reading", || {
                current_offset(&rig.gateway.snapshot()).is_some()
            })
            .await;
            leases
                .write_offset(-10, Some(60), "neo".into())
                .await
                .unwrap();
            let captured = Arc::new(Mutex::new(None));
            let copy = captured.clone();
            let path = rig._dir.path().join("control-lease.json");
            *rig.hook.lock().unwrap() = Some(Box::new(move || {
                *copy.lock().unwrap() = Some(std::fs::read(path).unwrap());
            }));
            *rig.reject.lock().unwrap() = rejection;
            let _ = leases.write_offset(2, None, "owner".into()).await;
            let bytes = captured.lock().unwrap().take().unwrap();
            let persisted: LeaseState = serde_json::from_slice(&bytes).unwrap();
            assert!(
                persisted.heating_offset.is_some(),
                "crash before dispatch/outcome lost restore protection"
            );
            assert_eq!(
                persisted
                    .heating_offset
                    .unwrap()
                    .pending_supersession
                    .unwrap()
                    .0,
                2
            );
            // The captured bytes are identical before dispatch and after Busy but before
            // rollback (or after acceptance but before clearing). Reload each crash image.
            for pump_offset in [-10, 2] {
                let fresh = self::rig(None);
                fresh
                    .gateway
                    .dispatch_command(Command::HeatingOffset {
                        offset: pump_offset,
                    })
                    .await
                    .unwrap();
                let dir = tempfile::tempdir().unwrap();
                std::fs::write(dir.path().join("control-lease.json"), &bytes).unwrap();
                let store = Arc::new(Store::open(dir.path(), "1234").unwrap());
                let recovered = Leases::start_with(
                    fresh.gateway.clone(),
                    Some(store.clone()),
                    Duration::from_secs(3600),
                );
                recovered.check().await;
                assert!(
                    recovered.status().heating_offset.is_some(),
                    "cached reading cannot reconcile"
                );
                fresh.clock.advance(1).unwrap();
                until("restart read-back", || {
                    offset_observed(&fresh.gateway.snapshot()) == Some(fresh.gateway.now_ms())
                })
                .await;
                recovered.check().await;
                assert_eq!(
                    store.lease_state().heating_offset.is_none(),
                    pump_offset == 2
                );
                fresh.clock.advance(60_000).unwrap();
                recovered.check().await;
                until("expiry value", || {
                    current_offset(&fresh.gateway.snapshot())
                        == Some(if pump_offset == 2 { 2 } else { 0 })
                })
                .await;
            }
        }
    }

    #[tokio::test]
    async fn lease_unresolved_supersession_never_blocks_a_new_owner_write() {
        let mut state = lease(-10, 0, 60_000);
        state.heating_offset.as_mut().unwrap().pending_supersession = Some((2, 0));
        let rig = rig(Some(state));
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        // No reading after the restart: the earlier owner outcome stays unknown.
        let renewal = leases
            .write_offset(-8, Some(60), "neo".into())
            .await
            .unwrap_err();
        assert_eq!(renewal.code, ErrorCode::Unavailable);
        leases.write_offset(3, None, "owner".into()).await.unwrap();
        assert!(leases.status().heating_offset.is_none());
        assert!(rig.store.lease_state().heating_offset.is_none());
    }

    #[tokio::test]
    async fn lease_completion_before_caller_resumes_allows_next_baseline() {
        let completion = Arc::new(Mutex::new(None));
        let rig = rig_with(None, Some(completion.clone()));
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        let clock = rig.clock.clone();
        *rig.hook.lock().unwrap() = Some(Box::new(move || {
            *completion.lock().unwrap() = Some((clock.now_ms(), true));
            clock.advance(10).unwrap();
        }));
        leases.write_offset(2, None, "owner".into()).await.unwrap();
        rig.clock.advance(1).unwrap();
        until("post-completion reading", || {
            offset_observed(&rig.gateway.snapshot()) == Some(rig.gateway.now_ms())
                && rig.gateway.offset_completion().is_some()
        })
        .await;
        leases
            .write_offset(-10, Some(60), "neo".into())
            .await
            .unwrap();
        assert_eq!(leases.status().heating_offset.unwrap().baseline, 2);
    }

    #[tokio::test]
    async fn lease_definite_rejection_reinstates_durable_owner_lease() {
        let rig = rig_with(None, Some(Arc::new(Mutex::new(None))));
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        leases
            .write_offset(-10, Some(60), "neo".into())
            .await
            .unwrap();
        for code in [
            ErrorCode::Busy,
            ErrorCode::Validation,
            ErrorCode::ControlsDisabled,
        ] {
            *rig.reject.lock().unwrap() = Some(code.clone());
            assert_eq!(
                rig.gateway
                    .command(Command::HeatingOffset { offset: 2 })
                    .await
                    .unwrap_err()
                    .code,
                code
            );
            assert_eq!(leases.status().heating_offset.unwrap().offset, -10);
            assert_eq!(rig.store.lease_state().heating_offset.unwrap().offset, -10);
        }
        rig.clock.advance(60_000).unwrap();
        leases.check().await;
        assert_eq!(
            rig.gateway.command_history().last().unwrap().command,
            Command::HeatingOffset { offset: 0 }
        );
    }

    #[tokio::test]
    async fn lease_definite_rejection_does_not_require_nonexistent_completion() {
        let rig = rig_with(None, Some(Arc::new(Mutex::new(None))));
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        *rig.reject.lock().unwrap() = Some(ErrorCode::Busy);
        assert_eq!(
            leases
                .write_offset(2, None, "owner".into())
                .await
                .unwrap_err()
                .code,
            ErrorCode::Busy
        );
        rig.clock.advance(1).unwrap();
        until("fresh reading", || {
            offset_observed(&rig.gateway.snapshot()) == Some(rig.gateway.now_ms())
        })
        .await;
        leases
            .write_offset(-10, Some(60), "neo".into())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn lease_lapsed_while_stopped_is_restored_at_startup() {
        // Persisted before a restart; expired by the time the gateway is back.
        let rig = rig(Some(lease(5, -3, 1_000)));
        rig.gateway
            .command(Command::HeatingOffset { offset: 5 })
            .await
            .unwrap();
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_millis(10),
        );
        let audit = Arc::new(nibe_persistence::audit::AuditLog::memory());
        leases.set_audit(audit.clone());
        until("the startup restore", || {
            current_offset(&rig.gateway.snapshot()) == Some(-3)
        })
        .await;
        rig.clock.advance(1).unwrap();
        until("the persisted restore", || {
            rig.store.lease_state().last_restore.is_some()
        })
        .await;
        let status = leases.status();
        assert!(status.heating_offset.is_none());
        let restore = status.last_restore.unwrap();
        assert!(restore.ok, "{}", restore.detail);
        assert_eq!((restore.baseline, restore.holder.as_str()), (-3, "neo"));
        assert!(rig.store.lease_state().heating_offset.is_none());
        // The restore is in the audit log, by the gateway, naming the holder and the value.
        let (entries, _) = audit.page(None, 10);
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.source, nibe_persistence::audit::AuditSource::Gateway);
        assert_eq!(
            (entry.actor.as_str(), entry.target.as_deref(), entry.ok),
            ("gateway", Some("47011"), true)
        );
        assert!(entry.detail.as_deref().unwrap().contains("neo"));
        assert!(entry.outcome.as_deref().unwrap().starts_with("restored"));
    }

    #[tokio::test]
    async fn active_lease_survives_restart_but_never_beyond_the_maximum() {
        let rig = rig(None);
        let far = rig.gateway.now_ms() + 86_400_000;
        rig.store.set_lease_state(&lease(4, 0, far)).unwrap();
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_millis(10),
        );
        let status = leases.status();
        assert_eq!(status.remaining_s, Some(u64::from(MAX_LEASE_SECONDS)));
        assert_eq!(
            rig.store
                .lease_state()
                .heating_offset
                .unwrap()
                .expires_at_ms,
            rig.gateway.now_ms() + u64::from(MAX_LEASE_SECONDS) * 1000
        );
        rig.clock
            .advance(u64::from(MAX_LEASE_SECONDS) * 1000)
            .unwrap();
        until("restore dispatch", || {
            current_offset(&rig.gateway.snapshot()) == Some(0)
        })
        .await;
        rig.clock.advance(1).unwrap();
        until("the restore", || leases.status().last_restore.is_some()).await;
    }

    #[tokio::test]
    async fn a_write_that_bypasses_the_lease_ends_it_without_a_restore() {
        let rig = rig(None);
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_millis(10),
        );
        until("a reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        let baseline = current_offset(&rig.gateway.snapshot()).unwrap();
        leases
            .write_offset(6, Some(60), "neo".into())
            .await
            .unwrap();
        assert_eq!(leases.status().heating_offset.unwrap().baseline, baseline);
        // MQTT commands go straight to the gateway, not through the lease.
        rig.gateway
            .command(Command::HeatingOffset { offset: 2 })
            .await
            .unwrap();
        until("the lease to end", || {
            leases.status().heating_offset.is_none()
        })
        .await;
        rig.clock.advance(61_000).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(leases.status().last_restore.is_none());
        until("the read-back", || {
            current_offset(&rig.gateway.snapshot()) == Some(2)
        })
        .await;
    }

    #[tokio::test]
    async fn lease_queued_restore_requires_terminal_outcome_and_later_reading() {
        let completion = Arc::new(Mutex::new(None));
        let rig = rig_with(None, Some(completion.clone()));
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        leases
            .write_offset(-10, Some(60), "neo".into())
            .await
            .unwrap();
        rig.clock.advance(60_000).unwrap();
        leases.check().await;
        rig.clock.advance(1).unwrap();
        until("baseline reading", || {
            current_offset(&rig.gateway.snapshot()) == Some(0)
        })
        .await;
        leases.check().await;
        assert!(
            leases.status().last_restore.is_none(),
            "queued is not applied"
        );
        *completion.lock().unwrap() = Some((rig.gateway.now_ms(), true));
        until("terminal outcome", || {
            rig.gateway.offset_completion().is_some()
        })
        .await;
        leases.check().await;
        assert!(
            leases.status().last_restore.is_none(),
            "cached pre-completion reading is insufficient"
        );
        rig.clock.advance(1).unwrap();
        until("later reading", || {
            offset_observed(&rig.gateway.snapshot()) == Some(rig.gateway.now_ms())
        })
        .await;
        leases.check().await;
        assert!(leases.status().last_restore.unwrap().ok);
    }

    #[tokio::test]
    async fn lease_mqtt_dispatch_waits_for_the_restore_operation_lock() {
        let rig = rig(None);
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        leases
            .write_offset(-10, Some(60), "neo".into())
            .await
            .unwrap();
        let guard = leases.operation.lock().await;
        let before = rig.gateway.command_history().len();
        let gateway = rig.gateway.clone();
        let writer =
            tokio::spawn(
                async move { gateway.command(Command::HeatingOffset { offset: 2 }).await },
            );
        tokio::task::yield_now().await;
        assert!(!writer.is_finished());
        assert_eq!(rig.gateway.command_history().len(), before);
        drop(guard);
        writer.await.unwrap().unwrap();
        rig.clock.advance(60_000).unwrap();
        leases.check().await;
        assert!(leases.status().heating_offset.is_none());
        assert!(leases.status().last_restore.is_none());
    }

    #[tokio::test]
    async fn lease_owner_supersession_requires_a_new_confirmed_baseline() {
        let rig = rig(None);
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        leases
            .write_offset(-10, Some(60), "neo".into())
            .await
            .unwrap();
        rig.gateway
            .command(Command::HeatingOffset { offset: 2 })
            .await
            .unwrap();
        assert!(leases.status().heating_offset.is_none());
        assert_eq!(
            leases
                .write_offset(-9, Some(60), "neo".into())
                .await
                .unwrap_err()
                .code,
            ErrorCode::Unavailable
        );
        rig.clock.advance(1).unwrap();
        until("owner reading", || {
            offset_observed(&rig.gateway.snapshot()) == Some(rig.gateway.now_ms())
        })
        .await;
        leases
            .write_offset(-9, Some(60), "neo".into())
            .await
            .unwrap();
        assert_eq!(leases.status().heating_offset.unwrap().baseline, 2);
    }

    #[tokio::test]
    async fn lease_persistence_failure_refuses_dispatch_and_preserves_previous_intent() {
        let rig = rig(None);
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        leases
            .write_offset(-10, Some(60), "neo".into())
            .await
            .unwrap();
        let before = rig.gateway.command_history().len();
        std::fs::remove_file(rig._dir.path().join("control-lease.json")).unwrap();
        std::fs::create_dir(rig._dir.path().join("control-lease.json")).unwrap();
        assert_eq!(
            rig.gateway
                .command(Command::HeatingOffset { offset: 2 })
                .await
                .unwrap_err()
                .code,
            ErrorCode::Unavailable
        );
        assert_eq!(rig.gateway.command_history().len(), before);
        assert!(leases.status().heating_offset.is_some());
    }

    #[tokio::test]
    async fn lease_restore_waits_for_post_completion_reading_and_keeps_baseline() {
        let rig = rig(None);
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        let baseline = current_offset(&rig.gateway.snapshot()).unwrap();
        leases
            .write_offset(-10, Some(60), "neo".into())
            .await
            .unwrap();
        rig.clock.advance(60_000).unwrap();
        leases.check().await;
        leases.check().await;
        assert!(leases.status().last_restore.is_none());
        assert_eq!(
            rig.store.lease_state().heating_offset.unwrap().baseline,
            baseline
        );
        leases
            .write_offset(-8, Some(60), "neo".into())
            .await
            .unwrap();
        assert_eq!(leases.status().heating_offset.unwrap().baseline, baseline);
    }

    #[tokio::test]
    async fn leased_write_needs_a_fresh_read_back() {
        let rig = rig(None);
        let leases = Leases::start_with(rig.gateway.clone(), None, Duration::from_millis(10));
        until("a reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        // Readings older than stale_after_ms are no baseline.
        rig.clock.freeze(false);
        rig.gateway
            .dev_action(DevAction::Fault {
                kind: "stale".into(),
            })
            .await
            .unwrap();
        rig.clock.advance(10_000).unwrap();
        let error = leases
            .write_offset(1, Some(60), "neo".into())
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(leases.status().heating_offset.is_none());
    }

    /// A setpoint as the F730 adapter shows it: writes complete when told, reads return the pump value.
    #[derive(Default)]
    struct Setpoint {
        clock: Option<Arc<SimClock>>,
        fixed: Option<i32>,
        value: Mutex<i32>,
        pending: Mutex<Option<i32>>,
        completion: Mutex<Option<(u64, bool)>>,
        writes: Mutex<Vec<i32>>,
    }
    impl Setpoint {
        fn finish(&self, accepted: bool) {
            let raw = self.pending.lock().unwrap().take().unwrap();
            if accepted {
                *self.value.lock().unwrap() = raw;
            }
            *self.completion.lock().unwrap() =
                Some((self.clock.as_ref().unwrap().now_ms(), accepted));
        }
    }
    impl LeaseChannel for Setpoint {
        fn write(&self, raw: i32) -> BoxFuture<'_, Result<Written, ServiceError>> {
            self.writes.lock().unwrap().push(raw);
            *self.pending.lock().unwrap() = Some(raw);
            *self.completion.lock().unwrap() = None;
            Box::pin(async move {
                Ok(Written {
                    receipt: None,
                    detail: "queued".into(),
                })
            })
        }
        fn read(&self, _after_ms: Option<u64>) -> BoxFuture<'_, Result<i32, ServiceError>> {
            Box::pin(async move { Ok(*self.value.lock().unwrap()) })
        }
        fn completion(&self) -> Option<(u64, bool)> {
            *self.completion.lock().unwrap()
        }
        fn restore(&self) -> Restore {
            self.fixed.map_or(Restore::Earlier, Restore::Fixed)
        }
    }

    #[test]
    fn holiday_mode_lapses_to_off_and_counters_are_not_leasable() {
        let restore = |register| {
            REGISTER_LEASES
                .iter()
                .find(|(r, _)| *r == register)
                .map(|(_, restore)| *restore)
        };
        assert_eq!(restore(48043), Some(Restore::Fixed(0)));
        assert_eq!(restore(47049), Some(Restore::Earlier));
        for excluded in [43005, 40940, 45171] {
            assert_eq!(restore(excluded), None);
        }
    }

    /// Holiday mode was on at the pump when the first lease came, whether the writer keeps it on or turns
    /// it off: the earlier 10 is not restored either way.
    #[tokio::test]
    async fn a_fixed_restore_ignores_what_the_pump_had_before_the_lease() {
        for leased in [10, 0] {
            let rig = rig(None);
            let leases = Leases::start_with(
                rig.gateway.clone(),
                Some(rig.store.clone()),
                Duration::from_secs(3600),
            );
            let holiday = Arc::new(Setpoint {
                clock: Some(rig.clock.clone()),
                fixed: Some(0),
                value: Mutex::new(10),
                ..Setpoint::default()
            });
            leases.add_channel(48043, holiday.clone());
            leases
                .write(48043, leased, Some(60), "controller".into())
                .await
                .unwrap();
            holiday.finish(true);
            assert_eq!(leases.register_status().leases[0].lease.baseline, 0);
            // A renewal keeps it, and the lease restores it on lapse like any other.
            leases
                .write(48043, leased, Some(60), "controller".into())
                .await
                .unwrap();
            holiday.finish(true);
            rig.clock.advance(60_000).unwrap();
            leases.check().await;
            assert_eq!(*holiday.writes.lock().unwrap(), [leased, leased, 0]);
            holiday.finish(true);
            leases.check().await;
            let saved = rig.store.lease_state();
            assert!(saved.restores[&48043].ok);
            assert!(!saved.registers.contains_key(&48043));
            assert_eq!(*holiday.value.lock().unwrap(), 0);
        }
    }

    #[tokio::test]
    async fn a_setpoint_lease_restores_through_its_own_channel_independently_of_the_offset() {
        let rig = rig(None);
        let leases = Leases::start_with(
            rig.gateway.clone(),
            Some(rig.store.clone()),
            Duration::from_secs(3600),
        );
        let setpoint = Arc::new(Setpoint {
            clock: Some(rig.clock.clone()),
            value: Mutex::new(450),
            ..Setpoint::default()
        });
        leases.add_channel(47049, setpoint.clone());
        until("reading", || {
            current_offset(&rig.gateway.snapshot()).is_some()
        })
        .await;
        leases
            .write_offset(-10, Some(120), "neo".into())
            .await
            .unwrap();
        leases
            .write(47049, 300, Some(60), "neo".into())
            .await
            .unwrap();
        setpoint.finish(true);
        rig.clock.advance(60_000).unwrap();
        leases.check().await;
        assert_eq!(*setpoint.writes.lock().unwrap(), [300, 450]);
        assert!(
            leases.status().heating_offset.is_some(),
            "offset lease unaffected"
        );
        leases.check().await;
        assert!(
            rig.store.lease_state().restores.is_empty(),
            "queued is not restored"
        );
        setpoint.finish(true);
        leases.check().await;
        let saved = rig.store.lease_state();
        assert!(saved.restores[&47049].ok);
        assert!(!saved.registers.contains_key(&47049));
        assert!(saved.heating_offset.is_some());
    }
}
