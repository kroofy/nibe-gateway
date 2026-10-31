//! The MQTT 3.1.1 packets the bridge needs (OASIS mqtt-v3.1.1, section 3). QoS 0 only: state is retained
//! and republished, commands are acknowledged on the result topic, so broker-side delivery guarantees add
//! nothing but state.

/// Largest packet accepted from the broker. Commands are tiny; anything larger is dropped with the connection.
pub const MAX_INCOMING: usize = 64 * 1024;

pub struct Connect<'a> {
    pub client_id: &'a str,
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
    pub keep_alive_s: u16,
    /// Last will: topic, payload, retained.
    pub will: Option<(&'a str, &'a [u8], bool)>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Packet {
    /// Return code; 0 = accepted.
    ConnAck(u8),
    Publish {
        topic: String,
        payload: Vec<u8>,
    },
    SubAck,
    PingResp,
    /// Anything else the broker may send (e.g. PUBACK); ignored.
    Other(u8),
}

fn put_str(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value);
}

fn frame(kind: u8, body: Vec<u8>) -> Vec<u8> {
    let mut out = vec![kind];
    let mut len = body.len();
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if len == 0 {
            break;
        }
    }
    out.extend(body);
    out
}

pub fn connect(c: &Connect) -> Vec<u8> {
    let mut body = Vec::new();
    put_str(&mut body, b"MQTT");
    body.push(4); // protocol level 3.1.1
    let mut flags = 0x02; // clean session
    if let Some((_, _, retain)) = c.will {
        flags |= 0x04 | if retain { 0x20 } else { 0 };
    }
    if c.username.is_some() {
        flags |= 0x80;
        if c.password.is_some() {
            flags |= 0x40;
        }
    }
    body.push(flags);
    body.extend_from_slice(&c.keep_alive_s.to_be_bytes());
    put_str(&mut body, c.client_id.as_bytes());
    if let Some((topic, payload, _)) = c.will {
        put_str(&mut body, topic.as_bytes());
        put_str(&mut body, payload);
    }
    if let Some(user) = c.username {
        put_str(&mut body, user.as_bytes());
        if let Some(password) = c.password {
            put_str(&mut body, password.as_bytes());
        }
    }
    frame(0x10, body)
}

pub fn publish(topic: &str, payload: &[u8], retain: bool) -> Vec<u8> {
    let mut body = Vec::with_capacity(topic.len() + payload.len() + 2);
    put_str(&mut body, topic.as_bytes());
    body.extend_from_slice(payload);
    frame(0x30 | u8::from(retain), body)
}

/// SUBSCRIBE with QoS 0 for each filter.
pub fn subscribe(packet_id: u16, filters: &[&str]) -> Vec<u8> {
    let mut body = packet_id.to_be_bytes().to_vec();
    for filter in filters {
        put_str(&mut body, filter.as_bytes());
        body.push(0);
    }
    frame(0x82, body)
}

pub fn ping() -> Vec<u8> {
    vec![0xC0, 0]
}

pub fn disconnect() -> Vec<u8> {
    vec![0xE0, 0]
}

/// Decode one packet from the front of `buf`. `Ok(None)` = need more bytes; `Ok(Some((packet, used)))`.
pub fn decode(buf: &[u8]) -> Result<Option<(Packet, usize)>, &'static str> {
    if buf.is_empty() {
        return Ok(None);
    }
    let (mut len, mut shift, mut index) = (0usize, 0, 1);
    loop {
        let Some(&byte) = buf.get(index) else {
            return Ok(None);
        };
        len |= usize::from(byte & 0x7F) << shift;
        index += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 21 {
            return Err("malformed remaining length");
        }
    }
    if len > MAX_INCOMING {
        return Err("packet too large");
    }
    let Some(body) = buf.get(index..index + len) else {
        return Ok(None);
    };
    let kind = buf[0];
    let packet = match kind >> 4 {
        2 => Packet::ConnAck(*body.get(1).ok_or("short CONNACK")?),
        3 => {
            let qos = (kind >> 1) & 3;
            let topic_len = usize::from(u16::from_be_bytes([
                *body.first().ok_or("short PUBLISH")?,
                *body.get(1).ok_or("short PUBLISH")?,
            ]));
            let topic = body.get(2..2 + topic_len).ok_or("short PUBLISH topic")?;
            // QoS 1/2 publishes carry a packet id; we subscribe at QoS 0, so this is only defensive.
            let start = 2 + topic_len + if qos > 0 { 2 } else { 0 };
            Packet::Publish {
                topic: String::from_utf8(topic.to_vec()).map_err(|_| "topic is not UTF-8")?,
                payload: body.get(start..).ok_or("short PUBLISH payload")?.to_vec(),
            }
        }
        9 => Packet::SubAck,
        13 => Packet::PingResp,
        other => Packet::Other(other),
    };
    Ok(Some((packet, index + len)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connect_matches_the_spec_example_layout() {
        let bytes = connect(&Connect {
            client_id: "c",
            username: Some("u"),
            password: Some("p"),
            keep_alive_s: 60,
            will: Some(("t", b"offline", true)),
        });
        assert_eq!(
            bytes,
            [
                0x10, 31, 0, 4, b'M', b'Q', b'T', b'T', 4, 0xE6, 0, 60, 0, 1, b'c', 0, 1, b't', 0,
                7, b'o', b'f', b'f', b'l', b'i', b'n', b'e', 0, 1, b'u', 0, 1, b'p'
            ]
        );
    }
    #[test]
    fn publish_round_trips_and_long_lengths_use_continuation_bytes() {
        let payload = vec![b'x'; 300];
        let bytes = publish("a/b", &payload, true);
        assert_eq!(&bytes[..3], &[0x31, 0xB1, 0x02]); // 305 = 0x31 + 2*128
        assert_eq!(
            decode(&bytes).unwrap(),
            Some((
                Packet::Publish {
                    topic: "a/b".into(),
                    payload
                },
                bytes.len()
            ))
        );
        assert_eq!(decode(&bytes[..bytes.len() - 1]).unwrap(), None, "partial");
    }
    #[test]
    fn broker_packets_decode_and_garbage_is_rejected() {
        assert_eq!(
            decode(&[0x20, 2, 0, 5]).unwrap(),
            Some((Packet::ConnAck(5), 4))
        );
        assert_eq!(
            decode(&[0x90, 3, 0, 1, 0]).unwrap(),
            Some((Packet::SubAck, 5))
        );
        assert_eq!(decode(&[0xD0, 0]).unwrap(), Some((Packet::PingResp, 2)));
        assert!(decode(&[0x30, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]).is_err());
        assert!(
            decode(&[0x30, 0xFF, 0xFF, 0x7F]).is_err(),
            "over MAX_INCOMING"
        );
        assert_eq!(subscribe(1, &["x"]), [0x82, 6, 0, 1, 0, 1, b'x', 0]);
    }
}
