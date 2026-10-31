//! Control lease on the heating offset (ADR 0123). A writer may attach `lease_seconds` to a heating-offset
//! write; if it does not write again before the lease lapses, the gateway writes the baseline back through
//! the same named control (so the ADR 0104 gates still apply). Nothing else is automated.
use nibe_core::*;
use nibe_persistence::{ControlLease, LeaseRestore, LeaseState, Store};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use utoipa::ToSchema;

pub const MIN_LEASE_SECONDS: u32 = 60;
pub const MAX_LEASE_SECONDS: u32 = 3600;
/// Wait between restore attempts after a transient failure (busy queue, wire down, timeout).
const RETRY_MS: u64 = 30_000;
/// F730 heat offset S1. The simulator has no 47011 and names its synthetic register `heating_offset`.
const HEAT_OFFSET_REGISTER: RegisterId = 47011;
const SIMULATED_NAME: &str = "heating_offset";

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LeaseStatus {
    /// The active lease, or one that lapsed and whose restore is still being retried.
    pub heating_offset: Option<ControlLease>,
    /// Seconds until the baseline is restored; 0 once lapsed.
    pub remaining_s: Option<u64>,
    pub last_restore: Option<LeaseRestore>,
}

pub struct Leases {
    gateway: Gateway,
    store: Option<Arc<Store>>,
    /// Serializes leased writes, unleased writes and restores, so a renewal never races a restore.
    operation: tokio::sync::Mutex<()>,
    state: Mutex<Inner>,
}
#[derive(Clone)]
struct Inner {
    lease: LeaseState,
    next_attempt_ms: u64,
    restore_started_ms: Option<u64>,
    last_write_ms: Option<u64>,
    recovery_after_ms: u64,
}

impl Leases {
    pub fn start(gateway: Gateway, store: Option<Arc<Store>>) -> Arc<Self> {
        Self::start_with(gateway, store, Duration::from_secs(1))
    }
    /// `tick` is how often expiry is checked against the gateway clock (tests use a short one).
    pub fn start_with(gateway: Gateway, store: Option<Arc<Store>>, tick: Duration) -> Arc<Self> {
        let mut lease = store.as_ref().map(|s| s.lease_state()).unwrap_or_default();
        // A clock still behind real time (no RTC, before NTP) must not stretch a persisted lease: it ends
        // at most one maximum lease from now. A later forward NTP step only makes it end sooner.
        let latest = gateway
            .now_ms()
            .saturating_add(u64::from(MAX_LEASE_SECONDS) * 1000);
        if let Some(active) = &mut lease.heating_offset {
            active.expires_at_ms = active.expires_at_ms.min(latest);
            tracing::info!(
                holder = %active.holder,
                baseline = active.baseline,
                "heating offset lease loaded from state"
            );
        }
        let leases = Arc::new(Self {
            gateway: gateway.clone(),
            store,
            operation: tokio::sync::Mutex::new(()),
            state: Mutex::new(Inner {
                lease,
                next_attempt_ms: 0,
                restore_started_ms: None,
                last_write_ms: None,
                recovery_after_ms: gateway
                    .now_ms()
                    .max(offset_observed(&gateway.snapshot()).unwrap_or(0)),
            }),
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

    pub fn status(&self) -> LeaseStatus {
        let inner = self.state.lock().unwrap();
        let now = self.gateway.now_ms();
        LeaseStatus {
            heating_offset: inner.lease.heating_offset.clone(),
            remaining_s: inner
                .lease
                .heating_offset
                .as_ref()
                .map(|l| l.expires_at_ms.saturating_sub(now).div_ceil(1000)),
            last_restore: inner.lease.last_restore.clone(),
        }
    }

    /// A heating-offset write. With `lease_seconds` it starts or renews the lease; without, it behaves as
    /// before and ends any lease (whoever wrote last decided the value).
    pub async fn write(
        &self,
        offset: i8,
        lease_seconds: Option<u32>,
        holder: String,
    ) -> Result<CommandReceipt, ServiceError> {
        let command = Command::HeatingOffset { offset };
        command.validate()?;
        if lease_seconds.is_some_and(|s| !(MIN_LEASE_SECONDS..=MAX_LEASE_SECONDS).contains(&s)) {
            return Err(ServiceError::validation(format!(
                "lease_seconds must be between {MIN_LEASE_SECONDS} and {MAX_LEASE_SECONDS}"
            )));
        }
        let _operation = self.operation.lock().await;
        // A new owner write replaces an unresolved one (its marker carries the new request), so
        // a missing read-back never locks the owner out; only renewals must wait for the outcome.
        if !self.reconcile_supersession()? && lease_seconds.is_some() {
            return Err(ServiceError::new(
                ErrorCode::Unavailable,
                "waiting for fresh owner supersession read-back",
            ));
        }
        let baseline = match lease_seconds {
            None => None,
            Some(_) => {
                let existing = self
                    .state
                    .lock()
                    .unwrap()
                    .lease
                    .heating_offset
                    .as_ref()
                    .map(|l| l.baseline);
                let snapshot = self.gateway.snapshot();
                match existing {
                    Some(baseline) => Some(baseline),
                    // Disabled controls report their usual error, not a missing read-back.
                    None if !snapshot.controls_enabled => None,
                    None => Some(current_offset(&snapshot).filter(|_| {
                        let inner = self.state.lock().unwrap();
                        inner.last_write_ms.is_none_or(|at| self.gateway.offset_completion().is_some_and(|(done, _)| done >= at && offset_observed(&snapshot).is_some_and(|t| t > done)))
                    }).ok_or_else(|| {
                        ServiceError::new(
                            ErrorCode::Unavailable,
                            "no fresh heating offset reading to use as the lease baseline; retry after the next poll",
                        )
                    })?),
                }
            }
        };
        let previous_intent = self.state.lock().unwrap().clone();
        let previous_holder = self.status().heating_offset.map(|l| l.holder);
        // Intent is durable before a request can reach the device.
        let now = self.gateway.now_ms();
        {
            let mut inner = self.state.lock().unwrap();
            let previous = inner.lease.heating_offset.take();
            if let (Some(seconds), Some(baseline)) = (lease_seconds, baseline) {
                let expires_at_ms = now.saturating_add(u64::from(seconds) * 1000);
                tracing::info!(%holder, offset, baseline, seconds, "heating offset lease granted");
                inner.lease.heating_offset = Some(ControlLease {
                    granted_at_ms: previous.map_or(now, |p| p.granted_at_ms),
                    holder,
                    offset,
                    baseline,
                    expires_at_ms,
                    pending_supersession: None,
                });

                inner.next_attempt_ms = 0;
            } else {
                inner.lease.heating_offset = previous.map(|mut lease| {
                    lease.pending_supersession = Some((offset, now));
                    lease
                });
            }
        }
        self.save()?;
        let receipt = self.gateway.dispatch_command(command).await.map(|mut r| {
            if let Some(seconds) = lease_seconds {
                r.detail = format!("{}; lease {seconds} s", r.detail);
            } else if let Some(holder) = previous_holder {
                r.detail = format!(
                    "{}; the lease held by {holder} ended without a restore",
                    r.detail
                );
            }
            r
        });
        // These F730/core errors occur before enqueueing: queue saturation, range/encoding
        // validation and control gates. Unavailable is ambiguous (also a lost reply after
        // enqueueing), as are Timeout/Disconnected/Malformed/Rejected; never roll those back.
        if receipt.as_ref().is_err_and(|e| {
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
            return receipt;
        }
        {
            let mut inner = self.state.lock().unwrap();
            inner.last_write_ms = Some(now);
            inner.restore_started_ms = None;
            if lease_seconds.is_none() {
                inner.lease.heating_offset = None;
            }
        }
        self.save()?;
        receipt
    }

    // An unresolved durable marker means a crash hid the dispatch outcome. Only a new
    // read-back can decide: matching the owner's request ends protection; any other
    // value retains the lease and its original expiry/baseline. Never replay the owner write.
    fn reconcile_supersession(&self) -> Result<bool, ServiceError> {
        let pending = self
            .state
            .lock()
            .unwrap()
            .lease
            .heating_offset
            .as_ref()
            .and_then(|l| l.pending_supersession);
        let Some((requested, _)) = pending else {
            return Ok(true);
        };
        let snapshot = self.gateway.snapshot();
        let after = self.state.lock().unwrap().recovery_after_ms;
        let Some(value) = current_offset(&snapshot)
            .filter(|_| offset_observed(&snapshot).is_some_and(|t| t > after))
        else {
            return Ok(false);
        };
        {
            let mut inner = self.state.lock().unwrap();
            if value == requested {
                inner.lease.heating_offset = None;
            } else if let Some(lease) = &mut inner.lease.heating_offset {
                lease.pending_supersession = None;
            }
        }
        self.save()?;
        Ok(true)
    }

    /// Restores under the same coordinator lock used by HTTP and MQTT writes.
    async fn check(&self) {
        let _operation = self.operation.lock().await;
        if !matches!(self.reconcile_supersession(), Ok(true)) {
            return;
        }
        let (lease, next_attempt_ms) = {
            let inner = self.state.lock().unwrap();
            let Some(lease) = inner.lease.heating_offset.clone() else {
                return;
            };
            (lease, inner.next_attempt_ms)
        };
        let now = self.gateway.now_ms();
        if now < lease.expires_at_ms {
            return;
        }
        let started = self.state.lock().unwrap().restore_started_ms;
        if let Some(started) = started {
            if let Some((done, accepted)) = self
                .gateway
                .offset_completion()
                .filter(|(t, _)| *t >= started)
            {
                let snapshot = self.gateway.snapshot();
                if accepted
                    && offset_observed(&snapshot).is_some_and(|t| t > done)
                    && current_offset(&snapshot) == Some(lease.baseline)
                {
                    self.finish(
                        &lease,
                        true,
                        "baseline confirmed by post-write read-back".into(),
                    );
                    return;
                }
            }
            if now < next_attempt_ms {
                return;
            }
        }
        if now < next_attempt_ms {
            return;
        }
        // The retained lease itself is the durable restore intent.
        if self.save().is_err() {
            return;
        }
        match self
            .gateway
            .dispatch_command(Command::HeatingOffset {
                offset: lease.baseline,
            })
            .await
        {
            Ok(_) => {
                let mut inner = self.state.lock().unwrap();
                inner.restore_started_ms = Some(now);
                inner.last_write_ms = Some(now);
                inner.next_attempt_ms = now.saturating_add(RETRY_MS);
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
                tracing::warn!(holder = %lease.holder, %error, "heating offset lease restore failed; retrying");
                // A late receipt of this attempt shows up as a write of the baseline and ends the lease.
                self.state.lock().unwrap().next_attempt_ms = now.saturating_add(RETRY_MS);
            }
            Err(error) => self.finish(&lease, false, error.to_string()),
        }
    }

    fn finish(&self, lease: &ControlLease, ok: bool, detail: String) {
        let at_ms = self.gateway.now_ms();
        if ok {
            tracing::info!(holder = %lease.holder, baseline = lease.baseline, "heating offset lease lapsed; baseline restored");
        } else {
            tracing::error!(holder = %lease.holder, baseline = lease.baseline, %detail, "heating offset lease lapsed; restore not possible");
        }
        {
            let mut inner = self.state.lock().unwrap();
            if ok {
                inner.lease.heating_offset = None;
            }
            inner.next_attempt_ms = if ok { 0 } else { u64::MAX };
            inner.lease.last_restore = Some(LeaseRestore {
                at_ms,
                holder: lease.holder.clone(),
                baseline: lease.baseline,
                ok,
                detail,
            });
        }
        let _ = self.save();
        self.gateway.notify(
            "lease",
            if ok {
                "heating offset lease lapsed; baseline restored"
            } else {
                "heating offset lease lapsed; restore failed"
            },
        );
    }

    fn save(&self) -> Result<(), ServiceError> {
        let state = self.state.lock().unwrap().lease.clone();
        if let Some(store) = &self.store {
            if let Err(error) = store.set_lease_state(&state) {
                self.state.lock().unwrap().lease = store.lease_state();
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
        Box::pin(self.write(offset, None, "unleased writer".into()))
    }
}
fn offset_observed(snapshot: &DeviceSnapshot) -> Option<u64> {
    snapshot
        .measurements
        .iter()
        .find(|m| m.id == HEAT_OFFSET_REGISTER || m.name == SIMULATED_NAME)
        .map(|m| m.observed_at_ms)
}

/// Fresh read-back of the heating offset, if any.
fn current_offset(snapshot: &DeviceSnapshot) -> Option<i8> {
    let reading = snapshot
        .measurements
        .iter()
        .find(|m| m.id == HEAT_OFFSET_REGISTER || m.name == SIMULATED_NAME)
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
            last_restore: None,
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
            leases.write(-10, Some(60), "neo".into()).await.unwrap();
            let captured = Arc::new(Mutex::new(None));
            let copy = captured.clone();
            let path = rig._dir.path().join("control-lease.json");
            *rig.hook.lock().unwrap() = Some(Box::new(move || {
                *copy.lock().unwrap() = Some(std::fs::read(path).unwrap());
            }));
            *rig.reject.lock().unwrap() = rejection;
            let _ = leases.write(2, None, "owner".into()).await;
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
        let renewal = leases.write(-8, Some(60), "neo".into()).await.unwrap_err();
        assert_eq!(renewal.code, ErrorCode::Unavailable);
        leases.write(3, None, "owner".into()).await.unwrap();
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
        leases.write(2, None, "owner".into()).await.unwrap();
        rig.clock.advance(1).unwrap();
        until("post-completion reading", || {
            offset_observed(&rig.gateway.snapshot()) == Some(rig.gateway.now_ms())
                && rig.gateway.offset_completion().is_some()
        })
        .await;
        leases.write(-10, Some(60), "neo".into()).await.unwrap();
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
        leases.write(-10, Some(60), "neo".into()).await.unwrap();
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
                .write(2, None, "owner".into())
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
        leases.write(-10, Some(60), "neo".into()).await.unwrap();
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
        leases.write(6, Some(60), "neo".into()).await.unwrap();
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
        leases.write(-10, Some(60), "neo".into()).await.unwrap();
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
        leases.write(-10, Some(60), "neo".into()).await.unwrap();
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
        leases.write(-10, Some(60), "neo".into()).await.unwrap();
        rig.gateway
            .command(Command::HeatingOffset { offset: 2 })
            .await
            .unwrap();
        assert!(leases.status().heating_offset.is_none());
        assert_eq!(
            leases
                .write(-9, Some(60), "neo".into())
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
        leases.write(-9, Some(60), "neo".into()).await.unwrap();
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
        leases.write(-10, Some(60), "neo".into()).await.unwrap();
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
        leases.write(-10, Some(60), "neo".into()).await.unwrap();
        rig.clock.advance(60_000).unwrap();
        leases.check().await;
        leases.check().await;
        assert!(leases.status().last_restore.is_none());
        assert_eq!(
            rig.store.lease_state().heating_offset.unwrap().baseline,
            baseline
        );
        leases.write(-8, Some(60), "neo".into()).await.unwrap();
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
        let error = leases.write(1, Some(60), "neo".into()).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(leases.status().heating_offset.is_none());
    }
}
