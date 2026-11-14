use std::path::Path;

use rusqlite::params;

use crate::Db;

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
pub fn load(db: &Db) -> Result<usize, Box<dyn std::error::Error>> {
    let path = std::env::var("ENRICHMENT_CSV")
        .unwrap_or_else(|_| "/usr/share/plane-stats/aircraftDatabase.csv".to_string());
    if !Path::new(&path).exists() {
        tracing::warn!("enrichment CSV not found at {path}; skipping");
        return Ok(0);
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
    tx.commit()?;
    Ok(rows.len())
}
