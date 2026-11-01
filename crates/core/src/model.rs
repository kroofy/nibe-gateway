use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub type RegisterId = u32;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceMode {
    Simulated,
    Replay,
    Hardware,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RegisterValue {
    Integer(i64),
    Decimal(f64),
    Boolean(bool),
    Text(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegisterType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    Boolean,
    Text,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegisterAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Unverified,
    CaptureVerified,
    HardwareVerified,
    Synthetic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationState {
    CatalogKnown,
    Observed,
    ConfirmedReadable,
    Unsupported,
    Unknown,
}

/// Metadata alone never grants write permission. `device-f730` supplies the real catalog.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct RegisterDefinition {
    pub id: RegisterId,
    pub name: String,
    pub description: String,
    pub data_type: RegisterType,
    /// decoded = raw * scale
    pub scale: f64,
    pub unit: Option<String>,
    pub access: RegisterAccess,
    pub verification: VerificationState,
    pub models: Vec<String>,
    pub firmware: Option<String>,
    pub source: String,
    pub catalog_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct RegisterReading {
    pub id: RegisterId,
    pub raw: Option<RegisterValue>,
    pub decoded: Option<RegisterValue>,
    pub observed_at_ms: u64,
    pub source: String,
    pub observation: ObservationState,
    pub error: Option<String>,
    /// Freshness limit for this reading (slow-polled registers); `None` uses the gateway's `stale_after_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct Measurement {
    pub id: RegisterId,
    pub name: String,
    pub value: Option<RegisterValue>,
    pub unit: Option<String>,
    pub observed_at_ms: u64,
    pub age_ms: u64,
    pub stale: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct DeviceIdentity {
    pub model: String,
    pub firmware: Option<String>,
    pub serial: Option<String>,
    pub source: String,
    pub verification: VerificationState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
}

/// Domain-only network view; the `network` crate owns real NetworkManager operations and secrets.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NetworkState {
    Unmanaged,
    Connected,
    SetupRequired,
    AuthenticationFailed,
    Recovering,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct DiscoveryStatus {
    pub state: String,
    pub observed_count: usize,
    pub active_probing_supported: bool,
    pub detail: String,
}
impl Default for DiscoveryStatus {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            observed_count: 0,
            active_probing_supported: false,
            detail:
                "Passive observations only; active probing requires a verified protocol adapter"
                    .into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct Diagnostics {
    pub polls: u64,
    pub updates: u64,
    pub transport_errors: u64,
    pub timeouts: u64,
    pub malformed_frames: u64,
    pub commands_accepted: u64,
    pub commands_rejected: u64,
    pub event_receivers_lagged: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceSnapshot {
    pub mode: DeviceMode,
    pub identity: Option<DeviceIdentity>,
    pub connection: ConnectionState,
    pub network: NetworkState,
    pub catalog: Vec<RegisterDefinition>,
    pub registers: Vec<RegisterReading>,
    pub measurements: Vec<Measurement>,
    pub discovery: DiscoveryStatus,
    pub diagnostics: Diagnostics,
    pub now_ms: u64,
    pub last_update_ms: Option<u64>,
    pub stale_after_ms: u64,
    pub stale: bool,
    pub last_error: Option<String>,
    pub controls_enabled: bool,
}
impl DeviceSnapshot {
    pub fn refresh(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
        self.measurements = self
            .registers
            .iter()
            .map(|r| {
                let definition = self.catalog.iter().find(|d| d.id == r.id);
                let age_ms = now_ms.saturating_sub(r.observed_at_ms);
                Measurement {
                    id: r.id,
                    name: definition
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| format!("unknown_{}", r.id)),
                    value: r.decoded.clone(),
                    unit: definition.and_then(|d| d.unit.clone()),
                    observed_at_ms: r.observed_at_ms,
                    age_ms,
                    stale: self.connection != ConnectionState::Connected
                        || age_ms > r.max_age_ms.unwrap_or(self.stale_after_ms)
                        || r.error.is_some(),
                    error: r.error.clone(),
                }
            })
            .collect();
        self.stale = self.connection != ConnectionState::Connected
            || self.measurements.is_empty()
            || self.measurements.iter().any(|m| m.stale);
    }
}

/// NIBE "Temporary Lux" (menu 2.1): extra hot water for a fixed time, or one heating run.
/// On the F730 this is register 48132, raw 0..=4 in this order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemporaryLux {
    Off,
    ThreeHours,
    SixHours,
    TwelveHours,
    OneTimeIncrease,
}
impl TemporaryLux {
    pub fn raw(self) -> i32 {
        self as i32
    }
    pub fn from_raw(raw: i64) -> Option<Self> {
        [
            Self::Off,
            Self::ThreeHours,
            Self::SixHours,
            Self::TwelveHours,
            Self::OneTimeIncrease,
        ]
        .into_iter()
        .find(|m| i64::from(m.raw()) == raw)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    TemporaryLux {
        mode: TemporaryLux,
    },
    HeatingOffset {
        offset: i8,
    },
    /// Acknowledge the pump's current alarm (NIBE alarm reset). Never sent unless enabled for hardware.
    AlarmReset,
}
impl Command {
    /// Synthetic limits only. Hardware verification must define actual supported controls.
    pub fn validate(&self) -> Result<(), ServiceError> {
        match self {
            Self::HeatingOffset { offset } if !(-10..=10).contains(offset) => Err(
                ServiceError::validation("offset must be between -10 and 10"),
            ),
            _ => Ok(()),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CommandReceipt {
    pub sequence: u64,
    pub command: Command,
    pub accepted_at_ms: u64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Validation,
    ControlsDisabled,
    Forbidden,
    Busy,
    Timeout,
    Disconnected,
    Malformed,
    Rejected,
    Unavailable,
    Unsupported,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ServiceError {
    pub code: ErrorCode,
    pub message: String,
}
impl ServiceError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, message)
    }
}
impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for ServiceError {}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GatewayEvent {
    pub sequence: u64,
    pub at_ms: u64,
    pub kind: String,
    pub detail: String,
}

#[cfg(feature = "dev")]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum DevAction {
    Scenario { name: String },
    Advance { milliseconds: u64 },
    Freeze { frozen: bool },
    Fault { kind: String },
    Reset,
}
