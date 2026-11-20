use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::poller::Alt;
use crate::{db_lock, Db};

const UA: &str = "plane-stats/0.1 (adsb.fi feeder)";

#[derive(Debug, Clone, Default)]
pub struct NetworkView {
    pub fetched_at: Option<f64>,
    pub ok: bool,
    pub error: Option<String>,
    pub radius_nm: f64,
    /// Rolling details for every hex in `recent`; a single missed poll must not
    /// drop an aircraft from the comparison.
    pub aircraft: HashMap<String, NetworkAircraft>,
    /// hex -> last poll time it was airborne, pruned at `LIVE_WINDOW_SECS`.
    pub recent: HashMap<String, f64>,
    pub status: NetworkStatus,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NetworkStatus {
    pub last_poll_at: Option<f64>,
    pub ok: bool,
    pub error: Option<String>,
    /// Aircraft in the most recent successful poll.
    pub aircraft: usize,
    pub radius_nm: f64,
    pub snapshot_ok: Option<bool>,
    pub snapshot_at: Option<f64>,
    pub snapshot_status: Option<u16>,
    pub totals: Option<NetworkTotals>,
    pub nearby_feeders: Option<usize>,
    pub nearby_feeders_at: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NetworkTotals {
    pub beast_feeders: Option<i64>,
    pub mlat_feeders: Option<i64>,
    pub total_aircraft: Option<i64>,
}

impl NetworkTotals {
    pub fn beast_str(&self) -> String {
        self.beast_feeders.map(fmt_thousands).unwrap_or_else(|| "-".into())
    }
    pub fn mlat_str(&self) -> String {
        self.mlat_feeders.map(fmt_thousands).unwrap_or_else(|| "-".into())
    }
    pub fn aircraft_str(&self) -> String {
        self.total_aircraft.map(fmt_thousands).unwrap_or_else(|| "-".into())
    }
}

impl NetworkStatus {
    pub fn nearby_label(&self) -> String {
        match self.nearby_feeders {
            Some(n) => format!("{n} within {:.0} NM", self.radius_nm + 50.0),
            None => "-".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkAircraft {
    pub hex: String,
    pub flight: Option<String>,
    pub registration: Option<String>,
    pub typecode: Option<String>,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub year: Option<String>,
    pub alt_ft: Option<i64>,
    pub on_ground: bool,
    pub gs_kts: Option<f64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub dst_nm: Option<f64>,
    pub dir_deg: Option<f64>,
    pub seen: Option<f64>,
    pub mlat_pos: bool,
}

#[derive(Deserialize)]
struct V3Response {
    #[serde(default)]
    ac: Vec<V3Aircraft>,
}

#[derive(Deserialize)]
struct V3Aircraft {
    hex: String,
    #[serde(default)]
    flight: Option<String>,
    #[serde(default)]
    r: Option<String>,
    #[serde(default)]
    t: Option<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default, rename = "ownOp")]
    owner: Option<String>,
    #[serde(default)]
    year: Option<serde_json::Value>,
    #[serde(default)]
    alt_baro: Option<Alt>,
    #[serde(default)]
    gs: Option<f64>,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    dst: Option<f64>,
    #[serde(default)]
    dir: Option<f64>,
    #[serde(default)]
    seen: Option<f64>,
    #[serde(default)]
    mlat: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct StatsResponse {
    #[serde(default, rename = "beastFeeders")]
    beast_feeders: Option<serde_json::Value>,
    #[serde(default, rename = "mlatFeeders")]
    mlat_feeders: Option<serde_json::Value>,
    #[serde(default, rename = "totalAircraft")]
    total_aircraft: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct CoverageFeeder {
    #[serde(default)]
    lat: String,
    #[serde(default)]
    lon: String,
}

static VIEW: LazyLock<RwLock<Arc<NetworkView>>> =
    LazyLock::new(|| RwLock::new(Arc::new(NetworkView::default())));

pub fn latest() -> Arc<NetworkView> {
    let guard = VIEW.read().unwrap_or_else(std::sync::PoisonError::into_inner);
    Arc::clone(&guard)
}

fn update_view(f: impl FnOnce(&mut NetworkView)) {
    let mut guard = VIEW.write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut next = (**guard).clone();
    f(&mut next);
    *guard = Arc::new(next);
}

pub(crate) fn distance_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R_NM: f64 = 3440.065;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R_NM * a.sqrt().asin()
}

#[derive(Clone)]
struct Config {
    api: String,
    web: String,
    radius: f64,
    poll_secs: f64,
    head_secs: f64,
    stats_secs: f64,
    coverage_secs: f64,
}

impl Config {
    fn from_env() -> Self {
        let api = env_str("ADSB_FI_API", "https://opendata.adsb.fi/api");
        let web = env_str("ADSB_FI_WEB_API", "https://api.adsb.fi");
        Self {
            api: api.trim_end_matches('/').to_string(),
            web: web.trim_end_matches('/').to_string(),
            radius: env_f64("NETWORK_RADIUS_NM", 100.0).clamp(10.0, 250.0),
            poll_secs: env_f64("NETWORK_POLL_SECS", 10.0).max(5.0),
            head_secs: env_f64("FEEDER_CHECK_SECS", 35.0).max(10.0),
            stats_secs: env_f64("NETWORK_STATS_SECS", 300.0).max(60.0),
            coverage_secs: env_f64("NETWORK_COVERAGE_SECS", 600.0).max(60.0),
        }
    }
}

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn now_epoch() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs_f64()
}

pub fn fmt_thousands(n: i64) -> String {
    crate::stats::thousands(n)
}

pub async fn run(db: Db) {
    let Some((lat, lon)) = crate::stats::rx_pos() else {
        tracing::warn!("network poller disabled: RX_LAT/RX_LON unset");
        return;
    };
    let cfg = Config::from_env();
    update_view(|v| {
        v.radius_nm = cfg.radius;
        v.status.radius_nm = cfg.radius;
    });
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(10))
        .build()
        .expect("reqwest client");
    tokio::join!(
        poll_loop(db, client.clone(), cfg.clone(), lat, lon),
        head_loop(client.clone(), cfg.clone()),
        stats_loop(client.clone(), cfg.clone()),
        coverage_loop(client, cfg, lat, lon),
    );
}

async fn poll_loop(db: Db, client: reqwest::Client, cfg: Config, lat: f64, lon: f64) {
    let mut backoff_until = 0.0;
    loop {
        let now = now_epoch();
        if now < backoff_until {
            tokio::time::sleep(Duration::from_secs_f64(
                (backoff_until - now).min(cfg.poll_secs),
            ))
            .await;
            continue;
        }
        match fetch_v3(&client, &cfg, lat, lon).await {
            Ok(ac) => {
                let now = now_epoch();
                let current: HashMap<String, NetworkAircraft> =
                    ac.iter().map(|a| (a.hex.clone(), a.clone())).collect();
                update_view(|v| {
                    v.fetched_at = Some(now);
                    v.ok = true;
                    v.error = None;
                    v.radius_nm = cfg.radius;
                    v.status.last_poll_at = Some(now);
                    v.status.ok = true;
                    v.status.error = None;
                    v.status.aircraft = current.len();
                    v.status.radius_nm = cfg.radius;
                    for a in &ac {
                        if !a.on_ground {
                            v.recent.insert(a.hex.clone(), now);
                        }
                    }
                    v.recent
                        .retain(|_, t| now - *t <= crate::stats::LIVE_WINDOW_SECS);
                    v.aircraft.extend(current);
                    let recent = &v.recent;
                    v.aircraft.retain(|hex, _| recent.contains_key(hex));
                });
                if let Err(e) = record_network(&db, now, &ac) {
                    tracing::warn!("network db write failed: {e}");
                }
                tracing::info!("network poll ok: {} aircraft", ac.len());
            }
            Err(FetchError::RateLimited) => {
                update_view(|v| {
                    v.ok = false;
                    v.error = Some("rate limited".into());
                    v.status.ok = false;
                    v.status.error = Some("rate limited".into());
                });
                tracing::warn!("adsb.fi rate limited; backing off 60 s");
                backoff_until = now_epoch() + 60.0;
            }
            Err(FetchError::Other(e)) => {
                update_view(|v| {
                    v.ok = false;
                    v.error = Some(e.clone());
                    v.status.ok = false;
                    v.status.error = Some(e.clone());
                });
                tracing::warn!("network poll failed: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs_f64(cfg.poll_secs)).await;
    }
}

enum FetchError {
    RateLimited,
    Other(String),
}

async fn fetch_v3(
    client: &reqwest::Client,
    cfg: &Config,
    lat: f64,
    lon: f64,
) -> Result<Vec<NetworkAircraft>, FetchError> {
    let url = format!(
        "{}/v3/lat/{lat}/lon/{lon}/dist/{:.0}",
        cfg.api, cfg.radius
    );
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Other(e.to_string()))?;
    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(FetchError::RateLimited);
    }
    if !resp.status().is_success() {
        return Err(FetchError::Other(format!("HTTP {}", resp.status())));
    }
    let body: V3Response = resp
        .json()
        .await
        .map_err(|e| FetchError::Other(e.to_string()))?;
    Ok(body.ac.into_iter().filter_map(normalize).collect())
}

fn normalize(a: V3Aircraft) -> Option<NetworkAircraft> {
    let hex = a.hex.trim().to_lowercase();
    if hex.is_empty() {
        return None;
    }
    let (alt_ft, on_ground) = a
        .alt_baro
        .as_ref()
        .map(Alt::split)
        .unwrap_or((None, false));
    Some(NetworkAircraft {
        hex,
        flight: clean(a.flight),
        registration: clean(a.r),
        typecode: clean(a.t),
        description: clean(a.desc),
        owner: clean(a.owner),
        year: a.year.and_then(value_to_string),
        alt_ft,
        on_ground,
        gs_kts: a.gs,
        lat: a.lat,
        lon: a.lon,
        dst_nm: a.dst,
        dir_deg: a.dir,
        seen: a.seen,
        mlat_pos: a.mlat.as_ref().is_some_and(|v| !v.is_empty()),
    })
}

fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn value_to_string(v: serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => clean(Some(s)),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn record_network(db: &Db, now: f64, ac: &[NetworkAircraft]) -> rusqlite::Result<()> {
    let mut conn = db_lock(db);
    let tx = conn.transaction()?;
    let day = crate::stats::local_day(now);
    {
        let mut day_stmt = tx.prepare(
            "INSERT INTO network_day_aircraft
               (day, hex, first_ts, last_ts, polls, max_alt_ft, max_gs_kts, max_range_nm, mlat_pos)
             VALUES (?1, ?2, ?3, ?3, 1, ?4, ?5, ?6, ?7)
             ON CONFLICT(day, hex) DO UPDATE SET
               last_ts = excluded.last_ts,
               polls = network_day_aircraft.polls + 1,
               max_alt_ft = nullif(MAX(ifnull(network_day_aircraft.max_alt_ft, -2147483648),
                                       ifnull(excluded.max_alt_ft, -2147483648)), -2147483648),
               max_gs_kts = nullif(MAX(ifnull(network_day_aircraft.max_gs_kts, -1e30),
                                        ifnull(excluded.max_gs_kts, -1e30)), -1e30),
               max_range_nm = nullif(MAX(ifnull(network_day_aircraft.max_range_nm, -1e30),
                                         ifnull(excluded.max_range_nm, -1e30)), -1e30),
               mlat_pos = MAX(network_day_aircraft.mlat_pos, excluded.mlat_pos)",
        )?;
        let mut enrich_stmt = tx.prepare(
            "INSERT INTO network_enrichment
               (hex, registration, typecode, description, owner, year, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(hex) DO UPDATE SET
               registration = coalesce(excluded.registration, network_enrichment.registration),
               typecode = coalesce(excluded.typecode, network_enrichment.typecode),
               description = coalesce(excluded.description, network_enrichment.description),
               owner = coalesce(excluded.owner, network_enrichment.owner),
               year = coalesce(excluded.year, network_enrichment.year),
               updated_at = excluded.updated_at",
        )?;
        for a in ac {
            if !a.on_ground {
                day_stmt.execute(params![
                    day.as_str(),
                    a.hex.as_str(),
                    now,
                    a.alt_ft,
                    a.gs_kts,
                    a.dst_nm,
                    if a.mlat_pos { 1i64 } else { 0 },
                ])?;
            }
            enrich_stmt.execute(params![
                a.hex.as_str(),
                a.registration,
                a.typecode,
                a.description,
                a.owner,
                a.year,
                now,
            ])?;
        }
    }
    tx.commit()
}

async fn head_loop(client: reqwest::Client, cfg: Config) {
    let mut head_supported = true;
    let mut last_full = 0.0;
    loop {
        let now = now_epoch();
        if head_supported {
            match client.head(format!("{}/v2/snapshot", cfg.api)).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
                        head_supported = false;
                        update_view(|v| {
                            v.status.snapshot_ok = None;
                            v.status.snapshot_status = Some(405);
                            v.status.snapshot_at = Some(now);
                        });
                    } else {
                        update_view(|v| {
                            v.status.snapshot_ok = Some(status.is_success());
                            v.status.snapshot_status = Some(status.as_u16());
                            v.status.snapshot_at = Some(now);
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!("feeder HEAD failed: {e}");
                    update_view(|v| {
                        v.status.snapshot_ok = Some(false);
                        v.status.snapshot_status = None;
                        v.status.snapshot_at = Some(now);
                    });
                }
            }
        } else if now - last_full >= 300.0 {
            last_full = now;
            match client.get(format!("{}/v2/snapshot", cfg.api)).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let _ = resp.bytes().await; // discard ~1 MB
                    update_view(|v| {
                        v.status.snapshot_ok = Some(status.is_success());
                        v.status.snapshot_status = Some(status.as_u16());
                        v.status.snapshot_at = Some(now);
                    });
                }
                Err(e) => tracing::warn!("feeder snapshot GET failed: {e}"),
            }
        }
        tokio::time::sleep(Duration::from_secs_f64(cfg.head_secs)).await;
    }
}

async fn stats_loop(client: reqwest::Client, cfg: Config) {
    loop {
        match client.get(format!("{}/v1/stats", cfg.web)).send().await {
            Ok(resp) if resp.status().is_success() => match resp.json::<StatsResponse>().await {
                Ok(s) => update_view(|v| {
                    v.status.totals = Some(NetworkTotals {
                        beast_feeders: parse_count(&s.beast_feeders),
                        mlat_feeders: parse_count(&s.mlat_feeders),
                        total_aircraft: parse_count(&s.total_aircraft),
                    });
                }),
                Err(e) => tracing::warn!("adsb.fi stats parse failed: {e}"),
            },
            Ok(resp) => tracing::warn!("adsb.fi stats HTTP {}", resp.status()),
            Err(e) => tracing::warn!("adsb.fi stats failed: {e}"),
        }
        tokio::time::sleep(Duration::from_secs_f64(cfg.stats_secs)).await;
    }
}

fn parse_count(v: &Option<serde_json::Value>) -> Option<i64> {
    match v.as_ref()? {
        serde_json::Value::String(s) => s.trim().parse().ok(),
        serde_json::Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

async fn coverage_loop(client: reqwest::Client, cfg: Config, lat: f64, lon: f64) {
    loop {
        match client.get(format!("{}/v1/coverage", cfg.web)).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<Vec<CoverageFeeder>>().await {
                    Ok(feeders) => {
                        let nearby = feeders
                            .iter()
                            .filter_map(|f| {
                                Some((
                                    f.lat.trim().parse::<f64>().ok()?,
                                    f.lon.trim().parse::<f64>().ok()?,
                                ))
                            })
                            .filter(|(flat, flon)| {
                                distance_nm(lat, lon, *flat, *flon) <= cfg.radius + 50.0
                            })
                            .count();
                        let now = now_epoch();
                        update_view(|v| {
                            v.status.nearby_feeders = Some(nearby);
                            v.status.nearby_feeders_at = Some(now);
                        });
                    }
                    Err(e) => tracing::warn!("adsb.fi coverage parse failed: {e}"),
                }
            }
            Ok(resp) => tracing::warn!("adsb.fi coverage HTTP {}", resp.status()),
            Err(e) => tracing::warn!("adsb.fi coverage failed: {e}"),
        }
        tokio::time::sleep(Duration::from_secs_f64(cfg.coverage_secs)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_degree_of_latitude_is_sixty_nm() {
        let d = distance_nm(45.0, -73.0, 46.0, -73.0);
        assert!((d - 60.0).abs() < 0.2, "{d}");
    }
}
