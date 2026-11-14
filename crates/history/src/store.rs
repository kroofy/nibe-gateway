//! SQLite schema, writes, rollups, retention and queries.
use crate::{Config, Kind, RegisterInfo, Sample};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::path::{Path, PathBuf};

// Raw samples are keyed by time first, so a commit lands on the last page or two and not on one page
// per register. Until 0.1.36 the key was (reg, t) with an index on t. Both layouts have the same
// columns and the same schema version, so an older gateway reads and writes this one after a rollback.
const SCHEMA_VERSION: i64 = 1;
const TIERS: [(&str, i64); 2] = [("agg_15m", 900), ("agg_1h", 3600)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError(pub String);
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError(e.to_string())
    }
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub struct Store {
    conn: Connection,
    path: PathBuf,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        // auto_vacuum only takes effect before the first table exists.
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA cache_size = -1024;
             PRAGMA temp_store = MEMORY;
             PRAGMA busy_timeout = 5000;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(StoreError(format!(
                "history schema {version} is newer than this gateway ({SCHEMA_VERSION}); not recording"
            )));
        }
        if version == 0 {
            conn.execute_batch(
                "BEGIN;
                 CREATE TABLE IF NOT EXISTS registers(
                     id INTEGER PRIMARY KEY, name TEXT NOT NULL, unit TEXT, scale REAL NOT NULL,
                     kind TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS raw(
                     t INTEGER NOT NULL, reg INTEGER NOT NULL, v INTEGER NOT NULL,
                     PRIMARY KEY(t, reg)) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS agg_15m(
                     reg INTEGER NOT NULL, t INTEGER NOT NULL, n INTEGER NOT NULL, min INTEGER NOT NULL,
                     max INTEGER NOT NULL, sum INTEGER NOT NULL, last INTEGER NOT NULL,
                     PRIMARY KEY(reg, t)) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS agg_1h(
                     reg INTEGER NOT NULL, t INTEGER NOT NULL, n INTEGER NOT NULL, min INTEGER NOT NULL,
                     max INTEGER NOT NULL, sum INTEGER NOT NULL, last INTEGER NOT NULL,
                     PRIMARY KEY(reg, t)) WITHOUT ROWID;
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub fn open_read_only(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA cache_size = -512;")?;
        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// Rebuilds a raw table in the layout used until 0.1.36, one day at a time to bound the sort.
    /// Takes about a minute on the Pi, so the writer thread runs it, not `open`.
    pub fn migrate(&mut self) -> Result<bool, StoreError> {
        let old: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND name = 'raw_t'",
            [],
            |r| r.get(0),
        )?;
        if old == 0 {
            return Ok(false);
        }
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "DROP TABLE IF EXISTS raw_new;
             CREATE TABLE raw_new(
                 t INTEGER NOT NULL, reg INTEGER NOT NULL, v INTEGER NOT NULL,
                 PRIMARY KEY(t, reg)) WITHOUT ROWID;",
        )?;
        let (first, last): (Option<i64>, Option<i64>) =
            tx.query_row("SELECT min(t), max(t) FROM raw", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        if let (Some(first), Some(last)) = (first, last) {
            let mut day = first;
            while day <= last {
                tx.execute(
                    "INSERT INTO raw_new(t, reg, v)
                     SELECT t, reg, v FROM raw WHERE t >= ?1 AND t < ?2 ORDER BY t, reg",
                    [day, day + 86_400],
                )?;
                day += 86_400;
            }
        }
        tx.execute_batch("DROP TABLE raw; ALTER TABLE raw_new RENAME TO raw;")?;
        tx.commit()?;
        self.vacuum()?;
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(true)
    }

    pub fn upsert_registers(&mut self, registers: &[RegisterInfo]) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO registers(id, name, unit, scale, kind) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET name = ?2, unit = ?3, scale = ?4, kind = ?5",
            )?;
            for r in registers {
                let kind = match r.kind {
                    Kind::Gauge => "gauge",
                    Kind::Counter => "counter",
                };
                stmt.execute(params![r.id, r.name, r.unit, r.scale, kind])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// One transaction: the samples and, per tier of `TIERS` with a time in `rollup_from`, the
    /// aggregate buckets at or after it. A rollup rewrites one page per register, far more than the
    /// samples cost, so the writer asks for it less often than it commits samples.
    pub fn append(
        &mut self,
        samples: &[Sample],
        rollup_from: [Option<i64>; 2],
    ) -> Result<(), StoreError> {
        if samples.is_empty() && rollup_from == [None, None] {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO raw(reg, t, v) VALUES (?1, ?2, ?3)")?;
            for s in samples {
                stmt.execute(params![s.register, s.t, s.raw])?;
            }
        }
        rollup(&tx, rollup_from)?;
        tx.commit()?;
        Ok(())
    }

    /// Recompute aggregates from raw for every bucket at or after `from` (crash recovery, tests).
    pub fn rollup_since(&mut self, from: i64) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        rollup(&tx, [Some(from); 2])?;
        tx.commit()?;
        Ok(())
    }

    pub fn last_time(&self) -> Result<Option<i64>, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT max(t) FROM raw", [], |r| r.get(0))?)
    }

    pub fn retain(&mut self, now: i64, config: &Config) -> Result<(), StoreError> {
        let day = 86_400;
        self.conn.execute(
            "DELETE FROM raw WHERE t < ?1",
            [now - config.raw_days * day],
        )?;
        self.conn.execute(
            "DELETE FROM agg_15m WHERE t < ?1",
            [now - config.m15_days * day],
        )?;
        self.conn.execute(
            "DELETE FROM agg_1h WHERE t < ?1",
            [now - config.h1_days * day],
        )?;
        self.vacuum()?;
        // Over the cap (owner decision): give up the oldest month of hourly aggregates, so the last
        // two weeks keep their raw samples. With no hourly data left, the oldest raw day, then the
        // oldest day of 15-minute aggregates.
        let steps = [("agg_1h", 30 * day), ("raw", day), ("agg_15m", day)];
        let mut step = 0;
        // Counted in pages, because the files only shrink at the checkpoint below and the deletes
        // themselves grow the write-ahead log.
        let pages = |conn: &Connection| -> Result<u64, StoreError> {
            Ok(conn.query_row(
                "SELECT page_count * page_size FROM pragma_page_count, pragma_page_size",
                [],
                |r| r.get::<_, i64>(0),
            )? as u64)
        };
        while pages(&self.conn)? > config.max_bytes && step < steps.len() {
            let (table, span) = steps[step];
            let removed = self.conn.execute(
                &format!("DELETE FROM {table} WHERE t < (SELECT min(t) FROM {table}) + ?1"),
                [span],
            )?;
            self.vacuum()?;
            if removed == 0 {
                step += 1;
            }
        }
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    pub fn erase(&mut self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "BEGIN; DELETE FROM raw; DELETE FROM agg_15m; DELETE FROM agg_1h; COMMIT;",
        )?;
        self.vacuum()?;
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// Gives free pages back to the filesystem. The pragma frees one page per step, and
    /// `execute_batch` steps once, which left the file at its largest size until 0.1.36.
    fn vacuum(&self) -> Result<(), StoreError> {
        let mut stmt = self.conn.prepare("PRAGMA incremental_vacuum")?;
        let mut rows = stmt.query([])?;
        while rows.next()?.is_some() {}
        Ok(())
    }

    pub fn size_bytes(&self) -> u64 {
        let wal = PathBuf::from(format!("{}-wal", self.path.display()));
        [&self.path, &wal]
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum()
    }

    pub fn query(&self, q: &Query) -> Result<Vec<Series>, StoreError> {
        let span = (q.to - q.from).max(1);
        let resolution = match q.resolution {
            Resolution::Auto if span <= 2 * 86_400 => Resolution::Raw,
            Resolution::Auto if span <= 60 * 86_400 => Resolution::M15,
            Resolution::Auto if span <= 400 * 86_400 => Resolution::H1,
            Resolution::Auto => Resolution::D1,
            other => other,
        };
        let (table, tier) = match resolution {
            Resolution::Raw => ("raw", 1),
            Resolution::M15 => ("agg_15m", 900),
            Resolution::H1 | Resolution::Auto => ("agg_1h", 3600),
            Resolution::D1 => ("agg_1h", 86_400),
        };
        // Wider buckets when the range holds more than max_points, in whole tier steps.
        let max_points = q.max_points.clamp(10, 10_000) as i64;
        let wanted = (span + max_points - 1) / max_points;
        let bucket = tier.max((wanted + tier - 1) / tier * tier);
        let start = q.from - q.from.rem_euclid(bucket);
        let mut meta = self
            .conn
            .prepare("SELECT name, unit, scale, kind FROM registers WHERE id = ?1")?;
        let mut scales = Vec::new();
        let mut out = Vec::new();
        for &register in &q.registers {
            let (name, unit, scale, kind) = meta
                .query_row([register], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, f64>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })
                .unwrap_or_else(|_| (format!("register {register}"), None, 1.0, "gauge".into()));
            scales.push(scale);
            out.push(Series {
                register,
                name,
                unit,
                kind,
                resolution: resolution.label(),
                bucket_s: bucket,
                points: Vec::new(),
            });
        }
        let mut open: Vec<Option<(i64, Row)>> = q.registers.iter().map(|_| None).collect();
        // Round away binary noise (51.800000000000004) without touching real precision.
        let tidy = |v: f64| (v * 1e6).round() / 1e6;
        let point = |b: i64, acc: &Row, scale: f64| Point {
            t: b * 1000,
            mean: tidy(acc.sum as f64 / acc.n.max(1) as f64 * scale),
            min: tidy(acc.min as f64 * scale),
            max: tidy(acc.max as f64 * scale),
            last: tidy(acc.last as f64 * scale),
        };
        let mut add = |i: usize, row: Row| {
            let b = row.t - row.t.rem_euclid(bucket);
            match &mut open[i] {
                Some((cb, acc)) if *cb == b => {
                    acc.n += row.n;
                    acc.sum += row.sum;
                    acc.min = acc.min.min(row.min);
                    acc.max = acc.max.max(row.max);
                    acc.last = row.last;
                }
                slot => {
                    if let Some((cb, acc)) = slot.replace((b, row)) {
                        out[i].points.push(point(cb, &acc, scales[i]));
                    }
                }
            }
        };
        let read = |r: &rusqlite::Row| {
            Ok((
                r.get::<_, u32>(0)?,
                Row {
                    t: r.get(1)?,
                    n: r.get(2)?,
                    min: r.get(3)?,
                    max: r.get(4)?,
                    sum: r.get(5)?,
                    last: r.get(6)?,
                },
            ))
        };
        if table == "raw" {
            // Raw is in time order, so one pass over the range serves every register asked for.
            let wanted: Vec<String> = q.registers.iter().map(u32::to_string).collect();
            let mut stmt = self.conn.prepare(&format!(
                "SELECT reg, t, 1, v, v, v, v FROM raw
                 WHERE t >= ?1 AND t < ?2 AND reg IN ({}) ORDER BY t",
                wanted.join(",")
            ))?;
            for row in stmt.query_map(params![start, q.to], read)? {
                let (register, row) = row?;
                for (i, _) in q.registers.iter().enumerate().filter(|r| *r.1 == register) {
                    add(i, row);
                }
            }
        } else {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT reg, t, n, min, max, sum, last FROM {table}
                 WHERE reg = ?1 AND t >= ?2 AND t < ?3 ORDER BY t"
            ))?;
            for (i, &register) in q.registers.iter().enumerate() {
                for row in stmt.query_map(params![register, start, q.to], read)? {
                    add(i, row?.1);
                }
            }
        }
        for (i, last) in open.into_iter().enumerate() {
            if let Some((cb, acc)) = last {
                out[i].points.push(point(cb, &acc, scales[i]));
            }
        }
        Ok(out)
    }
}

#[derive(Clone, Copy)]
struct Row {
    t: i64,
    n: i64,
    min: i64,
    max: i64,
    sum: i64,
    last: i64,
}

fn rollup(conn: &Connection, from: [Option<i64>; 2]) -> Result<(), StoreError> {
    for ((table, width), from) in TIERS.into_iter().zip(from) {
        let Some(from) = from else { continue };
        let start = from - from.rem_euclid(width);
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO {table}(reg, t, n, min, max, sum, last)
                 SELECT reg, (t / {width}) * {width} AS b, count(*), min(v), max(v), sum(v),
                        (SELECT r2.v FROM raw r2
                          WHERE r2.reg = raw.reg AND r2.t >= (raw.t / {width}) * {width}
                            AND r2.t < (raw.t / {width}) * {width} + {width}
                          ORDER BY r2.t DESC LIMIT 1)
                 FROM raw WHERE t >= ?1 GROUP BY reg, b"
            ),
            [start],
        )?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Auto,
    Raw,
    M15,
    H1,
    D1,
}
impl Resolution {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "auto" => Self::Auto,
            "raw" => Self::Raw,
            "15m" => Self::M15,
            "1h" => Self::H1,
            "1d" => Self::D1,
            _ => return None,
        })
    }
    fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Raw => "raw",
            Self::M15 => "15m",
            Self::H1 => "1h",
            Self::D1 => "1d",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Query {
    pub registers: Vec<u32>,
    /// Unix seconds, inclusive.
    pub from: i64,
    /// Unix seconds, exclusive.
    pub to: i64,
    pub resolution: Resolution,
    pub max_points: usize,
}

/// One bucket, scaled to engineering units. `t` is the bucket start in Unix milliseconds.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Point {
    pub t: i64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub last: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Series {
    pub register: u32,
    pub name: String,
    pub unit: Option<String>,
    /// `gauge` (use mean/min/max) or `counter` (use last; consecutive differences are usage).
    pub kind: String,
    pub resolution: &'static str,
    pub bucket_s: i64,
    pub points: Vec<Point>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("h.sqlite3")).unwrap();
        (dir, s)
    }
    fn samples(reg: u32, from: i64, step: i64, values: &[i64]) -> Vec<Sample> {
        values
            .iter()
            .enumerate()
            .map(|(i, &raw)| Sample {
                register: reg,
                t: from + i as i64 * step,
                raw,
            })
            .collect()
    }
    #[test]
    fn rollups_queries_and_scaling() {
        let (_d, mut s) = store();
        s.upsert_registers(&[RegisterInfo {
            id: 40004,
            name: "BT1".into(),
            unit: Some("°C".into()),
            scale: 0.1,
            kind: Kind::Gauge,
        }])
        .unwrap();
        // One hour of readings every 30 s: 0, 1, 2, … (×0.1 °C), split over two commits.
        let all = samples(40004, 7200, 30, &(0..120).collect::<Vec<_>>());
        // The first commit leaves its aggregates to the second one.
        s.append(&all[..50], [None; 2]).unwrap();
        s.append(&all[50..], [Some(all[0].t); 2]).unwrap();
        let q = |resolution, from, to, max_points| Query {
            registers: vec![40004],
            from,
            to,
            resolution,
            max_points,
        };
        let raw = s.query(&q(Resolution::Auto, 7200, 10_800, 1000)).unwrap();
        assert_eq!((raw[0].resolution, raw[0].points.len()), ("raw", 120));
        assert_eq!(raw[0].unit.as_deref(), Some("°C"));
        let m15 = s.query(&q(Resolution::M15, 7200, 10_800, 1000)).unwrap();
        assert_eq!(m15[0].points.len(), 4);
        let first = &m15[0].points[0];
        assert_eq!(first.t, 7_200_000);
        assert!(
            (first.mean - 1.45).abs() < 1e-9,
            "mean of 0..=29 is 14.5 ×0.1"
        );
        assert!((first.max - 2.9).abs() < 1e-9 && (first.last - 2.9).abs() < 1e-9);
        let h1 = s.query(&q(Resolution::H1, 7200, 10_800, 1000)).unwrap();
        assert_eq!(h1[0].points.len(), 1);
        assert!((h1[0].points[0].mean - 5.95).abs() < 1e-9);
        // Re-bucketing: 120 raw points into at most 12 buckets of 300 s.
        let coarse = s.query(&q(Resolution::Raw, 7200, 10_800, 12)).unwrap();
        assert_eq!((coarse[0].bucket_s, coarse[0].points.len()), (300, 12));
        // Unknown register: empty series with a placeholder name.
        let none = s
            .query(&Query {
                registers: vec![1],
                ..q(Resolution::Raw, 0, 100, 10)
            })
            .unwrap();
        assert!(none[0].points.is_empty());
    }
    #[test]
    fn retention_and_erase() {
        let (_d, mut s) = store();
        let day = 86_400;
        s.append(&samples(1, 0, day, &[1, 2, 3, 4, 5]), [Some(0); 2])
            .unwrap();
        let mut config = Config::new("/unused".into());
        config.raw_days = 2;
        config.m15_days = 3;
        s.retain(5 * day, &config).unwrap();
        let count = |s: &Store, table: &str| -> i64 {
            s.conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(count(&s, "raw"), 2, "days 3 and 4 kept");
        assert_eq!(count(&s, "agg_15m"), 3);
        assert_eq!(count(&s, "agg_1h"), 5);
        assert_eq!(s.last_time().unwrap(), Some(4 * day));
        // Over the size cap the oldest hourly aggregates go first, a month at a time; raw stays.
        let hours = samples(1, 5 * day, 3600, &(0..24 * 120).collect::<Vec<_>>());
        s.append(&hours, [Some(5 * day); 2]).unwrap();
        s.retain(125 * day, &config).unwrap(); // moves the write-ahead log into the file
        let (raw, hourly) = (count(&s, "raw"), count(&s, "agg_1h"));
        config.max_bytes = s.size_bytes() - 8192; // two pages too many
        s.retain(125 * day, &config).unwrap();
        assert_eq!(count(&s, "raw"), raw);
        assert_eq!(
            count(&s, "agg_1h"),
            hourly - 5 - 24 * 25,
            "five old rows and 25 days gone"
        );
        assert!(s.size_bytes() <= config.max_bytes);
        s.erase().unwrap();
        assert_eq!((count(&s, "raw"), count(&s, "agg_1h")), (0, 0));
        // Emptied pages go back to the filesystem: the size cap compares against the file size.
        s.append(
            &samples(1, 0, 1, &(0..20_000).collect::<Vec<_>>()),
            [None; 2],
        )
        .unwrap();
        s.erase().unwrap();
        assert!(s.size_bytes() < 64 * 1024, "{} bytes", s.size_bytes());
    }
    #[test]
    fn old_layout_is_rebuilt_in_time_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.sqlite3");
        drop(Store::open(&path).unwrap());
        // The raw table as 0.1.36 created it, with three days of two registers.
        let old = Connection::open(&path).unwrap();
        old.execute_batch(
            "DROP TABLE raw;
             CREATE TABLE raw(reg INTEGER NOT NULL, t INTEGER NOT NULL, v INTEGER NOT NULL,
                 PRIMARY KEY(reg, t)) WITHOUT ROWID;
             CREATE INDEX raw_t ON raw(t);",
        )
        .unwrap();
        for reg in [1, 2] {
            for i in 0..300 {
                old.execute(
                    "INSERT INTO raw(reg, t, v) VALUES (?1, ?2, ?3)",
                    params![reg, 1000 + i * 900, reg * 1000 + i],
                )
                .unwrap();
            }
        }
        drop(old);
        let mut s = Store::open(&path).unwrap();
        assert!(s.migrate().unwrap());
        assert!(!s.migrate().unwrap(), "nothing left to do");
        let key: String = s
            .conn
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE name = 'raw'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(key.contains("PRIMARY KEY(t, reg)"), "{key}");
        let (n, sum): (i64, i64) = s
            .conn
            .query_row("SELECT count(*), sum(v) FROM raw", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((n, sum), (600, 300 * 3000 + 2 * (299 * 300 / 2)));
        // Both registers in one pass, each with its own points; a register asked twice gets both.
        let series = s
            .query(&Query {
                registers: vec![2, 1, 2],
                from: 0,
                to: 1_000_000,
                resolution: Resolution::Raw,
                max_points: 10_000,
            })
            .unwrap();
        assert_eq!(
            series.iter().map(|x| x.points.len()).collect::<Vec<_>>(),
            [300; 3]
        );
        assert_eq!(
            (series[0].points[0].last, series[1].points[299].last),
            (2000.0, 1299.0)
        );
    }
    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.sqlite3");
        drop(Store::open(&path).unwrap());
        Connection::open(&path)
            .unwrap()
            .execute_batch("PRAGMA user_version = 9;")
            .unwrap();
        assert!(Store::open(&path).err().unwrap().0.contains("newer"));
    }
}
