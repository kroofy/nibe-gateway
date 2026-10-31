#!/usr/bin/env python3
"""Convert NibePi log.js RAW chunks to bounded capture-jsonl v1 (RX only).

NibePi's local wall-clock milliseconds are relative timing evidence, not physical
UART timings. Reject backwards clock jumps rather than inventing intervals.
Never copy non-RAW messages, paths, timestamps or device identity into output.
"""
import argparse
from datetime import datetime
import json
import os
from pathlib import Path
import re
import tempfile

RAW = re.compile(r"^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3}) RAW: \[([^\]]*)\]$")
MAX_BYTES = 8 * 1024 * 1024
MAX_RECORDS = 65536


def convert(source, target, synthetic=False):
    header = dict(format="nibe-gateway/capture-jsonl", version=1, synthetic=synthetic,
                  device="NibePi RAW import; identity unverified", serial="9600 8N1",
                  provenance="NibePi backend.js RAW chunks via log.js; RX only; relative local wall-clock ms, not UART timing; inspect and sanitize before sharing")
    total = 0

    def emit(value):
        nonlocal total
        line = json.dumps(value, separators=(",", ":")) + "\n"
        total += len(line.encode())
        if total > MAX_BYTES:
            raise ValueError("capture exceeds replay's 8 MiB limit; split the source log")
        target.write(line)

    emit(header)
    first = previous = None
    count = 0
    for number, line in enumerate(iter(lambda: source.readline(16385), ""), 1):
        if len(line) > 16384:
            raise ValueError(f"line {number}: oversized log line")
        if " RAW:" not in line:
            continue
        match = RAW.fullmatch(line.rstrip("\r\n"))
        if not match:
            raise ValueError(f"line {number}: malformed RAW record")
        try:
            timestamp = datetime.strptime(match[1], "%Y-%m-%d %H:%M:%S.%f")
            parts = match[2].split(",")
            if not 1 <= len(parts) <= 4096 or any(not re.fullmatch(r"\s*\d{1,3}\s*", p) for p in parts):
                raise ValueError()
            chunk = bytes(int(p) for p in parts)
        except ValueError:
            raise ValueError(f"line {number}: invalid timestamp or byte list") from None
        first = first or timestamp
        if previous is not None and timestamp < previous:
            raise ValueError(f"line {number}: clock moved backwards; split the log at the clock change")
        previous = timestamp
        count += 1
        if count > MAX_RECORDS:
            raise ValueError("too many chunks; split the log (maximum 65536)")
        elapsed = timestamp - first
        t_us = (elapsed.days * 86400 + elapsed.seconds) * 1000000 + elapsed.microseconds
        emit(dict(t_us=t_us, dir="rx", hex=chunk.hex()))
    if not count:
        raise ValueError("no RAW records found")
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path, help="new private .capture.jsonl file")
    parser.add_argument("--synthetic", action="store_true", help="mark synthetic test input explicitly")
    args = parser.parse_args()
    # Never leave a partial valid-looking capture or overwrite an existing file.
    temporary = None
    try:
        with args.source.open(encoding="utf-8") as source, tempfile.NamedTemporaryFile(
                mode="w", encoding="utf-8", dir=args.output.parent, delete=False) as target:
            temporary = Path(target.name)
            count = convert(source, target, args.synthetic)
            target.flush()
            os.fsync(target.fileno())
        os.link(temporary, args.output)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Import failed: {error}\n")
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    print(f"Imported {count} RX chunks; timing is logger wall-clock, identity remains unverified")


if __name__ == "__main__":
    main()
