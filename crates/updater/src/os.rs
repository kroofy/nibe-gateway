//! Full-system A/B updates (ADR 0106). Bundle: uncompressed tar with, in order, `manifest.json`,
//! `manifest.sig` (raw Ed25519 over the exact manifest bytes), `boot.tar` (one slot's kernel,
//! initramfs, dtbs, overlays and cmdline template) and `rootfs.img` (raw ext4 or squashfs, ADR 0117; the
//! filesystem must match the template's `rootfstype=` when it names one). Install streams the
//! root image to the inactive partition and arms a one-shot `tryboot`; confirm commits `config.txt`
//! only after the tried slot stayed healthy. Shared firmware files are never touched.
use crate::{atomic, fail, sync_dir, Result, Slot};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    process::Command,
};

pub const OS_COMPAT: &str = "nibe-gateway-os-v1";
/// `config.txt` comment line; the line after it must be `os_prefix=slot-a/` or `os_prefix=slot-b/`.
/// (config.txt has no inline comments, so the marker sits on its own line.)
pub const MARKER: &str = "# nibe-slot (managed by nibe-updater; do not edit)";
const MAX_MANIFEST: u64 = 64 * 1024;
const MAX_BOOT: u64 = 256 * 1024 * 1024;
const BUFFER: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OsManifest {
    pub format: u32,
    pub kind: String,
    pub compatibility: String,
    pub version: String,
    pub rootfs_size: u64,
    pub rootfs_sha256: String,
    pub boot_sha256: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OsPhase {
    Idle,
    Trying,
    Committed,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OsState {
    pub phase: OsPhase,
    pub from: Option<Slot>,
    pub to: Option<Slot>,
    pub version: Option<String>,
    /// Unix seconds of the last transition.
    pub at: u64,
    pub detail: Option<String>,
}
impl Default for OsState {
    fn default() -> Self {
        Self {
            phase: OsPhase::Idle,
            from: None,
            to: None,
            version: None,
            at: 0,
            detail: None,
        }
    }
}
/// Host paths; tests point them into a temp dir.
#[derive(Debug, Clone)]
pub struct OsPaths {
    pub cmdline: PathBuf,
    pub boot: PathBuf,
    pub by_partuuid: PathBuf,
    pub state_dir: PathBuf,
    /// Remount the boot partition rw around writes (false in tests).
    pub remount: bool,
}
impl Default for OsPaths {
    fn default() -> Self {
        Self {
            cmdline: "/proc/cmdline".into(),
            boot: "/boot/firmware".into(),
            by_partuuid: "/dev/disk/by-partuuid".into(),
            state_dir: "/var/lib/nibe-gateway/update-state".into(),
            remount: true,
        }
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
fn letter(slot: Slot) -> &'static str {
    match slot {
        Slot::A => "a",
        Slot::B => "b",
    }
}
fn partition(slot: Slot) -> &'static str {
    match slot {
        Slot::A => "02",
        Slot::B => "04",
    }
}
/// Running slot and disk identifier from the kernel command line
/// (`nibe.slot=a|b`, `root=PARTUUID=<disk>-02|04`); both must agree.
pub fn running(cmdline: &str) -> Result<(Slot, String)> {
    let token = |key: &str| {
        cmdline
            .split_whitespace()
            .find_map(|t| t.strip_prefix(key).map(str::to_owned))
    };
    let slot = match token("nibe.slot=").as_deref() {
        Some("a") => Slot::A,
        Some("b") => Slot::B,
        _ => return Err(fail("cmdline lacks nibe.slot=a|b")),
    };
    let root = token("root=PARTUUID=").ok_or_else(|| fail("cmdline lacks root=PARTUUID="))?;
    let (disk, part) = root
        .rsplit_once('-')
        .ok_or_else(|| fail("unexpected root PARTUUID"))?;
    if part != partition(slot) || disk.len() != 8 || !disk.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(fail("nibe.slot and root partition disagree"));
    }
    Ok((slot, disk.to_owned()))
}
/// Replace the root and slot tokens of a cmdline template for `slot`.
pub fn slot_cmdline(template: &str, disk: &str, slot: Slot) -> String {
    let mut tokens: Vec<String> = template
        .split_whitespace()
        .filter(|t| !t.starts_with("root=") && !t.starts_with("nibe.slot="))
        .map(str::to_owned)
        .collect();
    tokens.push(format!("root=PARTUUID={disk}-{}", partition(slot)));
    tokens.push(format!("nibe.slot={}", letter(slot)));
    tokens.join(" ") + "\n"
}
/// `config.txt` with the managed `os_prefix` line pointing at `slot`.
pub fn select_slot(config: &str, slot: Slot) -> Result<String> {
    let mut lines: Vec<String> = config.lines().map(str::to_owned).collect();
    let at = lines
        .iter()
        .position(|l| l.trim() == MARKER)
        .ok_or_else(|| fail("config.txt lacks the nibe-slot marker"))?;
    match lines.get_mut(at + 1) {
        Some(line) if line.trim_start().starts_with("os_prefix=") => {
            *line = format!("os_prefix={}/", Slot::name(slot));
        }
        _ => return Err(fail("nibe-slot marker is not followed by os_prefix=")),
    }
    Ok(lines.join("\n") + "\n")
}
/// Bytes of a root image needed to recognise its filesystem.
const ROOT_HEAD: usize = 4096;
/// Filesystem of a root image from its first bytes: squashfs magic `hsqs` at 0, ext2/3/4 magic 0xEF53 at 1080.
pub fn root_image_type(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"hsqs") {
        Some("squashfs")
    } else if head.get(1080..1082) == Some(&[0x53, 0xEF][..]) {
        Some("ext4")
    } else {
        None
    }
}
/// A slot boots only if its cmdline's `rootfstype=` (when present) names the filesystem of its root image:
/// the initramfs mounts the root with exactly that type (ADR 0117, mixed ext4/squashfs slots).
pub fn check_root_type(cmdline: &str, head: &[u8]) -> Result<()> {
    let declared = cmdline
        .split_whitespace()
        .find_map(|t| t.strip_prefix("rootfstype="));
    let Some(declared) = declared else {
        return Ok(());
    };
    match root_image_type(head) {
        Some(found) if found == declared => Ok(()),
        Some(found) => Err(fail(&format!(
            "cmdline says rootfstype={declared} but the root image is {found}"
        ))),
        None => Err(fail(&format!(
            "cmdline says rootfstype={declared} but the root image is neither ext4 nor squashfs"
        ))),
    }
}
fn read_head(from: &mut impl Read) -> Result<Vec<u8>> {
    let mut head = Vec::with_capacity(ROOT_HEAD);
    from.take(ROOT_HEAD as u64).read_to_end(&mut head)?;
    Ok(head)
}
/// The cmdline template inside a packed boot tar.
fn boot_cmdline(boot_tar: &mut impl Read) -> Result<String> {
    let mut archive = tar::Archive::new(boot_tar);
    for entry in archive.entries()? {
        let entry = entry?;
        if entry.path()?.as_os_str() == "cmdline.txt" {
            let mut text = String::new();
            entry.take(MAX_MANIFEST).read_to_string(&mut text)?;
            return Ok(text);
        }
    }
    Err(fail("boot files lack cmdline.txt"))
}
fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; BUFFER];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total += n as u64;
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}
fn append_file(builder: &mut tar::Builder<impl Write>, name: &str, path: &Path) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(fs::metadata(path)?.len());
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, name, File::open(path)?)?;
    Ok(())
}
/// Pack a slot boot directory (regular files and directories only) into a tar.
pub fn pack_boot(dir: &Path) -> Result<Vec<u8>> {
    fn walk(builder: &mut tar::Builder<Vec<u8>>, base: &Path, path: &Path) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let kind = entry.file_type()?;
            let name = entry
                .path()
                .strip_prefix(base)?
                .to_str()
                .ok_or_else(|| fail("invalid filename"))?
                .to_owned();
            if kind.is_symlink() {
                return Err(fail("symlinks forbidden in boot files"));
            } else if kind.is_dir() {
                walk(builder, base, &entry.path())?;
            } else {
                append_file(builder, &name, &entry.path())?;
            }
        }
        Ok(())
    }
    let mut builder = tar::Builder::new(Vec::new());
    walk(&mut builder, dir, dir)?;
    if !dir.join("cmdline.txt").is_file() {
        return Err(fail("boot files need a cmdline.txt template"));
    }
    Ok(builder.into_inner()?)
}
/// Build and sign an OS bundle. The root image is streamed, never loaded whole.
pub fn sign_bundle(
    secret: &[u8; 32],
    version: &str,
    rootfs: &Path,
    boot_dir: &Path,
    out: &Path,
) -> Result<OsManifest> {
    semver::Version::parse(version)?;
    check_root_type(
        &fs::read_to_string(boot_dir.join("cmdline.txt"))?,
        &read_head(&mut File::open(rootfs)?)?,
    )?;
    write_bundle(secret, version, rootfs, boot_dir, out)
}
fn write_bundle(
    secret: &[u8; 32],
    version: &str,
    rootfs: &Path,
    boot_dir: &Path,
    out: &Path,
) -> Result<OsManifest> {
    let boot = pack_boot(boot_dir)?;
    let (rootfs_size, rootfs_sha256) = hash_file(rootfs)?;
    let manifest = OsManifest {
        format: 1,
        kind: "os".into(),
        compatibility: OS_COMPAT.into(),
        version: version.into(),
        rootfs_size,
        rootfs_sha256,
        boot_sha256: crate::artifact::digest(&boot),
    };
    let bytes = serde_json::to_vec(&manifest)?;
    let signature = SigningKey::from_bytes(secret).sign(&bytes);
    let file = OpenOptions::new().create_new(true).write(true).open(out)?;
    let mut builder = tar::Builder::new(io::BufWriter::new(file));
    for (name, data) in [
        ("manifest.json", bytes.as_slice()),
        ("manifest.sig", &signature.to_bytes()[..]),
        ("boot.tar", boot.as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, data)?;
    }
    append_file(&mut builder, "rootfs.img", rootfs)?;
    builder.into_inner()?.into_inner()?.sync_all()?;
    Ok(manifest)
}

pub struct OsUpdater {
    pub paths: OsPaths,
    pub public: [u8; 32],
}
impl OsUpdater {
    fn state_path(&self) -> PathBuf {
        self.paths.state_dir.join("os-state.json")
    }
    pub fn state(&self) -> Result<OsState> {
        match fs::read(self.state_path()) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(OsState::default()),
            Err(e) => Err(e.into()),
        }
    }
    fn save(&self, state: &OsState) -> Result<()> {
        fs::create_dir_all(&self.paths.state_dir)?;
        atomic(&self.state_path(), &serde_json::to_vec(state)?, 0o600)
    }
    fn running(&self) -> Result<(Slot, String)> {
        running(&fs::read_to_string(&self.paths.cmdline)?)
    }
    pub fn status(&self) -> Result<serde_json::Value> {
        let running = self.running().ok().map(|(slot, _)| slot);
        Ok(serde_json::json!({"running": running, "state": self.state()?}))
    }
    fn boot_writable<T>(&self, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let remount = |mode: &str| -> Result<()> {
            if self.paths.remount
                && !Command::new("/bin/mount")
                    .args(["-o", &format!("remount,{mode}")])
                    .arg(&self.paths.boot)
                    .status()?
                    .success()
            {
                return Err(fail("cannot remount the boot partition"));
            }
            Ok(())
        };
        remount("rw")?;
        let result = work();
        let restored = remount("ro");
        let value = result?;
        restored?;
        Ok(value)
    }
    /// Verify, write the inactive slot and arm tryboot. Returns the slot to try.
    pub fn install(&self, bundle: &Path) -> Result<Slot> {
        if self.state()?.phase == OsPhase::Trying {
            return Err(fail(
                "an OS update is already being tried; run os-confirm first",
            ));
        }
        let (from, disk) = self.running()?;
        let to = from.other();
        let device = self
            .paths
            .by_partuuid
            .join(format!("{disk}-{}", partition(to)));
        let mut archive = tar::Archive::new(File::open(bundle)?);
        let mut entries = archive.entries()?;
        let mut next = |name: &str, limit: u64| -> Result<tar::Entry<'_, File>> {
            let entry = entries
                .next()
                .ok_or_else(|| fail("truncated OS bundle"))??;
            if entry.path()?.as_os_str() != name
                || !entry.header().entry_type().is_file()
                || entry.size() > limit
            {
                return Err(fail("unexpected OS bundle entry"));
            }
            Ok(entry)
        };
        let mut manifest_bytes = Vec::new();
        next("manifest.json", MAX_MANIFEST)?.read_to_end(&mut manifest_bytes)?;
        let mut signature = Vec::new();
        next("manifest.sig", 64)?.read_to_end(&mut signature)?;
        VerifyingKey::from_bytes(&self.public)?
            .verify_strict(&manifest_bytes, &Signature::from_slice(&signature)?)?;
        let manifest: OsManifest = serde_json::from_slice(&manifest_bytes)?;
        semver::Version::parse(&manifest.version)?;
        if manifest.format != 1 || manifest.kind != "os" || manifest.compatibility != OS_COMPAT {
            return Err(fail("incompatible OS bundle"));
        }
        let failed = |detail: &str| {
            let _ = self.save(&OsState {
                phase: OsPhase::Failed,
                from: Some(from),
                to: Some(to),
                version: Some(manifest.version.clone()),
                at: now(),
                detail: Some(detail.into()),
            });
            fail(detail)
        };
        // Refuse before touching anything if the boot config cannot select a slot.
        if let Err(e) = fs::read_to_string(self.paths.boot.join("config.txt"))
            .map_err(Into::into)
            .and_then(|c| select_slot(&c, to))
        {
            return Err(failed(&format!("config.txt cannot select a slot: {e}")));
        }
        // Boot files are staged on the data partition, never in RAM or tmpfs.
        fs::create_dir_all(&self.paths.state_dir)?;
        let staged = self.paths.state_dir.join("os-boot.tar");
        let boot_hash = copy_hashed(
            &mut next("boot.tar", MAX_BOOT)?,
            &mut OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&staged)?,
            None,
        )?;
        if boot_hash.1 != manifest.boot_sha256 {
            let _ = fs::remove_file(&staged);
            return Err(failed("boot files checksum mismatch"));
        }
        let mut rootfs = next("rootfs.img", manifest.rootfs_size)?;
        if rootfs.size() != manifest.rootfs_size {
            return Err(failed("root image size mismatch"));
        }
        // Never write a slot whose initramfs would mount the root with the wrong filesystem type.
        let head = read_head(&mut rootfs)?;
        if let Err(e) =
            boot_cmdline(&mut File::open(&staged)?).and_then(|c| check_root_type(&c, &head))
        {
            let _ = fs::remove_file(&staged);
            return Err(failed(&e.to_string()));
        }
        let mut rootfs = io::Cursor::new(head).chain(rootfs);
        let mut target = OpenOptions::new().write(true).open(&device)?;
        if target.seek(SeekFrom::End(0))? < manifest.rootfs_size {
            return Err(failed("inactive partition is smaller than the root image"));
        }
        target.seek(SeekFrom::Start(0))?;
        let written = copy_hashed(&mut rootfs, &mut target, Some(manifest.rootfs_size))?;
        target.sync_all()?;
        if written != (manifest.rootfs_size, manifest.rootfs_sha256.clone()) {
            return Err(failed("root image checksum mismatch"));
        }
        self.boot_writable(|| {
            let slot_dir = self.paths.boot.join(to.name());
            let fresh = self.paths.boot.join(format!(".{}.new", to.name()));
            let old = self.paths.boot.join(format!(".{}.old", to.name()));
            for dir in [&fresh, &old] {
                if dir.exists() {
                    fs::remove_dir_all(dir)?;
                }
            }
            extract_boot(&staged, &fresh)?;
            let cmdline = fs::read_to_string(fresh.join("cmdline.txt"))?;
            write_synced(
                &fresh.join("cmdline.txt"),
                slot_cmdline(&cmdline, &disk, to).as_bytes(),
            )?;
            if slot_dir.exists() {
                fs::rename(&slot_dir, &old)?;
            }
            fs::rename(&fresh, &slot_dir)?;
            if old.exists() {
                fs::remove_dir_all(&old)?;
            }
            let config = fs::read_to_string(self.paths.boot.join("config.txt"))?;
            write_synced(
                &self.paths.boot.join("tryboot.txt"),
                select_slot(&config, to)?.as_bytes(),
            )?;
            sync_dir(&self.paths.boot)
        })
        .map_err(|e| failed(&format!("boot partition update failed: {e}")))?;
        let _ = fs::remove_file(&staged);
        self.save(&OsState {
            phase: OsPhase::Trying,
            from: Some(from),
            to: Some(to),
            version: Some(manifest.version),
            at: now(),
            detail: None,
        })?;
        Ok(to)
    }
    /// After a boot: commit the tried slot if it runs and `healthy()` holds, otherwise fail it.
    /// `reboot()` is called when the tried slot is running but unhealthy (normal boot = old slot).
    pub fn confirm(
        &self,
        healthy: &mut dyn FnMut() -> Result<bool>,
        reboot: &mut dyn FnMut() -> Result<()>,
    ) -> Result<OsState> {
        let mut state = self.state()?;
        if state.phase != OsPhase::Trying {
            return Ok(state);
        }
        let (running, _) = self.running()?;
        let clear_tryboot = || -> Result<()> {
            self.boot_writable(|| {
                match fs::remove_file(self.paths.boot.join("tryboot.txt")) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
                    _ => {}
                }
                sync_dir(&self.paths.boot)
            })
        };
        state.at = now();
        if Some(running) == state.from {
            state.phase = OsPhase::Failed;
            state.detail = Some("tried slot did not boot; still on the previous slot".into());
            clear_tryboot()?;
            self.save(&state)?;
            return Ok(state);
        }
        if Some(running) != state.to {
            return Err(fail("running slot matches neither side of the OS update"));
        }
        if healthy()? {
            self.boot_writable(|| {
                let path = self.paths.boot.join("config.txt");
                let config = select_slot(&fs::read_to_string(&path)?, running)?;
                let fresh = self.paths.boot.join("config.txt.new");
                write_synced(&fresh, config.as_bytes())?;
                fs::rename(&fresh, &path)?;
                sync_dir(&self.paths.boot)
            })?;
            clear_tryboot()?;
            state.phase = OsPhase::Committed;
            state.detail = None;
            self.save(&state)?;
        } else {
            state.phase = OsPhase::Failed;
            state.detail = Some("tried slot was unhealthy during probation".into());
            clear_tryboot()?;
            self.save(&state)?;
            reboot()?;
        }
        Ok(state)
    }
}
fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
/// Copies through a fixed buffer while hashing; `exact` enforces the byte count.
fn copy_hashed(
    from: &mut impl Read,
    to: &mut impl Write,
    exact: Option<u64>,
) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; BUFFER];
    let mut total = 0u64;
    loop {
        let n = from.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        to.write_all(&buffer[..n])?;
        total += n as u64;
        if exact.is_some_and(|limit| total > limit) {
            return Err(fail("stream longer than declared"));
        }
    }
    to.flush()?;
    Ok((total, format!("{:x}", hasher.finalize())))
}
/// Extracts regular files and directories only, with plain relative paths.
fn extract_boot(tar_path: &Path, into: &Path) -> Result<()> {
    fs::create_dir_all(into)?;
    let mut archive = tar::Archive::new(File::open(tar_path)?);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(fail("unsafe boot file path"));
        }
        let target = into.join(&path);
        match entry.header().entry_type() {
            t if t.is_dir() => fs::create_dir_all(&target)?,
            t if t.is_file() => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out = File::create(&target)?;
                io::copy(&mut entry, &mut out)?;
                out.sync_all()?;
            }
            _ => return Err(fail("only regular boot files are allowed")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const DISK: &str = "abcd1234";
    struct Rig {
        dir: tempfile::TempDir,
        secret: [u8; 32],
        os: OsUpdater,
    }
    fn rig(slot: Slot) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        for sub in ["boot/slot-a", "dev", "state", "src/boot/overlays"] {
            fs::create_dir_all(p.join(sub)).unwrap();
        }
        fs::write(
            p.join("boot/config.txt"),
            format!("[all]\nenable_uart=1\n{MARKER}\nos_prefix=slot-a/\n"),
        )
        .unwrap();
        fs::write(p.join("boot/slot-a/kernel.img"), b"old kernel").unwrap();
        fs::write(
            p.join("cmdline"),
            format!(
                "console=tty1 root=PARTUUID={DISK}-{} rootwait nibe.slot={} nibe.overlay=1\n",
                partition(slot),
                letter(slot)
            ),
        )
        .unwrap();
        for part in ["02", "04"] {
            fs::write(p.join(format!("dev/{DISK}-{part}")), vec![0u8; 8192]).unwrap();
        }
        fs::write(p.join("src/boot/kernel.img"), b"new kernel").unwrap();
        fs::write(p.join("src/boot/overlays/disable-bt.dtbo"), b"dtbo").unwrap();
        fs::write(
            p.join("src/boot/cmdline.txt"),
            "console=tty1 root=PARTUUID=ffff-02 rootwait nibe.overlay=1\n",
        )
        .unwrap();
        fs::write(p.join("src/rootfs.img"), vec![7u8; 5000]).unwrap();
        let secret = [9u8; 32];
        let public = SigningKey::from_bytes(&secret).verifying_key().to_bytes();
        let os = OsUpdater {
            paths: OsPaths {
                cmdline: p.join("cmdline"),
                boot: p.join("boot"),
                by_partuuid: p.join("dev"),
                state_dir: p.join("state"),
                remount: false,
            },
            public,
        };
        Rig { dir, secret, os }
    }
    impl Rig {
        fn path(&self, sub: &str) -> PathBuf {
            self.dir.path().join(sub)
        }
        fn bundle(&self, name: &str) -> PathBuf {
            let out = self.path(name);
            sign_bundle(
                &self.secret,
                "0.2.0",
                &self.path("src/rootfs.img"),
                &self.path("src/boot"),
                &out,
            )
            .unwrap();
            out
        }
        fn boot_into(&self, slot: Slot) {
            fs::write(
                self.path("cmdline"),
                format!(
                    "root=PARTUUID={DISK}-{} nibe.slot={}",
                    partition(slot),
                    letter(slot)
                ),
            )
            .unwrap();
        }
    }
    #[test]
    fn install_writes_inactive_slot_and_arms_tryboot_only() {
        let r = rig(Slot::A);
        let bundle = r.bundle("os.tar");
        assert_eq!(r.os.install(&bundle).unwrap(), Slot::B);
        let device = fs::read(r.path(&format!("dev/{DISK}-04"))).unwrap();
        assert_eq!(&device[..5000], &[7u8; 5000][..]);
        assert_eq!(
            fs::read(r.path(&format!("dev/{DISK}-02"))).unwrap(),
            vec![0u8; 8192]
        );
        assert_eq!(
            fs::read(r.path("boot/slot-b/kernel.img")).unwrap(),
            b"new kernel"
        );
        assert!(r.path("boot/slot-b/overlays/disable-bt.dtbo").is_file());
        let cmdline = fs::read_to_string(r.path("boot/slot-b/cmdline.txt")).unwrap();
        assert!(cmdline.contains(&format!("root=PARTUUID={DISK}-04")));
        assert!(cmdline.contains("nibe.slot=b") && cmdline.contains("nibe.overlay=1"));
        assert!(!cmdline.contains("ffff"));
        let tryboot = fs::read_to_string(r.path("boot/tryboot.txt")).unwrap();
        assert!(tryboot.contains("os_prefix=slot-b/") && tryboot.contains("enable_uart=1"));
        let config = fs::read_to_string(r.path("boot/config.txt")).unwrap();
        assert!(
            config.contains("os_prefix=slot-a/"),
            "committed slot unchanged"
        );
        let state = r.os.state().unwrap();
        assert_eq!(
            (state.phase, state.from, state.to),
            (OsPhase::Trying, Some(Slot::A), Some(Slot::B))
        );
        assert!(r.os.install(&bundle).is_err(), "one try at a time");
        assert!(!r.path("state/os-boot.tar").exists());
    }
    #[test]
    fn wrong_key_or_corrupt_image_never_arms_tryboot() {
        let r = rig(Slot::A);
        let bundle = r.bundle("os.tar");
        let mut other = OsUpdater {
            paths: r.os.paths.clone(),
            public: SigningKey::from_bytes(&[1u8; 32])
                .verifying_key()
                .to_bytes(),
        };
        assert!(other.install(&bundle).is_err());
        assert_eq!(
            fs::read(r.path(&format!("dev/{DISK}-04"))).unwrap(),
            vec![0u8; 8192]
        );
        // Flip one byte inside rootfs.img (the last member) after signing.
        let mut bytes = fs::read(&bundle).unwrap();
        let at = bytes.windows(5000).position(|w| w == [7u8; 5000]).unwrap() + 100;
        bytes[at] = 8;
        fs::write(&bundle, bytes).unwrap();
        other.public = r.os.public;
        assert!(other.install(&bundle).is_err());
        assert_eq!(r.os.state().unwrap().phase, OsPhase::Failed);
        assert!(!r.path("boot/tryboot.txt").exists());
        assert!(!r.path("boot/slot-b").exists());
    }
    #[test]
    fn missing_slot_marker_fails_safely_before_writing() {
        let r = rig(Slot::A);
        fs::write(r.path("boot/config.txt"), "[all]\nos_prefix=slot-a/\n").unwrap();
        assert!(r.os.install(&r.bundle("os.tar")).is_err());
        assert_eq!(r.os.state().unwrap().phase, OsPhase::Failed);
        assert_eq!(
            fs::read(r.path(&format!("dev/{DISK}-04"))).unwrap(),
            vec![0u8; 8192]
        );
        assert!(!r.path("boot/tryboot.txt").exists() && !r.path("boot/slot-b").exists());
    }
    #[test]
    fn unsafe_boot_paths_are_rejected() {
        let r = rig(Slot::A);
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_mode(0o644);
        // Bypass the builder's own path checks to forge a traversal entry.
        header.as_gnu_mut().unwrap().name[..9].copy_from_slice(b"../escape");
        header.set_cksum();
        builder.append(&header, &b"x"[..]).unwrap();
        fs::write(r.path("evil.tar"), builder.into_inner().unwrap()).unwrap();
        assert!(extract_boot(&r.path("evil.tar"), &r.path("out")).is_err());
        assert!(!r.path("escape").exists());
    }
    #[test]
    fn confirm_commits_healthy_try_and_fails_otherwise() {
        let r = rig(Slot::A);
        r.os.install(&r.bundle("os.tar")).unwrap();
        // The try did not stick: firmware fell back to slot A.
        let mut rebooted = false;
        let state =
            r.os.confirm(&mut || Ok(true), &mut || {
                rebooted = true;
                Ok(())
            })
            .unwrap();
        assert_eq!(state.phase, OsPhase::Failed);
        assert!(!rebooted && !r.path("boot/tryboot.txt").exists());
        // A healthy try is committed.
        let r = rig(Slot::A);
        r.os.install(&r.bundle("os.tar")).unwrap();
        r.boot_into(Slot::B);
        let state = r.os.confirm(&mut || Ok(true), &mut || Ok(())).unwrap();
        assert_eq!(state.phase, OsPhase::Committed);
        let config = fs::read_to_string(r.path("boot/config.txt")).unwrap();
        assert!(config.contains("os_prefix=slot-b/") && config.contains("enable_uart=1"));
        assert!(!r.path("boot/tryboot.txt").exists());
        assert_eq!(
            r.os.confirm(&mut || Ok(false), &mut || Ok(())).unwrap(),
            state
        );
        // An unhealthy try fails and reboots into the committed (old) slot.
        let r = rig(Slot::A);
        r.os.install(&r.bundle("os.tar")).unwrap();
        r.boot_into(Slot::B);
        let mut rebooted = false;
        let state =
            r.os.confirm(&mut || Ok(false), &mut || {
                rebooted = true;
                Ok(())
            })
            .unwrap();
        assert_eq!(state.phase, OsPhase::Failed);
        assert!(rebooted);
        let config = fs::read_to_string(r.path("boot/config.txt")).unwrap();
        assert!(config.contains("os_prefix=slot-a/"));
    }
    fn ext4_head() -> Vec<u8> {
        let mut head = vec![0u8; 5000];
        head[1080..1082].copy_from_slice(&[0x53, 0xEF]);
        head
    }
    fn squashfs_head() -> Vec<u8> {
        let mut head = b"hsqs".to_vec();
        head.resize(5000, 3);
        head
    }
    #[test]
    fn root_type_must_match_cmdline_rootfstype() {
        assert_eq!(root_image_type(&ext4_head()), Some("ext4"));
        assert_eq!(root_image_type(&squashfs_head()), Some("squashfs"));
        assert_eq!(root_image_type(&[7u8; 5000]), None);
        assert_eq!(root_image_type(b"hs"), None);
        let ext4 = "console=tty1 rootfstype=ext4 rootwait";
        let squash = "console=tty1 rootfstype=squashfs rootwait";
        assert!(check_root_type(ext4, &ext4_head()).is_ok());
        assert!(check_root_type(squash, &squashfs_head()).is_ok());
        assert!(check_root_type(ext4, &squashfs_head()).is_err());
        assert!(check_root_type(squash, &ext4_head()).is_err());
        assert!(check_root_type(squash, &[7u8; 5000]).is_err());
        // Templates from before the squashfs root, without rootfstype=, keep working (the initramfs probes the type).
        assert!(check_root_type("console=tty1 rootwait", &[7u8; 5000]).is_ok());
    }
    #[test]
    fn squashfs_slot_installs_and_rollback_to_ext4_works() {
        // A (ext4, running) -> B (squashfs) -> commit -> A (ext4 again): each slot keeps its own rootfstype.
        let r = rig(Slot::A);
        fs::write(r.path("src/rootfs.img"), squashfs_head()).unwrap();
        fs::write(
            r.path("src/boot/cmdline.txt"),
            "console=tty1 root=PARTUUID=ffff-02 rootfstype=squashfs fsck.repair=yes rootwait nibe.overlay=1\n",
        )
        .unwrap();
        assert_eq!(r.os.install(&r.bundle("squash.tar")).unwrap(), Slot::B);
        assert!(fs::read(r.path(&format!("dev/{DISK}-04")))
            .unwrap()
            .starts_with(b"hsqs"));
        let cmdline = fs::read_to_string(r.path("boot/slot-b/cmdline.txt")).unwrap();
        assert!(
            cmdline.contains("rootfstype=squashfs")
                && cmdline.contains(&format!("root=PARTUUID={DISK}-04"))
        );
        r.boot_into(Slot::B);
        assert_eq!(
            r.os.confirm(&mut || Ok(true), &mut || Ok(()))
                .unwrap()
                .phase,
            OsPhase::Committed
        );
        fs::write(r.path("src/rootfs.img"), ext4_head()).unwrap();
        fs::write(
            r.path("src/boot/cmdline.txt"),
            "console=tty1 root=PARTUUID=ffff-02 rootfstype=ext4 rootwait nibe.overlay=1\n",
        )
        .unwrap();
        assert_eq!(r.os.install(&r.bundle("ext4.tar")).unwrap(), Slot::A);
        let device = fs::read(r.path(&format!("dev/{DISK}-02"))).unwrap();
        assert_eq!(&device[1080..1082], &[0x53, 0xEF]);
        assert!(fs::read_to_string(r.path("boot/slot-a/cmdline.txt"))
            .unwrap()
            .contains("rootfstype=ext4"));
        assert!(
            fs::read_to_string(r.path("boot/slot-b/cmdline.txt"))
                .unwrap()
                .contains("rootfstype=squashfs"),
            "the other slot is untouched"
        );
    }
    #[test]
    fn mismatched_root_type_is_refused_before_writing() {
        let r = rig(Slot::A);
        fs::write(r.path("src/rootfs.img"), ext4_head()).unwrap();
        fs::write(
            r.path("src/boot/cmdline.txt"),
            "console=tty1 root=PARTUUID=ffff-02 rootfstype=squashfs rootwait\n",
        )
        .unwrap();
        assert!(sign_bundle(
            &r.secret,
            "0.2.0",
            &r.path("src/rootfs.img"),
            &r.path("src/boot"),
            &r.path("x.tar")
        )
        .is_err());
        // A bundle signed without the check (older nibe-release) is still refused by the installer.
        write_bundle(
            &r.secret,
            "0.2.0",
            &r.path("src/rootfs.img"),
            &r.path("src/boot"),
            &r.path("bad.tar"),
        )
        .unwrap();
        let err = r.os.install(&r.path("bad.tar")).unwrap_err().to_string();
        assert!(err.contains("rootfstype=squashfs"), "{err}");
        assert_eq!(
            fs::read(r.path(&format!("dev/{DISK}-04"))).unwrap(),
            vec![0u8; 8192]
        );
        assert!(!r.path("boot/tryboot.txt").exists() && !r.path("boot/slot-b").exists());
        assert!(!r.path("state/os-boot.tar").exists());
        assert_eq!(r.os.state().unwrap().phase, OsPhase::Failed);
    }
    #[test]
    fn cmdline_and_config_parsing_is_strict() {
        assert_eq!(
            running("root=PARTUUID=ab12cd34-04 nibe.slot=b").unwrap(),
            (Slot::B, "ab12cd34".into())
        );
        assert!(running("root=PARTUUID=ab12cd34-02 nibe.slot=b").is_err());
        assert!(running("root=PARTUUID=ab12-04 nibe.slot=b").is_err());
        assert!(running("root=/dev/mmcblk0p2 nibe.slot=a").is_err());
        assert!(running("root=PARTUUID=ab12-02").is_err());
        assert!(select_slot("os_prefix=slot-a/\n", Slot::B).is_err());
        assert!(select_slot(&format!("{MARKER}\nkernel=x\n"), Slot::B).is_err());
    }
}
