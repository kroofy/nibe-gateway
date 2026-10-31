use crate::catalog::{Catalog, DataType, Entry};
use nibe_core::*;
use nibe_protocol::{Frame, MODBUS40};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WordOrder {
    #[default]
    Unknown,
    LowWordFirst,
    HighWordFirst,
}
#[derive(Debug, Clone)]
pub struct Observer {
    pub catalog: Catalog,
    pub identity: Option<DeviceIdentity>,
    pub word_order: WordOrder,
    pub readings: BTreeMap<u16, RegisterReading>,
    pub malformed: u64,
}
impl Observer {
    pub fn new(catalog: Catalog, word_order: WordOrder) -> Self {
        Self {
            catalog,
            identity: None,
            word_order,
            readings: BTreeMap::new(),
            malformed: 0,
        }
    }
    pub fn observe(&mut self, frame: &Frame, at_ms: u64, requested: Option<u16>) {
        if frame.address != MODBUS40 {
            return;
        }
        match frame.command {
            0x6d if frame.payload.len() >= 4 => {
                let text = String::from_utf8_lossy(&frame.payload[3..])
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                let model = text.split([' ', '-']).next().unwrap_or("").to_string();
                if model.is_empty() || !text.is_ascii() {
                    self.malformed += 1;
                    return;
                }
                if model != "F730" {
                    for r in self.readings.values_mut() {
                        r.decoded = None;
                        r.error =
                            Some("product identity does not match F730 catalog; raw only".into());
                        r.observation = ObservationState::Observed;
                    }
                }
                self.identity = Some(DeviceIdentity {
                    model,
                    firmware: Some(
                        u16::from_be_bytes([frame.payload[1], frame.payload[2]]).to_string(),
                    ),
                    serial: None,
                    source: format!("NIBE 0x6D product info: {text}"),
                    verification: VerificationState::Unverified,
                });
            }
            0x6a if frame.payload.len() == 6 => {
                let id = u16::from_le_bytes([frame.payload[0], frame.payload[1]]);
                let data = &frame.payload[2..];
                if id == 48852 {
                    let known = self.word_order;
                    self.word_order = match data[0] {
                        0 => WordOrder::HighWordFirst,
                        1 => WordOrder::LowWordFirst,
                        _ => WordOrder::Unknown,
                    };
                    if known == WordOrder::Unknown && self.word_order != WordOrder::Unknown {
                        self.redecode_word_order_pending();
                    }
                }
                self.insert(id, data, at_ms, false, requested == Some(id));
            }
            0x68 if frame.payload.len() % 4 == 0 => {
                let slots: Vec<_> = frame
                    .payload
                    .chunks_exact(4)
                    .map(|s| (u16::from_le_bytes([s[0], s[1]]), [s[2], s[3]]))
                    .collect();
                let mut i = 0;
                while i < slots.len() {
                    let (id, raw) = slots[i];
                    i += 1;
                    if id == u16::MAX {
                        continue;
                    }
                    if id == 48852 {
                        self.word_order = match raw[0] {
                            0 => WordOrder::HighWordFirst,
                            1 => WordOrder::LowWordFirst,
                            _ => WordOrder::Unknown,
                        };
                    }
                    if self
                        .catalog
                        .entry(id)
                        .is_some_and(|e| e.data_type.width() == 4)
                        && i < slots.len()
                        && slots[i].0 == id + 1
                    {
                        let next = slots[i].1;
                        i += 1;
                        self.insert(id, &[raw[0], raw[1], next[0], next[1]], at_ms, true, false);
                    } else {
                        self.insert(id, &raw, at_ms, true, false);
                    }
                }
            }
            0x68 | 0x6a | 0x6d => self.malformed += 1,
            _ => {}
        }
    }
    /// 32-bit readings that arrived before 48852 keep their raw bytes; decode them once the order is known
    /// instead of waiting for the next read (slow registers come round only every few minutes).
    fn redecode_word_order_pending(&mut self) {
        let pending: Vec<(u16, Vec<u8>, u64, bool)> = self
            .readings
            .iter()
            .filter(|(_, r)| {
                r.error
                    .as_deref()
                    .is_some_and(|e| e.starts_with(WORD_ORDER_UNKNOWN))
            })
            .filter_map(|(id, r)| match &r.raw {
                Some(RegisterValue::Text(text)) => {
                    Some((*id, unhex(text)?, r.observed_at_ms, r.source == "NIBE 0x68"))
                }
                _ => None,
            })
            .collect();
        for (id, bytes, at_ms, data_message) in pending {
            self.insert(id, &bytes, at_ms, data_message, false);
        }
    }
    fn insert(&mut self, id: u16, bytes: &[u8], at_ms: u64, data_message: bool, confirmed: bool) {
        // Retain unknown observations without allowing bus noise to grow RAM forever.
        if self.catalog.entry(id).is_none() && !self.readings.contains_key(&id) {
            let unknown: Vec<_> = self
                .readings
                .iter()
                .filter(|(id, _)| self.catalog.entry(**id).is_none())
                .collect();
            if unknown.len() >= 128 {
                if let Some((&oldest, _)) =
                    unknown.into_iter().min_by_key(|(_, r)| r.observed_at_ms)
                {
                    self.readings.remove(&oldest);
                }
            }
        }
        let definition = self.catalog.entry(id);
        let mut error = None;
        let mut raw = Some(RegisterValue::Text(hex(bytes)));
        let mut decoded = None;
        if self.identity.as_ref().is_some_and(|i| i.model != "F730") {
            error = Some("product identity does not match F730 catalog; raw only".into());
        } else if let Some(e) = definition {
            match decode_value(e, bytes, self.word_order, data_message) {
                Ok((r, d)) => {
                    raw = Some(RegisterValue::Integer(r));
                    decoded = d;
                    if decoded.is_none() {
                        error =
                            Some("unavailable: integer-limit sentinel (F730 hypothesis)".into());
                    }
                }
                Err(e) => error = Some(e),
            }
        } else {
            error = Some("unknown register; raw only".into());
        }
        let observation = if definition.is_none() {
            ObservationState::Unknown
        } else if decoded.is_some()
            && (confirmed
                || self
                    .readings
                    .get(&id)
                    .is_some_and(|r| r.observation == ObservationState::ConfirmedReadable))
        {
            ObservationState::ConfirmedReadable
        } else {
            ObservationState::Observed
        };
        self.readings.insert(
            id,
            RegisterReading {
                id: id.into(),
                raw,
                decoded,
                observed_at_ms: at_ms,
                source: if data_message {
                    "NIBE 0x68".into()
                } else {
                    "NIBE 0x6A".into()
                },
                observation,
                error,
                max_age_ms: None,
            },
        );
    }
}
const WORD_ORDER_UNKNOWN: &str = "32-bit word order unknown";
fn unhex(text: &str) -> Option<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn decode_value(
    e: &Entry,
    bytes: &[u8],
    order: WordOrder,
    data_message: bool,
) -> Result<(i64, Option<RegisterValue>), String> {
    if bytes.len() < e.data_type.width() {
        return Err("incomplete register value / missing second slot".into());
    }
    let mut b = [0; 4];
    let n = bytes.len().min(4);
    b[..n].copy_from_slice(&bytes[..n]);
    if e.data_type.width() == 4 {
        match (order,data_message) {
            (WordOrder::Unknown,_)=>return Err(format!("{WORD_ORDER_UNKNOWN}: read 48852 or supply verified override")),
            (WordOrder::HighWordFirst,true)=>return Err("0x68 word order with 48852=0 is conflicting in sources; raw only until hardware verified".into()),
            (WordOrder::HighWordFirst,false)=>b.rotate_left(2),
            _=>{}
        }
    }
    let (r, sentinel) = match e.data_type {
        DataType::U8 => (i64::from(b[0]), 255),
        DataType::S8 => (i64::from(b[0] as i8), -128),
        DataType::U16 => (i64::from(u16::from_le_bytes([b[0], b[1]])), 65535),
        DataType::S16 => (i64::from(i16::from_le_bytes([b[0], b[1]])), -32768),
        DataType::U32 => (i64::from(u32::from_le_bytes(b)), 4294967295),
        DataType::S32 => (i64::from(i32::from_le_bytes(b)), -2147483648),
    };
    // The F730 also marks absent 32-bit values (e.g. 42075 without an EME20) with the 16-bit
    // sentinel sign-extended: 0xFFFF8000, i.e. -32768 as s32 (verified on the owner's F730).
    let widened = e.data_type.width() == 4 && i32::from_le_bytes(b) == -32768;
    let decoded = if r == sentinel || widened {
        None
    } else if e.scale == 1 {
        Some(RegisterValue::Integer(r))
    } else {
        Some(RegisterValue::Decimal(r as f64 / f64::from(e.scale)))
    };
    Ok((r, decoded))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widened_16_bit_sentinel_marks_absent_32_bit_values() {
        let c = Catalog::bundled();
        // Wire bytes of 42075 on the owner's F730 (48852=0, high word first).
        let (raw, decoded) = decode_value(
            c.entry(42075).unwrap(),
            &[0xff, 0xff, 0x00, 0x80],
            WordOrder::HighWordFirst,
            false,
        )
        .unwrap();
        assert_eq!((raw, decoded), (0xFFFF_8000, None));
        let (_, counter) = decode_value(
            c.entry(43416).unwrap(),
            &[0, 0, 0x40, 0x7e],
            WordOrder::HighWordFirst,
            false,
        )
        .unwrap();
        assert_eq!(counter, Some(RegisterValue::Integer(32320)));
    }
    #[test]
    fn counters_read_before_the_word_order_are_decoded_once_48852_arrives() {
        let mut o = Observer::new(Catalog::bundled(), WordOrder::Unknown);
        let read = |id: u16, data: [u8; 4]| nibe_protocol::Frame {
            address: MODBUS40,
            command: 0x6a,
            payload: [id.to_le_bytes().as_slice(), &data].concat(),
        };
        o.observe(&read(43416, [0, 0, 0x40, 0x7e]), 1_000, Some(43416));
        assert!(o.readings[&43416].decoded.is_none());
        o.observe(&read(48852, [0, 0, 0, 0]), 2_000, Some(48852));
        let r = &o.readings[&43416];
        assert_eq!(r.decoded, Some(RegisterValue::Integer(32320)));
        assert_eq!(
            (r.error.as_deref(), r.observed_at_ms),
            (None, 1_000),
            "keeps its read time"
        );
    }
    #[test]
    fn widths_scaling_word_order_and_sentinels() {
        let c = Catalog::bundled();
        assert_eq!(
            decode_value(
                c.entry(40004).unwrap(),
                &[0x83, 0xff, 0, 0],
                WordOrder::Unknown,
                false
            )
            .unwrap()
            .1,
            Some(RegisterValue::Decimal(-12.5))
        );
        let counter = c.entry(43424).unwrap();
        for (order, b) in [
            (WordOrder::LowWordFirst, [0xf5, 0x12, 0, 0]),
            (WordOrder::HighWordFirst, [0, 0, 0xf5, 0x12]),
        ] {
            assert_eq!(decode_value(counter, &b, order, false).unwrap().0, 4853);
        }
        assert!(decode_value(counter, &[0; 4], WordOrder::Unknown, false).is_err());
        assert!(decode_value(counter, &[0; 4], WordOrder::HighWordFirst, true).is_err());
        assert_eq!(
            decode_value(
                c.entry(40033).unwrap(),
                &[0, 0x80],
                WordOrder::Unknown,
                false
            )
            .unwrap()
            .1,
            None
        );
        let mut signed = c.entry(40004).unwrap().clone();
        signed.data_type = DataType::S8;
        signed.scale = 1;
        assert_eq!(
            decode_value(&signed, &[0xfd, 0], WordOrder::Unknown, true)
                .unwrap()
                .0,
            -3
        );
    }
}
