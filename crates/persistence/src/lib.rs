//! Small, versioned, crash-safe state store. Never persists HTTP sessions or telemetry.
pub mod audit;
use fs2::FileExt;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const SCHEMA_VERSION: u32 = 2;
fn default_reader_version() -> u32 {
    2
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkConfig {
    pub ssid: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub schema_version: u32,
    /// Old readers may retain a newer additive schema only when explicitly allowed.
    #[serde(default = "default_reader_version")]
    pub min_reader_version: u32,
    pub admin_password_hash: Option<String>,
    pub network: Option<NetworkConfig>,
    /// API keys (ADR 0110): only SHA-256 hashes are stored. Additive; older readers keep it in `extra`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_keys: Vec<ApiKey>,
    /// MQTT bridge (ADR 0115). Additive; older readers keep it in `extra`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt: Option<MqttSettings>,
    /// Poll-list changes made at runtime from the admin UI, API or MQTT (ADR 0125), applied on top of the
    /// configured lists at every start. Additive; older readers keep it in `extra`.
    #[serde(default, skip_serializing_if = "PollOverrides::is_empty")]
    pub poll_overrides: PollOverrides,
    /// Preserve unrelated/new optional keys during updates and rollback.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            min_reader_version: SCHEMA_VERSION,
            admin_password_hash: None,
            network: None,
            api_keys: Vec::new(),
            mqtt: None,
            poll_overrides: PollOverrides::default(),
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiScope {
    /// Status, readings, explorer, events and the WebSocket stream.
    Read,
    /// Read plus the named controls.
    Control,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub scope: ApiScope,
    /// Hex SHA-256 of the full token; the token itself is shown once and never stored.
    pub hash: String,
    pub created_at_ms: u64,
}
/// How often a register is read: every poll round, about every slow interval, or not at all.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PollMode {
    Fast,
    Slow,
    Off,
}
/// Per-register poll choices made at runtime, overriding the configured lists. Only deviations from the
/// configuration are kept: choosing a register's configured mode again removes its entry.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct PollOverrides(pub BTreeMap<u16, PollMode>);
impl PollOverrides {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// A register's mode in the configured `fast` / `slow` lists (fast wins when listed in both).
    pub fn configured(id: u16, fast: &[u16], slow: &[u16]) -> PollMode {
        if fast.contains(&id) {
            PollMode::Fast
        } else if slow.contains(&id) {
            PollMode::Slow
        } else {
            PollMode::Off
        }
    }
    /// The effective (fast, slow) lists for configured `fast` and `slow`, in configured order, overrides last.
    pub fn apply(&self, fast: &[u16], slow: &[u16]) -> (Vec<u16>, Vec<u16>) {
        let keep = |id: &u16| !self.0.contains_key(id);
        let mut fast: Vec<u16> = fast.iter().copied().filter(keep).collect();
        let mut slow: Vec<u16> = slow.iter().copied().filter(keep).collect();
        for (&id, mode) in &self.0 {
            match mode {
                PollMode::Fast => fast.push(id),
                PollMode::Slow => slow.push(id),
                PollMode::Off => {}
            }
        }
        (fast, slow)
    }
    /// Record choosing `mode` for `id`, given the configured lists.
    pub fn record(&mut self, id: u16, mode: PollMode, fast: &[u16], slow: &[u16]) {
        if mode == Self::configured(id, fast, slow) {
            self.0.remove(&id);
        } else {
            self.0.insert(id, mode);
        }
    }
}
/// MQTT broker settings (ADR 0115). The password is stored as given, like the Wi-Fi password, in the
/// 0600 config on the persistent partition; it is never returned by the API. Deliberately no Debug.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MqttSettings {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub topic_prefix: String,
    pub discovery: bool,
    pub discovery_prefix: String,
    /// Accept commands on `<prefix>/<device>/set/...`; still limited to controls the gateway config enables.
    /// Also gates the per-register `register/<id>/set`, `set_raw`, `get`, `add` and `remove` topics (ADR 0125).
    pub allow_commands: bool,
}
impl Default for MqttSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 1883,
            username: None,
            password: None,
            topic_prefix: "nibe".into(),
            discovery: true,
            discovery_prefix: "homeassistant".into(),
            allow_commands: false,
        }
    }
}
impl MqttSettings {
    /// Rejects values that would make broken or wildcard topics or an unusable connection.
    pub fn validate(&self) -> Result<(), &'static str> {
        fn topic_ok(value: &str) -> bool {
            !(value.is_empty()
                || value.len() > 64
                || value.starts_with('/')
                || value.ends_with('/')
                || value.contains(['+', '#', '\0', ' ']))
        }
        if !topic_ok(&self.topic_prefix) {
            return Err("topic_prefix must be 1-64 characters without spaces, '+', '#' or a leading/trailing '/'");
        }
        if !topic_ok(&self.discovery_prefix) {
            return Err("discovery_prefix must be 1-64 characters without spaces, '+', '#' or a leading/trailing '/'");
        }
        if self.enabled
            && (self.host.is_empty()
                || self.host.len() > 253
                || !self
                    .host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_')))
        {
            return Err("host must be a hostname or IP address");
        }
        if self.port == 0 {
            return Err("port must be 1-65535");
        }
        if [&self.username, &self.password]
            .iter()
            .any(|v| v.as_ref().is_some_and(|v| v.len() > 256))
        {
            return Err("username and password are limited to 256 bytes");
        }
        Ok(())
    }
}
/// Deliberately no Debug: the caller must not accidentally log secrets.
#[derive(Clone, Serialize, Deserialize)]
pub struct SetupSecrets {
    pub setup_token: String,
    /// Unused since the setup AP is open (ADR 0060), but still written: releases before that require
    /// it to open the data directory, so an update to one of them or a rollback would not start.
    /// Those releases use it as their WPA2 AP password. Missing in files from firstboot.2/.3 builds;
    /// `Store::open` adds it.
    #[serde(default)]
    ap_password: String,
    pub ssid_suffix: String,
}
impl SetupSecrets {
    fn generate(identity: &str) -> Self {
        let suffix: String = identity.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let suffix = if suffix.len() >= 4 {
            suffix[suffix.len() - 4..].to_uppercase()
        } else {
            random_token(2).to_uppercase()
        };
        Self {
            setup_token: random_token(32),
            ap_password: random_token(16),
            ssid_suffix: suffix,
        }
    }
    pub fn ssid(&self) -> String {
        format!("NIBE-Gateway-{}", self.ssid_suffix)
    }
}
pub fn random_token(bytes: usize) -> String {
    let mut data = vec![0; bytes];
    OsRng.fill_bytes(&mut data);
    data.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct RegisterVerification {
    /// Evidence only; never grants write permission or establishes hardware verification.
    pub state: String,
    pub source: String,
    pub timestamp_ms: u64,
}
pub type VerificationOverlay = BTreeMap<u16, RegisterVerification>;

/// What a register means on this installation, set by the owner (ADR 0041). Separate from the shared
/// catalog and from the automatic observation overlay: a note never grants write permission and never
/// changes either of them.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Relevance {
    /// Works and matters on this pump.
    Relevant,
    /// Answers, but the function is not fitted or not used here (constant, sentinel, irrelevant).
    Absent,
    /// The decode or meaning is doubtful; do not rely on it.
    Suspicious,
    /// The register answers but the firmware does not keep it up to date.
    Unmaintained,
}
pub const MAX_REGISTER_NOTES: usize = 2048;
pub const MAX_NOTE_LABEL_CHARS: usize = 80;
pub const MAX_NOTE_TEXT_BYTES: usize = 256;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterNote {
    pub relevance: Relevance,
    /// The value was checked against the pump's own display.
    #[serde(default)]
    pub display_verified: Option<bool>,
    /// Short human meaning on this installation, e.g. "active fan speed step".
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Where the classification comes from (survey, display photo, owner test).
    pub source: String,
    pub timestamp_ms: u64,
}
impl RegisterNote {
    pub fn validate(&self) -> Result<(), &'static str> {
        fn text_ok(value: &str, max: usize) -> bool {
            !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        }
        if !text_ok(&self.source, MAX_NOTE_TEXT_BYTES) {
            return Err("source must be 1-256 bytes without control characters");
        }
        if self.label.as_deref().is_some_and(|l| {
            !text_ok(l, MAX_NOTE_TEXT_BYTES) || l.chars().count() > MAX_NOTE_LABEL_CHARS
        }) {
            return Err("label must be 1-80 characters without control characters");
        }
        if self
            .note
            .as_deref()
            .is_some_and(|n| !text_ok(n, MAX_NOTE_TEXT_BYTES))
        {
            return Err("note must be 1-256 bytes without control characters");
        }
        Ok(())
    }
}
pub type RegisterNotes = BTreeMap<u16, RegisterNote>;

/// A writer's lease on the heating offset (ADR 0123): if it is not renewed before `expires_at_ms`, the
/// gateway writes `baseline` back. Times are gateway clock milliseconds.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct ControlLease {
    /// API key name, or "admin" for a browser session.
    pub holder: String,
    /// The offset the holder last wrote.
    pub offset: i8,
    /// The offset read back before the first leased write; restored when the lease lapses.
    pub baseline: i8,
    pub granted_at_ms: u64,
    pub expires_at_ms: u64,
    /// Owner supersession awaiting a dispatch outcome (requested offset, attempt time).
    #[serde(default)]
    pub pending_supersession: Option<(i8, u64)>,
}
/// The last time a lapsed lease was handed back to the baseline.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct LeaseRestore {
    pub at_ms: u64,
    pub holder: String,
    pub baseline: i8,
    /// True when the restore write was accepted; the pump's read-back is the confirmation.
    pub ok: bool,
    pub detail: String,
}
/// Saved control leases (ADR 0123). The heating offset keeps its original fields, so a release
/// that leased only the heating offset (for example after an update rollback) still restores it; other registers are in
/// `registers`/`restores`, which such a release ignores.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LeaseState {
    #[serde(default)]
    pub heating_offset: Option<ControlLease>,
    #[serde(default)]
    pub last_restore: Option<LeaseRestore>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub registers: BTreeMap<u16, RegisterLease>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub restores: BTreeMap<u16, RegisterLeaseRestore>,
}
const LEASE_FILE: &str = "control-lease.json";
/// Heat offset S1, stored in the original `heating_offset` fields.
pub const HEAT_OFFSET_REGISTER: u16 = 47011;

/// A writer's lease on a register (ADR 0123): if it is not renewed before `expires_at_ms`, the
/// gateway writes `baseline` back. Raw values as on the bus; times are gateway clock milliseconds.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct RegisterLease {
    pub register: u16,
    /// API key name, "admin" for a browser session, or "mqtt".
    pub holder: String,
    /// The raw value the holder last wrote.
    pub raw: i32,
    /// The raw value read from the pump before the first leased write; restored when the lease lapses.
    pub baseline: i32,
    pub granted_at_ms: u64,
    pub expires_at_ms: u64,
    /// Unleased write awaiting its dispatch outcome (requested raw value, attempt time).
    #[serde(default)]
    pub pending_supersession: Option<(i32, u64)>,
}
/// The last time a lapsed lease on a register was handed back to its baseline.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct RegisterLeaseRestore {
    pub register: u16,
    pub at_ms: u64,
    pub holder: String,
    pub baseline: i32,
    /// True once the pump accepted the restore and read back the baseline.
    pub ok: bool,
    pub detail: String,
}
impl LeaseState {
    /// All leases and last restores by register, the heating offset included.
    pub fn by_register(
        &self,
    ) -> (
        BTreeMap<u16, RegisterLease>,
        BTreeMap<u16, RegisterLeaseRestore>,
    ) {
        let mut leases = self.registers.clone();
        let mut restores = self.restores.clone();
        if let Some(l) = &self.heating_offset {
            leases.insert(HEAT_OFFSET_REGISTER, l.to_register());
        }
        if let Some(r) = &self.last_restore {
            restores.insert(HEAT_OFFSET_REGISTER, r.to_register());
        }
        (leases, restores)
    }
    pub fn from_registers(
        leases: &BTreeMap<u16, RegisterLease>,
        restores: &BTreeMap<u16, RegisterLeaseRestore>,
    ) -> Self {
        let other = |id: &&u16| **id != HEAT_OFFSET_REGISTER;
        Self {
            heating_offset: leases
                .get(&HEAT_OFFSET_REGISTER)
                .and_then(ControlLease::from_register),
            last_restore: restores
                .get(&HEAT_OFFSET_REGISTER)
                .and_then(LeaseRestore::from_register),
            registers: leases
                .iter()
                .filter(|(id, _)| other(id))
                .map(|(id, l)| (*id, l.clone()))
                .collect(),
            restores: restores
                .iter()
                .filter(|(id, _)| other(id))
                .map(|(id, r)| (*id, r.clone()))
                .collect(),
        }
    }
}
impl ControlLease {
    pub fn to_register(&self) -> RegisterLease {
        RegisterLease {
            register: HEAT_OFFSET_REGISTER,
            holder: self.holder.clone(),
            raw: self.offset.into(),
            baseline: self.baseline.into(),
            granted_at_ms: self.granted_at_ms,
            expires_at_ms: self.expires_at_ms,
            pending_supersession: self.pending_supersession.map(|(v, t)| (v.into(), t)),
        }
    }
    /// `None` for values an offset cannot hold.
    pub fn from_register(l: &RegisterLease) -> Option<Self> {
        Some(Self {
            holder: l.holder.clone(),
            offset: i8::try_from(l.raw).ok()?,
            baseline: i8::try_from(l.baseline).ok()?,
            granted_at_ms: l.granted_at_ms,
            expires_at_ms: l.expires_at_ms,
            pending_supersession: match l.pending_supersession {
                Some((v, t)) => Some((i8::try_from(v).ok()?, t)),
                None => None,
            },
        })
    }
}
impl LeaseRestore {
    pub fn to_register(&self) -> RegisterLeaseRestore {
        RegisterLeaseRestore {
            register: HEAT_OFFSET_REGISTER,
            at_ms: self.at_ms,
            holder: self.holder.clone(),
            baseline: self.baseline.into(),
            ok: self.ok,
            detail: self.detail.clone(),
        }
    }
    pub fn from_register(r: &RegisterLeaseRestore) -> Option<Self> {
        Some(Self {
            at_ms: r.at_ms,
            holder: r.holder.clone(),
            baseline: i8::try_from(r.baseline).ok()?,
            ok: r.ok,
            detail: r.detail.clone(),
        })
    }
}

pub struct Store {
    dir: PathBuf,
    config: Mutex<Config>,
    secrets: Mutex<SetupSecrets>,
    verification: Mutex<VerificationOverlay>,
    notes: Mutex<RegisterNotes>,
    lease: Mutex<LeaseState>,
    _lock: File,
}
impl Store {
    pub fn open(dir: impl AsRef<Path>, identity: &str) -> io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        if dir.exists() && fs::symlink_metadata(&dir)?.file_type().is_symlink() {
            return Err(io::Error::other("data directory must not be a symlink"));
        }
        fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        let lock_path = dir.join(".lock");
        reject_symlink(&lock_path)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let lock = options.open(lock_path)?;
        lock.try_lock_exclusive()
            .map_err(|_| io::Error::other("data directory already in use"))?;
        let config_path = dir.join("config.json");
        let config = match read_private(&config_path)? {
            None => Config::default(),
            Some(bytes) => migrate(&bytes)?,
        };
        // Write only on initial creation or actual migration; startup does not wear the SD.
        atomic_json(&config_path, &config)?;
        let secrets_path = dir.join("setup-secrets.json");
        let secrets = match read_private(&secrets_path)? {
            Some(bytes) => {
                let mut secrets: SetupSecrets =
                    serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                if secrets.ap_password.is_empty() {
                    // Written by a build that dropped the field: earlier releases cannot open it.
                    secrets.ap_password = random_token(16);
                    atomic_json(&secrets_path, &secrets)?;
                }
                secrets
            }
            None => {
                let value = SetupSecrets::generate(identity);
                atomic_json(&secrets_path, &value)?;
                value
            }
        };
        // The same rule as earlier releases, so a file this release accepts is one they accept too.
        if secrets.setup_token.len() != 64
            || secrets.ap_password.len() != 32
            || secrets.ssid_suffix.len() != 4
            || !secrets.setup_token.bytes().all(|b| b.is_ascii_hexdigit())
            || !secrets.ap_password.bytes().all(|b| b.is_ascii_hexdigit())
            || !secrets.ssid_suffix.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(io::Error::other(
                "invalid setup secrets; restore backup rather than reset ownership",
            ));
        }
        let verification = match read_private(&dir.join("register-verification.json"))? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            None => BTreeMap::new(),
        };
        let notes: RegisterNotes = match read_private(&dir.join(NOTES_FILE))? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            None => BTreeMap::new(),
        };
        if notes.len() > MAX_REGISTER_NOTES || notes.values().any(|n| n.validate().is_err()) {
            return Err(io::Error::other("invalid register notes"));
        }
        let lease = match read_private(&dir.join(LEASE_FILE))? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            None => LeaseState::default(),
        };
        Ok(Self {
            lease: Mutex::new(lease),
            dir,
            verification: Mutex::new(verification),
            notes: Mutex::new(notes),
            config: Mutex::new(config),
            secrets: Mutex::new(secrets),
            _lock: lock,
        })
    }
    pub fn verification(&self) -> VerificationOverlay {
        self.verification.lock().unwrap().clone()
    }
    pub fn record_verification(
        &self,
        entries: impl IntoIterator<Item = (u16, String, String)>,
        timestamp_ms: u64,
    ) -> io::Result<()> {
        let mut current = self.verification.lock().unwrap();
        let mut next = current.clone();
        for (id, state, source) in entries {
            if !matches!(
                state.as_str(),
                "observed" | "confirmed-readable" | "unsupported"
            ) || source.len() > 256
            {
                return Err(io::Error::other("invalid register evidence"));
            }
            // A new passive sighting after restart cannot erase an established
            // matching read. Keep its original provenance and date in this mode.
            if next.get(&id).is_some_and(|old| {
                old.state == "confirmed-readable"
                    && state != "confirmed-readable"
                    && old.source.split(':').next() == source.split(':').next()
            }) {
                continue;
            }
            if !next
                .get(&id)
                .is_some_and(|old| old.state == state && old.source == source)
            {
                next.insert(
                    id,
                    RegisterVerification {
                        state,
                        source,
                        timestamp_ms,
                    },
                );
            }
        }
        if *current != next {
            atomic_json(&self.dir.join("register-verification.json"), &next)?;
            *current = next;
        }
        Ok(())
    }
    pub fn register_notes(&self) -> RegisterNotes {
        self.notes.lock().unwrap().clone()
    }
    /// Replaces all notes, or merges the given ones over the existing set. Validates everything
    /// before writing; an invalid entry changes nothing. Returns the stored set.
    pub fn set_register_notes(
        &self,
        entries: impl IntoIterator<Item = (u16, RegisterNote)>,
        replace: bool,
    ) -> io::Result<RegisterNotes> {
        let mut current = self.notes.lock().unwrap();
        let mut next = if replace {
            RegisterNotes::new()
        } else {
            current.clone()
        };
        for (id, note) in entries {
            note.validate()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            next.insert(id, note);
        }
        if next.len() > MAX_REGISTER_NOTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "too many register notes",
            ));
        }
        if *current != next {
            atomic_json(&self.dir.join(NOTES_FILE), &next)?;
            *current = next;
        }
        Ok(current.clone())
    }
    /// Control lease state (ADR 0123). Deliberately kept across factory reset: a lapsed lease must still
    /// hand the pump back to its baseline.
    pub fn lease_state(&self) -> LeaseState {
        self.lease.lock().unwrap().clone()
    }
    pub fn set_lease_state(&self, state: &LeaseState) -> io::Result<()> {
        let mut current = self.lease.lock().unwrap();
        if *current != *state {
            atomic_json(&self.dir.join(LEASE_FILE), state)?;
            *current = state.clone();
        }
        Ok(())
    }
    pub fn config(&self) -> Config {
        self.config.lock().unwrap().clone()
    }
    pub fn setup_secrets(&self) -> SetupSecrets {
        self.secrets.lock().unwrap().clone()
    }
    /// Serializes read-modify-write, so auth and network cannot overwrite each other.
    pub fn update(&self, change: impl FnOnce(&mut Config)) -> io::Result<()> {
        let mut config = self.config.lock().unwrap();
        let mut next = config.clone();
        change(&mut next);
        atomic_json(&self.dir.join("config.json"), &next)?;
        *config = next;
        Ok(())
    }
    pub fn factory_reset(&self, identity: &str) -> io::Result<()> {
        let mut config = self.config.lock().unwrap();
        let mut secrets = self.secrets.lock().unwrap();
        let next = SetupSecrets::generate(identity);
        // Rotate the physical setup proof first: interruption never resurrects an old token.
        atomic_json(&self.dir.join("setup-secrets.json"), &next)?;
        *secrets = next;
        atomic_json(&self.dir.join("config.json"), &Config::default())?;
        *config = Config::default();
        let mut verification = self.verification.lock().unwrap();
        atomic_json(
            &self.dir.join("register-verification.json"),
            &VerificationOverlay::new(),
        )?;
        verification.clear();
        let mut notes = self.notes.lock().unwrap();
        atomic_json(&self.dir.join(NOTES_FILE), &RegisterNotes::new())?;
        notes.clear();
        Ok(())
    }
}
const NOTES_FILE: &str = "register-notes.json";
fn migrate(bytes: &[u8]) -> io::Result<Config> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    match value["schema_version"].as_u64() {
        Some(1) => {
            let ssid = value
                .as_object_mut()
                .unwrap()
                .remove("wifi_ssid")
                .unwrap_or_default();
            value["network"] = if ssid.is_string() {
                serde_json::json!({"ssid":ssid})
            } else {
                serde_json::Value::Null
            };
            value["schema_version"] = SCHEMA_VERSION.into();
        }
        Some(v) if v == u64::from(SCHEMA_VERSION) => {}
        Some(v)
            if v > u64::from(SCHEMA_VERSION)
                && value["min_reader_version"].as_u64().is_some_and(|r| r == 2) => {}
        _ => {
            return Err(io::Error::other(
                "unsupported config schema; preserve data and upgrade binary",
            ))
        }
    }
    if value["min_reader_version"]
        .as_u64()
        .is_some_and(|r| r > u64::from(SCHEMA_VERSION) || r < 2)
    {
        return Err(io::Error::other("config requires a newer reader"));
    }
    if !value.as_object().is_some_and(|object| {
        object.contains_key("admin_password_hash") && object.contains_key("network")
    }) {
        return Err(io::Error::other(
            "incomplete config; restore backup rather than reset ownership",
        ));
    }
    serde_json::from_value(value).map_err(io::Error::other)
}
fn reject_symlink(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => {
            Err(io::Error::other("state path must be a regular file"))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
fn read_private(path: &Path) -> io::Result<Option<Vec<u8>>> {
    reject_symlink(path)?;
    match fs::read(path) {
        Ok(data) => {
            #[cfg(unix)]
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            Ok(Some(data))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn atomic_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    reject_symlink(path)?;
    if fs::read(path).ok().as_deref() == Some(bytes.as_slice()) {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing state parent"))?;
    let temp = parent.join(format!(".write-{}", random_token(12)));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heating_offset_lease_stays_readable_by_releases_before_setpoint_leases() {
        let lease = |register, raw, baseline| RegisterLease {
            register,
            holder: "neo".into(),
            raw,
            baseline,
            granted_at_ms: 1,
            expires_at_ms: 2,
            pending_supersession: Some((raw, 1)),
        };
        let leases = BTreeMap::from([
            (HEAT_OFFSET_REGISTER, lease(HEAT_OFFSET_REGISTER, -10, -2)),
            (47049, lease(47049, 300, 450)),
        ]);
        let json =
            serde_json::to_value(LeaseState::from_registers(&leases, &BTreeMap::new())).unwrap();
        // The shape a 0.1.29 gateway reads, which ignores unknown fields.
        #[derive(serde::Deserialize)]
        struct Before {
            heating_offset: Option<ControlLease>,
        }
        let before: Before = serde_json::from_value(json.clone()).unwrap();
        let offset = before.heating_offset.unwrap();
        assert_eq!((offset.offset, offset.baseline), (-10, -2));
        let state: LeaseState = serde_json::from_value(json).unwrap();
        assert_eq!(state.by_register().0, leases);
    }
    #[test]
    fn additive_v3_can_roll_back_without_losing_config_or_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, br#"{"schema_version":3,"min_reader_version":2,"admin_password_hash":"hash","network":{"ssid":"home","v3_network_option":42},"v3_option":{"keep":true}}"#).unwrap();
        let store = Store::open(dir.path(), "id").unwrap();
        assert_eq!(store.config().schema_version, 3);
        store
            .update(|c| {
                c.extra.insert("old_reader_edit".into(), true.into());
            })
            .unwrap();
        drop(store);
        let store = Store::open(dir.path(), "id").unwrap();
        assert_eq!(store.config().admin_password_hash.as_deref(), Some("hash"));
        assert_eq!(store.config().extra["v3_option"]["keep"], true);
        assert_eq!(
            store.config().network.unwrap().extra["v3_network_option"],
            42
        );
        drop(store);
        let mut config: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["min_reader_version"] = 3.into();
        fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        assert!(Store::open(dir.path(), "id").is_err());
    }
    #[test]
    fn evidence_survives_restart_and_writes_only_on_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "id").unwrap();
        let entry = || [(40004, "observed".into(), "replay:passive".into())];
        store.record_verification(entry(), 100).unwrap();
        let path = dir.path().join("register-verification.json");
        let metadata = fs::metadata(&path).unwrap().modified().unwrap();
        store.record_verification(entry(), 200).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), metadata);
        assert_eq!(store.verification()[&40004].timestamp_ms, 100);
        drop(store);
        let store = Store::open(dir.path(), "id").unwrap();
        assert_eq!(store.verification()[&40004].state, "observed");
        store
            .record_verification(
                [(40004, "confirmed-readable".into(), "replay:probe".into())],
                300,
            )
            .unwrap();
        assert_eq!(store.verification()[&40004].timestamp_ms, 300);
        store.record_verification(entry(), 400).unwrap();
        assert_eq!(store.verification()[&40004].state, "confirmed-readable");
        assert_eq!(store.verification()[&40004].timestamp_ms, 300);
        store.factory_reset("id").unwrap();
        assert!(store.verification().is_empty());
    }
    fn note(relevance: Relevance) -> RegisterNote {
        RegisterNote {
            relevance,
            display_verified: Some(true),
            label: Some("active fan speed step".into()),
            note: None,
            source: "owner test 2026-10-26".into(),
            timestamp_ms: 1,
        }
    }
    #[test]
    fn register_notes_validate_round_trip_and_die_with_factory_reset() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "id").unwrap();
        assert!(store.register_notes().is_empty());
        for bad in [
            RegisterNote {
                source: " ".into(),
                ..note(Relevance::Relevant)
            },
            RegisterNote {
                source: "x".repeat(257),
                ..note(Relevance::Relevant)
            },
            RegisterNote {
                label: Some("x".repeat(81)),
                ..note(Relevance::Relevant)
            },
            RegisterNote {
                note: Some("line\nbreak".into()),
                ..note(Relevance::Relevant)
            },
        ] {
            let error = store
                .set_register_notes([(40004, note(Relevance::Relevant)), (43108, bad)], false)
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
        assert!(
            store.register_notes().is_empty(),
            "invalid batch changes nothing"
        );
        assert!(!dir.path().join("register-notes.json").exists());
        store
            .set_register_notes(
                [
                    (43108, note(Relevance::Relevant)),
                    (41256, note(Relevance::Unmaintained)),
                ],
                false,
            )
            .unwrap();
        // Merge keeps other entries and replaces the same register.
        let merged = store
            .set_register_notes([(41256, note(Relevance::Absent))], false)
            .unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[&41256].relevance, Relevance::Absent);
        let path = dir.path().join("register-notes.json");
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["43108"]["relevance"], "relevant");
        assert_eq!(saved["43108"]["label"], "active fan speed step");
        drop(store);
        let store = Store::open(dir.path(), "id").unwrap();
        assert_eq!(store.register_notes()[&43108], note(Relevance::Relevant));
        assert!(
            store.verification().is_empty(),
            "notes never touch the overlay"
        );
        let replaced = store
            .set_register_notes([(40004, note(Relevance::Suspicious))], true)
            .unwrap();
        assert_eq!(replaced.keys().copied().collect::<Vec<_>>(), [40004]);
        store.factory_reset("id").unwrap();
        assert!(store.register_notes().is_empty());
        drop(store);
        assert!(Store::open(dir.path(), "id")
            .unwrap()
            .register_notes()
            .is_empty());
        // A file with unknown fields or an invalid entry is refused, like the overlay.
        fs::write(
            &path,
            br#"{"40004":{"relevance":"relevant","source":"s","timestamp_ms":1,"write":true}}"#,
        )
        .unwrap();
        assert!(Store::open(dir.path(), "id").is_err());
    }
    #[test]
    fn migration_permissions_restarts_and_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("config.json"), br#"{"schema_version":1,"admin_password_hash":null,"wifi_ssid":"Home WiFi","future_option":42}"#).unwrap();
        let store = Store::open(dir.path(), "abcdef123456").unwrap();
        assert_eq!(store.config().network.unwrap().ssid, "Home WiFi");
        let token = store.setup_secrets().setup_token;
        store
            .update(|c| c.admin_password_hash = Some("hash".into()))
            .unwrap();
        assert_eq!(store.config().extra["future_option"], 42);
        assert!(Store::open(dir.path(), "id").is_err());
        #[cfg(unix)]
        for name in ["config.json", "setup-secrets.json"] {
            assert_eq!(
                fs::metadata(dir.path().join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let modified = fs::metadata(dir.path().join("config.json"))
            .unwrap()
            .modified()
            .unwrap();
        drop(store);
        let store = Store::open(dir.path(), "other").unwrap();
        assert_eq!(store.setup_secrets().setup_token, token);
        assert_eq!(
            fs::metadata(dir.path().join("config.json"))
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
        store.factory_reset("abcdef123456").unwrap();
        assert!(store.config().admin_password_hash.is_none());
        assert!(store.config().network.is_none());
        assert_ne!(store.setup_secrets().setup_token, token);
    }
    #[test]
    fn setup_secrets_stay_readable_by_releases_before_the_open_ap() {
        // Releases before the open setup AP deserialize `ap_password` as a required field and check it is 32 hex
        // characters; without it they cannot open the data directory (update to them, or a rollback).
        let released = |path: &Path| {
            let v: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            let ap = v["ap_password"].as_str().expect("ap_password written");
            assert!(ap.len() == 32 && ap.bytes().all(|b| b.is_ascii_hexdigit()));
            v
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup-secrets.json");
        let store = Store::open(dir.path(), "abcdef123456").unwrap();
        released(&path);
        assert_eq!(store.setup_secrets().ssid(), "NIBE-Gateway-3456");
        drop(store);
        // A file written by firstboot.2/.3 has no `ap_password`: the next start adds one, keeping the token.
        let token = "a".repeat(64);
        fs::write(
            &path,
            format!(r#"{{"setup_token":"{token}","ssid_suffix":"BEEF"}}"#),
        )
        .unwrap();
        let store = Store::open(dir.path(), "abcdef123456").unwrap();
        assert_eq!(released(&path)["setup_token"], token.as_str());
        assert_eq!(store.setup_secrets().setup_token, token);
        drop(store);
        // A file from before the open setup AP loads unchanged.
        let legacy = format!(
            r#"{{"setup_token":"{token}","ap_password":"{}","ssid_suffix":"BEEF"}}"#,
            "b".repeat(32)
        );
        fs::write(&path, &legacy).unwrap();
        let store = Store::open(dir.path(), "abcdef123456").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            legacy,
            "no rewrite on a normal start"
        );
        assert_eq!(store.setup_secrets().ssid(), "NIBE-Gateway-BEEF");
        // A factory reset writes a full file too.
        store.factory_reset("abcdef123456").unwrap();
        released(&path);
    }
    #[test]
    fn corrupt_or_future_config_never_resets_to_unclaimed() {
        for bytes in [b"bad JSON".as_slice(), br#"{"schema_version":99}"#] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.json");
            fs::write(&path, bytes).unwrap();
            assert!(Store::open(dir.path(), "id").is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_secret_files() {
        let dir = tempfile::tempdir().unwrap();
        let external = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(external.path(), dir.path().join("setup-secrets.json")).unwrap();
        assert!(Store::open(dir.path(), "id").is_err());
    }
}
#[cfg(test)]
mod poll_override_tests {
    use super::*;
    #[test]
    fn overrides_move_registers_between_lists_and_keep_only_deviations() {
        let (fast, slow) = ([40004, 40008], [43416, 47041]);
        let mut o = PollOverrides::default();
        o.record(47049, PollMode::Slow, &fast, &slow);
        o.record(40004, PollMode::Off, &fast, &slow);
        o.record(43416, PollMode::Fast, &fast, &slow);
        assert_eq!(
            o.apply(&fast, &slow),
            (vec![40008, 43416], vec![47041, 47049])
        );
        // Choosing the configured mode again drops the override.
        o.record(40004, PollMode::Fast, &fast, &slow);
        o.record(43416, PollMode::Slow, &fast, &slow);
        o.record(47049, PollMode::Off, &fast, &slow);
        assert!(o.is_empty());
        // Stored as {"<id>": "<mode>"}; old settings files without the field still load.
        o.record(47049, PollMode::Slow, &fast, &slow);
        assert_eq!(serde_json::to_string(&o).unwrap(), r#"{"47049":"slow"}"#);
        let config: Config = serde_json::from_str(
            r#"{"schema_version":1,"admin_password_hash":null,"network":null}"#,
        )
        .unwrap();
        assert!(config.poll_overrides.is_empty());
    }
}
