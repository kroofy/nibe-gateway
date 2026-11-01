//! NIBE accessory-bus framing. Write requests exist only behind the device crate's gated write
//! engine; this crate has no notion of which registers may be written.
pub const ACK: u8 = 0x06;
pub const NAK: u8 = 0x15;
pub const MODBUS40: u16 = 0x0020;
pub const MAX_WIRE_PAYLOAD: usize = 128;
pub const MAX_FRAME: usize = MAX_WIRE_PAYLOAD + 6;

pub fn xor(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |a, b| a ^ b)
}
pub fn pump_checksum(bytes: &[u8]) -> u8 {
    match xor(bytes) {
        0x5c => 0xc5,
        sum => sum,
    }
}
fn checksum_ok(bytes: &[u8], got: u8) -> bool {
    let sum = xor(bytes);
    got == sum || (sum == 0x5c && got == 0xc5)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    TooLong,
    AmbiguousChecksum,
    UnverifiedEscaping,
}
impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EncodeError {}

/// Only constructible through a checked, read-only encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadRequest([u8; 6]);
impl ReadRequest {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
/// A checked MODBUS40 write request, sent only in place of the ACK to a write token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRequest([u8; 10]);
impl WriteRequest {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
/// `C0 6B 06 <register u16 LE> <value i32 LE> <xor>`, as NibePi (index.js setDataValue) sends it.
/// Like reads, any 0x5C in the data or checksum is refused: accessory-side escaping is unverified.
pub fn write_request(register: u16, value: i32) -> Result<WriteRequest, EncodeError> {
    let [lo, hi] = register.to_le_bytes();
    let v = value.to_le_bytes();
    let mut bytes = [0xc0, 0x6b, 6, lo, hi, v[0], v[1], v[2], v[3], 0];
    if bytes[3..9].contains(&0x5c) {
        return Err(EncodeError::UnverifiedEscaping);
    }
    let sum = xor(&bytes[..9]);
    if sum == 0x5c {
        return Err(EncodeError::AmbiguousChecksum);
    }
    bytes[9] = sum;
    Ok(WriteRequest(bytes))
}
pub fn read_request(register: u16) -> Result<ReadRequest, EncodeError> {
    let [lo, hi] = register.to_le_bytes();
    if lo == 0x5c || hi == 0x5c {
        return Err(EncodeError::UnverifiedEscaping);
    }
    let mut bytes = [0xc0, 0x69, 2, lo, hi, 0];
    let sum = xor(&bytes[..5]);
    if sum == 0x5c {
        return Err(EncodeError::AmbiguousChecksum);
    }
    bytes[5] = sum;
    Ok(ReadRequest(bytes))
}
/// Pump-side fixture encoder; never used to transmit accessory requests.
pub fn encode_pump(address: u16, command: u8, payload: &[u8]) -> Result<Vec<u8>, EncodeError> {
    let wire: Vec<_> = payload
        .iter()
        .flat_map(|&b| if b == 0x5c { vec![b, b] } else { vec![b] })
        .collect();
    if wire.len() > MAX_WIRE_PAYLOAD {
        return Err(EncodeError::TooLong);
    }
    let [hi, lo] = address.to_be_bytes();
    let mut bytes = vec![0x5c, hi, lo, command, wire.len() as u8];
    bytes.extend(wire);
    bytes.push(pump_checksum(&bytes[1..]));
    Ok(bytes)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub address: u16,
    pub command: u8,
    pub payload: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Frame(Frame),
    ChecksumError { address: u16 },
    InvalidEscape { address: u16 },
    Ack,
    Nak,
    Accessory,
    Garbage,
}
/// Only MODBUS40 is acknowledged. RMU40/internal traffic is always silent.
pub fn reply(event: &Event, pending: Option<&ReadRequest>) -> Option<Vec<u8>> {
    match event {
        Event::ChecksumError { address: MODBUS40 } | Event::InvalidEscape { address: MODBUS40 } => {
            Some(vec![NAK])
        }
        Event::Frame(Frame {
            address: MODBUS40,
            command: 0x69,
            payload,
        }) if payload.is_empty() => {
            Some(pending.map_or_else(|| vec![ACK], |r| r.as_bytes().to_vec()))
        }
        Event::Frame(Frame {
            address: MODBUS40, ..
        }) => Some(vec![ACK]),
        _ => None,
    }
}
fn plausible(bytes: &[u8]) -> bool {
    bytes.len() >= 5
        && bytes[4] as usize <= MAX_WIRE_PAYLOAD
        && ((bytes[1] == 0 && matches!(bytes[2], 2 | 0x16 | 0x19..=0x1c | 0x20))
            || ((0x41..=0x48).contains(&bytes[1]) && bytes[2] == 0xc9))
}
fn unescape(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut data = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        if b == 0x5c {
            if bytes.get(i) != Some(&b) {
                return None;
            }
            i += 1;
        }
        data.push(b);
    }
    Some(data)
}
/// True when `index` is the second byte of a `5C 5C` escape pair inside the data
/// area (from byte 5) of the frame starting at `buffer[0]`. A legitimate frame's
/// data never contains a lone `5C`, so such a position is never a real frame start;
/// treating it as one would let escaped payload bytes masquerade as a nested frame
/// (and make us transmit an ACK while the pump is still sending).
fn escaped_second(buffer: &[u8], index: usize) -> bool {
    let mut j = 5;
    while j < index {
        if buffer[j] == 0x5c && buffer.get(j + 1) == Some(&0x5c) {
            if j + 1 == index {
                return true;
            }
            j += 2;
        } else {
            j += 1;
        }
    }
    false
}
/// Incremental parser; memory is bounded regardless of input length/chunking.
/// A complete nested valid frame rescues a corrupt, incomplete length header.
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}
impl Decoder {
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }
    pub fn clear_partial(&mut self) {
        self.buffer.clear();
    }
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut out = vec![];
        for &byte in bytes {
            self.buffer.push(byte);
            self.parse(&mut out);
            debug_assert!(self.buffer.len() <= MAX_FRAME);
        }
        out
    }
    fn parse(&mut self, out: &mut Vec<Event>) {
        loop {
            let Some(&start) = self.buffer.first() else {
                return;
            };
            match start {
                ACK | NAK => {
                    out.push(if start == ACK { Event::Ack } else { Event::Nak });
                    self.buffer.remove(0);
                }
                0x5c => {
                    if self.buffer.len() < 5 {
                        return;
                    }
                    if !plausible(&self.buffer) {
                        self.buffer.remove(0);
                        out.push(Event::Garbage);
                        continue;
                    }
                    let end = self.buffer[4] as usize + 6;
                    if self.buffer.len() < end {
                        let rescue = (1..self.buffer.len()).find(|&i| {
                            let rest = &self.buffer[i..];
                            rest[0] == 0x5c
                                && !escaped_second(&self.buffer, i)
                                && plausible(rest)
                                && rest.len() >= rest[4] as usize + 6
                                && {
                                    let n = rest[4] as usize + 6;
                                    checksum_ok(&rest[1..n - 1], rest[n - 1])
                                        && unescape(&rest[5..n - 1]).is_some()
                                }
                        });
                        if let Some(i) = rescue {
                            self.buffer.drain(..i);
                            out.push(Event::Garbage);
                            continue;
                        }
                        return;
                    }
                    let address = u16::from_be_bytes([self.buffer[1], self.buffer[2]]);
                    if !checksum_ok(&self.buffer[1..end - 1], self.buffer[end - 1]) {
                        out.push(Event::ChecksumError { address });
                        self.buffer.remove(0); // Never swallow a frame after a corrupt LEN.
                    } else if let Some(payload) = unescape(&self.buffer[5..end - 1]) {
                        out.push(Event::Frame(Frame {
                            address,
                            command: self.buffer[3],
                            payload,
                        }));
                        self.buffer.drain(..end);
                    } else {
                        out.push(Event::InvalidEscape { address });
                        self.buffer.remove(0);
                    }
                }
                // An incomplete accessory C0 frame resyncs at the next 5C, so a crafted payload could in theory
                // produce a phantom pump frame. Accessory requests are short (6/10 bytes), so this is unlikely.
                0xc0 => {
                    if self.buffer.len() < 3 {
                        return;
                    }
                    let end = self.buffer[2] as usize + 4;
                    if self.buffer[2] as usize > MAX_WIRE_PAYLOAD {
                        self.buffer.remove(0);
                        out.push(Event::Garbage);
                        continue;
                    }
                    if self.buffer.len() < end {
                        // Echoes are not wire-critical; discard a corrupt prefix if a pump start follows.
                        if let Some(i) = self.buffer[1..].iter().position(|&b| b == 0x5c) {
                            self.buffer.drain(..i + 1);
                            continue;
                        }
                        return;
                    }
                    if checksum_ok(&self.buffer[..end - 1], self.buffer[end - 1]) {
                        self.buffer.drain(..end);
                        out.push(Event::Accessory);
                    } else {
                        self.buffer.remove(0);
                        out.push(Event::Garbage);
                    }
                }
                _ => {
                    self.buffer.remove(0);
                    out.push(Event::Garbage);
                }
            }
        }
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;
    #[test]
    fn write_request_matches_nibepi_layout_and_refuses_unverified_escaping() {
        let w = write_request(47011, -1).unwrap();
        let b = w.as_bytes();
        assert_eq!(
            &b[..9],
            &[0xc0, 0x6b, 6, 0xa3, 0xb7, 0xff, 0xff, 0xff, 0xff]
        );
        assert_eq!(b[9], xor(&b[..9]));
        assert_eq!(
            write_request(47011, 2).unwrap().as_bytes()[5..9],
            [2, 0, 0, 0]
        );
        assert_eq!(
            write_request(47011, 0x5c),
            Err(EncodeError::UnverifiedEscaping)
        );
        assert_eq!(
            write_request(0x5c00, 0),
            Err(EncodeError::UnverifiedEscaping)
        );
        let ambiguous = (0..=255)
            .find(|v| xor(&[0xc0, 0x6b, 6, 0xa3, 0xb7, *v as u8, 0, 0, 0]) == 0x5c)
            .unwrap();
        assert_eq!(
            write_request(47011, ambiguous),
            Err(EncodeError::AmbiguousChecksum)
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    #[test]
    fn published_vectors_and_safety() {
        let data = [0x4f, 0x9c, 0x5c, 0, 0x2c, 0];
        let bytes = encode_pump(MODBUS40, 0x6a, &data).unwrap();
        assert_eq!(
            bytes,
            [0x5c, 0, 0x20, 0x6a, 7, 0x4f, 0x9c, 0x5c, 0x5c, 0, 0x2c, 0, 0xb2]
        );
        assert_eq!(
            read_request(43424).unwrap().as_bytes(),
            [0xc0, 0x69, 2, 0xa0, 0xa9, 0xa2]
        );
        for id in [40043, 48968, 48732] {
            assert!(read_request(id).is_err());
        }
        let mut decoder = Decoder::default();
        let bad_c5 = [0x5c, 0, 0x20, 0x69, 0, 0xc5];
        assert!(matches!(
            decoder.feed(&bad_c5)[0],
            Event::ChecksumError { .. }
        ));
        let token = Event::Frame(Frame {
            address: MODBUS40,
            command: 0x6b,
            payload: vec![],
        });
        assert_eq!(
            reply(&token, Some(&read_request(43424).unwrap())),
            Some(vec![ACK])
        );
        assert_eq!(reply(&Event::ChecksumError { address: 0x19 }, None), None);
    }
    #[test]
    fn fixture_vectors() {
        let doc: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/serial-captures/protocol-vectors.synthetic.json"
        ))
        .unwrap();
        for v in doc["vectors"].as_array().unwrap() {
            if v["direction"] != "pump_to_accessory" {
                continue;
            }
            let s = v["hex"].as_str().unwrap();
            let bytes: Vec<_> = (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect();
            let events = Decoder::default().feed(&bytes);
            if let Some(expected) = v["expect"]["reply"].as_str() {
                let responses: Vec<_> = events
                    .iter()
                    .filter_map(|e| reply(e, None))
                    .flatten()
                    .collect();
                assert_eq!(
                    responses,
                    vec![u8::from_str_radix(expected, 16).unwrap()],
                    "{}",
                    v["name"]
                );
            }
        }
    }
    #[test]
    fn corrupt_length_resynchronizes() {
        let valid = encode_pump(MODBUS40, 0x69, &[]).unwrap();
        for len in [1, 8, 64, 128, 255] {
            let mut stream = vec![0, 255, 0x5c, 0, 0x20, 0x68, len, 0xff];
            stream.extend(&valid);
            assert!(Decoder::default()
                .feed(&stream)
                .contains(&Event::Frame(Frame {
                    address: MODBUS40,
                    command: 0x69,
                    payload: vec![]
                })));
        }
    }
    #[test]
    fn escaped_payload_bytes_never_rescue_a_phantom_nested_frame() {
        // Legitimate 0x68 data message: slot value 0x005C (escaped on the wire as
        // 5C 5C) followed by register 0xA920 with value bytes 00 89. The wire bytes
        // after the escape pair read `5C 00 20 A9 00 89`, a checksum-valid frame for
        // our address. It must not be emitted (we would ACK mid-transmission).
        let mut payload = vec![0x04, 0x9c, 0x5c, 0x00, 0x20, 0xa9, 0x00, 0x89];
        for slot in 0..18u16 {
            payload.extend((40100 + slot).to_le_bytes());
            payload.extend([0x10, 0x00]);
        }
        let bytes = encode_pump(MODBUS40, 0x68, &payload).unwrap();
        for chunk in [1, 3, 7, bytes.len()] {
            let mut d = Decoder::default();
            let events: Vec<_> = bytes.chunks(chunk).flat_map(|c| d.feed(c)).collect();
            assert_eq!(
                events,
                vec![Event::Frame(Frame {
                    address: MODBUS40,
                    command: 0x68,
                    payload: payload.clone()
                })],
                "chunk {chunk}"
            );
            let replies: Vec<_> = events.iter().filter_map(|e| reply(e, None)).collect();
            assert_eq!(replies, vec![vec![ACK]]);
        }
    }
    proptest! {
        #[test]
        fn arbitrary_chunks_roundtrip(payload in prop::collection::vec(any::<u8>(),0..60), chunk in 1usize..32) {
            let bytes = encode_pump(MODBUS40,0x68,&payload).unwrap();
            let mut d = Decoder::default();
            let events:Vec<_> = bytes.chunks(chunk).flat_map(|c|d.feed(c)).collect();
            prop_assert_eq!(events,vec![Event::Frame(Frame {address:MODBUS40,command:0x68,payload})]);
            prop_assert_eq!(d.buffered_len(),0);
        }
        #[test]
        fn garbage_is_bounded(bytes in prop::collection::vec(any::<u8>(),0..4096)) {
            let mut d=Decoder::default();
            for b in bytes { d.feed(&[b]); prop_assert!(d.buffered_len() <= MAX_FRAME); }
        }
    }
}
