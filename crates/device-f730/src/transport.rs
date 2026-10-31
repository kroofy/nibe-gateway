use crate::decode::hex;
use nibe_core::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{BufRead, Read, Write},
    path::Path,
    time::Duration,
};

pub const BUNDLED_CAPTURE: &str =
    include_str!("../../../fixtures/serial-captures/synthetic-f730-session.capture.jsonl");
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureHeader {
    pub format: String,
    pub version: u32,
    pub synthetic: bool,
    pub device: String,
    pub serial: String,
    pub provenance: String,
    #[serde(flatten)]
    pub metadata: std::collections::BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureRecord {
    pub t_us: u64,
    pub dir: String,
    pub hex: String,
}
impl CaptureRecord {
    pub fn bytes(&self) -> Result<Vec<u8>, ServiceError> {
        if self.hex.len() > 8192 || self.hex.len() % 2 != 0 || !self.hex.is_ascii() {
            return Err(ServiceError::validation("invalid or oversized capture hex"));
        }
        (0..self.hex.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&self.hex[i..i + 2], 16)
                    .map_err(|_| ServiceError::validation("invalid capture hex"))
            })
            .collect()
    }
}
#[derive(Debug, Clone)]
pub struct Capture {
    pub header: CaptureHeader,
    pub records: Vec<CaptureRecord>,
}
impl Capture {
    pub fn read(reader: impl BufRead) -> Result<Self, ServiceError> {
        let mut reader = reader;
        let header_line =
            read_line(&mut reader)?.ok_or_else(|| ServiceError::validation("empty capture"))?;
        let header: CaptureHeader = serde_json::from_str(&header_line)
            .map_err(|e| ServiceError::validation(e.to_string()))?;
        if header.format != "nibe-gateway/capture-jsonl"
            || header.version != 1
            || header.serial != "9600 8N1"
        {
            return Err(ServiceError::validation("unsupported capture header"));
        }
        let mut total = header_line.len();
        let mut records = vec![];
        let mut previous = 0;
        while let Some(line) = read_line(&mut reader)? {
            total += line.len();
            if total > 8 * 1024 * 1024 || records.len() >= 65536 {
                return Err(ServiceError::validation("capture size limit exceeded"));
            }
            let r: CaptureRecord =
                serde_json::from_str(&line).map_err(|e| ServiceError::validation(e.to_string()))?;
            if r.t_us < previous || !matches!(r.dir.as_str(), "rx" | "tx") {
                return Err(ServiceError::validation(
                    "capture timestamps must be monotonic and direction rx/tx",
                ));
            }
            r.bytes()?;
            previous = r.t_us;
            records.push(r);
        }
        Ok(Self { header, records })
    }
    pub fn open(path: &Path) -> Result<Self, ServiceError> {
        Self::read(std::io::BufReader::new(
            std::fs::File::open(path).map_err(io_error)?,
        ))
    }
    pub fn bundled() -> Self {
        Self::read(BUNDLED_CAPTURE.as_bytes()).expect("checked-in capture")
    }
    pub fn export(&self, mut writer: impl Write) -> Result<(), ServiceError> {
        serde_json::to_writer(&mut writer, &self.header)
            .map_err(|e| ServiceError::validation(e.to_string()))?;
        writer.write_all(b"\n").map_err(io_error)?;
        for r in &self.records {
            serde_json::to_writer(&mut writer, r)
                .map_err(|e| ServiceError::validation(e.to_string()))?;
            writer.write_all(b"\n").map_err(io_error)?;
        }
        writer.flush().map_err(io_error)
    }
}
fn read_line(reader: &mut impl BufRead) -> Result<Option<String>, ServiceError> {
    let mut line = String::new();
    let n = reader
        .by_ref()
        .take(16385)
        .read_line(&mut line)
        .map_err(io_error)?;
    if n > 16384 {
        return Err(ServiceError::validation("capture line exceeds 16 KiB"));
    }
    Ok((n != 0).then_some(line))
}
pub fn io_error(e: std::io::Error) -> ServiceError {
    ServiceError::new(ErrorCode::Disconnected, e.to_string())
}
/// Replays receive chunks at recorded relative times. Recorded TX is evidence only;
/// it is never injected as incoming pump traffic or sent to a physical port.
pub struct ReplayTransport {
    rx: VecDeque<(u64, Vec<u8>)>,
    start: tokio::time::Instant,
}
impl ReplayTransport {
    pub fn new(capture: &Capture) -> Result<Self, ServiceError> {
        let mut rx = VecDeque::new();
        for r in &capture.records {
            if r.dir == "rx" {
                rx.push_back((r.t_us, r.bytes()?));
            }
        }
        Ok(Self {
            rx,
            start: tokio::time::Instant::now(),
        })
    }
}
impl DeviceTransport for ReplayTransport {
    fn receive(&mut self) -> DeviceFuture<'_, Vec<u8>> {
        Box::pin(async move {
            let Some((at, bytes)) = self.rx.front() else {
                std::future::pending::<()>().await;
                unreachable!()
            };
            tokio::time::sleep_until(self.start + Duration::from_micros(*at)).await;
            let bytes = bytes.clone();
            self.rx.pop_front();
            Ok(bytes)
        })
    }
    fn send<'a>(&'a mut self, _: &'a [u8]) -> DeviceFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

#[cfg(feature = "serial")]
pub struct SerialTransport(tokio_serial::SerialStream);
#[cfg(feature = "serial")]
impl SerialTransport {
    pub fn open(path: &str) -> Result<Self, ServiceError> {
        use tokio_serial::SerialPortBuilderExt;
        let mut stream = tokio_serial::new(path, 9600)
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .flow_control(tokio_serial::FlowControl::None)
            .open_native_async()
            .map_err(|e| {
                ServiceError::new(ErrorCode::Unavailable, format!("serial port {path}: {e}"))
            })?;
        stream.set_exclusive(true).map_err(|e| io_error(e.into()))?;
        Ok(Self(stream))
    }
}
#[cfg(feature = "serial")]
impl DeviceTransport for SerialTransport {
    fn receive(&mut self) -> DeviceFuture<'_, Vec<u8>> {
        Box::pin(async move {
            use tokio::io::AsyncReadExt;
            let mut bytes = [0; 256];
            let n = AsyncReadExt::read(&mut self.0, &mut bytes)
                .await
                .map_err(io_error)?;
            if n == 0 {
                return Err(ServiceError::new(ErrorCode::Disconnected, "serial EOF"));
            }
            Ok(bytes[..n].to_vec())
        })
    }
    fn send<'a>(&'a mut self, frame: &'a [u8]) -> DeviceFuture<'a, ()> {
        Box::pin(async move {
            use tokio::io::AsyncWriteExt;
            AsyncWriteExt::write_all(&mut self.0, frame)
                .await
                .map_err(io_error)
        })
    }
}

/// Best-effort bounded recorder. Called only after replies have been submitted.
/// A separate disk thread writes JSONL; loss/failure counters are exposed live
/// and persisted in <capture>.summary.json on shutdown. Drop drains this thread
/// after wire servicing stops; it never waits for disk during an ACK.
pub struct Recorder {
    tx: Option<std::sync::mpsc::SyncSender<CaptureRecord>>,
    pub dropped: std::sync::Arc<std::sync::atomic::AtomicU64>,
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    task: Option<std::thread::JoinHandle<()>>,
}
impl Recorder {
    pub fn create(path: &Path, header: CaptureHeader) -> Result<Self, ServiceError> {
        let private_file = |path: &Path| -> std::io::Result<std::fs::File> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(path)
        };
        let mut writer = std::io::BufWriter::new(private_file(path).map_err(io_error)?);
        let mut summary_name = path.as_os_str().to_os_string();
        summary_name.push(".summary.json");
        let mut summary = private_file(Path::new(&summary_name)).map_err(io_error)?;
        // Remains incomplete if the process is killed before graceful drain.
        summary
            .write_all(b"{\"complete\":false}\n")
            .map_err(io_error)?;
        serde_json::to_writer(&mut writer, &header)
            .map_err(|e| ServiceError::validation(e.to_string()))?;
        writer.write_all(b"\n").map_err(io_error)?;
        writer.flush().map_err(io_error)?;
        let (tx, rx) = std::sync::mpsc::sync_channel::<CaptureRecord>(256);
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let disk_failed = failed.clone();
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let disk_dropped = dropped.clone();
        let task=std::thread::Builder::new()
            .name("nibe-capture-disk".into())
            .spawn(move || {
                let mut written=0u64;
                for r in rx {
                    if !disk_failed.load(std::sync::atomic::Ordering::Relaxed) {
                        if serde_json::to_writer(&mut writer,&r).is_err() || writer.write_all(b"\n").is_err() || writer.flush().is_err() {
                            disk_failed.store(true,std::sync::atomic::Ordering::Relaxed);
                        } else {written+=1;}
                    }
                }
                let lost=disk_dropped.load(std::sync::atomic::Ordering::Relaxed);
                let failed=disk_failed.load(std::sync::atomic::Ordering::Relaxed);
                let status=serde_json::json!({"complete":lost==0 && !failed,"dropped":lost,"disk_failed":failed,"records_written":written});
                use std::io::{Seek,SeekFrom};
                let result=summary.seek(SeekFrom::Start(0)).and_then(|_|summary.set_len(0)).and_then(|_|{
                    serde_json::to_writer(&mut summary,&status).map_err(std::io::Error::other)
                }).and_then(|_|summary.flush());
                if result.is_err(){disk_failed.store(true,std::sync::atomic::Ordering::Relaxed);}
            })
            .map_err(io_error)?;
        Ok(Self {
            tx: Some(tx),
            dropped,
            failed,
            task: Some(task),
        })
    }
    pub fn record(&self, t_us: u64, dir: &str, bytes: &[u8]) {
        if self.tx.as_ref().is_some_and(|tx| {
            tx.try_send(CaptureRecord {
                t_us,
                dir: dir.into(),
                hex: hex(bytes),
            })
            .is_err()
        }) {
            self.dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    pub fn failed(&self) -> bool {
        self.failed.load(std::sync::atomic::Ordering::Relaxed)
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_roundtrip_and_validation() {
        let c = Capture::bundled();
        let mut bytes = vec![];
        c.export(&mut bytes).unwrap();
        let c2 = Capture::read(bytes.as_slice()).unwrap();
        assert_eq!(c2.records.len(), c.records.len());
        for (a, b) in c.records.iter().zip(c2.records.iter()) {
            assert_eq!(a.bytes().unwrap(), b.bytes().unwrap());
            assert_eq!(a.t_us, b.t_us);
        }
        assert!(CaptureRecord {
            t_us: 0,
            dir: "rx".into(),
            hex: "åå".into()
        }
        .bytes()
        .is_err());
        assert!(Capture::read(b"{}".as_slice()).is_err());
    }
}
