use crate::{Result, ServiceManager};
use nibe_system::init::{self, Action, Init, Service};
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    process::Command,
    thread,
    time::{Duration, Instant},
};
/// Fixed service name and numeric loopback address: no remote URL, shell or proxy. Restarts go to
/// the detected init system: `systemctl restart nibe-gateway.service` or
/// `rc-service nibe-gateway restart`.
pub struct InitService {
    pub init: Init,
    pub address: SocketAddr,
    pub timeout: Duration,
    pub probation: Duration,
    pub max_restarts: u32,
}
impl ServiceManager for InitService {
    fn restart(&mut self) -> Result<()> {
        if !init::status(&self.init.commands(Service::Gateway, Action::Restart))? {
            return Err(crate::fail(match self.init {
                Init::Systemd => "systemd restart failed",
                Init::OpenRc => "openrc restart failed",
            }));
        }
        Ok(())
    }
    fn ready(&mut self, version: &str, release_id: &str) -> Result<bool> {
        let deadline = Instant::now() + self.timeout;
        let mut consecutive = 0;
        while Instant::now() < deadline {
            if probe(self.address, version, release_id) {
                consecutive += 1;
            } else {
                consecutive = 0;
            }
            if consecutive >= 3 {
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(500));
        }
        Ok(false)
    }
    fn probation(
        &mut self,
        version: &str,
        release_id: &str,
        restart: &mut dyn FnMut() -> Result<bool>,
    ) -> Result<bool> {
        monitor(
            &mut RealClock(Instant::now()),
            self.probation,
            self.timeout,
            self.max_restarts,
            &mut || health(self.address, Some((version, release_id))),
            restart,
        )
    }
}
pub fn probe(address: SocketAddr, version: &str, release_id: &str) -> bool {
    health(address, Some((version, release_id))).is_some_and(|h| h.ready && !h.fatal)
}
/// OS update confirmation: whichever app release runs must stay ready for `window`;
/// an outage longer than `outage` or a fatal device state fails the try. Restarts are tolerated twice.
pub fn os_probation(address: SocketAddr, window: Duration, outage: Duration) -> Result<bool> {
    monitor(
        &mut RealClock(Instant::now()),
        window,
        outage,
        3,
        &mut || health(address, None),
        &mut || Ok(true),
    )
}
#[derive(Clone, Debug)]
struct Sample {
    ready: bool,
    fatal: bool,
    instance: String,
    /// Error-free poll rounds the gateway completed since it started (0 if not reported).
    poll_rounds: u64,
}
/// Probation may end early after this much continuous readiness plus one error-free poll round:
/// across the first updates nothing changed after the first round. The full ready window
/// applies when no round completes (no pump, simulator, older gateway).
const EARLY_MINIMUM: Duration = Duration::from_secs(30);
trait Clock {
    fn now(&self) -> Duration;
    fn sleep(&mut self);
}
struct RealClock(Instant);
impl Clock for RealClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&mut self) {
        thread::sleep(Duration::from_millis(500));
    }
}
fn monitor(
    clock: &mut dyn Clock,
    window: Duration,
    outage: Duration,
    max_restarts: u32,
    probe: &mut dyn FnMut() -> Option<Sample>,
    restart: &mut dyn FnMut() -> Result<bool>,
) -> Result<bool> {
    let start = clock.now();
    let mut since = None;
    let mut unhealthy_since = None;
    let mut instance = None;
    let mut restarts = 0;
    // Bound flapping even when no single outage reaches the readiness timeout.
    let budget = window
        .saturating_add(outage)
        .saturating_mul(max_restarts.saturating_add(1));
    loop {
        let now = clock.now();
        match probe() {
            Some(sample) if sample.fatal => return Ok(false),
            Some(sample) if sample.ready && !sample.instance.is_empty() => {
                if instance.as_ref().is_some_and(|old| old != &sample.instance) {
                    restarts += 1;
                    if !restart()? || restarts >= max_restarts {
                        return Ok(false);
                    }
                    since = None;
                }
                instance = Some(sample.instance);
                unhealthy_since = None;
                let healthy_for = now.saturating_sub(*since.get_or_insert(now));
                if healthy_for >= window
                    || (healthy_for >= EARLY_MINIMUM && sample.poll_rounds >= 1)
                {
                    return Ok(true);
                }
            }
            _ => {
                since = None;
                if now.saturating_sub(*unhealthy_since.get_or_insert(now)) >= outage {
                    return Ok(false);
                }
            }
        }
        if now.saturating_sub(start) >= budget {
            return Ok(false);
        }
        clock.sleep();
    }
}
/// `expect` pins the app version and release id; `None` accepts any release (OS confirmation).
fn health(address: SocketAddr, expect: Option<(&str, &str)>) -> Option<Sample> {
    fn get(address: SocketAddr) -> Result<Vec<u8>> {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        stream
            .write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
        let mut bytes = Vec::new();
        stream.take(16385).read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    let Ok(bytes) = get(address) else {
        return None;
    };
    if bytes.len() > 16384 {
        return None;
    }
    let split = bytes.windows(4).position(|w| w == b"\r\n\r\n")?;
    let header = String::from_utf8_lossy(&bytes[..split]);
    if !header.lines().next().is_some_and(|l| {
        l.starts_with("HTTP/1.0 200 ")
            || l.starts_with("HTTP/1.1 200 ")
            || l.starts_with("HTTP/1.0 503 ")
            || l.starts_with("HTTP/1.1 503 ")
    }) {
        return None;
    }
    let Ok(json) = serde_json::from_slice::<Value>(&bytes[split + 4..]) else {
        return None;
    };
    if let Some((version, release_id)) = expect {
        if json["version"] != version || json["release_id"] != release_id {
            return None;
        }
    }
    Some(Sample {
        ready: header.lines().next().is_some_and(|l| l.contains(" 200 "))
            && json["status"] == "ok"
            && json["ready"] == true,
        fatal: json["device_fatal"] == true,
        instance: json["process_instance"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        poll_rounds: json["poll_rounds"].as_u64().unwrap_or(0),
    })
}

/// Explicit local test seam. Only the daemon launch configuration can name this
/// executable; the socket protocol cannot select programs or pass arguments.
pub struct Helper {
    pub executable: std::path::PathBuf,
    pub health: InitService,
}
impl ServiceManager for Helper {
    fn restart(&mut self) -> Result<()> {
        if !Command::new(&self.executable).status()?.success() {
            return Err(crate::fail("local helper failed"));
        }
        Ok(())
    }
    fn ready(&mut self, version: &str, release_id: &str) -> Result<bool> {
        self.health.ready(version, release_id)
    }
    fn probation(
        &mut self,
        version: &str,
        release_id: &str,
        restart: &mut dyn FnMut() -> Result<bool>,
    ) -> Result<bool> {
        self.health.probation(version, release_id, restart)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct FakeClock(Duration);
    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            self.0
        }
        fn sleep(&mut self) {
            self.0 += Duration::from_secs(1);
        }
    }
    #[test]
    fn probation_requires_continuous_readiness_and_counts_restarts() {
        let mut clock = FakeClock(Duration::ZERO);
        let mut samples = 0;
        let mut count = 0;
        let result = monitor(
            &mut clock,
            Duration::from_secs(600),
            Duration::from_secs(30),
            2,
            &mut || {
                samples += 1;
                Some(Sample {
                    ready: samples != 300,
                    fatal: false,
                    instance: "boot-a/process-a".into(),
                    poll_rounds: 0,
                })
            },
            &mut || {
                count += 1;
                Ok(true)
            },
        )
        .unwrap();
        assert!(result);
        assert_eq!(clock.0.as_secs(), 900);
        assert_eq!(count, 0);
        let mut clock = FakeClock(Duration::ZERO);
        let mut samples = 0;
        let result = monitor(
            &mut clock,
            Duration::from_secs(600),
            Duration::from_secs(30),
            2,
            &mut || {
                samples += 1;
                Some(Sample {
                    ready: true,
                    fatal: false,
                    instance: format!("process-{}", samples / 10),
                    poll_rounds: 0,
                })
            },
            &mut || {
                count += 1;
                Ok(true)
            },
        )
        .unwrap();
        assert!(!result);
        assert_eq!(count, 2);
    }
    #[test]
    fn one_error_free_poll_round_ends_probation_after_the_minimum() {
        // Rounds complete at 12 s, but readiness must also have lasted EARLY_MINIMUM.
        let mut clock = FakeClock(Duration::ZERO);
        let mut samples = 0u64;
        assert!(monitor(
            &mut clock,
            Duration::from_secs(600),
            Duration::from_secs(30),
            2,
            &mut || {
                samples += 1;
                Some(Sample {
                    ready: true,
                    fatal: false,
                    instance: "a".into(),
                    poll_rounds: u64::from(samples > 12),
                })
            },
            &mut || Ok(true)
        )
        .unwrap());
        assert_eq!(clock.0, EARLY_MINIMUM);
        // A restart resets the healthy period: the new process must earn its own minimum.
        let mut clock = FakeClock(Duration::ZERO);
        let mut samples = 0u64;
        assert!(monitor(
            &mut clock,
            Duration::from_secs(600),
            Duration::from_secs(30),
            3,
            &mut || {
                samples += 1;
                Some(Sample {
                    ready: true,
                    fatal: false,
                    instance: if samples < 20 { "a" } else { "b" }.into(),
                    poll_rounds: 1,
                })
            },
            &mut || Ok(true)
        )
        .unwrap());
        assert_eq!(clock.0.as_secs(), 19 + EARLY_MINIMUM.as_secs());
    }
    #[test]
    fn fatal_wire_error_and_unreachable_health_fail_probation() {
        for fatal in [false, true] {
            let mut clock = FakeClock(Duration::ZERO);
            assert!(!monitor(
                &mut clock,
                Duration::from_secs(600),
                Duration::from_secs(30),
                2,
                &mut || fatal.then(|| Sample {
                    ready: true,
                    fatal: true,
                    instance: "a".into(),
                    poll_rounds: 0,
                }),
                &mut || Ok(true)
            )
            .unwrap());
            assert_eq!(clock.0.as_secs(), if fatal { 0 } else { 30 });
        }
    }
}
