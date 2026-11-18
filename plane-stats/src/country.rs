//! State of registry from the ICAO 24-bit address.
//!
//! Source: ICAO Annex 10 Vol III, Table 9-1 (Amendment 92, effective
//! 2024-07-22), as published by `ibosoftnet/icao-aircraft-addresses`. The
//! table is generated once and committed; do not fetch it at runtime.

use rusqlite::{params, OptionalExtension};

use crate::Db;

/// `(start, end, ISO 3166-1 alpha-2, state name)`, inclusive, sorted by start.
/// An empty ISO code means the block has no country (ICAO reserved ranges).
pub(crate) const BLOCKS: &[(u32, u32, &str, &str)] = &[
    (0x004000, 0x0047ff, "ZW", "Zimbabwe"),
    (0x006000, 0x006fff, "MZ", "Mozambique"),
    (0x008000, 0x00ffff, "ZA", "South Africa"),
    (0x010000, 0x017fff, "EG", "Egypt"),
    (0x018000, 0x01ffff, "LY", "Libya"),
    (0x020000, 0x027fff, "MA", "Morocco"),
    (0x028000, 0x02ffff, "TN", "Tunisia"),
    (0x030000, 0x0307ff, "BW", "Botswana"),
    (0x032000, 0x032fff, "BI", "Burundi"),
    (0x034000, 0x034fff, "CM", "Cameroon"),
    (0x035000, 0x0357ff, "KM", "Comoros"),
    (0x036000, 0x036fff, "CG", "Congo"),
    (0x038000, 0x038fff, "CI", "Côte d’Ivoire"),
    (0x03e000, 0x03efff, "GA", "Gabon"),
    (0x040000, 0x040fff, "ET", "Ethiopia"),
    (0x042000, 0x042fff, "GQ", "Equatorial Guinea"),
    (0x044000, 0x044fff, "GH", "Ghana"),
    (0x046000, 0x046fff, "GN", "Guinea"),
    (0x048000, 0x0487ff, "GW", "Guinea-Bissau"),
    (0x04a000, 0x04a7ff, "LS", "Lesotho"),
    (0x04c000, 0x04cfff, "KE", "Kenya"),
    (0x050000, 0x050fff, "LR", "Liberia"),
    (0x054000, 0x054fff, "MG", "Madagascar"),
    (0x058000, 0x058fff, "MW", "Malawi"),
    (0x05a000, 0x05a7ff, "MV", "Maldives"),
    (0x05c000, 0x05cfff, "ML", "Mali"),
    (0x05e000, 0x05e7ff, "MR", "Mauritania"),
    (0x060000, 0x0607ff, "MU", "Mauritius"),
    (0x062000, 0x062fff, "NE", "Niger"),
    (0x064000, 0x064fff, "NG", "Nigeria"),
    (0x068000, 0x068fff, "UG", "Uganda"),
    (0x06a000, 0x06afff, "QA", "Qatar"),
    (0x06c000, 0x06cfff, "CF", "Central African Republic"),
    (0x06e000, 0x06efff, "RW", "Rwanda"),
    (0x070000, 0x070fff, "SN", "Senegal"),
    (0x074000, 0x0747ff, "SC", "Seychelles"),
    (0x076000, 0x0767ff, "SL", "Sierra Leone"),
    (0x078000, 0x078fff, "SO", "Somalia"),
    (0x07a000, 0x07a7ff, "SZ", "Eswatini"),
    (0x07c000, 0x07cfff, "SD", "Sudan"),
    (0x080000, 0x080fff, "TZ", "United Republic of Tanzania"),
    (0x084000, 0x084fff, "TD", "Chad"),
    (0x088000, 0x088fff, "TG", "Togo"),
    (0x08a000, 0x08afff, "ZM", "Zambia"),
    (0x08c000, 0x08cfff, "CD", "Democratic Republic of the Congo"),
    (0x090000, 0x090fff, "AO", "Angola"),
    (0x094000, 0x0947ff, "BJ", "Benin"),
    (0x096000, 0x0967ff, "CV", "Cabo Verde"),
    (0x098000, 0x0987ff, "DJ", "Djibouti"),
    (0x09a000, 0x09afff, "GM", "Gambia"),
    (0x09c000, 0x09cfff, "BF", "Burkina Faso"),
    (0x09e000, 0x09e7ff, "ST", "Sao Tome and Principe"),
    (0x0a0000, 0x0a7fff, "DZ", "Algeria"),
    (0x0a8000, 0x0a8fff, "BS", "Bahamas"),
    (0x0aa000, 0x0aa7ff, "BB", "Barbados"),
    (0x0ab000, 0x0ab7ff, "BZ", "Belize"),
    (0x0ac000, 0x0adfff, "CO", "Colombia"),
    (0x0ae000, 0x0aefff, "CR", "Costa Rica"),
    (0x0b0000, 0x0b0fff, "CU", "Cuba"),
    (0x0b2000, 0x0b2fff, "SV", "El Salvador"),
    (0x0b4000, 0x0b4fff, "GT", "Guatemala"),
    (0x0b6000, 0x0b6fff, "GY", "Guyana"),
    (0x0b8000, 0x0b8fff, "HT", "Haiti"),
    (0x0ba000, 0x0bafff, "HN", "Honduras"),
    (0x0bc000, 0x0bc7ff, "VC", "Saint Vincent and the Grenadines"),
    (0x0be000, 0x0befff, "JM", "Jamaica"),
    (0x0c0000, 0x0c0fff, "NI", "Nicaragua"),
    (0x0c2000, 0x0c2fff, "PA", "Panama"),
    (0x0c4000, 0x0c4fff, "DO", "Dominican Republic"),
    (0x0c6000, 0x0c6fff, "TT", "Trinidad and Tobago"),
    (0x0c8000, 0x0c8fff, "SR", "Suriname"),
    (0x0ca000, 0x0ca7ff, "AG", "Antigua and Barbuda"),
    (0x0cc000, 0x0cc7ff, "GD", "Grenada"),
    (0x0d0000, 0x0d7fff, "MX", "Mexico"),
    (0x0d8000, 0x0dffff, "VE", "Venezuela (Bolivarian Republic of)"),
    (0x100000, 0x1fffff, "RU", "Russian Federation"),
    (0x201000, 0x2017ff, "NA", "Namibia"),
    (0x202000, 0x2027ff, "ER", "Eritrea"),
    (0x300000, 0x33ffff, "IT", "Italy"),
    (0x340000, 0x37ffff, "ES", "Spain"),
    (0x380000, 0x3bffff, "FR", "France"),
    (0x3c0000, 0x3fffff, "DE", "Germany"),
    (0x400000, 0x43ffff, "GB", "United Kingdom"),
    (0x440000, 0x447fff, "AT", "Austria"),
    (0x448000, 0x44ffff, "BE", "Belgium"),
    (0x450000, 0x457fff, "BG", "Bulgaria"),
    (0x458000, 0x45ffff, "DK", "Denmark"),
    (0x460000, 0x467fff, "FI", "Finland"),
    (0x468000, 0x46ffff, "GR", "Greece"),
    (0x470000, 0x477fff, "HU", "Hungary"),
    (0x478000, 0x47ffff, "NO", "Norway"),
    (0x480000, 0x487fff, "NL", "Netherlands"),
    (0x488000, 0x48ffff, "PL", "Poland"),
    (0x490000, 0x497fff, "PT", "Portugal"),
    (0x498000, 0x49ffff, "CZ", "Czechia"),
    (0x4a0000, 0x4a7fff, "RO", "Romania"),
    (0x4a8000, 0x4affff, "SE", "Sweden"),
    (0x4b0000, 0x4b7fff, "CH", "Switzerland"),
    (0x4b8000, 0x4bffff, "TR", "Türkiye"),
    (0x4c0000, 0x4c7fff, "RS", "Serbia"),
    (0x4c8000, 0x4c87ff, "CY", "Cyprus"),
    (0x4ca000, 0x4cafff, "IE", "Ireland"),
    (0x4cc000, 0x4ccfff, "IS", "Iceland"),
    (0x4d0000, 0x4d07ff, "LU", "Luxembourg"),
    (0x4d2000, 0x4d27ff, "MT", "Malta"),
    (0x4d4000, 0x4d47ff, "MC", "Monaco"),
    (0x500000, 0x5007ff, "SM", "San Marino"),
    (0x501000, 0x5017ff, "AL", "Albania"),
    (0x501800, 0x501fff, "HR", "Croatia"),
    (0x502800, 0x502fff, "LV", "Latvia"),
    (0x503800, 0x503fff, "LT", "Lithuania"),
    (0x504800, 0x504fff, "MD", "Republic of Moldova"),
    (0x505800, 0x505fff, "SK", "Slovakia"),
    (0x506800, 0x506fff, "SI", "Slovenia"),
    (0x507800, 0x507fff, "UZ", "Uzbekistan"),
    (0x508000, 0x50ffff, "UA", "Ukraine"),
    (0x510000, 0x5107ff, "BY", "Belarus"),
    (0x511000, 0x5117ff, "EE", "Estonia"),
    (0x512000, 0x5127ff, "MK", "North Macedonia"),
    (0x513000, 0x5137ff, "BA", "Bosnia and Herzegovina"),
    (0x514000, 0x5147ff, "GE", "Georgia"),
    (0x515000, 0x5157ff, "TJ", "Tajikistan"),
    (0x516000, 0x5167ff, "ME", "Montenegro"),
    (0x600000, 0x6007ff, "AM", "Armenia"),
    (0x600800, 0x600fff, "AZ", "Azerbaijan"),
    (0x601000, 0x6017ff, "KG", "Kyrgyzstan"),
    (0x601800, 0x601fff, "TM", "Turkmenistan"),
    (0x680000, 0x6807ff, "BT", "Bhutan"),
    (0x681000, 0x6817ff, "FM", "Micronesia (Federated States of)"),
    (0x682000, 0x6827ff, "MN", "Mongolia"),
    (0x683000, 0x6837ff, "KZ", "Kazakhstan"),
    (0x684000, 0x6847ff, "PW", "Palau"),
    (0x700000, 0x700fff, "AF", "Afghanistan"),
    (0x702000, 0x702fff, "BD", "Bangladesh"),
    (0x704000, 0x704fff, "MM", "Myanmar"),
    (0x706000, 0x706fff, "KW", "Kuwait"),
    (0x708000, 0x708fff, "LA", "Lao People’s Democratic Republic"),
    (0x70a000, 0x70afff, "NP", "Nepal"),
    (0x70c000, 0x70c7ff, "OM", "Oman"),
    (0x70e000, 0x70efff, "KH", "Cambodia"),
    (0x710000, 0x717fff, "SA", "Saudi Arabia"),
    (0x718000, 0x71ffff, "KR", "Republic of Korea"),
    (0x720000, 0x727fff, "KP", "Democratic People’s Republic of Korea"),
    (0x728000, 0x72ffff, "IQ", "Iraq"),
    (0x730000, 0x737fff, "IR", "Iran (Islamic Republic of)"),
    (0x738000, 0x73ffff, "IL", "Israel"),
    (0x740000, 0x747fff, "JO", "Jordan"),
    (0x748000, 0x74ffff, "LB", "Lebanon"),
    (0x750000, 0x757fff, "MY", "Malaysia"),
    (0x758000, 0x75ffff, "PH", "Philippines"),
    (0x760000, 0x767fff, "PK", "Pakistan"),
    (0x768000, 0x76ffff, "SG", "Singapore"),
    (0x770000, 0x777fff, "LK", "Sri Lanka"),
    (0x778000, 0x77ffff, "SY", "Syrian Arab Republic"),
    (0x780000, 0x7bffff, "CN", "China"),
    (0x7c0000, 0x7fffff, "AU", "Australia"),
    (0x800000, 0x83ffff, "IN", "India"),
    (0x840000, 0x87ffff, "JP", "Japan"),
    (0x880000, 0x887fff, "TH", "Thailand"),
    (0x888000, 0x88ffff, "VN", "Viet Nam"),
    (0x890000, 0x890fff, "YE", "Yemen"),
    (0x894000, 0x894fff, "BH", "Bahrain"),
    (0x895000, 0x8957ff, "BN", "Brunei Darussalam"),
    (0x896000, 0x896fff, "AE", "United Arab Emirates"),
    (0x897000, 0x8977ff, "SB", "Solomon Islands"),
    (0x898000, 0x898fff, "PG", "Papua New Guinea"),
    (0x899000, 0x8997ff, "", "ICAO(2)"),
    (0x8a0000, 0x8a7fff, "ID", "Indonesia"),
    (0x900000, 0x9007ff, "MH", "Marshall Islands"),
    (0x901000, 0x9017ff, "CK", "Cook Islands"),
    (0x902000, 0x9027ff, "WS", "Samoa"),
    (0xa00000, 0xafffff, "US", "United States"),
    (0xc00000, 0xc3ffff, "CA", "Canada"),
    (0xc80000, 0xc87fff, "NZ", "New Zealand"),
    (0xc88000, 0xc88fff, "FJ", "Fiji"),
    (0xc8a000, 0xc8a7ff, "NR", "Nauru"),
    (0xc8c000, 0xc8c7ff, "LC", "Saint Lucia"),
    (0xc8d000, 0xc8d7ff, "TO", "Tonga"),
    (0xc8e000, 0xc8e7ff, "KI", "Kiribati"),
    (0xc90000, 0xc907ff, "VU", "Vanuatu"),
    (0xc91000, 0xc917ff, "AD", "Andorra"),
    (0xc92000, 0xc927ff, "DM", "Dominica"),
    (0xc93000, 0xc937ff, "KN", "Saint Kitts and Nevis"),
    (0xc94000, 0xc947ff, "SS", "South Sudan"),
    (0xc95000, 0xc957ff, "TL", "Timor-Leste"),
    (0xc97000, 0xc977ff, "TV", "Tuvalu"),
    (0xe00000, 0xe3ffff, "AR", "Argentina"),
    (0xe40000, 0xe7ffff, "BR", "Brazil"),
    (0xe80000, 0xe80fff, "CL", "Chile"),
    (0xe84000, 0xe84fff, "EC", "Ecuador"),
    (0xe88000, 0xe88fff, "PY", "Paraguay"),
    (0xe8c000, 0xe8cfff, "PE", "Peru"),
    (0xe90000, 0xe90fff, "UY", "Uruguay"),
    (0xe94000, 0xe94fff, "BO", "Bolivia (Plurinational State of)"),
    (0xf00000, 0xf07fff, "", "ICAO(1)"),
    (0xf09000, 0xf097ff, "", "ICAO(2)"),
];

/// ISO alpha-2 code for an ICAO 24-bit address, if it falls in a known block.
pub(crate) fn code_for_hex(hex: &str) -> Option<&'static str> {
    let addr = u32::from_str_radix(hex.trim(), 16).ok()?;
    let i = BLOCKS.partition_point(|&(start, _, _, _)| start <= addr);
    let &(start, end, code, _) = BLOCKS.get(i.checked_sub(1)?)?;
    (addr >= start && addr <= end && !code.is_empty()).then_some(code)
}

/// English state name for an ISO alpha-2 code, or "Unknown".
pub(crate) fn name(code: &str) -> &'static str {
    BLOCKS
        .iter()
        .find(|&&(_, _, c, _)| c == code)
        .map(|&(_, _, _, name)| name)
        .unwrap_or("Unknown")
}

/// Regional-indicator flag for an ISO alpha-2 code. Empty when the code is not
/// two ASCII letters: a flag emoji needs a country pair, not a placeholder.
pub(crate) fn flag(code: &str) -> String {
    let bytes = code.as_bytes();
    if bytes.len() != 2 || !bytes.iter().all(u8::is_ascii_alphabetic) {
        return String::new();
    }
    bytes
        .iter()
        .map(|b| char::from_u32(0x1F1E6 + u32::from(b.to_ascii_uppercase() - b'A')).unwrap_or('\u{fffd}'))
        .collect()
}

/// Fill `aircraft.country` for rows recorded before the column existed.
/// Idempotent; bump the stamp key if `BLOCKS` changes.
pub(crate) fn backfill(db: &Db) -> rusqlite::Result<usize> {
    let conn = crate::db_lock(db);
    let done = conn
        .query_row("SELECT 1 FROM enrichment_meta WHERE key = 'country_v1'", [], |_| Ok(()))
        .optional()?
        .is_some();
    if done {
        return Ok(0);
    }
    let hexes: Vec<String> = conn
        .prepare("SELECT hex FROM aircraft")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut n = 0;
    {
        let mut update = conn.prepare("UPDATE aircraft SET country = ?1 WHERE hex = ?2")?;
        for hex in &hexes {
            if let Some(code) = code_for_hex(hex) {
                n += update.execute(params![code, hex])?;
            }
        }
    }
    conn.execute(
        "INSERT OR REPLACE INTO enrichment_meta (key, value) VALUES ('country_v1', '1')",
        [],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_blocks() {
        assert_eq!(code_for_hex("a00000"), Some("US"));
        assert_eq!(code_for_hex("c00000"), Some("CA"));
        assert_eq!(code_for_hex("400000"), Some("GB"));
        assert_eq!(code_for_hex("380000"), Some("FR"));
        assert_eq!(code_for_hex("3c0000"), Some("DE"));
        assert_eq!(code_for_hex("7c0000"), Some("AU"));
        assert_eq!(code_for_hex("780000"), Some("CN"));
        assert_eq!(code_for_hex("840000"), Some("JP"));
    }

    #[test]
    fn inclusive_bounds() {
        assert_eq!(code_for_hex("a00000"), Some("US"));
        assert_eq!(code_for_hex("afffff"), Some("US"));
        assert_eq!(code_for_hex("b00000"), None);
    }

    #[test]
    fn unallocated_and_malformed() {
        assert_eq!(code_for_hex("000000"), None);
        assert_eq!(code_for_hex("~abcdef"), None);
        assert_eq!(code_for_hex("zzzzzz"), None);
        assert_eq!(code_for_hex(""), None);
    }

    #[test]
    fn flags() {
        assert_eq!(flag("CA"), "\u{1F1E8}\u{1F1E6}");
        assert_eq!(flag("us"), "\u{1F1FA}\u{1F1F8}");
        assert_eq!(flag(""), "");
        assert_eq!(flag("X"), "");
    }
}
