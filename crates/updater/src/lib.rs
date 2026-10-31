//! Privileged application-only updater. Root owns this tree; the gateway only
//! talks to the fixed Unix socket. No shell commands or client-supplied paths.
pub mod artifact;
pub mod os;
pub mod rpc;
pub mod service;
use artifact::{Manifest, Verified};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, OpenOptionsExt},
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn fail(message: &str) -> Box<dyn std::error::Error + Send + Sync> {
    std::io::Error::other(message).into()
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    A,
    B,
}
impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Self::A => "slot-a",
            Self::B => "slot-b",
        }
    }
    pub fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Staging,
    Staged,
    Switching,
    Checking,
    Probation,
    RollingBack,
    Complete,
    RolledBack,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateState {
    pub phase: Phase,
    pub active: Slot,
    pub previous: Option<Slot>,
    pub candidate: Option<Slot>,
    pub version: Option<String>,
    pub outcome: Option<String>,
    #[serde(default)]
    pub restart_count: u32,
}
impl Default for UpdateState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            active: Slot::A,
            previous: None,
            candidate: None,
            version: None,
            outcome: None,
            restart_count: 0,
        }
    }
}
pub trait ServiceManager {
    fn restart(&mut self) -> Result<()>;
    fn ready(&mut self, version: &str, release_id: &str) -> Result<bool>;
    /// Continuous health window; callbacks durably count process/boot changes.
    fn probation(
        &mut self,
        _version: &str,
        _release_id: &str,
        _restart: &mut dyn FnMut() -> Result<bool>,
    ) -> Result<bool> {
        Ok(true)
    }
}
/// Called after every durable boundary. Tests can emulate abrupt process loss.
pub trait Checkpoint {
    fn hit(&mut self, name: &str) -> Result<()>;
}
pub struct NoFault;
impl Checkpoint for NoFault {
    fn hit(&mut self, _: &str) -> Result<()> {
        Ok(())
    }
}
pub struct Updater {
    root: PathBuf,
    public: [u8; 32],
    target: String,
    _lock: File,
    pub state: UpdateState,
}
pub fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}
pub fn atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let parent = path.parent().ok_or_else(|| fail("missing parent"))?;
    let tmp = path.with_extension("tmp");
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(mode)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(tmp, path)?;
    sync_dir(parent)
}
impl Updater {
    pub fn open(root: &Path, public: [u8; 32], target: &str) -> Result<Self> {
        fs::create_dir_all(root.join("releases"))?;
        fs::create_dir_all(root.join("update-state"))?;
        sync_dir(root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(root.join("update-state/lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)?;
        let state_path = root.join("update-state/state.json");
        let state: UpdateState = if state_path.exists() {
            serde_json::from_slice(&fs::read(state_path)?)?
        } else {
            if fs::symlink_metadata(root.join("current")).is_ok() {
                return Err(fail("update state missing for installed release"));
            }
            UpdateState::default()
        };
        if !matches!(
            state.phase,
            Phase::Switching | Phase::Checking | Phase::Probation | Phase::RollingBack
        ) {
            match fs::read_link(root.join("current")) {
                Ok(link) if link == PathBuf::from(format!("releases/{}", state.active.name())) => {}
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && !root.join("update-state/state.json").exists() => {}
                _ => return Err(fail("active slot and update journal disagree")),
            }
        }
        Ok(Self {
            root: root.into(),
            public,
            target: target.into(),
            _lock: lock,
            state,
        })
    }
    fn save(&self) -> Result<()> {
        atomic(
            &self.root.join("update-state/state.json"),
            &serde_json::to_vec(&self.state)?,
            0o600,
        )
    }
    fn checkpoint(&self, fault: &mut dyn Checkpoint, name: &str) -> Result<()> {
        self.save()?;
        fault.hit(name)
    }
    fn slot(&self, s: Slot) -> PathBuf {
        self.root.join("releases").join(s.name())
    }
    fn manifest(&self, s: Slot) -> Result<Manifest> {
        Ok(serde_json::from_slice(&fs::read(
            self.slot(s).join("manifest.json"),
        )?)?)
    }
    fn switch(&self, s: Slot) -> Result<()> {
        if !self.slot(s).join("nibe-gateway").is_file() {
            return Err(fail("slot unavailable"));
        }
        let tmp = self.root.join("current.next");
        if fs::symlink_metadata(&tmp).is_ok() {
            fs::remove_file(&tmp)?;
        }
        symlink(format!("releases/{}", s.name()), &tmp)?;
        fs::rename(tmp, self.root.join("current"))?;
        sync_dir(&self.root)
    }
    pub fn stage(&mut self, bytes: &[u8], fault: &mut dyn Checkpoint) -> Result<()> {
        self.stage_confirmed(bytes, false, fault)
    }
    pub fn stage_confirmed(
        &mut self,
        bytes: &[u8],
        confirm_downgrade: bool,
        fault: &mut dyn Checkpoint,
    ) -> Result<()> {
        if matches!(
            self.state.phase,
            Phase::Switching
                | Phase::Checking
                | Phase::Probation
                | Phase::RollingBack
                | Phase::Staging
        ) {
            return Err(fail("recovery required"));
        }
        let verified = artifact::verify(bytes, &self.public, &self.target)?;
        let active = self.manifest(self.state.active)?;
        if semver::Version::parse(&verified.manifest.version)?
            .cmp_precedence(&semver::Version::parse(&active.version)?)
            .is_lt()
            && !confirm_downgrade
        {
            return Err(fail("downgrade confirmation required"));
        }
        let candidate = self.state.active.other();
        self.state.phase = Phase::Staging;
        self.state.candidate = Some(candidate);
        self.state.previous = None;
        self.state.outcome = None;
        self.state.version = Some(verified.manifest.version.clone());
        self.checkpoint(fault, "staging-intent")?;
        let path = self.slot(candidate);
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir(&path)?;
        sync_dir(&self.root.join("releases"))?;
        fault.hit("slot-cleared")?;
        self.write_slot(&path, verified, fault)?;
        self.state.phase = Phase::Staged;
        self.checkpoint(fault, "staged")
    }
    fn write_slot(
        &self,
        path: &Path,
        verified: Verified,
        fault: &mut dyn Checkpoint,
    ) -> Result<()> {
        for (name, bytes) in verified.files {
            let dest = path.join(&name);
            fs::create_dir_all(dest.parent().unwrap())?;
            // Slot is unpublished and journal says Staging. Write directly with
            // create_new so a signed filename can never alias another file's
            // temporary pathname. Recovery deletes the whole incomplete slot.
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(if name == "nibe-gateway" { 0o755 } else { 0o644 })
                .open(&dest)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fault.hit("payload-file")?;
        }
        atomic(&path.join("manifest.json"), &verified.manifest_bytes, 0o644)?;
        atomic(&path.join("manifest.sig"), &verified.signature, 0o644)?;
        // Sync every newly created directory, from leaves back to slot root.
        fn sync_tree(path: &Path) -> Result<()> {
            for item in fs::read_dir(path)? {
                let p = item?.path();
                if p.is_dir() {
                    sync_tree(&p)?;
                }
            }
            sync_dir(path)
        }
        sync_tree(path)?;
        sync_dir(&self.root.join("releases"))?;
        fault.hit("slot-synced")
    }
    pub fn prepare_activate(&mut self) -> Result<()> {
        if self.state.phase != Phase::Staged {
            return Err(fail("no staged release"));
        }
        self.state.restart_count = 0;
        self.state.previous = Some(self.state.active);
        self.state.phase = Phase::Switching;
        self.save()
    }
    pub fn activate(
        &mut self,
        service: &mut dyn ServiceManager,
        fault: &mut dyn Checkpoint,
    ) -> Result<()> {
        self.prepare_activate()?;
        fault.hit("switch-intent")?;
        self.finish_activate(service, fault)
    }
    pub fn finish_activate(
        &mut self,
        service: &mut dyn ServiceManager,
        fault: &mut dyn Checkpoint,
    ) -> Result<()> {
        if self.state.phase != Phase::Switching {
            return Err(fail("activation not prepared"));
        }
        let next = self.state.candidate.ok_or_else(|| fail("no candidate"))?;
        self.switch(next)?;
        fault.hit("symlink-switched")?;
        self.state.phase = Phase::Checking;
        self.checkpoint(fault, "checking")?;
        let manifest = self.manifest(next)?;
        let ready = service
            .restart()
            .and_then(|_| service.ready(&manifest.version, &manifest.sha256))
            .unwrap_or(false);
        fault.hit("readiness-returned")?;
        let ready = if ready {
            self.state.phase = Phase::Probation;
            self.checkpoint(fault, "probation")?;
            service
                .probation(&manifest.version, &manifest.sha256, &mut || {
                    self.state.restart_count = self.state.restart_count.saturating_add(1);
                    self.save()?;
                    Ok(true)
                })
                .unwrap_or(false)
        } else {
            false
        };
        if ready {
            self.state.active = next;
            self.state.phase = Phase::Complete;
            self.state.outcome = Some("activated".into());
            self.checkpoint(fault, "complete")
        } else {
            self.rollback(service, fault)
        }
    }
    pub fn rollback(
        &mut self,
        service: &mut dyn ServiceManager,
        fault: &mut dyn Checkpoint,
    ) -> Result<()> {
        self.prepare_rollback()?;
        fault.hit("rollback-intent")?;
        self.finish_rollback(service, fault)
    }
    pub fn prepare_rollback(&mut self) -> Result<()> {
        let previous = self
            .state
            .previous
            .ok_or_else(|| fail("no rollback slot"))?;
        if previous == self.state.active
            && !matches!(
                self.state.phase,
                Phase::Switching | Phase::Checking | Phase::Probation | Phase::RollingBack
            )
        {
            return Err(fail("previous release is already active"));
        }
        self.state.phase = Phase::RollingBack;
        self.save()
    }
    pub fn finish_rollback(
        &mut self,
        service: &mut dyn ServiceManager,
        fault: &mut dyn Checkpoint,
    ) -> Result<()> {
        if self.state.phase != Phase::RollingBack {
            return Err(fail("rollback not prepared"));
        }
        let old = self
            .state
            .previous
            .ok_or_else(|| fail("no rollback slot"))?;
        self.switch(old)?;
        fault.hit("rollback-switched")?;
        let manifest = self.manifest(old)?;
        let ready = service
            .restart()
            .and_then(|_| service.ready(&manifest.version, &manifest.sha256))
            .unwrap_or(false);
        fault.hit("rollback-readiness")?;
        self.state.active = old;
        self.state.candidate = None;
        // Keep previous until durable completion: recovery retries this same rollback.
        self.state.phase = if ready {
            Phase::RolledBack
        } else {
            Phase::Failed
        };
        self.state.outcome = Some(
            if ready {
                "rolled_back"
            } else {
                "rollback_not_ready"
            }
            .into(),
        );
        self.checkpoint(fault, "rollback-complete")?;
        if ready {
            Ok(())
        } else {
            Err(fail("rollback service not ready"))
        }
    }
    pub fn recover(&mut self, service: &mut dyn ServiceManager) -> Result<()> {
        match self.state.phase {
            Phase::Staging => {
                if let Some(slot) = self.state.candidate {
                    if self.slot(slot).exists() {
                        fs::remove_dir_all(self.slot(slot))?;
                        sync_dir(&self.root.join("releases"))?;
                    }
                }
                self.state.candidate = None;
                self.state.phase = Phase::Failed;
                self.state.outcome = Some("interrupted_staging".into());
                self.save()
            }
            Phase::Probation => {
                // Updater/host loss is itself a boot interruption. Recovery is
                // conservative even before the normal restart threshold.
                self.state.restart_count = self.state.restart_count.saturating_add(1);
                self.rollback(service, &mut NoFault)
            }
            Phase::Switching | Phase::Checking | Phase::RollingBack => {
                self.rollback(service, &mut NoFault)
            }
            _ => Ok(()),
        }
    }
    /// Bootstrap an already-provisioned factory slot; never used for an HTTP upload.
    pub fn initialize(&mut self) -> Result<()> {
        self.manifest(Slot::A)?;
        self.switch(Slot::A)?;
        self.save()
    }
}
