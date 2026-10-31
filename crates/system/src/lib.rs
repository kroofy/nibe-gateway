//! Root helper for system settings (ADR 0111): SSH keys for `nibe-admin` and Tailscale membership.
//! Fixed, validated JSON operations only; no shell, no free-form commands. Writes only the declared
//! persistent state (ADR 0107) and only on explicit requests.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub mod init;
pub mod provision;
pub mod supervise;

use init::{Action, Init, Service};

/// Largest accepted request or reply.
pub const MAX_MESSAGE: usize = 64 * 1024;
pub const MAX_KEYS: usize = 16;
/// Same prefixes as `nibe-provision.py` (ADR 0108).
const KEY_TYPES: [&str; 6] = [
    "ssh-ed25519",
    "ssh-rsa",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "sk-ssh-ed25519@openssh.com",
];
const TAILSCALE: &str = "/usr/bin/tailscale";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    SshStatus,
    SshSetKeys {
        keys: Vec<String>,
    },
    SshAddKey {
        key: String,
    },
    SshRemoveKey {
        fingerprint_sha256: String,
    },
    TailscaleStatus,
    TailscaleJoin {
        auth_key: String,
        #[serde(default)]
        hostname: Option<String>,
    },
    TailscaleDisconnect {
        #[serde(default)]
        logout: bool,
    },
    TailscaleDisable,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub error: Option<String>,
}

/// Where the managed state lives; injectable for tests.
#[derive(Debug, Clone)]
pub struct Paths {
    pub ssh_keys: PathBuf,
    pub tailscale_dir: PathBuf,
    /// Root-only runtime directory for the one-shot auth-key file.
    pub runtime_dir: PathBuf,
}
impl Default for Paths {
    fn default() -> Self {
        Self {
            ssh_keys: "/etc/ssh/authorized_keys/nibe-admin".into(),
            tailscale_dir: "/var/lib/tailscale".into(),
            runtime_dir: "/run/nibe-system".into(),
        }
    }
}

pub struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
}
/// Runs a fixed program with arguments; tests inject a fake.
pub trait Runner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> io::Result<Output>;
    /// The init system the commands go to (ADR 0117); fakes pin one.
    fn init(&self) -> Init {
        Init::detect()
    }
}
/// Real runner: absolute program paths, no shell, bounded wall time, stdout capped.
pub struct SystemRunner;
impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> io::Result<Output> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdout = child.stdout.take().expect("piped stdout");
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = (&mut stdout)
                .take(MAX_MESSAGE as u64)
                .read_to_end(&mut bytes);
            bytes
        });
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{program} timed out"),
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        Ok(Output {
            success: status.success(),
            stdout: reader.join().unwrap_or_default(),
        })
    }
}

fn invalid(message: &str) -> String {
    format!("invalid request: {message}")
}

/// A validated `authorized_keys` line: `<type> <base64 blob> [comment]`.
pub struct PublicKey {
    pub kind: String,
    pub blob: Vec<u8>,
    pub comment: String,
}
pub fn parse_key(line: &str) -> Result<PublicKey, String> {
    let line = line.trim();
    if line.is_empty() || line.len() > 8192 || line.contains(['\n', '\r', '\0']) {
        return Err(invalid("keys must be single lines under 8 KiB"));
    }
    let mut parts = line.splitn(3, ' ');
    let kind = parts.next().unwrap_or_default();
    if !KEY_TYPES.contains(&kind) {
        return Err(invalid("unsupported key type"));
    }
    let blob = base64_decode(parts.next().unwrap_or_default())
        .ok_or_else(|| invalid("key data is not base64"))?;
    // The blob starts with the length-prefixed key type; reject mismatches.
    let declared = blob
        .get(..4)
        .map(|n| u32::from_be_bytes([n[0], n[1], n[2], n[3]]) as usize)
        .and_then(|n| blob.get(4..4 + n));
    if declared != Some(kind.as_bytes()) {
        return Err(invalid("key data does not match its type"));
    }
    let comment = parts.next().unwrap_or_default().trim().to_string();
    if comment.chars().any(char::is_control) {
        return Err(invalid("key comment contains control characters"));
    }
    Ok(PublicKey {
        kind: kind.into(),
        blob,
        comment,
    })
}
/// OpenSSH-style `SHA256:<base64 without padding>`.
pub fn fingerprint(blob: &[u8]) -> String {
    format!(
        "SHA256:{}",
        base64_encode(&Sha256::digest(blob)).trim_end_matches('=')
    )
}

pub(crate) fn valid_auth_key(key: &str) -> bool {
    key.strip_prefix("tskey-").is_some_and(|rest| {
        (8..=200).contains(&rest.len())
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}
fn valid_hostname(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// Atomic write with explicit mode; the temporary file never outlives a failure.
fn write_atomic(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::other("no parent"))?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap().to_string_lossy()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Removes the one-shot auth-key file on every exit path.
struct Secret(PathBuf);
impl Drop for Secret {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub struct Helper<R: Runner> {
    pub paths: Paths,
    pub runner: R,
}
impl<R: Runner> Helper<R> {
    fn run(&self, program: &str, args: &[&str], seconds: u64) -> Result<Output, String> {
        let output = self
            .runner
            .run(program, args, Duration::from_secs(seconds))
            .map_err(|e| {
                format!(
                    "{} failed: {}",
                    program.rsplit('/').next().unwrap_or(program),
                    e.kind()
                )
            })?;
        Ok(output)
    }
    /// Starts/stops a managed service through the detected init system (ADR 0117), each step
    /// bounded by 30 s. Errors read as before on systemd (`systemctl start failed`).
    fn service(&self, service: Service, action: Action) -> Result<(), String> {
        for step in self.runner.init().commands(service, action) {
            if step.required {
                self.run_ok(step.program, &step.args, 30)?;
            } else {
                let _ = self.run(step.program, &step.args, 30);
            }
        }
        Ok(())
    }
    fn run_ok(&self, program: &str, args: &[&str], seconds: u64) -> Result<(), String> {
        if self.run(program, args, seconds)?.success {
            Ok(())
        } else {
            Err(format!(
                "{} {} failed",
                program.rsplit('/').next().unwrap_or(program),
                args.first().copied().unwrap_or_default()
            ))
        }
    }

    pub fn handle(&self, request: &Request) -> Result<Value, String> {
        match request {
            Request::SshStatus => self.ssh_status(),
            Request::SshSetKeys { keys } => self.ssh_set_keys(keys),
            Request::SshAddKey { key } => {
                let new = parse_key(key)?;
                let mut keys = self.current_keys()?;
                if !keys
                    .iter()
                    .any(|k| parse_key(k).is_ok_and(|k| k.blob == new.blob))
                {
                    keys.push(key.trim().to_string());
                }
                self.ssh_set_keys(&keys)
            }
            Request::SshRemoveKey { fingerprint_sha256 } => {
                let keys = self.current_keys()?;
                let kept: Vec<String> = keys
                    .iter()
                    .filter(|k| {
                        !parse_key(k).is_ok_and(|k| &fingerprint(&k.blob) == fingerprint_sha256)
                    })
                    .cloned()
                    .collect();
                if kept.len() == keys.len() {
                    return Err(invalid("no key with that fingerprint"));
                }
                self.ssh_set_keys(&kept)
            }
            Request::TailscaleStatus => self.tailscale_status(),
            Request::TailscaleJoin { auth_key, hostname } => {
                self.tailscale_join(auth_key, hostname.as_deref())
            }
            Request::TailscaleDisconnect { logout } => {
                self.run_ok(TAILSCALE, &[if *logout { "logout" } else { "down" }], 30)?;
                self.tailscale_status()
            }
            Request::TailscaleDisable => {
                let _ = self.run(TAILSCALE, &["down"], 30);
                match fs::remove_file(self.paths.tailscale_dir.join("enabled")) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("cannot remove marker: {}", e.kind())),
                }
                self.service(Service::Tailscale, Action::Disable)?;
                Ok(json!({ "enabled": false }))
            }
        }
    }

    fn ssh_status(&self) -> Result<Value, String> {
        let text = match fs::read_to_string(&self.paths.ssh_keys) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("cannot read keys: {}", e.kind())),
        };
        let keys: Vec<Value> = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .map(|line| match parse_key(line) {
                Ok(key) => json!({
                    "type": key.kind,
                    "comment": key.comment,
                    "fingerprint_sha256": fingerprint(&key.blob),
                }),
                Err(_) => json!({ "type": "unparsed", "comment": "", "fingerprint_sha256": null }),
            })
            .collect();
        Ok(json!({ "enabled": !keys.is_empty(), "keys": keys }))
    }

    fn current_keys(&self) -> Result<Vec<String>, String> {
        match fs::read_to_string(&self.paths.ssh_keys) {
            Ok(text) => Ok(text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(String::from)
                .collect()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(format!("cannot read keys: {}", e.kind())),
        }
    }
    fn ssh_set_keys(&self, keys: &[String]) -> Result<Value, String> {
        if keys.len() > MAX_KEYS {
            return Err(invalid("at most 16 keys"));
        }
        let mut lines = String::new();
        for key in keys {
            parse_key(key)?;
            lines.push_str(key.trim());
            lines.push('\n');
        }
        if keys.is_empty() {
            match fs::remove_file(&self.paths.ssh_keys) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("cannot remove keys: {}", e.kind())),
            }
            // Existing sessions survive (Debian ssh.service uses KillMode=process; Alpine's
            // sshd init script only signals the listener's pidfile).
            self.service(Service::Ssh, Action::Disable)?;
        } else {
            write_atomic(&self.paths.ssh_keys, lines.as_bytes(), 0o644)
                .map_err(|e| format!("cannot write keys: {}", e.kind()))?;
            self.service(Service::Ssh, Action::Enable)?;
        }
        self.ssh_status()
    }

    fn tailscale_status(&self) -> Result<Value, String> {
        let enabled = self.paths.tailscale_dir.join("enabled").exists();
        let status = match self.run(TAILSCALE, &["status", "--json"], 15) {
            Ok(output) if output.success || !output.stdout.is_empty() => {
                serde_json::from_slice::<Value>(&output.stdout).unwrap_or(Value::Null)
            }
            _ => Value::Null,
        };
        let me = &status["Self"];
        Ok(json!({
            "enabled": enabled,
            "backend_state": status["BackendState"].as_str().unwrap_or("Stopped"),
            "dns_name": me["DNSName"].as_str().map(|n| n.trim_end_matches('.')),
            "ips": me["TailscaleIPs"].as_array().cloned().unwrap_or_default(),
            "key_expiry": me["KeyExpiry"].as_str(),
        }))
    }

    fn tailscale_join(&self, auth_key: &str, hostname: Option<&str>) -> Result<Value, String> {
        if !valid_auth_key(auth_key) {
            return Err(invalid("auth key must look like tskey-…"));
        }
        if hostname.is_some_and(|h| !valid_hostname(h)) {
            return Err(invalid(
                "hostname must be 1-63 lowercase letters, digits or '-'",
            ));
        }
        fs::create_dir_all(&self.paths.tailscale_dir)
            .map_err(|e| format!("cannot create state dir: {}", e.kind()))?;
        write_atomic(&self.paths.tailscale_dir.join("enabled"), b"", 0o600)
            .map_err(|e| format!("cannot write marker: {}", e.kind()))?;
        self.service(Service::Tailscale, Action::Enable)?;
        let file = self.paths.runtime_dir.join("tailscale-auth-key");
        write_atomic(&file, auth_key.as_bytes(), 0o600)
            .map_err(|e| format!("cannot stage key: {}", e.kind()))?;
        let _secret = Secret(file.clone());
        let key_arg = format!("--auth-key=file:{}", file.display());
        let host_arg = hostname.map(|h| format!("--hostname={h}"));
        let mut args = vec!["up", "--reset", key_arg.as_str()];
        if let Some(h) = &host_arg {
            args.push(h);
        }
        self.run_ok(TAILSCALE, &args, 90)?;
        self.tailscale_status()
    }
}

fn send(stream: &mut UnixStream, bytes: &[u8]) -> io::Result<()> {
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)
}
fn receive(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut len = [0; 4];
    stream.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut bytes = vec![0; len];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}
/// Client side, used by the gateway: one request per connection.
pub fn request(socket: &Path, request: &Request) -> io::Result<Reply> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    send(&mut stream, &serde_json::to_vec(request)?)?;
    serde_json::from_slice(&receive(&mut stream)?).map_err(io::Error::other)
}
/// Server side: reads one bounded request and answers it. Never echoes request content.
pub fn serve_one<R: Runner>(helper: &Helper<R>, stream: &mut UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let reply = match receive(stream)
        .and_then(|b| serde_json::from_slice::<Request>(&b).map_err(io::Error::other))
    {
        Err(_) => Reply {
            ok: false,
            result: Value::Null,
            error: Some(invalid("malformed")),
        },
        Ok(request) => match helper.handle(&request) {
            Ok(result) => Reply {
                ok: true,
                result,
                error: None,
            },
            Err(error) => Reply {
                ok: false,
                result: Value::Null,
                error: Some(error),
            },
        },
    };
    send(stream, &serde_json::to_vec(&reply)?)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    if text.is_empty() || text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let v = B64.iter().position(|&b| b == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque};

    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        outputs: RefCell<VecDeque<Output>>,
        key_file_seen: RefCell<Option<(String, u32)>>,
        openrc: bool,
    }
    impl Runner for &Fake {
        fn init(&self) -> Init {
            if self.openrc {
                Init::OpenRc
            } else {
                Init::Systemd
            }
        }
        fn run(&self, program: &str, args: &[&str], _: Duration) -> io::Result<Output> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            if let Some(arg) = args.iter().find_map(|a| a.strip_prefix("--auth-key=file:")) {
                let mode = fs::metadata(arg)?.permissions().mode() & 0o777;
                *self.key_file_seen.borrow_mut() = Some((fs::read_to_string(arg)?, mode));
            }
            Ok(self.outputs.borrow_mut().pop_front().unwrap_or(Output {
                success: true,
                stdout: vec![],
            }))
        }
    }
    fn helper<'a>(dir: &Path, fake: &'a Fake) -> Helper<&'a Fake> {
        Helper {
            paths: Paths {
                ssh_keys: dir.join("ssh/authorized_keys/nibe-admin"),
                tailscale_dir: dir.join("tailscale"),
                runtime_dir: dir.join("run"),
            },
            runner: fake,
        }
    }
    // Real ed25519 public key layout: "ssh-ed25519" length-prefixed + 32-byte key.
    fn key(comment: &str) -> String {
        let mut blob = Vec::new();
        blob.extend_from_slice(&11u32.to_be_bytes());
        blob.extend_from_slice(b"ssh-ed25519");
        blob.extend_from_slice(&32u32.to_be_bytes());
        blob.extend_from_slice(&[7; 32]);
        format!("ssh-ed25519 {} {comment}", base64_encode(&blob))
    }

    #[test]
    fn base64_roundtrip_and_fingerprint_format() {
        for data in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 1, 2]] {
            if !data.is_empty() {
                assert_eq!(base64_decode(&base64_encode(data)).unwrap(), data);
            }
        }
        let fp = fingerprint(b"x");
        assert!(fp.starts_with("SHA256:") && !fp.ends_with('='));
    }

    #[test]
    fn key_validation() {
        assert!(parse_key(&key("me@laptop")).is_ok());
        for bad in [
            "ssh-ed25519".to_string(),
            "rm -rf /".into(),
            format!("{}\nssh-ed25519 x", key("a")),
            key("a").replace("ssh-ed25519 ", "ssh-rsa "),
            "ssh-ed25519 !!!notbase64".into(),
        ] {
            assert!(parse_key(&bad).is_err(), "{bad:?}");
        }
        assert!(valid_auth_key("tskey-auth-abc12345"));
        assert!(!valid_auth_key("tskey-a;b"));
        assert!(!valid_auth_key("nope"));
        assert!(valid_hostname("nibe-gateway") && !valid_hostname("-x") && !valid_hostname("Nibe"));
    }

    #[test]
    fn ssh_keys_are_written_atomically_and_toggle_the_service() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let h = helper(dir.path(), &fake);
        let status = h
            .handle(&Request::SshSetKeys {
                keys: vec![key("one"), key("two")],
            })
            .unwrap();
        assert_eq!(status["enabled"], true);
        assert_eq!(status["keys"].as_array().unwrap().len(), 2);
        assert_eq!(status["keys"][0]["comment"], "one");
        let mode = fs::metadata(&h.paths.ssh_keys)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o644);
        assert!(h
            .handle(&Request::SshSetKeys {
                keys: vec!["bad".into()]
            })
            .is_err());
        assert_eq!(
            fs::read_to_string(&h.paths.ssh_keys)
                .unwrap()
                .lines()
                .count(),
            2,
            "a rejected request leaves the file untouched"
        );
        let too_many = vec![key("k"); MAX_KEYS + 1];
        assert!(h.handle(&Request::SshSetKeys { keys: too_many }).is_err());
        let off = h.handle(&Request::SshSetKeys { keys: vec![] }).unwrap();
        assert_eq!(off["enabled"], false);
        assert!(!h.paths.ssh_keys.exists());
        assert_eq!(
            *fake.calls.borrow(),
            [
                "/usr/bin/systemctl start ssh.service",
                "/usr/bin/systemctl stop ssh.service"
            ]
        );
    }

    #[test]
    fn add_and_remove_single_keys_by_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let h = helper(dir.path(), &fake);
        h.handle(&Request::SshAddKey { key: key("one") }).unwrap();
        let status = h.handle(&Request::SshAddKey { key: key("one") }).unwrap();
        assert_eq!(
            status["keys"].as_array().unwrap().len(),
            1,
            "same key is not added twice"
        );
        let fp = status["keys"][0]["fingerprint_sha256"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(h
            .handle(&Request::SshRemoveKey {
                fingerprint_sha256: "SHA256:nope".into()
            })
            .is_err());
        let off = h
            .handle(&Request::SshRemoveKey {
                fingerprint_sha256: fp,
            })
            .unwrap();
        assert_eq!(off["enabled"], false, "removing the last key turns SSH off");
    }
    #[test]
    fn tailscale_join_stages_key_privately_and_always_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let h = helper(dir.path(), &fake);
        assert!(h
            .handle(&Request::TailscaleJoin {
                auth_key: "bad".into(),
                hostname: None
            })
            .is_err());
        assert!(fake.calls.borrow().is_empty(), "invalid input runs nothing");
        let status = br#"{"BackendState":"Running","Self":{"DNSName":"nibe-gateway.tail.ts.net.","TailscaleIPs":["100.1.2.3"],"KeyExpiry":"2027-04-22T00:00:00Z"}}"#;
        fake.outputs.borrow_mut().extend([
            Output {
                success: true,
                stdout: vec![],
            },
            Output {
                success: true,
                stdout: vec![],
            },
            Output {
                success: true,
                stdout: status.to_vec(),
            },
        ]);
        let result = h
            .handle(&Request::TailscaleJoin {
                auth_key: "tskey-auth-secret123".into(),
                hostname: Some("nibe-gateway".into()),
            })
            .unwrap();
        assert_eq!(result["backend_state"], "Running");
        assert_eq!(result["dns_name"], "nibe-gateway.tail.ts.net");
        assert_eq!(result["enabled"], true);
        assert_eq!(
            *fake.key_file_seen.borrow(),
            Some(("tskey-auth-secret123".into(), 0o600))
        );
        assert!(!h.paths.runtime_dir.join("tailscale-auth-key").exists());
        let calls = fake.calls.borrow().clone();
        assert!(
            calls.iter().all(|c| !c.contains("secret123")),
            "key never on a command line"
        );
        // Failure path removes the staged key too.
        fake.outputs.borrow_mut().extend([
            Output {
                success: true,
                stdout: vec![],
            },
            Output {
                success: false,
                stdout: vec![],
            },
        ]);
        assert!(h
            .handle(&Request::TailscaleJoin {
                auth_key: "tskey-auth-secret456".into(),
                hostname: None
            })
            .is_err());
        assert!(!h.paths.runtime_dir.join("tailscale-auth-key").exists());
    }

    #[test]
    fn tailscale_disable_removes_marker_and_stops_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let h = helper(dir.path(), &fake);
        fs::create_dir_all(&h.paths.tailscale_dir).unwrap();
        fs::write(h.paths.tailscale_dir.join("enabled"), b"").unwrap();
        assert_eq!(
            h.handle(&Request::TailscaleDisable).unwrap()["enabled"],
            false
        );
        assert!(!h.paths.tailscale_dir.join("enabled").exists());
        assert_eq!(
            fake.calls.borrow().last().unwrap(),
            "/usr/bin/systemctl stop tailscaled.service"
        );
        h.handle(&Request::TailscaleDisconnect { logout: true })
            .unwrap();
        assert!(fake
            .calls
            .borrow()
            .iter()
            .any(|c| c == "/usr/bin/tailscale logout"));
    }

    #[test]
    fn openrc_toggles_alpine_services_and_runlevels() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            openrc: true,
            ..Fake::default()
        };
        let h = helper(dir.path(), &fake);
        h.handle(&Request::SshSetKeys {
            keys: vec![key("one")],
        })
        .unwrap();
        h.handle(&Request::SshSetKeys { keys: vec![] }).unwrap();
        h.handle(&Request::TailscaleJoin {
            auth_key: "tskey-auth-secret123".into(),
            hostname: None,
        })
        .unwrap();
        fs::write(h.paths.tailscale_dir.join("enabled"), b"").unwrap();
        // `rc-update del` exits 1 when already absent; disabling still succeeds.
        fake.outputs.borrow_mut().extend([
            Output {
                success: true,
                stdout: vec![],
            },
            Output {
                success: true,
                stdout: vec![],
            },
            Output {
                success: false,
                stdout: vec![],
            },
        ]);
        h.handle(&Request::TailscaleDisable).unwrap();
        let calls: Vec<String> = fake
            .calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("/sbin/"))
            .cloned()
            .collect();
        assert_eq!(
            calls,
            [
                "/sbin/rc-update add sshd default",
                "/sbin/rc-service sshd start",
                "/sbin/rc-service sshd stop",
                "/sbin/rc-update del sshd default",
                "/sbin/rc-update add tailscale default",
                "/sbin/rc-service tailscale start",
                "/sbin/rc-service tailscale stop",
                "/sbin/rc-update del tailscale default",
            ]
        );
        assert!(fake.calls.borrow().iter().all(|c| !c.contains("systemctl")));
        // A failed start is reported with the program and verb, as on systemd.
        let fake = Fake {
            openrc: true,
            ..Fake::default()
        };
        fake.outputs.borrow_mut().extend([
            Output {
                success: true,
                stdout: vec![],
            },
            Output {
                success: false,
                stdout: vec![],
            },
        ]);
        let h = helper(dir.path(), &fake);
        assert_eq!(
            h.handle(&Request::SshSetKeys {
                keys: vec![key("one")]
            })
            .unwrap_err(),
            "rc-service sshd failed"
        );
    }

    #[test]
    fn socket_roundtrip_and_malformed_requests() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("control.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let root = dir.path().to_path_buf();
        let server = std::thread::spawn(move || {
            let fake = Fake::default();
            let h = helper(&root, &fake);
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                serve_one(&h, &mut stream).unwrap();
            }
        });
        let reply = request(&socket, &Request::SshStatus).unwrap();
        assert!(reply.ok);
        assert_eq!(reply.result["enabled"], false);
        let mut raw = UnixStream::connect(&socket).unwrap();
        send(&mut raw, br#"{"op":"run","cmd":"id"}"#).unwrap();
        let reply: Reply = serde_json::from_slice(&receive(&mut raw).unwrap()).unwrap();
        assert!(!reply.ok);
        server.join().unwrap();
    }
}
