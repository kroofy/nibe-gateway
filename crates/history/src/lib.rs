//! On-device history in SQLite (ADR 0114): raw samples for 14 days, 15-minute and hourly aggregates for
//! 13 months and 10 years. One writer thread commits buffered samples once per flush interval; queries
//! open their own read-only connection (WAL allows them alongside the writer).
mod recorder;
mod store;

pub use recorder::{clock_synced, spawn_recorder};

pub use store::{Point, Query, Resolution, Series, Store, StoreError};

use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

pub fn sqlite_version() -> &'static str {
    rusqlite::version()
}

/// How a register aggregates: gauges by mean/min/max, counters by last value and change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Gauge,
    Counter,
}
impl Kind {
    /// Energy, hours and start counters only grow; everything else is a gauge.
    pub fn from_unit(unit: Option<&str>, name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        if matches!(unit, Some("kWh" | "Wh" | "MWh" | "h"))
            || name.contains("starts")
            || name.contains("op.time")
            || name.contains("operating time")
        {
            Kind::Counter
        } else {
            Kind::Gauge
        }
    }
}

#[derive(Clone, Debug)]
pub struct RegisterInfo {
    pub id: u32,
    pub name: String,
    pub unit: Option<String>,
    pub scale: f64,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub register: u32,
    /// Unix seconds.
    pub t: i64,
    /// Raw register value; the scale is applied on query.
    pub raw: i64,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub dir: PathBuf,
    pub flush: Duration,
    pub raw_days: i64,
    pub m15_days: i64,
    pub h1_days: i64,
    /// Database size above which the oldest raw days are deleted early.
    pub max_bytes: u64,
}
impl Config {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            flush: Duration::from_secs(60),
            raw_days: 14,
            m15_days: 396,
            h1_days: 3653,
            max_bytes: 160 * 1024 * 1024,
        }
    }
    pub fn path(&self) -> PathBuf {
        self.dir.join("history.sqlite3")
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    pub enabled: bool,
    pub path: Option<String>,
    pub size_bytes: u64,
    pub last_flush_ms: Option<u64>,
    pub last_flush_duration_ms: Option<u64>,
    pub samples_written: u64,
    /// Samples dropped because the clock was not trustworthy or went backwards.
    pub samples_dropped_clock: u64,
    pub buffered: usize,
    pub clock_synced: bool,
    pub last_error: Option<String>,
}

enum Message {
    Samples(Vec<Sample>),
    Registers(Vec<RegisterInfo>),
    Flush(mpsc::Sender<()>),
    Erase(mpsc::Sender<Result<(), String>>),
    Stop,
}

/// Handle to the writer thread. Cloneable; dropping all handles does not stop the thread — call `stop`.
#[derive(Clone)]
pub struct History {
    tx: mpsc::SyncSender<Message>,
    status: Arc<Mutex<Status>>,
    config: Arc<Config>,
}

/// Decides whether a sample time can be trusted (ADR 0114 owner rule): with NTP synced always; without,
/// only moving forward from the last stored time and within `MAX_UNSYNCED_GAP_S` of it.
pub const MAX_UNSYNCED_GAP_S: i64 = 6 * 3600;
/// Registers are read at different moments and batched every 2 s, so a sample may be slightly older than
/// the newest stored one; only steps further back than this count as the clock going backwards.
pub const CLOCK_SLACK_S: i64 = 120;
pub fn time_trusted(t: i64, synced: bool, last_stored: Option<i64>) -> bool {
    match (synced, last_stored) {
        (true, Some(last)) => t >= last,
        (true, None) => true,
        (false, Some(last)) => t >= last && t - last <= MAX_UNSYNCED_GAP_S,
        (false, None) => false,
    }
}

impl History {
    pub fn open(config: Config, clock_synced: fn() -> bool) -> Result<Self, StoreError> {
        std::fs::create_dir_all(&config.dir).map_err(|e| StoreError(e.to_string()))?;
        let store = Store::open(&config.path())?;
        let status = Arc::new(Mutex::new(Status {
            enabled: true,
            path: Some(config.path().display().to_string()),
            ..Status::default()
        }));
        let (tx, rx) = mpsc::sync_channel(64);
        let config = Arc::new(config);
        let thread_status = status.clone();
        let thread_config = config.clone();
        std::thread::Builder::new()
            .name("history-writer".into())
            .spawn(move || writer(store, rx, thread_status, thread_config, clock_synced))
            .map_err(|e| StoreError(e.to_string()))?;
        Ok(Self { tx, status, config })
    }
    /// Never blocks the caller: a full channel drops the batch and counts it.
    pub fn record(&self, samples: Vec<Sample>) {
        if samples.is_empty() {
            return;
        }
        let n = samples.len() as u64;
        if self.tx.try_send(Message::Samples(samples)).is_err() {
            self.status.lock().unwrap().last_error =
                Some(format!("writer busy; dropped {n} samples"));
        }
    }
    pub fn registers(&self, registers: Vec<RegisterInfo>) {
        let _ = self.tx.try_send(Message::Registers(registers));
    }
    /// Commit the buffer now (shutdown, tests). Waits up to 10 s.
    pub fn flush(&self) {
        let (done, wait) = mpsc::channel();
        if self.tx.send(Message::Flush(done)).is_ok() {
            let _ = wait.recv_timeout(Duration::from_secs(10));
        }
    }
    /// Delete all history (factory reset without "keep history").
    pub fn erase(&self) -> Result<(), String> {
        let (done, wait) = mpsc::channel();
        self.tx
            .send(Message::Erase(done))
            .map_err(|_| "history writer stopped".to_string())?;
        wait.recv_timeout(Duration::from_secs(30))
            .map_err(|_| "history erase timed out".to_string())?
    }
    pub fn stop(&self) {
        self.flush();
        let _ = self.tx.send(Message::Stop);
    }
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
    /// Runs on the caller's thread with its own read-only connection; use `spawn_blocking` from async code.
    pub fn query(&self, query: &Query) -> Result<Vec<Series>, StoreError> {
        Store::open_read_only(&self.config.path())?.query(query)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn writer(
    mut store: Store,
    rx: mpsc::Receiver<Message>,
    status: Arc<Mutex<Status>>,
    config: Arc<Config>,
    clock_synced: fn() -> bool,
) {
    let mut buffer: Vec<Sample> = Vec::new();
    let mut last_stored = store.last_time().ok().flatten();
    let mut next_flush = Instant::now() + config.flush;
    let mut next_retention = Instant::now() + Duration::from_secs(120);
    // Rebuild aggregates that a crash may have left behind.
    if let Err(e) = store.rollup_since(last_stored.map_or(0, |t| t - 2 * 3600)) {
        status.lock().unwrap().last_error = Some(e.0);
    }
    loop {
        let wait = next_flush.saturating_duration_since(Instant::now());
        let message = match rx.recv_timeout(wait) {
            Ok(m) => Some(m),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => Some(Message::Stop),
        };
        let mut flush_now = message.is_none();
        let mut ack = None;
        let mut stop = false;
        match message {
            Some(Message::Samples(samples)) => {
                let synced = clock_synced();
                let mut dropped = 0;
                for s in samples {
                    let newest = last_stored.max(buffer.iter().map(|b| b.t).max());
                    if time_trusted(s.t, synced, newest.map(|t| t - CLOCK_SLACK_S)) {
                        buffer.push(s);
                    } else {
                        dropped += 1;
                    }
                }
                let mut st = status.lock().unwrap();
                st.samples_dropped_clock += dropped;
                st.buffered = buffer.len();
                st.clock_synced = synced;
            }
            Some(Message::Registers(registers)) => {
                if let Err(e) = store.upsert_registers(&registers) {
                    status.lock().unwrap().last_error = Some(e.0);
                }
            }
            Some(Message::Flush(done)) => {
                flush_now = true;
                ack = Some(done);
            }
            Some(Message::Erase(done)) => {
                buffer.clear();
                let result = store.erase().map_err(|e| e.0);
                last_stored = None;
                let _ = done.send(result);
            }
            Some(Message::Stop) => {
                flush_now = true;
                stop = true;
            }
            None => {}
        }
        if flush_now {
            next_flush = Instant::now() + config.flush;
            if !buffer.is_empty() {
                let started = Instant::now();
                let result = store.append(&buffer);
                let mut st = status.lock().unwrap();
                match result {
                    Ok(()) => {
                        last_stored = buffer.iter().map(|s| s.t).max().max(last_stored);
                        st.samples_written += buffer.len() as u64;
                        st.last_flush_ms = Some(now_ms());
                        st.last_flush_duration_ms = Some(started.elapsed().as_millis() as u64);
                        st.last_error = None;
                        buffer.clear();
                    }
                    Err(e) => {
                        st.last_error = Some(e.0);
                        // Keep at most one hour of samples while the disk misbehaves.
                        if buffer.len() > 200_000 {
                            buffer.drain(..buffer.len() - 200_000);
                        }
                    }
                }
                st.buffered = buffer.len();
                st.size_bytes = store.size_bytes();
            }
            if let Some(done) = ack {
                let _ = done.send(());
            }
        }
        if Instant::now() >= next_retention {
            next_retention = Instant::now() + Duration::from_secs(3600);
            let now = (now_ms() / 1000) as i64;
            if let Err(e) = store.retain(now, &config) {
                status.lock().unwrap().last_error = Some(e.0);
            }
            status.lock().unwrap().size_bytes = store.size_bytes();
        }
        if stop {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_rule_follows_the_owner_decision() {
        assert!(time_trusted(100, true, None));
        assert!(!time_trusted(99, true, Some(100)), "never backwards");
        assert!(
            !time_trusted(1_000, false, None),
            "unsynced without history: wait for NTP"
        );
        assert!(
            time_trusted(200, false, Some(100)),
            "unsynced, short gap after a reboot"
        );
        assert!(!time_trusted(
            100 + MAX_UNSYNCED_GAP_S + 1,
            false,
            Some(100)
        ));
    }
    #[test]
    fn writer_thread_buffers_commits_and_answers_queries() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::new(dir.path().to_path_buf());
        config.flush = Duration::from_secs(3600); // only explicit flushes in this test
        let h = History::open(config, || true).unwrap();
        h.registers(vec![RegisterInfo {
            id: 43416,
            name: "Compressor starts".into(),
            unit: None,
            scale: 1.0,
            kind: Kind::Counter,
        }]);
        let at = 1_792_000_000;
        h.record(vec![
            Sample {
                register: 43416,
                t: at,
                raw: 100,
            },
            Sample {
                register: 43416,
                t: at + 30,
                raw: 101,
            },
        ]);
        h.record(vec![Sample {
            register: 43416,
            t: at - 5,
            raw: 99,
        }]); // read slightly earlier than the newest: kept
        h.record(vec![Sample {
            register: 43416,
            t: at - 1000,
            raw: 98,
        }]); // clock went back: dropped
        assert!(
            h.query(&query(at)).unwrap()[0].points.is_empty(),
            "nothing before a flush"
        );
        h.flush();
        let series = h.query(&query(at)).unwrap();
        assert_eq!(series[0].kind, "counter");
        assert_eq!(series[0].points.len(), 3);
        let status = h.status();
        assert_eq!(
            (status.samples_written, status.samples_dropped_clock),
            (3, 1)
        );
        h.erase().unwrap();
        assert!(h.query(&query(at)).unwrap()[0].points.is_empty());
        h.stop();
    }
    fn query(at: i64) -> Query {
        Query {
            registers: vec![43416],
            from: at - 60,
            to: at + 60,
            resolution: Resolution::Raw,
            max_points: 100,
        }
    }
    #[test]
    fn kinds_from_units_and_names() {
        assert_eq!(Kind::from_unit(Some("kWh"), "x"), Kind::Counter);
        assert_eq!(
            Kind::from_unit(None, "Compressor starts EB100"),
            Kind::Counter
        );
        assert_eq!(Kind::from_unit(Some("°C"), "BT1"), Kind::Gauge);
    }
}
