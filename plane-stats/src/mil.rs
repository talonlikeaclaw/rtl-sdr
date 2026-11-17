//! Military ICAO address ranges.
//!
//! readsb only emits `dbFlags` when it is started with `--db-file`, and the
//! sdr-enthusiasts image (which `readsb-v4l` builds on) passes no such file,
//! so `aircraft.json` never carries the military bit. tar1090 itself falls
//! back to the ICAO ranges below (`planeObject.js: milRange()`), which is why
//! its map marks e.g. RCAF `C2xxxx` aircraft as military even when they are
//! absent from the aircraft database.
//!
//! Keep in sync with `tar1090-db/db/ranges.js` (served by the tar1090 image
//! as `/db-<version>/ranges.js`). Inclusive bounds, lowercase hex.

pub(crate) const RANGES: &[(u32, u32)] = &[
    (0xadf7c8, 0xafffff),
    (0x010070, 0x01008f),
    (0x0a4000, 0x0a4fff),
    (0x33ff00, 0x33ffff),
    (0x350000, 0x37ffff),
    (0x3aa000, 0x3affff),
    (0x3b7000, 0x3bffff),
    (0x3ea000, 0x3ebfff),
    (0x3f4000, 0x3fbfff),
    (0x400000, 0x40003f),
    (0x43c000, 0x43cfff),
    (0x444000, 0x446fff),
    (0x44f000, 0x44ffff),
    (0x457000, 0x457fff),
    (0x45f400, 0x45f4ff),
    (0x468000, 0x4683ff),
    (0x473c00, 0x473c0f),
    (0x478100, 0x4781ff),
    (0x480000, 0x480fff),
    (0x48d800, 0x48d87f),
    (0x497c00, 0x497cff),
    (0x498420, 0x49842f),
    (0x4b7000, 0x4b7fff),
    (0x4b8200, 0x4b82ff),
    (0x70c070, 0x70c07f),
    (0x710258, 0x71028f),
    (0x710380, 0x71039f),
    (0x738a00, 0x738aff),
    (0x7cf800, 0x7cfaff),
    (0x800200, 0x8002ff),
    (0xc20000, 0xc3ffff),
    (0xe40000, 0xe41fff),
];

/// True when `hex` is a 24-bit ICAO address inside a military allocation.
/// Non-ICAO addresses (`~`-prefixed) and malformed input are not military.
pub(crate) fn is_military(hex: &str) -> bool {
    match u32::from_str_radix(hex, 16) {
        Ok(addr) => RANGES.iter().any(|&(lo, hi)| (lo..=hi).contains(&addr)),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_military;

    #[test]
    fn recognizes_military_allocations() {
        // RCAF CC-177 that prompted this fallback; not in tar1090's db.
        assert!(is_military("c2b3eb"));
        // RCAF CC-130H, also flagged by the database.
        assert!(is_military("c2af27"));
        // Inclusive bounds.
        assert!(is_military("c20000"));
        assert!(is_military("c3ffff"));
        assert!(is_military("adf7c8"));
        assert!(is_military("afffff"));
    }

    #[test]
    fn rejects_civil_and_non_icao_addresses() {
        assert!(!is_military("c0187a")); // Air Canada A320
        assert!(!is_military("c1ffff")); // just below the Canadian mil block
        assert!(!is_military("c40000")); // just above it
        assert!(!is_military("~c2b3eb"));
        assert!(!is_military(""));
    }
}
