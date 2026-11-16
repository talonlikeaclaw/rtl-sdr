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
const APP_JS: &[u8] = include_bytes!("../static/app.js");

pub fn router(db: Db) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/today", get(today_page))
        .route("/activity", get(activity_page))
        .route("/aircraft/:hex", get(aircraft_profile))
        .route("/api/aircraft/:hex", get(api_aircraft_hex))
        .route("/partials/table", get(table_partial))
        .route("/partials/table/rows", get(table_rows_partial))
        .route("/partials/cards", get(cards_partial))
        .route("/partials/emergencies", get(emergencies_partial))
        .route("/partials/live", get(live_partial))
        .route("/api/aircraft", get(api_aircraft))
        .route("/api/emergencies", get(api_emergencies))
        .route("/api/live", get(api_live))
        .route("/api/today", get(api_today))
        .route("/api/activity", get(api_activity))
        .route("/api/stats", get(api_stats))
        .route("/htmx.min.js", get(htmx_js))
        .route("/app.css", get(app_css))
        .route("/app.js", get(app_js))
        .with_state(db)
        // The fleet table and `/api/aircraft` are a few hundred KB of markup.
        .layer(tower_http::compression::CompressionLayer::new())
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
    live: Vec<AircraftRow>,
    recent: Vec<AircraftRow>,
    emergencies: Vec<stats::EmergencyRow>,
    sort: String,
    dir: String,
    q: String,
    filter: stats::FleetFilter,
    counts: stats::FilterCounts,
    poller: stats::PollerHealth,
    tar1090: String,
    now: f64,
    rx: Option<(f64, f64)>,
    page: u32,
    pages: u32,
    total: i64,
    from_row: u32,
    to_row: u32,
    prev_page: Option<u32>,
    next_page: Option<u32>,
}

/// Pager state for the all-time fleet table.
struct PageNav {
    page: u32,
    pages: u32,
    total: i64,
    from_row: u32,
    to_row: u32,
    prev_page: Option<u32>,
    next_page: Option<u32>,
}

/// Clamp the requested page into range and derive everything the pager shows.
fn paginate(
    db: &Db,
    requested: Option<u32>,
    filter: &stats::FleetFilter,
    now: f64,
) -> rusqlite::Result<PageNav> {
    let total = stats::aircraft_count(db, filter, now)?;
    let size = u64::from(stats::PAGE_SIZE);
    let pages = ((total.max(0) as u64).div_ceil(size)).max(1) as u32;
    let page = requested.unwrap_or(1).clamp(1, pages);
    let (from_row, to_row) = if total > 0 {
        let from = (page - 1) * stats::PAGE_SIZE + 1;
        (from, (u64::from(from) + size - 1).min(total as u64) as u32)
    } else {
        (0, 0)
    };
    Ok(PageNav {
        page,
        pages,
        total,
        from_row,
        to_row,
        prev_page: (page > 1).then_some(page - 1),
        next_page: (page < pages).then_some(page + 1),
    })
}

/// Everything the fleet table needs, resolved once for both the page and the
/// partial endpoints.
struct FleetView {
    nav: PageNav,
    rows: Vec<AircraftRow>,
    sort: String,
    dir: String,
    filter: stats::FleetFilter,
    counts: stats::FilterCounts,
}

fn fleet_view(db: &Db, q: &SortParams, now: f64) -> Result<FleetView, AppError> {
    let sort = if stats::is_valid_sort(&q.sort) { q.sort.clone() } else { "last_seen".to_string() };
    let dir = if stats::is_valid_dir(&q.dir) {
        q.dir.clone()
    } else {
        stats::sort_column(&sort).1.to_string()
    };
    let filter = q.filter();
    let nav = paginate(db, q.page, &filter, now)?;
    let rows = stats::aircraft_sorted(
        db,
        &sort,
        &dir,
        &filter,
        stats::PAGE_SIZE,
        (nav.page - 1) * stats::PAGE_SIZE,
        now,
    )?;
    let counts = stats::filter_counts(db, now)?;
    Ok(FleetView { nav, rows, sort, dir, filter, counts })
}

async fn index(
    State(db): State<Db>,
    Query(q): Query<SortParams>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let now = now();
    let view = fleet_view(&db, &q, now)?;
    let st = stats::stats_cached(&db, now)?;
    let live = stats::live_aircraft(&db, now)?;
    let recent = stats::recent_arrivals(&db)?;
    let emergencies = stats::emergency_log(&db, 20)?;
    let poller = stats::PollerHealth::new(stats::last_poll_age(&db, now)?);
    Ok(Html(
        IndexTemplate {
            stats: st,
            rows: view.rows,
            live,
            recent,
            emergencies,
            sort: view.sort,
            dir: view.dir,
            q: q.q.clone(),
            filter: view.filter,
            counts: view.counts,
            poller,
            tar1090: tar1090_base(&headers),
            now,
            rx: stats::rx_pos(),
            page: view.nav.page,
            pages: view.nav.pages,
            total: view.nav.total,
            from_row: view.nav.from_row,
            to_row: view.nav.to_row,
            prev_page: view.nav.prev_page,
            next_page: view.nav.next_page,
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
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    cat: String,
    #[serde(default)]
    mil: Option<String>,
    #[serde(default)]
    today: Option<String>,
    #[serde(default)]
    new7: Option<String>,
    #[serde(default)]
    interesting: Option<String>,
    #[serde(default)]
    privacy: Option<String>,
    #[serde(default)]
    emergency: Option<String>,
    #[serde(default)]
    regular: Option<String>,
    #[serde(default)]
    dormant: Option<String>,
    #[serde(default)]
    alt_min: Option<String>,
    #[serde(default)]
    alt_max: Option<String>,
}

impl SortParams {
    fn filter(&self) -> stats::FleetFilter {
        let cat = match self.cat.as_str() {
            "fixed" | "heli" => self.cat.clone(),
            _ => String::new(),
        };
        stats::FleetFilter {
            q: self.q.clone(),
            cat,
            mil: self.mil.as_deref() == Some("1"),
            today: self.today.as_deref() == Some("1"),
            new7: self.new7.as_deref() == Some("1"),
            interesting: self.interesting.as_deref() == Some("1"),
            privacy: self.privacy.as_deref() == Some("1"),
            emergency: self.emergency.as_deref() == Some("1"),
            regular: self.regular.as_deref() == Some("1"),
            dormant: self.dormant.as_deref() == Some("1"),
            alt_min: parse_alt(&self.alt_min),
            alt_max: parse_alt(&self.alt_max),
        }
    }
}

fn parse_alt(v: &Option<String>) -> Option<i64> {
    v.as_deref().and_then(|s| s.trim().parse::<i64>().ok())
}

/// Percent-encode a query-string value. Only the free-text search can carry
/// anything outside `[A-Za-z0-9._~-]`.
fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The dashboard URL prefix that reproduces the current filter, ready for a
/// `sort`/`dir`/`page` suffix. Ends in `&` when non-empty, so the templates can
/// append without caring. Without this every sort, filter and page lived only
/// in hidden inputs and the address bar never changed.
fn fleet_query(filter: &stats::FleetFilter) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !filter.q.trim().is_empty() {
        parts.push(format!("q={}", enc(filter.q.trim())));
    }
    if !filter.cat.is_empty() {
        parts.push(format!("cat={}", filter.cat));
    }
    for (name, on) in [
        ("mil", filter.mil),
        ("interesting", filter.interesting),
        ("privacy", filter.privacy),
        ("emergency", filter.emergency),
        ("today", filter.today),
        ("new7", filter.new7),
        ("dormant", filter.dormant),
        ("regular", filter.regular),
    ] {
        if on {
            parts.push(format!("{name}=1"));
        }
    }
    if let Some(v) = filter.alt_min {
        parts.push(format!("alt_min={v}"));
    }
    if let Some(v) = filter.alt_max {
        parts.push(format!("alt_max={v}"));
    }
    if parts.is_empty() { "/?".to_string() } else { format!("/?{}&", parts.join("&")) }
}

/// The address-bar URL for a fleet view. Returned to htmx as `HX-Push-Url` so
/// sort, filter and page changes are bookmarkable and Back works, without the
/// templates having to reconstruct the query string.
fn canonical_url(filter: &stats::FleetFilter, sort: &str, dir: &str, page: u32) -> String {
    format!("{}sort={sort}&dir={dir}&page={page}", fleet_query(filter))
}

#[derive(Template)]
#[template(path = "partials/table.html")]
struct TableTemplate {
    rows: Vec<AircraftRow>,
    sort: String,
    dir: String,
    filter: stats::FleetFilter,
    tar1090: String,
    now: f64,
    page: u32,
    pages: u32,
    total: i64,
    from_row: u32,
    to_row: u32,
    prev_page: Option<u32>,
    next_page: Option<u32>,
}

async fn table_partial(
    State(db): State<Db>,
    Query(q): Query<SortParams>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let now = now();
    let view = fleet_view(&db, &q, now)?;
    let url = canonical_url(&view.filter, &view.sort, &view.dir, view.nav.page);
    let body = TableTemplate {
        rows: view.rows,
        sort: view.sort,
        dir: view.dir,
        filter: view.filter,
        tar1090: tar1090_base(&headers),
        now,
        page: view.nav.page,
        pages: view.nav.pages,
        total: view.nav.total,
        from_row: view.nav.from_row,
        to_row: view.nav.to_row,
        prev_page: view.nav.prev_page,
        next_page: view.nav.next_page,
    }
    .render()?;
    let mut out = HeaderMap::new();
    if let Ok(v) = axum::http::HeaderValue::from_str(&url) {
        // Search-as-you-type fires every 400 ms; pushing each keystroke would
        // bury the Back button. Replace for those, push for discrete actions.
        let from_search = headers
            .get("hx-trigger")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("fleet-search"));
        out.insert(if from_search { "hx-replace-url" } else { "hx-push-url" }, v);
    }
    Ok((out, Html(body)).into_response())
}

/// Rows only, for the periodic refresh: swapping just the `<tbody>` keeps the
/// table's horizontal scroll position and does not touch the headers.
#[derive(Template)]
#[template(path = "partials/table_rows.html")]
struct TableRowsTemplate {
    rows: Vec<AircraftRow>,
    tar1090: String,
    now: f64,
}

async fn table_rows_partial(
    State(db): State<Db>,
    Query(q): Query<SortParams>,
    headers: HeaderMap,
) -> Result<Html<String>, AppError> {
    let now = now();
    let view = fleet_view(&db, &q, now)?;
    Ok(Html(
        TableRowsTemplate { rows: view.rows, tar1090: tar1090_base(&headers), now }.render()?,
    ))
}

#[derive(Template)]
#[template(path = "partials/cards.html")]
struct CardsTemplate {
    stats: Stats,
}

async fn cards_partial(State(db): State<Db>) -> Result<Html<String>, AppError> {
    Ok(Html(CardsTemplate { stats: stats::stats_cached(&db, now())? }.render()?))
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
    live: Vec<AircraftRow>,
    tar1090: String,
    rx: Option<(f64, f64)>,
}

async fn live_partial(State(db): State<Db>, headers: HeaderMap) -> Result<Html<String>, AppError> {
    let now = now();
    Ok(Html(
        LiveTemplate {
            live: stats::live_aircraft(&db, now)?,
            tar1090: tar1090_base(&headers),
            rx: stats::rx_pos(),
        }
        .render()?,
    ))
}

async fn api_live(State(db): State<Db>) -> Result<Json, AppError> {
    Ok(Json(serde_json::to_value(stats::live_aircraft(&db, now())?)?))
}

#[derive(Template)]
#[template(path = "today.html")]
struct TodayTemplate {
    summary: stats::TodaySummary,
    hours: Vec<stats::HourBucket>,
    hours_max: i64,
    records: Vec<stats::DayRecord>,
    new: Vec<AircraftRow>,
    tar1090: String,
}

async fn today_page(State(db): State<Db>, headers: HeaderMap) -> Result<Html<String>, AppError> {
    let now = now();
    let hours = stats::hour_histogram(&db, now)?;
    let hours_max = hours.iter().map(|h| h.count).max().unwrap_or(0);
    Ok(Html(
        TodayTemplate {
            summary: stats::today_summary(&db, now)?,
            hours,
            hours_max,
            records: stats::records_today(&db, now)?,
            new: stats::new_today(&db, now)?,
            tar1090: tar1090_base(&headers),
        }
        .render()?,
    ))
}

async fn api_today(State(db): State<Db>) -> Result<Json, AppError> {
    let now = now();
    Ok(Json(serde_json::json!({
        "summary": stats::today_summary(&db, now)?,
        "hours": stats::hour_histogram(&db, now)?,
        "records": stats::records_today(&db, now)?,
        "new": stats::new_today(&db, now)?,
    })))
}

#[derive(Template)]
#[template(path = "activity.html")]
struct ActivityTemplate {
    heatmap: stats::Heatmap,
    /// Hour labels for the heatmap header; empty strings keep the grid aligned.
    hours: Vec<String>,
    /// Every hour, for the table view — the grid header only labels every third.
    hours_full: Vec<String>,
    range: stats::LineChart,
    messages: stats::LineChart,
    tar1090: String,
}

async fn activity_page(State(db): State<Db>, headers: HeaderMap) -> Result<Html<String>, AppError> {
    let now = now();
    let heatmap = stats::weekday_hour_heatmap(&db)?;
    let range = stats::range_by_day(&db, now)?;
    let messages = stats::messages_by_day(&db, now)?;
    let hours = (0..24)
        .map(|h| if h % 3 == 0 { format!("{h:02}") } else { String::new() })
        .collect();
    let hours_full = (0..24).map(|h| format!("{h:02}")).collect();
    Ok(Html(
        ActivityTemplate {
            heatmap,
            hours,
            hours_full,
            range: stats::LineChart::build(&range),
            messages: stats::LineChart::build(&messages),
            tar1090: tar1090_base(&headers),
        }
        .render()?,
    ))
}

async fn api_activity(State(db): State<Db>) -> Result<Json, AppError> {
    Ok(Json(serde_json::to_value(stats::activity(&db, now())?)?))
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
    // The JSON API stays a full list; pagination is a UI concern (PLAN.md step 3).
    let filter = q.filter();
    let rows = stats::aircraft_sorted(&db, &q.sort, &q.dir, &filter, u32::MAX, 0, now())?;
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
        AppError(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
    }
}
impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        tracing::error!("template: {e}");
        AppError(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        tracing::error!("json: {e}");
        AppError(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
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
        match serde_json::to_vec(&self.0) {
            Ok(body) => ([(header::CONTENT_TYPE, "application/json")], body).into_response(),
            Err(e) => AppError::from(e).into_response(),
        }
    }
}

async fn htmx_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        HTMX,
    )
}

async fn app_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css"), (header::CACHE_CONTROL, "no-cache")],
        APP_CSS,
    )
}

async fn app_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        APP_JS,
    )
}

struct AppError(StatusCode, String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}
