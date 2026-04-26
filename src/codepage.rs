/// Maps the DBF code-page mark byte (header offset 29) to an
/// encoding label understood by the `encoding_rs` crate.
///
/// Reference: dBase / Visual FoxPro documentation and the list
/// maintained by the Python `dbf` library (ethanfurman/dbf).
pub fn encoding_for_mark(mark: u8) -> Option<&'static encoding_rs::Encoding> {
    use encoding_rs::*;
    match mark {
        // ── OEM code pages ────────────────────────────────────────────
        0x01 => Some(IBM866),       // DOS USA (CP437, best approximation)
        0x02 => Some(IBM866),       // DOS Multilingual (CP850) – common default
        0x03 => Some(WINDOWS_1252), // Windows ANSI (CP1252)
        0x04 => Some(IBM866),       // Standard Macintosh
        0x08 => Some(IBM866),       // Danish OEM
        0x09 => Some(IBM866),       // Dutch OEM
        0x0A => Some(IBM866),       // Dutch OEM (secondary)
        0x0B => Some(IBM866),       // Finnish OEM
        0x0D => Some(IBM866),       // French OEM
        0x0E => Some(IBM866),       // French OEM (secondary)
        0x0F => Some(IBM866),       // German OEM
        0x10 => Some(IBM866),       // German OEM (secondary)
        0x11 => Some(IBM866),       // Italian OEM
        0x12 => Some(IBM866),       // Italian OEM (secondary)
        0x13 => Some(IBM866),       // Japanese Shift-JIS OEM
        0x14 => Some(IBM866),       // Spanish OEM (secondary)
        0x15 => Some(IBM866),       // Swedish OEM
        0x16 => Some(IBM866),       // Swedish OEM (secondary)
        0x17 => Some(IBM866),       // Norwegian OEM
        0x18 => Some(IBM866),       // Spanish OEM
        0x19 => Some(IBM866),       // English OEM (British)
        0x1A => Some(IBM866),       // English OEM (British secondary)
        0x1B => Some(IBM866),       // English OEM (US)
        0x1C => Some(ISO_8859_2),   // French OEM (secondary)
        0x1D => Some(IBM866),       // German OEM
        0x1F => Some(IBM866),       // Czech OEM
        0x22 => Some(IBM866),       // Hungarian OEM
        0x23 => Some(IBM866),       // Polish OEM
        0x24 => Some(IBM866),       // Portuguese OEM
        0x25 => Some(IBM866),       // Portuguese OEM (secondary)
        0x26 => Some(IBM866),       // Russian OEM
        // ── Specific well-known CP marks ─────────────────────────────
        0x40 => Some(IBM866),         // Romanian OEM
        0x4D => Some(BIG5),           // Chinese GBK (Simplified)
        0x4E => Some(EUC_KR),         // Korean (EUC)
        0x4F => Some(BIG5),           // Chinese Big5 (Traditional)
        0x50 => Some(EUC_JP),         // Thai
        0x57 => Some(WINDOWS_1252),   // ANSI
        0x58 => Some(WINDOWS_1252),   // Western European ANSI
        0x59 => Some(WINDOWS_1252),   // Spanish ANSI
        0x64 => Some(IBM866),         // Eastern European MS-DOS (CP852)
        0x65 => Some(IBM866),         // Russian MS-DOS
        0x66 => Some(IBM866),         // Nordic MS-DOS
        0x67 => Some(IBM866),         // Icelandic MS-DOS
        0x6A => Some(IBM866),         // Greek MS-DOS (437G)
        0x6B => Some(IBM866),         // MS-DOS Russian
        0x6C => Some(IBM866),         // MS-DOS Czech (Kamenicky)
        0x6D => Some(IBM866),         // MS-DOS Slovak (Kamenicky)
        0x6E => Some(IBM866),         // MS-DOS Polish (Mazovia)
        0x78 => Some(BIG5),           // Chinese traditional (Taiwan)
        0x79 => Some(EUC_KR),         // Korean
        0x7A => Some(GB18030),        // Chinese simplified
        0x7B => Some(SHIFT_JIS),      // Japanese
        0x7C => Some(EUC_KR),         // Korean
        0x7D => Some(WINDOWS_1255),   // Hebrew Windows
        0x7E => Some(WINDOWS_1256),   // Arabic Windows
        0x96 => Some(X_MAC_CYRILLIC), // Russian Macintosh
        0x97 => None,                 // Eastern European Macintosh (Not supported by encoding_rs)
        0x98 => None,                 // Greek Macintosh (Not supported by encoding_rs)
        0xC8 => Some(WINDOWS_1250),   // Windows EE
        0xC9 => Some(WINDOWS_1251),   // Russian Windows
        0xCA => Some(WINDOWS_1254),   // Turkish Windows
        0xCB => Some(WINDOWS_1253),   // Greek Windows
        0xCC => Some(WINDOWS_1257),   // Baltic Windows
        _ => None,
    }
}

/// Decode a byte slice using the given encoding, falling back to
/// lossy UTF-8 if `encoding` is `None` or the decoding has errors.
pub fn decode_bytes(raw: &[u8], encoding: Option<&'static encoding_rs::Encoding>) -> String {
    match encoding {
        Some(enc) => {
            let (cow, _encoding_used, _had_errors) = enc.decode(raw);
            cow.into_owned()
        }
        None => String::from_utf8_lossy(raw).into_owned(),
    }
}

/// Encode a Rust `&str` into the target encoding.  Returns `None` if the
/// string cannot be represented without loss (unmappable characters).
pub fn encode_str(text: &str, encoding: Option<&'static encoding_rs::Encoding>) -> Option<Vec<u8>> {
    match encoding {
        Some(enc) => {
            let (cow, _enc, had_errors) = enc.encode(text);
            if had_errors {
                None
            } else {
                Some(cow.into_owned())
            }
        }
        None => Some(text.as_bytes().to_vec()),
    }
}

/// Human-readable label for a code-page mark, useful for error messages
/// and `__repr__`.
pub fn label_for_mark(mark: u8) -> &'static str {
    match mark {
        0x00 => "unset",
        0x01 => "CP437",
        0x02 => "CP850",
        0x03 => "CP1252",
        0x04 => "MacRoman",
        0x08..=0x1B => "CP850 (OEM variant)",
        0x1C | 0x1D => "ISO-8859-2",
        0x26 => "CP866",
        0x40 => "CP852 (Romanian)",
        0x4D => "GBK",
        0x4E => "EUC-KR",
        0x4F => "Big5",
        0x57..=0x59 => "CP1252",
        0x64 => "CP852",
        0x65 => "CP866",
        0x78 => "Big5",
        0x79 => "EUC-KR",
        0x7A => "GB18030",
        0x7B => "Shift-JIS",
        0x7D => "CP1255",
        0x7E => "CP1256",
        0x96 => "Mac Cyrillic",
        0x97 => "Mac CE",
        0x98 => "Mac Greek",
        0xC8 => "CP1250",
        0xC9 => "CP1251",
        0xCA => "CP1254",
        0xCB => "CP1253",
        0xCC => "CP1257",
        _ => "unknown",
    }
}

/// Attempt to find the code-page mark byte for a given encoding name or
/// alias (e.g. `"cp1252"`, `"windows-1252"`, `"utf-8"`).  Returns `None`
/// when no mapping exists (the caller should decide whether to error or
/// use 0x00).
pub fn mark_for_name(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    let lower = lower.trim();
    match lower {
        "cp437" | "ibm437" => Some(0x01),
        "cp850" | "ibm850" => Some(0x02),
        "cp1252" | "windows-1252" | "windows_1252" | "ansi" | "latin-1" | "iso-8859-1" => {
            Some(0x03)
        }
        "cp866" | "ibm866" => Some(0x65),
        "cp852" | "ibm852" => Some(0x64),
        "cp1250" | "windows-1250" | "windows_1250" => Some(0xC8),
        "cp1251" | "windows-1251" | "windows_1251" => Some(0xC9),
        "cp1253" | "windows-1253" | "windows_1253" => Some(0xCB),
        "cp1254" | "windows-1254" | "windows_1254" => Some(0xCA),
        "cp1255" | "windows-1255" | "windows_1255" => Some(0x7D),
        "cp1256" | "windows-1256" | "windows_1256" => Some(0x7E),
        "cp1257" | "windows-1257" | "windows_1257" => Some(0xCC),
        "shift-jis" | "shift_jis" | "sjis" | "shiftjis" => Some(0x7B),
        "euc-kr" | "euc_kr" => Some(0x7C),
        "big5" => Some(0x7A),
        "gb18030" | "gbk" | "gb2312" => Some(0x7A),
        "utf-8" | "utf8" => Some(0x00), // UTF-8 has no standard DBF mark
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp1252_roundtrip() {
        let enc = encoding_for_mark(0x03).unwrap();
        let original = "Stra\u{00df}e"; // "Straße"
        let (encoded, _, errors) = enc.encode(original);
        assert!(!errors);
        let decoded = decode_bytes(&encoded, Some(enc));
        assert_eq!(decoded, original);
    }

    #[test]
    fn cp1251_cyrillic_roundtrip() {
        let enc = encoding_for_mark(0xC9).unwrap();
        let original = "\u{041F}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}"; // Привет
        let (encoded, _, errors) = enc.encode(original);
        assert!(!errors);
        let decoded = decode_bytes(&encoded, Some(enc));
        assert_eq!(decoded, original);
    }

    #[test]
    fn mark_round_trip() {
        assert_eq!(mark_for_name("cp1252"), Some(0x03));
        assert_eq!(mark_for_name("windows-1251"), Some(0xC9));
        assert!(encoding_for_mark(mark_for_name("cp1250").unwrap()).is_some());
    }

    #[test]
    fn unknown_mark_returns_none() {
        assert!(encoding_for_mark(0xFF).is_none());
    }
}
