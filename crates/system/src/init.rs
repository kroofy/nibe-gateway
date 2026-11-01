//! Init-system abstraction: one build runs on Raspberry Pi OS (systemd) and Alpine (OpenRC).
//! Detection is a runtime check of `/run/systemd/system` (what `sd_booted()` checks). Every invocation
//! is a fixed absolute program with fixed arguments; no shell and no caller-supplied names.
use crate::Runner;
use std::{io, path::Path, time::Duration};

/// systemd creates this directory early in boot; it never exists under OpenRC.
pub const SYSTEMD_MARKER: &str = "/run/systemd/system";
pub const SYSTEMCTL: &str = "/usr/bin/systemctl";
/// OpenRC 0.63 (Alpine 3.24 `openrc` package) installs both into /sbin.
pub const RC_SERVICE: &str = "/sbin/rc-service";
pub const RC_UPDATE: &str = "/sbin/rc-update";
/// Busybox `reboot` applet on Alpine: signals init, which runs `openrc shutdown` and reboots.
pub const BUSYBOX_REBOOT: &str = "/sbin/reboot";
/// Busybox `sync`; on Raspberry Pi OS (merged /usr) the same path is coreutils.
pub const SYNC: &str = "/bin/sync";
/// OpenRC runlevel the image starts its services in.
pub const RUNLEVEL: &str = "default";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Init {
    Systemd,
    OpenRc,
}

/// The only services our binaries manage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Gateway,
    Ssh,
    Tailscale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Restart,
    /// Exit status 0 only while the service runs (`is-active` / `status`).
    IsActive,
    /// Start now and keep starting at boot. On systemd enablement follows the provisioning marker
    /// files (`ConditionPathExists=` drop-ins), so this is a plain `start`.
    /// On OpenRC the service is also added to the default runlevel.
    Enable,
    /// Stop and drop from the boot sequence; the mirror of [`Action::Enable`].
    Disable,
}

/// One fixed command. `required == false` steps may fail without failing the action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub program: &'static str,
    pub args: Vec<&'static str>,
    pub required: bool,
}
impl Invocation {
    fn new(program: &'static str, args: &[&'static str]) -> Self {
        Self {
            program,
            args: args.to_vec(),
            required: true,
        }
    }
    fn optional(mut self) -> Self {
        self.required = false;
        self
    }
    /// `program arg arg`, for logs and tests.
    pub fn display(&self) -> String {
        std::iter::once(self.program)
            .chain(self.args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Init {
    /// The running system's init.
    pub fn detect() -> Self {
        Self::detect_at(Path::new(SYSTEMD_MARKER))
    }
    /// Test seam: systemd if `marker` is a directory, otherwise OpenRC.
    pub fn detect_at(marker: &Path) -> Self {
        if marker.is_dir() {
            Self::Systemd
        } else {
            Self::OpenRc
        }
    }
    /// Unit or init-script name. Alpine names come from its packages: `/etc/init.d/sshd`
    /// (openssh-server-common-openrc) and `/etc/init.d/tailscale` (tailscale-openrc).
    pub fn name(self, service: Service) -> &'static str {
        match (self, service) {
            (Self::Systemd, Service::Gateway) => "nibe-gateway.service",
            (Self::Systemd, Service::Ssh) => "ssh.service",
            (Self::Systemd, Service::Tailscale) => "tailscaled.service",
            (Self::OpenRc, Service::Gateway) => "nibe-gateway",
            (Self::OpenRc, Service::Ssh) => "sshd",
            (Self::OpenRc, Service::Tailscale) => "tailscale",
        }
    }
    /// The commands for `action`, in order.
    pub fn commands(self, service: Service, action: Action) -> Vec<Invocation> {
        let name = self.name(service);
        match self {
            Self::Systemd => vec![match action {
                Action::Start | Action::Enable => Invocation::new(SYSTEMCTL, &["start", name]),
                Action::Stop | Action::Disable => Invocation::new(SYSTEMCTL, &["stop", name]),
                Action::Restart => Invocation::new(SYSTEMCTL, &["restart", name]),
                Action::IsActive => Invocation::new(SYSTEMCTL, &["is-active", "--quiet", name]),
            }],
            Self::OpenRc => {
                let rc = |verb| Invocation::new(RC_SERVICE, &[name, verb]);
                match action {
                    Action::Start => vec![rc("start")],
                    Action::Stop => vec![rc("stop")],
                    Action::Restart => vec![rc("restart")],
                    Action::IsActive => vec![rc("status")],
                    // `rc-update add` exits 0 when the service is already in the runlevel.
                    Action::Enable => vec![
                        Invocation::new(RC_UPDATE, &["add", name, RUNLEVEL]),
                        rc("start"),
                    ],
                    // `rc-update del` exits 1 when the service is not in the runlevel: that is
                    // the state we want, so it may fail.
                    Action::Disable => vec![
                        rc("stop"),
                        Invocation::new(RC_UPDATE, &["del", name, RUNLEVEL]).optional(),
                    ],
                }
            }
        }
    }
    /// Runs `commands(service, action)`, each bounded by `timeout`. `Ok(false)` when a required step
    /// exits non-zero (for `IsActive`: not running); later steps are skipped.
    pub fn run(
        self,
        runner: &dyn Runner,
        service: Service,
        action: Action,
        timeout: Duration,
    ) -> io::Result<bool> {
        for step in self.commands(service, action) {
            match runner.run(step.program, &step.args, timeout) {
                Ok(output) if output.success => {}
                Ok(_) if !step.required => {}
                Err(_) if !step.required => {}
                Ok(_) => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }
}

/// Runs `steps` like `Command::status` (inherited stdio, no timeout).
/// `Ok(false)` when a required step exits non-zero; later steps are skipped.
pub fn status(steps: &[Invocation]) -> io::Result<bool> {
    for step in steps {
        let ok = std::process::Command::new(step.program)
            .args(&step.args)
            .status()
            .map(|s| s.success());
        match ok {
            Ok(true) => {}
            Ok(false) | Err(_) if !step.required => {}
            Ok(false) => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

/// The kernel copies at most 255 bytes of a RESTART2 argument; NUL cannot be passed.
pub fn valid_reboot_argument(argument: &str) -> bool {
    !argument.is_empty() && argument.len() < 256 && !argument.contains('\0')
}

/// OpenRC reboot. Without an argument: busybox `reboot`, which asks init for an orderly
/// shutdown and returns at once. With one (`"0 tryboot"`): busybox cannot pass it, so a detached
/// child (own session, stdio on /dev/null) waits 2 s, stops the gateway (bounded by `stop_timeout`),
/// syncs and calls reboot(2) `LINUX_REBOOT_CMD_RESTART2`; the Raspberry Pi firmware driver reads
/// the argument. The caller returns `Ok` and exits normally first, as after `systemctl reboot`, so
/// `os-install` over SSH still reports success. Root only; call from a single-threaded process.
pub fn openrc_reboot(
    runner: &dyn Runner,
    argument: Option<&str>,
    stop_timeout: Duration,
) -> io::Result<()> {
    let Some(argument) = argument else {
        let ok = std::process::Command::new(BUSYBOX_REBOOT)
            .status()?
            .success();
        return if ok {
            Ok(())
        } else {
            Err(io::Error::other("reboot failed"))
        };
    };
    if !valid_reboot_argument(argument) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid reboot argument",
        ));
    }
    // SAFETY: geteuid(2) cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "reboot needs root",
        ));
    }
    // SAFETY: fork(2) from the single-threaded os-install path; the child only uses fresh
    // processes and threads of its own and leaves with _exit(2).
    match unsafe { libc::fork() } {
        -1 => Err(io::Error::last_os_error()),
        0 => {
            detach();
            std::thread::sleep(Duration::from_secs(2));
            let error = tryboot_now(runner, argument, stop_timeout);
            kmsg(&format!(
                "nibe-updater: reboot \"{argument}\" failed: {error}"
            ));
            // SAFETY: ends the forked child without running the parent's exit handlers.
            unsafe { libc::_exit(1) }
        }
        _ => Ok(()),
    }
}

/// New session (no SIGHUP when the SSH session ends), stdio on /dev/null.
fn detach() {
    // SAFETY: plain syscalls on our own process and descriptors.
    unsafe {
        libc::setsid();
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
        if null >= 0 {
            for fd in 0..3 {
                libc::dup2(null, fd);
            }
            if null > 2 {
                libc::close(null);
            }
        }
    }
}

/// One line to the kernel log (dmesg / syslog), the only place left after detaching.
fn kmsg(line: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open("/dev/kmsg") {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}

/// Stop the gateway, sync, RESTART2. Returns only if the kernel refused the reboot.
fn tryboot_now(runner: &dyn Runner, argument: &str, stop_timeout: Duration) -> io::Error {
    let openrc = Init::OpenRc;
    match openrc.run(runner, Service::Gateway, Action::Stop, stop_timeout) {
        Ok(true) => {}
        // A stuck stop must not keep the new slot from its try: reboot anyway.
        _ => kmsg("nibe-updater: rc-service nibe-gateway stop failed; rebooting anyway"),
    }
    let error = restart2(argument);
    // Still running: bring the gateway back rather than leave the Pi without it.
    let _ = openrc.run(runner, Service::Gateway, Action::Start, stop_timeout);
    error
}

/// sync(2), then reboot(2) with `LINUX_REBOOT_CMD_RESTART2`. Neither glibc's nor musl's `reboot()`
/// wrapper takes the argument (both are `reboot(int)`), so this is the raw syscall. Needs
/// CAP_SYS_BOOT; returns the error if the kernel refuses.
fn restart2(argument: &str) -> io::Error {
    let Ok(argument) = std::ffi::CString::new(argument) else {
        return io::Error::new(io::ErrorKind::InvalidInput, "NUL in reboot argument");
    };
    // SAFETY: sync(2) has no arguments; reboot(2) reads a NUL-terminated string we own for the
    // duration of the call. On success the call does not return.
    unsafe {
        libc::sync();
        libc::syscall(
            libc::SYS_reboot,
            libc::LINUX_REBOOT_MAGIC1,
            libc::LINUX_REBOOT_MAGIC2,
            libc::LINUX_REBOOT_CMD_RESTART2,
            argument.as_ptr(),
        );
    }
    io::Error::last_os_error()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Output;
    use std::cell::RefCell;

    #[test]
    fn detection_follows_the_systemd_runtime_directory() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("systemd/system");
        assert_eq!(Init::detect_at(&marker), Init::OpenRc);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, b"").unwrap();
        assert_eq!(
            Init::detect_at(&marker),
            Init::OpenRc,
            "a file is not enough"
        );
        std::fs::remove_file(&marker).unwrap();
        std::fs::create_dir(&marker).unwrap();
        assert_eq!(Init::detect_at(&marker), Init::Systemd);
    }

    fn lines(init: Init, service: Service, action: Action) -> Vec<String> {
        init.commands(service, action)
            .iter()
            .map(Invocation::display)
            .collect()
    }

    #[test]
    fn systemd_commands_are_unchanged() {
        use Action::*;
        let s = Init::Systemd;
        assert_eq!(
            lines(s, Service::Gateway, Restart),
            ["/usr/bin/systemctl restart nibe-gateway.service"]
        );
        assert_eq!(
            lines(s, Service::Ssh, Enable),
            ["/usr/bin/systemctl start ssh.service"]
        );
        assert_eq!(
            lines(s, Service::Ssh, Disable),
            ["/usr/bin/systemctl stop ssh.service"]
        );
        assert_eq!(
            lines(s, Service::Tailscale, Disable),
            ["/usr/bin/systemctl stop tailscaled.service"]
        );
        assert_eq!(
            lines(s, Service::Tailscale, IsActive),
            ["/usr/bin/systemctl is-active --quiet tailscaled.service"]
        );
    }

    #[test]
    fn openrc_commands_use_alpine_service_names() {
        use Action::*;
        let o = Init::OpenRc;
        assert_eq!(
            lines(o, Service::Gateway, Restart),
            ["/sbin/rc-service nibe-gateway restart"]
        );
        assert_eq!(
            lines(o, Service::Gateway, Stop),
            ["/sbin/rc-service nibe-gateway stop"]
        );
        assert_eq!(
            lines(o, Service::Ssh, IsActive),
            ["/sbin/rc-service sshd status"]
        );
        assert_eq!(
            lines(o, Service::Tailscale, Enable),
            [
                "/sbin/rc-update add tailscale default",
                "/sbin/rc-service tailscale start"
            ]
        );
        assert_eq!(
            lines(o, Service::Ssh, Disable),
            [
                "/sbin/rc-service sshd stop",
                "/sbin/rc-update del sshd default"
            ]
        );
        assert!(o
            .commands(Service::Ssh, Disable)
            .iter()
            .all(|c| c.required == (c.program == RC_SERVICE)));
    }

    #[test]
    fn reboot_arguments_are_bounded() {
        assert!(valid_reboot_argument("0 tryboot"));
        assert!(!valid_reboot_argument(""));
        assert!(!valid_reboot_argument("0\0tryboot"));
        assert!(!valid_reboot_argument(&"x".repeat(256)));
        let runner = failing("");
        let error = openrc_reboot(&runner, Some("a\0b"), Duration::from_secs(1)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(runner.calls.borrow().is_empty(), "nothing stopped");
    }

    #[test]
    fn status_runs_steps_with_inherited_stdio() {
        let ok = Invocation::new("/bin/true", &[]);
        let bad = Invocation::new("/bin/false", &[]);
        assert!(status(&[ok.clone(), bad.clone().optional(), ok.clone()]).unwrap());
        assert!(!status(&[bad, ok]).unwrap());
        assert!(status(&[Invocation::new("/nonexistent", &[])]).is_err());
    }

    struct Scripted {
        failing: &'static str,
        calls: RefCell<Vec<String>>,
    }
    impl Runner for Scripted {
        fn run(&self, program: &str, args: &[&str], _: Duration) -> io::Result<Output> {
            let line = format!("{program} {}", args.join(" "));
            let success = line != self.failing;
            self.calls.borrow_mut().push(line);
            Ok(Output {
                success,
                stdout: vec![],
            })
        }
    }
    fn failing(line: &'static str) -> Scripted {
        Scripted {
            failing: line,
            calls: RefCell::default(),
        }
    }

    #[test]
    fn optional_steps_may_fail_and_required_ones_stop_the_sequence() {
        let second = Duration::from_secs(1);
        let runner = failing("/sbin/rc-update del sshd default");
        assert!(Init::OpenRc
            .run(&runner, Service::Ssh, Action::Disable, second)
            .unwrap());
        assert_eq!(runner.calls.borrow().len(), 2);
        let runner = failing("/sbin/rc-update add tailscale default");
        assert!(!Init::OpenRc
            .run(&runner, Service::Tailscale, Action::Enable, second)
            .unwrap());
        assert_eq!(
            *runner.calls.borrow(),
            ["/sbin/rc-update add tailscale default"],
            "start is skipped after a failed enable"
        );
        let runner = failing("/sbin/rc-service sshd status");
        assert!(!Init::OpenRc
            .run(&runner, Service::Ssh, Action::IsActive, second)
            .unwrap());
    }
}
