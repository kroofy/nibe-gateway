use nibe_system::init::{self, Action as InitAction, Init, Service};
use nibe_updater::{
    artifact::PI_TARGET,
    rpc::{self, Action},
    service::InitService,
    NoFault, Result, ServiceManager, Updater,
};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");
    fn value(args: &[String], name: &str, default: &str) -> String {
        args.windows(2)
            .find(|a| a[0] == name)
            .map(|a| a[1].clone())
            .unwrap_or(default.into())
    }
    let socket = PathBuf::from(value(&args, "--socket", "/run/nibe-updater/control.sock"));
    if let Some(os_command) = command.strip_prefix("os-") {
        return os(os_command, &args, &value);
    }
    if command == "client" {
        let action = match args.get(1).map(String::as_str) {
            Some("status") => Action::Status,
            Some("upload") if args.iter().any(|a| a == "--confirm-downgrade") => {
                Action::UploadDowngrade
            }
            Some("upload") => Action::Upload,
            Some("activate") => Action::Activate,
            Some("rollback") => Action::Rollback,
            Some("restart") => Action::Restart,
            _ => return Err(nibe_updater::fail("invalid client action")),
        };
        let bytes = if matches!(action, Action::Upload | Action::UploadDowngrade) {
            fs::read(
                args.get(2)
                    .ok_or_else(|| nibe_updater::fail("artifact path required"))?,
            )?
        } else {
            vec![]
        };
        let reply = rpc::request(&socket, action, &bytes)?;
        println!("{}", serde_json::to_string(&reply)?);
        if !reply.ok {
            return Err(nibe_updater::fail("updater rejected request"));
        }
        return Ok(());
    }
    if command != "serve" && command != "recover" {
        println!("nibe-updater os-install BUNDLE [--no-reboot] | os-status | os-confirm [--probation-seconds 600] [--key PUBLIC_KEY]\nnibe-updater serve|recover [--root DIR --key PUBLIC_KEY --socket PATH --target TARGET --health-address 127.0.0.1:8080 --probation-seconds 600 --probation-max-restarts 2]\nnibe-updater client status|upload FILE|activate|rollback|restart [--socket PATH] [--confirm-downgrade]");
        return Ok(());
    }
    let root = PathBuf::from(value(&args, "--root", "/var/lib/nibe-gateway"));
    let public: [u8; 32] = fs::read(value(&args, "--key", "/etc/nibe-gateway/release.pub"))?
        .try_into()
        .map_err(|_| nibe_updater::fail("public key must be 32 bytes"))?;
    let address =
        value(&args, "--health-address", "127.0.0.1:8080").parse::<std::net::SocketAddr>()?;
    if !address.ip().is_loopback() {
        return Err(nibe_updater::fail("health must use loopback"));
    }
    let target = value(&args, "--target", PI_TARGET);
    let helper = args
        .windows(2)
        .find(|a| a[0] == "--service-helper")
        .map(|a| PathBuf::from(&a[1]));
    if helper.as_ref().is_some_and(|p| !p.is_absolute()) {
        return Err(nibe_updater::fail("helper path must be absolute"));
    }
    let probation_seconds: u64 = value(&args, "--probation-seconds", "600").parse()?;
    let max_restarts: u32 = value(&args, "--probation-max-restarts", "2").parse()?;
    if probation_seconds > 86400 || !(1..=20).contains(&max_restarts) {
        return Err(nibe_updater::fail(
            "probation must be <=86400 seconds; restart limit must be 1..20",
        ));
    }
    let init = Init::detect();
    let service_factory = move || -> Box<dyn ServiceManager> {
        let health = InitService {
            init,
            address,
            timeout: Duration::from_secs(30),
            probation: Duration::from_secs(probation_seconds),
            max_restarts,
        };
        match &helper {
            Some(executable) => Box::new(nibe_updater::service::Helper {
                executable: executable.clone(),
                health,
            }),
            None => Box::new(health),
        }
    };
    let mut updater = Updater::open(&root, public, &target)?;
    let mut service = service_factory();
    // Recovery runs before any new operation. Do not silently reset malformed state.
    updater.recover(service.as_mut())?;
    if command == "recover" {
        return Ok(());
    }
    if socket.exists() {
        fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o660))?;
    let updater = Arc::new(Mutex::new(updater));
    // Bound sockets/threads; take the mutation lock before reading upload bytes
    // so at most one 48-MiB payload is allocated, alongside a status client.
    let clients = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for stream in listener.incoming() {
        let mut stream = stream?;
        use std::sync::atomic::Ordering;
        if clients.fetch_add(1, Ordering::SeqCst) >= 2 {
            clients.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let clients = clients.clone();
        let updater = updater.clone();
        let root = root.clone();
        let service_factory = service_factory.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let action = rpc::read_action(&mut stream)?;
                if matches!(action, Action::Status) {
                    let state: serde_json::Value =
                        serde_json::from_slice(&fs::read(root.join("update-state/state.json"))?)?;
                    return rpc::reply(&mut stream, true, state);
                }
                // Single admitted mutation. No queue of privileged restarts/uploads.
                let mut updater = match updater.try_lock() {
                    Ok(u) => u,
                    Err(_) => {
                        return rpc::reply(
                            &mut stream,
                            false,
                            serde_json::json!({"outcome":"busy"}),
                        )
                    }
                };
                let mut service = service_factory();
                if matches!(action, Action::Upload | Action::UploadDowngrade) {
                    let bytes = rpc::read_upload(&mut stream)?;
                    let ok = updater
                        .stage_confirmed(
                            &bytes,
                            matches!(action, Action::UploadDowngrade),
                            &mut NoFault,
                        )
                        .is_ok();
                    if !ok {
                        let _ = updater.recover(service.as_mut());
                    }
                    let state = serde_json::to_value(&updater.state)?;
                    // A client may send its next request, such as activate, as soon as it has the reply.
                    drop(updater);
                    return rpc::reply(&mut stream, ok, state);
                }
                // Ack durable intent before restarting the calling gateway. The
                // independent daemon completes even after its client disappears.
                match action {
                    Action::Activate => updater.prepare_activate()?,
                    Action::Rollback => updater.prepare_rollback()?,
                    _ => {}
                }
                let acknowledgement =
                    rpc::reply(&mut stream, true, serde_json::json!({"outcome":"accepted"}));
                match action {
                    Action::Activate => updater.finish_activate(service.as_mut(), &mut NoFault)?,
                    Action::Rollback => updater.finish_rollback(service.as_mut(), &mut NoFault)?,
                    Action::Restart => service.restart()?,
                    _ => {}
                }
                acknowledgement
            })();
            clients.fetch_sub(1, Ordering::SeqCst);
            if result.is_err() {
                let _ = rpc::reply(
                    &mut stream,
                    false,
                    serde_json::json!({"outcome":"operation_failed"}),
                );
            }
        });
    }
    Ok(())
}

/// Full-system A/B commands. Root only; paths are fixed, never client-supplied.
fn os(
    command: &str,
    args: &[String],
    value: &dyn Fn(&[String], &str, &str) -> String,
) -> Result<()> {
    use nibe_updater::os::{OsPaths, OsUpdater};
    let public: [u8; 32] = fs::read(value(args, "--key", "/etc/nibe-gateway/release.pub"))?
        .try_into()
        .map_err(|_| nibe_updater::fail("public key must be 32 bytes"))?;
    let updater = OsUpdater {
        paths: OsPaths::default(),
        public,
    };
    let init = Init::detect();
    let reboot = |argument: Option<&str>| -> Result<()> {
        if init == Init::OpenRc {
            // No systemctl: busybox reboot, or reboot(2) RESTART2 for tryboot.
            return init::openrc_reboot(
                &nibe_system::SystemRunner,
                argument,
                Duration::from_secs(60),
            )
            .map_err(|e| nibe_updater::fail(&format!("reboot failed: {e}")));
        }
        let mut command = std::process::Command::new("/usr/bin/systemctl");
        command.arg("reboot");
        // systemd 252 deprecates the positional reboot argument.
        command.args(argument.map(|a| format!("--reboot-argument={a}")));
        if !command.status()?.success() {
            return Err(nibe_updater::fail("systemctl reboot failed"));
        }
        Ok(())
    };
    match command {
        "status" => println!("{}", serde_json::to_string(&updater.status()?)?),
        "install" => {
            let bundle = args
                .get(1)
                .filter(|a| !a.starts_with("--"))
                .ok_or_else(|| nibe_updater::fail("bundle path required"))?;
            let slot = updater.install(std::path::Path::new(bundle))?;
            println!("{}", serde_json::to_string(&updater.status()?)?);
            if !args.iter().any(|a| a == "--no-reboot") {
                eprintln!("rebooting once into {} (tryboot)", slot.name());
                reboot(Some("0 tryboot"))?;
            }
        }
        "confirm" => {
            let seconds: u64 = value(args, "--probation-seconds", "600").parse()?;
            if seconds > 86400 {
                return Err(nibe_updater::fail("probation must be <=86400 seconds"));
            }
            // NIBE_HEALTH_URL=http://127.0.0.1:8080/health (gateway unit); loopback only.
            let url = std::env::var("NIBE_HEALTH_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080/health".into());
            let address: std::net::SocketAddr = url
                .strip_prefix("http://")
                .and_then(|rest| rest.split('/').next())
                .ok_or_else(|| nibe_updater::fail("NIBE_HEALTH_URL must be http://IP:PORT/health"))?
                .parse()?;
            if !address.ip().is_loopback() {
                return Err(nibe_updater::fail("health must use loopback"));
            }
            let state = updater.confirm(
                &mut || {
                    Ok(nibe_updater::service::os_probation(
                        address,
                        Duration::from_secs(seconds),
                        Duration::from_secs(120),
                    )? && remote_access_healthy(init))
                },
                &mut || reboot(None),
            )?;
            println!("{}", serde_json::to_string(&state)?);
        }
        _ => return Err(nibe_updater::fail("unknown os command")),
    }
    Ok(())
}
/// A new OS must keep the owner's ways in, not only a healthy gateway: committing a slot that broke
/// SSH or Tailscale would strand a Pi that sits inside a heat pump. Checked only for the
/// paths that were provisioned.
/// `systemctl is-active --quiet ssh.service|tailscaled.service` or `rc-service sshd|tailscale status`.
fn remote_access_healthy(init: Init) -> bool {
    let active = |service: Service| {
        init::status(&init.commands(service, InitAction::IsActive)).unwrap_or(false)
    };
    let ssh = !std::path::Path::new("/etc/ssh/authorized_keys/nibe-admin").exists()
        || active(Service::Ssh);
    let tailscale =
        !std::path::Path::new("/var/lib/tailscale/enabled").exists() || active(Service::Tailscale);
    if !ssh || !tailscale {
        eprintln!("os-confirm: remote access unhealthy (ssh ok: {ssh}, tailscale ok: {tailscale})");
    }
    ssh && tailscale
}
