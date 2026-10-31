use serde::Deserialize;
use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub device: String,
    pub serial: String,
    pub wire: nibe_device_f730::WirePolicy,
    pub capture: Option<PathBuf>,
    pub record_capture: Option<PathBuf>,
    pub catalog: Option<PathBuf>,
    pub word_swap: String,
    pub bind: SocketAddr,
    pub scenario: String,
    pub freeze_time: bool,
    pub stale_after_ms: u64,
    pub allow_simulated_controls: bool,
    /// Opt-in for authenticated, bounded read requests on the live bus (ADR 0101). Never enables writes.
    pub allow_hardware_reads: bool,
    /// Registers read continuously on hardware/replay (ADR 0101); needs `allow_hardware_reads` on hardware.
    pub poll_registers: Vec<u16>,
    /// Registers read about every `slow_poll_interval_s` instead of every round (counters, settings).
    pub slow_poll_registers: Vec<u16>,
    pub slow_poll_interval_s: u64,
    /// Owner opt-in for named controls on hardware (ADR 0125). Needs `writable_registers`.
    pub allow_hardware_writes: bool,
    /// Explicit registers the named controls may write, e.g. [47011].
    pub writable_registers: Vec<u16>,
    /// Every register writable through the API with any encodable value (ADR 0125). On by default; takes
    /// effect only with `--device hardware` and `--allow-hardware-reads` (the read-back). `--no-open-writes`
    /// turns it off and leaves only the gated named controls.
    pub open_writes: bool,
    /// Enables the alarm-reset control on hardware (ADR 0105).
    pub allow_alarm_reset: bool,
    /// NibePi parity: reset alarm 251 automatically, at most every 10 minutes. Implies allow_alarm_reset.
    pub alarm_auto_reset: bool,
    pub admin_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub network: String,
    pub network_interface: String,
    pub secure_cookie: bool,
    pub recover_network: bool,
    /// GitHub `owner/name` checked by the Updates page (ADR 0124); `null` / `--update-repository none` disables it.
    pub update_repository: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            device: "simulated".into(),
            serial: "/dev/ttyAMA0".into(),
            wire: nibe_device_f730::WirePolicy::default(),
            capture: None,
            record_capture: None,
            catalog: None,
            word_swap: "auto".into(),
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
            scenario: "normal_heating".into(),
            freeze_time: false,
            stale_after_ms: 5000,
            allow_simulated_controls: false,
            allow_hardware_reads: false,
            poll_registers: vec![],
            slow_poll_registers: vec![],
            slow_poll_interval_s: 600,
            allow_hardware_writes: false,
            writable_registers: vec![],
            open_writes: true,
            allow_alarm_reset: false,
            alarm_auto_reset: false,
            admin_dir: Some("apps/admin/dist".into()),
            data_dir: None,
            network: "auto".into(),
            network_interface: "wlan0".into(),
            secure_cookie: false,
            recover_network: false,
            update_repository: Some(nibe_api::releases::DEFAULT_REPOSITORY.into()),
        }
    }
}
fn id_list(value: Option<String>, flag: &str) -> Result<Vec<u16>, Box<dyn Error>> {
    Ok(value
        .ok_or(format!("{flag} requires comma-separated register IDs"))?
        .split(',')
        .filter(|id| !id.trim().is_empty())
        .map(|id| id.trim().parse::<u16>())
        .collect::<Result<_, _>>()?)
}
impl Config {
    pub fn load() -> Result<Option<Self>, Box<dyn Error>> {
        Self::parse(std::env::args().skip(1).collect())
    }
    fn parse(args: Vec<String>) -> Result<Option<Self>, Box<dyn Error>> {
        if args.iter().any(|arg| arg == "--help" || arg == "-h") {
            println!("nibe-gateway [--config FILE.json] [--device simulated|replay|hardware] [--bind 127.0.0.1:8080]\n  [--scenario NAME] [--freeze-time] [--allow-simulated-controls] [--allow-hardware-reads] [--poll-registers ID,ID,...]
  [--allow-hardware-writes --writable-registers ID,ID,...] [--no-open-writes] [--allow-alarm-reset] [--alarm-auto-reset]
  [--admin-dir PATH]\n[--data-dir PATH] [--network auto|simulated|network-manager] [--network-interface wlan0]\n  [--secure-cookie] [--recover-network] [--update-repository OWNER/NAME|none]\n[--serial /dev/ttyAMA0] [--capture FILE.jsonl] [--record-capture NEW.jsonl]\n  [--catalog FILE.json] [--word-swap auto|0|1]\nHardware serial requires --features serial. All API data requires admin authentication.\nSimulation controls require a --features dev build. CLI options override JSON config.");
            return Ok(None);
        }
        let mut config_path = None;
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if arg == "--config" {
                config_path = Some(
                    args.get(index + 1)
                        .ok_or("--config requires a file")?
                        .clone(),
                );
                index += 2;
            } else if matches!(
                arg.as_str(),
                "--device"
                    | "--bind"
                    | "--scenario"
                    | "--admin-dir"
                    | "--data-dir"
                    | "--network"
                    | "--network-interface"
                    | "--serial"
                    | "--capture"
                    | "--record-capture"
                    | "--catalog"
                    | "--word-swap"
                    | "--poll-registers"
                    | "--writable-registers"
                    | "--update-repository"
            ) {
                index += 2;
            } else {
                index += 1;
            }
        }
        let mut config: Config = if let Some(path) = config_path {
            serde_json::from_slice(&std::fs::read(path)?)?
        } else {
            Self::default()
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    args.next().ok_or("--config requires a file")?;
                }
                "--device" => config.device = args.next().ok_or("--device requires a value")?,
                "--serial" => config.serial = args.next().ok_or("--serial requires a path")?,
                "--capture" => {
                    config.capture = Some(args.next().ok_or("--capture requires a path")?.into())
                }
                "--record-capture" => {
                    config.record_capture = Some(
                        args.next()
                            .ok_or("--record-capture requires a new path")?
                            .into(),
                    )
                }
                "--catalog" => {
                    config.catalog = Some(args.next().ok_or("--catalog requires a path")?.into())
                }
                "--word-swap" => {
                    config.word_swap = args.next().ok_or("--word-swap requires auto, 0, or 1")?
                }
                "--bind" => {
                    config.bind = args.next().ok_or("--bind requires an address")?.parse()?
                }
                "--scenario" => {
                    config.scenario = args.next().ok_or("--scenario requires a name")?
                }
                "--admin-dir" => {
                    config.admin_dir =
                        Some(args.next().ok_or("--admin-dir requires a path")?.into())
                }
                "--data-dir" => {
                    config.data_dir = Some(args.next().ok_or("--data-dir requires a path")?.into())
                }
                "--network" => config.network = args.next().ok_or("--network requires a value")?,
                "--network-interface" => {
                    config.network_interface =
                        args.next().ok_or("--network-interface requires a value")?
                }
                "--update-repository" => {
                    let value = args
                        .next()
                        .ok_or("--update-repository requires OWNER/NAME or none")?;
                    config.update_repository = (value != "none").then_some(value);
                }
                "--secure-cookie" => config.secure_cookie = true,
                "--recover-network" => config.recover_network = true,
                "--freeze-time" => config.freeze_time = true,
                "--allow-simulated-controls" => config.allow_simulated_controls = true,
                "--allow-hardware-reads" => config.allow_hardware_reads = true,
                "--poll-registers" => {
                    config.poll_registers = id_list(args.next(), "--poll-registers")?
                }
                "--allow-hardware-writes" => config.allow_hardware_writes = true,
                "--no-open-writes" => config.open_writes = false,
                "--allow-alarm-reset" => config.allow_alarm_reset = true,
                "--alarm-auto-reset" => config.alarm_auto_reset = true,
                "--writable-registers" => {
                    config.writable_registers = id_list(args.next(), "--writable-registers")?
                }
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }
        Ok(Some(config))
    }
    pub fn network_backend(&self) -> &str {
        if self.network == "auto" {
            if self.device != "hardware" {
                "simulated"
            } else {
                "network-manager"
            }
        } else {
            &self.network
        }
    }
    /// Open writes apply only where a write can be sent and read back (ADR 0125).
    pub fn open_writes_active(&self) -> bool {
        self.open_writes && self.device == "hardware" && self.allow_hardware_reads
    }
    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.clone().unwrap_or_else(|| {
            if self.device != "hardware" {
                "./.data".into()
            } else {
                "/var/lib/nibe-gateway".into()
            }
        })
    }
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        match self.device.as_str() {
            "simulated" | "replay" => {}
            "hardware" if cfg!(feature = "serial") => {}
            "hardware" => {
                return Err("hardware transport requires --features serial (no fallback)".into())
            }
            _ => return Err("--device must be simulated, replay, or hardware".into()),
        }
        if let Some(repository) = &self.update_repository {
            if !nibe_api::releases::valid_repository(repository) {
                return Err("--update-repository must be OWNER/NAME or none".into());
            }
        }
        if !matches!(self.word_swap.as_str(), "auto" | "0" | "1") {
            return Err("--word-swap must be auto, 0, or 1".into());
        }
        if self.device != "simulated" && (self.freeze_time || self.allow_simulated_controls) {
            return Err("freeze-time and simulated controls require --device simulated".into());
        }
        if self.allow_hardware_reads && self.device != "hardware" {
            return Err("--allow-hardware-reads requires --device hardware".into());
        }
        if self.allow_hardware_writes == self.writable_registers.is_empty() {
            return Err(
                "--allow-hardware-writes and --writable-registers must be set together".into(),
            );
        }
        if self.allow_hardware_writes && (self.device != "hardware" || !self.allow_hardware_reads) {
            return Err(
                "hardware writes need --device hardware and --allow-hardware-reads (read-back)"
                    .into(),
            );
        }
        if (self.allow_alarm_reset || self.alarm_auto_reset)
            && (self.device != "hardware" || !self.allow_hardware_reads)
        {
            return Err("alarm reset needs --device hardware and --allow-hardware-reads".into());
        }
        if !(self.poll_registers.is_empty() && self.slow_poll_registers.is_empty())
            && (self.device == "simulated"
                || (self.device == "hardware" && !self.allow_hardware_reads))
        {
            return Err(
                "--poll-registers needs --device replay, or hardware with --allow-hardware-reads"
                    .into(),
            );
        }
        match self.network_backend() {
            "auto" | "simulated" => {}
            "network-manager" if cfg!(feature = "network-manager") => {}
            _ => {
                return Err(
                    "--network must be auto, simulated, or network-manager (requires feature)"
                        .into(),
                )
            }
        }
        self.wire.validate()?;
        if self.stale_after_ms == 0 {
            return Err("stale_after_ms must be positive".into());
        }
        if self.allow_simulated_controls && !cfg!(feature = "dev") {
            return Err("simulated controls require a --features dev build".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_modes_and_accepts_authenticated_lan_binding() {
        assert!(Config {
            device: "replay".into(),
            ..Config::default()
        }
        .validate()
        .is_ok());
        assert!(Config {
            device: "unknown".into(),
            ..Config::default()
        }
        .validate()
        .is_err());
        assert!(Config {
            bind: "0.0.0.0:8080".parse().unwrap(),
            ..Config::default()
        }
        .validate()
        .is_ok());
        assert!(Config {
            bind: "[::1]:8080".parse().unwrap(),
            ..Config::default()
        }
        .validate()
        .is_ok());
    }
    #[test]
    fn cli_overrides_and_invalid_options() {
        let config = Config::parse(vec![
            "--scenario".into(),
            "cold_weather".into(),
            "--freeze-time".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(config.scenario, "cold_weather");
        assert!(config.freeze_time);
        assert!(Config::parse(vec!["--bind".into()]).is_err());
        assert!(Config::parse(vec!["--unknown".into()]).is_err());
        let reads = Config::parse(vec!["--allow-hardware-reads".into()])
            .unwrap()
            .unwrap();
        assert!(reads.allow_hardware_reads);
        assert!(
            reads.validate().is_err(),
            "hardware reads need --device hardware"
        );
        assert!(!Config::default().allow_hardware_reads);
        // Open writes are on by default but need hardware with reads for the read-back (ADR 0125).
        assert!(Config::default().open_writes && !Config::default().open_writes_active());
        let hardware = Config {
            device: "hardware".into(),
            allow_hardware_reads: true,
            ..Config::default()
        };
        assert!(hardware.open_writes_active());
        let off = Config::parse(vec!["--no-open-writes".into()])
            .unwrap()
            .unwrap();
        assert!(!Config {
            device: "hardware".into(),
            allow_hardware_reads: true,
            ..off
        }
        .open_writes_active());
        let poll = Config::parse(vec!["--poll-registers".into(), "40004, 47011".into()])
            .unwrap()
            .unwrap();
        assert_eq!(poll.poll_registers, [40004, 47011]);
        assert!(
            poll.validate().is_err(),
            "polling needs replay or hardware reads"
        );
        assert!(Config::parse(vec!["--poll-registers".into(), "x".into()]).is_err());
        let writes = Config::parse(vec![
            "--allow-hardware-writes".into(),
            "--writable-registers".into(),
            "47011".into(),
        ])
        .unwrap()
        .unwrap();
        assert!(writes.validate().is_err(), "writes need hardware reads");
        assert!(Config::parse(vec!["--allow-hardware-writes".into()])
            .unwrap()
            .unwrap()
            .validate()
            .is_err());
    }
}
