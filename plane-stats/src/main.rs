use std::sync::{Arc, Mutex};

use rusqlite::Connection;

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
           min_rssi_dbfs REAL
         );
         CREATE TABLE IF NOT EXISTS poll_log (
           ts REAL PRIMARY KEY, aircraft_count INTEGER, messages INTEGER
         );",
    )
    .expect("run migrations");

    let db: Db = Arc::new(Mutex::new(conn));

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
