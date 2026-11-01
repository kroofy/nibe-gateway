//! History queries and CSV export. Admin session or any API key (read scope).
use crate::{auth::AuthError, provisioning::ProvisioningState};
use axum::{
    extract::{Query as QueryParams, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use nibe_history::{History, Query, Resolution, Series};
use serde::Serialize;
use std::collections::HashMap;
use utoipa::ToSchema;

const MAX_REGISTERS: usize = 8;

#[derive(Serialize, ToSchema)]
pub struct HistoryPoint {
    /// Bucket start, Unix milliseconds.
    pub t: i64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub last: f64,
}
#[derive(Serialize, ToSchema)]
pub struct HistorySeries {
    pub register: u32,
    pub name: String,
    pub unit: Option<String>,
    /// `gauge` (plot mean/min/max) or `counter` (plot `last`; differences are usage).
    pub kind: String,
    /// raw, 15m, 1h or 1d.
    pub resolution: String,
    pub bucket_s: i64,
    pub points: Vec<HistoryPoint>,
}
#[derive(Serialize, ToSchema)]
pub struct HistoryResponse {
    pub from: i64,
    pub to: i64,
    pub series: Vec<HistorySeries>,
}
#[derive(Serialize, ToSchema)]
pub struct HistoryStatus {
    pub enabled: bool,
    pub size_bytes: u64,
    pub last_flush_ms: Option<u64>,
    pub last_flush_duration_ms: Option<u64>,
    pub samples_written: u64,
    pub samples_dropped_clock: u64,
    pub buffered: usize,
    pub clock_synced: bool,
    pub last_error: Option<String>,
}

fn history(state: &ProvisioningState) -> Result<History, AuthError> {
    state.history.clone().ok_or_else(AuthError::unavailable)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn invalid(message: &'static str) -> AuthError {
    AuthError::new(StatusCode::UNPROCESSABLE_ENTITY, "validation", message)
}

/// Parses `registers`, `from`, `to` (Unix ms; default the last 24 h), `resolution`, `max_points`.
fn parse(params: &HashMap<String, String>) -> Result<(Query, i64, i64), AuthError> {
    let registers: Vec<u32> = params
        .get("registers")
        .ok_or_else(|| invalid("registers is required, e.g. registers=40004,40013"))?
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().parse::<u32>())
        .collect::<Result<_, _>>()
        .map_err(|_| invalid("registers must be comma-separated register numbers"))?;
    if registers.is_empty() || registers.len() > MAX_REGISTERS {
        return Err(invalid("give 1 to 8 registers"));
    }
    let number = |key: &str| -> Result<Option<i64>, AuthError> {
        params
            .get(key)
            .map(|v| {
                v.parse::<i64>()
                    .map_err(|_| invalid("from/to/max_points must be integers"))
            })
            .transpose()
    };
    let to = number("to")?.unwrap_or_else(now_ms);
    let from = number("from")?.unwrap_or(to - 24 * 3600 * 1000);
    if from >= to || to - from > 11 * 366 * 86_400 * 1000 {
        return Err(invalid("from must be before to, at most 11 years apart"));
    }
    let resolution = match params.get("resolution") {
        Some(r) => {
            Resolution::parse(r).ok_or_else(|| invalid("resolution is auto, raw, 15m, 1h or 1d"))?
        }
        None => Resolution::Auto,
    };
    let max_points = number("max_points")?.unwrap_or(1000).clamp(10, 10_000) as usize;
    Ok((
        Query {
            registers,
            from: from.div_euclid(1000),
            to: (to + 999).div_euclid(1000),
            resolution,
            max_points,
        },
        from,
        to,
    ))
}

async fn run(history: History, query: Query) -> Result<Vec<Series>, AuthError> {
    tokio::task::spawn_blocking(move || history.query(&query))
        .await
        .map_err(|_| AuthError::unavailable())?
        .map_err(|_| AuthError::unavailable())
}

#[utoipa::path(get, path = "/api/v1/history", params(
    ("registers" = String, Query, description = "1-8 comma-separated register numbers"),
    ("from" = Option<i64>, Query, description = "Unix ms, default 24 h before `to`"),
    ("to" = Option<i64>, Query, description = "Unix ms, default now"),
    ("resolution" = Option<String>, Query, description = "auto (default), raw, 15m, 1h or 1d"),
    ("max_points" = Option<i64>, Query, description = "10-10000, default 1000; buckets widen to fit"),
), description = "Recorded history of polled registers: raw 14 days, 15-minute 13 months, hourly 10 years.", responses((status = 200, body = HistoryResponse), (status = 422), (status = 503)))]
pub async fn get_history(
    State(state): State<ProvisioningState>,
    QueryParams(params): QueryParams<HashMap<String, String>>,
) -> Result<Json<HistoryResponse>, AuthError> {
    let history = history(&state)?;
    let (query, from, to) = parse(&params)?;
    let series = run(history, query).await?;
    Ok(Json(HistoryResponse {
        from,
        to,
        series: series
            .into_iter()
            .map(|s| HistorySeries {
                register: s.register,
                name: s.name,
                unit: s.unit,
                kind: s.kind,
                resolution: s.resolution.into(),
                bucket_s: s.bucket_s,
                points: s
                    .points
                    .into_iter()
                    .map(|p| HistoryPoint {
                        t: p.t,
                        mean: p.mean,
                        min: p.min,
                        max: p.max,
                        last: p.last,
                    })
                    .collect(),
            })
            .collect(),
    }))
}

#[utoipa::path(get, path = "/api/v1/history/export.csv", description = "Same query as /api/v1/history as CSV (one row per register and bucket).", responses((status = 200, content_type = "text/csv"), (status = 422)))]
pub async fn export_csv(
    State(state): State<ProvisioningState>,
    QueryParams(params): QueryParams<HashMap<String, String>>,
) -> Result<Response, AuthError> {
    let history = history(&state)?;
    let (query, _, _) = parse(&params)?;
    let series = run(history, query).await?;
    let mut csv = String::from("time_utc,register,name,unit,resolution,mean,min,max,last\n");
    for s in &series {
        let name = s.name.replace('"', "\"\"");
        let unit = s.unit.as_deref().unwrap_or("");
        for p in &s.points {
            csv.push_str(&format!(
                "{},{},\"{}\",{},{},{},{},{},{}\n",
                utc(p.t / 1000),
                s.register,
                name,
                unit,
                s.resolution,
                p.mean,
                p.min,
                p.max,
                p.last
            ));
        }
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"nibe-history.csv\"",
            ),
        ],
        csv,
    )
        .into_response())
}

#[utoipa::path(get, path = "/api/v1/history/status", responses((status = 200, body = HistoryStatus), (status = 503)))]
pub async fn status(
    State(state): State<ProvisioningState>,
) -> Result<Json<HistoryStatus>, AuthError> {
    let s = history(&state)?.status();
    Ok(Json(HistoryStatus {
        enabled: s.enabled,
        size_bytes: s.size_bytes,
        last_flush_ms: s.last_flush_ms,
        last_flush_duration_ms: s.last_flush_duration_ms,
        samples_written: s.samples_written,
        samples_dropped_clock: s.samples_dropped_clock,
        buffered: s.buffered,
        clock_synced: s.clock_synced,
        last_error: s.last_error,
    }))
}

/// `YYYY-MM-DDTHH:MM:SSZ` for Unix seconds (proleptic Gregorian, days-from-civil inverse).
fn utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utc_formatting() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(1_792_941_175), "2026-10-25T15:12:55Z");
        assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
    }
}
