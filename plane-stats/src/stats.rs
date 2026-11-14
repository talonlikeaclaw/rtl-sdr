use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::Db;

#[derive(Debug, serde::Serialize)]
pub struct AircraftRow {
    pub hex: String,
    pub first_seen: f64,
    pub last_seen: f64,
    pub sightings: i64,
    pub last_flight: Option<String>,
    pub last_squawk: Option<String>,
    pub last_category: Option<String>,
    pub max_alt_ft: Option<i64>,
    pub max_gs_kts: Option<f64>,
    pub max_range_nm: Option<f64>,
    pub min_rssi_dbfs: Option<f64>,
}

fn row_to_aircraft(r: &Row) -> rusqlite::Result<AircraftRow> {
    Ok(AircraftRow {
        hex: r.get(0)?,
        first_seen: r.get(1)?,
        last_seen: r.get(2)?,
        sightings: r.get(3)?,
        last_flight: r.get(4)?,
        last_squawk: r.get(5)?,
        last_category: r.get(6)?,
        max_alt_ft: r.get(7)?,
        max_gs_kts: r.get(8)?,
        max_range_nm: r.get(9)?,
        min_rssi_dbfs: r.get(10)?,
    })
}

impl AircraftRow {
    fn fmt_ts(epoch: f64) -> String {
        chrono::DateTime::from_timestamp(epoch as i64, 0).expect("valid ts").with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    }
    pub fn first_seen_str(&self) -> String {
        Self::fmt_ts(self.first_seen)
    }
    pub fn last_seen_str(&self) -> String {
        Self::fmt_ts(self.last_seen)
    }
}

impl Record {
    pub fn label(&self) -> String {
        match self.hex.parse::<i64>() {
            Ok(h) => chrono::DateTime::from_timestamp(h, 0).expect("valid ts").with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:00")
                .to_string(),
            Err(_) => self.hex.clone(),
        }
    }
    pub fn value_str(&self) -> String {
        if (self.value - self.value.round()).abs() < f64::EPSILON {
            format!("{}", self.value as i64)
        } else {
            format!("{:.1}", self.value)
        }
    }
}

const COLS: &str = "hex, first_seen, last_seen, sightings, last_flight, last_squawk, \
                    last_category, max_alt_ft, max_gs_kts, max_range_nm, min_rssi_dbfs";

/// Whitelist sort keys -> (SQL column, default direction).
pub fn sort_column(sort: &str) -> (&'static str, &'static str) {
    match sort {
        "hex" => ("hex", "asc"),
        "callsign" => ("last_flight", "asc"),
        "first_seen" => ("first_seen", "desc"),
        "last_seen" => ("last_seen", "desc"),
        "sightings" => ("sightings", "desc"),
        "max_alt_ft" => ("max_alt_ft", "desc"),
        "max_gs_kts" => ("max_gs_kts", "desc"),
        "max_range_nm" => ("max_range_nm", "desc"),
        _ => ("last_seen", "desc"),
    }
}

pub fn is_valid_dir(dir: &str) -> bool {
    dir == "asc" || dir == "desc"
}

pub fn aircraft_sorted(db: &Db, sort: &str, dir: &str) -> rusqlite::Result<Vec<AircraftRow>> {
    let (col, default_dir) = sort_column(sort);
    let dir = if is_valid_dir(dir) { dir } else { default_dir };
    let conn = db.lock().expect("db mutex");
    let sql = format!("SELECT {COLS} FROM aircraft ORDER BY {col} {dir}, hex ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_aircraft)?;
    rows.collect()
}

pub fn recent_arrivals(db: &Db) -> rusqlite::Result<Vec<AircraftRow>> {
    let conn = db.lock().expect("db mutex");
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM aircraft ORDER BY first_seen DESC LIMIT 10"
    ))?;
    let rows = stmt.query_map([], row_to_aircraft)?;
    rows.collect()
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Record {
    pub hex: String,
    pub value: f64,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Stats {
    pub total_aircraft: i64,
    pub currently_airborne: i64,
    pub first_seen_today: i64,
    pub max_range: Option<Record>,
    pub fastest: Option<Record>,
    pub highest: Option<Record>,
    pub busiest_hour: Option<Record>, // value = aircraft count that hour, hex = hour label
}

fn record(conn: &Connection, what: &str) -> rusqlite::Result<Option<Record>> {
    let (col, table) = match what {
        "range" => ("max_range_nm", "aircraft"),
        "speed" => ("max_gs_kts", "aircraft"),
        "alt" => ("max_alt_ft", "aircraft"),
        _ => unreachable!(),
    };
    let sql = format!("SELECT hex, {col} FROM {table} WHERE {col} IS NOT NULL ORDER BY {col} DESC LIMIT 1");
    conn.query_row(&sql, [], |r| {
        Ok(Record { hex: r.get(0)?, value: r.get(1)? })
    })
    .optional()
}

pub fn stats(db: &Db, now: f64) -> rusqlite::Result<Stats> {
    let conn = db.lock().expect("db mutex");
    let total_aircraft: i64 = conn.query_row("SELECT COUNT(*) FROM aircraft", [], |r| r.get(0))?;
    let currently_airborne: i64 =
        conn.query_row("SELECT COUNT(*) FROM aircraft WHERE last_seen > ?1", params![now - 60.0], |r| r.get(0))?;
    let today_start = (chrono::Local::now().date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight"))
    .and_utc()
    .timestamp() as f64;
    let first_seen_today: i64 =
        conn.query_row("SELECT COUNT(*) FROM aircraft WHERE first_seen >= ?1", params![today_start], |r| {
            r.get(0)
        })?;

    let busiest_hour: Option<Record> = conn
        .query_row(
            "SELECT CAST(ts/3600 AS INTEGER) * 3600, MAX(aircraft_count)
             FROM poll_log GROUP BY 1 ORDER BY 2 DESC LIMIT 1",
            [],
            |r| Ok(Record { hex: r.get::<_, i64>(0)?.to_string(), value: r.get(1)? }),
        )
        .optional()?;

    Ok(Stats {
        total_aircraft,
        currently_airborne,
        first_seen_today,
        max_range: record(&conn, "range")?,
        fastest: record(&conn, "speed")?,
        highest: record(&conn, "alt")?,
        busiest_hour,
    })
}
