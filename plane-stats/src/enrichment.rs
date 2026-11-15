use std::path::Path;

use rusqlite::{params, OptionalExtension};

use crate::Db;

/// Result of an enrichment load attempt.
pub enum Outcome {
    /// CSV was (re)loaded into the table.
    Loaded(usize),
    /// CSV unchanged since the last load; the table was left untouched.
    Unchanged(usize),
}

/// Columns loaded from the OpenSky aircraft database CSV into the `enrichment`
/// table: (CSV header, DB column). The DB table must have these columns
/// (see the migration in main.rs).
const COLS: &[(&str, &str)] = &[
    ("icao24", "icao24"),
    ("registration", "registration"),
    ("manufacturername", "manufacturer"),
    ("model", "model"),
    ("typecode", "typecode"),
    ("owner", "owner"),
    ("operator", "operator"),
    ("serialnumber", "serialnumber"),
    ("icaoaircrafttype", "icaoaircrafttype"),
    ("operatorcallsign", "operatorcallsign"),
    ("registered", "registered"),
    ("reguntil", "reguntil"),
    ("status", "status"),
    ("built", "built"),
    ("firstflightdate", "firstflightdate"),
    ("engines", "engines"),
    ("notes", "notes"),
    ("categoryDescription", "categorydescription"),
];

/// Loads the OpenSky aircraft database CSV into the `enrichment` table.
/// Never fatal: missing file or parse errors are logged and skipped.
///
/// Reloading ~500k rows takes minutes on a Pi, so the CSV's size+mtime is
/// recorded in `enrichment_meta` and an unchanged file is skipped.
pub fn load(db: &Db) -> Result<Outcome, Box<dyn std::error::Error>> {
    let path = std::env::var("ENRICHMENT_CSV")
        .unwrap_or_else(|_| "/usr/share/plane-stats/aircraftDatabase.csv".to_string());
    if !Path::new(&path).exists() {
        tracing::warn!("enrichment CSV not found at {path}; skipping");
        return Ok(Outcome::Unchanged(0));
    }
    let stamp = stamp_for(&path)?;

    {
        let conn = db.lock().expect("db mutex");
        let previous: Option<String> = conn
            .query_row("SELECT value FROM enrichment_meta WHERE key = 'csv'", [], |r| r.get(0))
            .optional()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM enrichment", [], |r| r.get(0))?;
        if previous.as_deref() == Some(stamp.as_str()) && count > 0 {
            return Ok(Outcome::Unchanged(count as usize));
        }
    }

    let file = std::fs::File::open(&path)?;
    let mut reader = csv::ReaderBuilder::new().trim(csv::Trim::All).from_reader(file);

    let headers = reader.headers()?.clone();
    let idx: Vec<Option<usize>> =
        COLS.iter().map(|(header, _)| headers.iter().position(|h| h.eq_ignore_ascii_case(header))).collect();
    let Some(i_icao) = idx[0] else {
        return Err(format!("unexpected CSV headers: {headers:?}").into());
    };

    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    for result in reader.records() {
        let record = result?;
        let get = |i: Option<usize>| -> Option<String> {
            i.and_then(|i| record.get(i)).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
        };
        let Some(icao24) = get(Some(i_icao)) else { continue };
        let mut row: Vec<Option<String>> = idx.iter().map(|&i| get(i)).collect();
        row[0] = Some(icao24.to_lowercase());
        rows.push(row);
    }

    let mut conn = db.lock().expect("db mutex");
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM enrichment", [])?;
    let col_names: Vec<&str> = COLS.iter().map(|(_, c)| *c).collect();
    let placeholders: Vec<String> = (1..=COLS.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "INSERT OR REPLACE INTO enrichment ({}) VALUES ({})",
        col_names.join(", "),
        placeholders.join(", ")
    );
    {
        let mut stmt = tx.prepare(&sql)?;
        for r in &rows {
            stmt.execute(rusqlite::params_from_iter(r.iter()))?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO enrichment_meta (key, value) VALUES ('csv', ?1)",
        params![stamp],
    )?;
    tx.commit()?;
    Ok(Outcome::Loaded(rows.len()))
}

/// Identity of the CSV file: byte length plus mtime in nanoseconds. A rebuilt
/// image copies a fresh file, so its mtime changes and the reload runs.
fn stamp_for(path: &str) -> std::io::Result<String> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(format!("{}:{}", meta.len(), mtime))
}
