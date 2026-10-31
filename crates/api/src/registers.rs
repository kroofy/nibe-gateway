//! Register operations by number, shared by every integration surface (ADR 0125, ADR 0126): write any
//! register, read any register now, and choose how often it is polled (fast, slow or off; saved in the
//! settings file). The admin UI, HTTP and MQTT all call these, so they behave the same.
use crate::lease::{BoxFuture, LeaseChannel, Leases, Written, SETPOINTS};
use nibe_core::{Command, ErrorCode, Gateway, RegisterReading, RegisterValue, ServiceError};
use nibe_device_f730::{catalog::Catalog, writes::WriteState, F730Handle, HEAT_OFFSET_S1};
use nibe_persistence::{PollMode, PollOverrides, Store};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use utoipa::ToSchema;

/// A read gets at most three 5 s attempts on the bus, after any read already queued.
const READ_WAIT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WriteValue {
    /// As sent on the bus.
    Raw(i32),
    /// Multiplied by the catalog scale and rounded (47.0 °C at scale 10 is raw 470).
    Scaled(f64),
}

/// The poll lists in effect, and the subset chosen at runtime (saved in the settings file).
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct PollLists {
    pub fast: Vec<u16>,
    pub slow: Vec<u16>,
    /// Registers whose mode differs from the gateway configuration.
    pub overrides: std::collections::BTreeMap<u16, PollMode>,
}

pub struct Registers {
    handle: F730Handle,
    gateway: Gateway,
    store: Option<Arc<Store>>,
    /// Poll lists from the gateway configuration; saved overrides apply on top.
    fast: Vec<u16>,
    slow: Vec<u16>,
    /// Serializes poll changes so the settings file and the adapter cannot diverge.
    poll_lock: tokio::sync::Mutex<()>,
    /// Control leases (ADR 0123, ADR 0127): every write to a leasable register goes through them.
    leases: Option<Arc<Leases>>,
}

/// A setpoint through the shared register write (ADR 0125), as a lease sees it (ADR 0127).
struct SetpointChannel {
    handle: F730Handle,
    register: u16,
}
impl LeaseChannel for SetpointChannel {
    fn write(&self, raw: i32) -> BoxFuture<'_, Result<Written, ServiceError>> {
        Box::pin(async move {
            self.handle.write(self.register, raw).await?;
            Ok(Written {
                receipt: None,
                detail: queued(self.register, raw),
            })
        })
    }
    fn read(&self, after_ms: Option<u64>) -> BoxFuture<'_, Result<i32, ServiceError>> {
        Box::pin(async move {
            // Every accepted write is read back, so a reading after it is usually there already.
            let cached = after_ms.and_then(|after| {
                self.handle
                    .snapshot()
                    .readings
                    .get(&self.register)
                    .filter(|r| r.observed_at_ms > after)
                    .cloned()
            });
            let reading = match cached {
                Some(reading) => reading,
                None => read_now(&self.handle, self.register).await?,
            };
            match reading.raw {
                // A sentinel ("unavailable") decodes to nothing and is no value to restore.
                Some(RegisterValue::Integer(raw)) if reading.decoded.is_some() => {
                    i32::try_from(raw)
                        .map_err(|_| ServiceError::validation("raw value does not fit a register"))
                }
                _ => Err(ServiceError::new(
                    ErrorCode::Malformed,
                    reading
                        .error
                        .unwrap_or_else(|| format!("register {} has no value", self.register)),
                )),
            }
        })
    }
    fn completion(&self) -> Option<(u64, bool)> {
        let snapshot = self.handle.snapshot();
        let last = snapshot.probe.writes.last_for(self.register)?;
        match last.state {
            WriteState::Accepted => Some((last.updated_at_ms, true)),
            WriteState::Denied | WriteState::Timeout => Some((last.updated_at_ms, false)),
            WriteState::Queued | WriteState::Sent => None,
        }
    }
}
fn queued(register: u16, raw: i32) -> String {
    format!("write {register}={raw} queued; confirm with probe.writes.last and the read-back value")
}

impl Registers {
    pub fn new(
        handle: F730Handle,
        gateway: Gateway,
        store: Option<Arc<Store>>,
        fast: Vec<u16>,
        slow: Vec<u16>,
        leases: Option<Arc<Leases>>,
    ) -> Arc<Self> {
        if let Some(leases) = &leases {
            for register in SETPOINTS {
                leases.add_channel(
                    register,
                    Arc::new(SetpointChannel {
                        handle: handle.clone(),
                        register,
                    }),
                );
            }
        }
        Arc::new(Self {
            handle,
            gateway,
            store,
            fast,
            slow,
            poll_lock: tokio::sync::Mutex::new(()),
            leases,
        })
    }
    fn overrides(&self) -> PollOverrides {
        self.store
            .as_ref()
            .map(|s| s.config().poll_overrides)
            .unwrap_or_default()
    }
    /// Apply the poll choices saved before a restart. Call once at start.
    pub async fn restore_polls(&self) -> Result<(), ServiceError> {
        let overrides = self.overrides();
        if overrides.is_empty() {
            return Ok(());
        }
        let (fast, slow) = overrides.apply(&self.fast, &self.slow);
        self.handle.set_polls(fast, slow).await
    }
    /// The poll lists in effect.
    pub fn polls(&self) -> PollLists {
        let poll = &self.handle.snapshot().probe.poll;
        PollLists {
            fast: poll.registers.clone(),
            slow: poll.slow_registers.clone(),
            overrides: self.overrides().0,
        }
    }
    /// Poll `register` fast, slow, or not at all. Applied at once and saved in the settings file, so it
    /// survives restarts and updates. Same limits as the configuration (catalog registers, at most 64).
    pub async fn set_poll(&self, register: u16, mode: PollMode) -> Result<PollLists, ServiceError> {
        let _guard = self.poll_lock.lock().await;
        let mut overrides = self.overrides();
        overrides.record(register, mode, &self.fast, &self.slow);
        let (fast, slow) = overrides.apply(&self.fast, &self.slow);
        self.handle.set_polls(fast.clone(), slow.clone()).await?;
        if let Some(store) = self.store.clone() {
            let saved = overrides.clone();
            tokio::task::spawn_blocking(move || {
                store.update(|config| config.poll_overrides = saved)
            })
            .await
            .map_err(|_| unsaved())?
            .map_err(|_| unsaved())?;
        }
        Ok(PollLists {
            fast,
            slow,
            overrides: overrides.0,
        })
    }
    /// Raw value for `value` at `register`.
    pub fn raw(catalog: &Catalog, register: u16, value: WriteValue) -> Result<i32, ServiceError> {
        match value {
            WriteValue::Raw(raw) => Ok(raw),
            WriteValue::Scaled(value) => {
                let entry = catalog.entry(register).ok_or_else(|| {
                    ServiceError::validation(format!(
                        "register {register} is not in the catalog; give a raw value"
                    ))
                })?;
                let scaled = (value * f64::from(entry.scale.max(1))).round();
                if !scaled.is_finite()
                    || scaled < f64::from(i32::MIN)
                    || scaled > f64::from(i32::MAX)
                {
                    return Err(ServiceError::validation("value does not fit a register"));
                }
                Ok(scaled as i32)
            }
        }
    }
    /// Queue one write (ADR 0125): no allowlist or range, sent once, read back. Returns the raw value.
    /// Leasable registers (47011 through its named control, and the setpoints) go through the lease
    /// coordinator (ADR 0123, ADR 0127), so a write without `lease_seconds` ends any lease on them.
    pub async fn write(
        &self,
        register: u16,
        value: WriteValue,
        lease_seconds: Option<u32>,
        holder: String,
    ) -> Result<(i32, String), ServiceError> {
        let raw = Self::raw(&self.handle.snapshot().catalog, register, value)?;
        if let Some(leases) = self.leases.as_ref().filter(|l| l.is_leasable(register)) {
            let written = leases.write(register, raw, lease_seconds, holder).await?;
            return Ok((raw, written.detail));
        }
        if lease_seconds.is_some() {
            return Err(match &self.leases {
                None => ServiceError::new(
                    ErrorCode::Unsupported,
                    "control leases need the gateway state store",
                ),
                Some(_) => ServiceError::validation(format!(
                    "register {register} cannot be leased; leasable: 47011 and {SETPOINTS:?}"
                )),
            });
        }
        if register == HEAT_OFFSET_S1 {
            let offset = i8::try_from(raw)
                .map_err(|_| ServiceError::validation("offset must be between -10 and 10"))?;
            let receipt = self
                .gateway
                .command(Command::HeatingOffset { offset })
                .await?;
            return Ok((raw, receipt.detail));
        }
        self.handle.write(register, raw).await?;
        Ok((raw, queued(register, raw)))
    }
    /// Read any register number now and wait for the pump's answer. One-off reads stay out of the live
    /// snapshot (only the poll list is live), so the value is returned here instead.
    pub async fn read(&self, register: u16) -> Result<RegisterReading, ServiceError> {
        read_now(&self.handle, register).await
    }
}

async fn read_now(handle: &F730Handle, register: u16) -> Result<RegisterReading, ServiceError> {
    let observed_at = || {
        handle
            .snapshot()
            .readings
            .get(&register)
            .map(|r| r.observed_at_ms)
    };
    let before = observed_at();
    handle.read_once(register).await?;
    let deadline = tokio::time::Instant::now() + READ_WAIT;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if observed_at() != before {
            if let Some(reading) = handle.snapshot().readings.get(&register) {
                return Ok(reading.clone());
            }
        }
    }
    Err(ServiceError::new(
        ErrorCode::Timeout,
        format!("no answer for register {register}; the pump may not support it"),
    ))
}

fn unsaved() -> ServiceError {
    ServiceError::new(
        ErrorCode::Unavailable,
        "the poll change is active but could not be saved; it reverts at restart",
    )
}

/// MQTT's per-register topics use the same operations (ADR 0126).
impl nibe_mqtt::RegisterAccess for Registers {
    fn write(
        &self,
        register: u16,
        value: f64,
        raw: bool,
        lease_seconds: Option<u32>,
    ) -> nibe_mqtt::BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            let value = if raw {
                if value.fract() != 0.0
                    || value < f64::from(i32::MIN)
                    || value > f64::from(i32::MAX)
                {
                    return Err("set_raw needs an integer".into());
                }
                WriteValue::Raw(value as i32)
            } else {
                WriteValue::Scaled(value)
            };
            Registers::write(self, register, value, lease_seconds, "mqtt".into())
                .await
                .map(|(_, detail)| detail)
                .map_err(|e| e.message)
        })
    }
    fn poll(&self, register: u16, mode: &str) -> nibe_mqtt::BoxFuture<'_, Result<String, String>> {
        let mode = mode.to_string();
        Box::pin(async move {
            let parsed =
                serde_json::from_value::<PollMode>(serde_json::Value::String(mode.clone()))
                    .map_err(|_| format!("poll needs fast, slow or off, got {mode:?}"))?;
            let lists = Registers::set_poll(self, register, parsed)
                .await
                .map_err(|e| e.message)?;
            Ok(format!(
                "{register} polled {mode}; {} fast, {} slow",
                lists.fast.len(),
                lists.slow.len()
            ))
        })
    }
    fn read(&self, register: u16) -> nibe_mqtt::BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            let reading = Registers::read(self, register)
                .await
                .map_err(|e| e.message)?;
            reading
                .decoded
                .or(reading.raw)
                .map(|v| nibe_mqtt::plain(&v))
                .ok_or_else(|| reading.error.unwrap_or_else(|| "no value".into()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_passes_through_and_scaled_uses_the_catalog() {
        let catalog = Catalog::bundled();
        let raw = |register, value| Registers::raw(&catalog, register, value);
        // Raw is never range-checked (ADR 0125), even far outside the catalog range.
        assert_eq!(raw(47049, WriteValue::Raw(9999)).unwrap(), 9999);
        // 47049 HW stop temperature economy is scale 10: 47.0 °C is raw 470.
        assert_eq!(raw(47049, WriteValue::Scaled(47.0)).unwrap(), 470);
        assert_eq!(raw(47104, WriteValue::Scaled(90.0)).unwrap(), 90);
        assert!(
            raw(1, WriteValue::Scaled(1.0)).is_err(),
            "scaled needs a catalog entry"
        );
        assert_eq!(raw(1, WriteValue::Raw(1)).unwrap(), 1);
        assert!(raw(47049, WriteValue::Scaled(f64::NAN)).is_err());
        assert!(raw(47049, WriteValue::Scaled(1e12)).is_err());
    }
}
