//! Aircraft type profiles: ICAO Doc 8643 names/specs plus Wikipedia history
//! and photos.
//!
//! Everything here is cached in SQLite. The network is only touched by the
//! background task in [`run`]; a failed fetch leaves the previous rows in
//! place and the UI keeps rendering whatever it has. The type dex is a
//! nice-to-have, never a startup dependency.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;

use crate::stats::title_case;
use crate::{db_lock, Db};

const DOC_URL: &str = "https://doc8643.icao.int/External/AircraftTypes";
const WIKI_API: &str = "https://en.wikipedia.org/w/api.php";
const WIKI_LIST_PAGE: &str = "List_of_aircraft_type_designators";
const WIKI_UA: &str = "plane-stats/0.1 (personal ADS-B dashboard)";

/// Doc 8643 changes rarely; a month-old copy is fine.
const DOC_TTL: f64 = 30.0 * 86_400.0;
/// The Wikipedia designator table changes more often than Doc 8643.
const WIKI_MAP_TTL: f64 = 7.0 * 86_400.0;
/// Re-fetch a type profile after this long. A type with no article is stamped
/// too, so a miss does not retry every cycle.
const PROFILE_TTL: f64 = 30.0 * 86_400.0;
/// Space out Wikipedia calls: the API is happy with a few per second, but this
/// is a background nicety on a Pi, not a scraper.
const FETCH_GAP: Duration = Duration::from_millis(2000);
const RETRY_GAP: Duration = Duration::from_secs(3600);

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs_f64()
}

/// One row of the ICAO Doc 8643 JSON. The API returns several rows per
/// designator (variants, military names, business-jet conversions).
#[derive(Debug, Deserialize)]
struct DocRow {
    #[serde(rename = "ModelFullName", default)]
    model: String,
    #[serde(rename = "Designator", default)]
    designator: String,
    #[serde(rename = "ManufacturerCode", default)]
    manufacturer: String,
    #[serde(rename = "AircraftDescription", default)]
    category: String,
    #[serde(rename = "EngineCount", default)]
    engine_count: String,
    #[serde(rename = "EngineType", default)]
    engine_type: String,
    #[serde(rename = "WTC", default)]
    wtc: String,
}

#[derive(Debug)]
struct CanonType {
    designator: String,
    manufacturer: String,
    model: String,
    engine_type: String,
    engine_count: String,
    wtc: String,
    category: String,
}

/// Reduce the Doc 8643 rows to one canonical row per designator: the most
/// common manufacturer, then its shortest model name. `B738` has `737-800`
/// and `737-800 BBJ2`; the first is the one a spotter means. `C172` has rows
/// from Cessna and Aviones Colombia; Cessna wins on frequency.
fn canonicalize(rows: Vec<DocRow>) -> Vec<CanonType> {
    let mut by_code: BTreeMap<String, Vec<DocRow>> = BTreeMap::new();
    for row in rows {
        let code = row.designator.trim().to_uppercase();
        if code.len() < 2 || code.len() > 4 || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
            continue;
        }
        by_code.entry(code).or_default().push(row);
    }
    by_code
        .into_iter()
        .filter_map(|(designator, rows)| {
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for row in &rows {
                if !row.manufacturer.trim().is_empty() {
                    *counts.entry(row.manufacturer.trim()).or_default() += 1;
                }
            }
            let manufacturer = counts
                .into_iter()
                .max_by_key(|(name, n)| (*n, std::cmp::Reverse(name.len())))
                .map(|(name, _)| name.to_string())
                .unwrap_or_default();
            let base = rows
                .iter()
                .filter(|r| manufacturer.is_empty() || r.manufacturer.trim() == manufacturer)
                .min_by_key(|r| (r.model.len(), r.model.as_str()))?;
            Some(CanonType {
                designator,
                manufacturer,
                model: base.model.trim().to_string(),
                engine_type: base.engine_type.trim().to_string(),
                engine_count: base.engine_count.trim().to_string(),
                wtc: base.wtc.trim().to_string(),
                category: base.category.trim().to_string(),
            })
        })
        .collect()
}

/// `| B738 || 738 || [[Boeing 737 Next Generation|Boeing 737-800]]` -> the
/// first link target. The table is small and stable, and the link target is
/// the article the type page should open.
fn parse_wiki_map(wikitext: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in wikitext.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_start_matches('|').split("||").map(str::trim).collect();
        if cells.len() < 3 {
            continue;
        }
        let code = cells[0].trim().to_uppercase();
        if code.len() < 2 || code.len() > 4 || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
            continue;
        }
        if let Some(title) = first_link(cells[2]) {
            out.push((code, title));
        }
    }
    out
}

fn first_link(cell: &str) -> Option<String> {
    let start = cell.find("[[")? + 2;
    let rest = &cell[start..];
    let end = rest.find("]]")?;
    let target = rest[..end].split('|').next()?.split('#').next()?.trim();
    if target.is_empty() {
        None
    } else {
        Some(target.replace('_', " "))
    }
}

/// `+1967-04-09T00:00:00Z` with Wikidata precision 11 -> `1967-04-09`.
/// Precision 9 is a year only; printing a fake January 1st would be worse.
fn fmt_time(value: &Value) -> Option<String> {
    let time = value["time"].as_str()?.strip_prefix('+').unwrap_or_default();
    match value["precision"].as_i64().unwrap_or(11) {
        11 => time.get(..10).map(str::to_string),
        10 => time.get(..7).map(str::to_string),
        _ => time.get(..4).map(str::to_string),
    }
}

/// `+10510` -> `10,510`; `+34.30` -> `34.3`.
fn fmt_amount(amount: &str) -> Option<String> {
    let amount = amount.strip_prefix('+').unwrap_or(amount);
    let value: f64 = amount.parse().ok()?;
    if value.fract().abs() < 0.05 {
        Some(group_thousands(value.round() as i64))
    } else {
        Some(format!("{value:.1}"))
    }
}

fn group_thousands(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(WIKI_UA)
        .timeout(Duration::from_secs(30))
        .build()
}

/// Background loop: refresh Doc 8643, the Wikipedia designator map, and the
/// profiles for types actually seen. Every failure is logged and retried on
/// the next cycle; the server never waits on it.
pub async fn run(db: Db) {
    let client = match client() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("type dex client: {e}");
            return;
        }
    };
    loop {
        if let Err(e) = load_doc8643(&db, &client).await {
            tracing::warn!("type dex: Doc 8643 load failed: {e}");
        }
        if let Err(e) = load_wiki_map(&db, &client).await {
            tracing::warn!("type dex: Wikipedia designator map failed: {e}");
        }
        if let Err(e) = enrich_profiles(&db, &client).await {
            tracing::warn!("type dex: profile fetch stopped: {e}");
        }
        tokio::time::sleep(RETRY_GAP).await;
    }
}

fn fresh(db: &Db, key: &str, ttl: f64, now: f64) -> rusqlite::Result<bool> {
    let conn = db_lock(db);
    let value: Option<String> = conn
        .query_row("SELECT value FROM enrichment_meta WHERE key = ?1", params![key], |r| r.get(0))
        .optional()?;
    Ok(value.and_then(|v| v.parse::<f64>().ok()).is_some_and(|ts| now - ts < ttl))
}

async fn load_doc8643(db: &Db, client: &reqwest::Client) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = now();
    if fresh(db, "doc8643_v1", DOC_TTL, now)? {
        return Ok(());
    }
    // reqwest omits Content-Length for an empty body; the ICAO endpoint
    // answers 411 without it. Send the header explicitly.
    let rows: Vec<DocRow> = client
        .post(DOC_URL)
        .header(reqwest::header::CONTENT_LENGTH, "0")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let types = canonicalize(rows);
    if types.is_empty() {
        return Err("Doc 8643 returned no designators".into());
    }

    let conn = db_lock(db);
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO aircraft_types
               (designator, manufacturer, model, engine_type, engine_count, wtc, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(designator) DO UPDATE SET
               manufacturer = excluded.manufacturer,
               model        = excluded.model,
               engine_type  = excluded.engine_type,
               engine_count = excluded.engine_count,
               wtc          = excluded.wtc,
               category     = excluded.category",
        )?;
        for t in &types {
            stmt.execute(params![
                t.designator,
                t.manufacturer,
                t.model,
                t.engine_type,
                t.engine_count,
                t.wtc,
                t.category
            ])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO enrichment_meta (key, value) VALUES ('doc8643_v1', ?1)",
        params![now.to_string()],
    )?;
    tx.commit()?;
    tracing::info!("type dex: loaded {} ICAO designators", types.len());
    Ok(())
}

async fn load_wiki_map(db: &Db, client: &reqwest::Client) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = now();
    if fresh(db, "wiki_map_v1", WIKI_MAP_TTL, now)? {
        return Ok(());
    }
    let body: Value = client
        .get(WIKI_API)
        .query(&[
            ("action", "parse"),
            ("page", WIKI_LIST_PAGE),
            ("prop", "wikitext"),
            ("format", "json"),
            ("formatversion", "2"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let wikitext = body["parse"]["wikitext"].as_str().ok_or("no wikitext in parse response")?;
    let map = parse_wiki_map(wikitext);
    if map.is_empty() {
        return Err("designator table parsed to zero rows".into());
    }

    let conn = db_lock(db);
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM type_wiki_map", [])?;
    {
        let mut stmt = tx.prepare("INSERT OR REPLACE INTO type_wiki_map (designator, title) VALUES (?1, ?2)")?;
        for (code, title) in &map {
            stmt.execute(params![code, title])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO enrichment_meta (key, value) VALUES ('wiki_map_v1', ?1)",
        params![now.to_string()],
    )?;
    tx.commit()?;
    tracing::info!("type dex: loaded {} Wikipedia type links", map.len());
    Ok(())
}

/// Types seen on the air that have no fresh profile yet.
fn pending_profiles(db: &Db, now: f64) -> rusqlite::Result<Vec<(String, Option<String>, Option<String>, Option<String>)>> {
    let conn = db_lock(db);
    let mut stmt = conn.prepare(
        "SELECT s.designator, t.model, t.manufacturer, m.title
         FROM (SELECT DISTINCT e.typecode AS designator
               FROM aircraft a CROSS JOIN enrichment e ON e.icao24 = a.hex
               WHERE e.typecode IS NOT NULL AND e.typecode <> '') s
         LEFT JOIN aircraft_types t ON t.designator = s.designator
         LEFT JOIN type_wiki_map m ON m.designator = s.designator
         WHERE NOT EXISTS (
             SELECT 1 FROM aircraft_types x
             WHERE x.designator = s.designator
               AND x.wiki_fetched_at IS NOT NULL
               AND x.wiki_fetched_at > ?1
         )
         ORDER BY s.designator",
    )?;
    let rows = stmt.query_map(params![now - PROFILE_TTL], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    rows.collect()
}

fn is_rate_limited(e: &(dyn std::error::Error + 'static)) -> bool {
    e.downcast_ref::<reqwest::Error>()
        .and_then(|e| e.status())
        .is_some_and(|s| s == reqwest::StatusCode::TOO_MANY_REQUESTS)
}

async fn enrich_profiles(db: &Db, client: &reqwest::Client) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = now();
    let pending = pending_profiles(db, now)?;
    if pending.is_empty() {
        return Ok(());
    }
    tracing::info!("type dex: fetching {} type profiles", pending.len());

    // Resolve each designator to an article title. The curated table covers
    // the airliners; the rest use the Doc 8643 manufacturer + model, which the
    // batch query resolves through Wikipedia's normalisation and redirects.
    let mut by_title: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (code, model, manufacturer, mapped) in pending {
        let title = match mapped {
            Some(title) => Some(title),
            // The designator table misses most GA and helicopters. Doc 8643
            // gives a canonical manufacturer + model, which is usually the
            // article title or a redirect; the batch query resolves it (or
            // reports it missing) without a per-type search request.
            None => match (manufacturer.as_deref(), model.as_deref()) {
                (Some(mf), Some(m)) if !mf.is_empty() && !m.is_empty() => {
                    Some(format!("{} {m}", title_case(mf)))
                }
                _ => None,
            },
        };
        match title {
            Some(title) => by_title.entry(title).or_default().push(code),
            None => mark_attempt(db, &code, now)?,
        }
    }

    // One request carries 20 titles. The extracts API is the expensive one, so
    // batching is what keeps this under Wikipedia's rate limit.
    let titles: Vec<String> = by_title.keys().cloned().collect();
    let mut pages: HashMap<String, PageProfile> = HashMap::new();
    let mut redirects: HashMap<String, String> = HashMap::new();
    for chunk in titles.chunks(20) {
        let batch = fetch_pages_retry(client, chunk).await?;
        pages.extend(batch.pages);
        redirects.extend(batch.redirects);
        tokio::time::sleep(FETCH_GAP).await;
    }

    // Attach the cached page to every designator that resolved to it.
    let mut resolved: Vec<(String, PageProfile)> = Vec::new();
    for (title, codes) in by_title {
        let final_title = resolve_title(&title, &redirects);
        match pages.get(&final_title) {
            Some(page) => {
                for code in codes {
                    resolved.push((code, page.clone()));
                }
            }
            None => {
                for code in codes {
                    mark_attempt(db, &code, now)?;
                }
            }
        }
    }

    // Wikidata facts in one request per 50 QIDs.
    let qids: Vec<String> =
        resolved.iter().filter_map(|(_, p)| p.qid.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    let mut facts: HashMap<String, Facts> = HashMap::new();
    for chunk in qids.chunks(50) {
        facts.extend(fetch_facts_retry(client, chunk).await?);
        tokio::time::sleep(FETCH_GAP).await;
    }

    for (code, page) in resolved {
        let (first_flight, total_produced, wingspan) = page
            .qid
            .as_ref()
            .and_then(|qid| facts.get(qid))
            .cloned()
            .unwrap_or((None, None, None));
        let profile = Profile {
            title: page.title,
            extract: page.extract,
            image: page.image,
            url: page.url,
            first_flight,
            total_produced,
            wingspan,
        };
        save_profile(db, &code, &profile, now)?;
    }
    Ok(())
}

struct Profile {
    title: String,
    extract: Option<String>,
    image: Option<String>,
    url: String,
    first_flight: Option<String>,
    total_produced: Option<String>,
    wingspan: Option<String>,
}

#[derive(Clone)]
struct PageProfile {
    title: String,
    extract: Option<String>,
    image: Option<String>,
    qid: Option<String>,
    url: String,
}

struct PageBatch {
    pages: HashMap<String, PageProfile>,
    /// `normalized` and `redirects` merged: requested title -> final title.
    redirects: HashMap<String, String>,
}

/// One MediaWiki query for up to 20 titles: intro extract, page image and the
/// Wikidata QID. Batching keeps a first boot at ~6 requests instead of 106.
async fn fetch_pages(
    client: &reqwest::Client,
    titles: &[String],
) -> Result<PageBatch, Box<dyn std::error::Error + Send + Sync>> {
    let joined = titles.join("|");
    let body: Value = client
        .get(WIKI_API)
        .query(&[
            ("action", "query"),
            ("prop", "pageimages|extracts|pageprops"),
            ("exintro", "1"),
            ("explaintext", "1"),
            ("exsentences", "6"),
            ("exlimit", "20"),
            ("pilimit", "50"),
            ("pithumbsize", "640"),
            ("titles", joined.as_str()),
            ("format", "json"),
            ("formatversion", "2"),
            ("redirects", "1"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mut redirects = HashMap::new();
    for key in ["normalized", "redirects"] {
        if let Some(items) = body["query"][key].as_array() {
            for item in items {
                if let (Some(from), Some(to)) = (item["from"].as_str(), item["to"].as_str()) {
                    redirects.insert(from.to_string(), to.to_string());
                }
            }
        }
    }

    let mut pages = HashMap::new();
    if let Some(items) = body["query"]["pages"].as_array() {
        for page in items {
            if page["missing"].as_bool() == Some(true) || page["invalid"].is_string() {
                continue;
            }
            let Some(title) = page["title"].as_str() else { continue };
            let extract = page["extract"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
            let image = page["thumbnail"]["source"].as_str().map(str::to_string);
            let qid = page["pageprops"]["wikibase_item"].as_str().map(str::to_string);
            let url = format!("https://en.wikipedia.org/wiki/{}", title.replace(' ', "_"));
            pages.insert(
                title.to_string(),
                PageProfile { title: title.to_string(), extract, image, qid, url },
            );
        }
    }
    Ok(PageBatch { pages, redirects })
}

async fn fetch_pages_retry(
    client: &reqwest::Client,
    titles: &[String],
) -> Result<PageBatch, Box<dyn std::error::Error + Send + Sync>> {
    let mut tries = 0;
    loop {
        match fetch_pages(client, titles).await {
            Ok(batch) => return Ok(batch),
            Err(e) if is_rate_limited(e.as_ref()) && tries < 3 => {
                tries += 1;
                tracing::warn!("type dex: page batch rate limited, backing off 60 s");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

fn resolve_title(title: &str, redirects: &HashMap<String, String>) -> String {
    let mut current = title.replace('_', " ");
    for _ in 0..5 {
        match redirects.get(&current) {
            Some(next) => current = next.clone(),
            None => break,
        }
    }
    current
}

type Facts = (Option<String>, Option<String>, Option<String>);

async fn fetch_facts(
    client: &reqwest::Client,
    qids: &[String],
) -> Result<HashMap<String, Facts>, Box<dyn std::error::Error + Send + Sync>> {
    let joined = qids.join("|");
    let body: Value = client
        .get("https://www.wikidata.org/w/api.php")
        .query(&[
            ("action", "wbgetentities"),
            ("ids", joined.as_str()),
            ("props", "claims"),
            ("format", "json"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut out = HashMap::new();
    for qid in qids {
        out.insert(qid.clone(), facts_from_claims(&body["entities"][qid.as_str()]["claims"]));
    }
    Ok(out)
}

async fn fetch_facts_retry(
    client: &reqwest::Client,
    qids: &[String],
) -> Result<HashMap<String, Facts>, Box<dyn std::error::Error + Send + Sync>> {
    let mut tries = 0;
    loop {
        match fetch_facts(client, qids).await {
            Ok(facts) => return Ok(facts),
            Err(e) if is_rate_limited(e.as_ref()) && tries < 3 => {
                tries += 1;
                tracing::warn!("type dex: Wikidata batch rate limited, backing off 60 s");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

fn facts_from_claims(claims: &Value) -> Facts {
    let first_flight = claims["P606"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| fmt_time(&c["mainsnak"]["datavalue"]["value"]));
    let total_produced = claims["P1092"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| c["mainsnak"]["datavalue"]["value"]["amount"].as_str())
        .and_then(fmt_amount);
    let wingspan = claims["P2050"].as_array().and_then(|a| a.first()).and_then(|c| {
        let value = &c["mainsnak"]["datavalue"]["value"];
        let unit = value["unit"].as_str()?;
        // Q11573 is the metre. Anything else (feet, hands) is skipped rather
        // than shown with the wrong unit.
        if !unit.ends_with("/Q11573") {
            return None;
        }
        fmt_amount(value["amount"].as_str()?).map(|v| format!("{v} m"))
    });
    (first_flight, total_produced, wingspan)
}

fn save_profile(db: &Db, code: &str, profile: &Profile, now: f64) -> rusqlite::Result<()> {
    let conn = db_lock(db);
    conn.execute(
        "INSERT INTO aircraft_types
           (designator, wiki_title, wiki_extract, wiki_image, wiki_url,
            first_flight, total_produced, wingspan_m, wiki_fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(designator) DO UPDATE SET
           wiki_title     = excluded.wiki_title,
           wiki_extract   = excluded.wiki_extract,
           wiki_image     = excluded.wiki_image,
           wiki_url       = excluded.wiki_url,
           first_flight   = excluded.first_flight,
           total_produced = excluded.total_produced,
           wingspan_m     = excluded.wingspan_m,
           wiki_fetched_at = excluded.wiki_fetched_at",
        params![
            code,
            profile.title,
            profile.extract,
            profile.image,
            profile.url,
            profile.first_flight,
            profile.total_produced,
            profile.wingspan,
            now
        ],
    )?;
    Ok(())
}

/// Stamp a type whose article could not be resolved, so a missing page does
/// not cost a request every six hours.
fn mark_attempt(db: &Db, code: &str, now: f64) -> rusqlite::Result<()> {
    let conn = db_lock(db);
    conn.execute(
        "INSERT INTO aircraft_types (designator, wiki_fetched_at) VALUES (?1, ?2)
         ON CONFLICT(designator) DO UPDATE SET wiki_fetched_at = excluded.wiki_fetched_at",
        params![code, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(model: &str, code: &str, manufacturer: &str) -> DocRow {
        DocRow {
            model: model.into(),
            designator: code.into(),
            manufacturer: manufacturer.into(),
            category: "LandPlane".into(),
            engine_count: "2".into(),
            engine_type: "Jet".into(),
            wtc: "M".into(),
        }
    }

    /// The canonical row is what the card shows and what the Wikipedia search
    /// is built from: the base model, not a business-jet conversion.
    #[test]
    fn canonicalize_prefers_the_base_model_of_the_common_manufacturer() {
        let rows = vec![
            doc("737-800 BBJ2", "B738", "BOEING"),
            doc("737-800", "B738", "BOEING"),
            doc("172", "C172", "AVIONES COLOMBIA"),
            doc("172", "C172", "CESSNA"),
            doc("172 Skyhawk", "C172", "CESSNA"),
            doc("R172", "C172", "CESSNA"),
        ];
        let types = canonicalize(rows);
        let b738 = types.iter().find(|t| t.designator == "B738").unwrap();
        assert_eq!(b738.model, "737-800");
        assert_eq!(b738.manufacturer, "BOEING");
        let c172 = types.iter().find(|t| t.designator == "C172").unwrap();
        assert_eq!(c172.model, "172");
        assert_eq!(c172.manufacturer, "CESSNA");
    }

    /// The wikitext table is the only curated designator -> article mapping;
    /// a parser regression would silently send every card to a search guess.
    #[test]
    fn parses_designator_rows_and_ignores_headers() {
        let wikitext = "\
{| class=\"wikitable\"
|-
! ICAO !! IATA !! Model
|-
| B738 || 738 || [[Boeing 737 Next Generation|Boeing 737-800]]
|-
| B38M || 7M8 || [[Boeing 737 MAX#737 MAX 8|Boeing 737 MAX 8]]
|-
| A20N || 32N || [[Airbus A320neo]]
|}";
        let map = parse_wiki_map(wikitext);
        assert_eq!(map.len(), 3);
        assert!(map.contains(&("B738".into(), "Boeing 737 Next Generation".into())));
        assert!(map.contains(&("B38M".into(), "Boeing 737 MAX".into())));
        assert!(map.contains(&("A20N".into(), "Airbus A320neo".into())));
    }

    #[test]
    fn formats_wikidata_dates_by_precision() {
        let day = serde_json::json!({"time": "+1967-04-09T00:00:00Z", "precision": 11});
        let year = serde_json::json!({"time": "+1967-01-01T00:00:00Z", "precision": 9});
        assert_eq!(fmt_time(&day).as_deref(), Some("1967-04-09"));
        assert_eq!(fmt_time(&year).as_deref(), Some("1967"));
    }

    #[test]
    fn formats_wikidata_quantities() {
        assert_eq!(fmt_amount("+10510").as_deref(), Some("10,510"));
        assert_eq!(fmt_amount("+34.30").as_deref(), Some("34.3"));
        assert_eq!(fmt_amount("-5").as_deref(), Some("-5"));
    }
}
