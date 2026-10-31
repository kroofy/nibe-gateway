//! MODBUS40 writes, one at a time, never retried, always read back. Either open (ADR 0125: any register, any
//! encodable value; the caller decides) or gated (ADR 0104: explicit allowlist, catalog range).
use crate::catalog::Catalog;
use nibe_core::{ErrorCode, ServiceError};
use nibe_protocol::{write_request, WriteRequest};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};
use utoipa::ToSchema;

/// A queued write that never meets a write token is abandoned after this long.
pub const QUEUE_TIMEOUT_MS: u64 = 15_000;
/// Time allowed for the pump's 0x6C response after our write request.
pub const RESPONSE_TIMEOUT_MS: u64 = 5_000;
/// NIBE alarm reset: writing 1 acknowledges the current alarm (NibePi index.js). Read back via 45001.
pub const ALARM_RESET: u16 = 45171;
pub const ALARM: u16 = 45001;
/// The MODBUS40 communication alarm NibePi resets automatically.
pub const ALARM_MODBUS_COMMUNICATION: i32 = 251;
/// Auto-reset retries at most this often while 45001 still reads 251. After a bus outage the pump
/// keeps 251 latched for a while and accepts resets without clearing it (owner's F730, 2026-10-25);
/// NibePi retried every ~15 s. One write per minute is bounded and harmless.
pub const AUTO_RESET_INTERVAL_MS: u64 = 60_000;
/// Entries that must never be written through the allowlist path, whatever the allowlist says.
const BLOCKED_FLAGS: [&str; 3] = ["never_write", "alarm_reset", "protocol_control"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WriteState {
    Queued,
    Sent,
    Accepted,
    Denied,
    /// No write token or no 0x6C in time. The value may or may not have been applied: see the read-back.
    Timeout,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WriteOutcome {
    pub register: u16,
    pub raw: i32,
    pub state: WriteState,
    pub queued_at_ms: u64,
    pub updated_at_ms: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct WriteStatus {
    pub enabled: bool,
    /// Open writes (ADR 0125): every register is writable and no catalog range applies.
    #[serde(default)]
    pub open: bool,
    pub allowlist: Vec<u16>,
    pub sent: u64,
    pub accepted: u64,
    pub denied: u64,
    pub timeouts: u64,
    pub last: Option<WriteOutcome>,
    /// Latest offset attempt, retained independently of other controls. Queued/Sent
    /// replaces older completion evidence, including attempts in the same clock tick.
    #[serde(skip)]
    #[schema(ignore)]
    pub(crate) last_offset: Option<WriteOutcome>,
    /// Manual alarm-reset control enabled (ADR 0105).
    #[serde(default)]
    pub alarm_reset: bool,
    /// NibePi-style automatic reset of alarm 251.
    #[serde(default)]
    pub alarm_auto_reset: bool,
    #[serde(default)]
    pub auto_resets: u64,
}
/// Alarm-reset options (ADR 0105). Auto implies manual.
#[derive(Debug, Clone, Copy, Default)]
pub struct AlarmResetPolicy {
    pub manual: bool,
    pub auto_251: bool,
}

pub struct WriteEngine {
    catalog: Arc<Catalog>,
    allowlist: BTreeSet<u16>,
    open: bool,
    /// The write, its encoded request, and the register to read back afterwards.
    current: Option<(WriteOutcome, WriteRequest, u16)>,
    /// Second half of an alarm reset: queued once the first write is accepted.
    follow_up: Option<(u16, i32, u16)>,
    last_auto_ms: Option<u64>,
    pub status: WriteStatus,
}
impl WriteEngine {
    /// `allowlist` empty means writes are disabled. Every listed register is validated up front.
    pub fn new(catalog: Arc<Catalog>, allowlist: &[u16]) -> Result<Self, ServiceError> {
        Self::with_alarm_reset(catalog, allowlist, AlarmResetPolicy::default())
    }
    pub fn with_alarm_reset(
        catalog: Arc<Catalog>,
        allowlist: &[u16],
        alarm: AlarmResetPolicy,
    ) -> Result<Self, ServiceError> {
        Self::with_policy(catalog, allowlist, alarm, false)
    }
    /// `open` makes every register writable with any encodable value (ADR 0125); the allowlist is then unused.
    pub fn with_policy(
        catalog: Arc<Catalog>,
        allowlist: &[u16],
        alarm: AlarmResetPolicy,
        open: bool,
    ) -> Result<Self, ServiceError> {
        let engine = Self {
            catalog,
            allowlist: allowlist.iter().copied().collect(),
            open,
            current: None,
            follow_up: None,
            last_auto_ms: None,
            status: WriteStatus {
                enabled: open || !allowlist.is_empty(),
                open,
                allowlist: allowlist.to_vec(),
                alarm_reset: alarm.manual || alarm.auto_251,
                alarm_auto_reset: alarm.auto_251,
                ..WriteStatus::default()
            },
        };
        for &id in allowlist {
            engine.check_register(id)?;
        }
        Ok(engine)
    }
    fn check_register(&self, id: u16) -> Result<(i64, i64), ServiceError> {
        let e = self.catalog.entry(id).ok_or_else(|| {
            ServiceError::validation(format!("register {id} is not in the catalog"))
        })?;
        if e.access != "read_write" || e.flags.iter().any(|f| BLOCKED_FLAGS.contains(&f.as_str())) {
            return Err(ServiceError::validation(format!(
                "register {id} is not a writable setting"
            )));
        }
        match (e.raw_min, e.raw_max) {
            (Some(min), Some(max)) if min <= max => Ok((min, max)),
            _ => Err(ServiceError::validation(format!(
                "register {id} has no catalog range; writes need explicit limits"
            ))),
        }
    }
    /// Queue one write. Open mode rejects only unencodable requests and a busy engine; gated mode also
    /// rejects anything not allowlisted or out of the catalog range.
    pub fn queue(&mut self, id: u16, raw: i32, now_ms: u64) -> Result<(), ServiceError> {
        if !self.status.enabled {
            return Err(ServiceError::new(
                ErrorCode::ControlsDisabled,
                "hardware writes are disabled; set open_writes, or allow_hardware_writes and writable_registers",
            ));
        }
        if self.open {
            return self.enqueue(id, raw, id, now_ms);
        }
        if !self.allowlist.contains(&id) {
            return Err(ServiceError::new(
                ErrorCode::ControlsDisabled,
                format!("register {id} is not in writable_registers"),
            ));
        }
        let (min, max) = self.check_register(id)?;
        if !(min..=max).contains(&i64::from(raw)) {
            return Err(ServiceError::validation(format!(
                "raw value {raw} outside catalog range {min}..={max} for register {id}"
            )));
        }
        self.enqueue(id, raw, id, now_ms)
    }
    /// Acknowledge the current alarm: a 0 -> 1 edge on 45171, then read back 45001 and 45171.
    pub fn queue_alarm_reset(&mut self, now_ms: u64) -> Result<(), ServiceError> {
        if !self.status.alarm_reset {
            return Err(ServiceError::new(
                ErrorCode::ControlsDisabled,
                "alarm reset is disabled; set allow_alarm_reset",
            ));
        }
        self.enqueue_alarm_reset(now_ms)
    }
    /// The F730 accepted repeated 45171=1 writes without clearing 251, while the app's reset cleared it
    /// at once (2026-10-25): it appears to act on a 0 -> 1 edge. Write 0, then 1 once 0 is accepted.
    fn enqueue_alarm_reset(&mut self, now_ms: u64) -> Result<(), ServiceError> {
        self.enqueue(ALARM_RESET, 0, ALARM_RESET, now_ms)?;
        self.follow_up = Some((ALARM_RESET, 1, ALARM));
        Ok(())
    }
    /// A read of 45001 arrived. Queues a reset only for alarm 251, if enabled, idle and not done recently.
    pub fn observe_alarm(&mut self, code: i32, now_ms: u64) -> bool {
        let due = self
            .last_auto_ms
            .is_none_or(|t| now_ms.saturating_sub(t) >= AUTO_RESET_INTERVAL_MS);
        if !self.status.alarm_auto_reset || code != ALARM_MODBUS_COMMUNICATION || !due {
            return false;
        }
        if self.enqueue_alarm_reset(now_ms).is_err() {
            return false;
        }
        self.last_auto_ms = Some(now_ms);
        self.status.auto_resets += 1;
        true
    }
    fn enqueue(
        &mut self,
        id: u16,
        raw: i32,
        readback: u16,
        now_ms: u64,
    ) -> Result<(), ServiceError> {
        if self.current.is_some() {
            return Err(ServiceError::new(
                ErrorCode::Busy,
                "another write is in progress",
            ));
        }
        let request = write_request(id, raw).map_err(|e| {
            ServiceError::validation(format!("unsafe write request {id}={raw}: {e}"))
        })?;
        let outcome = WriteOutcome {
            register: id,
            raw,
            state: WriteState::Queued,
            queued_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        if outcome.register == crate::runtime::HEAT_OFFSET_S1 {
            self.status.last_offset = Some(outcome.clone());
        }
        self.status.last = Some(outcome.clone());
        self.current = Some((outcome, request, readback));
        Ok(())
    }
    /// Called for an empty write token: returns the request to send instead of an ACK.
    pub fn on_token(&mut self, now_ms: u64) -> Option<WriteRequest> {
        self.expire(now_ms);
        let (outcome, request, _) = self.current.as_mut()?;
        if outcome.state != WriteState::Queued {
            return None;
        }
        outcome.state = WriteState::Sent;
        outcome.updated_at_ms = now_ms;
        self.status.sent += 1;
        if outcome.register == crate::runtime::HEAT_OFFSET_S1 {
            self.status.last_offset = Some(outcome.clone());
        }
        self.status.last = Some(outcome.clone());
        Some(request.clone())
    }
    /// The pump's 0x6C answer to our sent request. Returns the register to read back.
    /// Returns the registers to read back (none while an alarm reset's second write is pending).
    pub fn response(&mut self, accepted: bool, now_ms: u64) -> Vec<u16> {
        let Some((mut outcome, _, readback)) = self
            .current
            .take_if(|(o, _, _)| o.state == WriteState::Sent)
        else {
            return Vec::new();
        };
        outcome.state = if accepted {
            self.status.accepted += 1;
            WriteState::Accepted
        } else {
            self.status.denied += 1;
            WriteState::Denied
        };
        outcome.updated_at_ms = now_ms;
        let register = outcome.register;
        if outcome.register == crate::runtime::HEAT_OFFSET_S1 {
            self.status.last_offset = Some(outcome.clone());
        }
        self.status.last = Some(outcome);
        match self.follow_up.take() {
            Some((id, raw, then)) if accepted => {
                let _ = self.enqueue(id, raw, then, now_ms);
                Vec::new()
            }
            // After an alarm reset also read 45171, to see whether the pump returns it to 0.
            _ if register == ALARM_RESET => vec![readback, ALARM_RESET],
            _ => vec![readback],
        }
    }
    /// Abandons stale writes without retrying. Returns a register to read back after a sent timeout.
    pub fn expire(&mut self, now_ms: u64) -> Option<u16> {
        let (outcome, _, _) = self.current.as_ref()?;
        let (limit, since) = match outcome.state {
            WriteState::Queued => (QUEUE_TIMEOUT_MS, outcome.queued_at_ms),
            _ => (RESPONSE_TIMEOUT_MS, outcome.updated_at_ms),
        };
        if now_ms.saturating_sub(since) < limit {
            return None;
        }
        let (mut outcome, _, readback) = self.current.take()?;
        self.follow_up = None;
        let was_sent = outcome.state == WriteState::Sent;
        outcome.state = WriteState::Timeout;
        outcome.updated_at_ms = now_ms;
        self.status.timeouts += 1;
        if outcome.register == crate::runtime::HEAT_OFFSET_S1 {
            self.status.last_offset = Some(outcome.clone());
        }
        self.status.last = Some(outcome);
        was_sent.then_some(readback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine(allow: &[u16]) -> WriteEngine {
        WriteEngine::new(Arc::new(Catalog::bundled()), allow).unwrap()
    }
    #[test]
    fn allowlist_flags_and_ranges_are_enforced() {
        for blocked in [45171, 48852, 40004] {
            assert!(WriteEngine::new(Arc::new(Catalog::bundled()), &[blocked]).is_err());
        }
        // Temporary Lux may be allowlisted; its range is the description's 0..=4 (ADR 0113).
        let mut lux = engine(&[48132]);
        assert!(lux.queue(48132, 5, 0).is_err());
        assert!(lux.queue(48132, 4, 0).is_ok());
        assert!(
            engine(&[]).queue(47011, 1, 0).is_err(),
            "disabled without allowlist"
        );
        let mut e = engine(&[47011]);
        assert!(e.queue(47007, 5, 0).is_err(), "not allowlisted");
        assert!(e.queue(47011, 11, 0).is_err(), "out of range");
        e.queue(47011, -1, 0).unwrap();
        assert!(e.queue(47011, 0, 0).is_err(), "one write at a time");
    }
    #[test]
    fn open_writes_accept_any_register_and_value_but_stay_one_at_a_time() {
        let catalog = Arc::new(Catalog::bundled());
        let mut e =
            WriteEngine::with_policy(catalog, &[], AlarmResetPolicy::default(), true).unwrap();
        assert!(e.status.enabled && e.status.open);
        // Outside the catalog range, a never-write flag, and an ID missing from the catalog.
        for (id, raw) in [(47011, 50), (45171, 1), (1, 7)] {
            e.queue(id, raw, 0).unwrap();
            assert_eq!(e.on_token(1).unwrap(), write_request(id, raw).unwrap());
            assert!(e.queue(id, raw, 2).is_err(), "busy until answered");
            // 45171 is also read back a second time, as after any alarm-reset write.
            assert!(
                e.response(true, 3).contains(&id),
                "read back the written register"
            );
        }
        // Encoding safety still applies: a 0x5c byte in the payload is never sent.
        assert!(e.queue(47011, 0x5c, 4).is_err());
    }
    #[test]
    fn alarm_reset_is_separately_gated_reads_back_45001_and_auto_is_rate_limited() {
        let catalog = Arc::new(Catalog::bundled());
        let mut off = WriteEngine::new(catalog.clone(), &[47011]).unwrap();
        assert!(off.queue_alarm_reset(0).is_err(), "not enabled");
        assert!(
            off.queue(ALARM_RESET, 1, 0).is_err(),
            "never via the allowlist path"
        );
        assert!(!off.observe_alarm(251, 0));
        let auto = AlarmResetPolicy {
            manual: false,
            auto_251: true,
        };
        let mut e = WriteEngine::with_alarm_reset(catalog, &[], auto).unwrap();
        assert!(!e.observe_alarm(183, 0), "only alarm 251 is auto-reset");
        assert!(e.observe_alarm(251, 0));
        // A 0 -> 1 edge: write 0, then 1 once 0 is accepted, then read back 45001 and 45171.
        assert_eq!(
            e.on_token(100).unwrap(),
            write_request(ALARM_RESET, 0).unwrap()
        );
        assert!(
            e.response(true, 150).is_empty(),
            "second half still pending"
        );
        assert_eq!(
            e.on_token(200).unwrap(),
            write_request(ALARM_RESET, 1).unwrap()
        );
        assert_eq!(e.response(true, 250), vec![ALARM, ALARM_RESET]);
        assert!(
            !e.observe_alarm(251, AUTO_RESET_INTERVAL_MS - 1),
            "rate limited"
        );
        assert!(e.observe_alarm(251, AUTO_RESET_INTERVAL_MS));
        assert_eq!(e.status.auto_resets, 2);
        assert!(e.queue_alarm_reset(AUTO_RESET_INTERVAL_MS).is_err(), "busy");
    }
    #[test]
    fn token_response_and_timeouts_never_retry() {
        let mut e = engine(&[47011]);
        e.queue(47011, -1, 0).unwrap();
        let sent = e.on_token(500).unwrap();
        assert_eq!(sent, write_request(47011, -1).unwrap());
        assert!(e.on_token(1500).is_none(), "a sent write is never resent");
        assert_eq!(e.response(true, 1600), vec![47011]);
        assert_eq!(e.status.last.as_ref().unwrap().state, WriteState::Accepted);
        e.queue(47011, -2, 2000).unwrap();
        e.on_token(2500).unwrap();
        assert_eq!(
            e.expire(7500),
            Some(47011),
            "sent timeout asks for a read-back"
        );
        assert_eq!(e.status.last.as_ref().unwrap().state, WriteState::Timeout);
        e.queue(47011, -2, 8000).unwrap();
        assert_eq!(e.expire(8000 + QUEUE_TIMEOUT_MS), None);
        assert!(e.on_token(30_000).is_none());
        assert!(
            e.response(false, 30_001).is_empty(),
            "unsolicited 0x6C is ignored"
        );
        assert_eq!(
            (e.status.sent, e.status.accepted, e.status.timeouts),
            (2, 1, 2)
        );
    }
}
