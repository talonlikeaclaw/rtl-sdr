use askama::Template;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::stats::{self, AircraftRow, Stats};
use crate::Db;

const HTMX: &[u8] = include_bytes!("../static/htmx.min.js");
const APP_CSS: &[u8] = include_bytes!("../static/app.css");

pub fn router(db: Db) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/aircraft/:hex", get(aircraft_profile))
        .route("/api/aircraft/:hex", get(api_aircraft_hex))
        .route("/partials/table", get(table_partial))
        .route("/partials/cards", get(cards_partial))
        .route("/partials/emergencies", get(emergencies_partial))
        .route("/partials/live", get(live_partial))
        .route("/api/aircraft", get(api_aircraft))
        .route("/api/emergencies", get(api_emergencies))
        .route("/api/live", get(api_live))
        .route("/api/stats", get(api_stats))
        .route("/htmx.min.js", get(htmx_js))
        .route("/app.css", get(app_css))
        .with_state(db)
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs_f64()
}

/// Base URL of the tar1090 instance, as seen by the browser.
///
/// `TAR1090_URL` wins when set (reverse proxy, https, different host). Otherwise
/// derive it from the request's `Host` header, swapping the port to 8080, so a
/// page opened at `pi.local:8081` links to `pi.local:8080`.
fn tar1090_base(headers: &HeaderMap) -> String {
    if let Ok(url) = std::env::var("TAR1090_URL") {
        let url = url.trim();
        if !url.is_empty() {
            return url.trim_end_matches('/').to_string();
        }
    }
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("localhost");
    format!("http://{}:8080", strip_port(host))
}

/// `host:port` -> `host`, keeping IPv6 literals intact (`[::1]:8081` -> `[::1]`).
fn strip_port(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        return match rest.find(']') {
            Some(end) => &host[..end + 2],
            None => host,
        };
    }
    match host.rsplit_once(':') {
        Some((h, port)) if !h.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    }
}

fn ts(epoch: f64) -> String {
    chrono::DateTime::from_timestamp(epoch as i64, 0).expect("valid ts").with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    stats: Stats,
    rows: Vec<AircraftRow>,
    recent: Vec<AircraftRow>,
    emergencies: Vec<stats::EmergencyRow>,
    sort: String,
    dir: String,
    q: String,
    tar1090: String,
    now: f64,
    rx: Option<(f64, f64)>,
}

async fn index(
    State(db): State<Db>,
    Query(q): Query<SortParams>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let sort = if q.sort.is_empty() { "last_seen".to_string() } else { q.sort.clone() };
    let dir = if stats::is_valid_dir(&q.dir) {
        q.dir.clone()
    } else {
        stats::sort_column(&sort).1.to_string()
    };
    let rows = stats::aircraft_sorted(&db, &sort, &dir, &q.q)?;
    let now = now();
    let st = stats::stats(&db, now)?;
    let recent = stats::recent_arrivals(&db)?;
    let emergencies = stats::emergency_log(&db, 20)?;
    Ok(Html(
        IndexTemplate {
            stats: st,
            rows,
            recent,
            emergencies,
            sort,
            dir,
            q: q.q.clone(),
            tar1090: tar1090_base(&headers),
            now,
            rx: stats::rx_pos(),
        }
        .render()?,
    ))
}

#[derive(Deserialize, Default)]
struct SortParams {
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
    #[serde(default)]
    q: String,
}
#[derive(Template)]
#[template(path = "partials/table.html")]
struct TableTemplate {
    rows: Vec<AircraftRow>,
    sort: String,
    dir: String,
    q: String,
    tar1090: String,
    now: f64,
}

async fn table_partial(
    State(db): State<Db>,
    Query(q): Query<SortParams>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let rows = stats::aircraft_sorted(&db, &q.sort, &q.dir, &q.q)?;
    let sort = if q.sort.is_empty() { "last_seen".to_string() } else { q.sort.clone() };
    let dir = if stats::is_valid_dir(&q.dir) {
        q.dir
    } else {
        stats::sort_column(&sort).1.to_string()
    };
    Ok(Html(
        TableTemplate { rows, sort, dir, q: q.q, tar1090: tar1090_base(&headers), now: now() }.render()?,
    ))
}

#[derive(Template)]
#[template(path = "partials/cards.html")]
struct CardsTemplate {
    stats: Stats,
}

async fn cards_partial(State(db): State<Db>) -> Result<Html<String>, AppError> {
    Ok(Html(CardsTemplate { stats: stats::stats(&db, now())? }.render()?))
}

#[derive(Template)]
#[template(path = "partials/emergencies.html")]
struct EmergenciesTemplate {
    emergencies: Vec<stats::EmergencyRow>,
}

async fn emergencies_partial(State(db): State<Db>) -> Result<Html<String>, AppError> {
    Ok(Html(EmergenciesTemplate { emergencies: stats::emergency_log(&db, 20)? }.render()?))
}

#[derive(Template)]
#[template(path = "partials/live.html")]
struct LiveTemplate {
    rows: Vec<AircraftRow>,
    tar1090: String,
    rx: Option<(f64, f64)>,
}

async fn live_partial(State(db): State<Db>, headers: HeaderMap) -> Result<Html<String>, AppError> {
    let now = now();
    Ok(Html(
        LiveTemplate {
            rows: stats::live_aircraft(&db, now)?,
            tar1090: tar1090_base(&headers),
            rx: stats::rx_pos(),
        }
        .render()?,
    ))
}

async fn api_live(State(db): State<Db>) -> Result<Json, AppError> {
    Ok(Json(serde_json::to_value(stats::live_aircraft(&db, now())?)?))
}

#[derive(Deserialize, Default)]
struct LimitParams {
    #[serde(default)]
    limit: Option<u32>,
}

async fn api_emergencies(
    State(db): State<Db>,
    Query(q): Query<LimitParams>,
) -> Result<Json, AppError> {
    let limit = q.limit.unwrap_or(20).clamp(1, 500);
    Ok(Json(serde_json::to_value(stats::emergency_log(&db, limit)?)?))
}

async fn api_aircraft(State(db): State<Db>, Query(q): Query<SortParams>) -> Result<Json, AppError> {
    let rows = stats::aircraft_sorted(&db, &q.sort, &q.dir, &q.q)?;
    Ok(Json(serde_json::to_value(rows)?))
}

#[derive(Template)]
#[template(path = "aircraft.html")]
struct AircraftTemplate {
    row: AircraftRow,
    enrich: Option<stats::Enrichment>,
    callsigns: Vec<stats::CallsignRow>,
    visits: Vec<VisitView>,
    tar1090: String,
    now: f64,
}

struct VisitView {
    start_ts: f64,
    end_ts: f64,
    polls: i64,
    duration: String,
}

impl VisitView {
    fn fmt_ts(epoch: f64) -> String {
        chrono::DateTime::from_timestamp(epoch as i64, 0).expect("valid ts").with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }
    pub fn start_str(&self) -> String {
        Self::fmt_ts(self.start_ts)
    }
    pub fn end_str(&self) -> String {
        Self::fmt_ts(self.end_ts)
    }
}

fn fmt_duration(secs: f64) -> String {
    let total_min = (secs / 60.0).round() as i64;
    let h = total_min / 60;
    let m = total_min % 60;
    if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") }
}

async fn aircraft_profile(
    State(db): State<Db>,
    axum::extract::Path(hex): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let hex = hex.to_lowercase();
    let Some(row) = stats::aircraft_by_hex(&db, &hex)? else {
        return Err(AppError(StatusCode::NOT_FOUND, format!("unknown aircraft {hex}")));
    };
    let enrich = stats::enrichment_for(&db, &hex)?;
    let callsigns = stats::callsigns_for(&db, &hex)?;
    let visits = stats::visits_for(&db, &hex)?
        .into_iter()
        .map(|v| VisitView {
            start_ts: v.start_ts,
            end_ts: v.end_ts,
            polls: v.polls,
            duration: fmt_duration(v.end_ts - v.start_ts),
        })
        .collect();
    Ok(Html(
        AircraftTemplate {
            row,
            enrich,
            callsigns,
            visits,
            tar1090: tar1090_base(&headers),
            now: now(),
        }
        .render()?,
    ))
}

async fn api_aircraft_hex(
    State(db): State<Db>,
    axum::extract::Path(hex): axum::extract::Path<String>,
) -> Result<Json, AppError> {
    let hex = hex.to_lowercase();
    let Some(row) = stats::aircraft_by_hex(&db, &hex)? else {
        return Err(AppError(StatusCode::NOT_FOUND, format!("unknown aircraft {hex}")));
    };
    let enrich = stats::enrichment_for(&db, &hex)?;
    let callsigns = stats::callsigns_for(&db, &hex)?;
    let visits = stats::visits_for(&db, &hex)?;
    Ok(Json(serde_json::json!({
        "aircraft": row,
        "enrichment": enrich,
        "callsigns": callsigns,
        "visits": visits,
    })))
}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        tracing::error!("db: {e}");
        AppError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}
impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        tracing::error!("template: {e}");
        AppError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        tracing::error!("json: {e}");
        AppError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}
async fn api_stats(State(db): State<Db>) -> Result<Json, AppError> {
    let mut v = serde_json::to_value(stats::stats(&db, now())?)?;
    v["now"] = serde_json::json!(now());
    Ok(Json(v))
}


struct Json(serde_json::Value);
impl IntoResponse for Json {
    fn into_response(self) -> Response {
        ([(header::CONTENT_TYPE, "application/json")], self.0.to_string()).into_response()
    }
}

async fn htmx_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript")], HTMX.to_vec())
}

async fn app_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], APP_CSS.to_vec())
}

struct AppError(StatusCode, String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}
