use chrono::Timelike;
use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension, Row};

use std::collections::HashMap;

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

/// Days of history plotted on `/activity`. `poll_log` is pruned at 90 days, so
/// the messages series can never be longer than this.
const RANGE_DAYS: i64 = 90;
const MESSAGES_DAYS: i64 = 30;

/// A stored baro rate is the most negative climb, so the descent row flips the
/// sign: the label already says "descent".
fn descent_str(v: Option<i64>) -> String {
    v.map(|v| v.abs().to_string()).unwrap_or_else(|| "-".into())
}

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

/// ICAO type designators are two to four alphanumeric characters. Shared by
/// the `/type/:code` route and every row that links to it, so a row can never
/// render a link the route would reject.
pub fn valid_designator(code: &str) -> bool {
    (2..=4).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric())
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
    /// The type designator as displayed, or `-`.
    pub fn type_code(&self) -> &str {
        self.typecode.as_deref().unwrap_or("-")
    }
    /// Deep link to this aircraft's type page. OpenSky's `typecode` is free
    /// text, so link only when the dex route would accept the code.
    pub fn type_url(&self) -> Option<String> {
        self.typecode.as_deref().filter(|t| valid_designator(t)).map(|t| format!("/type/{t}"))
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
    /// All-time max climb with an explicit sign: an aircraft that only ever
    /// descended has a negative max, which must not render as `+-1200`.
    pub fn max_climb_str(&self) -> String {
        self.max_baro_rate.map(|v| format!("{v:+}")).unwrap_or_else(|| "-".into())
    }
    /// The most negative baro rate as a positive descent rate. The label
    /// already names the direction, so `-1408` under "Max descent" reads as a
    /// double negative; `climb_str` keeps its sign because "Climb" does not.
    pub fn max_descent_str(&self) -> String {
        descent_str(self.min_baro_rate)
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
        fmt_num(self.value)
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
    /// `/type/<code>` — only `top_type` stores a designator in `hex`.
    pub fn type_url(&self) -> Option<String> {
        valid_designator(&self.hex).then(|| format!("/type/{}", self.hex))
    }
}

const COLS: &str = "a.hex, a.first_seen, a.last_seen, a.sightings, a.last_flight, a.last_squawk, \
                    a.last_category, a.max_alt_ft, a.max_gs_kts, a.max_range_nm, a.max_rssi_dbfs, \
                    a.last_lat, a.last_lon, a.last_track, a.max_baro_rate, a.min_baro_rate, \
                    a.last_nav_alt_ft, a.last_nav_heading, a.last_emergency, \
                    coalesce(nullif(e.registration,''), n.registration), \
                    coalesce(nullif(e.typecode,''), n.typecode), \
                    a.last_alt_ft, a.last_gs_kts, a.last_range_nm, a.last_baro_rate, a.last_msg_age";
const FROM: &str = "FROM aircraft a LEFT JOIN enrichment e ON e.icao24 = a.hex \
                    LEFT JOIN network_enrichment n ON n.hex = a.hex";

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
        "registration" => ("coalesce(nullif(e.registration,''), n.registration)", "asc"),
        "typecode" => ("coalesce(nullif(e.typecode,''), n.typecode)", "asc"),
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

/// Local calendar day (`YYYY-MM-DD`) for an epoch timestamp. Matches the
/// `day` key the poller writes into `day_aircraft`.
pub fn local_day(now: f64) -> String {
    chrono::DateTime::from_timestamp(now as i64, 0)
        .expect("valid ts")
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string()
}

/// Local midnight for the day containing `now`, as an epoch timestamp. Matches
/// the cards' "today" bucket and the poller's `day_aircraft` day key.
fn local_day_start(now: f64) -> f64 {
    let dt = chrono::DateTime::from_timestamp(now as i64, 0)
        .expect("valid ts")
        .with_timezone(&chrono::Local);
    let midnight = dt.date_naive().and_hms_opt(0, 0, 0).expect("valid midnight");
    // A DST transition can make local midnight ambiguous or nonexistent; fall
    // back to subtracting the elapsed seconds of the day instead of panicking
    // while the DB mutex is held.
    midnight.and_local_timezone(chrono::Local).earliest().map_or_else(
        || now - f64::from(dt.time().num_seconds_from_midnight()),
        |m| m.timestamp() as f64,
    )
}

fn fleet_needle(search: &str) -> String {
    let mut out = String::with_capacity(search.len() + 2);
    out.push('%');
    for ch in search.trim().to_lowercase().chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
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
    /// Human labels for the filters currently on, for the summary line. A view
    /// that silently returns an empty table is the failure this prevents.
    pub fn active_labels(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.q.trim().is_empty() {
            out.push(format!("\u{201c}{}\u{201d}", self.q.trim()));
        }
        if self.mil {
            out.push("Military".into());
        }
        if self.interesting {
            out.push("Interesting".into());
        }
        if self.privacy {
            out.push("Privacy".into());
        }
        if self.emergency {
            out.push("Emergency history".into());
        }
        if self.today {
            out.push("Heard today".into());
        }
        if self.new7 {
            out.push("New this week".into());
        }
        if self.dormant {
            out.push("Not seen this week".into());
        }
        if self.regular {
            out.push("Regulars".into());
        }
        match self.cat.as_str() {
            "fixed" => out.push("Fixed wing".into()),
            "heli" => out.push("Helicopter".into()),
            _ => {}
        }
        if let Some(v) = self.alt_min {
            out.push(format!("max altitude \u{2265} {v} ft"));
        }
        if let Some(v) = self.alt_max {
            out.push(format!("max altitude \u{2264} {v} ft"));
        }
        out
    }

    pub fn is_active(&self) -> bool {
        !self.active_labels().is_empty()
    }

    fn to_sql(&self, now: f64) -> FleetSql {
        let mut clauses: Vec<&'static str> = Vec::new();
        let mut params: Vec<(&'static str, Value)> = Vec::new();

        if !self.q.trim().is_empty() {
            clauses.push(
                "(a.hex LIKE :q ESCAPE '\\' OR a.last_flight LIKE :q ESCAPE '\\'
                  OR e.registration LIKE :q ESCAPE '\\' OR e.model LIKE :q ESCAPE '\\'
                  OR e.typecode LIKE :q ESCAPE '\\'
                  OR e.owner LIKE :q ESCAPE '\\' OR e.operator LIKE :q ESCAPE '\\'
                  OR n.registration LIKE :q ESCAPE '\\'
                  OR n.typecode LIKE :q ESCAPE '\\'
                  OR n.description LIKE :q ESCAPE '\\'
                  OR n.owner LIKE :q ESCAPE '\\')",
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
            params.push((":today_start", Value::Real(local_day_start(now))));
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
    let conn = crate::db_lock(db);
    let mut f = filter.to_sql(now);
    f.params.push((":limit", Value::Integer(i64::from(limit))));
    f.params.push((":offset", Value::Integer(i64::from(offset))));
    // NULL and empty values sort last in both directions: a callsign sort that
    // opens on a screen of blanks reads as a broken control, not a sort.
    let sql = format!(
        "SELECT {COLS} {FROM} {}
         ORDER BY ({col} IS NULL), ({col} = ''), {col} {dir}, a.hex ASC
         LIMIT :limit OFFSET :offset",
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
    let conn = crate::db_lock(db);
    let f = filter.to_sql(now);
    // Only the search clause touches `enrichment`; skip the join when unused.
    let from = if filter.q.trim().is_empty() { "FROM aircraft a" } else { FROM };
    let sql = format!("SELECT COUNT(*) {from} {}", f.where_clause);
    let mut stmt = conn.prepare(&sql)?;
    let params = f.bind();
    stmt.query_row(&params[..], |r| r.get(0))
}

pub fn recent_arrivals(db: &Db) -> rusqlite::Result<Vec<AircraftRow>> {
    let conn = crate::db_lock(db);
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {FROM} ORDER BY a.first_seen DESC LIMIT 5"
    ))?;
    let rows = stmt.query_map([], row_to_aircraft)?;
    rows.collect()
}

/// Seconds since the poller last wrote a row, or `None` if it never has.
pub fn last_poll_age(db: &Db, now: f64) -> rusqlite::Result<Option<f64>> {
    let conn = crate::db_lock(db);
    let last: Option<f64> = conn.query_row("SELECT MAX(ts) FROM poll_log", [], |r| r.get(0))?;
    Ok(last.map(|ts| (now - ts).max(0.0)))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FeedFileStatus {
    pub ok: bool,
    pub path: String,
    pub error: Option<String>,
    pub age_secs: Option<f64>,
    pub aircraft_count: Option<usize>,
    pub messages: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FeederStatus {
    pub feed: FeedFileStatus,
    pub network: crate::network::NetworkStatus,
    pub network_age_secs: Option<f64>,
    pub snapshot_age_secs: Option<f64>,
    pub local_poller_age_secs: Option<f64>,
}

impl FeederStatus {
    pub fn network_age_str(&self) -> String {
        self.network_age_secs
            .map(|a| format!("{a:.0}s ago"))
            .unwrap_or_else(|| "never".into())
    }
    pub fn snapshot_age_str(&self) -> String {
        self.snapshot_age_secs
            .map(|a| format!("{a:.0}s ago"))
            .unwrap_or_else(|| "never".into())
    }
    pub fn local_poller_str(&self) -> String {
        self.local_poller_age_secs
            .map(|a| format!("{a:.0}s ago"))
            .unwrap_or_else(|| "no polls yet".into())
    }
}

#[derive(serde::Deserialize)]
struct FeedPayload {
    #[serde(default)]
    now: f64,
    #[serde(default)]
    messages: i64,
    #[serde(default)]
    aircraft: Vec<serde_json::Value>,
}

pub fn feeder_status(db: &Db, now: f64) -> rusqlite::Result<FeederStatus> {
    let view = crate::network::latest();
    let path = std::env::var("FEED_JSON")
        .unwrap_or_else(|_| "/run/adsbfi-feed/aircraft.json".to_string());
    let feed = read_feed_file(&path, now);
    let network = view.status.clone();
    let network_age_secs = view.fetched_at.map(|t| (now - t).max(0.0));
    let snapshot_age_secs = network.snapshot_at.map(|t| (now - t).max(0.0));
    Ok(FeederStatus {
        feed,
        network,
        network_age_secs,
        snapshot_age_secs,
        local_poller_age_secs: last_poll_age(db, now)?,
    })
}

fn read_feed_file(path: &str, now: f64) -> FeedFileStatus {
    let meta = std::fs::metadata(path);
    let mtime = meta
        .as_ref()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64());
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<FeedPayload>(&text) {
            Ok(p) => FeedFileStatus {
                ok: true,
                path: path.to_string(),
                error: None,
                age_secs: mtime
                    .map(|t| (now - t).max(0.0))
                    .or(Some((now - p.now).max(0.0))),
                aircraft_count: Some(p.aircraft.len()),
                messages: Some(p.messages),
            },
            Err(e) => FeedFileStatus {
                ok: false,
                path: path.to_string(),
                error: Some(e.to_string()),
                age_secs: mtime.map(|t| (now - t).max(0.0)),
                aircraft_count: None,
                messages: None,
            },
        },
        Err(e) => FeedFileStatus {
            ok: false,
            path: path.to_string(),
            error: Some(e.to_string()),
            age_secs: mtime.map(|t| (now - t).max(0.0)),
            aircraft_count: None,
            messages: None,
        },
    }
}

/// Whether the receiver is still answering. The dashboard's whole liveness
/// claim hangs off this, so it is a first-class datum, not a decoration.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PollerHealth {
    pub age_secs: Option<f64>,
}

/// A poll older than this means the receiver has stopped answering.
const POLL_STALE_SECS: f64 = 30.0;
/// Beyond this the poller is presumed dead rather than briefly late.
const POLL_DOWN_SECS: f64 = 300.0;

impl PollerHealth {
    pub fn new(age_secs: Option<f64>) -> Self {
        Self { age_secs }
    }

    /// `live` / `stale` / `down` — the class the pill carries.
    pub fn state(&self) -> &'static str {
        match self.age_secs {
            None => "down",
            Some(a) if a < POLL_STALE_SECS => "live",
            Some(a) if a < POLL_DOWN_SECS => "stale",
            _ => "down",
        }
    }

    pub fn is_live(&self) -> bool {
        self.state() == "live"
    }

    fn age_phrase(&self) -> String {
        match self.age_secs {
            None => "never".to_string(),
            Some(a) => {
                let s = a.round() as i64;
                if s < 60 {
                    format!("{s} s ago")
                } else if s < 3600 {
                    format!("{} m ago", s / 60)
                } else {
                    format!("{} h ago", s / 3600)
                }
            }
        }
    }

    /// The pill's visible text.
    pub fn label(&self) -> String {
        match self.state() {
            "live" => format!("live · {}", self.age_phrase()),
            "stale" => format!("stalled · {}", self.age_phrase()),
            _ => format!("poller down · {}", self.age_phrase()),
        }
    }

    /// Announced once when the state leaves `live`. `None` while healthy, so
    /// the alert region is absent and nothing is re-read on every tick.
    pub fn alert(&self) -> Option<String> {
        match self.state() {
            "live" => None,
            "stale" => Some(format!("Receiver stalled — last poll {}", self.age_phrase())),
            _ => Some(format!("Poller down — last poll {}", self.age_phrase())),
        }
    }

    /// The empty live panel's version of the state, as a full sentence.
    ///
    /// An empty panel says "nothing overhead" only while polls are landing.
    /// With the receiver silent the same empty list means the opposite — the
    /// page would be claiming clear skies it cannot see. `None` means healthy.
    pub fn sentence(&self) -> Option<String> {
        match self.state() {
            "live" => None,
            "stale" => Some(format!(
                "The receiver stalled {} — nothing new is being logged, so this is not clear skies.",
                self.age_phrase()
            )),
            _ => Some(format!(
                "The poller stopped {} — nothing is being logged, so this is not clear skies.",
                self.age_phrase()
            )),
        }
    }
}

/// Per-flag totals, so a filter chip can say how many rows it would match
/// instead of silently returning an empty table.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct FilterCounts {
    pub total: i64,
    pub mil: i64,
    pub interesting: i64,
    pub privacy: i64,
    pub emergency: i64,
    pub today: i64,
    pub new7: i64,
    pub dormant: i64,
    pub regular: i64,
    pub fixed: i64,
    pub heli: i64,
}

pub fn filter_counts(db: &Db, now: f64) -> rusqlite::Result<FilterCounts> {
    let conn = crate::db_lock(db);
    let today_start = local_day_start(now);
    let new7_start = now - 7.0 * 86_400.0;
    let dormant_start = now - DORMANT_SECS;
    conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM((db_flags & 1) = 1), 0),
                COALESCE(SUM((db_flags & 2) = 2), 0),
                COALESCE(SUM((db_flags & 12) != 0), 0),
                COALESCE(SUM(EXISTS (SELECT 1 FROM emergency_log el WHERE el.hex = aircraft.hex)), 0),
                COALESCE(SUM(last_seen >= ?1), 0),
                COALESCE(SUM(first_seen >= ?2), 0),
                COALESCE(SUM(last_seen < ?3), 0),
                COALESCE(SUM((SELECT COUNT(*) FROM visits v WHERE v.hex = aircraft.hex) >= ?4), 0),
                COALESCE(SUM(last_category IN ('A1','A2','A3','A4','A5','A6')), 0),
                COALESCE(SUM(last_category = 'A7'), 0)
         FROM aircraft",
        params![today_start, new7_start, dormant_start, REGULAR_VISITS],
        |r| {
            Ok(FilterCounts {
                total: r.get(0)?,
                mil: r.get(1)?,
                interesting: r.get(2)?,
                privacy: r.get(3)?,
                emergency: r.get(4)?,
                today: r.get(5)?,
                new7: r.get(6)?,
                dormant: r.get(7)?,
                regular: r.get(8)?,
                fixed: r.get(9)?,
                heli: r.get(10)?,
            })
        },
    )
}

/// Aircraft still on the tar1090 map, nearest first (unpositioned last).
pub fn live_aircraft(db: &Db, now: f64) -> rusqlite::Result<Vec<AircraftRow>> {
    let conn = crate::db_lock(db);
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {FROM}
         WHERE a.last_seen > ?1
         ORDER BY a.last_range_nm IS NULL, a.last_range_nm ASC
         LIMIT 50"
    ))?;
    let rows = stmt.query_map(params![now - LIVE_WINDOW_SECS], row_to_aircraft)?;
    rows.collect()
}

/// One local hour of today's traffic: peak aircraft overhead in that hour.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HourBucket {
    pub hour: u32,
    pub count: i64,
}

impl HourBucket {
    pub fn label(&self) -> String {
        format!("{:02}", self.hour)
    }
    /// Label only every third hour; the rest keep the row height.
    pub fn show_label(&self) -> bool {
        self.hour % 3 == 0
    }
    /// Bar height as a percentage of the busiest hour. Any traffic gets a 2%
    /// floor so a quiet hour stays visible.
    pub fn pct(&self, max: &i64) -> i64 {
        if self.count <= 0 || *max <= 0 {
            0
        } else {
            (self.count * 100 / *max).max(2)
        }
    }
}

/// A metric's best value today, plus the best on any earlier day.
#[derive(Debug, serde::Serialize)]
pub struct DayRecord {
    pub metric: &'static str,
    pub unit: &'static str,
    pub today: f64,
    pub prior: Option<f64>,
    /// True only when a prior day exists and today beat it.
    pub improved: bool,
    pub hex: String,
    pub ident: String,
}

impl DayRecord {
    pub fn today_str(&self) -> String {
        fmt_num(self.today)
    }
    pub fn prior_str(&self) -> String {
        self.prior.map(fmt_num).unwrap_or_else(|| "-".into())
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct TodaySummary {
    pub day: String,
    pub unique: i64,
    /// Aircraft in today's ledger whose all-time first sighting was today.
    /// Always a subset of `unique`.
    pub new: i64,
    /// Poll cycles recorded today.
    pub polls: i64,
    /// Aircraft-poll sightings today (sum of `day_aircraft.polls`).
    pub sightings: i64,
    /// Messages received today: sum of positive deltas of tar1090's cumulative
    /// `messages` counter across today's polls.
    pub messages: i64,
    pub peak_airborne: Option<i64>,
    pub first_ts: Option<f64>,
    pub last_ts: Option<f64>,
}

impl TodaySummary {
    fn fmt_ts(epoch: f64) -> String {
        chrono::DateTime::from_timestamp(epoch as i64, 0)
            .expect("valid ts")
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string()
    }
    pub fn first_str(&self) -> Option<String> {
        self.first_ts.map(Self::fmt_ts)
    }
    pub fn last_str(&self) -> Option<String> {
        self.last_ts.map(Self::fmt_ts)
    }
    /// `HH:MM–HH:MM` local span of today's activity.
    pub fn span_str(&self) -> String {
        match (self.first_str(), self.last_str()) {
            (Some(a), Some(b)) => format!("{a}–{b}"),
            _ => "-".into(),
        }
    }
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < f64::EPSILON {
        format!("{}", v as i64)
    } else {
        format!("{v:.1}")
    }
}

fn clock_str(epoch: f64) -> String {
    chrono::DateTime::from_timestamp(epoch as i64, 0)
        .expect("valid ts")
        .with_timezone(&chrono::Local)
        .format("%H:%M:%S")
        .to_string()
}

/// Compact y-axis labels: large counters read as `394k` instead of six digits.
fn fmt_axis(v: f64) -> String {
    let a = v.abs();
    if a >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if a >= 10_000.0 {
        format!("{:.0}k", v / 1_000.0)
    } else if a >= 1_000.0 {
        format!("{:.1}k", v / 1_000.0)
    } else {
        fmt_num(v)
    }
}

/// Today's row in the daily ledger, aggregated.
pub fn today_summary(db: &Db, now: f64) -> rusqlite::Result<TodaySummary> {
    let conn = crate::db_lock(db);
    let day = local_day(now);
    let today_start = local_day_start(now);
    let (unique, sightings, first_ts, last_ts) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(polls), 0), MIN(first_ts), MAX(last_ts)
         FROM day_aircraft WHERE day = ?1",
        params![day],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    // Scoped to today's ledger rows so `new ⊆ unique`: on the day the ledger
    // was created, aircraft first seen earlier that day have no ledger row.
    let new: i64 = conn.query_row(
        "SELECT COUNT(*) FROM day_aircraft d JOIN aircraft a ON a.hex = d.hex
         WHERE d.day = ?1 AND a.first_seen >= ?2",
        params![day, today_start],
        |r| r.get(0),
    )?;
    let (polls, peak_airborne) = conn.query_row(
        "SELECT COUNT(*), MAX(aircraft_count) FROM poll_log WHERE ts >= ?1",
        params![today_start],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    // `poll_log.messages` is tar1090's cumulative counter, not a per-poll count.
    // Sum the positive deltas so the digest reports messages received today and
    // a readsb restart mid-day does not produce a negative contribution.
    let messages: i64 = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN d > 0 THEN d ELSE 0 END), 0) FROM (
           SELECT messages - LAG(messages) OVER (ORDER BY ts) AS d
           FROM poll_log WHERE ts >= ?1
         )",
        params![today_start],
        |r| r.get(0),
    )?;
    Ok(TodaySummary {
        day,
        unique,
        new,
        polls,
        sightings,
        messages,
        peak_airborne,
        first_ts,
        last_ts,
    })
}

/// Peak aircraft overhead per local hour today, always 24 buckets.
pub fn hour_histogram(db: &Db, now: f64) -> rusqlite::Result<Vec<HourBucket>> {
    let conn = crate::db_lock(db);
    let today_start = local_day_start(now);
    let tz_offset = chrono::Local::now().offset().local_minus_utc() as i64;
    let mut stmt = conn.prepare(
        "SELECT CAST(((ts + ?1) % 86400) / 3600 AS INTEGER) AS hour, MAX(max_count)
         FROM poll_hour WHERE ts >= ?2 GROUP BY hour",
    )?;
    let mut buckets: Vec<HourBucket> =
        (0..24).map(|hour| HourBucket { hour, count: 0 }).collect();
    let rows = stmt.query_map(params![tz_offset, today_start], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (hour, count) = row?;
        if let Some(b) = buckets.get_mut(hour as usize) {
            b.count = count;
        }
    }
    Ok(buckets)
}

/// Weekday labels in heatmap row order, Monday first.
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// One cell of the weekday × hour activity grid.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HeatCell {
    pub hour: u32,
    pub count: i64,
}

impl HeatCell {
    /// 0 = no traffic; 1..=5 = intensity relative to the busiest cell.
    pub fn level(&self, max: &i64) -> u8 {
        if self.count <= 0 || *max <= 0 {
            0
        } else {
            (((self.count * 5 + *max - 1) / *max) as u8).clamp(1, 5)
        }
    }
    pub fn title(&self, weekday: &str) -> String {
        format!("{weekday} {:02}:00 · {} aircraft", self.hour, self.count)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HeatRow {
    pub weekday: u32,
    pub label: &'static str,
    pub cells: Vec<HeatCell>,
    /// Sum of the row's per-slot peaks; ranks weekdays against each other.
    pub total: i64,
}

/// Peak aircraft overhead for every weekday × local hour slot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Heatmap {
    pub rows: Vec<HeatRow>,
    pub max: i64,
    /// Weekday label of the busiest slot, empty when there are no polls.
    pub peak_weekday: String,
    pub peak_hour: u32,
}

/// `poll_hour` is the same MAX-per-hour rollup as `poll_log`, so grouping it
/// gives identical cells with ~360× fewer rows. The local offset is applied in
/// SQL like the cards, so the busiest cell equals the "busiest hour" card.
pub fn weekday_hour_heatmap(db: &Db) -> rusqlite::Result<Heatmap> {
    let conn = crate::db_lock(db);
    let tz_offset = chrono::Local::now().offset().local_minus_utc() as i64;
    let mut counts = [[0i64; 24]; 7];
    let mut stmt = conn.prepare(
        "SELECT CAST((((ts + ?1) / 86400) + 3) % 7 AS INTEGER) AS weekday,
                CAST(((ts + ?1) % 86400) / 3600 AS INTEGER) AS hour,
                MAX(max_count)
         FROM poll_hour GROUP BY weekday, hour",
    )?;
    let rows = stmt.query_map(params![tz_offset], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
    })?;
    for row in rows {
        let (weekday, hour, count) = row?;
        if let (Ok(w), Ok(h)) = (usize::try_from(weekday), usize::try_from(hour)) {
            if let Some(slot) = counts.get_mut(w).and_then(|cells| cells.get_mut(h)) {
                *slot = count;
            }
        }
    }

    let mut max = 0i64;
    let mut peak = (0usize, 0usize);
    let mut out = Vec::with_capacity(7);
    for (weekday, cells) in counts.iter().enumerate() {
        for (hour, count) in cells.iter().enumerate() {
            if *count > max {
                max = *count;
                peak = (weekday, hour);
            }
        }
        out.push(HeatRow {
            weekday: weekday as u32,
            label: WEEKDAYS[weekday],
            cells: cells
                .iter()
                .enumerate()
                .map(|(hour, count)| HeatCell { hour: hour as u32, count: *count })
                .collect(),
            total: cells.iter().sum(),
        });
    }
    Ok(Heatmap {
        rows: out,
        max,
        peak_weekday: if max > 0 { WEEKDAYS[peak.0].to_string() } else { String::new() },
        peak_hour: peak.1 as u32,
    })
}

/// One point in a daily series (`day` is the local `YYYY-MM-DD` key).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DayValue {
    pub day: String,
    pub value: f64,
}

/// Best range per local day, one point per day, last 90 days.
pub fn range_by_day(db: &Db, now: f64) -> rusqlite::Result<Vec<DayValue>> {
    let conn = crate::db_lock(db);
    let cutoff = local_day(now - (RANGE_DAYS - 1) as f64 * 86_400.0);
    let mut stmt = conn.prepare(
        "SELECT day, MAX(max_range_nm) FROM day_aircraft
         WHERE day >= ?1 AND max_range_nm IS NOT NULL
         GROUP BY day ORDER BY day",
    )?;
    let rows = stmt.query_map(params![cutoff], |r| {
        Ok(DayValue { day: r.get(0)?, value: r.get(1)? })
    })?;
    rows.collect()
}

/// Messages received per local day, last 30 days. `poll_log.messages` is
/// tar1090's cumulative counter, so sum the positive per-poll deltas.
pub fn messages_by_day(db: &Db, now: f64) -> rusqlite::Result<Vec<DayValue>> {
    let conn = crate::db_lock(db);
    let cutoff = now - (MESSAGES_DAYS - 1) as f64 * 86_400.0;
    let tz_offset = chrono::Local::now().offset().local_minus_utc() as i64;
    let mut stmt = conn.prepare(
        "SELECT day, COALESCE(SUM(CASE WHEN delta > 0 THEN delta ELSE 0 END), 0)
         FROM (
           SELECT CAST((ts + ?1) / 86400 AS INTEGER) AS day,
                  messages - LAG(messages) OVER (ORDER BY ts) AS delta
           FROM poll_log WHERE ts >= ?2
         )
         GROUP BY day ORDER BY day",
    )?;
    let rows = stmt.query_map(params![tz_offset, cutoff], |r| {
        let day: i64 = r.get(0)?;
        // `day` is already a local day index (offset applied in SQL). Format it
        // in UTC directly: converting back through `Local` would apply the
        // historical DST offset and can shift the label one day around the
        // spring transition.
        let day = chrono::DateTime::from_timestamp(day * 86_400, 0)
            .expect("valid ts")
            .format("%Y-%m-%d")
            .to_string();
        Ok(DayValue { day, value: r.get(1)? })
    })?;
    rows.collect()
}

#[derive(Debug, serde::Serialize)]
pub struct Activity {
    pub heatmap: Heatmap,
    pub range: Vec<DayValue>,
    pub messages: Vec<DayValue>,
}

pub fn activity(db: &Db, now: f64) -> rusqlite::Result<Activity> {
    Ok(Activity {
        heatmap: weekday_hour_heatmap(db)?,
        range: range_by_day(db, now)?,
        messages: messages_by_day(db, now)?,
    })
}

/// Fixed viewBox for the activity charts; CSS scales it to the panel width.
const CHART_W: f64 = 640.0;
const CHART_H: f64 = 120.0;
const CHART_PAD: f64 = 8.0;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChartPoint {
    pub x: f64,
    pub y: f64,
    pub label: String,
    pub value: f64,
}

impl ChartPoint {
    pub fn value_str(&self) -> String {
        fmt_num(self.value)
    }
}

/// A y-axis tick: label text plus its position as a percentage from the top
/// (for the HTML label column) and in viewBox units (for the SVG gridline).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AxisLabel {
    pub text: String,
    pub top: f64,
    pub y: f64,
}

/// A daily series scaled into a fixed SVG viewBox.
#[derive(Debug, serde::Serialize)]
pub struct LineChart {
    pub path: String,
    pub area: String,
    pub points: Vec<ChartPoint>,
    pub min: f64,
    pub max: f64,
    pub first_label: String,
    pub last_label: String,
    pub width: f64,
    pub height: f64,
    pub y_labels: Vec<AxisLabel>,
    pub plot_left: f64,
    pub plot_right: f64,
}

impl LineChart {
    /// One point per day, evenly spaced. Missing days compress rather than
    /// leaving calendar gaps, so the line stays readable on a sparse ledger.
    pub fn build(series: &[DayValue]) -> Self {
        let (width, height) = (CHART_W, CHART_H);
        if series.is_empty() {
            return Self {
                path: String::new(),
                area: String::new(),
                points: Vec::new(),
                min: 0.0,
                max: 0.0,
                first_label: String::new(),
                last_label: String::new(),
                width,
                height,
                y_labels: Vec::new(),
                plot_left: 0.0,
                plot_right: 0.0,
            };
        }
        let min = series.iter().map(|d| d.value).fold(f64::INFINITY, f64::min);
        let max = series.iter().map(|d| d.value).fold(f64::NEG_INFINITY, f64::max);
        let flat = max - min < f64::EPSILON;
        let span = if flat { 1.0 } else { max - min };
        let n = series.len();
        let step = if n > 1 { (width - 2.0 * CHART_PAD) / (n - 1) as f64 } else { 0.0 };
        let points: Vec<ChartPoint> = series
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let x = if n > 1 { CHART_PAD + step * i as f64 } else { width / 2.0 };
                let y = if flat {
                    height / 2.0
                } else {
                    height - CHART_PAD - ((d.value - min) / span) * (height - 2.0 * CHART_PAD)
                };
                ChartPoint { x, y, label: short_day(&d.day), value: d.value }
            })
            .collect();
        let mut path = String::new();
        let mut area = format!("M {:.1},{:.1}", points[0].x, height - CHART_PAD);
        for (i, p) in points.iter().enumerate() {
            let cmd = if i == 0 { "M" } else { "L" };
            path.push_str(&format!(" {cmd} {:.1},{:.1}", p.x, p.y));
            area.push_str(&format!(" L {:.1},{:.1}", p.x, p.y));
        }
        area.push_str(&format!(" L {:.1},{:.1} Z", points[n - 1].x, height - CHART_PAD));
        let y_labels = if flat {
            vec![AxisLabel { text: fmt_axis(min), top: 50.0, y: height / 2.0 }]
        } else {
            vec![
                AxisLabel {
                    text: fmt_axis(max),
                    top: CHART_PAD / height * 100.0,
                    y: CHART_PAD,
                },
                AxisLabel {
                    text: fmt_axis((min + max) / 2.0),
                    top: 50.0,
                    y: height / 2.0,
                },
                AxisLabel {
                    text: fmt_axis(min),
                    top: (height - CHART_PAD) / height * 100.0,
                    y: height - CHART_PAD,
                },
            ]
        };
        Self {
            path: path.trim_start().to_string(),
            area,
            min,
            max,
            first_label: points[0].label.clone(),
            last_label: points[n - 1].label.clone(),
            points,
            width,
            height,
            y_labels,
            plot_left: CHART_PAD,
            plot_right: width - CHART_PAD,
        }
    }
}

fn short_day(day: &str) -> String {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|d| d.format("%b %d").to_string())
        .unwrap_or_else(|_| day.to_string())
}

/// Today's best altitude, speed, and range against every earlier day.
pub fn records_today(db: &Db, now: f64) -> rusqlite::Result<Vec<DayRecord>> {
    let conn = crate::db_lock(db);
    let day = local_day(now);
    let mut out = Vec::with_capacity(3);
    for (metric, unit, col) in [
        ("Altitude", "ft", "max_alt_ft"),
        ("Speed", "kt", "max_gs_kts"),
        ("Range", "nm", "max_range_nm"),
    ] {
        if let Some(rec) = day_record(&conn, &day, metric, unit, col)? {
            out.push(rec);
        }
    }
    Ok(out)
}

/// `col` is one of the fixed metric columns above, never user input.
fn day_record(
    conn: &Connection,
    day: &str,
    metric: &'static str,
    unit: &'static str,
    col: &str,
) -> rusqlite::Result<Option<DayRecord>> {
    let today = conn
        .query_row(
            &format!(
                "SELECT d.hex, d.{col}, e.registration, e.typecode, a.last_flight
                 FROM day_aircraft d
                 LEFT JOIN aircraft a ON a.hex = d.hex
                 LEFT JOIN enrichment e ON e.icao24 = d.hex
                 WHERE d.day = ?1 AND d.{col} IS NOT NULL
                 ORDER BY d.{col} DESC LIMIT 1"
            ),
            params![day],
            |r| {
                let hex: String = r.get(0)?;
                let registration: Option<String> = r.get(2)?;
                let typecode: Option<String> = r.get(3)?;
                let callsign: Option<String> = r.get(4)?;
                let name = registration.or(callsign).unwrap_or_else(|| hex.clone());
                let ident = match typecode {
                    Some(t) => format!("{name} · {t}"),
                    None => name,
                };
                Ok(DayRecord {
                    metric,
                    unit,
                    today: r.get(1)?,
                    prior: None,
                    improved: false,
                    hex,
                    ident,
                })
            },
        )
        .optional()?;
    let Some(mut rec) = today else { return Ok(None) };
    let prior: Option<f64> = conn.query_row(
        &format!("SELECT MAX({col}) FROM day_aircraft WHERE day < ?1"),
        params![day],
        |r| r.get(0),
    )?;
    rec.improved = prior.is_some_and(|p| rec.today > p);
    rec.prior = prior;
    Ok(Some(rec))
}

/// Aircraft seen for the first time today, newest first. Restricted to rows in
/// today's ledger so the list matches `TodaySummary::new` and never exceeds
/// `unique`.
pub fn new_today(db: &Db, now: f64) -> rusqlite::Result<Vec<AircraftRow>> {
    let conn = crate::db_lock(db);
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {FROM} JOIN day_aircraft d ON d.hex = a.hex AND d.day = ?1
         WHERE a.first_seen >= ?2 ORDER BY a.first_seen DESC LIMIT 500"
    ))?;
    let rows = stmt.query_map(
        params![local_day(now), local_day_start(now)],
        row_to_aircraft,
    )?;
    rows.collect()
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Record {
    pub hex: String,
    pub value: f64,
    pub name: Option<String>,
    pub typecode: Option<String>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Stats {
    pub total_aircraft: i64,
    pub currently_airborne: i64,
    pub first_seen_today: i64,
    /// Distinct aircraft in today's `day_aircraft` ledger.
    pub heard_today: i64,
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
    /// One-line trend for the "overhead now" card.
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
        "SELECT a.hex, {col},
                coalesce(nullif(e.registration,''), n.registration),
                coalesce(nullif(e.typecode,''), n.typecode),
                a.last_flight
         FROM aircraft a
         LEFT JOIN enrichment e ON e.icao24 = a.hex
         LEFT JOIN network_enrichment n ON n.hex = a.hex
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

/// `Stats` is 16 queries over all-time aggregates; the cards tick asks for it
/// every 10 s. Serve a 10 s-old copy instead of re-running it.
const STATS_TTL_SECS: f64 = 10.0;

static STATS_CACHE: std::sync::LazyLock<std::sync::Mutex<Option<(f64, Stats)>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

pub fn stats_cached(db: &Db, now: f64) -> rusqlite::Result<Stats> {
    {
        let guard = STATS_CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((at, s)) = guard.as_ref() {
            if now - at < STATS_TTL_SECS {
                return Ok(s.clone());
            }
        }
    }
    // Deliberately outside the cache lock: the queries take ~20 ms and a
    // concurrent caller must not be serialized behind them.
    let s = stats(db, now)?;
    *STATS_CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((now, s.clone()));
    Ok(s)
}

pub fn stats(db: &Db, now: f64) -> rusqlite::Result<Stats> {
    let conn = crate::db_lock(db);
    let total_aircraft: i64 = conn.query_row("SELECT COUNT(*) FROM aircraft", [], |r| r.get(0))?;
    let currently_airborne: i64 = conn.query_row(
        "SELECT COUNT(*) FROM aircraft WHERE last_seen > ?1",
        params![now - LIVE_WINDOW_SECS],
        |r| r.get(0),
    )?;
    let today_start = local_day_start(now);
    // Bucket poll_log by local hour/day. The offset is applied in SQL so the
    // stored epoch labels line up with what the cards render in local time.
    let tz_offset = chrono::Local::now().offset().local_minus_utc() as f64;
    let first_seen_today: i64 =
        conn.query_row("SELECT COUNT(*) FROM aircraft WHERE first_seen >= ?1", params![today_start], |r| {
            r.get(0)
        })?;
    let heard_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM day_aircraft WHERE day = ?1",
        params![local_day(now)],
        |r| r.get(0),
    )?;
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
            "SELECT CAST(CAST((ts + ?1) / 3600 AS INTEGER) * 3600 - ?1 AS INTEGER), MAX(max_count)
             FROM poll_hour GROUP BY 1 ORDER BY 2 DESC LIMIT 1",
            params![tz_offset],
            |r| Ok(Record { hex: r.get::<_, i64>(0)?.to_string(), value: r.get(1)?, ..Default::default() }),
        )
        .optional()?;

    // CROSS JOIN pins aircraft as the outer loop; with a plain JOIN SQLite scans
    // all ~520k enrichment rows (SCAN e) to answer a 343-row question.
    let types_seen: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT e.typecode) FROM aircraft a
         CROSS JOIN enrichment e ON e.icao24 = a.hex WHERE e.typecode IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let top_type: Option<Record> = conn
        .query_row(
            "SELECT e.typecode, COUNT(*) FROM aircraft a
             CROSS JOIN enrichment e ON e.icao24 = a.hex
             WHERE e.typecode IS NOT NULL
             GROUP BY e.typecode ORDER BY 2 DESC LIMIT 1",
            [],
            |r| Ok(Record { hex: r.get(0)?, value: r.get(1)?, ..Default::default() }),
        )
        .optional()?;

    let top_operator: Option<Record> = conn
        .query_row(
            "SELECT e.operator, COUNT(DISTINCT a.hex) FROM aircraft a
             CROSS JOIN enrichment e ON e.icao24 = a.hex
             WHERE e.operator IS NOT NULL
             GROUP BY e.operator ORDER BY 2 DESC LIMIT 1",
            [],
            |r| Ok(Record { hex: r.get(0)?, value: r.get(1)?, ..Default::default() }),
        )
        .optional()?;

    let top_airframe: Option<Record> = conn
        .query_row(
            "SELECT a.hex, a.sightings,
                    coalesce(nullif(e.registration,''), n.registration),
                    coalesce(nullif(e.typecode,''), n.typecode),
                    a.last_flight
             FROM aircraft a
             LEFT JOIN enrichment e ON e.icao24 = a.hex
             LEFT JOIN network_enrichment n ON n.hex = a.hex
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
            "SELECT CAST(CAST((ts + ?1) / 86400 AS INTEGER) * 86400 - ?1 AS INTEGER), MAX(max_count)
             FROM poll_hour GROUP BY 1 ORDER BY 2 DESC LIMIT 1",
            params![tz_offset],
            |r| Ok(Record { hex: r.get::<_, i64>(0)?.to_string(), value: r.get(1)?, ..Default::default() }),
        )
        .optional()?;

    Ok(Stats {
        total_aircraft,
        currently_airborne,
        first_seen_today,
        heard_today,
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
    let conn = crate::db_lock(db);
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
    let conn = crate::db_lock(db);
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
    let conn = crate::db_lock(db);
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
    let conn = crate::db_lock(db);
    let mut stmt = conn.prepare(
        "SELECT start_ts, end_ts, polls FROM visits WHERE hex = ?1 ORDER BY start_ts DESC LIMIT 200",
    )?;
    let rows = stmt.query_map(params![hex], |r| {
        Ok(VisitRow { start_ts: r.get(0)?, end_ts: r.get(1)?, polls: r.get(2)? })
    })?;
    rows.collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CoverageRow {
    pub hex: String,
    pub flight: Option<String>,
    pub registration: Option<String>,
    pub typecode: Option<String>,
    pub alt_ft: Option<i64>,
    pub gs_kts: Option<f64>,
    pub dst_nm: Option<f64>,
    pub dir_deg: Option<f64>,
    pub seen: Option<f64>,
    pub mlat_pos: bool,
    pub last_seen_local: Option<String>,
    pub local: bool,
}

impl CoverageRow {
    pub fn type_url(&self) -> Option<String> {
        self.typecode
            .as_deref()
            .filter(|c| valid_designator(c))
            .map(|c| format!("/type/{c}"))
    }
    pub fn map_url(&self, base: &str) -> Option<String> {
        self.local.then(|| format!("{base}/?icao={}", self.hex))
    }
    pub fn trace_url(&self, base: &str) -> Option<String> {
        self.local.then(|| format!("{base}/?showTrace={}", self.hex))
    }
    pub fn adsb_url(&self) -> String {
        format!("https://globe.adsb.fi/?icao={}", self.hex)
    }
    pub fn alt_str(&self) -> String {
        self.alt_ft.map(|v| v.to_string()).unwrap_or_else(|| "-".into())
    }
    pub fn dst_str(&self) -> String {
        self.dst_nm.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into())
    }
    pub fn dir_str(&self) -> String {
        self.dir_deg.map(|v| format!("{v:.0}\u{00b0}")).unwrap_or_else(|| "-".into())
    }
    pub fn seen_str(&self) -> String {
        self.seen.map(|v| format!("{v:.0}s")).unwrap_or_else(|| "-".into())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkCoverage {
    pub fetched_at: Option<f64>,
    pub age_secs: Option<f64>,
    pub ok: bool,
    pub error: Option<String>,
    pub stale: bool,
    pub radius_nm: f64,
    pub local_count: usize,
    pub network_count: usize,
    pub overlap: usize,
    pub unique: usize,
    pub missed: usize,
    pub unique_rows: Vec<CoverageRow>,
    pub missed_rows: Vec<CoverageRow>,
}

impl NetworkCoverage {
    pub fn status_str(&self) -> String {
        if self.ok {
            match self.age_secs {
                Some(a) => format!("network poll {a:.0}s ago"),
                None => "network poll never completed".into(),
            }
        } else {
            match &self.error {
                Some(e) => format!("network unavailable: {e}"),
                None => "network unavailable".into(),
            }
        }
    }
}

/// Diff the local receiver's live set against the network's rolling set.
///
/// `network::latest()` is called before `db_lock` so the view lock is never
/// held while waiting for SQLite.
pub fn network_coverage(db: &Db, now: f64) -> rusqlite::Result<NetworkCoverage> {
    let view = crate::network::latest();
    let conn = crate::db_lock(db);
    let cutoff = now - LIVE_WINDOW_SECS;

    let mut local: HashMap<String, CoverageRow> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT a.hex, a.last_flight,
                coalesce(nullif(e.registration,''), n.registration),
                coalesce(nullif(e.typecode,''), n.typecode),
                a.last_alt_ft, a.last_gs_kts, a.last_range_nm,
                a.last_lat, a.last_lon, a.last_seen
         FROM aircraft a
         LEFT JOIN enrichment e ON e.icao24 = a.hex
         LEFT JOIN network_enrichment n ON n.hex = a.hex
         WHERE a.last_seen > ?1 AND (a.last_alt_ft IS NULL OR a.last_alt_ft > 0)",
    )?;
    let rows = stmt.query_map(params![cutoff], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, Option<f64>>(5)?,
            r.get::<_, Option<f64>>(6)?,
            r.get::<_, Option<f64>>(7)?,
            r.get::<_, Option<f64>>(8)?,
            r.get::<_, f64>(9)?,
        ))
    })?;
    let rx = rx_pos();
    for row in rows {
        let (hex, flight, registration, typecode, alt_ft, gs_kts, dst_nm, last_lat, last_lon, last_seen) =
            row?;
        let dir_deg = match (rx, last_lat, last_lon) {
            (Some((rlat, rlon)), Some(lat), Some(lon)) => Some(bearing_deg(rlat, rlon, lat, lon)),
            _ => None,
        };
        local.insert(
            hex.clone(),
            CoverageRow {
                hex,
                flight,
                registration,
                typecode,
                alt_ft,
                gs_kts,
                dst_nm,
                dir_deg,
                seen: Some((now - last_seen).max(0.0)),
                mlat_pos: false,
                last_seen_local: Some(clock_str(last_seen)),
                local: true,
            },
        );
    }

    let mut network: HashMap<String, CoverageRow> = HashMap::new();
    for (hex, a) in &view.aircraft {
        if a.on_ground || !view.recent.contains_key(hex) {
            continue;
        }
        network.insert(
            hex.clone(),
            CoverageRow {
                hex: hex.clone(),
                flight: a.flight.clone(),
                registration: a.registration.clone(),
                typecode: a.typecode.clone(),
                alt_ft: a.alt_ft,
                gs_kts: a.gs_kts,
                dst_nm: a.dst_nm,
                dir_deg: a.dir_deg,
                seen: a.seen,
                mlat_pos: a.mlat_pos,
                last_seen_local: None,
                local: false,
            },
        );
    }

    let mut unique_rows = Vec::new();
    let mut overlap = 0usize;
    for (hex, row) in &local {
        if network.contains_key(hex) {
            overlap += 1;
        } else {
            unique_rows.push(row.clone());
        }
    }
    let mut missed_rows: Vec<CoverageRow> = network
        .iter()
        .filter(|(hex, _)| !local.contains_key(*hex))
        .map(|(_, row)| row.clone())
        .collect();

    unique_rows.sort_by(|a, b| {
        a.seen
            .unwrap_or(f64::MAX)
            .total_cmp(&b.seen.unwrap_or(f64::MAX))
    });
    missed_rows.sort_by(|a, b| {
        a.seen
            .unwrap_or(f64::MAX)
            .total_cmp(&b.seen.unwrap_or(f64::MAX))
    });

    let age_secs = view.fetched_at.map(|t| (now - t).max(0.0));
    let stale = !view.ok || age_secs.map_or(true, |a| a > LIVE_WINDOW_SECS);
    Ok(NetworkCoverage {
        fetched_at: view.fetched_at,
        age_secs,
        ok: view.ok,
        error: view.error.clone(),
        stale,
        radius_nm: view.radius_nm,
        local_count: local.len(),
        network_count: network.len(),
        overlap,
        unique: unique_rows.len(),
        missed: missed_rows.len(),
        unique_rows,
        missed_rows,
    })
}

#[derive(Debug, Default, serde::Serialize)]
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
    pub description: Option<String>,
    /// True when `network_enrichment` contributed any field.
    pub network: bool,
}

fn nonblank(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn enrichment_for(db: &Db, hex: &str) -> rusqlite::Result<Option<Enrichment>> {
    let conn = crate::db_lock(db);
    let sky = conn
        .query_row(
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
                    description: None,
                    network: false,
                })
            },
        )
        .optional()?;

    let net: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = conn
        .query_row(
            "SELECT registration, typecode, description, owner, year
             FROM network_enrichment WHERE hex = ?1",
            params![hex],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;

    match (sky, net) {
        (None, None) => Ok(None),
        (Some(e), None) => Ok(Some(e)),
        (None, Some((registration, typecode, description, owner, year))) => {
            Ok(Some(Enrichment {
                registration: nonblank(registration),
                typecode: nonblank(typecode),
                description: nonblank(description),
                owner: nonblank(owner),
                built: nonblank(year),
                network: true,
                ..Default::default()
            }))
        }
        (Some(mut e), Some((registration, typecode, description, owner, year))) => {
            e.registration = nonblank(e.registration).or(nonblank(registration));
            e.typecode = nonblank(e.typecode).or(nonblank(typecode));
            e.owner = nonblank(e.owner).or(nonblank(owner));
            e.built = nonblank(e.built).or(nonblank(year));
            e.description = nonblank(description);
            e.network = true;
            Ok(Some(e))
        }
    }
}

/// One row of the type dex: a designator seen on the air, its ICAO Doc 8643
/// identity, the cached Wikipedia profile, and the local catch stats.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TypeDexRow {
    pub designator: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub engine_type: Option<String>,
    pub engine_count: Option<String>,
    pub wtc: Option<String>,
    pub category: Option<String>,
    pub wiki_title: Option<String>,
    pub wiki_extract: Option<String>,
    pub wiki_image: Option<String>,
    pub wiki_url: Option<String>,
    pub first_flight: Option<String>,
    pub total_produced: Option<String>,
    pub wingspan_m: Option<String>,
    pub caught: i64,
    pub registered: i64,
    pub first_seen: Option<f64>,
    pub last_seen: Option<f64>,
    pub sightings: i64,
    pub max_alt_ft: Option<i64>,
    pub max_gs_kts: Option<f64>,
    pub max_range_nm: Option<f64>,
}

/// Doc 8643 manufacturer codes are upper case; "BOEING 737-800" shouts.
pub(crate) fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) if word.chars().all(|c| !c.is_ascii_lowercase()) => {
                    format!("{}{}", first.to_ascii_uppercase(), chars.as_str().to_ascii_lowercase())
                }
                _ => word.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl TypeDexRow {
    // Accessors for the template's `if let Some(..)` patterns: matching a
    // borrowed `Option<String>` by value would move out of the row.
    pub fn image(&self) -> Option<&str> {
        self.wiki_image.as_deref()
    }
    pub fn extract(&self) -> Option<&str> {
        self.wiki_extract.as_deref()
    }
    pub fn wiki(&self) -> Option<&str> {
        self.wiki_url.as_deref()
    }
    pub fn model_label(&self) -> String {
        self.model.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| "-".into())
    }
    pub fn first_flight_str(&self) -> Option<&str> {
        self.first_flight.as_deref()
    }
    pub fn total_produced_str(&self) -> Option<&str> {
        self.total_produced.as_deref()
    }
    pub fn wingspan_str(&self) -> Option<&str> {
        self.wingspan_m.as_deref()
    }
    pub fn wtc_str(&self) -> Option<&str> {
        self.wtc.as_deref()
    }
    pub fn display_name(&self) -> String {
        // The Wikipedia title is the recognizable name ("Boeing 737 MAX",
        // "Bombardier CRJ700 series"); Doc 8643's canonical model can be a
        // military variant or an odd sub-type. Fall back to Doc 8643 when the
        // profile has not been fetched yet.
        if let Some(title) = self.wiki_title.as_deref().filter(|t| !t.trim().is_empty()) {
            return title.to_string();
        }
        match (self.manufacturer.as_deref(), self.model.as_deref()) {
            (Some(mf), Some(model)) if !mf.is_empty() && !model.is_empty() => {
                format!("{} {model}", title_case(mf))
            }
            (_, Some(model)) if !model.is_empty() => model.to_string(),
            _ => self.designator.clone(),
        }
    }
    fn fmt_day(epoch: Option<f64>) -> String {
        epoch
            .and_then(|ts| chrono::DateTime::from_timestamp(ts as i64, 0))
            .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".into())
    }
    pub fn first_seen_str(&self) -> String {
        Self::fmt_day(self.first_seen)
    }
    pub fn last_seen_str(&self) -> String {
        Self::fmt_day(self.last_seen)
    }
    /// Bar width for the card. A type with one airframe out of 5,000 still
    /// gets a visible sliver; a full bar means every registered airframe.
    pub fn pct(&self) -> i64 {
        if self.registered > 0 {
            ((self.caught * 100) / self.registered).clamp(1, 100)
        } else {
            100
        }
    }
    pub fn pct_str(&self) -> String {
        if self.registered > 0 {
            format!("{:.1}%", self.caught as f64 * 100.0 / self.registered as f64)
        } else {
            "-".into()
        }
    }
    /// Caught / registered, used for the "rarest" sort.
    pub fn rarity_rank(&self) -> f64 {
        if self.registered > 0 {
            self.caught as f64 / self.registered as f64
        } else {
            1.0
        }
    }
    pub fn engine_str(&self) -> String {
        match (self.engine_count.as_deref(), self.engine_type.as_deref()) {
            (Some(count), Some(kind)) if !count.is_empty() && !kind.is_empty() => {
                format!("{count} × {kind}")
            }
            (Some(count), _) if !count.is_empty() => count.to_string(),
            (_, Some(kind)) if !kind.is_empty() => kind.to_string(),
            _ => "-".into(),
        }
    }
    pub fn category_str(&self) -> String {
        match self.category.as_deref() {
            Some("LandPlane") => "Land plane".into(),
            Some("Helicopter") => "Helicopter".into(),
            Some("Amphibian") => "Amphibian".into(),
            Some(c) if !c.is_empty() => c.to_string(),
            _ => "-".into(),
        }
    }
}

/// The dex query: every designator heard on the air, joined to its cached
/// profile. The `registered` subquery is the rarity denominator — how many
/// airframes of this type exist in the OpenSky database, not just overhead.
const TYPE_DEX_SQL: &str = "WITH seen AS (
       SELECT e.typecode AS designator,
              COUNT(DISTINCT a.hex) AS caught,
              MIN(a.first_seen) AS first_seen,
              MAX(a.last_seen) AS last_seen,
              SUM(a.sightings) AS sightings,
              MAX(a.max_alt_ft) AS max_alt_ft,
              MAX(a.max_gs_kts) AS max_gs_kts,
              MAX(a.max_range_nm) AS max_range_nm
       FROM aircraft a CROSS JOIN enrichment e ON e.icao24 = a.hex
       WHERE e.typecode IS NOT NULL AND e.typecode <> ''
       GROUP BY e.typecode
     )
     SELECT s.designator, s.caught, s.first_seen, s.last_seen, s.sightings,
            s.max_alt_ft, s.max_gs_kts, s.max_range_nm,
            t.manufacturer, t.model, t.engine_type, t.engine_count, t.wtc, t.category,
            t.wiki_title, t.wiki_extract, t.wiki_image, t.wiki_url,
            t.first_flight, t.total_produced, t.wingspan_m,
            (SELECT COUNT(*) FROM enrichment e2 WHERE e2.typecode = s.designator) AS registered
     FROM seen s LEFT JOIN aircraft_types t ON t.designator = s.designator
     ORDER BY s.caught DESC, s.designator";

fn row_to_type(r: &Row) -> rusqlite::Result<TypeDexRow> {
    Ok(TypeDexRow {
        designator: r.get(0)?,
        caught: r.get(1)?,
        first_seen: r.get(2)?,
        last_seen: r.get(3)?,
        sightings: r.get(4)?,
        max_alt_ft: r.get(5)?,
        max_gs_kts: r.get(6)?,
        max_range_nm: r.get(7)?,
        manufacturer: r.get(8)?,
        model: r.get(9)?,
        engine_type: r.get(10)?,
        engine_count: r.get(11)?,
        wtc: r.get(12)?,
        category: r.get(13)?,
        wiki_title: r.get(14)?,
        wiki_extract: r.get(15)?,
        wiki_image: r.get(16)?,
        wiki_url: r.get(17)?,
        first_flight: r.get(18)?,
        total_produced: r.get(19)?,
        wingspan_m: r.get(20)?,
        registered: r.get(21)?,
    })
}

pub fn type_dex(db: &Db) -> rusqlite::Result<Vec<TypeDexRow>> {
    let conn = crate::db_lock(db);
    let mut stmt = conn.prepare(TYPE_DEX_SQL)?;
    let rows = stmt.query_map([], row_to_type)?;
    rows.collect()
}

#[derive(Debug, serde::Serialize)]
pub struct TypeAirframe {
    pub hex: String,
    pub registration: Option<String>,
    pub operator: Option<String>,
    pub owner: Option<String>,
    pub first_seen: f64,
    pub last_seen: f64,
    pub sightings: i64,
    pub max_alt_ft: Option<i64>,
    pub max_gs_kts: Option<f64>,
    pub max_range_nm: Option<f64>,
}

impl TypeAirframe {
    fn fmt_ts(epoch: f64) -> String {
        chrono::DateTime::from_timestamp(epoch as i64, 0)
            .expect("valid ts")
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    }
    pub fn first_seen_str(&self) -> String {
        Self::fmt_ts(self.first_seen)
    }
    pub fn last_seen_str(&self) -> String {
        Self::fmt_ts(self.last_seen)
    }
    pub fn operator_str(&self) -> String {
        self.operator
            .as_deref()
            .or(self.owner.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or("-")
            .to_string()
    }
}

#[derive(Debug, serde::Serialize)]
pub struct TypeOperator {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct TypeDetail {
    pub row: TypeDexRow,
    pub airframes: Vec<TypeAirframe>,
    pub operators: Vec<TypeOperator>,
}

pub fn type_detail(db: &Db, code: &str) -> rusqlite::Result<Option<TypeDetail>> {
    let Some(row) = type_dex(db)?.into_iter().find(|r| r.designator == code) else {
        return Ok(None);
    };
    let conn = crate::db_lock(db);
    let mut stmt = conn.prepare(
        "SELECT a.hex, e.registration, e.operator, e.owner,
                a.first_seen, a.last_seen, a.sightings,
                a.max_alt_ft, a.max_gs_kts, a.max_range_nm
         FROM aircraft a CROSS JOIN enrichment e ON e.icao24 = a.hex
         WHERE e.typecode = ?1 ORDER BY a.last_seen DESC",
    )?;
    let airframes = stmt
        .query_map(params![code], |r| {
            Ok(TypeAirframe {
                hex: r.get(0)?,
                registration: r.get(1)?,
                operator: r.get(2)?,
                owner: r.get(3)?,
                first_seen: r.get(4)?,
                last_seen: r.get(5)?,
                sightings: r.get(6)?,
                max_alt_ft: r.get(7)?,
                max_gs_kts: r.get(8)?,
                max_range_nm: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(NULLIF(e.operator, ''), NULLIF(e.owner, '')) AS name,
                COUNT(DISTINCT a.hex) AS n
         FROM aircraft a CROSS JOIN enrichment e ON e.icao24 = a.hex
         WHERE e.typecode = ?1
           AND COALESCE(NULLIF(e.operator, ''), NULLIF(e.owner, '')) IS NOT NULL
         GROUP BY 1 ORDER BY 2 DESC, 1 LIMIT 6",
    )?;
    let operators = stmt
        .query_map(params![code], |r| Ok(TypeOperator { name: r.get(0)?, count: r.get(1)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(TypeDetail { row, airframes, operators }))
}

#[derive(Debug, serde::Serialize)]
pub struct TypeDexSummary {
    /// Distinct designators heard.
    pub types: i64,
    /// Distinct airframes heard.
    pub airframes: i64,
    /// Designators in the cached ICAO Doc 8643 table.
    pub designators: i64,
}

impl TypeDexSummary {
    /// Dex completion: caught types out of every ICAO designator. Small by
    /// design — the point of the number is how much is still out there.
    pub fn pct_str(&self) -> String {
        if self.designators > 0 {
            format!("{:.1}%", self.types as f64 * 100.0 / self.designators as f64)
        } else {
            "-".into()
        }
    }
}

pub fn type_dex_summary(db: &Db) -> rusqlite::Result<TypeDexSummary> {
    let conn = crate::db_lock(db);
    let types = conn.query_row(
        "SELECT COUNT(DISTINCT e.typecode) FROM aircraft a
         CROSS JOIN enrichment e ON e.icao24 = a.hex
         WHERE e.typecode IS NOT NULL AND e.typecode <> ''",
        [],
        |r| r.get(0),
    )?;
    let airframes = conn.query_row("SELECT COUNT(*) FROM aircraft", [], |r| r.get(0))?;
    let designators = conn.query_row(
        "SELECT COUNT(*) FROM aircraft_types WHERE model IS NOT NULL AND model <> ''",
        [],
        |r| r.get(0),
    )?;
    Ok(TypeDexSummary { types, airframes, designators })
}

#[cfg(test)]
mod tests {
    use super::PollerHealth;

    /// The pill's three states are what tells the operator the receiver died;
    /// the boundaries are the whole contract.
    #[test]
    fn poller_health_states() {
        assert_eq!(PollerHealth::new(None).state(), "down");
        assert_eq!(PollerHealth::new(Some(0.0)).state(), "live");
        assert_eq!(PollerHealth::new(Some(29.9)).state(), "live");
        assert_eq!(PollerHealth::new(Some(30.0)).state(), "stale");
        assert_eq!(PollerHealth::new(Some(299.0)).state(), "stale");
        assert_eq!(PollerHealth::new(Some(300.0)).state(), "down");
        assert_eq!(PollerHealth::new(Some(7200.0)).state(), "down");
    }

    #[test]
    fn poller_health_only_alerts_when_not_live() {
        assert_eq!(PollerHealth::new(Some(4.0)).alert(), None);
        assert!(PollerHealth::new(Some(90.0)).alert().unwrap().starts_with("Receiver stalled"));
        assert!(PollerHealth::new(Some(7200.0)).alert().unwrap().starts_with("Poller down"));
        assert!(PollerHealth::new(None).alert().is_some());
    }

    /// The live panel is empty both when the sky is clear and when the receiver
    /// is dead. Only the poller's health tells them apart, and the panel must
    /// never let a dead receiver read as clear skies.
    #[test]
    fn live_panel_never_claims_clear_skies_when_the_poller_is_not_live() {
        assert_eq!(PollerHealth::new(Some(4.0)).sentence(), None);
        for age in [Some(90.0), Some(7200.0), None] {
            let s = PollerHealth::new(age).sentence().expect("a sentence when not live");
            assert!(s.contains("not clear skies"), "{s}");
        }
    }

    /// `min_baro_rate` is stored as the most negative climb, so the descent
    /// row has to flip the sign: the label already says "descent" and
    /// `-1408` under it reads as a double negative.
    #[test]
    fn max_descent_reads_as_a_positive_rate() {
        assert_eq!(super::descent_str(Some(-1408)), "1408");
        assert_eq!(super::descent_str(Some(0)), "0");
        assert_eq!(super::descent_str(None), "-");
    }

    /// OpenSky's `typecode` is free text. A row must link only when the
    /// `/type/:code` route would accept the code, or the dashboard ships
    /// dead links.
    #[test]
    fn type_links_only_where_the_dex_route_accepts_the_code() {
        for ok in ["B738", "C172", "A20N", "E75L"] {
            assert!(super::valid_designator(ok), "{ok}");
        }
        for bad in ["", "B", "B7380", "B 38", "B-38", "B738!"] {
            assert!(!super::valid_designator(bad), "{bad}");
        }
        let valid = super::Record { hex: "B738".into(), ..Default::default() };
        assert_eq!(valid.type_url().as_deref(), Some("/type/B738"));
        let invalid = super::Record { hex: "B7380".into(), ..Default::default() };
        assert_eq!(invalid.type_url(), None);
    }
}
