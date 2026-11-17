use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension};

mod dbflags;
mod enrichment;
mod mil;
mod network;
mod poller;
mod stats;
mod types;
mod web;

pub type Db = Arc<Mutex<Connection>>;

/// Lock the shared connection, recovering from a poisoned mutex instead of
/// panicking: one panic must not take every later DB read down with it.
pub fn db_lock(db: &Db) -> std::sync::MutexGuard<'_, Connection> {
    db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "plane_stats=info".into()),
        )
        .init();

    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "/data/planes.db".to_string());
    if let Some(dir) = std::path::Path::new(&db_path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.pragma_update(None, "journal_mode", "WAL").expect("WAL");
    conn.pragma_update(None, "synchronous", "NORMAL").expect("synchronous");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS aircraft (
           hex TEXT PRIMARY KEY,
           first_seen REAL NOT NULL,
           last_seen  REAL NOT NULL,
           sightings INTEGER NOT NULL DEFAULT 0,
           last_flight TEXT,
           last_squawk TEXT,
           last_category TEXT,
           max_alt_ft  INTEGER,
           max_gs_kts  REAL,
           max_range_nm REAL,
           max_rssi_dbfs REAL,
           last_lat REAL,
           last_lon REAL,
           last_track REAL,
           max_baro_rate INTEGER,
           min_baro_rate INTEGER,
           last_nav_alt_ft INTEGER,
           last_nav_heading REAL,
           last_emergency TEXT
         );
         CREATE TABLE IF NOT EXISTS poll_log (
           ts REAL PRIMARY KEY, aircraft_count INTEGER, messages INTEGER
         );
         CREATE TABLE IF NOT EXISTS visits (
           hex TEXT NOT NULL,
           start_ts REAL NOT NULL,
           end_ts REAL NOT NULL,
           polls INTEGER NOT NULL DEFAULT 1,
           PRIMARY KEY (hex, start_ts)
         );
         CREATE TABLE IF NOT EXISTS aircraft_callsigns (
           hex TEXT NOT NULL,
           callsign TEXT NOT NULL,
           first_seen REAL NOT NULL,
           last_seen REAL NOT NULL,
           sightings INTEGER NOT NULL DEFAULT 1,
           PRIMARY KEY (hex, callsign)
         );
         CREATE TABLE IF NOT EXISTS enrichment (
           icao24 TEXT PRIMARY KEY,
           registration TEXT,
           manufacturer TEXT,
           model TEXT,
           typecode TEXT,
           owner TEXT,
           operator TEXT
         );
         CREATE TABLE IF NOT EXISTS enrichment_meta (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS emergency_log (
           ts REAL NOT NULL,
           hex TEXT NOT NULL,
           squawk TEXT,
           emergency TEXT NOT NULL,
           PRIMARY KEY (ts, hex)
         );
         CREATE TABLE IF NOT EXISTS poll_hour (
           ts INTEGER PRIMARY KEY,
           max_count INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS day_aircraft (
           day TEXT NOT NULL,
           hex TEXT NOT NULL,
           first_ts REAL NOT NULL,
           last_ts  REAL NOT NULL,
           polls INTEGER NOT NULL DEFAULT 1,
           max_alt_ft INTEGER,
           max_gs_kts REAL,
           max_range_nm REAL,
           PRIMARY KEY (day, hex)
         );
         CREATE TABLE IF NOT EXISTS aircraft_types (
           designator TEXT PRIMARY KEY,
           manufacturer TEXT,
           model TEXT,
           engine_type TEXT,
           engine_count TEXT,
           wtc TEXT,
           category TEXT,
           wiki_title TEXT,
           wiki_extract TEXT,
           wiki_image TEXT,
           wiki_url TEXT,
           first_flight TEXT,
           total_produced TEXT,
           wingspan_m TEXT,
           wiki_fetched_at REAL
         );
         CREATE TABLE IF NOT EXISTS type_wiki_map (
           designator TEXT PRIMARY KEY,
           title TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS network_day_aircraft (
           day TEXT NOT NULL,
           hex TEXT NOT NULL,
           first_ts REAL NOT NULL,
           last_ts  REAL NOT NULL,
           polls INTEGER NOT NULL DEFAULT 1,
           max_alt_ft INTEGER,
           max_gs_kts REAL,
           max_range_nm REAL,
           mlat_pos INTEGER NOT NULL DEFAULT 0,
           PRIMARY KEY (day, hex)
         );
         CREATE TABLE IF NOT EXISTS network_enrichment (
           hex TEXT PRIMARY KEY,
           registration TEXT,
           typecode TEXT,
           description TEXT,
           owner TEXT,
           year TEXT,
           updated_at REAL NOT NULL
         );",
    )
    .expect("run migrations");

    // Add columns introduced after the initial schema; idempotent.
    const NEW_COLUMNS: &[(&str, &str, &str)] = &[
        ("aircraft", "last_lat", "REAL"),
        ("aircraft", "last_lon", "REAL"),
        ("aircraft", "last_track", "REAL"),
        ("aircraft", "max_rssi_dbfs", "REAL"),
        ("aircraft", "max_baro_rate", "INTEGER"),
        ("aircraft", "min_baro_rate", "INTEGER"),
        ("aircraft", "last_nav_alt_ft", "INTEGER"),
        ("aircraft", "last_nav_heading", "REAL"),
        ("aircraft", "last_emergency", "TEXT"),
        ("enrichment", "serialnumber", "TEXT"),
        ("enrichment", "icaoaircrafttype", "TEXT"),
        ("enrichment", "operatorcallsign", "TEXT"),
        ("enrichment", "registered", "TEXT"),
        ("enrichment", "reguntil", "TEXT"),
        ("enrichment", "status", "TEXT"),
        ("enrichment", "built", "TEXT"),
        ("enrichment", "firstflightdate", "TEXT"),
        ("enrichment", "engines", "TEXT"),
        ("enrichment", "notes", "TEXT"),
        ("enrichment", "categorydescription", "TEXT"),
        ("aircraft", "last_alt_ft", "INTEGER"),
        ("aircraft", "last_gs_kts", "REAL"),
        ("aircraft", "last_range_nm", "REAL"),
        ("aircraft", "last_baro_rate", "REAL"),
        ("aircraft", "last_msg_age", "REAL"),
        ("aircraft", "db_flags", "INTEGER"),
    ];
    for (table, col, ddl) in NEW_COLUMNS {
        if let Err(e) = conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} {ddl}"), []) {
            if !e.to_string().contains("duplicate column name") {
                panic!("migration failed for {col}: {e}");
            }
        }
    }

    // `min_rssi_dbfs` stored the weakest reading but was shown as the strongest.
    // It is replaced by `max_rssi_dbfs`; drop the stale column when present.
    if let Err(e) = conn.execute("ALTER TABLE aircraft DROP COLUMN min_rssi_dbfs", []) {
        if !e.to_string().contains("no such column") {
            panic!("migration failed dropping min_rssi_dbfs: {e}");
        }
    }

    // The fleet table sorts by any of these columns; without an index every
    // page is a full scan plus sort of the whole table.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS aircraft_last_seen  ON aircraft(last_seen DESC, hex);
         CREATE INDEX IF NOT EXISTS aircraft_first_seen ON aircraft(first_seen DESC, hex);
         CREATE INDEX IF NOT EXISTS aircraft_sightings  ON aircraft(sightings DESC, hex);
         CREATE INDEX IF NOT EXISTS aircraft_max_alt    ON aircraft(max_alt_ft DESC, hex);
         CREATE INDEX IF NOT EXISTS aircraft_max_gs     ON aircraft(max_gs_kts DESC, hex);
         CREATE INDEX IF NOT EXISTS aircraft_max_range  ON aircraft(max_range_nm DESC, hex);
         CREATE INDEX IF NOT EXISTS aircraft_max_rssi   ON aircraft(max_rssi_dbfs DESC, hex);
         CREATE INDEX IF NOT EXISTS emergency_log_hex   ON emergency_log(hex);
         CREATE INDEX IF NOT EXISTS day_aircraft_hex    ON day_aircraft(hex);
         CREATE INDEX IF NOT EXISTS enrichment_typecode ON enrichment(typecode);
         CREATE INDEX IF NOT EXISTS network_day_aircraft_hex ON network_day_aircraft(hex);",
    )
    .expect("create indexes");

    // `poll_hour` is the hourly rollup the cards read instead of grouping the
    // whole `poll_log` every 10 s. Fill it once from the existing log.
    let backfilled = conn
        .query_row("SELECT 1 FROM enrichment_meta WHERE key = 'poll_hour_v1'", [], |_| Ok(()))
        .optional()
        .expect("poll_hour meta")
        .is_some();
    if !backfilled {
        conn.execute_batch(
            "DELETE FROM poll_hour;
             INSERT INTO poll_hour (ts, max_count)
               SELECT CAST(ts / 3600 AS INTEGER) * 3600, MAX(aircraft_count) FROM poll_log GROUP BY 1;
             INSERT OR REPLACE INTO enrichment_meta (key, value) VALUES ('poll_hour_v1', '1');",
        )
        .expect("backfill poll_hour");
    }

    // The readsb image never emits `dbFlags` (it runs without `--db-file`), so
    // the military bit is derived from ICAO ranges in the poller. Backfill the
    // rows recorded before that fallback existed; bump the key if `mil::RANGES`
    // changes.
    let mil_backfilled = conn
        .query_row("SELECT 1 FROM enrichment_meta WHERE key = 'mil_ranges_v1'", [], |_| Ok(()))
        .optional()
        .expect("mil_ranges meta")
        .is_some();
    if !mil_backfilled {
        let tx = conn.unchecked_transaction().expect("mil backfill tx");
        for &(lo, hi) in mil::RANGES {
            tx.execute(
                "UPDATE aircraft SET db_flags = coalesce(db_flags, 0) | 1
                 WHERE hex BETWEEN ?1 AND ?2",
                rusqlite::params![format!("{lo:06x}"), format!("{hi:06x}")],
            )
            .expect("backfill military flags");
        }
        tx.execute(
            "INSERT OR REPLACE INTO enrichment_meta (key, value) VALUES ('mil_ranges_v1', '1')",
            [],
        )
        .expect("mil_ranges meta");
        tx.commit().expect("mil backfill commit");
    }

    let db: Db = Arc::new(Mutex::new(conn));

    match (
        std::env::var("RX_LAT").ok().and_then(|v| v.trim().parse::<f64>().ok()),
        std::env::var("RX_LON").ok().and_then(|v| v.trim().parse::<f64>().ok()),
    ) {
        (Some(lat), Some(lon)) => stats::set_rx_pos(lat, lon),
        _ => tracing::warn!("RX_LAT/RX_LON not set; bearing column disabled"),
    }

    match enrichment::load(&db) {
        Ok(enrichment::Outcome::Loaded(n)) => tracing::info!("loaded {n} enrichment rows"),
        Ok(enrichment::Outcome::Unchanged(n)) => {
            tracing::info!("enrichment unchanged ({n} rows), skipping reload")
        }
        Err(e) => tracing::warn!("enrichment load failed: {e}"),
    }

    // The live readsb only flags aircraft it hears after loading its DB, so
    // backfill military/interesting/PIA/LADD for rows already in the ledger.
    match dbflags::backfill(&db) {
        Ok(0) => {}
        Ok(n) => tracing::info!("tar1090 flags backfilled on {n} aircraft"),
        Err(e) => tracing::warn!("tar1090 flag backfill failed: {e}"),
    }

    // The 520k-row enrichment insert leaves a WAL as large as the database.
    if let Err(e) = crate::db_lock(&db).execute_batch("PRAGMA wal_checkpoint(TRUNCATE)") {
        tracing::warn!("wal checkpoint failed: {e}");
    }

    let source =
        std::env::var("AIRCRAFT_JSON_URL").unwrap_or_else(|_| "http://tar1090:80/data/aircraft.json".to_string());

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(async move {
        tokio::spawn(poller::run(db.clone(), source));
        tokio::spawn(network::run(db.clone()));
        // Doc 8643 + Wikipedia profiles are cached in SQLite; the task never
        // blocks startup and every page renders without it.
        tokio::spawn(types::run(db.clone()));

        let app = web::router(db);
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8081").await.expect("bind 8081");
        tracing::info!("listening on 0.0.0.0:8081");
        axum::serve(listener, app).await.expect("serve");
    });
}
