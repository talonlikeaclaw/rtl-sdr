//! Historical flag backfill from tar1090-db's `aircraft.csv.gz`.
//!
//! The live readsb only emits `dbFlags` for aircraft it hears after the DB is
//! loaded, so rows recorded before `TAR1090_ENABLE_AC_DB` existed would keep
//! zero flags until the aircraft flies past again. The CSV is baked into the
//! image (decompressed) and its flag bits are ORed into every aircraft already
//! in the ledger, once per CSV version.

use std::collections::HashSet;
use std::path::Path;
use std::time::UNIX_EPOCH;

use rusqlite::{params, OptionalExtension};

use crate::Db;

pub fn backfill(db: &Db) -> Result<usize, Box<dyn std::error::Error>> {
    let path = std::env::var("TAR1090_DB_CSV")
        .unwrap_or_else(|_| "/usr/share/plane-stats/aircraft-db.csv".to_string());
    if !Path::new(&path).exists() {
        tracing::warn!("tar1090 DB CSV not found at {path}; skipping flag backfill");
        return Ok(0);
    }
    let stamp = stamp_for(&path)?;

    let seen: HashSet<String> = {
        let conn = crate::db_lock(db);
        let previous: Option<String> = conn
            .query_row("SELECT value FROM enrichment_meta WHERE key = 'tar1090_db'", [], |r| {
                r.get(0)
            })
            .optional()?;
        if previous.as_deref() == Some(stamp.as_str()) {
            return Ok(0);
        }
        let mut stmt = conn.prepare("SELECT hex FROM aircraft")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<HashSet<_>>>()?
    };

    // CSV columns: icao;registration;type;flags;... with flags as one
    // character per bit, least significant first ("0010" = PIA).
    let file = std::fs::File::open(&path)?;
    let mut reader =
        csv::ReaderBuilder::new().delimiter(b';').has_headers(false).flexible(true).from_reader(file);

    let mut flags: Vec<(String, u16)> = Vec::new();
    for result in reader.records() {
        let record = result?;
        let (Some(hex), Some(raw)) = (record.get(0), record.get(3)) else {
            continue;
        };
        let hex = hex.trim().to_ascii_lowercase();
        if !seen.contains(&hex) {
            continue;
        }
        let bits = parse_flags(raw);
        if bits != 0 {
            flags.push((hex, bits));
        }
    }

    let conn = crate::db_lock(db);
    let tx = conn.unchecked_transaction()?;
    let mut updated = 0;
    {
        let mut stmt = tx.prepare(
            "UPDATE aircraft SET db_flags = coalesce(db_flags, 0) | ?1 WHERE hex = ?2",
        )?;
        for (hex, bits) in &flags {
            updated += stmt.execute(params![bits, hex])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO enrichment_meta (key, value) VALUES ('tar1090_db', ?1)",
        params![stamp],
    )?;
    tx.commit()?;
    Ok(updated)
}

/// `"0010"` -> PIA (bit 2), `"0001"` -> LADD (bit 3), `"10"` -> military.
fn parse_flags(raw: &str) -> u16 {
    raw.trim()
        .chars()
        .enumerate()
        .take(16)
        .filter(|(_, c)| *c == '1')
        .fold(0u16, |acc, (j, _)| acc | (1 << j))
}

fn stamp_for(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta.modified()?.duration_since(UNIX_EPOCH)?.as_secs();
    Ok(format!("{}:{mtime}", meta.len()))
}

#[cfg(test)]
mod tests {
    use super::parse_flags;

    #[test]
    fn parses_tar1090_flag_strings() {
        assert_eq!(parse_flags("00"), 0);
        assert_eq!(parse_flags("10"), 1); // military
        assert_eq!(parse_flags("01"), 2); // interesting
        assert_eq!(parse_flags("0010"), 4); // PIA
        assert_eq!(parse_flags("0001"), 8); // LADD
        assert_eq!(parse_flags("11"), 3);
        assert_eq!(parse_flags("0101"), 10);
        assert_eq!(parse_flags("1001"), 9);
        assert_eq!(parse_flags(" 0010 "), 4);
    }

    /// The backfill must touch only aircraft already in the ledger, keep
    /// existing bits, and leave zero-flag rows alone.
    #[test]
    fn backfills_only_seen_aircraft() {
        use std::sync::{Arc, Mutex};

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE aircraft (hex TEXT PRIMARY KEY, db_flags INTEGER);
             CREATE TABLE enrichment_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO aircraft (hex, db_flags) VALUES
               ('ae145e', NULL), ('c0187a', NULL), ('c2b3eb', 1);",
        )
        .unwrap();
        let db: crate::Db = Arc::new(Mutex::new(conn));

        let path = std::env::temp_dir().join(format!("tar1090-db-test-{}.csv", std::process::id()));
        std::fs::write(
            &path,
            "AE145E;06-6160;C17;10;;;x;\nC0187A;C-FGKN;A321;00;;;x;\n\
             C2B3EB;177703;C17;10;;;x;\nZZZZZZ;;;0010;;;x;\n",
        )
        .unwrap();
        std::env::set_var("TAR1090_DB_CSV", &path);
        let n = super::backfill(&db).unwrap();
        assert_eq!(n, 2);

        let conn = db.lock().unwrap();
        let get = |hex: &str| -> Option<i64> {
            conn.query_row("SELECT db_flags FROM aircraft WHERE hex = ?1", [hex], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(get("ae145e"), Some(1));
        assert_eq!(get("c0187a"), None);
        assert_eq!(get("c2b3eb"), Some(1));
        std::fs::remove_file(&path).ok();
    }
}
