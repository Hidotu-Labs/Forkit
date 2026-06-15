use std::io::Read;

/// Fetch an HTTP/HTTPS URL, returning `(final_url, bytes)`.
pub fn fetch_url_bytes(url: &str) -> Result<(String, Vec<u8>), String> {
    let resp = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .call()
        .map_err(|e| format!("request failed: {e}"))?;

    let final_url = resp.get_url().to_owned();

    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read body failed: {e}"))?;

    Ok((final_url, bytes))
}

/// Fetch an HTTP/HTTPS URL, returning `(final_url, body)`.
pub fn fetch_url(url: &str) -> Result<(String, String), String> {
    let resp = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .call()
        .map_err(|e| format!("request failed: {e}"))?;

    let final_url = resp.get_url().to_owned();

    // Extract charset from Content-Type header (e.g. "text/html; charset=ISO-8859-9")
    let charset = resp
        .header("Content-Type")
        .and_then(|ct| {
            let lower = ct.to_ascii_lowercase();
            let cs_pos = lower.find("charset=")?;
            let rest = ct[cs_pos + 8..].trim_start_matches(|c: char| c == '"' || c == '\'');
            let end = rest.find(|c: char| c == ';' || c == '"' || c == '\'' || c.is_ascii_whitespace())
                .unwrap_or(rest.len());
            Some(rest[..end].to_ascii_lowercase())
        })
        .unwrap_or_default();

    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read body failed: {e}"))?;

    // Decode bytes → String according to the declared charset.
    // For UTF-8 (or no charset), use from_utf8 with lossy fallback.
    // For ISO-8859-* / Windows-125x / Latin-* variants, use the
    // appropriate single-byte decode table.
    let body = match charset.as_str() {
        "utf-8" | "utf8" | "" => {
            String::from_utf8(bytes)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
        }
        cs => {
            let table: &[char; 128] = if cs.contains("1254") || cs == "iso-8859-9"
                || cs == "iso8859-9" || cs == "latin5" || cs == "l5"
            {
                &ISO_8859_9_HIGH   // Turkish
            } else if cs.contains("1252") || cs == "windows-1252" || cs == "win-1252" {
                &WINDOWS_1252_HIGH
            } else if cs.contains("1250") || cs == "windows-1250" {
                &WINDOWS_1250_HIGH
            } else {
                // Generic ISO-8859-1 / Latin-1 for anything else
                &ISO_8859_1_HIGH
            };
            decode_single_byte(&bytes, table)
        }
    };

    Ok((final_url, body))
}

/// Decode a single-byte encoded byte slice using the provided table for
/// the high 128 codepoints (0x80–0xFF).  ASCII range (0x00–0x7F) maps 1:1.
pub fn decode_single_byte_pub(bytes: &[u8], high: &[char; 128]) -> String {
    decode_single_byte(bytes, high)
}

/// Decode a single-byte encoded byte slice using the provided table for
/// the high 128 codepoints (0x80–0xFF).  ASCII range (0x00–0x7F) maps 1:1.
fn decode_single_byte(bytes: &[u8], high: &[char; 128]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b < 0x80 {
            out.push(b as char);
        } else {
            out.push(high[(b - 0x80) as usize]);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Single-byte encoding tables (high half: 0x80–0xFF → char)
// ---------------------------------------------------------------------------

/// ISO-8859-9 (Latin-5 / Turkish) — same as ISO-8859-1 except 6 codepoints:
/// 0xD0→Ğ, 0xDD→İ, 0xDE→Ş, 0xF0→ğ, 0xFD→ı, 0xFE→ş
pub const ISO_8859_9_HIGH: [char; 128] = [
    // 0x80–0x9F (same as ISO-8859-1 C1 controls — render as replacement)
    '\u{0080}','\u{0081}','\u{0082}','\u{0083}','\u{0084}','\u{0085}','\u{0086}','\u{0087}',
    '\u{0088}','\u{0089}','\u{008A}','\u{008B}','\u{008C}','\u{008D}','\u{008E}','\u{008F}',
    '\u{0090}','\u{0091}','\u{0092}','\u{0093}','\u{0094}','\u{0095}','\u{0096}','\u{0097}',
    '\u{0098}','\u{0099}','\u{009A}','\u{009B}','\u{009C}','\u{009D}','\u{009E}','\u{009F}',
    // 0xA0–0xCF (identical to ISO-8859-1 up to 0xCF)
    '\u{00A0}','¡','¢','£','¤','¥','¦','§','¨','©','ª','«','¬','\u{00AD}','®','¯',
    '°','±','²','³','´','µ','¶','·','¸','¹','º','»','¼','½','¾','¿',
    'À','Á','Â','Ã','Ä','Å','Æ','Ç','È','É','Ê','Ë','Ì','Í','Î','Ï',
    'Ğ', // 0xD0 → Ğ  (ISO-8859-1 has Ð here)
    'Ñ','Ò','Ó','Ô','Õ','Ö','×','Ø','Ù','Ú','Û','Ü',
    'İ', // 0xDD → İ  (ISO-8859-1 has Ý here)
    'Ş', // 0xDE → Ş  (ISO-8859-1 has Þ here)
    'ß',
    // 0xE0–0xFF
    'à','á','â','ã','ä','å','æ','ç','è','é','ê','ë','ì','í','î','ï',
    'ğ', // 0xF0 → ğ  (ISO-8859-1 has ð here)
    'ñ','ò','ó','ô','õ','ö','÷','ø','ù','ú','û','ü',
    'ı', // 0xFD → ı  (ISO-8859-1 has ý here)
    'ş', // 0xFE → ş  (ISO-8859-1 has þ here)
    'ÿ',
];

/// ISO-8859-1 (Latin-1) — high byte value equals Unicode codepoint directly.
pub const ISO_8859_1_HIGH: [char; 128] = [
    '\u{0080}','\u{0081}','\u{0082}','\u{0083}','\u{0084}','\u{0085}','\u{0086}','\u{0087}',
    '\u{0088}','\u{0089}','\u{008A}','\u{008B}','\u{008C}','\u{008D}','\u{008E}','\u{008F}',
    '\u{0090}','\u{0091}','\u{0092}','\u{0093}','\u{0094}','\u{0095}','\u{0096}','\u{0097}',
    '\u{0098}','\u{0099}','\u{009A}','\u{009B}','\u{009C}','\u{009D}','\u{009E}','\u{009F}',
    '\u{00A0}','¡','¢','£','¤','¥','¦','§','¨','©','ª','«','¬','\u{00AD}','®','¯',
    '°','±','²','³','´','µ','¶','·','¸','¹','º','»','¼','½','¾','¿',
    'À','Á','Â','Ã','Ä','Å','Æ','Ç','È','É','Ê','Ë','Ì','Í','Î','Ï',
    'Ð','Ñ','Ò','Ó','Ô','Õ','Ö','×','Ø','Ù','Ú','Û','Ü','Ý','Þ','ß',
    'à','á','â','ã','ä','å','æ','ç','è','é','ê','ë','ì','í','î','ï',
    'ð','ñ','ò','ó','ô','õ','ö','÷','ø','ù','ú','û','ü','ý','þ','ÿ',
];

/// Windows-1252 — like ISO-8859-1 but 0x80–0x9F contains printable chars.
pub const WINDOWS_1252_HIGH: [char; 128] = [
    '€','\u{FFFD}','‚','ƒ','„','…','†','‡','ˆ','‰','Š','‹','Œ','\u{FFFD}','Ž','\u{FFFD}',
    '\u{FFFD}','\u{2018}','\u{2019}','\u{201C}','\u{201D}','•','–','—','˜','™','š','›','œ','\u{FFFD}','ž','Ÿ',
    '\u{00A0}','¡','¢','£','¤','¥','¦','§','¨','©','ª','«','¬','\u{00AD}','®','¯',
    '°','±','²','³','´','µ','¶','·','¸','¹','º','»','¼','½','¾','¿',
    'À','Á','Â','Ã','Ä','Å','Æ','Ç','È','É','Ê','Ë','Ì','Í','Î','Ï',
    'Ð','Ñ','Ò','Ó','Ô','Õ','Ö','×','Ø','Ù','Ú','Û','Ü','Ý','Þ','ß',
    'à','á','â','ã','ä','å','æ','ç','è','é','ê','ë','ì','í','î','ï',
    'ð','ñ','ò','ó','ô','õ','ö','÷','ø','ù','ú','û','ü','ý','þ','ÿ',
];

/// Windows-1250 (Central European).
pub const WINDOWS_1250_HIGH: [char; 128] = [
    '€','\u{FFFD}','‚','\u{FFFD}','„','…','†','‡','\u{FFFD}','‰','Š','‹','Ś','\u{0164}','Ž','\u{0179}',
    '\u{FFFD}','\u{2018}','\u{2019}','\u{201C}','\u{201D}','•','–','—','\u{FFFD}','™','š','›','ś','\u{0165}','ž','\u{017A}',
    '\u{00A0}','\u{02C7}','˘','Ł','¤','Ą','¦','§','¨','©','Ş','«','¬','\u{00AD}','®','Ż',
    '°','±','˛','ł','´','µ','¶','·','¸','ą','ş','»','Ľ','\u{02DD}','ľ','ż',
    'Ŕ','Á','Â','Ă','Ä','Ĺ','Ć','Ç','Č','É','Ę','Ë','Ě','Í','Î','Ď',
    'Đ','Ń','Ň','Ó','Ô','Ő','Ö','×','Ř','Ů','Ú','Ű','Ü','Ý','Ţ','ß',
    'ŕ','á','â','ă','ä','ĺ','ć','ç','č','é','ę','ë','ě','í','î','ď',
    'đ','ń','ň','ó','ô','ő','ö','÷','ř','ů','ú','ű','ü','ý','ţ','˙',
];

/// Normalise a user-typed or href-extracted URL:
/// - bare `example.com/path`  → `https://example.com/path`
/// - `/absolute/path`         → `https://{base_origin}/absolute/path`
/// - `relative/path`          → `https://{base_origin}/{base_dir}/relative/path`
/// - `//example.com/path`     → `https://example.com/path`
/// - already has scheme       → returned unchanged
pub fn resolve_url(input: &str, base: &str) -> String {
    let input = input.trim();

    // Already fully qualified
    if input.starts_with("http://") || input.starts_with("https://")
        || input.starts_with("file://")
    {
        return input.to_owned();
    }

    // Protocol-relative  //example.com/…
    if input.starts_with("//") {
        return format!("https:{}", input);
    }

    // Fragment-only or javascript: — return base unchanged
    if input.starts_with('#') || input.starts_with("javascript:") || input.is_empty() {
        return base.to_owned();
    }

    let origin = url_origin(base);

    // Absolute path on same origin
    if input.starts_with('/') {
        return format!("{}{}", origin, input);
    }

    // Relative path — resolve against base directory
    let base_dir = base_directory(base);
    format!("{}/{}", base_dir.trim_end_matches('/'), input)
}

/// Try HTTPS first; if that fails, fall back to HTTP.
/// Returns `(final_url, body)` or an error string.
pub fn fetch_with_auto_https(url: &str) -> Result<(String, String), String> {
    // If already has a scheme, try as-is then maybe flip to https
    let upgraded = if url.starts_with("http://") {
        url.replacen("http://", "https://", 1)
    } else {
        url.to_owned()
    };

    // Try the (possibly upgraded) URL first
    match fetch_url(&upgraded) {
        Ok(r) => return Ok(r),
        Err(e) => {
            // Only fall back to HTTP if we actually tried to upgrade
            if upgraded != url {
                eprintln!("HTTPS failed ({}), retrying over HTTP…", e);
                fetch_url(url)
            } else {
                Err(e)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

/// Extract `scheme://host` from a URL.
fn url_origin(url: &str) -> &str {
    // Find end of "scheme://host[:port]"
    if let Some(after_scheme) = url.find("://") {
        let rest = &url[after_scheme + 3..];
        let path_start = rest.find('/').unwrap_or(rest.len());
        &url[..after_scheme + 3 + path_start]
    } else {
        url
    }
}

/// Extract `scheme://host/path/to/` (directory part) from a URL.
fn base_directory(url: &str) -> &str {
    if let Some(q) = url.find('?') {
        // Drop query string before looking for last slash
        let without_q = &url[..q];
        if let Some(last_slash) = without_q.rfind('/') {
            return &url[..last_slash];
        }
    }
    if let Some(last_slash) = url.rfind('/') {
        // Don't strip past the "://"
        if last_slash > url.find("://").map(|i| i + 3).unwrap_or(0) {
            return &url[..last_slash];
        }
    }
    url
}
