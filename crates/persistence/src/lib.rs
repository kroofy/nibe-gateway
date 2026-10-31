//! Small, versioned, crash-safe state store. Never persists HTTP sessions or telemetry.
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
    pub ap_password: String,
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

/// What a register means on this installation, set by the owner (ADR 0119). Separate from the shared
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
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LeaseState {
    #[serde(default)]
    pub heating_offset: Option<ControlLease>,
    #[serde(default)]
    pub last_restore: Option<LeaseRestore>,
}
const LEASE_FILE: &str = "control-lease.json";

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
            Some(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            None => {
                let value = SetupSecrets::generate(identity);
                atomic_json(&secrets_path, &value)?;
                value
            }
        };
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
