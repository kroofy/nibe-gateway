//! MQTT bridge with Home Assistant discovery (ADR 0115). Publishes readings and forwards the named
//! commands the gateway already offers; it has no register-by-number write and no automation (ADR 0113).
pub mod codec;
mod topics;

use codec::Packet;
use nibe_core::{Command, DeviceSnapshot, Gateway, TemporaryLux};
use nibe_persistence::MqttSettings;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, watch},
    time::{timeout, Instant},
};
pub use topics::{device_id, Topics};

const KEEP_ALIVE: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STATUS_EVERY: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const REFRESH_EVERY: Duration = Duration::from_secs(300);

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
}

pub fn start(
    gateway: Gateway,
    identity: Identity,
    controls: Controls,
    settings: Option<MqttSettings>,
) -> Bridge {
    let (tx, rx) = watch::channel(settings);
    let status = Arc::new(Mutex::new(Status {
        device_id: identity.device_id.clone(),
        ..Status::default()
    }));
    tokio::spawn(run(gateway, identity, controls, rx, status.clone()));
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
        let mut announced = false;
        let mut last_status = Instant::now() - STATUS_EVERY;
        let mut ping_sent: Option<Instant> = None;
        let (results_tx, mut results) = mpsc::channel::<String>(8);
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            let snapshot = self.gateway.snapshot();
            if self.settings.discovery && !announced {
                self.announce(&mut conn, &snapshot).await?;
                announced = true;
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
                Some(result) = results.recv() => {
                    conn.publish(&self.topics.result, result.as_bytes(), false).await?;
                }
                packet = conn.read(Duration::from_millis(1000)) => match packet? {
                    Some(Packet::PingResp) => ping_sent = None,
                    Some(Packet::Publish { topic, payload }) => {
                        if topic == self.topics.ha_status {
                            if payload == b"online" {
                                // Home Assistant restarted: it needs discovery and current state again.
                                announced = false;
                                sent.clear();
                            }
                        } else if let Some(name) = topic.strip_prefix(&self.topics.command_prefix) {
                            self.status.lock().unwrap().commands_received += 1;
                            self.command(name, &payload, results_tx.clone());
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
    fn command(&self, name: &str, payload: &[u8], results: mpsc::Sender<String>) {
        let text = String::from_utf8_lossy(payload).trim().to_string();
        let parsed = parse_command(name, &text, self.controls);
        let gateway = self.gateway.clone();
        let name = name.to_string();
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
            let body = match outcome {
                Ok(detail) => serde_json::json!({"command": name, "ok": true, "detail": detail}),
                Err(error) => serde_json::json!({"command": name, "ok": false, "error": error}),
            };
            let _ = results.send(body.to_string()).await;
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
}
