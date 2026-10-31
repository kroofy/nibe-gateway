//! Topic layout and Home Assistant discovery payloads (ADR 0115).
use crate::Controls;
use nibe_core::{DeviceSnapshot, RegisterDefinition, RegisterId, RegisterValue};
use nibe_persistence::MqttSettings;
use serde_json::{json, Value};

/// `nibe_` + the last 8 hex digits of the Pi serial (or the sanitized identity when it has none).
pub fn device_id(identity: &str) -> String {
    let hex: String = identity
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect::<String>()
        .to_lowercase();
    let tail = if hex.len() >= 8 {
        hex[hex.len() - 8..].to_string()
    } else {
        identity
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(16)
            .collect::<String>()
            .to_lowercase()
    };
    format!("nibe_{}", if tail.is_empty() { "gateway" } else { &tail })
}

pub struct Topics {
    base: String,
    discovery_prefix: String,
    device: String,
    pub availability: String,
    pub status: String,
    pub result: String,
    pub command_prefix: String,
    pub command_filter: String,
    /// `<base>/register/`: per-register commands are `<prefix><id>/set|set_raw|get|poll` (ADR 0126).
    pub register_command_prefix: String,
    pub ha_status: String,
}
impl Topics {
    pub fn new(settings: &MqttSettings, device: &str) -> Self {
        let base = format!("{}/{device}", settings.topic_prefix);
        Self {
            availability: format!("{base}/availability"),
            status: format!("{base}/status"),
            result: format!("{base}/result"),
            command_prefix: format!("{base}/set/"),
            command_filter: format!("{base}/set/+"),
            register_command_prefix: format!("{base}/register/"),
            ha_status: format!("{}/status", settings.discovery_prefix),
            discovery_prefix: settings.discovery_prefix.clone(),
            device: device.into(),
            base,
        }
    }
    /// Exactly the command suffixes, so the bridge never receives its own `register/<id>` values.
    pub fn register_command_filters(&self) -> Vec<String> {
        ["set", "set_raw", "get", "poll"]
            .iter()
            .map(|action| format!("{}+/{action}", self.register_command_prefix))
            .collect()
    }
    pub fn register(&self, id: RegisterId) -> String {
        format!("{}/register/{id}", self.base)
    }
    fn config(&self, component: &str, object: &str) -> String {
        format!(
            "{}/{component}/{}/{object}/config",
            self.discovery_prefix, self.device
        )
    }

    /// Retained discovery configs: one sensor per polled register, an alarm problem sensor, and the
    /// enabled controls when commands are allowed.
    pub fn discovery(
        &self,
        snapshot: &DeviceSnapshot,
        gateway_version: &str,
        allow_commands: bool,
        controls: Controls,
    ) -> Vec<(String, Value)> {
        let identity = snapshot.identity.as_ref();
        let model = identity.map_or("NIBE heat pump", |i| i.model.as_str());
        let device = json!({
            "identifiers": [self.device],
            "name": format!("NIBE {model}"),
            "manufacturer": "NIBE",
            "model": model,
            "sw_version": identity.and_then(|i| i.firmware.clone()),
        });
        let origin = json!({"name": "nibe-gateway", "sw_version": gateway_version});
        let common = |name: &str, unique: &str| {
            json!({
                "name": name,
                "unique_id": format!("{}_{unique}", self.device),
                "availability_topic": self.availability,
                "device": device,
                "origin": origin,
            })
        };
        let mut out = Vec::new();
        for reading in &snapshot.registers {
            let Some(definition) = snapshot.catalog.iter().find(|d| d.id == reading.id) else {
                continue;
            };
            let mut config = common(&definition.description, &format!("r{}", reading.id));
            merge(&mut config, sensor_classes(definition));
            merge(
                &mut config,
                json!({"state_topic": self.register(reading.id)}),
            );
            out.push((self.config("sensor", &format!("r{}", reading.id)), config));
        }
        if snapshot.registers.iter().any(|r| r.id == ALARM) {
            let mut config = common("Alarm active", "alarm");
            merge(
                &mut config,
                json!({
                    "state_topic": self.register(ALARM),
                    "value_template": "{{ 'ON' if value | int(0) != 0 else 'OFF' }}",
                    "device_class": "problem",
                }),
            );
            out.push((self.config("binary_sensor", "alarm"), config));
        }
        let command = |name: &str| format!("{}{name}", self.command_prefix);
        let controls = if allow_commands {
            controls
        } else {
            Controls::default()
        };
        // Configs for disabled controls are cleared (empty retained payload) so stale entities disappear.
        let mut control = |component: &str, object: &str, enabled: bool, config: Value| {
            out.push((
                self.config(component, object),
                if enabled { config } else { Value::Null },
            ));
        };
        let mut offset = common("Heat offset", "heating_offset");
        merge(
            &mut offset,
            json!({
                "command_topic": command("heating_offset"),
                "state_topic": self.register(HEAT_OFFSET),
                "min": -10, "max": 10, "step": 1, "mode": "box",
            }),
        );
        control("number", "heating_offset", controls.heating_offset, offset);
        let mut lux = common("Temporary Lux", "temporary_lux");
        merge(
            &mut lux,
            json!({
                "command_topic": command("temporary_lux"),
                "state_topic": self.register(TEMPORARY_LUX),
                "value_template": "{{ ['off','three_hours','six_hours','twelve_hours','one_time_increase'][value | int(0)] | default('off') }}",
                "options": ["off", "three_hours", "six_hours", "twelve_hours", "one_time_increase"],
            }),
        );
        control("select", "temporary_lux", controls.temporary_lux, lux);
        let mut reset = common("Reset alarm", "alarm_reset");
        merge(
            &mut reset,
            json!({"command_topic": command("alarm_reset"), "payload_press": "PRESS"}),
        );
        control("button", "alarm_reset", controls.alarm_reset, reset);
        out
    }
}

const ALARM: RegisterId = 45001;
const HEAT_OFFSET: RegisterId = 47011;
const TEMPORARY_LUX: RegisterId = 48132;

fn merge(target: &mut Value, extra: Value) {
    if let (Value::Object(t), Value::Object(e)) = (target, extra) {
        t.extend(e);
    }
}

/// Home Assistant device/state class from the unit, so values land in the right cards and the Energy
/// dashboard. Unknown units stay plain sensors.
fn sensor_classes(definition: &RegisterDefinition) -> Value {
    let unit = definition.unit.as_deref().unwrap_or("");
    let (device_class, state_class) = match unit {
        "°C" => (Some("temperature"), Some("measurement")),
        "kWh" | "Wh" => (Some("energy"), Some("total_increasing")),
        "W" | "kW" => (Some("power"), Some("measurement")),
        "A" => (Some("current"), Some("measurement")),
        "V" => (Some("voltage"), Some("measurement")),
        "Hz" => (Some("frequency"), Some("measurement")),
        "%" | "l/m" | "l/min" | "m3/h" | "bar" => (None, Some("measurement")),
        _ => (None, None),
    };
    let mut out = json!({});
    if !unit.is_empty() {
        merge(&mut out, json!({"unit_of_measurement": unit}));
    }
    if let Some(class) = device_class {
        merge(&mut out, json!({"device_class": class}));
    }
    if let Some(class) = state_class {
        merge(&mut out, json!({"state_class": class}));
    }
    out
}

/// Plain-text state payload: what Home Assistant and most dashboards parse without templates.
pub fn plain(value: &RegisterValue) -> String {
    match value {
        RegisterValue::Integer(v) => v.to_string(),
        RegisterValue::Decimal(v) => {
            let text = format!("{v:.2}");
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        RegisterValue::Boolean(v) => if *v { "ON" } else { "OFF" }.into(),
        RegisterValue::Text(v) => v.clone(),
    }
}

pub fn status_json(snapshot: &DeviceSnapshot) -> String {
    let alarm = snapshot
        .registers
        .iter()
        .find(|r| r.id == ALARM)
        .and_then(|r| r.decoded.as_ref())
        .map(plain);
    json!({
        "connection": snapshot.connection,
        "stale": snapshot.stale,
        "alarm": alarm,
        "last_update_ms": snapshot.last_update_ms,
        "last_error": snapshot.last_error,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_id_uses_the_serial_tail() {
        assert_eq!(device_id("00000000a1b2c3d4"), "nibe_a1b2c3d4");
        assert_eq!(device_id("simulated"), "nibe_simulated");
        assert_eq!(device_id(""), "nibe_gateway");
    }
    #[test]
    fn plain_values_are_bare_numbers() {
        assert_eq!(plain(&RegisterValue::Decimal(20.7)), "20.7");
        assert_eq!(plain(&RegisterValue::Decimal(46.0)), "46");
        assert_eq!(plain(&RegisterValue::Decimal(-0.5)), "-0.5");
        assert_eq!(plain(&RegisterValue::Integer(251)), "251");
        assert_eq!(plain(&RegisterValue::Boolean(true)), "ON");
    }
    #[test]
    fn topics_follow_prefix_and_device() {
        let t = Topics::new(&MqttSettings::default(), "nibe_ab");
        assert_eq!(t.availability, "nibe/nibe_ab/availability");
        assert_eq!(t.register(40004), "nibe/nibe_ab/register/40004");
        assert_eq!(t.command_filter, "nibe/nibe_ab/set/+");
        assert_eq!(
            t.register_command_filters(),
            [
                "nibe/nibe_ab/register/+/set",
                "nibe/nibe_ab/register/+/set_raw",
                "nibe/nibe_ab/register/+/get",
                "nibe/nibe_ab/register/+/poll"
            ]
        );
        assert_eq!(t.ha_status, "homeassistant/status");
        assert_eq!(
            t.config("sensor", "r40004"),
            "homeassistant/sensor/nibe_ab/r40004/config"
        );
    }
}
