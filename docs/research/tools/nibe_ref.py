#!/usr/bin/env python3
"""Reference model of the NIBE F-series accessory (MODBUS40-emulation) serial protocol.

Purpose: an executable, dependency-free (stdlib only) oracle for WS04's Rust
implementation and the generator of the *synthetic* fixtures in
fixtures/serial-captures/. It is NOT production code.

Every rule implemented here is documented, with source citations, in
docs/research/protocol.md and docs/research/protocol-summary-for-implementers.md.

Usage:
    python3 docs/research/tools/nibe_ref.py selftest      # check against third-party published vectors
    python3 docs/research/tools/nibe_ref.py gen OUT_DIR   # (re)generate synthetic fixtures

Python >= 3.8, no third-party packages.
"""
from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from functools import reduce
from pathlib import Path
from typing import List, Optional

START_PUMP = 0x5C  # frames sent by the heat pump (master)
START_ACC = 0xC0  # frames sent by an accessory (us)
ACK = 0x06
NAK = 0x15

ADDR_MODBUS40 = 0x0020  # 2 bytes on the wire, big-endian: 00 20
ADDR_RMU40_S1 = 0x0019
ADDR_SMS40 = 0x0016

CMD_RMU_WRITE_REQ = 0x60
CMD_RMU_DATA_MSG = 0x62
CMD_RMU_DATA_REQ = 0x63
CMD_DATA_MSG = 0x68
CMD_READ_REQ = 0x69  # pump->acc with LEN 0 = "read token"; acc->pump = read request
CMD_READ_RESP = 0x6A
CMD_WRITE_REQ = 0x6B  # pump->acc with LEN 0 = "write token"; acc->pump = write request
CMD_WRITE_RESP = 0x6C
CMD_PRODUCT_INFO = 0x6D
CMD_ACCESSORY_VERSION_REQ = 0xEE

# Values at the integer limit are treated as "no value" by yozik04/nibe
# (nibe/connection/encoders.py: is_hitting_integer_limit). HYPOTHESIS for F730.
SENTINELS = {"u8": 0xFF, "s8": -0x80, "u16": 0xFFFF, "s16": -0x8000, "u32": 0xFFFFFFFF, "s32": -0x80000000}


def xor8(data: bytes) -> int:
    return reduce(lambda a, b: a ^ b, data, 0)


def escape(payload: bytes) -> bytes:
    """Pump->accessory: every 0x5C inside the data section is sent twice."""
    return payload.replace(b"\x5c", b"\x5c\x5c")


def unescape(wire_payload: bytes) -> bytes:
    return wire_payload.replace(b"\x5c\x5c", b"\x5c")


def pump_checksum(addr_cmd_len_and_wire_payload: bytes) -> int:
    """XOR over addr_hi, addr_lo, cmd, LEN and the *escaped* payload (NOT the 0x5C start byte).
    If the result is 0x5C it is transmitted as 0xC5."""
    c = xor8(addr_cmd_len_and_wire_payload)
    return 0xC5 if c == 0x5C else c


def encode_pump_frame(addr: int, cmd: int, payload: bytes) -> bytes:
    """Build a frame as the HEAT PUMP would send it (used to make synthetic fixtures)."""
    wire = escape(payload)
    if len(wire) > 0xFF:
        raise ValueError("payload too long")
    hdr = bytes([(addr >> 8) & 0xFF, addr & 0xFF, cmd, len(wire)])
    return bytes([START_PUMP]) + hdr + wire + bytes([pump_checksum(hdr + wire)])


class UnverifiedEncoding(Exception):
    pass


def encode_acc_frame(cmd: int, payload: bytes, on_checksum_5c: str = "refuse") -> bytes:
    """Build a frame as an ACCESSORY sends it: C0 CMD LEN DATA CHK, CHK = XOR of all previous bytes
    INCLUDING the C0 start byte. No escaping is applied by any studied implementation.

    If the checksum would be 0x5C the sources disagree (nibe-py substitutes 0xC5, NibePi does not), so
    the default policy is to refuse. on_checksum_5c: "refuse" | "c5" | "raw".
    """
    if b"\x5c" in payload and on_checksum_5c == "refuse":
        # Not a checksum issue, but also unverified: no source escapes request payloads.
        raise UnverifiedEncoding("request payload contains 0x5C; escaping behaviour unverified")
    body = bytes([START_ACC, cmd, len(payload)]) + payload
    c = xor8(body)
    if c == 0x5C:
        if on_checksum_5c == "refuse":
            raise UnverifiedEncoding("request checksum is 0x5C; substitution rule unverified")
        if on_checksum_5c == "c5":
            c = 0xC5
    return body + bytes([c])


def read_request(register: int, **kw) -> bytes:
    return encode_acc_frame(CMD_READ_REQ, register.to_bytes(2, "little"), **kw)


def write_request(register: int, raw_value_4: bytes, **kw) -> bytes:
    assert len(raw_value_4) == 4
    return encode_acc_frame(CMD_WRITE_REQ, register.to_bytes(2, "little") + raw_value_4, **kw)


# ----------------------------------------------------------------------------------------------
# Stream parser (accessory side)
# ----------------------------------------------------------------------------------------------
@dataclass
class Event:
    kind: str  # pump_frame | acc_frame | ack | nak | checksum_error | garbage
    raw: bytes
    addr: Optional[int] = None
    cmd: Optional[int] = None
    payload: Optional[bytes] = None  # unescaped
    extra: dict = field(default_factory=dict)

    def to_json(self) -> dict:
        d = {"kind": self.kind, "raw": self.raw.hex()}
        if self.addr is not None:
            d["addr"] = f"0x{self.addr:04x}"
        if self.cmd is not None:
            d["cmd"] = f"0x{self.cmd:02x}"
        if self.payload is not None:
            d["payload"] = self.payload.hex()
        d.update(self.extra)
        return d


MAX_WIRE_LEN = 0x80  # largest LEN seen in published captures is 0x54; generous bound (HYPOTHESIS)
KNOWN_ACC_ADDRS = {0x0002, 0x0016, 0x0019, 0x001A, 0x001B, 0x001C, 0x0020}


def plausible_pump_header(b1: int, b2: int, ln: int) -> bool:
    """Header sanity check used for resynchronisation. Address byte pairs seen in sources:
    00 xx (accessories, yozik04/nibe Address enum) and 4n C9 (heat pump internal, 16-bit)."""
    if ln > MAX_WIRE_LEN:
        return False
    if b1 == 0x00:
        return b2 in KNOWN_ACC_ADDRS
    return 0x41 <= b1 <= 0x48 and b2 == 0xC9


def parse_stream(buf: bytes) -> List[Event]:
    """Split a received byte stream into events. Resilient to garbage:
    - a 0x5C is only treated as a frame start if the header is plausible;
    - on checksum mismatch a checksum_error event is emitted and scanning resumes at the NEXT byte
      (not after the claimed frame), so a corrupted LEN cannot swallow following frames;
    - a trailing incomplete frame is reported as garbage (a streaming parser keeps it buffered).
    """
    out: List[Event] = []
    i = 0
    garbage = bytearray()

    def flush():
        if garbage:
            out.append(Event("garbage", bytes(garbage)))
            garbage.clear()

    n = len(buf)
    while i < n:
        b = buf[i]
        if b == START_PUMP and i + 5 < n and plausible_pump_header(buf[i + 1], buf[i + 2], buf[i + 4]):
            ln = buf[i + 4]
            end = i + 6 + ln
            if end <= n:
                frame = buf[i:end]
                calc = xor8(frame[1 : 5 + ln])
                got = frame[5 + ln]
                addr = (frame[1] << 8) | frame[2]
                flush()
                if got == calc or (calc == 0x5C and got == 0xC5):
                    out.append(Event("pump_frame", frame, addr, frame[3], unescape(frame[5 : 5 + ln])))
                    i = end
                else:
                    out.append(Event("checksum_error", frame, addr, frame[3], None,
                                     {"calc": f"0x{calc:02x}", "got": f"0x{got:02x}"}))
                    i += 1
                continue
        if b == START_ACC and i + 3 < n:
            ln = buf[i + 2]
            end = i + 4 + ln
            if end <= n:
                calc = xor8(buf[i : end - 1])
                got = buf[end - 1]
                if got == calc or (calc == 0x5C and got == 0xC5):
                    flush()
                    frame = buf[i:end]
                    out.append(Event("acc_frame", frame, None, frame[1], frame[3 : 3 + ln]))
                    i = end
                    continue
        if b == ACK:
            flush(); out.append(Event("ack", bytes([b]))); i += 1; continue
        if b == NAK:
            flush(); out.append(Event("nak", bytes([b]))); i += 1; continue
        garbage.append(b)
        i += 1
    flush()
    return out


def accessory_reply(ev: Event, own_addr: int = ADDR_MODBUS40, pending_read: Optional[bytes] = None) -> Optional[bytes]:
    """What a READ-ONLY MODBUS40 emulation sends after a received event (see protocol summary §5)."""
    if ev.kind == "checksum_error":
        return bytes([NAK]) if ev.addr == own_addr else None
    if ev.kind != "pump_frame" or ev.addr != own_addr:
        return None
    if ev.cmd == CMD_READ_REQ and len(ev.payload) == 0 and pending_read is not None:
        return pending_read
    # write token: a read-only gateway never writes -> ACK
    return bytes([ACK])


# ----------------------------------------------------------------------------------------------
# Value decoding
# ----------------------------------------------------------------------------------------------
def decode_data_msg(payload: bytes) -> List[dict]:
    """0x68 payload: N x (addr u16 LE, value 2 bytes). addr 0xFFFF = empty slot."""
    if len(payload) % 4:
        raise ValueError("data message payload not multiple of 4")
    rows = []
    for k in range(0, len(payload), 4):
        a = int.from_bytes(payload[k : k + 2], "little")
        rows.append({"register": a, "raw16": payload[k + 2 : k + 4].hex(), "empty": a == 0xFFFF})
    return rows


def decode_raw(size: str, b: bytes) -> int:
    signed = size.startswith("s")
    if size in ("u8", "s8"):
        return int.from_bytes(b[:1], "little", signed=signed)  # high byte ignored (00 on wire)
    if size in ("u16", "s16"):
        return int.from_bytes(b[:2], "little", signed=signed)
    if size in ("u32", "s32"):
        return int.from_bytes(b[:4], "little", signed=signed)
    raise ValueError(size)


def is_sentinel(size: str, v: int) -> bool:
    lim = SENTINELS[size]
    return v <= lim if lim < 0 else v >= lim


# ----------------------------------------------------------------------------------------------
# Self test against third-party published bytes
# ----------------------------------------------------------------------------------------------
THIRD_PARTY = [
    # (hex, expected kind, note, source)
    ("5c00206a074f9c5c5c002c00b2", "pump_frame", "read resp 40015 with escaped 0x5C value",
     "yozik04/nibe tests/connection/test_nibegw_message_parsing.py::test_parse_escaped_read_response"),
    ("5c00206a060cb901000000f8", "pump_frame", "read resp 47372=1",
     "yozik04/nibe tests/connection/test_nibegw_message_parsing.py::test_parse_read_response"),
    ("5c00206a060cb901000000f9", "checksum_error", "same with wrong checksum",
     "yozik04/nibe tests/connection/test_nibegw_message_parsing.py::test_parse_read_response_with_wrong_crc"),
    ("5c00206a06a0a9f5120000a2", "pump_frame", "read resp 43424 = 4853 (word_swap=1)",
     "yozik04/nibe tests/connection/test_nibegw.py::test_read_s32_coil"),
    ("5c00206c01014c", "pump_frame", "write resp OK", "yozik04/nibe test_succesfull_write_response"),
    ("5c00206c01004d", "pump_frame", "write resp DENIED", "yozik04/nibe test_failed_write_response"),
    ("5c00206850449c2d00489cf4014c9c56014d9cf8014e9cc4014f9c4b00509c2800619cef00fda700004ea80a0080a80000ada90000afa9000004bc000067be0000a3b7010063befd006d9cf8006e9cff00eeacc800c5",
     "pump_frame", "data msg whose XOR is 0x5C -> sent as 0xC5", "yozik04/nibe test_parse_multiple_with_5c_checksum"),
    ("5c00206851449c2c00489cf1014c9c59014d9cf8014e9cc4014f9c5c5c00509c2d00619cee00fda700004ea80a0080a80000ada90000afa9000004bc000067be0000a3b7010063befd006d9cf8006e9cff00eeacc80019",
     "pump_frame", "data msg LEN 0x51 = 80 + 1 escape byte", "yozik04/nibe test_parse_multiple_escaped_value"),
    ("5c00206d0d0124e346313235352d313220529f", "pump_frame", "product info F1255-12 R",
     "yozik04/nibe test_parse_product_data"),
    ("5c00206b004b", "pump_frame", "write token", "openhab nibeheatpump contrib/NibeGW/RasPi/nibegw.c test data"),
    ("5c0020690049", "pump_frame", "read token", "openhab nibegw.c test data"),
    ("5c0019ee00f7", "pump_frame", "accessory-version token to RMU40 S1", "yozik04/nibe test_read_product_info_with_extras"),
]

THIRD_PARTY_REQ = [
    ("c06902a0a9a2", 43424, None, "yozik04/nibe tests/connection/test_nibegw.py::test_read_s32_coil"),
    ("c069023930a2", 12345, None, "yozik04/nibe test_build_read_request"),
    ("c06b06393006120f00bf", 12345, bytes.fromhex("06120f00"), "yozik04/nibe test_build_write_request"),
    ("c06b0673b0010000006f", 45171, bytes.fromhex("01000000"), "anerdins/nibepi backend.js sendQueue[0] (alarm reset)"),
    ("c06b06d4be00000000c7", 48852, bytes.fromhex("00000000"), "anerdins/nibepi backend.js sendQueue[1] (word swap 0)"),
]


def selftest() -> int:
    fails = 0
    for hx, kind, note, src in THIRD_PARTY:
        evs = parse_stream(bytes.fromhex(hx))
        # a checksum error is followed by the re-scanned remainder (reported as garbage)
        ok = evs[0].kind == kind and (len(evs) == 1 or kind == "checksum_error")
        print(("OK  " if ok else "FAIL"), note, "|", src)
        fails += not ok
    for hx, reg, val, src in THIRD_PARTY_REQ:
        got = read_request(reg) if val is None else write_request(reg, val)
        ok = got.hex() == hx
        print(("OK  " if ok else "FAIL"), "request", reg, got.hex(), "|", src)
        fails += not ok
    # combined stream with trailing ACK and garbage
    evs = parse_stream(bytes.fromhex("0102" "5c0019ee00f7" "c0ee03ee0101c3" "06" "5c00206d0d0124e346313235352d313220529f"))
    kinds = [e.kind for e in evs]
    ok = kinds == ["garbage", "pump_frame", "acc_frame", "ack", "pump_frame"]
    print(("OK  " if ok else "FAIL"), "mixed stream", kinds)
    fails += not ok
    # escaped value decodes to 0x5C 0x00
    ev = parse_stream(bytes.fromhex("5c00206a074f9c5c5c002c00b2"))[0]
    ok = ev.payload == bytes.fromhex("4f9c5c002c00") and decode_raw("s16", ev.payload[2:4]) == 92
    print(("OK  " if ok else "FAIL"), "unescape -> 40015 raw 92 (9.2 C)")
    fails += not ok
    print("FAILURES:", fails)
    return 1 if fails else 0


# ----------------------------------------------------------------------------------------------
# Synthetic fixture generation
# ----------------------------------------------------------------------------------------------
PROVENANCE = ("SYNTHETIC. Generated by docs/research/tools/nibe_ref.py from the protocol rules in "
              "docs/research/protocol.md. Not captured from any device. Register values, product "
              "string, firmware number and all timings are invented.")


def s16(v: float, scale: int) -> bytes:
    return int(round(v * scale)).to_bytes(2, "little", signed=True)


def u16(v: int) -> bytes:
    return v.to_bytes(2, "little")


def slot(reg: int, two: bytes) -> bytes:
    return reg.to_bytes(2, "little") + two


def synthetic_f730_data_msg() -> tuple[bytes, list]:
    rows = [
        (40004, s16(-3.2, 10), "s16", 10, -3.2),
        (40008, s16(35.1, 10), "s16", 10, 35.1),
        (40012, s16(29.8, 10), "s16", 10, 29.8),
        (40013, s16(50.2, 10), "s16", 10, 50.2),
        (40014, s16(34.8, 10), "s16", 10, 34.8),  # raw 0x015C -> escaped on the wire
        (40025, s16(21.5, 10), "s16", 10, 21.5),
        (40026, s16(6.2, 10), "s16", 10, 6.2),
        (40033, s16(21.4, 10), "s16", 10, 21.4),
        (43005, s16(-120.0, 10), "s16", 10, -120.0),
        (43084, s16(0.0, 100), "s16", 100, 0.0),
        (43086, u16(30), "u8", 1, 30),
        (43136, u16(520), "u16", 10, 52.0),
        (43437, u16(45), "u8", 1, 45),
        (45001, u16(0), "s16", 1, 0),
    ]
    payload = b"".join(slot(r, b) for r, b, *_ in rows)
    # 32-bit: two consecutive slots, low word at N, high word at N+1 (see protocol.md §7.3)
    payload += slot(43416, u16(12345 & 0xFFFF)) + slot(43417, u16(12345 >> 16))
    payload += slot(43420, u16(23456 & 0xFFFF)) + slot(43421, u16(23456 >> 16))
    payload += slot(0xFFFF, b"\x00\x00") * 2
    assert len(payload) == 80
    expected = [{"register": r, "size": sz, "scale": sc, "value": v} for r, _, sz, sc, v in rows]
    expected += [
        {"register": 43416, "size": "s32", "scale": 1, "value": 12345,
         "note": "low word slot 43416, high word slot 43417 (word order HYPOTHESIS, see protocol.md 7.3)"},
        {"register": 43420, "size": "s32", "scale": 1, "value": 23456,
         "note": "low word slot 43420, high word slot 43421"},
    ]
    return payload, expected


def session_counts(events: list) -> dict:
    """Derive the expected counters from the timeline itself, and cross-check that every reply in the
    timeline is exactly what accessory_reply() would send (self-consistency of the fixture)."""
    rx_events = []
    for e in events:
        if e["dir"] == "rx":
            rx_events += parse_stream(bytes.fromhex(e["hex"]))
    tx = [bytes.fromhex(e["hex"]) for e in events if e["dir"] == "tx"]
    replies = []
    read_tokens = 0
    for ev in rx_events:
        pending = None
        if ev.kind == "pump_frame" and ev.addr == ADDR_MODBUS40 and ev.cmd == CMD_READ_REQ and not ev.payload:
            read_tokens += 1
            if read_tokens == 2:  # the session queues one read (43424) before the 2nd read token
                pending = read_request(43424)
        r = accessory_reply(ev, pending_read=pending)
        if r is not None:
            replies.append(r)
    assert replies == tx, (replies, tx)
    pf = [e for e in rx_events if e.kind == "pump_frame"]
    return {
        "pump_frames_for_us": sum(1 for e in pf if e.addr == ADDR_MODBUS40),
        "pump_frames_for_other_addresses": sum(1 for e in pf if e.addr != ADDR_MODBUS40),
        "checksum_errors": sum(1 for e in rx_events if e.kind == "checksum_error"),
        "acks_received": sum(1 for e in rx_events if e.kind == "ack"),
        "acks_sent": sum(1 for b in tx if b == b"\x06"),
        "naks_sent": sum(1 for b in tx if b == b"\x15"),
        "read_requests_sent": sum(1 for b in tx if b[:2] == b"\xc0\x69"),
    }


def gen(out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    A = ADDR_MODBUS40
    vectors = []

    def add(name, direction, frame: bytes, expect: dict, kind="synthetic", provenance=PROVENANCE, status="derived-from-sources"):
        vectors.append({"name": name, "kind": kind, "direction": direction, "hex": frame.hex(),
                        "expect": expect, "rule_status": status, "provenance": provenance})

    read_token = encode_pump_frame(A, CMD_READ_REQ, b"")
    write_token = encode_pump_frame(A, CMD_WRITE_REQ, b"")
    add("read_token", "pump_to_accessory", read_token,
        {"event": "read_token", "reply_if_idle": "06", "reply_if_read_pending": "<read request frame>"},
        status="matches third-party bytes (openHAB nibegw.c test data)")
    add("write_token", "pump_to_accessory", write_token,
        {"event": "write_token", "reply_read_only_gateway": "06"},
        status="matches third-party bytes (openHAB nibegw.c test data)")

    dm, dm_expect = synthetic_f730_data_msg()
    dm_frame = encode_pump_frame(A, CMD_DATA_MSG, dm)
    add("data_msg_f730_20_slots_with_escape", "pump_to_accessory", dm_frame,
        {"event": "data_message", "wire_len_byte": dm_frame[4], "unescaped_payload_len": 80,
         "escaped_bytes": dm_frame[5:-1].count(b"\x5c\x5c"), "reply": "06", "values": dm_expect})

    bad = bytearray(dm_frame); bad[-1] ^= 0x01
    add("data_msg_bad_checksum", "pump_to_accessory", bytes(bad),
        {"event": "checksum_error", "reply": "15"})

    # Find a payload whose checksum hits 0x5C to demonstrate the 0xC5 substitution (search a value).
    for v in range(0, 0x10000):
        p = slot(40004, u16(v)) + slot(0xFFFF, b"\x00\x00") * 19
        f = encode_pump_frame(A, CMD_DATA_MSG, p)
        if f[-1] == 0xC5 and b"\x5c" not in p:
            add("data_msg_checksum_5c_sent_as_c5", "pump_to_accessory", f,
                {"event": "data_message", "calc_xor": "0x5c", "wire_checksum": "0xc5", "reply": "06",
                 "values": [{"register": 40004, "size": "s16", "scale": 10,
                             "value": int.from_bytes(u16(v), "little", signed=True) / 10}]},
                status="rule verified by third-party real capture (yozik04/nibe test_parse_multiple_with_5c_checksum)")
            break

    # read response for 43424 = 4853 under both word-swap settings
    add("read_resp_43424_wordswap1", "pump_to_accessory",
        encode_pump_frame(A, CMD_READ_RESP, (43424).to_bytes(2, "little") + (4853).to_bytes(4, "little")),
        {"event": "read_response", "register": 43424, "value_if_48852_eq_1": 4853, "reply": "06"},
        status="byte-identical to third-party vector yozik04/nibe test_read_s32_coil (word_swap=True)")
    add("read_resp_43424_wordswap0", "pump_to_accessory",
        encode_pump_frame(A, CMD_READ_RESP, (43424).to_bytes(2, "little") + bytes.fromhex("0000f512")),
        {"event": "read_response", "register": 43424, "value_if_48852_eq_0": 4853, "reply": "06"},
        status="HYPOTHESIS: high word first when 48852=0 (NibePi backend/index.js decodeMessage for cmd 0x6A)")
    add("read_resp_40004_s16_negative", "pump_to_accessory",
        encode_pump_frame(A, CMD_READ_RESP, (40004).to_bytes(2, "little") + s16(-12.5, 10) + b"\xff\xff"),
        {"event": "read_response", "register": 40004, "value": -12.5, "reply": "06",
         "note": "upper 2 bytes of 4-byte value ignored for 16-bit registers; synthetic sign-extension FF FF"})
    add("read_resp_40033_sentinel_8000", "pump_to_accessory",
        encode_pump_frame(A, CMD_READ_RESP, (40033).to_bytes(2, "little") + b"\x00\x80\x00\x00"),
        {"event": "read_response", "register": 40033, "value": None, "reason": "s16 -32768 sentinel = no sensor",
         "reply": "06"}, status="HYPOTHESIS (yozik04/nibe encoders.py is_hitting_integer_limit)")
    add("write_resp_ok", "pump_to_accessory", encode_pump_frame(A, CMD_WRITE_RESP, b"\x01"),
        {"event": "write_response", "accepted": True, "reply": "06"},
        status="byte-identical to third-party vector 5c00206c01014c")
    add("write_resp_denied", "pump_to_accessory", encode_pump_frame(A, CMD_WRITE_RESP, b"\x00"),
        {"event": "write_response", "accepted": False, "reply": "06"},
        status="byte-identical to third-party vector 5c00206c01004d")
    pi = b"\x01" + (1234).to_bytes(2, "big") + b"F730 CU 3x400V"
    add("product_info_f730_synthetic", "pump_to_accessory", encode_pump_frame(A, CMD_PRODUCT_INFO, pi),
        {"event": "product_info", "unknown_byte": 1, "firmware": 1234, "model_string": "F730 CU 3x400V",
         "model_family": "F730", "reply": "06",
         "note": "model string and firmware INVENTED; real F730 string unknown"})
    add("rmu40_s1_token_not_for_us", "pump_to_accessory", encode_pump_frame(ADDR_RMU40_S1, CMD_ACCESSORY_VERSION_REQ, b""),
        {"event": "frame_for_other_address", "reply": None},
        status="byte-identical to third-party vector 5c0019ee00f7")
    add("heatpump_internal_16bit_addr_not_for_us", "pump_to_accessory", bytes.fromhex("5c41c9f7007f"),
        {"event": "frame_for_other_address", "addr": "0x41c9", "reply": None}, kind="third_party_published",
        provenance="yozik04/nibe tests/connection/test_nibegw_message_parsing.py::test_parse_token_response_16bit_address (real bus data from another NIBE model, not our F730)",
        status="third-party")

    # accessory -> pump
    add("read_request_40004", "accessory_to_pump", read_request(40004), {"register": 40004})
    add("read_request_43424", "accessory_to_pump", read_request(43424), {"register": 43424},
        status="byte-identical to third-party vector c06902a0a9a2")
    add("ack", "accessory_to_pump", bytes([ACK]), {"meaning": "ACK"})
    add("nak", "accessory_to_pump", bytes([NAK]), {"meaning": "NAK"})
    add("nibepi_startup_write_alarm_reset_45171_DO_NOT_SEND", "accessory_to_pump", write_request(45171, b"\x01\x00\x00\x00"),
        {"register": 45171, "raw": 1, "warning": "NibePi sends this on every start; our gateway must never send it by default"},
        kind="documented_from_source", provenance="anerdins/nibepi backend.js line 37 sendQueue[0]", status="verified-source")
    add("nibepi_startup_write_wordswap_48852_DO_NOT_SEND", "accessory_to_pump", write_request(48852, b"\x00\x00\x00\x00"),
        {"register": 48852, "raw": 0, "warning": "NibePi sends this on every start; our gateway must never send it"},
        kind="documented_from_source", provenance="anerdins/nibepi backend.js line 37 sendQueue[1]", status="verified-source")
    for reg in (40043, 48968):
        body = bytes([START_ACC, CMD_READ_REQ, 2]) + reg.to_bytes(2, "little")
        add(f"read_request_{reg}_checksum_is_5c_UNVERIFIED", "accessory_to_pump", body + b"\x5c",
            {"register": reg, "calc_xor": "0x5c", "alternative_hex_nibe_py_style": (body + b"\xc5").hex(),
             "policy": "do not send until verified on hardware; see protocol.md 6.2"},
            status="CONFLICT between sources")

    doc = {
        "format": "nibe-gateway/protocol-vectors",
        "version": 1,
        "contains_captures_from_our_f730": False,
        "summary": PROVENANCE + " Entries with kind != 'synthetic' carry their own provenance.",
        "generator": "python3 docs/research/tools/nibe_ref.py gen fixtures/serial-captures",
        "spec": "docs/research/protocol-summary-for-implementers.md",
        "vectors": vectors,
    }
    (out_dir / "protocol-vectors.synthetic.json").write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")

    # ---- synthetic bus session (timeline) ----
    t = 0
    lines = [{"format": "nibe-gateway/capture-jsonl", "version": 1, "synthetic": True,
              "device": "SYNTHETIC F730 (no real device)", "serial": "9600 8N1",
              "provenance": PROVENANCE,
              "fields": {"t_us": "monotonic microseconds since start", "dir": "rx = bytes from bus, tx = bytes we sent",
                         "hex": "raw bytes exactly as on the wire (escaped)"}}]

    def rx(b, gap_ms=40):
        nonlocal t
        t += gap_ms * 1000
        lines.append({"t_us": t, "dir": "rx", "hex": b.hex()})
        t += int(len(b) * 1041.7)  # 9600 baud 8N1 ~= 1.0417 ms/byte

    def tx(b, gap_ms=2):
        nonlocal t
        t += gap_ms * 1000
        lines.append({"t_us": t, "dir": "tx", "hex": b.hex()})
        t += int(len(b) * 1041.7)

    rx(b"\x00\xff\x5c", 0)  # line noise incl. a stray start byte
    rx(encode_pump_frame(A, CMD_PRODUCT_INFO, pi)); tx(b"\x06")
    rx(read_token); tx(b"\x06")
    rx(write_token); tx(b"\x06")
    rx(dm_frame); tx(b"\x06")
    rx(encode_pump_frame(ADDR_RMU40_S1, CMD_ACCESSORY_VERSION_REQ, b""))  # not for us: no tx
    rx(read_token); tx(read_request(43424)); rx(b"\x06", 5)  # pump ACKs our request (HYPOTHESIS)
    rx(write_token); tx(b"\x06")
    rx(encode_pump_frame(A, CMD_READ_RESP, (43424).to_bytes(2, "little") + (4853).to_bytes(4, "little"))); tx(b"\x06")
    rx(bytes(bad)); tx(b"\x15")
    rx(dm_frame); tx(b"\x06")
    expected_session = {
        "synthetic": True,
        "provenance": PROVENANCE,
        "assumes": {"word_swap_48852": 1},
        **session_counts(lines[1:]),
        "product": {"model_family": "F730", "firmware": 1234},
        "final_values": {str(e["register"]): e["value"] for e in dm_expect} | {"43424": 4853},
    }
    with (out_dir / "synthetic-f730-session.capture.jsonl").open("w") as fh:
        for ln in lines:
            fh.write(json.dumps(ln, ensure_ascii=False) + "\n")
    (out_dir / "synthetic-f730-session.expected.json").write_text(json.dumps(expected_session, indent=2) + "\n")
    print(f"wrote {len(vectors)} vectors and a {len(lines)-1}-event session to {out_dir}")


if __name__ == "__main__":
    if len(sys.argv) >= 2 and sys.argv[1] == "selftest":
        sys.exit(selftest())
    if len(sys.argv) == 3 and sys.argv[1] == "gen":
        sys.exit(selftest() or gen(Path(sys.argv[2])) or 0)
    print(__doc__)
    sys.exit(2)
