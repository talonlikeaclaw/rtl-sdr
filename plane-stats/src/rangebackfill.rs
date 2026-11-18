//! One-shot backfill of `range_sector` from tar1090's persisted heatmap.
//!
//! plane-stats only started recording per-sector range at deploy; historical
//! per-poll positions were never stored. tar1090's `globe_history` does keep
//! the raw heatmap points (16-byte records: hex, lat*1e6, lon*1e6, alt/gs),
//! gzipped into half-hour `.bin.ttf` files. We read them once, take the
//! farthest airborne point per 10° sector per day, and merge it into
//! `range_sector`. Nothing is written back to tar1090's volume.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use rusqlite::{params, OptionalExtension};

use crate::Db;

/// Start of every point block inside a heatmap file.
const MAGIC: u32 = 0x0E7F_7C9D;
/// Positions at or beyond this latitude field are callsign metadata blocks.
const META_LAT: i32 = 1_073_741_824;
/// Ground contacts are encoded as raw altitude -123.
const GROUND_ALT: i32 = -123;
/// Sanity cap: a point farther than this cannot be a local reception.
const MAX_NM: f64 = 600.0;

fn day_from_path(path: &Path) -> Option<String> {
    let mut parts = path.iter().rev();
    parts.next()?; // filename
    parts.next()?; // heatmap
    let day = parts.next()?.to_str()?;
    let month = parts.next()?.to_str()?;
    let year = parts.next()?.to_str()?;
    if day.len() == 2 && month.len() == 2 && year.len() == 4 {
        Some(format!("{year}-{month}-{day}"))
    } else {
        None
    }
}

/// Parse one decompressed heatmap file into (day, sector) -> max range.
fn parse_points(
    data: &[u8],
    day: &str,
    rx: (f64, f64),
    out: &mut HashMap<(String, i64), f64>,
) -> bool {
    let n = data.len() / 4;
    if n < 4 {
        return false;
    }
    let u = |i: usize| -> u32 {
        u32::from_le_bytes([data[4 * i], data[4 * i + 1], data[4 * i + 2], data[4 * i + 3]])
    };
    let mut i = 0usize;
    let mut any = false;
    while i + 3 < n {
        if u(i) != MAGIC {
            i += 1;
            continue;
        }
        i += 4;
        while i + 3 < n && u(i) != MAGIC {
            let lat_raw = u(i + 1) as i32;
            if lat_raw <= META_LAT {
                let lon_raw = u(i + 2) as i32;
                let mut alt = (u(i + 3) & 0xffff) as i32;
                if alt & 0x8000 != 0 {
                    alt -= 0x1_0000;
                }
                let lat = f64::from(lat_raw) / 1e6;
                let lon = f64::from(lon_raw) / 1e6;
                if alt != GROUND_ALT && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
                {
                    let dst = crate::network::distance_nm(rx.0, rx.1, lat, lon);
                    if dst.is_finite() && dst > 0.0 && dst <= MAX_NM {
                        let bearing = crate::stats::bearing_deg(rx.0, rx.1, lat, lon);
                        let sector = crate::stats::sector_for_bearing(bearing);
                        let entry = out.entry((day.to_string(), sector)).or_insert(0.0);
                        if dst > *entry {
                            *entry = dst;
                        }
                        any = true;
                    }
                }
            }
            i += 4;
        }
    }
    any
}

fn heat_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(years) = std::fs::read_dir(root) else { return files };
    for year in years.flatten() {
        let Ok(months) = std::fs::read_dir(year.path()) else { continue };
        for month in months.flatten() {
            let Ok(days) = std::fs::read_dir(month.path()) else { continue };
            for day in days.flatten() {
                let heat = day.path().join("heatmap");
                let Ok(entries) = std::fs::read_dir(heat) else { continue };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.ends_with(".bin.ttf") {
                        files.push(path);
                    }
                }
            }
        }
    }
    files.sort();
    files
}

/// Read every heatmap file once, merge the envelope into `range_sector`, and
/// stamp `range_sector_backfill_v1`. Missing directory or empty history is
/// logged and left unstamped, so a later boot can retry.
pub fn run(db: &Db) -> rusqlite::Result<()> {
    let Some(rx) = crate::stats::rx_pos() else {
        tracing::info!("coverage backfill skipped: RX_LAT/RX_LON not set");
        return Ok(());
    };
    let root = std::env::var("GLOBE_HISTORY").unwrap_or_else(|_| "/globe_history".to_string());
    let root = Path::new(&root);
    if !root.is_dir() {
        tracing::info!("coverage backfill skipped: {} not mounted", root.display());
        return Ok(());
    }
    {
        let conn = crate::db_lock(db);
        let done = conn
            .query_row(
                "SELECT 1 FROM enrichment_meta WHERE key = 'range_sector_backfill_v1'",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if done {
            return Ok(());
        }
    }

    let files = heat_files(root);
    if files.is_empty() {
        tracing::info!("coverage backfill: no heatmap files under {}", root.display());
        return Ok(());
    }
    let mut out: HashMap<(String, i64), f64> = HashMap::new();
    let mut parsed = 0usize;
    for path in &files {
        let Some(day) = day_from_path(path) else { continue };
        let Ok(bytes) = std::fs::read(path) else { continue };
        let mut decoder = GzDecoder::new(bytes.as_slice());
        let mut raw = Vec::new();
        if decoder.read_to_end(&mut raw).is_err() {
            continue;
        }
        if parse_points(&raw, &day, rx, &mut out) {
            parsed += 1;
        }
    }
    if parsed == 0 {
        tracing::warn!("coverage backfill: parsed no points from {} files", files.len());
        return Ok(());
    }

    let mut conn = crate::db_lock(db);
    let tx = conn.transaction()?;
    let mut sectors = 0usize;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO range_sector (day, sector, max_range_nm, polls)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(day, sector) DO UPDATE SET
               max_range_nm = MAX(range_sector.max_range_nm, excluded.max_range_nm),
               polls = range_sector.polls + 1",
        )?;
        for ((day, sector), dst) in &out {
            stmt.execute(params![day, sector, dst])?;
            sectors += 1;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO enrichment_meta (key, value) VALUES ('range_sector_backfill_v1', ?1)",
        params![format!("{} files, {sectors} sectors", files.len())],
    )?;
    tx.commit()?;
    tracing::info!(
        "coverage backfill: {} files -> {} day/sector rows",
        parsed,
        sectors
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64, alt: i32) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&0x4b187du32.to_le_bytes());
        bytes[4..8].copy_from_slice(&((lat * 1e6) as i32).to_le_bytes());
        bytes[8..12].copy_from_slice(&((lon * 1e6) as i32).to_le_bytes());
        bytes[12..16].copy_from_slice(&(alt as u32).to_le_bytes());
        bytes
    }

    #[test]
    fn parses_points_and_skips_ground_and_metadata() {
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        // North of the receiver at 100 NM-ish.
        data.extend_from_slice(&point(46.5, -73.6, 100));
        // Ground contact must be ignored.
        data.extend_from_slice(&point(46.0, -73.6, GROUND_ALT));
        // Callsign metadata block must be ignored.
        let mut meta = [0u8; 16];
        meta[4..8].copy_from_slice(&(META_LAT + 1).to_le_bytes());
        data.extend_from_slice(&meta);

        let mut out = HashMap::new();
        assert!(parse_points(&data, "2026-11-17", (45.5, -73.6), &mut out));
        assert_eq!(out.len(), 1);
        let ((day, sector), dst) = out.iter().next().unwrap();
        assert_eq!(day, "2026-11-17");
        assert_eq!(*sector, 0);
        assert!((*dst - 60.0).abs() < 1.0);
    }

    #[test]
    fn day_from_heatmap_path() {
        assert_eq!(
            day_from_path(Path::new("/gh/2026/11/17/heatmap/39.bin.ttf")).as_deref(),
            Some("2026-11-17")
        );
        assert_eq!(day_from_path(Path::new("/gh/nope")), None);
    }
}
