//! Feeds the history writer from the gateway's live readings (the poll list).
use crate::{History, Kind, RegisterInfo, Sample};
use nibe_core::{Gateway, RegisterValue};
use std::{collections::HashMap, time::Duration};

/// True when the kernel clock is NTP-synchronised (adjtimex reports no TIME_ERROR).
#[cfg(target_os = "linux")]
pub fn clock_synced() -> bool {
    // SAFETY: adjtimex with modes = 0 only reads the kernel clock state into the zeroed struct.
    unsafe {
        let mut tx: libc::timex = std::mem::zeroed();
        libc::adjtimex(&mut tx) != libc::TIME_ERROR
    }
}
#[cfg(not(target_os = "linux"))]
pub fn clock_synced() -> bool {
    true
}

/// Samples each new observation once; publishes register metadata when the set of registers changes.
/// Ends (with a final flush) when the gateway shuts down.
pub fn spawn_recorder(gateway: Gateway, history: History) {
    tokio::spawn(async move {
        let mut seen: HashMap<u32, u64> = HashMap::new();
        let mut known: Vec<u32> = Vec::new();
        let mut shutdown = gateway.shutdown_receiver();
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = shutdown.changed() => break,
            }
            let snapshot = gateway.snapshot();
            let mut ids: Vec<u32> = snapshot.registers.iter().map(|r| r.id).collect();
            ids.sort_unstable();
            if ids != known {
                let infos = snapshot
                    .registers
                    .iter()
                    .filter_map(|r| snapshot.catalog.iter().find(|d| d.id == r.id))
                    .map(|d| RegisterInfo {
                        id: d.id,
                        name: d.description.clone(),
                        unit: d.unit.clone(),
                        scale: d.scale,
                        kind: Kind::from_unit(d.unit.as_deref(), &d.description),
                    })
                    .collect();
                history.registers(infos);
                known = ids;
            }
            // The gateway clock is monotonic from its start (bus timing must never jump), so it stays
            // behind real time when the gateway started before NTP sync, e.g. after every power cut.
            // Convert each observation to wall time with the current offset between the two clocks.
            let offset_ms = wall_ms() - snapshot.now_ms as i64;
            let mut batch = Vec::new();
            for r in &snapshot.registers {
                if r.decoded.is_none() || seen.get(&r.id) == Some(&r.observed_at_ms) {
                    continue;
                }
                let raw = match &r.raw {
                    Some(RegisterValue::Integer(v)) => *v,
                    Some(RegisterValue::Boolean(b)) => i64::from(*b),
                    _ => continue,
                };
                seen.insert(r.id, r.observed_at_ms);
                batch.push(Sample {
                    register: r.id,
                    t: wall_seconds(r.observed_at_ms, offset_ms),
                    raw,
                });
            }
            history.record(batch);
        }
        let h = history.clone();
        let _ = tokio::task::spawn_blocking(move || h.stop()).await;
    });
}

fn wall_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Wall-clock Unix seconds of an observation stamped by the gateway clock.
fn wall_seconds(observed_at_ms: u64, offset_ms: i64) -> i64 {
    (observed_at_ms as i64 + offset_ms).div_euclid(1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observations_are_shifted_by_the_clock_offset() {
        // Gateway clock started 8 948 s behind real time (owner's Pi after a power cut, 2026-10-25).
        let gateway_now = 1_792_946_447_434u64;
        let offset = 8_948_492i64;
        assert_eq!(wall_seconds(gateway_now - 2_000, offset), 1_792_955_393);
        assert_eq!(wall_seconds(1_000, 0), 1);
    }
}
