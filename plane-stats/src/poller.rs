use std::time::Duration;

use rusqlite::{params, OptionalExtension};
use serde::Deserialize;

use crate::Db;

const POLL_INTERVAL: Duration = Duration::from_secs(10);
const VISIT_GAP_SECS: f64 = 600.0;
const PRUNE_AFTER_DAYS: f64 = 90.0;

/// `alt_baro` is either a number or the string "ground". The string itself
/// carries no information, so it is parsed and discarded.
#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum Alt {
    Ft(f64),
    Ground(serde::de::IgnoredAny),
}

impl Alt {
    /// `(altitude_ft, on_ground)`. `None` altitude is neither ground nor known.
    pub(crate) fn split(&self) -> (Option<i64>, bool) {
        match self {
            Alt::Ft(ft) => (Some(*ft as i64), false),
            Alt::Ground(_) => (None, true),
        }
    }
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
    seen: Option<f64>,
    lat: Option<f64>,
    lon: Option<f64>,
    track: Option<f64>,
    baro_rate: Option<f64>,
    nav_altitude_mcp: Option<f64>,
    nav_heading: Option<f64>,
    emergency: Option<String>,
    #[serde(rename = "dbFlags")]
    db_flags: Option<i64>,
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
                        .filter(|a| a.seen.unwrap_or(f64::MAX) <= crate::stats::LIVE_WINDOW_SECS)
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
    let mut conn = crate::db_lock(db);
    let tx = conn.transaction()?;
    // Local calendar day for the daily ledger; derived from the poll timestamp
    // so a poll that straddles midnight is filed under the day it was taken.
    let day = crate::stats::local_day(now);

    {
        let mut stmt = tx.prepare(
            "INSERT INTO aircraft
               (hex, first_seen, last_seen, sightings,
                last_flight, last_squawk, last_category,
                max_alt_ft, max_gs_kts, max_range_nm, max_rssi_dbfs,
                last_lat, last_lon, last_track,
                max_baro_rate, min_baro_rate,
                last_nav_alt_ft, last_nav_heading, last_emergency,
                last_alt_ft, last_gs_kts, last_range_nm, last_baro_rate, last_msg_age, db_flags)
             VALUES (?1, ?2, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?14, ?15, ?16,
                     ?17, ?7, ?8, ?13, ?18, ?19)
             ON CONFLICT(hex) DO UPDATE SET
               last_seen = excluded.last_seen,
               sightings = aircraft.sightings + 1,
               last_flight = coalesce(excluded.last_flight, aircraft.last_flight),
               last_squawk = coalesce(excluded.last_squawk, aircraft.last_squawk),
               last_category = coalesce(excluded.last_category, aircraft.last_category),
               last_lat = coalesce(excluded.last_lat, aircraft.last_lat),
               last_lon = coalesce(excluded.last_lon, aircraft.last_lon),
               last_track = coalesce(excluded.last_track, aircraft.last_track),
               last_nav_alt_ft = coalesce(excluded.last_nav_alt_ft, aircraft.last_nav_alt_ft),
               last_nav_heading = coalesce(excluded.last_nav_heading, aircraft.last_nav_heading),
               last_emergency = coalesce(excluded.last_emergency, aircraft.last_emergency),
               last_alt_ft = excluded.last_alt_ft,
               last_gs_kts = excluded.last_gs_kts,
               last_range_nm = excluded.last_range_nm,
               last_baro_rate = excluded.last_baro_rate,
               last_msg_age = excluded.last_msg_age,
               db_flags = coalesce(excluded.db_flags, aircraft.db_flags),
               max_alt_ft = nullif(MAX(ifnull(aircraft.max_alt_ft, -2147483648),
                                       ifnull(excluded.max_alt_ft, -2147483648)), -2147483648),
               max_gs_kts = nullif(MAX(ifnull(aircraft.max_gs_kts, -1e30),
                                        ifnull(excluded.max_gs_kts, -1e30)), -1e30),
               max_range_nm = nullif(MAX(ifnull(aircraft.max_range_nm, -1e30),
                                         ifnull(excluded.max_range_nm, -1e30)), -1e30),
               max_rssi_dbfs = nullif(MAX(ifnull(aircraft.max_rssi_dbfs, -1e30),
                                           ifnull(excluded.max_rssi_dbfs, -1e30)), -1e30),
               max_baro_rate = nullif(MAX(ifnull(aircraft.max_baro_rate, -2147483648),
                                          ifnull(excluded.max_baro_rate, -2147483648)), -2147483648),
               min_baro_rate = nullif(MIN(ifnull(aircraft.min_baro_rate, 2147483647),
                                          ifnull(excluded.min_baro_rate, 2147483647)), 2147483647)",
        )?;

        let mut callsigns = tx.prepare(
            "INSERT INTO aircraft_callsigns (hex, callsign, first_seen, last_seen, sightings)
             VALUES (?1, ?2, ?3, ?3, 1)
             ON CONFLICT(hex, callsign) DO UPDATE SET
               last_seen = excluded.last_seen,
               sightings = aircraft_callsigns.sightings + 1",
        )?;
        let mut last_visit = tx.prepare(
            "SELECT start_ts, end_ts FROM visits WHERE hex = ?1 ORDER BY start_ts DESC LIMIT 1",
        )?;
        let mut visits = tx.prepare(
            "INSERT INTO visits (hex, start_ts, end_ts, polls) VALUES (?1, ?2, ?2, 1)",
        )?;
        let mut visit_extend = tx.prepare(
            "UPDATE visits SET end_ts = ?1, polls = polls + 1 WHERE hex = ?2 AND start_ts = ?3",
        )?;
        // One row per emergency episode: a 10-minute emergency squawk would
        // otherwise log ~60 rows at the 10 s poll rate.
        let mut emergency = tx.prepare(
            "INSERT INTO emergency_log (ts, hex, squawk, emergency)
             SELECT ?1, ?2, ?3, ?4
             WHERE NOT EXISTS (
               SELECT 1 FROM emergency_log WHERE hex = ?2 AND emergency = ?4 AND ts > ?1 - 600
             )",
        )?;
        // Daily ledger: one row per aircraft per local day, MAX metrics so the
        // `/today` digest can compare today against every earlier day.
        let mut day_aircraft = tx.prepare(
            "INSERT INTO day_aircraft
               (day, hex, first_ts, last_ts, polls, max_alt_ft, max_gs_kts, max_range_nm)
             VALUES (?1, ?2, ?3, ?3, 1, ?4, ?5, ?6)
             ON CONFLICT(day, hex) DO UPDATE SET
               last_ts = excluded.last_ts,
               polls = day_aircraft.polls + 1,
               max_alt_ft = nullif(MAX(ifnull(day_aircraft.max_alt_ft, -2147483648),
                                       ifnull(excluded.max_alt_ft, -2147483648)), -2147483648),
               max_gs_kts = nullif(MAX(ifnull(day_aircraft.max_gs_kts, -1e30),
                                        ifnull(excluded.max_gs_kts, -1e30)), -1e30),
               max_range_nm = nullif(MAX(ifnull(day_aircraft.max_range_nm, -1e30),
                                         ifnull(excluded.max_range_nm, -1e30)), -1e30)",
        )?;

        for a in live {
            let hex = a.hex.to_lowercase();
            let alt_ft = match &a.alt_baro {
                Some(Alt::Ft(ft)) => Some(*ft as i64),
                Some(Alt::Ground(_)) => None,
                None => None,
            };
            // Current altitude for the live panel: ground counts as 0 ft, while
            // `max_alt_ft` above keeps ignoring ground contacts.
            let last_alt_ft = match &a.alt_baro {
                Some(Alt::Ft(ft)) => Some(*ft as i64),
                Some(Alt::Ground(_)) => Some(0),
                None => None,
            };
            let flight = a.flight.as_deref().map(str::trim).filter(|s| !s.is_empty());
            stmt.execute(params![
                hex.as_str(),
                now,
                flight,
                a.squawk,
                a.category,
                alt_ft,
                a.gs,
                a.r_dst,
                a.rssi,
                a.lat,
                a.lon,
                a.track,
                a.baro_rate.map(|v| v as i64),
                a.nav_altitude_mcp.map(|v| v as i64),
                a.nav_heading,
                a.emergency,
                last_alt_ft,
                a.seen,
                a.db_flags,
            ])?;

            day_aircraft.execute(params![day.as_str(), hex.as_str(), now, alt_ft, a.gs, a.r_dst])?;

            let emergency_str = a.emergency.as_deref().filter(|e| *e != "none");
            let emergency_squawk =
                a.squawk.as_deref().filter(|s| matches!(*s, "7500" | "7600" | "7700"));
            if emergency_str.is_some() || emergency_squawk.is_some() {
                emergency.execute(params![
                    now,
                    hex.as_str(),
                    a.squawk,
                    emergency_str.unwrap_or("squawk"),
                ])?;
            }

            if let Some(cs) = flight {
                callsigns.execute(params![hex.as_str(), cs, now])?;
            }

            let last: Option<(f64, f64)> = last_visit
                .query_row(params![hex.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            match last {
                None => {
                    visits.execute(params![hex.as_str(), now])?;
                }
                Some((_, end_ts)) if now - end_ts > VISIT_GAP_SECS => {
                    visits.execute(params![hex.as_str(), now])?;
                }
                Some((start_ts, _)) => {
                    visit_extend.execute(params![now, hex.as_str(), start_ts])?;
                }
            }
        }
    }

    tx.execute(
        "INSERT OR REPLACE INTO poll_log (ts, aircraft_count, messages) VALUES (?1, ?2, ?3)",
        params![now, live.len() as i64, messages],
    )?;
    // Hourly rollup the cards read; keeps them off a full `poll_log` scan.
    tx.execute(
        "INSERT INTO poll_hour (ts, max_count)
         VALUES (CAST(?1 / 3600 AS INTEGER) * 3600, ?2)
         ON CONFLICT(ts) DO UPDATE SET max_count = MAX(max_count, excluded.max_count)",
        params![now, live.len() as i64],
    )?;
    tx.commit()
}

fn prune(db: &Db, now: f64) -> rusqlite::Result<()> {
    let conn = crate::db_lock(db);
    let cutoff = now - PRUNE_AFTER_DAYS * 86_400.0;
    let n = conn.execute("DELETE FROM poll_log WHERE ts < ?1", params![cutoff])?;
    conn.execute("DELETE FROM poll_hour WHERE ts < ?1", params![cutoff])?;
    tracing::info!("pruned {n} poll_log rows older than {PRUNE_AFTER_DAYS} days");
    // Return the freed pages' WAL back to the filesystem.
    if let Err(e) = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)") {
        tracing::warn!("wal checkpoint failed: {e}");
    }
    Ok(())
}
