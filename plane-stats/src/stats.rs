use chrono::TimeZone;
use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::Db;

/// An aircraft is "live" (still on the tar1090 map) when the poller recorded
/// it within this window. The poller only ingests tar1090 entries whose
/// `seen` age is at most this value, so the two stay in sync.
pub const LIVE_WINDOW_SECS: f64 = 60.0;

/// Rows per page of the all-time fleet table.
pub const PAGE_SIZE: u32 = 100;

/// A "regular" was seen on at least this many separate visits. A visit ends
/// after 10 minutes without a poll (`poller::VISIT_GAP_SECS`), so this counts
/// distinct passes rather than one long one.
const REGULAR_VISITS: i64 = 5;

/// "Dormant" aircraft have not been seen for this long.
const DORMANT_SECS: f64 = 7.0 * 86_400.0;

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
    pub max_rssi_dbfs: Option<f64>,
    pub last_lat: Option<f64>,
    pub last_lon: Option<f64>,
    pub last_track: Option<f64>,
    pub max_baro_rate: Option<i64>,
    pub min_baro_rate: Option<i64>,
    pub last_nav_alt_ft: Option<i64>,
    pub last_nav_heading: Option<f64>,
    pub last_emergency: Option<String>,
    pub registration: Option<String>,
    pub typecode: Option<String>,
    pub last_alt_ft: Option<i64>,
    pub last_gs_kts: Option<f64>,
    pub last_range_nm: Option<f64>,
    pub last_baro_rate: Option<f64>,
    pub last_msg_age: Option<f64>,
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
        max_rssi_dbfs: r.get(10)?,
        last_lat: r.get(11)?,
        last_lon: r.get(12)?,
        last_track: r.get(13)?,
        max_baro_rate: r.get(14)?,
        min_baro_rate: r.get(15)?,
        last_nav_alt_ft: r.get(16)?,
        last_nav_heading: r.get(17)?,
        last_emergency: r.get(18)?,
        registration: r.get(19)?,
        typecode: r.get(20)?,
        last_alt_ft: r.get(21)?,
        last_gs_kts: r.get(22)?,
        last_range_nm: r.get(23)?,
        last_baro_rate: r.get(24)?,
        last_msg_age: r.get(25)?,
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
    pub fn lat_lon_str(&self) -> String {
        match (self.last_lat, self.last_lon) {
            (Some(lat), Some(lon)) => format!("{lat:.4}, {lon:.4}"),
            _ => "-".into(),
        }
    }
    pub fn track_str(&self) -> String {
        self.last_track.map(|t| format!("{t:.0}\u{00b0}")).unwrap_or_else(|| "-".into())
    }
    pub fn nav_alt_str(&self) -> String {
        self.last_nav_alt_ft.map(|v| v.to_string()).unwrap_or_else(|| "-".into())
    }
    pub fn nav_hdg_str(&self) -> String {
        self.last_nav_heading.map(|t| format!("{t:.0}\u{00b0}")).unwrap_or_else(|| "-".into())
    }
    pub fn emergency_str(&self) -> String {
        self.last_emergency.as_deref().unwrap_or("-").to_string()
    }
    pub fn emergency_active(&self) -> bool {
        matches!(self.last_emergency.as_deref(), Some(e) if e != "none")
    }
    /// Whether tar1090 still has this aircraft on its live map.
    pub fn is_live(&self, now: &f64) -> bool {
        *now - self.last_seen <= LIVE_WINDOW_SECS
    }
    /// tar1090 deep link that selects this aircraft on the live map.
    ///
    /// No `hideSidebar`: tar1090 persists that flag in its own localStorage, so a
    /// link from here would permanently change the user's tar1090 layout. tar1090
    /// also strips consumed params from the URL.
    pub fn map_url(&self, base: &str) -> String {
        format!("{base}/?icao={}", self.hex)
    }
    /// tar1090 deep link that shows this aircraft's track history.
    pub fn trace_url(&self, base: &str) -> String {
        format!("{base}/?showTrace={}", self.hex)
    }
    pub fn alt_str(&self) -> String {
        self.last_alt_ft.map(|v| v.to_string()).unwrap_or_else(|| "-".into())
    }
    pub fn gs_str(&self) -> String {
        self.last_gs_kts.map(|v| format!("{v:.0}")).unwrap_or_else(|| "-".into())
    }
    pub fn range_str(&self) -> String {
        self.last_range_nm.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into())
    }
    pub fn climb_str(&self) -> String {
        self.last_baro_rate.map(|v| format!("{v:+.0}")).unwrap_or_else(|| "-".into())
    }
    pub fn age_str(&self) -> String {
        self.last_msg_age.map(|v| format!("{v:.0}")).unwrap_or_else(|| "-".into())
    }
    /// Compass bearing from the receiver to this aircraft, if both positions are known.
    pub fn bearing_str(&self, rx: &Option<(f64, f64)>) -> String {
        match (rx, self.last_lat, self.last_lon) {
            (Some((rx_lat, rx_lon)), Some(lat), Some(lon)) => {
                format!("{:.0}\u{00b0}", bearing_deg(*rx_lat, *rx_lon, lat, lon))
            }
            _ => "-".into(),
        }
    }
}

/// Receiver position, set once at startup from `RX_LAT`/`RX_LON`.
static RX_POS: std::sync::OnceLock<(f64, f64)> = std::sync::OnceLock::new();

pub fn set_rx_pos(lat: f64, lon: f64) {
    let _ = RX_POS.set((lat, lon));
}

pub fn rx_pos() -> Option<(f64, f64)> {
    RX_POS.get().copied()
}

/// Initial great-circle bearing from (rx_lat, rx_lon) to (lat, lon), in degrees 0-360.
pub fn bearing_deg(rx_lat: f64, rx_lon: f64, lat: f64, lon: f64) -> f64 {
    let (phi1, phi2) = (rx_lat.to_radians(), lat.to_radians());
    let d_lambda = (lon - rx_lon).to_radians();
    let y = d_lambda.sin() * phi2.cos();
    let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * d_lambda.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

impl Record {
    pub fn label(&self) -> String {
        match self.hex.parse::<i64>() {
            Ok(h) => chrono::DateTime::from_timestamp(h, 0).expect("valid ts").with_timezone(&chrono::Local)
                .format("%b %d · %H:00")
                .to_string(),
            Err(_) => self.hex.clone(),
        }
    }
    pub fn label_day(&self) -> String {
        match self.hex.parse::<i64>() {
            Ok(d) => chrono::DateTime::from_timestamp(d, 0).expect("valid ts").with_timezone(&chrono::Local)
                .format("%b %d")
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
    /// Human identity for a record holder: registration or callsign, plus type.
    pub fn ident(&self) -> String {
        match (&self.name, &self.typecode) {
            (Some(n), Some(t)) => format!("{n} · {t}"),
            (Some(n), None) => n.clone(),
            (None, Some(t)) => t.clone(),
            (None, None) => self.hex.clone(),
        }
    }
}

const COLS: &str = "a.hex, a.first_seen, a.last_seen, a.sightings, a.last_flight, a.last_squawk, \
                    a.last_category, a.max_alt_ft, a.max_gs_kts, a.max_range_nm, a.max_rssi_dbfs, \
                    a.last_lat, a.last_lon, a.last_track, a.max_baro_rate, a.min_baro_rate, \
                    a.last_nav_alt_ft, a.last_nav_heading, a.last_emergency, \
                    e.registration, e.typecode, \
                    a.last_alt_ft, a.last_gs_kts, a.last_range_nm, a.last_baro_rate, a.last_msg_age";
const FROM: &str = "FROM aircraft a LEFT JOIN enrichment e ON e.icao24 = a.hex";

/// Sort keys accepted from the query string.
const SORT_KEYS: &[&str] = &[
    "hex",
    "callsign",
    "first_seen",
    "last_seen",
    "sightings",
    "max_alt_ft",
    "max_gs_kts",
    "max_range_nm",
    "registration",
    "typecode",
];

/// Whether `sort` names a column the fleet table can order by.
pub fn is_valid_sort(sort: &str) -> bool {
    SORT_KEYS.contains(&sort)
}

/// Whitelist sort keys -> (SQL column, default direction).
pub fn sort_column(sort: &str) -> (&'static str, &'static str) {
    match sort {
        "hex" => ("a.hex", "asc"),
        "callsign" => ("a.last_flight", "asc"),
        "first_seen" => ("a.first_seen", "desc"),
        "last_seen" => ("a.last_seen", "desc"),
        "sightings" => ("a.sightings", "desc"),
        "max_alt_ft" => ("a.max_alt_ft", "desc"),
        "max_gs_kts" => ("a.max_gs_kts", "desc"),
        "max_range_nm" => ("a.max_range_nm", "desc"),
        "registration" => ("e.registration", "asc"),
        "typecode" => ("e.typecode", "asc"),
        _ => ("a.last_seen", "desc"),
    }
}

pub fn is_valid_dir(dir: &str) -> bool {
    dir == "asc" || dir == "desc"
}

/// Filters for the all-time fleet table. Empty/false fields are inactive.
#[derive(Debug, Clone, Default)]
pub struct FleetFilter {
    pub q: String,
    /// "" = any, "fixed" = A1-A6, "heli" = A7.
    pub cat: String,
    pub mil: bool,
    pub today: bool,
    pub new7: bool,
    /// tar1090 `dbFlags` bit 1.
    pub interesting: bool,
    /// tar1090 `dbFlags` bits 2 (PIA) or 3 (LADD).
    pub privacy: bool,
    /// Has at least one row in `emergency_log`.
    pub emergency: bool,
    /// Seen on `REGULAR_VISITS` or more separate visits.
    pub regular: bool,
    /// Not seen for `DORMANT_SECS`.
    pub dormant: bool,
    pub alt_min: Option<i64>,
    pub alt_max: Option<i64>,
}

/// Local midnight as an epoch timestamp, matching the cards' "today" bucket.
fn local_today_start() -> f64 {
    chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_local_timezone(chrono::Local)
        .single()
        .expect("unambiguous local midnight")
        .timestamp() as f64
}

fn fleet_needle(search: &str) -> String {
    format!("%{}%", search.trim().to_lowercase())
}

/// Percent-encode every byte outside the unreserved set so a needle survives
/// `&`, `=`, `#`, spaces, and non-ASCII inside a link.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A WHERE clause plus its named parameters, ready to bind.
struct FleetSql {
    where_clause: String,
    params: Vec<(&'static str, Value)>,
}

impl FleetSql {
    fn bind(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        self.params.iter().map(|(n, v)| (*n, v as &dyn rusqlite::ToSql)).collect()
    }
}

impl FleetFilter {
    fn to_sql(&self, now: f64) -> FleetSql {
        let mut clauses: Vec<&'static str> = Vec::new();
        let mut params: Vec<(&'static str, Value)> = Vec::new();

        if !self.q.trim().is_empty() {
            clauses.push(
                "(a.hex LIKE :q OR a.last_flight LIKE :q
                  OR e.registration LIKE :q OR e.model LIKE :q OR e.typecode LIKE :q
                  OR e.owner LIKE :q OR e.operator LIKE :q)",
            );
            params.push((":q", Value::Text(fleet_needle(&self.q))));
        }
        match self.cat.as_str() {
            "fixed" => clauses.push("a.last_category IN ('A1','A2','A3','A4','A5','A6')"),
            "heli" => clauses.push("a.last_category = 'A7'"),
            _ => {}
        }
        if self.mil {
            clauses.push("(a.db_flags & 1) = 1");
        }
        if self.today {
            clauses.push("a.last_seen >= :today_start");
            params.push((":today_start", Value::Real(local_today_start())));
        }
        if self.new7 {
            clauses.push("a.first_seen >= :new7_start");
            params.push((":new7_start", Value::Real(now - 7.0 * 86_400.0)));
        }
        if self.interesting {
            clauses.push("(a.db_flags & 2) = 2");
        }
        if self.privacy {
            clauses.push("(a.db_flags & 12) != 0");
        }
        if self.emergency {
            clauses.push("EXISTS (SELECT 1 FROM emergency_log el WHERE el.hex = a.hex)");
        }
        if self.regular {
            clauses.push("(SELECT COUNT(*) FROM visits v WHERE v.hex = a.hex) >= :regular_visits");
            params.push((":regular_visits", Value::Integer(REGULAR_VISITS)));
        }
        if self.dormant {
            clauses.push("a.last_seen < :dormant_start");
            params.push((":dormant_start", Value::Real(now - DORMANT_SECS)));
        }
        if let Some(v) = self.alt_min {
            clauses.push("a.max_alt_ft >= :alt_min");
            params.push((":alt_min", Value::Integer(v)));
        }
        if let Some(v) = self.alt_max {
            clauses.push("a.max_alt_ft <= :alt_max");
            params.push((":alt_max", Value::Integer(v)));
        }

        let where_clause =
            if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };
        FleetSql { where_clause, params }
    }

    /// Query-string suffix (leading `&`) carrying the active filters, for links
    /// that already set `sort`/`dir` (and `page`).
    pub fn query_suffix(&self) -> String {
        let mut s = String::new();
        let q = self.q.trim();
        if !q.is_empty() {
            s.push_str("&q=");
            s.push_str(&urlencode(q));
        }
        if self.cat == "fixed" || self.cat == "heli" {
            s.push_str("&cat=");
            s.push_str(&self.cat);
        }
        if self.mil {
            s.push_str("&mil=1");
        }
        if self.today {
            s.push_str("&today=1");
        }
        if self.new7 {
            s.push_str("&new7=1");
        }
        if self.interesting {
            s.push_str("&interesting=1");
        }
        if self.privacy {
            s.push_str("&privacy=1");
        }
        if self.emergency {
            s.push_str("&emergency=1");
        }
        if self.regular {
            s.push_str("&regular=1");
        }
        if self.dormant {
            s.push_str("&dormant=1");
        }
        if let Some(v) = self.alt_min {
            s.push_str(&format!("&alt_min={v}"));
        }
        if let Some(v) = self.alt_max {
            s.push_str(&format!("&alt_max={v}"));
        }
        s
    }
}

pub fn aircraft_sorted(
    db: &Db,
    sort: &str,
    dir: &str,
    filter: &FleetFilter,
    limit: u32,
    offset: u32,
    now: f64,
) -> rusqlite::Result<Vec<AircraftRow>> {
    let (col, default_dir) = sort_column(sort);
    let dir = if is_valid_dir(dir) { dir } else { default_dir };
    let conn = db.lock().expect("db mutex");
    let mut f = filter.to_sql(now);
    f.params.push((":limit", Value::Integer(i64::from(limit))));
    f.params.push((":offset", Value::Integer(i64::from(offset))));
    let sql = format!(
        "SELECT {COLS} {FROM} {} ORDER BY {col} {dir}, a.hex ASC LIMIT :limit OFFSET :offset",
        f.where_clause
    );
    let mut stmt = conn.prepare(&sql)?;
    let params = f.bind();
    let rows: Vec<AircraftRow> =
        stmt.query_map(&params[..], row_to_aircraft)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Number of aircraft matching the same filter as `aircraft_sorted`.
pub fn aircraft_count(db: &Db, filter: &FleetFilter, now: f64) -> rusqlite::Result<i64> {
    let conn = db.lock().expect("db mutex");
    let f = filter.to_sql(now);
    let sql = format!("SELECT COUNT(*) {FROM} {}", f.where_clause);
    let mut stmt = conn.prepare(&sql)?;
    let params = f.bind();
    stmt.query_row(&params[..], |r| r.get(0))
}

pub fn recent_arrivals(db: &Db) -> rusqlite::Result<Vec<AircraftRow>> {
    let conn = db.lock().expect("db mutex");
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {FROM} ORDER BY a.first_seen DESC LIMIT 10"
    ))?;
    let rows = stmt.query_map([], row_to_aircraft)?;
    rows.collect()
}

/// Aircraft still on the tar1090 map, nearest first (unpositioned last).
pub fn live_aircraft(db: &Db, now: f64) -> rusqlite::Result<Vec<AircraftRow>> {
    let conn = db.lock().expect("db mutex");
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {FROM}
         WHERE a.last_seen > ?1
         ORDER BY a.last_range_nm IS NULL, a.last_range_nm ASC
         LIMIT 50"
    ))?;
    let rows = stmt.query_map(params![now - LIVE_WINDOW_SECS], row_to_aircraft)?;
    rows.collect()
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Record {
    pub hex: String,
    pub value: f64,
    pub name: Option<String>,
    pub typecode: Option<String>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Stats {
    pub total_aircraft: i64,
    pub currently_airborne: i64,
    pub first_seen_today: i64,
    pub new_last_7d: i64,
    pub avg_airborne_24h: Option<f64>,
    pub first_seen: Option<f64>,
    pub max_range: Option<Record>,
    pub fastest: Option<Record>,
    pub highest: Option<Record>,
    pub strongest: Option<Record>,
    pub busiest_hour: Option<Record>, // value = aircraft count that hour, hex = hour label
    pub types_seen: i64,
    pub top_type: Option<Record>, // value = aircraft count, hex = typecode
    pub top_operator: Option<Record>, // value = distinct airframes, hex = operator name
    pub top_airframe: Option<Record>, // value = polls seen, hex = aircraft hex
    pub busiest_day: Option<Record>,  // value = peak aircraft count, hex = day label
}

impl Stats {
    pub fn first_seen_str(&self) -> Option<String> {
        self.first_seen.map(|ts| {
            chrono::DateTime::from_timestamp(ts as i64, 0)
                .expect("valid ts")
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
    }
    /// One-line trend for the "visible now" card.
    pub fn airborne_trend(&self) -> String {
        match self.avg_airborne_24h {
            Some(avg) if avg > 0.0 => {
                let delta = self.currently_airborne as f64 - avg;
                let rounded = delta.abs().round() as i64;
                if rounded == 0 {
                    return format!("level with 24h avg {avg:.1}");
                }
                let arrow = if delta > 0.0 { "▲" } else { "▼" };
                format!("{arrow} {rounded} vs 24h avg {avg:.1}")
            }
            _ => "no 24h baseline yet".into(),
        }
    }
}

fn record(conn: &Connection, what: &str) -> rusqlite::Result<Option<Record>> {
    let col = match what {
        "range" => "a.max_range_nm",
        "speed" => "a.max_gs_kts",
        "alt" => "a.max_alt_ft",
        "signal" => "a.max_rssi_dbfs",
        _ => unreachable!(),
    };
    let sql = format!(
        "SELECT a.hex, {col}, e.registration, e.typecode, a.last_flight
         FROM aircraft a LEFT JOIN enrichment e ON e.icao24 = a.hex
         WHERE {col} IS NOT NULL ORDER BY {col} DESC LIMIT 1"
    );
    conn.query_row(&sql, [], |r| {
        let registration: Option<String> = r.get(2)?;
        let callsign: Option<String> = r.get(4)?;
        Ok(Record {
            hex: r.get(0)?,
            value: r.get(1)?,
            name: registration.or(callsign),
            typecode: r.get(3)?,
        })
    })
    .optional()
}

pub fn stats(db: &Db, now: f64) -> rusqlite::Result<Stats> {
    let conn = db.lock().expect("db mutex");
    let total_aircraft: i64 = conn.query_row("SELECT COUNT(*) FROM aircraft", [], |r| r.get(0))?;
    let currently_airborne: i64 =
        conn.query_row("SELECT COUNT(*) FROM aircraft WHERE last_seen > ?1", params![now - 60.0], |r| r.get(0))?;
    let today_start = local_today_start();
    // Bucket poll_log by local hour/day. The offset is applied in SQL so the
    // stored epoch labels line up with what the cards render in local time.
    let tz_offset = chrono::Local::now().offset().local_minus_utc() as f64;
    let first_seen_today: i64 =
        conn.query_row("SELECT COUNT(*) FROM aircraft WHERE first_seen >= ?1", params![today_start], |r| {
            r.get(0)
        })?;
    let new_last_7d: i64 = conn.query_row(
        "SELECT COUNT(*) FROM aircraft WHERE first_seen >= ?1",
        params![now - 7.0 * 86_400.0],
        |r| r.get(0),
    )?;
    let avg_airborne_24h: Option<f64> = conn.query_row(
        "SELECT AVG(aircraft_count) FROM poll_log WHERE ts >= ?1",
        params![now - 86_400.0],
        |r| r.get(0),
    )?;
    let first_seen: Option<f64> =
        conn.query_row("SELECT MIN(first_seen) FROM aircraft", [], |r| r.get(0))?;

    let busiest_hour: Option<Record> = conn
        .query_row(
            "SELECT CAST(CAST((ts + ?1) / 3600 AS INTEGER) * 3600 - ?1 AS INTEGER), MAX(aircraft_count)
             FROM poll_log GROUP BY 1 ORDER BY 2 DESC LIMIT 1",
            params![tz_offset],
            |r| Ok(Record { hex: r.get::<_, i64>(0)?.to_string(), value: r.get(1)?, ..Default::default() }),
        )
        .optional()?;

    let types_seen: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT e.typecode) FROM aircraft a
         JOIN enrichment e ON e.icao24 = a.hex WHERE e.typecode IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let top_type: Option<Record> = conn
        .query_row(
            "SELECT e.typecode, COUNT(*) FROM aircraft a
             JOIN enrichment e ON e.icao24 = a.hex
             WHERE e.typecode IS NOT NULL
             GROUP BY e.typecode ORDER BY 2 DESC LIMIT 1",
            [],
            |r| Ok(Record { hex: r.get(0)?, value: r.get(1)?, ..Default::default() }),
        )
        .optional()?;

    let top_operator: Option<Record> = conn
        .query_row(
            "SELECT e.operator, COUNT(DISTINCT a.hex) FROM aircraft a
             JOIN enrichment e ON e.icao24 = a.hex
             WHERE e.operator IS NOT NULL
             GROUP BY e.operator ORDER BY 2 DESC LIMIT 1",
            [],
            |r| Ok(Record { hex: r.get(0)?, value: r.get(1)?, ..Default::default() }),
        )
        .optional()?;

    let top_airframe: Option<Record> = conn
        .query_row(
            "SELECT a.hex, a.sightings, e.registration, e.typecode, a.last_flight
             FROM aircraft a LEFT JOIN enrichment e ON e.icao24 = a.hex
             ORDER BY a.sightings DESC LIMIT 1",
            [],
            |r| {
                let registration: Option<String> = r.get(2)?;
                let callsign: Option<String> = r.get(4)?;
                Ok(Record {
                    hex: r.get(0)?,
                    value: r.get(1)?,
                    name: registration.or(callsign),
                    typecode: r.get(3)?,
                })
            },
        )
        .optional()?;

    let busiest_day: Option<Record> = conn
        .query_row(
            "SELECT CAST(CAST((ts + ?1) / 86400 AS INTEGER) * 86400 - ?1 AS INTEGER), MAX(aircraft_count)
             FROM poll_log GROUP BY 1 ORDER BY 2 DESC LIMIT 1",
            params![tz_offset],
            |r| Ok(Record { hex: r.get::<_, i64>(0)?.to_string(), value: r.get(1)?, ..Default::default() }),
        )
        .optional()?;

    Ok(Stats {
        total_aircraft,
        currently_airborne,
        first_seen_today,
        new_last_7d,
        avg_airborne_24h,
        first_seen,
        max_range: record(&conn, "range")?,
        fastest: record(&conn, "speed")?,
        highest: record(&conn, "alt")?,
        strongest: record(&conn, "signal")?,
        busiest_hour,
        types_seen,
        top_type,
        top_operator,
        top_airframe,
        busiest_day,
    })
}

pub fn aircraft_by_hex(db: &Db, hex: &str) -> rusqlite::Result<Option<AircraftRow>> {
    let conn = db.lock().expect("db mutex");
    conn.query_row(
        &format!("SELECT {COLS} {FROM} WHERE a.hex = ?1"),
        params![hex],
        row_to_aircraft,
    )
    .optional()
}

#[derive(Debug, serde::Serialize)]
pub struct EmergencyRow {
    pub ts: f64,
    pub hex: String,
    pub squawk: Option<String>,
    pub emergency: String,
}

impl EmergencyRow {
    pub fn ts_str(&self) -> String {
        chrono::DateTime::from_timestamp(self.ts as i64, 0).expect("valid ts").with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }
}

pub fn emergency_log(db: &Db, limit: u32) -> rusqlite::Result<Vec<EmergencyRow>> {
    let conn = db.lock().expect("db mutex");
    let mut stmt = conn.prepare(
        "SELECT ts, hex, squawk, emergency FROM emergency_log ORDER BY ts DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(EmergencyRow {
            ts: r.get(0)?,
            hex: r.get(1)?,
            squawk: r.get(2)?,
            emergency: r.get(3)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, serde::Serialize)]
pub struct CallsignRow {
    pub callsign: String,
    pub first_seen: f64,
    pub last_seen: f64,
    pub sightings: i64,
}

impl CallsignRow {
    fn fmt_ts(epoch: f64) -> String {
        chrono::DateTime::from_timestamp(epoch as i64, 0).expect("valid ts").with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }
    pub fn first_seen_str(&self) -> String {
        Self::fmt_ts(self.first_seen)
    }
    pub fn last_seen_str(&self) -> String {
        Self::fmt_ts(self.last_seen)
    }
}

pub fn callsigns_for(db: &Db, hex: &str) -> rusqlite::Result<Vec<CallsignRow>> {
    let conn = db.lock().expect("db mutex");
    let mut stmt = conn.prepare(
        "SELECT callsign, first_seen, last_seen, sightings
         FROM aircraft_callsigns WHERE hex = ?1 ORDER BY last_seen DESC",
    )?;
    let rows = stmt.query_map(params![hex], |r| {
        Ok(CallsignRow {
            callsign: r.get(0)?,
            first_seen: r.get(1)?,
            last_seen: r.get(2)?,
            sightings: r.get(3)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, serde::Serialize)]
pub struct VisitRow {
    pub start_ts: f64,
    pub end_ts: f64,
    pub polls: i64,
}

pub fn visits_for(db: &Db, hex: &str) -> rusqlite::Result<Vec<VisitRow>> {
    let conn = db.lock().expect("db mutex");
    let mut stmt = conn.prepare(
        "SELECT start_ts, end_ts, polls FROM visits WHERE hex = ?1 ORDER BY start_ts DESC",
    )?;
    let rows = stmt.query_map(params![hex], |r| {
        Ok(VisitRow { start_ts: r.get(0)?, end_ts: r.get(1)?, polls: r.get(2)? })
    })?;
    rows.collect()
}

#[derive(Debug, serde::Serialize)]
pub struct Enrichment {
    pub registration: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub typecode: Option<String>,
    pub owner: Option<String>,
    pub operator: Option<String>,
    pub serialnumber: Option<String>,
    pub icaoaircrafttype: Option<String>,
    pub operatorcallsign: Option<String>,
    pub registered: Option<String>,
    pub reguntil: Option<String>,
    pub status: Option<String>,
    pub built: Option<String>,
    pub firstflightdate: Option<String>,
    pub engines: Option<String>,
    pub notes: Option<String>,
    pub categorydescription: Option<String>,
}

pub fn enrichment_for(db: &Db, hex: &str) -> rusqlite::Result<Option<Enrichment>> {
    let conn = db.lock().expect("db mutex");
    conn.query_row(
        "SELECT registration, manufacturer, model, typecode, owner, operator,
                serialnumber, icaoaircrafttype, operatorcallsign,
                registered, reguntil, status, built, firstflightdate, engines, notes,
                categorydescription
         FROM enrichment WHERE icao24 = ?1",
        params![hex],
        |r| {
            Ok(Enrichment {
                registration: r.get(0)?,
                manufacturer: r.get(1)?,
                model: r.get(2)?,
                typecode: r.get(3)?,
                owner: r.get(4)?,
                operator: r.get(5)?,
                serialnumber: r.get(6)?,
                icaoaircrafttype: r.get(7)?,
                operatorcallsign: r.get(8)?,
                registered: r.get(9)?,
                reguntil: r.get(10)?,
                status: r.get(11)?,
                built: r.get(12)?,
                firstflightdate: r.get(13)?,
                engines: r.get(14)?,
                notes: r.get(15)?,
                categorydescription: r.get(16)?,
            })
        },
    )
    .optional()
}
