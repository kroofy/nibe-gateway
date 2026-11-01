//! MQTT bridge with Home Assistant discovery. Publishes readings, forwards the named commands,
//! and reads or writes any register by number through the same operations as the HTTP API.
//! No automation.
pub mod codec;
mod topics;

use codec::Packet;
use nibe_core::{Command, DeviceSnapshot, Gateway, TemporaryLux};
use nibe_persistence::{
    audit::{AuditLog, AuditSource, NewAuditEntry},
    MqttSettings,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, watch},
    time::{timeout, Instant},
};
pub use topics::{device_id, plain, Topics};

const KEEP_ALIVE: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STATUS_EVERY: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const REFRESH_EVERY: Duration = Duration::from_secs(300);

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Register operations by number, implemented by the API crate over the F730 adapter so MQTT
/// and HTTP share one code path. Ok carries a human-readable "queued" detail.
pub trait RegisterAccess: Send + Sync {
    /// `raw` false: `value` is scaled by the catalog; true: `value` is the raw integer. A lease applies
    /// only to leasable registers; without one a write ends any lease.
    fn write(
        &self,
        register: u16,
        value: f64,
        raw: bool,
        lease_seconds: Option<u32>,
    ) -> BoxFuture<'_, Result<String, String>>;
    /// Reads now and returns the value as plain text, as published on `register/<id>`.
    fn read(&self, register: u16) -> BoxFuture<'_, Result<String, String>>;
    /// Poll `fast`, `slow` or `off`, saved across restarts.
    fn poll(&self, register: u16, mode: &str) -> BoxFuture<'_, Result<String, String>>;
}

/// A per-register command: `<base>/register/<id>/<action>`.
#[derive(Debug, PartialEq)]
pub enum RegisterCommand {
    /// Value and optional lease in seconds.
    Set(f64, Option<u32>),
    SetRaw(f64, Option<u32>),
    Get,
    /// `fast`, `slow` or `off`.
    Poll(String),
}
/// A `set`/`set_raw` payload: a plain number (a decimal comma is accepted), or
/// `{"value": 45.5, "lease_seconds": 600}` for a leased write.
fn parse_set(action: &str, payload: &str) -> Result<(f64, Option<u32>), String> {
    let payload = payload.trim();
    if payload.starts_with('{') {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Leased {
            value: f64,
            #[serde(default)]
            lease_seconds: Option<u32>,
        }
        let leased: Leased = serde_json::from_str(payload).map_err(|e| {
            format!("{action} needs a number or {{\"value\": n, \"lease_seconds\": s}}: {e}")
        })?;
        return Ok((leased.value, leased.lease_seconds));
    }
    payload
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| (v, None))
        .ok_or_else(|| format!("{action} needs a number, got {payload:?}"))
}
/// Parses the part after `<base>/register/`.
pub fn parse_register_command(rest: &str, payload: &str) -> Result<(u16, RegisterCommand), String> {
    let (id, action) = rest
        .split_once('/')
        .ok_or_else(|| format!("unknown register topic {rest}"))?;
    let id = id
        .parse::<u16>()
        .map_err(|_| format!("{id} is not a register number"))?;
    let command = match action {
        "set" => {
            let (value, lease) = parse_set(action, payload)?;
            RegisterCommand::Set(value, lease)
        }
        "set_raw" => {
            let (value, lease) = parse_set(action, payload)?;
            RegisterCommand::SetRaw(value, lease)
        }
        "get" => RegisterCommand::Get,
        "poll" => match payload.trim() {
            mode @ ("fast" | "slow" | "off") => RegisterCommand::Poll(mode.into()),
            other => return Err(format!("poll needs fast, slow or off, got {other:?}")),
        },
        _ => return Err(format!("unknown register action {action}")),
    };
    Ok((id, command))
}

/// Which named controls the gateway configuration enables; commands and discovery follow it.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Controls {
    pub heating_offset: bool,
    pub temporary_lux: bool,
    pub alarm_reset: bool,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Disabled,
    Connecting,
    Connected,
    Error,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    pub state: State,
    pub broker: Option<String>,
    pub device_id: String,
    pub last_error: Option<String>,
    pub connected_since_ms: Option<u64>,
    pub messages_published: u64,
    pub commands_received: u64,
}

#[derive(Clone)]
pub struct Bridge {
    status: Arc<Mutex<Status>>,
    settings: watch::Sender<Option<MqttSettings>>,
    controls: Controls,
}
impl Bridge {
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
    pub fn controls(&self) -> Controls {
        self.controls
    }
    /// Apply new settings; the bridge reconnects (or stops) on its own.
    pub fn apply(&self, settings: Option<MqttSettings>) {
        self.settings.send_replace(settings);
    }
}

pub struct Identity {
    pub device_id: String,
    pub gateway_version: String,
    /// Where commands that change something are recorded; `None` records nothing.
    pub audit: Option<Arc<AuditLog>>,
}

/// Records a command that changes something; reads (`get`) are not recorded.
fn audit(log: Option<&AuditLog>, command: &str, payload: &str, outcome: &Result<String, String>) {
    let Some(log) = log else { return };
    let target = command
        .strip_prefix("register/")
        .and_then(|rest| rest.split('/').next())
        .map(str::to_string);
    log.record(NewAuditEntry {
        source: AuditSource::Mqtt,
        actor: "mqtt".into(),
        action: format!("MQTT {command}"),
        target,
        detail: (!payload.is_empty()).then(|| payload.to_string()),
        ok: outcome.is_ok(),
        outcome: Some(match outcome {
            Ok(detail) => detail.clone(),
            Err(error) => error.clone(),
        }),
    });
}

pub fn start(
    gateway: Gateway,
    identity: Identity,
    controls: Controls,
    settings: Option<MqttSettings>,
    registers: Option<Arc<dyn RegisterAccess>>,
) -> Bridge {
    let (tx, rx) = watch::channel(settings);
    let status = Arc::new(Mutex::new(Status {
        device_id: identity.device_id.clone(),
        ..Status::default()
    }));
    tokio::spawn(run(
        gateway,
        identity,
        controls,
        registers,
        rx,
        status.clone(),
    ));
    Bridge {
        status,
        settings: tx,
        controls,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

async fn run(
    gateway: Gateway,
    identity: Identity,
    controls: Controls,
    registers: Option<Arc<dyn RegisterAccess>>,
    mut settings: watch::Receiver<Option<MqttSettings>>,
    status: Arc<Mutex<Status>>,
) {
    let mut shutdown = gateway.shutdown_receiver();
    let mut backoff = Duration::from_secs(1);
    loop {
        let current = settings.borrow_and_update().clone();
        let Some(current) = current.filter(|s| s.enabled) else {
            *status.lock().unwrap() = Status {
                device_id: identity.device_id.clone(),
                ..Status::default()
            };
            tokio::select! {
                changed = settings.changed() => if changed.is_err() { return },
                _ = shutdown.changed() => return,
            }
            continue;
        };
        {
            let mut s = status.lock().unwrap();
            s.state = State::Connecting;
            s.broker = Some(format!("{}:{}", current.host, current.port));
            s.connected_since_ms = None;
        }
        let session = Session {
            gateway: &gateway,
            topics: Topics::new(&current, &identity.device_id),
            settings: &current,
            identity: &identity,
            controls,
            registers: registers.clone(),
            status: &status,
        };
        let outcome = tokio::select! {
            outcome = session.run() => outcome,
            changed = settings.changed() => {
                if changed.is_err() { return }
                Ok(End::Reconfigured)
            }
            _ = shutdown.changed() => return,
        };
        match outcome {
            Ok(End::Reconfigured) => backoff = Duration::from_secs(1),
            Err(error) => {
                tracing::warn!(%error, "mqtt connection ended");
                let connected = {
                    let mut s = status.lock().unwrap();
                    let was = s.state == State::Connected;
                    s.state = State::Error;
                    s.last_error = Some(error);
                    s.connected_since_ms = None;
                    was
                };
                if connected {
                    backoff = Duration::from_secs(1);
                }
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    changed = settings.changed() => if changed.is_err() { return },
                    _ = shutdown.changed() => return,
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

enum End {
    Reconfigured,
}

struct Session<'a> {
    gateway: &'a Gateway,
    topics: Topics,
    settings: &'a MqttSettings,
    identity: &'a Identity,
    controls: Controls,
    registers: Option<Arc<dyn RegisterAccess>>,
    status: &'a Arc<Mutex<Status>>,
}

struct Conn {
    stream: TcpStream,
    buf: Vec<u8>,
    last_sent: Instant,
    published: u64,
}
impl Conn {
    async fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.stream
            .write_all(bytes)
            .await
            .map_err(|e| format!("write failed: {e}"))?;
        self.last_sent = Instant::now();
        Ok(())
    }
    async fn publish(&mut self, topic: &str, payload: &[u8], retain: bool) -> Result<(), String> {
        self.published += 1;
        self.send(&codec::publish(topic, payload, retain)).await
    }
    /// Next packet; `None` when the read timed out without a full packet.
    async fn read(&mut self, wait: Duration) -> Result<Option<Packet>, String> {
        loop {
            if let Some((packet, used)) = codec::decode(&self.buf)? {
                self.buf.drain(..used);
                return Ok(Some(packet));
            }
            let mut chunk = [0u8; 2048];
            match timeout(wait, self.stream.read(&mut chunk)).await {
                Err(_) => return Ok(None),
                Ok(Ok(0)) => return Err("broker closed the connection".into()),
                Ok(Ok(n)) => self.buf.extend_from_slice(&chunk[..n]),
                Ok(Err(e)) => return Err(format!("read failed: {e}")),
            }
        }
    }
}

impl Session<'_> {
    async fn run(self) -> Result<End, String> {
        let address = format!("{}:{}", self.settings.host, self.settings.port);
        let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(&address))
            .await
            .map_err(|_| format!("connecting to {address} timed out"))?
            .map_err(|e| format!("connecting to {address}: {e}"))?;
        stream.set_nodelay(true).ok();
        let mut conn = Conn {
            stream,
            buf: Vec::new(),
            last_sent: Instant::now(),
            published: 0,
        };
        conn.send(&codec::connect(&codec::Connect {
            client_id: &self.identity.device_id,
            username: self.settings.username.as_deref().filter(|u| !u.is_empty()),
            password: self.settings.password.as_deref(),
            keep_alive_s: KEEP_ALIVE.as_secs() as u16,
            will: Some((&self.topics.availability, b"offline", true)),
        }))
        .await?;
        match conn.read(CONNECT_TIMEOUT).await? {
            Some(Packet::ConnAck(0)) => {}
            Some(Packet::ConnAck(code)) => return Err(connack_error(code)),
            _ => return Err("broker did not answer CONNECT".into()),
        }
        let mut filters = Vec::new();
        if self.settings.allow_commands {
            filters.push(self.topics.command_filter.clone());
            if self.registers.is_some() {
                filters.extend(self.topics.register_command_filters());
            }
        }
        if self.settings.discovery {
            filters.push(self.topics.ha_status.clone());
        }
        if !filters.is_empty() {
            let refs: Vec<&str> = filters.iter().map(String::as_str).collect();
            conn.send(&codec::subscribe(1, &refs)).await?;
        }
        conn.publish(&self.topics.availability, b"online", true)
            .await?;
        {
            let mut s = self.status.lock().unwrap();
            s.state = State::Connected;
            s.last_error = None;
            s.connected_since_ms = Some(now_ms());
        }
        tracing::info!(broker = %address, device = %self.identity.device_id, "mqtt connected");
        let mut sent: HashMap<nibe_core::RegisterId, (String, Instant)> = HashMap::new();
        // Registers covered by the last discovery; `None` until announced. Registers whose first reading
        // arrives later (slow polls) trigger a new announcement, so they appear without restarting Home Assistant.
        let mut announced: Option<std::collections::BTreeSet<nibe_core::RegisterId>> = None;
        let mut last_status = Instant::now() - STATUS_EVERY;
        let mut ping_sent: Option<Instant> = None;
        // (topic, payload): `None` is the result topic; `Some` is a value read on request (`register/<id>/get`).
        let (results_tx, mut results) = mpsc::channel::<(Option<String>, String)>(8);
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            let snapshot = self.gateway.snapshot();
            if self.settings.discovery {
                let current: std::collections::BTreeSet<_> =
                    snapshot.registers.iter().map(|r| r.id).collect();
                if announced
                    .as_ref()
                    .is_none_or(|done| !current.is_subset(done))
                {
                    self.announce(&mut conn, &snapshot).await?;
                    announced = Some(current);
                }
            }
            for reading in &snapshot.registers {
                let Some(value) = &reading.decoded else {
                    continue;
                };
                // Publish changes at once and unchanged values every REFRESH_EVERY, so consumers
                // can tell a steady value from a dead feed without a message per poll.
                let text = topics::plain(value);
                if sent
                    .get(&reading.id)
                    .is_some_and(|(last, at)| *last == text && at.elapsed() < REFRESH_EVERY)
                {
                    continue;
                }
                conn.publish(&self.topics.register(reading.id), text.as_bytes(), true)
                    .await?;
                sent.insert(reading.id, (text, Instant::now()));
            }
            if last_status.elapsed() >= STATUS_EVERY {
                last_status = Instant::now();
                conn.publish(
                    &self.topics.status,
                    topics::status_json(&snapshot).as_bytes(),
                    true,
                )
                .await?;
            }
            if let Some(at) = ping_sent {
                if at.elapsed() > KEEP_ALIVE / 2 {
                    return Err("broker stopped answering pings".into());
                }
            } else if conn.last_sent.elapsed() >= KEEP_ALIVE / 2 {
                conn.send(&codec::ping()).await?;
                ping_sent = Some(Instant::now());
            }
            self.status.lock().unwrap().messages_published = conn.published;
            tokio::select! {
                _ = tick.tick() => {}
                Some((topic, payload)) = results.recv() => {
                    let topic = topic.as_deref().unwrap_or(&self.topics.result);
                    conn.publish(topic, payload.as_bytes(), false).await?;
                }
                packet = conn.read(Duration::from_millis(1000)) => match packet? {
                    Some(Packet::PingResp) => ping_sent = None,
                    Some(Packet::Publish { topic, payload }) => {
                        if topic == self.topics.ha_status {
                            if payload == b"online" {
                                // Home Assistant restarted: it needs discovery and current state again.
                                announced = None;
                                sent.clear();
                            }
                        } else if let Some(name) = topic.strip_prefix(&self.topics.command_prefix) {
                            self.status.lock().unwrap().commands_received += 1;
                            self.command(name, &payload, results_tx.clone());
                        } else if let Some(rest) = topic.strip_prefix(&self.topics.register_command_prefix) {
                            self.status.lock().unwrap().commands_received += 1;
                            self.register_command(rest, &payload, results_tx.clone());
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    async fn announce(&self, conn: &mut Conn, snapshot: &DeviceSnapshot) -> Result<(), String> {
        for (topic, payload) in self.topics.discovery(
            snapshot,
            &self.identity.gateway_version,
            self.settings.allow_commands,
            self.controls,
        ) {
            // An empty retained payload removes the entity (disabled control).
            let body = if payload.is_null() {
                String::new()
            } else {
                payload.to_string()
            };
            conn.publish(&topic, body.as_bytes(), true).await?;
        }
        Ok(())
    }

    /// Runs the named command off the connection loop; the outcome goes to the result topic.
    fn command(&self, name: &str, payload: &[u8], results: mpsc::Sender<(Option<String>, String)>) {
        let text = String::from_utf8_lossy(payload).trim().to_string();
        let parsed = parse_command(name, &text, self.controls);
        let gateway = self.gateway.clone();
        let name = name.to_string();
        let log = self.identity.audit.clone();
        tokio::spawn(async move {
            let outcome = match parsed {
                Ok(command) => gateway
                    .command(command)
                    .await
                    .map(|receipt| receipt.detail)
                    .map_err(|e| e.message),
                Err(message) => Err(message),
            };
            tracing::info!(command = %name, ok = outcome.is_ok(), "mqtt command");
            audit(log.as_deref(), &name, &text, &outcome);
            let body = match outcome {
                Ok(detail) => serde_json::json!({"command": name, "ok": true, "detail": detail}),
                Err(error) => serde_json::json!({"command": name, "ok": false, "error": error}),
            };
            let _ = results.send((None, body.to_string())).await;
        });
    }
}

impl Session<'_> {
    /// Runs a per-register command off the connection loop; the outcome goes to the result topic.
    fn register_command(
        &self,
        rest: &str,
        payload: &[u8],
        results: mpsc::Sender<(Option<String>, String)>,
    ) {
        let text = String::from_utf8_lossy(payload).trim().to_string();
        let parsed = if self.settings.allow_commands {
            parse_register_command(rest, &text)
        } else {
            Err("commands are disabled in the MQTT settings".into())
        };
        let registers = self.registers.clone();
        let topic = rest.to_string();
        let log = self.identity.audit.clone();
        let is_read = rest.ends_with("/get");
        let value_prefix = self.topics.register_command_prefix.clone();
        tokio::spawn(async move {
            let outcome = match (parsed, registers) {
                (Err(message), _) => Err(message),
                (Ok(_), None) => Err("register commands need --device hardware".into()),
                (Ok((id, command)), Some(registers)) => match command {
                    RegisterCommand::Set(value, lease) => {
                        registers.write(id, value, false, lease).await
                    }
                    RegisterCommand::SetRaw(value, lease) => {
                        registers.write(id, value, true, lease).await
                    }
                    RegisterCommand::Poll(mode) => registers.poll(id, &mode).await,
                    RegisterCommand::Get => {
                        let value = registers.read(id).await;
                        if let Ok(value) = &value {
                            // Same topic and text as a polled value, not retained (a one-off read).
                            let _ = results
                                .send((Some(format!("{value_prefix}{id}")), value.clone()))
                                .await;
                        }
                        value
                    }
                },
            };
            tracing::info!(topic = %topic, ok = outcome.is_ok(), "mqtt register command");
            let command = format!("register/{topic}");
            if !is_read {
                audit(log.as_deref(), &command, &text, &outcome);
            }
            let body = match outcome {
                Ok(detail) => serde_json::json!({"command": command, "ok": true, "detail": detail}),
                Err(error) => serde_json::json!({"command": command, "ok": false, "error": error}),
            };
            let _ = results.send((None, body.to_string())).await;
        });
    }
}

fn connack_error(code: u8) -> String {
    match code {
        1 => "broker refused: unacceptable protocol version".into(),
        2 => "broker refused the client id".into(),
        3 => "broker unavailable".into(),
        4 => "broker refused: bad username or password".into(),
        5 => "broker refused: not authorized".into(),
        other => format!("broker refused the connection (code {other})"),
    }
}

/// Maps a command topic suffix and payload to a named command. Only enabled controls are accepted.
pub fn parse_command(name: &str, payload: &str, controls: Controls) -> Result<Command, String> {
    match name {
        "heating_offset" if controls.heating_offset => payload
            .parse::<i8>()
            .map(|offset| Command::HeatingOffset { offset })
            .map_err(|_| "heating_offset needs an integer -10..10".into()),
        "temporary_lux" if controls.temporary_lux => serde_json::from_value::<TemporaryLux>(
            serde_json::Value::String(payload.into()),
        )
        .map(|mode| Command::TemporaryLux { mode })
        .map_err(|_| {
            "temporary_lux needs off, three_hours, six_hours, twelve_hours or one_time_increase"
                .into()
        }),
        "alarm_reset" if controls.alarm_reset => Ok(Command::AlarmReset),
        "heating_offset" | "temporary_lux" | "alarm_reset" => Err(format!(
            "{name} is not enabled in the gateway configuration"
        )),
        _ => Err(format!("unknown command {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_follow_enabled_controls_and_never_take_register_numbers() {
        let all = Controls {
            heating_offset: true,
            temporary_lux: true,
            alarm_reset: true,
        };
        assert_eq!(
            parse_command("heating_offset", "-3", all),
            Ok(Command::HeatingOffset { offset: -3 })
        );
        assert_eq!(
            parse_command("temporary_lux", "six_hours", all),
            Ok(Command::TemporaryLux {
                mode: TemporaryLux::SixHours
            })
        );
        assert_eq!(
            parse_command("alarm_reset", "PRESS", all),
            Ok(Command::AlarmReset)
        );
        assert!(parse_command("heating_offset", "x", all).is_err());
        assert!(parse_command("temporary_lux", "2", all).is_err());
        assert!(parse_command("47011", "1", all).is_err());
        assert!(parse_command("heating_offset", "1", Controls::default())
            .unwrap_err()
            .contains("not enabled"));
    }
    #[test]
    fn register_commands_take_any_number_and_a_decimal_comma() {
        assert_eq!(
            parse_register_command("47049/set", "47,5"),
            Ok((47049, RegisterCommand::Set(47.5, None)))
        );
        assert_eq!(
            parse_register_command("1/set_raw", "470"),
            Ok((1, RegisterCommand::SetRaw(470.0, None)))
        );
        assert_eq!(
            parse_register_command("47049/set", r#" {"value": 45.5, "lease_seconds": 600} "#),
            Ok((47049, RegisterCommand::Set(45.5, Some(600))))
        );
        assert_eq!(
            parse_register_command("47049/set_raw", r#"{"value": 455}"#),
            Ok((47049, RegisterCommand::SetRaw(455.0, None)))
        );
        assert_eq!(
            parse_register_command("40004/get", ""),
            Ok((40004, RegisterCommand::Get))
        );
        assert!(parse_register_command("47049/set", "warm").is_err());
        assert!(parse_register_command("47049/set", "inf").is_err());
        assert!(parse_register_command("47049/set", r#"{"value": 45, "lease": 600}"#).is_err());
        assert!(parse_register_command("47049/set", r#"{"lease_seconds": 600}"#).is_err());
        assert!(parse_register_command("x/get", "").is_err());
        assert_eq!(
            parse_register_command("47049/poll", " slow\n"),
            Ok((47049, RegisterCommand::Poll("slow".into())))
        );
        assert!(parse_register_command("47049/poll", "sometimes").is_err());
        assert!(parse_register_command("40004/remove", "").is_err());
        assert!(parse_register_command("40004", "").is_err());
    }
}
