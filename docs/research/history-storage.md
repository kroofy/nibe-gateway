# On-device history storage: numbers and calculations

Date: 2026-10-25 · Supports [ADR 0114](../decisions/0114-history-storage.md) (proposed) · Design only, nothing implemented.

Legend: **[V]** verified in this repo or on the owner's hardware · **[A]** assumption / estimate, to be measured (HP-33).

## 1. Inputs

| Fact | Value | Source |
|---|---|---|
| Polled registers | 30 (up to 64 allowed) | [V] `hardware.json` `poll_registers`, ADR 0101 |
| Poll round | ~34 s for 30 registers → ~0.88 reads/s | [V] owner's F730 |
| Read rate ceiling | ≤ 1 read/s (one read token ~1 s, one outstanding read) | [V] ADR 0101 |
| Value types | s8/u8/s16/u16/s32/u32, all raw values fit i32 (catalog `raw_max` ≤ 2^31−1) | [V] `fixtures/register-catalogs/f730.json` |
| Counters (monotonic) | 41846, 41848, 41850, 44298, 44300 (kWh ×0.1), 43416 (starts), 43420, 43424 (h) | [V] catalog `two_slot_32bit` u32/s32 |
| p3 data partition | ext4, fixed 512 MiB, label `nibe-data`, `defaults,noatime,nodev,nosuid` (→ `data=ordered`, `commit=5`, no `lazytime`) | [V] `export-image/prerun.sh`, `stage-nibe/00-install/00-run.sh` |
| p3 current users | app slots 2 × (~8.4 MB binary + ~0.25 MB admin), `state/`, `system/` (NM, ssh, tailscale), `update-state/` | [V] ADR 0070/0107, ARMv6 release binary 8.38 MB |
| p3 free for history | ~512 − 16 (journal) − 25 (5 % reserved) − ~35 (slots, staging, state) ≈ **430 MiB** | [A] journal/reserve are mke2fs defaults for this size; check with `tune2fs -l` (HP-33) |
| Gateway write access | only `/var/lib/nibe-gateway/state` (`ReadWritePaths`, `ProtectSystem=strict`) | [V] `packaging/systemd/nibe-gateway.service` |
| RAM | 512 MB board; 81 MB in use system-wide after slim OS update | [V] HARDWARE_PENDING 2026-10-25 |
| Clock | no RTC; `systemd-timesyncd` installed; root is a tmpfs overlay, so timesyncd's clock file does not persist | [V] ADR 0117, `nibe-overlay`; boot-time clock value [A] |

## 2. Samples per day

- 86 400 s × 30/34 reads/s = **76 235 samples/day** (2 541 per register; one per register per ~34 s).
- Independent of the number of polled registers: the token rate caps it at ≤ 86 400/day. More registers means
  fewer samples per register, not more raw data. Aggregate tiers do scale with register count.

## 3. Record formats and sizes

All integers little-endian. One file = 512-byte header + blocks. Files are time-partitioned so retention is
`unlink`, never a rewrite.

**File header (512 B):** magic `NIBEHIST`, format version (u16), tier (raw/15m/1h), period start, created-at,
gateway version, register table: up to 64 × {register u16, data_type u8, scale u16, kind u8 (gauge/counter/
state)}, header CRC32. The table makes a file self-describing: a later catalog change cannot reinterpret old
values. A restart with a different poll list starts a new file for the same period (`-1`, `-2` suffix).

**Block header (16 B):** magic u16, kind u8, flags u8 (bit0 clock-synced, bit1 recomputed), count u16,
reserved u16, base_time u32 (unix s, UTC), CRC32 over header+entries. Blocks are 8-byte aligned.

**Raw entry (8 B):** dt u16 (s since base_time), register index u8, flags u8, value i32 (decoded raw, unscaled).
Unavailable values (e.g. −32768 sentinel, timeouts) are not stored; absence = no sample.

**Aggregate entry (20 B):** register index u8, flags u8, count u16, min i32, max i32, last i32, mean f32.
One block per bucket: 16 + 30 × 20 = **616 B** (bucket start = base_time). Counters use `last` (delta = last −
previous bucket's last; negative → reset, delta = last); gauges use mean/min/max; states (alarm, enums) use
last/min/max ("was ever non-zero" = max). Storage is uniform; the query layer picks by `kind`.

| Tier | Per day | Per year | Retention (default) | Size at retention |
|---|---|---|---|---|
| raw (every poll, ~34 s/register), daily files | 0.63 MB (max 0.71 MB at 1 read/s) | — | 14 days | **8.9 MB** |
| 5-min (rejected, for comparison) | 177 KB | 64.8 MB | — | — |
| 15-min aggregates, monthly files | 59 KB | 21.6 MB | 13 months (396 d) | **23.4 MB** |
| 1-hour aggregates, yearly files | 14.8 KB | 5.4 MB | 10 years, size-capped | **5.4 MB/year** |
| 1-day | derived from 1-hour at query time (8 760 rows/year), not stored | | | |

Totals (30 registers): **35.8 MB after 1 year, 43.1 MB after 2, 59.3 MB after 5, 86.2 MB after 10.**
With 64 registers: 15-min tier 49.3 MB, hourly 11.4 MB/year → ~120 MB after 5 years. Default cap
**160 MiB** (≈ 37 % of the free p3 space); when exceeded, delete oldest files first in the order raw beyond
retention → 15-min beyond retention → oldest hourly year. Never let p3 free space fall below 128 MiB (app OTA
staging needs a third binary copy; OS bundles are streamed, never stored on p3).

Why 15-min and not 5-min: 5-min for a year is 65 MB and adds nothing the UI needs once raw covers the last two
weeks (a week view at 15-min = 672 points; a month at 1 h = 720; a year at 1 h = 8 760 → re-bucketed).

## 4. RAM

| Item | Size |
|---|---|
| Pending raw block (1 flush at 60 s) | ~440 B (53 entries) |
| Open accumulators (30 registers × 2 tiers) | < 2 KB |
| Pre-NTP buffer (§7), capped | ≤ 64 k samples × 16 B = 1 MiB |
| Query: streaming 64 KiB read buffer + output bounded by `max_points` | < 1 MB per request, one query at a time |

No history is cached in RAM; the kernel page cache handles repeated reads. Query cost [A]: a 1-year graph scans
the 5.4 MB hourly file (~0.3 s at ~20 MB/s SD read); a 14-day raw view scans 8.9 MB; a 1-year 15-min view
21.6 MB (~1–1.5 s), which `resolution=auto` avoids. CRC is verified per block while scanning.

## 5. Flush cadence vs SD writes

**Important mechanism [V, kernel default]:** what decides how often the SD card is written is when the
application calls `write()`, not only `fsync()`. Dirty page-cache data is written back after
`dirty_expire_centisecs` (30 s) by the flusher. So samples are buffered in the gateway's own memory and written
once per flush interval with `write` + `fdatasync`; writing each sample immediately would mean a writeback every
~30 s regardless of the chosen interval.

Per-flush filesystem cost model [A]:
- **baseline** (plain append, default mount): data pages touched (tail 4 KiB page is rewritten until full)
  + ~12 KiB ext4 journal (descriptor, inode table block, commit block) because `i_size` changes.
- **optimised**: the day's raw file is pre-written with zeros at creation (1 MiB, once a day), so appends are
  in-place overwrites of allocated blocks: no `i_size` change, no block allocation, `fdatasync` writes only the
  data page. mtime updates still dirty the inode and get journaled at the next 5 s commit unless p3 is mounted
  with `lazytime` (separate packaging change, measured first). Zeros also give a clean torn-tail end marker.
- Aggregate files add 96 + 24 bucket-close writes/day at every flush interval.

| Flush | Flushes/day | Payload/flush | Logical writes baseline | 5 y baseline | 5 y × WA 10 | Optimised | 5 y opt. | Max loss on power cut |
|---|---|---|---|---|---|---|---|---|
| 1 min | 1 440 | 440 B | 24.4 MiB/day | 43 GiB | 0.47 TB | 7.1 MiB/day | 12.6 GiB | ≤ 60 s (~53 samples, < 2 rounds) |
| 5 min | 288 | 2.1 KB | 6.4 MiB/day | 11 GiB | 0.12 TB | 2.6 MiB/day | 4.6 GiB | ≤ 5 min (~9 rounds) |
| 15 min | 96 | 6.4 KB | 3.8 MiB/day | 6.7 GiB | 0.07 TB | 2.2 MiB/day | 4.0 GiB | ≤ 15 min |
| 30 min | 48 | 12.7 KB | 3.2 MiB/day | 5.7 GiB | 0.06 TB | 2.2 MiB/day | 4.0 GiB | ≤ 30 min |
| 60 min | 24 | 25.4 KB | 2.8 MiB/day | 5.0 GiB | 0.05 TB | 2.1 MiB/day | 3.8 GiB | ≤ 60 min |

WA 10 = assumed SD-internal write amplification for small random writes (flash pages are typically 16–32 KiB,
erase blocks 4–8 MiB; cheap cards garbage-collect poorly) [A].

Comparisons:
- **OS updates [V size, A frequency]:** each writes a ~756 MiB root image + 20 MB boot; 12 per year = ~45 GiB
  over 5 years, sequential (WA ≈ 1). 1-min history (baseline) is of the same order; optimised it is a quarter.
- **NibePi [A, rough]:** `graph.json` pretty-printed, up to 5 000 `{x,y}` points per series at ~60 B each;
  with ~20 full series that is ~6 MB rewritten per hour ≈ 144 MB/day, plus remounting root rw. Our 1-min
  baseline writes ~6× less per day and never touches the root filesystem.
- **SD endurance [A]:** consumer cards rarely publish TBW. A conservative planning figure for a cheap 16–32 GB
  card is ~1 TB written; "high endurance" cards are rated far higher. 1-min baseline with pessimistic WA uses
  ~0.5 TB in 5 years — inside the budget but not negligible; optimised ~0.14 TB.

**Conclusion:** flush every **60 s**. Wear is not the scarce resource once the format is append-in-place;
data loss is what the owner notices, and a pump shut-down cuts the Pi's power at exactly the moment whose data
is most interesting. 15/30-min intervals save ~2–3 MiB/day (optimised) and lose up to 15–30 min. Make it
configurable (`history.flush_secs`, 10..=1800, default 60); if HP-33 measures > 40 MiB/day added on p3, drop
the default to 300 s rather than adding complexity.

## 6. Crash safety

- Writer order per flush: append raw block(s) to today's file → `fdatasync`; at a bucket close also append the
  aggregate block(s) and `fdatasync` those files. Daily rollover: `ftruncate` the finished day file to its used
  length, `fsync` it, create + pre-zero the next day file, `fsync` the directory. Retention: `unlink` +
  directory `fsync`, once a day. No file is ever rewritten.
- Recovery on start: scan each open file's blocks. A block is valid iff magic, `count` bound, CRC and
  `base_time` ≥ previous block's all hold. On an invalid block, search forward at 8-byte alignment for the next
  valid block (an SD card may tear a whole flash page, damaging already-synced blocks that share the 4 KiB
  page being rewritten); the write position is after the last valid block; everything after it is zeroed again.
  Worst case [A]: the flush interval plus the other blocks in one 4 KiB page (≤ ~9 min at 1-min flush) on a card
  that tears pages; typical: ≤ one flush interval.
- Aggregates are derived data: on start, any bucket missing from the 15-min/1-h files but covered by raw (14 days)
  is recomputed and appended with flag `recomputed`. A torn aggregate tail is therefore always repairable.
- The history writer runs on its own thread (nice 10) behind a bounded channel; the wire thread only does
  `try_send` and counts drops. `fdatasync` latency on SD can be 10–500 ms [A] and must never touch the 10 ms
  reply deadline (ADR 0040).

## 7. Clock

- At boot the clock is wrong until NTP: the Pi Zero has no RTC and timesyncd's clock file sits on the tmpfs
  overlay [V]. What time the Pi starts with (image build time or systemd's built-in epoch) is [A] → HP-33.
- Samples taken before sync are kept in RAM with a monotonic timestamp (`CLOCK_BOOTTIME`), capped at 1 MiB. When
  sync is detected (`/run/systemd/timesync/synchronized` exists, or `ntp_adjtime` reports no `STA_UNSYNC`) they
  are re-anchored: `wall = sync_wall − (sync_mono − sample_mono)`, then written. If the cap fills or the boot ends
  unsynced, they are dropped and counted (`history.unsynced_dropped`). Never write wall times below the binary's
  build time or below the last persisted record.
- After sync, a backward step > 2 s ends the current block; samples with time < last written time are dropped
  (counted). Forward jumps are just gaps.
- A LAN without internet never syncs: history then stays off (visible in status). The owner can point
  timesyncd at a local NTP server (system settings, ADR 0111) — open question whether that is needed.

## 8. Versions and updates

- History lives in `/var/lib/nibe-gateway/state/history/` (p3, inside the existing `ReadWritePaths`), so OS A/B
  updates (ADR 0106) and app slot switches (ADR 0070) do not touch it.
- File names carry the format version: `raw/20261025.v1.nhr`, `15m/202610.v1.nhr`, `1h/2026.v1.nhr`. A newer
  reader reads all older versions; a format change starts new files at the next period. After a rollback, the
  older app skips files with a higher version (graph shows a gap) and writes its own version for the same
  period; the newer app later merges both by time. Retention deletes by period regardless of version. This
  matches ADR 0095's "additive, rollback-safe" rule without a migration step.
- Factory reset keeps history unless an explicit "erase history" is chosen (open question).

## 9. Simulation and tests (local, no hardware)

The history store takes an injected clock and data directory, so the simulator (time advance/freeze) can
generate days of data in seconds. Unit tests: torn tail at every byte offset, a zeroed/garbled 4 KiB page in the
middle, rollover, retention, aggregate recomputation, reader of v1 files with an unknown v2 file present,
pre-sync buffering. A host tool can generate two years of synthetic files for the HP-33 query-timing check.
