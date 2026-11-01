//! First-boot provisioning from `/boot/firmware/nibe-provision.json`, ported from
//! `nibe-provision.py` (Python was removed from the image). Applies the file once, then overwrites and deletes it.
//! Never logs secrets: errors name the field, not its value.
use crate::{parse_key, valid_auth_key, Runner};
use serde_json::{Map, Value};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

pub struct ProvisionPaths {
    pub boot: PathBuf,
    pub source: PathBuf,
    pub ssh_keys: PathBuf,
    pub tailscale_dir: PathBuf,
    pub gateway_file: PathBuf,
}
impl Default for ProvisionPaths {
    fn default() -> Self {
        Self {
            boot: "/boot/firmware".into(),
            source: "/boot/firmware/nibe-provision.json".into(),
            ssh_keys: "/etc/ssh/authorized_keys/nibe-admin".into(),
            tailscale_dir: "/var/lib/tailscale".into(),
            gateway_file: "/var/lib/nibe-gateway/state/provision.json".into(),
        }
    }
}

/// Owner of the gateway hand-off file; `None` keeps the caller's (tests).
pub type Owner = Option<(u32, u32)>;

pub fn validate(config: &Value) -> Result<(), String> {
    let Some(map) = config.as_object() else {
        return Err("top level must be a JSON object".into());
    };
    let allowed = [
        "wifi",
        "admin_password",
        "ssh_authorized_keys",
        "tailscale_auth_key",
    ];
    let mut unknown: Vec<&str> = map
        .keys()
        .map(String::as_str)
        .filter(|k| !allowed.contains(k))
        .collect();
    if !unknown.is_empty() {
        unknown.sort_unstable();
        return Err(format!("unknown keys: {unknown:?}"));
    }
    if let Some(wifi) = map.get("wifi").filter(|v| !v.is_null()) {
        let ssid = wifi.get("ssid").and_then(Value::as_str);
        let password = match wifi.get("password") {
            None => Some(""),
            Some(p) => p.as_str(),
        };
        if !(wifi.is_object()
            && ssid.is_some_and(|s| (1..=32).contains(&s.len()))
            && password.is_some_and(|p| p.chars().count() <= 64))
        {
            return Err("wifi needs ssid (1-32 bytes) and password (<= 64 chars)".into());
        }
    }
    if let Some(password) = map.get("admin_password").filter(|v| !v.is_null()) {
        if !password
            .as_str()
            .is_some_and(|p| (12..=256).contains(&p.chars().count()))
        {
            return Err("admin_password must be 12-256 characters".into());
        }
    }
    match map.get("ssh_authorized_keys") {
        None | Some(Value::Null) => {}
        Some(Value::Array(keys)) => {
            for key in keys {
                let Some(line) = key.as_str() else {
                    return Err(
                        "ssh_authorized_keys must be a list of single-line public keys".into(),
                    );
                };
                parse_key(line).map_err(|_| {
                    "ssh_authorized_keys must be a list of single-line public keys".to_string()
                })?;
            }
        }
        Some(_) => {
            return Err("ssh_authorized_keys must be a list of single-line public keys".into())
        }
    }
    if let Some(key) = map.get("tailscale_auth_key").filter(|v| !v.is_null()) {
        if !key.as_str().is_some_and(valid_auth_key) {
            return Err("tailscale_auth_key must look like tskey-...".into());
        }
    }
    Ok(())
}

/// tmp + fsync + chown + rename, so a power cut never leaves a half-written secret.
fn write_private(path: &Path, data: &[u8], owner: Owner, mode: u32) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
    ));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
    }
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    if let Some((uid, gid)) = owner {
        std::os::unix::fs::chown(&tmp, Some(uid), Some(gid))?;
    }
    fs::rename(&tmp, path)
}

/// FAT has no secure delete: overwrite with zeros, sync, unlink. The boot partition is mounted read-only
/// and is remounted read-write only for this.
fn remove_source(paths: &ProvisionPaths, runner: &dyn Runner, remount: bool) -> io::Result<()> {
    let boot = paths.boot.to_string_lossy().into_owned();
    if remount {
        let ok = runner.run(
            "/bin/mount",
            &["-o", "remount,rw", &boot],
            Duration::from_secs(30),
        )?;
        if !ok.success {
            return Err(io::Error::other(
                "remounting the boot partition read-write failed",
            ));
        }
    }
    let result = (|| {
        let size = fs::metadata(&paths.source)?.len() as usize;
        let mut file = OpenOptions::new().write(true).open(&paths.source)?;
        file.write_all(&vec![0; size])?;
        file.sync_all()?;
        drop(file);
        fs::remove_file(&paths.source)
    })();
    if remount {
        let _ = runner.run(crate::init::SYNC, &[], Duration::from_secs(30));
        let _ = runner.run(
            "/bin/mount",
            &["-o", "remount,ro", &boot],
            Duration::from_secs(30),
        );
    }
    result
}

/// Applies the file; returns what was applied (no secrets). `gateway_owner` is the nibe-gateway account.
pub fn provision(
    paths: &ProvisionPaths,
    runner: &dyn Runner,
    gateway_owner: Owner,
    remount: bool,
) -> Result<String, String> {
    let name = paths
        .source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("nibe-provision.json");
    let text = fs::read(&paths.source).map_err(|e| format!("cannot read {name}: {}", e.kind()))?;
    let config: Value =
        serde_json::from_slice(&text).map_err(|_| format!("cannot read {name}: invalid JSON"))?;
    validate(&config)?;
    let map = config.as_object().cloned().unwrap_or_default();
    let mut applied = Vec::new();
    let keys: Vec<&str> = map
        .get("ssh_authorized_keys")
        .and_then(Value::as_array)
        .map(|k| k.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !keys.is_empty() {
        if let Some(dir) = paths.ssh_keys.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("ssh key directory: {}", e.kind()))?;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("ssh key directory: {}", e.kind()))?;
        }
        let body: String = keys.iter().map(|k| format!("{}\n", k.trim())).collect();
        write_private(&paths.ssh_keys, body.as_bytes(), None, 0o644)
            .map_err(|e| format!("writing ssh keys: {}", e.kind()))?;
        let keygen = runner
            .run("/usr/bin/ssh-keygen", &["-A"], Duration::from_secs(120))
            .map_err(|e| format!("ssh-keygen: {}", e.kind()))?;
        if !keygen.success {
            return Err("ssh-keygen -A failed".into());
        }
        applied.push(format!("{} ssh key(s)", keys.len()));
    }
    if let Some(key) = map.get("tailscale_auth_key").and_then(Value::as_str) {
        write_private(
            &paths.tailscale_dir.join("auth-key"),
            key.as_bytes(),
            None,
            0o600,
        )
        .map_err(|e| format!("writing tailscale key: {}", e.kind()))?;
        write_private(&paths.tailscale_dir.join("enabled"), b"", None, 0o600)
            .map_err(|e| format!("enabling tailscale: {}", e.kind()))?;
        applied.push("tailscale".into());
    }
    let gateway: Map<String, Value> = ["admin_password", "wifi"]
        .iter()
        .filter_map(|k| {
            map.get(*k)
                .filter(|v| !v.is_null())
                .map(|v| (k.to_string(), v.clone()))
        })
        .collect();
    if !gateway.is_empty() {
        let body = serde_json::to_vec(&gateway).map_err(|e| e.to_string())?;
        write_private(&paths.gateway_file, &body, gateway_owner, 0o600)
            .map_err(|e| format!("writing gateway settings: {}", e.kind()))?;
        applied.extend(gateway.keys().cloned());
    }
    remove_source(paths, runner, remount).map_err(|e| format!("removing {name}: {e}"))?;
    Ok(if applied.is_empty() {
        "nothing".into()
    } else {
        applied.join(", ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Output;
    use serde_json::json;
    use std::cell::RefCell;

    const KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGxvcmVtaXBzdW1kb2xvcnNpdGFtZXRjb25zZWN0ZXR1cg laptop";

    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
    }
    impl Runner for Fake {
        fn run(&self, program: &str, args: &[&str], _: Duration) -> io::Result<Output> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            Ok(Output {
                success: true,
                stdout: vec![],
            })
        }
    }
    fn paths(dir: &Path) -> ProvisionPaths {
        ProvisionPaths {
            boot: dir.join("boot"),
            source: dir.join("boot/nibe-provision.json"),
            ssh_keys: dir.join("etc/ssh/authorized_keys/nibe-admin"),
            tailscale_dir: dir.join("var/lib/tailscale"),
            gateway_file: dir.join("state/provision.json"),
        }
    }

    #[test]
    fn validation_matches_the_python_rules() {
        assert!(validate(&json!({})).is_ok());
        assert!(validate(&json!([])).is_err());
        assert!(validate(&json!({"extra": 1}))
            .unwrap_err()
            .contains("unknown keys"));
        assert!(validate(&json!({"wifi": {"ssid": ""}})).is_err());
        assert!(validate(&json!({"wifi": {"ssid": "x".repeat(33)}})).is_err());
        assert!(validate(&json!({"wifi": {"ssid": "home", "password": "p".repeat(65)}})).is_err());
        assert!(validate(&json!({"wifi": {"ssid": "home"}})).is_ok());
        assert!(validate(&json!({"admin_password": "short"})).is_err());
        assert!(validate(&json!({"ssh_authorized_keys": ["ssh-ed25519 notbase64!"]})).is_err());
        assert!(validate(&json!({"ssh_authorized_keys": "x"})).is_err());
        assert!(validate(&json!({"ssh_authorized_keys": [KEY]})).is_ok());
        assert!(validate(&json!({"tailscale_auth_key": "tskey-x"})).is_err());
        assert!(validate(&json!({"tailscale_auth_key": "tskey-auth-abc12345"})).is_ok());
    }

    #[test]
    fn applies_everything_then_wipes_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        fs::create_dir_all(&p.boot).unwrap();
        let secret = "a-long-admin-password";
        fs::write(
            &p.source,
            json!({"wifi": {"ssid": "home", "password": "wifi-secret"}, "admin_password": secret,
                   "ssh_authorized_keys": [KEY], "tailscale_auth_key": "tskey-auth-abc12345"})
            .to_string(),
        )
        .unwrap();
        let runner = Fake::default();
        let summary = provision(&p, &runner, None, true).unwrap();
        assert_eq!(summary, "1 ssh key(s), tailscale, admin_password, wifi");
        assert!(!summary.contains(secret));
        assert_eq!(fs::read_to_string(&p.ssh_keys).unwrap(), format!("{KEY}\n"));
        let mode = |f: &Path| fs::metadata(f).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&p.ssh_keys), 0o644);
        assert_eq!(mode(p.ssh_keys.parent().unwrap()), 0o755);
        assert_eq!(
            fs::read_to_string(p.tailscale_dir.join("auth-key")).unwrap(),
            "tskey-auth-abc12345"
        );
        assert_eq!(mode(&p.tailscale_dir.join("auth-key")), 0o600);
        assert!(p.tailscale_dir.join("enabled").exists());
        let gateway: Value = serde_json::from_slice(&fs::read(&p.gateway_file).unwrap()).unwrap();
        assert_eq!(
            gateway,
            json!({"admin_password": secret, "wifi": {"ssid": "home", "password": "wifi-secret"}})
        );
        assert_eq!(mode(&p.gateway_file), 0o600);
        assert!(!p.source.exists(), "source removed");
        let calls = runner.calls.borrow();
        assert_eq!(
            calls.first().map(String::as_str),
            Some("/usr/bin/ssh-keygen -A")
        );
        assert!(calls
            .iter()
            .any(|c| c.ends_with("remount,rw ".to_string().as_str()) || c.contains("remount,rw")));
        assert!(calls.last().unwrap().contains("remount,ro"));
    }

    #[test]
    fn invalid_files_are_left_untouched_and_nothing_is_applied() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        fs::create_dir_all(&p.boot).unwrap();
        fs::write(&p.source, r#"{"admin_password": "short"}"#).unwrap();
        let runner = Fake::default();
        assert!(provision(&p, &runner, None, true)
            .unwrap_err()
            .contains("12-256"));
        assert!(p.source.exists() && !p.gateway_file.exists());
        assert!(runner.calls.borrow().is_empty(), "no remount, no keygen");
        fs::write(&p.source, "not json").unwrap();
        assert!(provision(&p, &runner, None, true)
            .unwrap_err()
            .contains("invalid JSON"));
    }
}
