//! sd_notify readiness/watchdog for the gateway, ported from `supervise.py` (ADR 0116 removes Python).
//! Only successful HTTP health probes refresh the watchdog. Device disconnection stays an HTTP-200 degraded
//! status, so a heat-pump fault cannot restart-loop the API. The supervisor wraps any gateway release, so
//! OS and app updates stay independent.
//!
//! Without `NOTIFY_SOCKET` (OpenRC `supervise-daemon`, ADR 0117) nobody else enforces the watchdog, so
//! the supervisor does: no healthy probe within `NIBE_START_TIMEOUT_SEC` (default 60) of the start, or
//! for `NIBE_WATCHDOG_SEC` (default 30) after the last healthy one, stops the child (SIGTERM, SIGKILL
//! after 10 s) and exits 1 so supervise-daemon respawns it.
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    os::unix::net::UnixDatagram,
    path::Path,
    process::{Child, Command},
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
    thread,
    time::{Duration, Instant},
};

static STOPPING: AtomicBool = AtomicBool::new(false);
static CHILD: AtomicI32 = AtomicI32::new(0);

extern "C" fn forward(signal: libc::c_int) {
    STOPPING.store(true, Ordering::SeqCst);
    let pid = CHILD.load(Ordering::SeqCst);
    if pid > 0 {
        // SAFETY: kill(2) is async-signal-safe; pid is our own child.
        unsafe {
            libc::kill(pid, signal);
        }
    }
}

/// Sends one datagram to `$NOTIFY_SOCKET` (path or `@abstract`); silently does nothing without one.
pub fn notify(message: &str) {
    let Ok(address) = std::env::var("NOTIFY_SOCKET") else {
        return;
    };
    let Ok(socket) = UnixDatagram::unbound() else {
        return;
    };
    if let Some(name) = address.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        if let Ok(addr) = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()) {
            let _ = socket.send_to_addr(message.as_bytes(), &addr);
        }
    } else {
        let _ = socket.send_to(message.as_bytes(), &address);
    }
}

/// `http://host:port/path` only; the health check never goes through a proxy.
pub fn parse_url(url: &str) -> Option<(SocketAddr, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_string()), |(a, p)| (a, format!("/{p}")));
    let address = authority.to_socket_addrs().ok()?.next()?;
    Some((address, path))
}

/// HTTP 200 with JSON `status == "ok"` and `ready == true`, within 2 s.
pub fn healthy(address: SocketAddr, path: &str) -> bool {
    let attempt = || -> std::io::Result<bool> {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        stream.write_all(
            format!("GET {path} HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )?;
        let mut bytes = Vec::new();
        stream.take(16385).read_to_end(&mut bytes)?;
        let Some(split) = bytes.windows(4).position(|w| w == b"\r\n\r\n") else {
            return Ok(false);
        };
        let status_line = String::from_utf8_lossy(&bytes[..split]);
        if status_line
            .lines()
            .next()
            .is_none_or(|l| l.split(' ').nth(1) != Some("200"))
        {
            return Ok(false);
        }
        let Ok(body) = serde_json::from_slice::<serde_json::Value>(&bytes[split + 4..]) else {
            return Ok(false);
        };
        Ok(body["status"] == "ok" && body["ready"] == true)
    };
    attempt().unwrap_or(false)
}

fn release_id(program: &Path) -> Option<String> {
    let manifest = program.canonicalize().ok()?.parent()?.join("manifest.json");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(manifest).ok()?).ok()?;
    json["sha256"].as_str().map(str::to_string)
}

fn exited(child: &mut Child) -> Option<std::process::ExitStatus> {
    child.try_wait().ok().flatten()
}

/// The supervisor's own health watchdog, used when systemd does not provide one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Watchdog {
    /// Allowed time from start to the first healthy probe.
    pub start_timeout: Duration,
    /// Allowed time without a healthy probe once the gateway has been healthy.
    pub timeout: Duration,
    /// SIGTERM to SIGKILL.
    pub kill_grace: Duration,
}
impl Watchdog {
    /// `None` under systemd (`NOTIFY_SOCKET` set): `WatchdogSec=` owns the watchdog there.
    pub fn from_env() -> Option<Self> {
        if std::env::var_os("NOTIFY_SOCKET").is_some() {
            return None;
        }
        let seconds = |name: &str, default: f64| {
            let value = std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= 86_400.0)
                .unwrap_or(default);
            Duration::from_secs_f64(value)
        };
        Some(Self {
            start_timeout: seconds("NIBE_START_TIMEOUT_SEC", 60.0),
            timeout: seconds("NIBE_WATCHDOG_SEC", 30.0),
            kill_grace: Duration::from_secs(10),
        })
    }
}

/// SIGTERM, then SIGKILL after `grace`; reaps the child.
fn terminate(child: &mut Child, grace: Duration) {
    // SAFETY: kill(2) on our own, not yet reaped child.
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if exited(child).is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs `command` and reports readiness/watchdog while its health URL answers. Returns the exit code.
pub fn supervise(command: &[String], health_url: &str) -> i32 {
    supervise_with(command, health_url, Watchdog::from_env())
}

/// [`supervise`] with an explicit own watchdog (`None`: systemd's, via sd_notify).
pub fn supervise_with(command: &[String], health_url: &str, watchdog: Option<Watchdog>) -> i32 {
    let Some((address, path)) = parse_url(health_url) else {
        eprintln!("nibe-supervise: NIBE_HEALTH_URL must be http://host:port/path");
        return 2;
    };
    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..]);
    for key in ["NOTIFY_SOCKET", "WATCHDOG_USEC", "WATCHDOG_PID"] {
        cmd.env_remove(key);
    }
    if let Some(id) = release_id(Path::new(&command[0])) {
        cmd.env("NIBE_RELEASE_ID", id);
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!(
                "nibe-supervise: cannot start {}: {}",
                command[0],
                error.kind()
            );
            return 1;
        }
    };
    CHILD.store(child.id() as i32, Ordering::SeqCst);
    // SAFETY: installs an async-signal-safe handler (atomics + kill only).
    unsafe {
        let handler = forward as extern "C" fn(libc::c_int);
        libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, handler as libc::sighandler_t);
    }
    let watchdog_usec: f64 = match watchdog {
        // Probe three times per own watchdog period, like systemd's WatchdogSec/3.
        Some(own) => own.timeout.as_secs_f64() * 1_000_000.0,
        None => std::env::var("WATCHDOG_USEC")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30_000_000.0),
    };
    let interval = Duration::from_secs_f64((watchdog_usec / 3_000_000.0).clamp(0.1, 5.0));
    let mut ready = false;
    let started = Instant::now();
    let mut last_healthy = started;
    let status = loop {
        if let Some(status) = exited(&mut child) {
            break Some(status);
        }
        let stopping = STOPPING.load(Ordering::SeqCst);
        if !stopping && healthy(address, &path) {
            if !ready {
                notify("READY=1\nSTATUS=Gateway HTTP health ready");
                ready = true;
            }
            last_healthy = Instant::now();
            notify("WATCHDOG=1");
        } else if !stopping {
            notify("STATUS=Waiting for gateway HTTP health");
            if let Some(own) = watchdog {
                let (since, limit, what) = if ready {
                    (last_healthy, own.timeout, "unhealthy")
                } else {
                    (started, own.start_timeout, "not healthy after start")
                };
                if since.elapsed() >= limit {
                    eprintln!(
                        "nibe-supervise: watchdog: gateway {what} for {:.0} s; stopping it for a restart",
                        limit.as_secs_f64()
                    );
                    terminate(&mut child, own.kill_grace);
                    // A stop request that raced the watchdog still ends cleanly.
                    return i32::from(!STOPPING.load(Ordering::SeqCst));
                }
            }
        }
        // Notice the child's exit promptly without hammering the health endpoint.
        let deadline = Instant::now() + interval;
        while Instant::now() < deadline {
            if exited(&mut child).is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    };
    let status = match status {
        Some(status) => status,
        None => {
            let _ = child.kill();
            match child.wait() {
                Ok(status) => status,
                Err(_) => return 1,
            }
        }
    };
    if STOPPING.load(Ordering::SeqCst) {
        return 0;
    }
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(0), _) | (None, None) => 1,
        (Some(code), _) => code,
        (None, Some(signal)) => 128 + signal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(response: &'static str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        address
    }
    #[test]
    fn health_needs_200_ok_and_ready() {
        let ok = serve_once("HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\",\"ready\":true}");
        assert!(healthy(ok, "/health"));
        let degraded = serve_once("HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\",\"ready\":false}");
        assert!(!healthy(degraded, "/health"));
        let error = serve_once(
            "HTTP/1.1 503 Service Unavailable\r\n\r\n{\"status\":\"ok\",\"ready\":true}",
        );
        assert!(!healthy(error, "/health"));
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        assert!(!healthy(closed, "/health"));
    }
    /// Answers every request; healthy while `flag` is set.
    fn serve_toggle(flag: std::sync::Arc<AtomicBool>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0; 1024];
                let _ = stream.read(&mut buf);
                let ready = flag.load(Ordering::SeqCst);
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {} X\r\n\r\n{{\"status\":\"ok\",\"ready\":{ready}}}",
                        if ready { 200 } else { 503 }
                    )
                    .as_bytes(),
                );
            }
        });
        format!("http://{address}/health")
    }
    fn own(start: f64, timeout: f64, grace: f64) -> Option<Watchdog> {
        Some(Watchdog {
            start_timeout: Duration::from_secs_f64(start),
            timeout: Duration::from_secs_f64(timeout),
            kill_grace: Duration::from_secs_f64(grace),
        })
    }
    fn sh(script: &str) -> Vec<String> {
        vec!["/bin/sh".into(), "-c".into(), script.into()]
    }

    #[test]
    fn own_watchdog_enforces_the_start_timeout() {
        let url = serve_toggle(Default::default());
        let started = Instant::now();
        assert_eq!(
            supervise_with(&sh("exec sleep 30"), &url, own(0.5, 0.3, 5.0)),
            1
        );
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(500) && elapsed < Duration::from_secs(4),
            "{elapsed:?}"
        );
    }
    #[test]
    fn own_watchdog_stops_a_gateway_that_turns_unhealthy() {
        let flag = std::sync::Arc::new(AtomicBool::new(true));
        let url = serve_toggle(flag.clone());
        let toggler = {
            let flag = flag.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(1500));
                flag.store(false, Ordering::SeqCst);
            })
        };
        let started = Instant::now();
        // The start timeout alone (0.5 s) would have fired long before 1.5 s: health resets it.
        assert_eq!(
            supervise_with(&sh("exec sleep 30"), &url, own(0.5, 0.6, 5.0)),
            1
        );
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(1800) && elapsed < Duration::from_secs(5),
            "{elapsed:?}"
        );
        toggler.join().unwrap();
    }
    #[test]
    fn own_watchdog_kills_a_child_that_ignores_sigterm() {
        let url = serve_toggle(Default::default());
        let started = Instant::now();
        assert_eq!(
            supervise_with(&sh("trap '' TERM; exec sleep 30"), &url, own(0.2, 0.2, 0.5)),
            1
        );
        let elapsed = started.elapsed();
        // Start timeout plus the full SIGTERM grace: only SIGKILL ends this child.
        assert!(
            elapsed >= Duration::from_millis(700) && elapsed < Duration::from_secs(4),
            "{elapsed:?}"
        );
    }
    #[test]
    fn without_own_watchdog_an_unhealthy_child_keeps_running() {
        let url = serve_toggle(Default::default());
        assert_eq!(
            supervise_with(&sh("sleep 1; exit 5"), &url, None),
            5,
            "systemd owns the watchdog"
        );
    }
    #[test]
    fn urls_are_http_only() {
        assert_eq!(
            parse_url("http://127.0.0.1:8080/health"),
            Some(("127.0.0.1:8080".parse().unwrap(), "/health".into()))
        );
        assert!(parse_url("https://127.0.0.1/health").is_none());
    }
    #[test]
    fn exit_codes_follow_the_child() {
        let url = "http://127.0.0.1:9/health";
        assert_eq!(
            supervise(&["/bin/sh".into(), "-c".into(), "exit 3".into()], url),
            3
        );
        assert_eq!(
            supervise(&["/bin/sh".into(), "-c".into(), "exit 0".into()], url),
            1,
            "a clean exit is still a failure"
        );
        assert_eq!(
            supervise(&["/bin/sh".into(), "-c".into(), "kill -9 $$".into()], url),
            137
        );
        assert_eq!(supervise(&["/nonexistent".into()], url), 1);
    }
}
