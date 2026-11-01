//! Root helper service. One connection at a time; each carries one bounded request.
use nibe_system::{serve_one, Helper, Paths, SystemRunner};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => {}
        // Wraps the gateway for systemd readiness/watchdog (replaces supervise.py); without
        // NOTIFY_SOCKET (OpenRC supervise-daemon) it enforces the health watchdog itself.
        Some("supervise") if args.len() > 1 => {
            let url = std::env::var("NIBE_HEALTH_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080/health".into());
            std::process::exit(nibe_system::supervise::supervise(&args[1..], &url));
        }
        // First-boot provisioning (replaces nibe-provision.py).
        Some("provision") => {
            let owner = gateway_owner();
            match nibe_system::provision::provision(
                &nibe_system::provision::ProvisionPaths::default(),
                &SystemRunner,
                owner,
                true,
            ) {
                Ok(applied) => {
                    println!("nibe-provision: applied {applied}");
                    return Ok(());
                }
                Err(message) => {
                    eprintln!("nibe-provision: {message}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!(
                "nibe-system serve [--socket PATH] | supervise COMMAND [ARGS...] | provision"
            );
            std::process::exit(2);
        }
    }
    let socket: PathBuf = args
        .iter()
        .position(|a| a == "--socket")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| "/run/nibe-system/control.sock".into());
    if socket.exists() {
        fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(&socket)?;
    // Group (nibe-gateway) comes from the unit's Group=; the gateway account may connect.
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o660))?;
    let helper = Helper {
        paths: Paths::default(),
        runner: SystemRunner,
    };
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = serve_one(&helper, &mut stream) {
                    eprintln!("nibe-system: request failed: {}", error.kind());
                }
            }
            Err(error) => eprintln!("nibe-system: accept failed: {}", error.kind()),
        }
    }
    Ok(())
}

/// uid/gid of the nibe-gateway account, which owns the provisioning hand-off file.
fn gateway_owner() -> Option<(u32, u32)> {
    let name = std::ffi::CString::new("nibe-gateway").ok()?;
    // SAFETY: getpwnam returns a pointer into static storage or null; read immediately, single-threaded.
    unsafe {
        let entry = libc::getpwnam(name.as_ptr());
        (!entry.is_null()).then(|| ((*entry).pw_uid, (*entry).pw_gid))
    }
}
