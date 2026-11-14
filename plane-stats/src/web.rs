use askama::Template;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
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
        .route("/partials/table", get(table_partial))
        .route("/partials/cards", get(cards_partial))
        .route("/api/aircraft", get(api_aircraft))
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
    sort: String,
    dir: String,
}

async fn index(State(db): State<Db>, Query(q): Query<SortParams>) -> Result<Html<String>, AppError> {
    let sort = if q.sort.is_empty() { "last_seen".to_string() } else { q.sort.clone() };
    let dir = if stats::is_valid_dir(&q.dir) {
        q.dir.clone()
    } else {
        stats::sort_column(&sort).1.to_string()
    };
    let rows = stats::aircraft_sorted(&db, &sort, &dir)?;
    let st = stats::stats(&db, now())?;
    let recent = stats::recent_arrivals(&db)?;
    Ok(Html(
        IndexTemplate { stats: st, rows, recent, sort, dir }
            .render()?,
    ))
}

#[derive(Deserialize, Default)]
struct SortParams {
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
}
#[derive(Template)]
#[template(path = "partials/table.html")]
struct TableTemplate {
    rows: Vec<AircraftRow>,
    sort: String,
    dir: String,
}

async fn table_partial(
    State(db): State<Db>,
    Query(q): Query<SortParams>,
) -> Result<Html<String>, AppError> {
    let rows = stats::aircraft_sorted(&db, &q.sort, &q.dir)?;
    let sort = if q.sort.is_empty() { "last_seen".to_string() } else { q.sort.clone() };
    let dir = if stats::is_valid_dir(&q.dir) {
        q.dir
    } else {
        stats::sort_column(&sort).1.to_string()
    };
    Ok(Html(TableTemplate { rows, sort, dir }.render()?))
}

#[derive(Template)]
#[template(path = "partials/cards.html")]
struct CardsTemplate {
    stats: Stats,
}

async fn cards_partial(State(db): State<Db>) -> Result<Html<String>, AppError> {
    Ok(Html(CardsTemplate { stats: stats::stats(&db, now())? }.render()?))
}

async fn api_aircraft(State(db): State<Db>) -> Result<Json, AppError> {
    let rows = stats::aircraft_sorted(&db, "last_seen", "desc")?;
    Ok(Json(serde_json::to_value(rows)?))
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
