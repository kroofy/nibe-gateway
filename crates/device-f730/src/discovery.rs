use crate::catalog::Catalog;
use nibe_core::{DeviceMode, ErrorCode, ServiceError};
use nibe_protocol::{read_request, ReadRequest};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use utoipa::ToSchema;

/// The F730 offers a read token about every 1.02 s; a 1000 ms floor made jitter skip every other token
/// (one read per ~2.8 s). 800 ms takes each token while still allowing at most one read per token.
pub const MIN_PROBE_INTERVAL_MS: u64 = 800;
pub const READ_TIMEOUT_MS: u64 = 5000;
pub const MAX_PROBES: usize = 32;
pub const MAX_POLL: usize = 64;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RegisterState {
    CatalogKnown,
    Observed,
    ConfirmedReadable,
    Unsupported,
    Unverified,
    Unknown,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ProbeStatus {
    pub running: bool,
    pub queued: usize,
    pub outstanding: Option<u16>,
    pub completed: usize,
    pub cancelled: bool,
    pub states: BTreeMap<u16, RegisterState>,
    pub detail: String,
    #[serde(default)]
    pub poll: PollStatus,
    #[serde(default)]
    pub writes: crate::writes::WriteStatus,
}
/// Continuous background reads (ADR 0102): one register per free read token, round-robin.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct PollStatus {
    pub registers: Vec<u16>,
    pub reads: u64,
    pub timeouts: u64,
    /// Complete passes over the poll list in which every read was answered (update probation, ADR 0109).
    #[serde(default)]
    pub rounds_ok: u64,
    /// Read about every `slow_interval_s` (counters, settings); they never count toward rounds.
    #[serde(default)]
    pub slow_registers: Vec<u16>,
    #[serde(default)]
    pub slow_interval_s: u64,
}
/// Owns one outstanding read. API authentication is mandatory before start();
/// hardware mode rejects every start unless the operator opted in (ADR 0101).
pub struct ProbeEngine {
    pub catalog: Arc<Catalog>,
    mode: DeviceMode,
    hardware_reads: bool,
    queue: VecDeque<u16>,
    pending: Option<(u16, u64, u8)>,
    /// The outstanding read belongs to the poll list, not to a probe session.
    polling: bool,
    poll_cursor: usize,
    /// Read-back after a write, served before probes and polls.
    readback: VecDeque<u16>,
    /// The outstanding read is a poll-list read that counts toward a round.
    counting: bool,
    round_reads: usize,
    round_timeouts: usize,
    last_sent: Option<u64>,
    /// When each slow register was last asked for.
    slow_last: BTreeMap<u16, u64>,
    /// Fast poll reads since the last probe or slow read; they take turns so neither starves.
    fast_streak: u32,
    pub status: ProbeStatus,
}
impl ProbeEngine {
    pub fn new(catalog: Arc<Catalog>, mode: DeviceMode, hardware_reads: bool) -> Self {
        Self {
            catalog,
            mode,
            hardware_reads,
            queue: VecDeque::new(),
            pending: None,
            polling: false,
            poll_cursor: 0,
            readback: VecDeque::new(),
            counting: false,
            round_reads: 0,
            round_timeouts: 0,
            last_sent: None,
            slow_last: BTreeMap::new(),
            fast_streak: 0,
            status: ProbeStatus::default(),
        }
    }
    pub fn start(&mut self, ids: &[u16], authenticated: bool) -> Result<ProbeStatus, ServiceError> {
        if !authenticated {
            return Err(ServiceError::new(
                ErrorCode::Forbidden,
                "authenticated discovery capability required",
            ));
        }
        self.reads_allowed()?;
        if self.status.running {
            return Err(ServiceError::new(
                ErrorCode::Busy,
                "a probe session is already running",
            ));
        }
        if ids.is_empty() || ids.len() > MAX_PROBES {
            return Err(ServiceError::validation(
                "provide 1..32 explicit catalog IDs; no address scans",
            ));
        }
        self.validate_ids(ids)?;
        self.queue = ids.iter().copied().collect();
        let poll = std::mem::take(&mut self.status.poll);
        let writes = std::mem::take(&mut self.status.writes);
        self.status=ProbeStatus{running:true,queued:ids.len(),detail:"read-only; one per token, >=1s spacing, 5s timeout, at most 3 attempts; unsupported is a timeout hypothesis".into(),poll,writes,..ProbeStatus::default()};
        Ok(self.status.clone())
    }
    /// Configure the continuous poll list. Same rules as probes; probe sessions take turns with it.
    pub fn set_poll(&mut self, ids: &[u16]) -> Result<(), ServiceError> {
        self.set_polls(ids, &[], 0)
    }
    /// Fast registers are read round-robin; slow ones about every `slow_interval_s` (at most one in two
    /// reads). Together at most 64; a register listed in both stays fast.
    pub fn set_polls(
        &mut self,
        fast: &[u16],
        slow: &[u16],
        slow_interval_s: u64,
    ) -> Result<(), ServiceError> {
        let slow: Vec<u16> = slow
            .iter()
            .copied()
            .filter(|id| !fast.contains(id))
            .collect();
        if !fast.is_empty() || !slow.is_empty() {
            self.reads_allowed()?;
            if fast.len() + slow.len() > MAX_POLL {
                return Err(ServiceError::validation(
                    "at most 64 poll registers in total",
                ));
            }
            for ids in [fast, &slow[..]] {
                if !ids.is_empty() {
                    self.validate_ids(ids)?;
                }
            }
            if !slow.is_empty() && !(60..=86_400).contains(&slow_interval_s) {
                return Err(ServiceError::validation(
                    "slow poll interval must be 60..=86400 s",
                ));
            }
        }
        self.status.poll.registers = fast.to_vec();
        self.status.poll.slow_registers = slow;
        self.status.poll.slow_interval_s = slow_interval_s;
        self.slow_last.clear();
        self.poll_cursor = 0;
        Ok(())
    }
    fn slow_due(&self, now_ms: u64) -> Option<u16> {
        let interval = self.status.poll.slow_interval_s * 1000;
        self.status.poll.slow_registers.iter().copied().find(|id| {
            self.slow_last
                .get(id)
                .is_none_or(|t| now_ms.saturating_sub(*t) >= interval)
        })
    }
    /// Schedule a read of a just-written register; bypasses the probe allowlist check (already validated).
    pub fn readback(&mut self, id: u16) {
        if !self.readback.contains(&id) {
            self.readback.push_back(id);
        }
    }
    fn reads_allowed(&self) -> Result<(), ServiceError> {
        if self.mode == DeviceMode::Hardware && !self.hardware_reads {
            return Err(ServiceError::new(
                ErrorCode::Forbidden,
                "hardware read requests are disabled; start the gateway with --allow-hardware-reads",
            ));
        }
        Ok(())
    }
    fn validate_ids(&self, ids: &[u16]) -> Result<(), ServiceError> {
        let mut unique = std::collections::BTreeSet::new();
        for &id in ids {
            let e = self.catalog.entry(id).ok_or_else(|| {
                ServiceError::validation(format!("register {id} is not in the catalog"))
            })?;
            if !readable(id, e) || !unique.insert(id) {
                return Err(ServiceError::validation("only distinct catalog entries are permitted, excluding never-write/alarm-reset entries other than 48852"));
            }
            read_request(id)
                .map_err(|e| ServiceError::validation(format!("unsafe read request {id}: {e}")))?;
        }
        Ok(())
    }
    pub fn cancel(&mut self) -> ProbeStatus {
        self.queue.clear();
        self.pending = None;
        self.polling = false;
        self.status.running = false;
        self.status.cancelled = true;
        self.sync();
        self.status.clone()
    }
    fn sync(&mut self) {
        let probe_pending = self.pending.filter(|_| !self.polling);
        self.status.queued = self.queue.len();
        self.status.outstanding = probe_pending.map(|p| p.0);
        self.status.running = !self.queue.is_empty() || probe_pending.is_some();
    }
    pub fn on_token(&mut self, now_ms: u64) -> Option<ReadRequest> {
        self.expire(now_ms);
        if self
            .last_sent
            .is_some_and(|t| now_ms.saturating_sub(t) < MIN_PROBE_INTERVAL_MS)
        {
            return None;
        }
        if let Some((id, _, attempts)) = self.pending {
            // A zero timestamp marker schedules a retry; never send an in-flight read again.
            if self.pending.is_some_and(|p| p.1 != u64::MAX) {
                return None;
            }
            self.pending = Some((id, now_ms, attempts + 1));
            self.last_sent = Some(now_ms);
            self.sync();
            return read_request(id).ok();
        }
        self.counting = false;
        let id = match self.readback.pop_front() {
            Some(id) => {
                self.polling = true;
                id
            }
            None => {
                // Probes and due slow registers alternate with fast reads, so a probe session no longer
                // stalls live values and live values never stall a probe.
                let fast_empty = self.status.poll.registers.is_empty();
                let other_turn = fast_empty || self.fast_streak >= 1;
                let other = if other_turn {
                    if let Some(id) = self.queue.pop_front() {
                        Some((id, false))
                    } else {
                        self.slow_due(now_ms).map(|id| (id, true))
                    }
                } else {
                    None
                };
                match other {
                    Some((id, slow)) => {
                        self.fast_streak = 0;
                        self.polling = slow;
                        if slow {
                            self.slow_last.insert(id, now_ms);
                        }
                        id
                    }
                    None => {
                        let poll = &self.status.poll.registers;
                        let id = *poll.get(self.poll_cursor % poll.len().max(1))?;
                        self.poll_cursor = (self.poll_cursor + 1) % poll.len();
                        self.polling = true;
                        self.counting = true;
                        self.fast_streak += 1;
                        id
                    }
                }
            }
        };
        self.pending = Some((id, now_ms, 1));
        self.last_sent = Some(now_ms);
        self.sync();
        read_request(id).ok()
    }
    pub fn expire(&mut self, now_ms: u64) {
        if let Some((id, sent, attempts)) = self.pending {
            if sent != u64::MAX && now_ms.saturating_sub(sent) >= READ_TIMEOUT_MS {
                if self.polling {
                    // A missed poll read is skipped; the register comes round again.
                    self.status.poll.timeouts += 1;
                    self.pending = None;
                    self.polling = false;
                    if std::mem::take(&mut self.counting) {
                        self.round_timeouts += 1;
                        self.close_round();
                    }
                } else if attempts >= 3 {
                    self.status.states.insert(id, RegisterState::Unsupported);
                    self.status.completed += 1;
                    self.pending = None;
                } else {
                    self.pending = Some((id, u64::MAX, attempts));
                }
                self.sync();
            }
        }
    }
    /// A round ends after one attempt per poll register; it counts only if none timed out.
    fn close_round(&mut self) {
        let size = self.status.poll.registers.len();
        if size > 0 && self.round_reads + self.round_timeouts >= size {
            if self.round_timeouts == 0 {
                self.status.poll.rounds_ok += 1;
            }
            self.round_reads = 0;
            self.round_timeouts = 0;
        }
    }
    pub fn response(&mut self, id: u16) -> bool {
        if !self.pending.is_some_and(|p| p.0 == id && p.1 != u64::MAX) {
            return false;
        }
        self.pending = None;
        if std::mem::take(&mut self.polling) {
            self.status.poll.reads += 1;
            if std::mem::take(&mut self.counting) {
                self.round_reads += 1;
                self.close_round();
            }
        } else {
            self.status.completed += 1;
            self.status.states.insert(id, RegisterState::Observed);
        }
        self.sync();
        true
    }
}
/// A read request never changes the pump, so settings (`read_write`) may be read too. Entries flagged as
/// never-write or alarm-reset stay excluded as a precaution, except the word-swap setting 48852.
fn readable(id: u16, e: &crate::catalog::Entry) -> bool {
    // Reading never acts: the word-swap setting may be read, not written. The alarm-reset register
    // is only read back internally after a reset write (`readback`), never on request.
    id == 48852
        || (matches!(e.access.as_str(), "read" | "read_write")
            && !e
                .flags
                .iter()
                .any(|f| f == "never_write" || f == "alarm_reset"))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn engine(mode: DeviceMode) -> ProbeEngine {
        ProbeEngine::new(Arc::new(Catalog::bundled()), mode, false)
    }
    #[test]
    fn authentication_allowlist_and_hardware_gate() {
        let mut e = engine(DeviceMode::Replay);
        assert!(e.start(&[40004], false).is_err());
        for ids in [
            vec![1],
            vec![40043],
            vec![48968],
            vec![48732],
            vec![45171],
            vec![40004, 40004],
        ] {
            assert!(e.start(&ids, true).is_err());
        }
        assert!(engine(DeviceMode::Hardware).start(&[48852], true).is_err());
        assert!(e.start(&[48852, 40004], true).is_ok());
        // Opt-in hardware reads: settings are readable, never-write/alarm-reset entries are not.
        let mut hw = ProbeEngine::new(Arc::new(Catalog::bundled()), DeviceMode::Hardware, true);
        assert!(hw.start(&[40004], false).is_err());
        assert!(hw.start(&[45171], true).is_err());
        assert!(hw.start(&[48732], true).is_err());
        assert!(hw
            .start(&[48852, 40004, 47011, 47007, 47398, 43005], true)
            .is_ok());
    }
    #[test]
    fn poll_list_round_robins_yields_to_probes_and_skips_timeouts() {
        assert!(engine(DeviceMode::Hardware).set_poll(&[40004]).is_err());
        let mut e = ProbeEngine::new(Arc::new(Catalog::bundled()), DeviceMode::Hardware, true);
        assert!(e.set_poll(&[45171]).is_err());
        e.set_poll(&[40004, 47011]).unwrap();
        let id = |r: Option<ReadRequest>| {
            let b = r.unwrap();
            u16::from_le_bytes([b.as_bytes()[3], b.as_bytes()[4]])
        };
        assert_eq!(id(e.on_token(0)), 40004);
        assert!(!e.status.running, "polling is not a probe session");
        assert!(e.response(40004));
        assert_eq!(id(e.on_token(1000)), 47011);
        e.expire(6000); // timed out: skipped, not retried
        assert_eq!(e.status.poll.timeouts, 1);
        e.start(&[40013], true).unwrap();
        assert_eq!(id(e.on_token(7000)), 40013, "probe sessions take priority");
        assert!(e.response(40013));
        assert_eq!(id(e.on_token(8000)), 40004);
        assert!(e.response(40004));
        assert_eq!((e.status.poll.reads, e.status.completed), (2, 1));
        assert_eq!(e.status.poll.rounds_ok, 0, "the first round had a timeout");
        assert_eq!(id(e.on_token(9000)), 47011);
        assert!(e.response(47011));
        assert_eq!(e.status.poll.rounds_ok, 1, "40004 + 47011 both answered");
        e.readback(47011);
        assert_eq!(id(e.on_token(10_000)), 47011);
        assert!(e.response(47011));
        assert_eq!(
            e.status.poll.rounds_ok, 1,
            "read-backs do not count toward rounds"
        );
    }
    #[test]
    fn probes_and_slow_registers_take_turns_with_fast_polling() {
        let mut e = ProbeEngine::new(Arc::new(Catalog::bundled()), DeviceMode::Hardware, true);
        assert!(
            e.set_polls(&[40004], &[43416], 10).is_err(),
            "interval too short"
        );
        e.set_polls(&[40004, 40008], &[43416, 40004], 600).unwrap();
        assert_eq!(
            e.status.poll.slow_registers,
            [43416],
            "a fast register stays fast"
        );
        let t = std::cell::Cell::new(0u64);
        let read = |e: &mut ProbeEngine| {
            t.set(t.get() + 1000);
            let b = e.on_token(t.get()).unwrap();
            let id = u16::from_le_bytes([b.as_bytes()[3], b.as_bytes()[4]]);
            assert!(e.response(id));
            id
        };
        // The slow register is due at start, but only every other read goes to it.
        assert_eq!(read(&mut e), 40004);
        assert_eq!(read(&mut e), 43416);
        assert_eq!(read(&mut e), 40008);
        assert_eq!(
            e.status.poll.rounds_ok, 1,
            "slow reads never count toward rounds"
        );
        // Not due again for 600 s: fast reads only; a probe session alternates with them.
        assert_eq!(read(&mut e), 40004);
        assert_eq!(read(&mut e), 40008);
        e.start(&[40013, 40014], true).unwrap();
        assert_eq!(read(&mut e), 40013);
        assert_eq!(
            read(&mut e),
            40004,
            "polling continues during a probe session"
        );
        assert_eq!(read(&mut e), 40014);
        assert_eq!(read(&mut e), 40008);
        t.set(601_000);
        assert_eq!(read(&mut e), 43416, "due again after the interval");
    }
    #[test]
    fn tokens_rate_limits_matching_timeouts_and_cancellation() {
        let mut e = engine(DeviceMode::Replay);
        e.start(&[40004, 40008], true).unwrap();
        assert!(e.on_token(0).is_some());
        assert!(!e.response(40008));
        assert!(e.on_token(1000).is_none());
        assert!(e.response(40004));
        assert!(e.on_token(MIN_PROBE_INTERVAL_MS - 1).is_none());
        assert!(e.on_token(1000).is_some());
        assert!(e.on_token(5999).is_none());
        assert!(e.on_token(6000).is_some());
        assert!(e.on_token(11000).is_some());
        e.expire(16000);
        assert_eq!(e.status.states[&40008], RegisterState::Unsupported);
        assert!(!e.status.running);
        e.start(&[48852], true).unwrap();
        e.cancel();
        assert!(e.on_token(20000).is_none());
        assert!(!e.response(48852));
    }
}
