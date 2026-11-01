//! Audit log: every change sent to the gateway or the pump, newest last, kept in a bounded
//! append-only file on the data partition. Entries are small and rare (people and integrations changing
//! things), so each is appended as one JSON line without fsync; the file rotates once at `ROTATE_BYTES`, which
//! bounds SD-card use to about twice that. Files are written on a dedicated thread, so a slow SD card never
//! stalls the async runtime. Recording never fails the change it records: if the file cannot be written the
//! entry stays in memory and a warning is printed once.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    sync::{mpsc, Mutex},
};
use utoipa::ToSchema;

/// Entries kept in memory and returned by the API.
pub const KEEP: usize = 1000;
/// The current file is renamed to `audit.1.jsonl` (replacing the older one) at this size.
pub const ROTATE_BYTES: u64 = 256 * 1024;
/// Longest stored detail; longer text is cut with an ellipsis.
pub const MAX_DETAIL: usize = 600;
const FILE: &str = "audit.jsonl";
const ROTATED: &str = "audit.1.jsonl";

/// Where a change came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditSource {
    /// The admin UI or an API key over HTTP.
    Http,
    /// A command on the MQTT bridge.
    Mqtt,
    /// The gateway on its own (a lapsed lease restored).
    Gateway,
    /// The pump's answer to a register write, whoever asked for it.
    Pump,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AuditEntry {
    /// Increases by one per entry, across restarts.
    pub seq: u64,
    /// Wall-clock time (Unix ms). Before NTP sync on a Pi without RTC this can be in the past.
    pub at_ms: u64,
    pub source: AuditSource,
    /// Who asked: `admin`, an API key's name, `mqtt`, `gateway` or `pump`. Never a credential.
    pub actor: String,
    /// What was changed, in words ("Register write", "MQTT settings changed").
    pub action: String,
    /// The register or object concerned, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The request with secrets removed, or what the gateway did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub ok: bool,
    /// HTTP status and error message, MQTT result, or pump write state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// An entry to record; the log assigns `seq` and `at_ms`.
#[derive(Debug, Clone)]
pub struct NewAuditEntry {
    pub source: AuditSource,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub detail: Option<String>,
    pub ok: bool,
    pub outcome: Option<String>,
}

pub struct AuditLog {
    /// To the file writer thread; `None` for a memory-only log.
    writer: Option<Mutex<mpsc::Sender<Message>>>,
    inner: Mutex<Inner>,
}
struct Inner {
    entries: VecDeque<AuditEntry>,
    next_seq: u64,
}
enum Message {
    Entry(AuditEntry),
    Flush(mpsc::Sender<()>),
}

impl AuditLog {
    /// A log that is not saved (tests, simulation without a data directory).
    pub fn memory() -> Self {
        Self {
            writer: None,
            inner: Mutex::new(Inner {
                entries: VecDeque::new(),
                next_seq: 1,
            }),
        }
    }
    /// Loads the newest `KEEP` entries saved in `dir`. Unreadable lines (a torn last line after a power cut)
    /// are skipped.
    pub fn open(dir: &Path) -> Self {
        let mut entries = VecDeque::new();
        for name in [ROTATED, FILE] {
            let Ok(file) = File::open(dir.join(name)) else {
                continue;
            };
            for line in BufReader::new(file).lines() {
                let Ok(line) = line else { break };
                if let Ok(entry) = serde_json::from_str::<AuditEntry>(&line) {
                    entries.push_back(entry);
                    if entries.len() > KEEP {
                        entries.pop_front();
                    }
                }
            }
        }
        let next_seq = entries.back().map_or(1, |e| e.seq + 1);
        end_torn_line(&dir.join(FILE));
        let mut file_bytes = std::fs::metadata(dir.join(FILE)).map_or(0, |m| m.len());
        let (tx, rx) = mpsc::channel::<Message>();
        let dir = dir.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("audit-log".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                let mut warned = false;
                for message in rx {
                    match message {
                        Message::Entry(entry) => {
                            if let Err(error) = append(&dir, &entry, &mut file_bytes) {
                                if !warned {
                                    warned = true;
                                    eprintln!("audit log not saved ({error}); entries stay in memory until restart");
                                }
                            }
                        }
                        Message::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            });
        if let Err(error) = &spawned {
            eprintln!(
                "audit log writer not started ({error}); entries stay in memory until restart"
            );
        }
        Self {
            writer: spawned.ok().map(|_| Mutex::new(tx)),
            inner: Mutex::new(Inner { entries, next_seq }),
        }
    }
    /// Waits until every entry recorded so far is written (shutdown, tests).
    pub fn flush(&self) {
        let Some(writer) = &self.writer else { return };
        let (done, wait) = mpsc::channel();
        if writer.lock().unwrap().send(Message::Flush(done)).is_ok() {
            let _ = wait.recv();
        }
    }
    pub fn record(&self, new: NewAuditEntry) -> AuditEntry {
        let mut inner = self.inner.lock().unwrap();
        let entry = AuditEntry {
            seq: inner.next_seq,
            at_ms: now_ms(),
            source: new.source,
            actor: clip(new.actor, 80),
            action: clip(new.action, 120),
            target: new.target.map(|t| clip(t, 80)),
            detail: new.detail.map(|d| clip(d, MAX_DETAIL)),
            ok: new.ok,
            outcome: new.outcome.map(|o| clip(o, MAX_DETAIL)),
        };
        inner.next_seq += 1;
        inner.entries.push_back(entry.clone());
        if inner.entries.len() > KEEP {
            inner.entries.pop_front();
        }
        if let Some(writer) = &self.writer {
            // Sent under the entries lock, so the file keeps the order of `seq`.
            let _ = writer.lock().unwrap().send(Message::Entry(entry.clone()));
        }
        entry
    }
    /// Up to `limit` entries older than `before` (all when `None`), newest first, and whether older ones remain.
    pub fn page(&self, before: Option<u64>, limit: usize) -> (Vec<AuditEntry>, bool) {
        let inner = self.inner.lock().unwrap();
        let mut older = inner
            .entries
            .iter()
            .rev()
            .filter(|e| before.is_none_or(|b| e.seq < b));
        let page: Vec<_> = older.by_ref().take(limit).cloned().collect();
        let more = older.next().is_some();
        (page, more)
    }
}

fn append(dir: &Path, entry: &AuditEntry, file_bytes: &mut u64) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(entry).map_err(std::io::Error::other)?;
    line.push(b'\n');
    if *file_bytes > 0 && *file_bytes + line.len() as u64 > ROTATE_BYTES {
        std::fs::rename(dir.join(FILE), dir.join(ROTATED))?;
        *file_bytes = 0;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(dir.join(FILE))?;
    file.write_all(&line)?;
    *file_bytes += line.len() as u64;
    Ok(())
}

/// A power cut can leave a partial last line; end it so the next entry starts on its own line.
fn end_torn_line(path: &Path) {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = OpenOptions::new().read(true).append(true).open(path) else {
        return;
    };
    let mut last = [0u8];
    if file.seek(SeekFrom::End(-1)).is_ok()
        && file.read_exact(&mut last).is_ok()
        && last[0] != b'\n'
    {
        let _ = file.write_all(b"\n");
    }
}

fn clip(mut text: String, max: usize) -> String {
    if text.chars().count() > max {
        text = text.chars().take(max.saturating_sub(1)).collect();
        text.push('…');
    }
    text
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn change(action: &str) -> NewAuditEntry {
        NewAuditEntry {
            source: AuditSource::Http,
            actor: "admin".into(),
            action: action.into(),
            target: Some("47011".into()),
            detail: Some("x".repeat(2000)),
            ok: true,
            outcome: None,
        }
    }
    #[test]
    fn survives_restart_pages_newest_first_and_clips() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path());
        for i in 0..5 {
            log.record(change(&format!("change {i}")));
        }
        log.flush();
        let reopened = AuditLog::open(dir.path());
        let (page, more) = reopened.page(None, 2);
        assert_eq!(page.iter().map(|e| e.seq).collect::<Vec<_>>(), [5, 4]);
        assert!(more);
        assert_eq!(page[0].action, "change 4");
        assert_eq!(page[0].detail.as_ref().unwrap().chars().count(), MAX_DETAIL);
        let (rest, more) = reopened.page(Some(4), 10);
        assert_eq!(rest.iter().map(|e| e.seq).collect::<Vec<_>>(), [3, 2, 1]);
        assert!(!more);
        assert_eq!(reopened.record(change("next")).seq, 6);
    }
    #[test]
    fn rotates_once_and_skips_a_torn_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path());
        // Each entry is about 700 bytes, so 1000 entries rotate at least once.
        for i in 0..1000 {
            log.record(change(&format!("change {i}")));
        }
        log.flush();
        assert!(dir.path().join(ROTATED).exists());
        assert!(std::fs::metadata(dir.path().join(FILE)).unwrap().len() <= ROTATE_BYTES);
        let mut file = OpenOptions::new()
            .append(true)
            .open(dir.path().join(FILE))
            .unwrap();
        file.write_all(b"{\"seq\": 99").unwrap();
        let reopened = AuditLog::open(dir.path());
        let (page, _) = reopened.page(None, 1);
        assert_eq!(page[0].seq, 1000);
        // The torn line is ended, so the next entry survives another restart.
        reopened.record(change("after the cut"));
        reopened.flush();
        let (page, _) = AuditLog::open(dir.path()).page(None, 1);
        assert_eq!(
            (page[0].seq, page[0].action.as_str()),
            (1001, "after the cut")
        );
        // Both files together still reach back past the rotation.
        let (all, _) = reopened.page(None, KEEP);
        assert!(all.len() > 300, "{}", all.len());
    }
}
