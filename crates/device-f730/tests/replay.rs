use nibe_core::{Clock, ObservationState, RegisterValue, SystemClock};
use nibe_device_f730::{
    catalog::Catalog,
    decode::{Observer, WordOrder},
    start_adapter,
    transport::Capture,
    AdapterConfig,
};
use nibe_protocol::{Decoder, Event, MODBUS40};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[test]
fn complete_capture_matches_expected_values_identity_replies_and_counters() {
    let capture = Capture::bundled();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../fixtures/serial-captures/synthetic-f730-session.expected.json"
    ))
    .unwrap();
    let mut decoder = Decoder::default();
    let mut observer = Observer::new(Catalog::bundled(), WordOrder::LowWordFirst);
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut pending = None;
    for (index, r) in capture.records.iter().enumerate() {
        let bytes = r.bytes().unwrap();
        if r.dir == "tx" {
            let kind = match bytes.as_slice() {
                [6] => "acks_sent",
                [0x15] => "naks_sent",
                [0xc0, 0x69, 2, lo, hi, _] => {
                    let id = u16::from_le_bytes([*lo, *hi]);
                    assert_eq!(bytes, nibe_protocol::read_request(id).unwrap().as_bytes());
                    pending = Some(id);
                    "read_requests_sent"
                }
                _ => panic!("fixture contains an unauthorized transmission"),
            };
            *counts.entry(kind).or_default() += 1;
            continue;
        }
        let mut replies = vec![];
        // Exercise streaming across arbitrary capture boundaries AND single bytes.
        for byte in bytes {
            for event in decoder.feed(&[byte]) {
                let queued = capture
                    .records
                    .get(index + 1)
                    .filter(|next| next.dir == "tx")
                    .and_then(|r| {
                        let b = r.bytes().unwrap();
                        match b.as_slice() {
                            [0xc0, 0x69, 2, lo, hi, _] => Some(
                                nibe_protocol::read_request(u16::from_le_bytes([*lo, *hi]))
                                    .unwrap(),
                            ),
                            _ => None,
                        }
                    });
                if let Some(reply) = nibe_protocol::reply(&event, queued.as_ref()) {
                    replies.extend(reply);
                }
                match event {
                    Event::Frame(f) => {
                        let key = if f.address == MODBUS40 {
                            "pump_frames_for_us"
                        } else {
                            "pump_frames_for_other_addresses"
                        };
                        *counts.entry(key).or_default() += 1;
                        observer.observe(&f, r.t_us / 1000, pending);
                        if f.command == 0x6a {
                            pending = None;
                        }
                    }
                    Event::ChecksumError { .. } => {
                        *counts.entry("checksum_errors").or_default() += 1
                    }
                    Event::Ack => *counts.entry("acks_received").or_default() += 1,
                    _ => {}
                }
            }
        }
        if !replies.is_empty() {
            assert_eq!(
                replies,
                capture.records[index + 1].bytes().unwrap(),
                "reply at record {index}"
            );
        }
    }
    for (key, value) in counts {
        assert_eq!(value, expected[key].as_u64().unwrap(), "{key}");
    }
    let identity = observer.identity.unwrap();
    assert_eq!(identity.model, expected["product"]["model_family"]);
    assert_eq!(identity.firmware.as_deref(), Some("1234"));
    let values = expected["final_values"].as_object().unwrap();
    assert_eq!(observer.readings.len(), values.len());
    for (id, value) in values {
        let reading = &observer.readings[&id.parse::<u16>().unwrap()];
        let actual = match reading.decoded.as_ref().unwrap() {
            RegisterValue::Integer(i) => *i as f64,
            RegisterValue::Decimal(f) => *f,
            _ => panic!("numeric value expected"),
        };
        assert!(
            (actual - value.as_f64().unwrap()).abs() < 0.00001,
            "register {id}: {actual}"
        );
    }
    assert_eq!(
        observer.readings[&43424].observation,
        ObservationState::ConfirmedReadable
    );
    assert_eq!(observer.malformed, 0);
}

/// First capture from the owner's F730 (receive-only, no accessory answering; see the file header).
#[test]
fn real_f730_passive_capture_decodes_identity_and_empty_log_set() {
    let capture = Capture::open(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/serial-captures/real-f730-passive-2026-10-24.capture.jsonl"
    )))
    .unwrap();
    assert!(capture.records.iter().all(|r| r.dir == "rx"));
    let mut decoder = Decoder::default();
    let mut observer = Observer::new(Catalog::bundled(), WordOrder::Unknown);
    let mut commands: BTreeMap<u8, u64> = BTreeMap::new();
    let (mut others, mut checksum_errors, mut acks) = (0, 0, 0);
    for r in &capture.records {
        for event in decoder.feed(&r.bytes().unwrap()) {
            match &event {
                Event::Frame(f) if f.address == MODBUS40 => {
                    *commands.entry(f.command).or_default() += 1;
                    if f.command == 0x68 {
                        // The pump's LOG.SET is empty: every slot is register 0xFFFF.
                        assert!(f.payload.chunks(4).all(|c| c[..2] == [0xff, 0xff]));
                    }
                    observer.observe(f, r.t_us / 1000, None);
                }
                Event::Frame(_) => others += 1,
                Event::ChecksumError { .. } => checksum_errors += 1,
                _ => {}
            }
            if nibe_protocol::reply(&event, None) == Some(vec![6]) {
                acks += 1;
            }
        }
    }
    assert_eq!(
        commands,
        BTreeMap::from([(0x68, 12), (0x69, 54), (0x6b, 54), (0x6d, 6), (0xee, 35)])
    );
    assert_eq!((others, checksum_errors, acks), (0, 0, 161));
    let identity = observer.identity.unwrap();
    assert_eq!(identity.model, "F730");
    assert_eq!(identity.firmware.as_deref(), Some("9721"));
    assert_eq!(identity.source, "NIBE 0x6D product info: F730 Cu 3x400V");
    assert!(observer.readings.is_empty());
    assert_eq!(observer.malformed, 0);
}

/// HP-26 run 1 on the owner's F730: our recorded replies and the pump's read responses (M2 acceptance).
#[test]
fn real_f730_read_session_reproduces_replies_and_decodes_verified_values() {
    let capture = Capture::open(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/serial-captures/real-f730-reads-2026-10-24.capture.jsonl"
    )))
    .unwrap();
    let mut decoder = Decoder::default();
    let mut observer = Observer::new(Catalog::bundled(), WordOrder::Unknown);
    let mut pending: Option<u16> = None;
    let (mut read_requests, mut nak_or_errors) = (0, 0);
    let records = &capture.records;
    let mut index = 0;
    while index < records.len() {
        let record = &records[index];
        index += 1;
        if record.dir != "rx" {
            continue;
        }
        // The recorder writes each rx chunk followed by the replies sent for it.
        let mut recorded = vec![];
        while index < records.len() && records[index].dir == "tx" {
            recorded.push(records[index].bytes().unwrap());
            index += 1;
        }
        let request = recorded.iter().find(|b| b.len() == 6).map(|b| {
            let id = u16::from_le_bytes([b[3], b[4]]);
            assert_eq!(b, nibe_protocol::read_request(id).unwrap().as_bytes());
            id
        });
        let mut replies = vec![];
        for event in decoder.feed(&record.bytes().unwrap()) {
            let token = matches!(&event, Event::Frame(f) if f.address == MODBUS40 && f.command == 0x69 && f.payload.is_empty());
            let queued = request
                .filter(|_| token)
                .map(|id| nibe_protocol::read_request(id).unwrap());
            if let Some(reply) = nibe_protocol::reply(&event, queued.as_ref()) {
                replies.push(reply);
            }
            if queued.is_some() {
                read_requests += 1;
                pending = request;
            }
            match event {
                Event::Frame(f) if f.address == MODBUS40 => {
                    let requested = (f.command == 0x6a && pending.is_some())
                        .then(|| pending.take())
                        .flatten();
                    observer.observe(&f, record.t_us / 1000, requested);
                }
                Event::Frame(_) | Event::ChecksumError { .. } | Event::Nak => nak_or_errors += 1,
                _ => {}
            }
        }
        assert_eq!(replies, recorded, "replies at t_us {}", record.t_us);
    }
    assert_eq!((read_requests, nak_or_errors), (8, 0));
    assert_eq!(observer.word_order, WordOrder::HighWordFirst);
    assert_eq!(observer.identity.unwrap().model, "F730");
    let expected = [
        (48852, 0.0),
        (40004, 9.6),
        (40013, 40.1),
        (40033, 20.7),
        (43005, 1.2),
        (47011, -2.0),
        (47007, 5.0),
        (47398, 20.0),
    ];
    for (id, value) in expected {
        let reading = &observer.readings[&id];
        assert_eq!(
            reading.observation,
            ObservationState::ConfirmedReadable,
            "{id}"
        );
        let actual = match reading.decoded.as_ref().unwrap() {
            RegisterValue::Integer(i) => *i as f64,
            RegisterValue::Decimal(f) => *f,
            other => panic!("numeric value expected for {id}: {other:?}"),
        };
        assert!((actual - value).abs() < 1e-9, "register {id}: {actual}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn wire_replies_continue_while_application_executor_is_blocked() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let (_device, handle) = start_adapter(AdapterConfig::replay(), clock).await.unwrap();
    // Deliberately prevent the application/HTTP executor from consuming observations.
    // The independent UART reactor still services all fixture frames and ACKs.
    std::thread::sleep(Duration::from_millis(1200));
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if handle.snapshot().diagnostics.frames_for_us == 8 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let state = handle.snapshot();
    assert_eq!(state.readings.len(), 17);
    assert_eq!(state.diagnostics.acks_sent, 8); // No active read queued: the read token receives ACK.
    assert_eq!(state.diagnostics.naks_sent, 1);
    assert_eq!(state.diagnostics.read_requests_sent, 0);
    assert_eq!(state.diagnostics.ack_deadline_misses, 0);
    assert_eq!(state.diagnostics.observations_dropped, 0);
    assert!(!state.capabilities.writes);
    handle.shutdown();
}

#[test]
fn unknown_registers_mismatched_identity_and_malformed_payloads_are_visible() {
    let mut observer = Observer::new(Catalog::bundled(), WordOrder::Unknown);
    observer.observe(
        &nibe_protocol::Frame {
            address: MODBUS40,
            command: 0x6a,
            payload: vec![1, 0, 1, 2, 3, 4],
        },
        0,
        None,
    );
    assert_eq!(observer.readings[&1].observation, ObservationState::Unknown);
    assert!(observer.readings[&1].decoded.is_none());
    observer.observe(
        &nibe_protocol::Frame {
            address: MODBUS40,
            command: 0x68,
            payload: vec![0],
        },
        0,
        None,
    );
    assert_eq!(observer.malformed, 1);
    observer.observe(
        &nibe_protocol::Frame {
            address: MODBUS40,
            command: 0x6a,
            payload: vec![0x44, 0x9c, 10, 0, 0, 0],
        },
        0,
        None,
    );
    assert!(observer.readings[&40004].decoded.is_some());
    observer.observe(
        &nibe_protocol::Frame {
            address: MODBUS40,
            command: 0x6d,
            payload: [&[1, 0, 1][..], b"F1255"].concat(),
        },
        0,
        None,
    );
    assert!(observer.readings[&40004].decoded.is_none());
    for id in 1u16..1000 {
        observer.observe(
            &nibe_protocol::Frame {
                address: MODBUS40,
                command: 0x6a,
                payload: [id.to_le_bytes().as_slice(), &[0, 0, 0, 0]].concat(),
            },
            u64::from(id),
            None,
        );
    }
    assert!(observer.readings.len() <= 129);
}
