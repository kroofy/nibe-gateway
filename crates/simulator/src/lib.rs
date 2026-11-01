//! Synthetic domain simulator. IDs 1..=5 are deliberately NOT F730 addresses.
//! Recorded serial replay lives in `device-f730` and is the authoritative protocol surface.
use nibe_core::*;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

pub const INITIAL_TIME_MS: u64 = 1_700_000_000_000;

pub struct SimClock {
    state: Mutex<ClockState>,
}
struct ClockState {
    base_ms: u64,
    anchor: Instant,
    frozen: bool,
}
impl Default for SimClock {
    fn default() -> Self {
        Self {
            state: Mutex::new(ClockState {
                base_ms: INITIAL_TIME_MS,
                anchor: Instant::now(),
                frozen: false,
            }),
        }
    }
}
impl Clock for SimClock {
    fn now_ms(&self) -> u64 {
        clock_now(&self.state.lock().expect("clock lock poisoned"))
    }
}
fn clock_now(state: &ClockState) -> u64 {
    state.base_ms.saturating_add(if state.frozen {
        0
    } else {
        state.anchor.elapsed().as_millis() as u64
    })
}
impl SimClock {
    pub fn advance(&self, milliseconds: u64) -> Result<(), ServiceError> {
        // Bound test inputs and preserve timestamp ordering (including freshness math).
        if milliseconds > 86_400_000 {
            return Err(ServiceError::validation("advance must be at most one day"));
        }
        let mut state = self.state.lock().expect("clock lock poisoned");
        state.base_ms = clock_now(&state).saturating_add(milliseconds);
        state.anchor = Instant::now();
        Ok(())
    }
    pub fn freeze(&self, frozen: bool) {
        let mut state = self.state.lock().expect("clock lock poisoned");
        state.base_ms = clock_now(&state);
        state.anchor = Instant::now();
        state.frozen = frozen;
    }
    pub fn reset(&self) {
        let mut state = self.state.lock().expect("clock lock poisoned");
        state.base_ms = INITIAL_TIME_MS;
        state.anchor = Instant::now();
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    None,
    Disconnect,
    Stale,
    Malformed,
    Timeout,
    RejectedWrite,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Scenario {
    pub name: String,
    pub outdoor: f64,
    pub hot_water: f64,
    pub compressor_running: bool,
    pub network: NetworkState,
    pub fault: Fault,
}
pub fn scenarios() -> Vec<Scenario> {
    serde_json::from_str(include_str!("../../../fixtures/scenarios/scenarios.json"))
        .expect("checked-in scenarios must be valid")
}

pub struct SimulatedDevice {
    scenario: Scenario,
    clock: Arc<SimClock>,
    fault: Fault,
    last_readings: Option<Vec<RegisterReading>>,
    heating_offset: i8,
    temporary_lux: TemporaryLux,
    boost_until_ms: Option<u64>,
}
impl SimulatedDevice {
    pub fn new(clock: Arc<SimClock>, name: &str) -> Result<Self, ServiceError> {
        let scenario = find_scenario(name)?;
        Ok(Self {
            fault: scenario.fault,
            scenario,
            clock,
            last_readings: None,
            heating_offset: 0,
            temporary_lux: TemporaryLux::Off,
            boost_until_ms: None,
        })
    }
    pub fn set_scenario(&mut self, name: &str) -> Result<(), ServiceError> {
        let scenario = find_scenario(name)?;
        self.fault = scenario.fault;
        self.scenario = scenario;
        Ok(())
    }
    pub fn inject_fault(&mut self, fault: Fault) {
        self.fault = fault;
    }
    pub fn clock(&self) -> &Arc<SimClock> {
        &self.clock
    }
    fn readings(&self, now_ms: u64) -> Vec<RegisterReading> {
        let hot_water = if self.temporary_lux != TemporaryLux::Off {
            55.0
        } else {
            self.scenario.hot_water
        };
        let values = [
            (
                1,
                RegisterValue::Integer((self.scenario.outdoor * 10.0).round() as i64),
                RegisterValue::Decimal(self.scenario.outdoor),
            ),
            (
                2,
                RegisterValue::Integer((hot_water * 10.0).round() as i64),
                RegisterValue::Decimal(hot_water),
            ),
            (
                3,
                RegisterValue::Boolean(self.scenario.compressor_running),
                RegisterValue::Boolean(self.scenario.compressor_running),
            ),
            (
                4,
                RegisterValue::Integer(self.heating_offset.into()),
                RegisterValue::Integer(self.heating_offset.into()),
            ),
            (
                5,
                RegisterValue::Integer(self.temporary_lux.raw().into()),
                RegisterValue::Text(format!("{:?}", self.temporary_lux)),
            ),
        ];
        values
            .into_iter()
            .map(|(id, raw, decoded)| RegisterReading {
                id,
                raw: Some(raw),
                decoded: Some(decoded),
                observed_at_ms: now_ms,
                source: "synthetic".into(),
                observation: ObservationState::Observed,
                error: None,
                max_age_ms: None,
            })
            .collect()
    }
}
fn find_scenario(name: &str) -> Result<Scenario, ServiceError> {
    scenarios()
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| ServiceError::validation(format!("unknown scenario: {name}")))
}
impl Device for SimulatedDevice {
    fn mode(&self) -> DeviceMode {
        DeviceMode::Simulated
    }
    fn poll(&mut self, now_ms: u64) -> DeviceFuture<'_, DeviceUpdate> {
        Box::pin(async move {
            match self.fault {
                Fault::Disconnect => {
                    return Err(ServiceError::new(
                        ErrorCode::Disconnected,
                        "simulated serial disconnect",
                    ))
                }
                Fault::Malformed => {
                    return Err(ServiceError::new(
                        ErrorCode::Malformed,
                        "simulated malformed frame; cache retained",
                    ))
                }
                Fault::Timeout => return std::future::pending().await,
                _ => {}
            }
            if self
                .boost_until_ms
                .is_some_and(|deadline| now_ms >= deadline)
            {
                self.temporary_lux = TemporaryLux::Off;
                self.boost_until_ms = None;
            }
            let readings = if self.fault == Fault::Stale {
                if self.last_readings.is_none() {
                    let old = self.readings(now_ms.saturating_sub(60_000));
                    self.last_readings = Some(old.clone());
                    Some(old)
                } else {
                    None
                }
            } else {
                let readings = self.readings(now_ms);
                self.last_readings = Some(readings.clone());
                Some(readings)
            };
            Ok(DeviceUpdate {
                identity: DeviceIdentity {
                    model: "F730 (synthetic)".into(),
                    firmware: Some("sim-1".into()),
                    serial: None,
                    source: "synthetic fixture".into(),
                    verification: VerificationState::Synthetic,
                },
                connection: ConnectionState::Connected,
                network: self.scenario.network,
                catalog: catalog(),
                readings,
            })
        })
    }
    fn execute(&mut self, command: Command, now_ms: u64) -> DeviceFuture<'_, String> {
        Box::pin(async move {
            command.validate()?;
            match self.fault {
                Fault::RejectedWrite => {
                    return Err(ServiceError::new(
                        ErrorCode::Rejected,
                        "simulated device rejected command",
                    ))
                }
                Fault::Disconnect => {
                    return Err(ServiceError::new(
                        ErrorCode::Disconnected,
                        "simulated device disconnected",
                    ))
                }
                Fault::Timeout => return std::future::pending().await,
                Fault::Malformed | Fault::Stale => {
                    return Err(ServiceError::new(
                        ErrorCode::Unavailable,
                        "simulated state is not reliable enough for controls",
                    ))
                }
                Fault::None => {}
            }
            match command {
                Command::HeatingOffset { offset } => self.heating_offset = offset,
                Command::AlarmReset => {}
                Command::TemporaryLux { mode } => {
                    self.temporary_lux = mode;
                    // A one-time increase ends after a simulated hour; the pump ends it when hot.
                    let hours = match mode {
                        TemporaryLux::Off => None,
                        TemporaryLux::ThreeHours => Some(3),
                        TemporaryLux::SixHours => Some(6),
                        TemporaryLux::TwelveHours => Some(12),
                        TemporaryLux::OneTimeIncrease => Some(1),
                    };
                    self.boost_until_ms = hours.map(|h: u64| now_ms.saturating_add(h * 3_600_000));
                }
            }
            Ok("applied to synthetic device only".into())
        })
    }
    fn discover(&mut self) -> DeviceFuture<'_, DiscoveryStatus> {
        Box::pin(async move {
            match self.fault {
                Fault::Timeout => return std::future::pending().await,
                Fault::Disconnect | Fault::Malformed => {
                    return Err(ServiceError::new(
                        ErrorCode::Unavailable,
                        "simulated device unavailable",
                    ))
                }
                _ => {}
            }
            Ok(DiscoveryStatus {
                state: "complete".into(),
                observed_count: self.last_readings.as_ref().map_or(0, Vec::len),
                active_probing_supported: false,
                detail: "Synthetic passive observation only; no probing or register writes".into(),
            })
        })
    }
    #[cfg(feature = "dev")]
    fn dev_action(&mut self, action: DevAction) -> Result<(), ServiceError> {
        match action {
            DevAction::Scenario { name } => self.set_scenario(&name)?,
            DevAction::Advance { milliseconds } => self.clock.advance(milliseconds)?,
            DevAction::Freeze { frozen } => self.clock.freeze(frozen),
            DevAction::Fault { kind } => {
                self.fault = serde_json::from_value(serde_json::Value::String(kind)).map_err(|_| ServiceError::validation("unknown fault; use none, disconnect, stale, malformed, timeout, rejected_write"))?;
            }
            DevAction::Reset => {
                self.set_scenario("normal_heating")?;
                self.clock.reset();
                self.last_readings = None;
                self.heating_offset = 0;
                self.temporary_lux = TemporaryLux::Off;
                self.boost_until_ms = None;
            }
        }
        Ok(())
    }
}

pub fn catalog() -> Vec<RegisterDefinition> {
    [
        (1, "outdoor_temperature", RegisterType::I16, 0.1, Some("°C")),
        (
            2,
            "hot_water_temperature",
            RegisterType::I16,
            0.1,
            Some("°C"),
        ),
        (3, "compressor_running", RegisterType::Boolean, 1.0, None),
        (4, "heating_offset", RegisterType::I8, 1.0, None),
        (5, "temporary_lux", RegisterType::U8, 1.0, None),
    ]
    .into_iter()
    .map(|(id, name, data_type, scale, unit)| RegisterDefinition {
        id,
        name: name.into(),
        description: "Synthetic demonstration value; not a physical F730 address".into(),
        data_type,
        scale,
        unit: unit.map(String::from),
        access: if id >= 4 {
            RegisterAccess::ReadWrite
        } else {
            RegisterAccess::ReadOnly
        },
        verification: VerificationState::Synthetic,
        models: vec!["F730 (synthetic)".into()],
        firmware: Some("sim-1".into()),
        source: "fixtures/scenarios/scenarios.json".into(),
        catalog_version: "synthetic-v1".into(),
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_clock_is_repeatable_and_advance_is_bounded() {
        let clock = SimClock::default();
        clock.freeze(true);
        clock.reset();
        assert_eq!(clock.now_ms(), INITIAL_TIME_MS);
        clock.advance(6001).unwrap();
        assert_eq!(clock.now_ms(), INITIAL_TIME_MS + 6001);
        assert!(clock.advance(u64::MAX).is_err());
    }
    #[tokio::test]
    async fn fixtures_produce_distinct_domain_states_and_failures() {
        let clock = Arc::new(SimClock::default());
        clock.freeze(true);
        clock.reset();
        for scenario in scenarios() {
            let mut device = SimulatedDevice::new(clock.clone(), &scenario.name).unwrap();
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(10),
                device.poll(clock.now_ms()),
            )
            .await;
            match scenario.fault {
                Fault::Timeout => assert!(result.is_err()),
                Fault::Disconnect => {
                    assert_eq!(result.unwrap().unwrap_err().code, ErrorCode::Disconnected)
                }
                Fault::Malformed => {
                    assert_eq!(result.unwrap().unwrap_err().code, ErrorCode::Malformed)
                }
                _ => {
                    let update = result.unwrap().unwrap();
                    assert_eq!(update.network, scenario.network);
                    let readings = update.readings.unwrap();
                    assert_eq!(
                        readings[0].decoded,
                        Some(RegisterValue::Decimal(scenario.outdoor))
                    );
                    assert_eq!(
                        readings[2].decoded,
                        Some(RegisterValue::Boolean(scenario.compressor_running))
                    );
                    if scenario.fault == Fault::Stale {
                        assert_eq!(readings[0].observed_at_ms, INITIAL_TIME_MS - 60_000);
                    }
                }
            }
        }
    }
    #[tokio::test]
    async fn rejected_write_does_not_mutate_device() {
        let clock = Arc::new(SimClock::default());
        let mut device = SimulatedDevice::new(clock, "rejected_write").unwrap();
        assert_eq!(
            device
                .execute(Command::HeatingOffset { offset: 4 }, 0)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Rejected
        );
        device.inject_fault(Fault::None);
        assert_eq!(
            device.poll(1).await.unwrap().readings.unwrap()[3].decoded,
            Some(RegisterValue::Integer(0))
        );
    }
    #[tokio::test]
    async fn temporary_lux_expires_at_simulated_deadline() {
        let clock = Arc::new(SimClock::default());
        clock.freeze(true);
        clock.reset();
        let mut device = SimulatedDevice::new(clock.clone(), "normal_heating").unwrap();
        device
            .execute(
                Command::TemporaryLux {
                    mode: TemporaryLux::ThreeHours,
                },
                clock.now_ms(),
            )
            .await
            .unwrap();
        clock.advance(3 * 3_600_000 - 1).unwrap();
        assert_eq!(
            device.poll(clock.now_ms()).await.unwrap().readings.unwrap()[1].decoded,
            Some(RegisterValue::Decimal(55.0))
        );
        clock.advance(1).unwrap();
        assert_eq!(
            device.poll(clock.now_ms()).await.unwrap().readings.unwrap()[1].decoded,
            Some(RegisterValue::Decimal(48.0))
        );
    }
}
