//! Flight routes from adsbdb, held in memory only.
//!
//! adsbdb's terms forbid copying, publishing or incorporating its route data
//! into another database. This module therefore never writes routes to SQLite
//! and never exports them: a bounded, expiring in-memory cache is the whole
//! store, and only the aircraft profile renders a result.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use std::time::Duration;

use rusqlite::params;
use serde::Deserialize;

use crate::Db;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Route {
    pub callsign: String,
    pub airline: Option<String>,
    pub origin_iata: String,
    pub origin_icao: String,
    pub origin_name: String,
    pub dest_iata: String,
    pub dest_icao: String,
    pub dest_name: String,
    pub fetched_at: f64,
}

impl Route {
    pub fn summary(&self) -> String {
        format!("{} → {}", self.origin_iata, self.dest_iata)
    }
    pub fn airports(&self) -> String {
        format!("{} → {}", self.origin_name, self.dest_name)
    }
}

struct CachedRoute {
    route: Option<Route>,
    fetched_at: f64,
}

static CACHE: LazyLock<RwLock<HashMap<String, CachedRoute>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

const CACHE_CAP: usize = 5_000;

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(default)
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs_f64()
}

/// Trim, upper-case and accept only 2–8 alphanumerics. Callsigns in the
/// ledger can carry trailing spaces or a stray `*`.
fn normalize(callsign: &str) -> Option<String> {
    let s: String = callsign.trim().to_uppercase();
    ((2..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(s)
}

#[derive(Deserialize)]
struct Response {
    response: Option<FlightRouteResponse>,
}

#[derive(Deserialize)]
struct FlightRouteResponse {
    flightroute: Option<FlightRoute>,
}

#[derive(Deserialize)]
struct FlightRoute {
    callsign: Option<String>,
    airline: Option<Airline>,
    origin: Option<Airport>,
    destination: Option<Airport>,
}

#[derive(Deserialize)]
struct Airline {
    name: Option<String>,
}

#[derive(Deserialize)]
struct Airport {
    iata_code: Option<String>,
    icao_code: Option<String>,
    name: Option<String>,
}

fn parse_route(body: &str, fetched_at: f64) -> Option<Route> {
    let parsed: Response = serde_json::from_str(body).ok()?;
    let fr = parsed.response?.flightroute?;
    let origin = fr.origin?;
    let destination = fr.destination?;
    Some(Route {
        callsign: fr.callsign.unwrap_or_default(),
        airline: fr.airline.and_then(|a| a.name),
        origin_iata: origin.iata_code?,
        origin_icao: origin.icao_code.unwrap_or_default(),
        origin_name: origin.name.unwrap_or_default(),
        dest_iata: destination.iata_code?,
        dest_icao: destination.icao_code.unwrap_or_default(),
        dest_name: destination.name.unwrap_or_default(),
        fetched_at,
    })
}

fn cached(callsign: &str, now: f64, pos_ttl: f64, neg_ttl: f64) -> Option<Option<Route>> {
    let cache = CACHE.read().unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = cache.get(callsign)?;
    let ttl = if entry.route.is_some() { pos_ttl } else { neg_ttl };
    (now - entry.fetched_at < ttl).then(|| entry.route.clone())
}

fn store(callsign: String, route: Option<Route>, now: f64) {
    let mut cache = CACHE.write().unwrap_or_else(std::sync::PoisonError::into_inner);
    if cache.len() >= CACHE_CAP {
        let mut ages: Vec<(String, f64)> =
            cache.iter().map(|(k, v)| (k.clone(), v.fetched_at)).collect();
        ages.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (key, _) in ages.into_iter().take(CACHE_CAP / 10) {
            cache.remove(&key);
        }
    }
    cache.insert(callsign, CachedRoute { route, fetched_at: now });
}

/// Route for the callsign this aircraft last flew. No network on this path.
pub fn for_hex(db: &Db, hex: &str) -> Option<Route> {
    let conn = crate::db_lock(db);
    let callsign: Option<String> = conn
        .query_row("SELECT last_flight FROM aircraft WHERE hex = ?1", params![hex], |r| r.get(0))
        .ok()
        .flatten()
        .or_else(|| {
            conn.query_row(
                "SELECT callsign FROM aircraft_callsigns WHERE hex = ?1
                 ORDER BY last_seen DESC LIMIT 1",
                params![hex],
                |r| r.get(0),
            )
            .ok()
        });
    drop(conn);
    let callsign = normalize(callsign.as_deref()?)?;
    let cache = CACHE.read().unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.get(&callsign).and_then(|e| e.route.clone())
}

pub async fn run(db: Db) {
    if std::env::var("ROUTE_LOOKUP").map(|v| v.eq_ignore_ascii_case("off")).unwrap_or(false) {
        tracing::info!("route lookup disabled (ROUTE_LOOKUP=off)");
        return;
    }
    let api = std::env::var("ROUTE_API").unwrap_or_else(|_| "https://api.adsbdb.com/v0".to_string());
    let poll_secs = env_f64("ROUTE_POLL_SECS", 60.0).max(10.0);
    let pos_ttl = env_f64("ROUTE_TTL_SECS", 604_800.0);
    let neg_ttl = env_f64("ROUTE_NEG_TTL_SECS", 2_592_000.0);
    let client = match reqwest::Client::builder().timeout(Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("route client failed: {e}");
            return;
        }
    };

    loop {
        let candidates = collect_candidates(&db, now());
        for callsign in candidates {
            let ts = now();
            if cached(&callsign, ts, pos_ttl, neg_ttl).is_some() {
                continue;
            }
            let url = format!("{}/callsign/{}", api.trim_end_matches('/'), callsign);
            match client
                .get(&url)
                .header("User-Agent", "plane-stats/0.1 (personal ADS-B dashboard)")
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        match resp.text().await {
                            Ok(body) => match parse_route(&body, ts) {
                                Some(route) => store(callsign, Some(route), ts),
                                None => store(callsign, None, ts),
                            },
                            Err(e) => tracing::warn!("route read failed for {callsign}: {e}"),
                        }
                    } else if status == reqwest::StatusCode::NOT_FOUND {
                        store(callsign, None, ts);
                    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        tracing::warn!("route lookup rate limited; backing off 60 s");
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        break;
                    } else {
                        tracing::warn!("route lookup {callsign} returned {status}");
                    }
                }
                Err(e) => tracing::warn!("route lookup {callsign} failed: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        prune(pos_ttl, neg_ttl);
        tokio::time::sleep(Duration::from_secs_f64(poll_secs)).await;
    }
}

fn collect_candidates(db: &Db, now: f64) -> Vec<String> {
    let conn = crate::db_lock(db);
    let mut out: Vec<String> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT last_flight FROM aircraft
         WHERE last_seen > ?1 AND last_flight IS NOT NULL",
    ) {
        if let Ok(rows) = stmt.query_map(params![now - 3600.0], |r| r.get::<_, String>(0)) {
            out.extend(rows.filter_map(Result::ok));
        }
    }
    if let Ok(mut stmt) = conn.prepare(
        "SELECT callsign FROM aircraft_callsigns WHERE last_seen > ?1
         GROUP BY callsign ORDER BY MAX(last_seen) DESC LIMIT 200",
    ) {
        if let Ok(rows) = stmt.query_map(params![now - 7.0 * 86_400.0], |r| r.get::<_, String>(0)) {
            out.extend(rows.filter_map(Result::ok));
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.into_iter().filter_map(|c| normalize(&c)).filter(|c| seen.insert(c.clone())).collect()
}

fn prune(pos_ttl: f64, neg_ttl: f64) {
    let now = now();
    let mut cache = CACHE.write().unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.retain(|_, e| {
        let ttl = if e.route.is_some() { pos_ttl } else { neg_ttl };
        now - e.fetched_at < ttl
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callsigns_are_normalized_or_rejected() {
        assert_eq!(normalize(" aca123 ").as_deref(), Some("ACA123"));
        assert_eq!(normalize("POE*12"), None);
        assert_eq!(normalize("A"), None);
        assert_eq!(normalize("TOOLONGCALLSIGN"), None);
        assert_eq!(normalize("ACA-123"), None);
    }

    #[test]
    fn parses_a_route_and_rejects_an_incomplete_one() {
        let body = r#"{"response":{"flightroute":{"callsign":"ACA123",
            "airline":{"name":"Air Canada"},
            "origin":{"iata_code":"YYZ","icao_code":"CYYZ","name":"Toronto Pearson"},
            "destination":{"iata_code":"YVR","icao_code":"CYVR","name":"Vancouver"}}}}"#;
        let route = parse_route(body, 1.0).expect("route");
        assert_eq!(route.origin_iata, "YYZ");
        assert_eq!(route.dest_iata, "YVR");
        assert_eq!(route.airline.as_deref(), Some("Air Canada"));

        let missing = r#"{"response":{"flightroute":{"callsign":"X","origin":{"iata_code":null}}}}"#;
        assert!(parse_route(missing, 1.0).is_none());
    }
}
