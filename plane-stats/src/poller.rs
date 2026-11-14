use std::time::Duration;

use rusqlite::params;
use serde::Deserialize;

use crate::Db;

const POLL_INTERVAL: Duration = Duration::from_secs(10);
const LIVE_SEEN_MAX: f64 = 60.0;
const PRUNE_AFTER_DAYS: f64 = 90.0;

/// `alt_baro` is either a number or the string "ground".
#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum Alt {
    Ft(f64),
    Ground(String),
}

#[derive(Deserialize, Debug)]
struct Aircraft {
    hex: String,
    flight: Option<String>,
    alt_baro: Option<Alt>,
    gs: Option<f64>,
    r_dst: Option<f64>,
    rssi: Option<f64>,
    squawk: Option<String>,
    category: Option<String>,
    messages: Option<i64>,
    seen: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct Payload {
    now: f64,
    aircraft: Vec<Aircraft>,
    messages: i64,
}

pub async fn run(db: Db, source: String) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .expect("reqwest client");
    let mut last_prune: f64 = 0.0;

    loop {
        match client.get(&source).send().await {
            Ok(resp) => match resp.json::<Payload>().await {
                Ok(payload) => {
                    // tar1090's `now` is epoch seconds; guard against ms just in case.
                    let now = if payload.now > 1e12 { payload.now / 1000.0 } else { payload.now };
                    let live: Vec<&Aircraft> = payload
                        .aircraft
                        .iter()
                        .filter(|a| a.seen.unwrap_or(f64::MAX) <= LIVE_SEEN_MAX)
                        .collect();
                    match record_poll(&db, now, &live, payload.messages) {
                        Ok(()) => tracing::info!(
                            "poll ok: {} aircraft, {} messages",
                            live.len(),
                            payload.messages
                        ),
                        Err(e) => tracing::warn!("db write failed: {e}"),
                    }
                    if now - last_prune > 86_400.0 {
                        if let Err(e) = prune(&db, now) {
                            tracing::warn!("prune failed: {e}");
                        } else {
                            last_prune = now;
                        }
                    }
                }
                Err(e) => tracing::warn!("bad payload from {source}: {e}"),
            },
            Err(e) => tracing::warn!("fetch {source} failed: {e}"),
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn record_poll(db: &Db, now: f64, live: &[&Aircraft], messages: i64) -> rusqlite::Result<()> {
    let mut conn = db.lock().expect("db mutex");
    let tx = conn.transaction()?;

    {
        let mut stmt = tx.prepare(
            "INSERT INTO aircraft
               (hex, first_seen, last_seen, sightings,
                last_flight, last_squawk, last_category,
                max_alt_ft, max_gs_kts, max_range_nm, min_rssi_dbfs)
             VALUES (?1, ?2, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(hex) DO UPDATE SET
               last_seen = excluded.last_seen,
               sightings = aircraft.sightings + 1,
               last_flight = coalesce(excluded.last_flight, aircraft.last_flight),
               last_squawk = coalesce(excluded.last_squawk, aircraft.last_squawk),
               last_category = coalesce(excluded.last_category, aircraft.last_category),
               max_alt_ft = nullif(MAX(ifnull(aircraft.max_alt_ft, -2147483648),
                                       ifnull(excluded.max_alt_ft, -2147483648)), -2147483648),
               max_gs_kts = nullif(MAX(ifnull(aircraft.max_gs_kts, -1e30),
                                        ifnull(excluded.max_gs_kts, -1e30)), -1e30),
               max_range_nm = nullif(MAX(ifnull(aircraft.max_range_nm, -1e30),
                                         ifnull(excluded.max_range_nm, -1e30)), -1e30),
               min_rssi_dbfs = nullif(MIN(ifnull(aircraft.min_rssi_dbfs, 1e30),
                                          ifnull(excluded.min_rssi_dbfs, 1e30)), 1e30)",
        )?;

        for a in live {
            let alt_ft = match &a.alt_baro {
                Some(Alt::Ft(ft)) => Some(*ft as i64),
                Some(Alt::Ground(_)) => None,
                None => None,
            };
            let flight = a.flight.as_deref().map(str::trim).filter(|s| !s.is_empty());
            stmt.execute(params![
                a.hex.to_lowercase(),
                now,
                flight,
                a.squawk,
                a.category,
                alt_ft,
                a.gs,
                a.r_dst,
                a.rssi,
            ])?;
        }
    }

    tx.execute(
        "INSERT OR REPLACE INTO poll_log (ts, aircraft_count, messages) VALUES (?1, ?2, ?3)",
        params![now, live.len() as i64, messages],
    )?;
    tx.commit()
}

fn prune(db: &Db, now: f64) -> rusqlite::Result<()> {
    let conn = db.lock().expect("db mutex");
    let cutoff = now - PRUNE_AFTER_DAYS * 86_400.0;
    let n = conn.execute("DELETE FROM poll_log WHERE ts < ?1", params![cutoff])?;
    tracing::info!("pruned {n} poll_log rows older than {PRUNE_AFTER_DAYS} days");
    Ok(())
}
