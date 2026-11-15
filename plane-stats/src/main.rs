use std::sync::{Arc, Mutex};

use rusqlite::Connection;

mod enrichment;
mod poller;
mod stats;
mod web;

pub type Db = Arc<Mutex<Connection>>;

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

    let db: Db = Arc::new(Mutex::new(conn));

    match enrichment::load(&db) {
        Ok(enrichment::Outcome::Loaded(n)) => tracing::info!("loaded {n} enrichment rows"),
        Ok(enrichment::Outcome::Unchanged(n)) => {
            tracing::info!("enrichment unchanged ({n} rows), skipping reload")
        }
        Err(e) => tracing::warn!("enrichment load failed: {e}"),
    }

    let source =
        std::env::var("AIRCRAFT_JSON_URL").unwrap_or_else(|_| "http://tar1090:80/data/aircraft.json".to_string());

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(async move {
        tokio::spawn(poller::run(db.clone(), source));

        let app = web::router(db);
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8081").await.expect("bind 8081");
        tracing::info!("listening on 0.0.0.0:8081");
        axum::serve(listener, app).await.expect("serve");
    });
}
